//! Mandyhub updates itself: at start it asks GitHub for the newest release of
//! the launcher; if it is newer than this build and has an .exe attached, a
//! window offers "Update now" / "Later". Updating downloads the new exe next to
//! this one, swaps them (a running exe can be renamed on Windows, not
//! overwritten), starts the new one and closes this one.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};

const RELEASES: &str = "https://api.github.com/repos/Z-rox/mandyhub/releases/latest";

#[derive(Clone)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub notes: String,
}

#[derive(Default)]
pub struct SelfUpdate {
    pub found: Option<Release>,
    pub dismissed: bool,
    pub busy: bool,
    pub error: Option<String>,
}

/// "v0.1.5" or "0.1.5" as numbers, for comparing.
fn parse(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-']).map_while(|p| p.parse().ok()).collect()
}

fn newer(remote: &str, mine: &str) -> bool {
    parse(remote) > parse(mine)
}

/// Starts the look for a newer release (and clears what an earlier update left).
pub fn check(state: Arc<Mutex<SelfUpdate>>, ctx: egui::Context) {
    if let Ok(me) = std::env::current_exe() {
        let _ = std::fs::remove_file(me.with_extension("old.exe"));
    }
    std::thread::spawn(move || {
        let Ok(resp) = ureq::get(RELEASES).set("User-Agent", "mandyhub").call() else { return };
        let Ok(body) = resp.into_json::<serde_json::Value>() else { return };
        let tag = body["tag_name"].as_str().unwrap_or_default().to_string();
        if !newer(&tag, env!("CARGO_PKG_VERSION")) {
            return;
        }
        let url = body["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"].as_str().is_some_and(|n| n.to_lowercase().ends_with(".exe")))
            .and_then(|a| a["browser_download_url"].as_str())
            .map(str::to_string);
        let Some(url) = url else { return };
        let notes = crate::clean_text(body["body"].as_str().unwrap_or_default());
        state.lock().unwrap().found = Some(Release { version: tag, url, notes });
        ctx.request_repaint();
    });
}

/// Downloads the new exe, swaps it with this one and starts it.
fn install(rel: Release) -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| format!("where am I? {e}"))?;
    let new: PathBuf = me.with_extension("new.exe");
    let old: PathBuf = me.with_extension("old.exe");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut ureq::get(&rel.url).set("User-Agent", "mandyhub").call().map_err(|e| format!("download failed: {e}"))?.into_reader(),
        &mut bytes,
    )
    .map_err(|e| format!("download failed: {e}"))?;
    // An exe starts with "MZ"; a short or wrong file is not put in place.
    if bytes.len() < 1 << 20 || !bytes.starts_with(b"MZ") {
        return Err("the download is not a Windows program".into());
    }
    std::fs::write(&new, &bytes).map_err(|e| format!("cannot save the new version: {e}"))?;
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&me, &old).map_err(|e| format!("cannot move this version aside: {e}"))?;
    if let Err(e) = std::fs::rename(&new, &me) {
        let _ = std::fs::rename(&old, &me);
        return Err(format!("cannot put the new version in place: {e}"));
    }
    std::process::Command::new(&me).spawn().map_err(|e| format!("cannot start the new version: {e}"))?;
    std::process::exit(0);
}

/// The window, while a newer release is known and not put off.
pub fn window(state: &Arc<Mutex<SelfUpdate>>, ctx: &egui::Context) {
    let (rel, busy, error) = {
        let s = state.lock().unwrap();
        match (&s.found, s.dismissed) {
            (Some(r), false) => (r.clone(), s.busy, s.error.clone()),
            _ => return,
        }
    };
    egui::Window::new("Mandyhub update")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("Mandyhub {} is out", rel.version)).size(20.0).strong());
            ui.label(RichText::new(format!("You have v{}.", env!("CARGO_PKG_VERSION"))).color(crate::DIM));
            if !rel.notes.trim().is_empty() {
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    ui.label(rel.notes.chars().take(2000).collect::<String>());
                });
            }
            if let Some(e) = &error {
                ui.label(RichText::new(e).color(crate::BAD));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if busy {
                    ui.spinner();
                    ui.label("Downloading the new version…");
                    return;
                }
                if ui.button(RichText::new("Update now").strong()).clicked() {
                    state.lock().unwrap().busy = true;
                    let (state, rel, ctx) = (state.clone(), rel.clone(), ctx.clone());
                    std::thread::spawn(move || {
                        if let Err(e) = install(rel) {
                            let mut s = state.lock().unwrap();
                            s.busy = false;
                            s.error = Some(e);
                        }
                        ctx.request_repaint();
                    });
                }
                if ui.button("Later").clicked() {
                    state.lock().unwrap().dismissed = true;
                }
            });
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions_compare_as_numbers() {
        assert!(super::newer("v0.1.10", "0.1.9"));
        assert!(super::newer("v0.2.0", "0.1.5"));
        assert!(!super::newer("v0.1.5", "0.1.5"));
        assert!(!super::newer("v0.1.4", "0.1.5"));
    }
}
