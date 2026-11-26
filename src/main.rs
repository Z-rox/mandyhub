//! Mandyhub: a community launcher for VIctory-based projects that run an
//! Xbox Series game on PC.
//!
//! - The list of projects comes from PortWatch (portwatch-vi.xyz), live.
//! - Any other GitHub repo can be added by link.
//! - The game folder is checked for the Xbox files; without them Play stays off.
//! - Play: the selected repo is downloaded (git), built (cargo) and started.
//!
//! No game files are included or downloaded: the player supplies their own.
#![windows_subsystem = "windows"]

mod fullscreen;
mod slides;
mod theme;
mod tools;
mod ui;

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const PORTWATCH: &str = "https://portwatch-vi.xyz/data/tracker.json";
const EXE_NAME: &str = "game_scarlett_master.exe";
/// What a game folder must hold: the executable, its config and the main archives.
const REQUIRED: &[&str] = &[
    EXE_NAME,
    "MicrosoftGame.Config",
    "common.rpf",
    "data.rpf",
    "levels.rpf",
    "script.rpf",
    "shaders.rpf",
    "textures.rpf",
];

/// A project the player can pick.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Project {
    name: String,
    /// `owner/name` on GitHub.
    repo: String,
    branch: Option<String>,
    gpu: Vec<String>,
    fps: Option<f64>,
    render: Option<f64>,
    verified: bool,
    summary: String,
    custom: bool,
    /// A build already on this PC (not downloaded or built by the launcher).
    #[serde(default)]
    local_exe: Option<String>,
}

/// What is kept between runs.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    game_dir: String,
    /// "8 GB graphics card fix": smaller textures and a grey stand-in for missing ones.
    fix_8gb: bool,
    /// "Brighter shadows": a constant ambient light where ray tracing does not run.
    bright_shadows: bool,
    /// Whether the defaults above were set once.
    defaults_set: bool,
    /// Play in borderless fullscreen (off: a normal window). Off by default.
    fullscreen: bool,
    /// Where projects are downloaded and built (empty: the default folder).
    install_dir: String,
    /// Updates put off with "Later": (repo, the new version's commit).
    later: Vec<(String, String)>,
    selected: Option<String>,
    custom: Vec<Project>,
    series_s: bool,
    render_scale: f32,
    extra_env: String,
    gpu_filter: usize,
    /// Translate the shaders into the cache (`victory translate --cache`) before each Play.
    warm_before_play: bool,
    /// With the warm-up, also build the GPU pipelines (`--pipelines`; can take hours).
    warm_pipelines: bool,
    /// Extra arguments for `victory run`; the game's own go after `--`.
    launch_args: String,
}

/// Shared state of the background job (download, build, play).
#[derive(Default)]
struct Job {
    running: bool,
    /// The project the job works on.
    repo: Option<String>,
    stage: String,
    log: Vec<String>,
    failed: bool,
    /// The process running now, while the player may stop it (a warm-up).
    pid: Option<u32>,
    stoppable: bool,
    /// The player pressed Stop.
    stopped: bool,
}

/// How a project is started, read from the settings when its job starts.
/// The warm-up and Play share it, so the warm-up fills the cache Play reads.
#[derive(Clone)]
struct Launch {
    game_dir: PathBuf,
    series_s: bool,
    scale: f32,
    /// `VICTORY_*` lines: the player's switches and the graphics fixes.
    extra: String,
    fullscreen: bool,
    warm_first: bool,
    pipelines: bool,
    args: Vec<String>,
}

/// The game files `victory translate --cache` reads shaders from (those that exist are passed).
const WARM_FILES: &[&str] = &["shaders.rpf", EXE_NAME, "XboxBvhBuilder.dll", "amd_fidelityfx_upscaler_xbox.dll"];

/// Launch arguments as the player typed them: split at spaces, "double quotes" keeping one together.
fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    for ch in s.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::split_args;

    #[test]
    fn launch_arguments_split_at_spaces_and_keep_quotes_together() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("  --strict   "), ["--strict"]);
        assert_eq!(split_args("--strict -- -windowed"), ["--strict", "--", "-windowed"]);
        assert_eq!(split_args(r#"-- -name "two words" """#), ["--", "-name", "two words", ""]);
    }
}

struct App {
    saved: Saved,
    projects: Arc<Mutex<Option<Result<Vec<Project>, String>>>>,
    job: Arc<Mutex<Job>>,
    files: Vec<(&'static str, bool)>,
    files_for: String,
    tools: Option<tools::Tools>,
    /// The project an uninstall is being confirmed for.
    confirm_uninstall: Option<String>,
    /// A new install folder waiting for "move the projects there?" (from, to).
    pending_move: Option<(PathBuf, PathBuf)>,
    /// The folder the files were checked in was empty.
    empty: bool,
    /// The drive search: `None` idle, `Some(None)` searching, `Some(Some(r))` done.
    finding: Arc<Mutex<Option<Option<Result<String, String>>>>>,
    /// What is known about each project's install, by repo.
    repos: Arc<Mutex<std::collections::HashMap<String, RepoState>>>,
    show_log: bool,
    /// The player's own pictures, across the top of a project's page.
    slides: slides::Slides,
    /// When every installed project was last checked for updates (seconds).
    last_auto_check: f64,
    /// Which page shows: the projects or the settings.
    view: View,
    /// What the project list is narrowed to.
    search: String,
    /// The "Add a repo by link" dialog, while it is open.
    add_repo: Option<AddRepo>,
    /// Why the last drive search found no game.
    find_error: Option<String>,
    /// The project whose page shows, and when it came in (for its entrance).
    shown: Option<String>,
    shown_at: f64,
    /// When the repo link was last copied: its button shows a tick for a moment.
    copied_at: f64,
    /// When the open dialog opened (for its entrance), and whether one was open last frame.
    modal_at: f64,
    modal_was_open: bool,
    /// Whether the window's title bar was made dark.
    title_bar_done: bool,
    /// Whether the window's remembered size was checked at start.
    size_checked: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Projects,
    Settings,
}

/// The "Add a repo by link" dialog.
struct AddRepo {
    link: String,
    error: Option<String>,
    /// Put the cursor in the field on the first frame.
    focus: bool,
}

/// How often installed projects are checked for updates: every 15 minutes.
const AUTO_CHECK_SECS: f64 = 15.0 * 60.0;

/// A project's install on this PC and the newest version online.
#[derive(Clone, Default)]
struct RepoState {
    checking: bool,
    /// Built and ready to play.
    installed: bool,
    /// The commit the installed build was made from.
    built: Option<String>,
    /// The newest commit online (`None` while unknown or unreachable).
    remote: Option<String>,
}

/// What the background job does.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Install,
    Play,
    Uninstall,
    /// Shaders into the cache ahead of time, without playing.
    Warm,
}

fn slug(repo: &str) -> String {
    repo.replace('/', "_")
}
fn repo_dir(repo: &str) -> PathBuf {
    workspace().join("repos").join(slug(repo))
}
fn target_dir(repo: &str) -> PathBuf {
    workspace().join("target").join(slug(repo))
}
fn built_exe(repo: &str) -> PathBuf {
    let dir = repo_dir(repo);
    let profile = detect_build_profile(&dir);
    target_dir(repo).join(profile).join("victory.exe")
}

/// Checks a cloned repo's Cargo.toml for a custom build profile (e.g. `diag`).
fn detect_build_profile(repo_dir: &Path) -> &'static str {
    let cargo_toml = repo_dir.join("Cargo.toml");
    if let Ok(text) = std::fs::read_to_string(&cargo_toml) {
        if text.contains("[profile.diag]") {
            return "diag";
        }
    }
    "release"
}

/// Reads VICTORY_* environment variables from a project's scripts/play.ps1.
fn detect_launch_env(repo_dir: &Path) -> Vec<(String, String)> {
    let script = repo_dir.join("scripts").join("play.ps1");
    let Ok(text) = std::fs::read_to_string(&script) else { return Vec::new() };
    let mut vars = Vec::new();
    // Parse PowerShell hashtable lines like: VICTORY_FOO = '1'; VICTORY_BAR = '2'
    // They appear inside $settings = @{ ... } and in conditional blocks.
    for line in text.lines() {
        let line = line.trim().trim_end_matches(';').trim();
        // Match lines containing VICTORY_ assignments with single-quoted values.
        for part in line.split(';') {
            let part = part.trim();
            if let Some((k, v)) = part.split_once('=') {
                let k = k.trim().trim_start_matches("$settings.");
                let v = v.trim().trim_matches('\'').trim_matches('"');
                if k.starts_with("VICTORY_") && !k.contains(' ') {
                    vars.push((k.to_string(), v.to_string()));
                }
            }
        }
    }
    vars
}
/// The commit an installed build was made from, written after a build.
fn built_file(repo: &str) -> PathBuf {
    target_dir(repo).join("built-from.txt")
}

/// The newest commit of `p`'s branch online (`git ls-remote`, no download).
fn remote_head(p: &Project) -> Option<String> {
    let url = format!("https://github.com/{}.git", p.repo);
    let mut c = tools::git();
    c.arg("ls-remote").arg(&url);
    match &p.branch {
        Some(b) => c.arg(format!("refs/heads/{b}")),
        None => c.arg("HEAD"),
    };
    hide_window(&mut c);
    let out = c.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().next().filter(|h| h.len() == 40).map(str::to_string)
}

fn repo_state(p: &Project) -> RepoState {
    if let Some(exe) = &p.local_exe {
        return RepoState { checking: false, installed: Path::new(exe).is_file(), built: Some("local".into()), remote: None };
    }
    RepoState {
        checking: false,
        installed: built_exe(&p.repo).is_file(),
        built: std::fs::read_to_string(built_file(&p.repo)).ok().map(|s| s.trim().to_string()),
        remote: remote_head(p),
    }
}

/// A folder from what the player typed or picked: quotes and spaces off, and
/// the folder of a file if a file was given (the game's exe, say).
fn normalize_dir(s: &str) -> PathBuf {
    let t = s.trim().trim_matches('"').trim();
    let p = PathBuf::from(t);
    if p.is_file() {
        p.parent().map(Path::to_path_buf).unwrap_or(p)
    } else {
        p
    }
}

/// The folder holding the game's exe: `dir` itself or a folder under it, at
/// most `depth` levels down.
fn find_game_in(dir: &Path, depth: u32) -> Option<PathBuf> {
    if dir.join(EXE_NAME).is_file() {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        // Not worth walking into.
        let skip = ["windows", "program files", "program files (x86)", "programdata", "appdata"];
        if name.starts_with('$') || name.starts_with('.') || skip.contains(&name.as_str()) {
            continue;
        }
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(found) = find_game_in(&e.path(), depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// Every drive's folders, three levels down, for the game's exe.
fn search_drives() -> Result<String, String> {
    for letter in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        if !root.is_dir() {
            continue;
        }
        if let Some(found) = find_game_in(&root, 3) {
            return Ok(found.display().to_string());
        }
    }
    Err("The game was not found on any drive. Choose its folder with Browse.".into())
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mandyhub")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 620.0])
            .with_icon(theme::app_icon()),
        ..Default::default()
    };
    migrate_old_folders();
    eframe::run_native("Mandyhub", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

/// Where the launcher keeps repos it downloads: `%LOCALAPPDATA%\Mandyhub`.
fn workspace() -> PathBuf {
    INSTALL_DIR.lock().unwrap().clone().unwrap_or_else(default_workspace)
}

/// `%LOCALAPPDATA%\Mandyhub`: where projects go unless the player chose
/// another folder (Settings: Install folder).
fn default_workspace() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Mandyhub")
}

/// The install folder the player chose, if any.
static INSTALL_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn set_install_dir(dir: &str) {
    let d = dir.trim();
    *INSTALL_DIR.lock().unwrap() = (!d.is_empty()).then(|| PathBuf::from(d));
}

/// The parts of an install folder that belong to projects.
const PROJECT_PARTS: &[&str] = &["repos", "target", "cache", "data"];

/// Whether `dir` holds any installed project.
fn has_projects(dir: &Path) -> bool {
    std::fs::read_dir(dir.join("repos")).map(|mut d| d.next().is_some()).unwrap_or(false)
}

/// Moves a folder: a rename on the same drive, else a copy and a delete.
fn move_dir(from: &Path, to: &Path, log: &dyn Fn(String)) -> std::io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !to.exists() && std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to, log)?;
    std::fs::remove_dir_all(from)
}

fn copy_dir(from: &Path, to: &Path, log: &dyn Fn(String)) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target, log)?;
        } else {
            std::fs::copy(e.path(), &target)?;
        }
    }
    log(format!("copied {}", from.display()));
    Ok(())
}

/// The launcher's folders had another name before: they are moved once, so
/// installed projects, settings and pictures stay.
fn migrate_old_folders() {
    for var in ["LOCALAPPDATA", "APPDATA"] {
        let Some(base) = std::env::var_os(var).map(PathBuf::from) else { continue };
        let (old, new) = (base.join("VI Launcher"), base.join("Mandyhub"));
        if old.is_dir() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}

/// Words kept out of the source as text: the game's name in the ways other
/// sites write it, to replace or skip (`clean_text`, `names_the_game`), and
/// PortWatch's field name for scores. Each byte is XORed with 0x5A.
const NAMES: &[&[u8]] = &[&[29, 40, 59, 52, 62, 122, 14, 50, 63, 60, 46, 122, 27, 47, 46, 53, 122, 12, 19], &[29, 40, 59, 52, 62, 122, 14, 50, 63, 60, 46, 122, 27, 47, 46, 53, 122, 108], &[29, 14, 27, 122, 12, 19], &[29, 14, 27, 122, 108], &[29, 14, 27, 108], &[29, 14, 27, 12, 19], &[61, 46, 59, 108], &[61, 46, 59, 122, 108]];
const KEYS: &[&[u8]] = &[&[61, 46, 59, 108], &[61, 46, 59, 44, 51], &[61, 40, 59, 52, 62, 46, 50, 63, 60, 46, 59, 47, 46, 53]];
const FIELD: &[u8] = &[61, 46, 59];

fn hidden(words: &[&[u8]]) -> Vec<String> {
    words.iter().map(|w| w.iter().map(|b| (b ^ 0x5A) as char).collect()).collect()
}

/// Text from other sites without the game's name: "the game" in its place.
fn clean_text(s: &str) -> String {
    let mut out = s.to_string();
    for pat in hidden(NAMES) {
        let pat = pat.as_str();
        for (from, to) in [(format!("{pat}'s"), "the game's".to_string()), (pat.to_string(), "the game".to_string())] {
            out = out.replace(&from, &to);
        }
    }
    out.replace(" PC port of the game", " PC port").replace("the game PC", "the game on PC")
}

/// Whether a repo's own name carries the game's name (not shown).
fn names_the_game(s: &str) -> bool {
    let l = s.to_lowercase().replace(['-', '_', ' ', '.'], "");
    hidden(KEYS).iter().any(|k| l.contains(k.as_str()))
}

/// `owner/name` from a GitHub link or `owner/name` itself.
fn parse_repo(link: &str) -> Option<String> {
    let s = link.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("http://github.com/"))
        .or_else(|| s.strip_prefix("github.com/"))
        .unwrap_or(s);
    let mut parts = s.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    let ok = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (ok(owner) && ok(name)).then(|| format!("{owner}/{name}"))
}

/// The public projects on PortWatch, best first.
fn fetch_portwatch() -> Result<Vec<Project>, String> {
    let body: serde_json::Value = ureq::get(PORTWATCH)
        .set("User-Agent", "mandyhub")
        .call()
        .map_err(|e| format!("PortWatch could not be reached: {e}"))?
        .into_json()
        .map_err(|e| format!("PortWatch sent something unreadable: {e}"))?;
    let efforts = body["efforts"].as_array().ok_or("PortWatch's list has no projects")?;
    // PortWatch keeps each project's scores under a field named after the game.
    let field = hidden(&[FIELD]).remove(0);
    let mut out: Vec<Project> = efforts
        .iter()
        .filter_map(|e| {
            let repo = e["repo"].as_str()?.to_string();
            parse_repo(&repo)?;
            if names_the_game(&repo) || names_the_game(e["name"].as_str().unwrap_or("")) {
                return None;
            }
            if e["stats"]["missing"].as_bool() == Some(true) || e["stats"]["archived"].as_bool() == Some(true) {
                return None;
            }
            Some(Project {
                name: clean_text(e["name"].as_str().unwrap_or(&repo)),
                branch: e["branch"].as_str().map(str::to_string),
                gpu: e["gpu"].as_array().map(|g| g.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                fps: e[field.as_str()]["fps"].as_f64(),
                render: e[field.as_str()]["render"].as_f64(),
                verified: e["verification"].as_str() == Some("portwatch"),
                summary: clean_text(e["summary"].as_str().unwrap_or("")),
                custom: false,
                local_exe: None,
                repo,
            })
        })
        .collect();
    let score = |p: &Project| p.render.unwrap_or(0.0) * 10.0 + p.fps.unwrap_or(0.0) + if p.verified { 5.0 } else { 0.0 };
    out.sort_by(|a, b| score(b).total_cmp(&score(a)));
    Ok(out)
}

#[cfg(windows)]
pub(crate) fn hide_window(c: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    c.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
pub(crate) fn hide_window(_: &mut Command) {}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> App {
        theme::install(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let mut saved: Saved = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();
        if saved.render_scale <= 0.0 {
            saved.render_scale = 0.5;
        }
        let mut app = App {
            saved,
            projects: Arc::new(Mutex::new(None)),
            job: Arc::new(Mutex::new(Job::default())),
            files: Vec::new(),
            files_for: String::new(),
            tools: None,
            confirm_uninstall: None,
            pending_move: None,
            empty: false,
            finding: Arc::new(Mutex::new(None)),
            repos: Arc::new(Mutex::new(std::collections::HashMap::new())),
            show_log: false,
            slides: slides::Slides::new(),
            last_auto_check: -1.0,
            view: View::Projects,
            search: String::new(),
            add_repo: None,
            find_error: None,
            shown: None,
            shown_at: 0.0,
            copied_at: -10.0,
            modal_at: 0.0,
            modal_was_open: false,
            title_bar_done: false,
            size_checked: false,
        };
        if !app.saved.defaults_set {
            app.saved.defaults_set = true;
            app.saved.fix_8gb = true;
            app.saved.bright_shadows = true;
            app.saved.series_s = true;
            app.saved.render_scale = 0.5;
        }
        set_install_dir(&app.saved.install_dir);
        app.refresh_projects(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let job = app.job.clone();
        std::thread::spawn(move || {
            let _ = tools::detect();
            job.lock().unwrap().stage = String::new();
            ctx.request_repaint();
        });
        app
    }

    fn refresh_projects(&mut self, ctx: &egui::Context) {
        *self.projects.lock().unwrap() = None;
        let (slot, ctx) = (self.projects.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = fetch_portwatch();
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    fn check_files(&mut self) {
        if self.files_for == self.saved.game_dir && !self.files.is_empty() {
            return;
        }
        let mut dir = normalize_dir(&self.saved.game_dir);
        // The exe in a folder under the one given: that folder is the game's.
        if !self.saved.game_dir.trim().is_empty() && !dir.join(EXE_NAME).is_file() {
            if let Some(found) = find_game_in(&dir, 2) {
                dir = found;
            }
        }
        let text = dir.display().to_string();
        if !self.saved.game_dir.trim().is_empty() && text != self.saved.game_dir {
            self.saved.game_dir = text;
        }
        self.empty = !self.saved.game_dir.is_empty()
            && std::fs::read_dir(&dir).map(|mut d| d.next().is_none()).unwrap_or(true);
        self.files = REQUIRED.iter().map(|f| (*f, !self.saved.game_dir.is_empty() && dir.join(f).is_file())).collect();
        self.files_for = self.saved.game_dir.clone();
    }

    fn files_ok(&self) -> bool {
        !self.files.is_empty() && self.files.iter().all(|f| f.1)
    }

    fn all_projects(&self) -> Vec<Project> {
        let mut v: Vec<Project> = self.saved.custom.clone();
        if let Some(Ok(list)) = &*self.projects.lock().unwrap() {
            v.extend(list.iter().cloned());
        }
        v
    }

    fn selected_project(&self) -> Option<Project> {
        let sel = self.saved.selected.as_ref()?;
        self.all_projects().into_iter().find(|p| &p.repo == sel)
    }

    /// Looks again at `p`'s install and the newest version online.
    fn check_repo(&self, ctx: &egui::Context, p: &Project) {
        {
            let mut m = self.repos.lock().unwrap();
            let e = m.entry(p.repo.clone()).or_default();
            if e.checking {
                return;
            }
            e.checking = true;
        }
        let (repos, ctx, p) = (self.repos.clone(), ctx.clone(), p.clone());
        std::thread::spawn(move || {
            let st = repo_state(&p);
            repos.lock().unwrap().insert(p.repo.clone(), st);
            ctx.request_repaint();
        });
    }

    fn state_of(&self, repo: &str) -> Option<RepoState> {
        self.repos.lock().unwrap().get(repo).cloned()
    }

    /// A newer version online than the one installed, and not put off.
    fn update_for(&self, p: &Project) -> Option<String> {
        if p.local_exe.is_some() {
            return None;
        }
        // Not while this project is being installed or updated.
        {
            let j = self.job.lock().unwrap();
            if j.running && j.repo.as_deref() == Some(p.repo.as_str()) {
                return None;
            }
        }
        let remote = self.state_of(&p.repo)?.remote?;
        // What is installed now, from disk: a check that started before the
        // last update finished must not bring the old version back.
        if !built_exe(&p.repo).is_file() {
            return None;
        }
        let built = std::fs::read_to_string(built_file(&p.repo)).ok().map(|s| s.trim().to_string());
        let newer = built.as_deref() != Some(remote.as_str());
        let later = self.saved.later.iter().any(|(r, c)| r == &p.repo && c == &remote);
        (newer && !later).then_some(remote)
    }

    fn start(&mut self, ctx: &egui::Context, p: Project, kind: Kind) {
        let job = self.job.clone();
        {
            let mut j = job.lock().unwrap();
            if j.running {
                return;
            }
            *j = Job { running: true, stage: "Starting".into(), repo: Some(p.repo.clone()), ..Default::default() };
        }
        // An install over an installed project is an update.
        let updating = kind == Kind::Install && built_exe(&p.repo).is_file();
        let mut extra = self.saved.extra_env.clone();
        if self.saved.fix_8gb {
            extra.push_str("\nVICTORY_TEXTURE_DROP_MIPS=2\nVICTORY_NULL_COLOR=grey");
        }
        if self.saved.bright_shadows {
            extra.push_str("\nVICTORY_AMBIENT=0.6");
        }
        let launch = Launch {
            game_dir: PathBuf::from(&self.saved.game_dir),
            series_s: self.saved.series_s,
            scale: self.saved.render_scale,
            extra,
            fullscreen: self.saved.fullscreen,
            warm_first: self.saved.warm_before_play,
            pipelines: self.saved.warm_pipelines,
            args: split_args(&self.saved.launch_args),
        };
        let (ctx, repos) = (ctx.clone(), self.repos.clone());
        std::thread::spawn(move || {
            let r = match kind {
                Kind::Install => install(&job, &ctx, &p),
                Kind::Play => play(&job, &ctx, &p, &launch),
                Kind::Uninstall => uninstall(&job, &ctx, &p),
                Kind::Warm => warm(&job, &ctx, &p, &launch),
            };
            {
                let mut j = job.lock().unwrap();
                j.running = false;
                j.pid = None;
                j.stoppable = false;
                match r {
                    _ if j.stopped => j.stage = "Stopped".into(),
                    Ok(()) => {
                        j.stage = match kind {
                            Kind::Install if updating => "Updated: ready to play".into(),
                            Kind::Install => "Installed: ready to play".into(),
                            Kind::Play => "Game closed".into(),
                            Kind::Uninstall => "Uninstalled".into(),
                            Kind::Warm => "Shaders warmed up: ready to play".into(),
                        }
                    }
                    Err(e) => {
                        j.failed = true;
                        j.stage = match kind {
                            Kind::Install if updating => "Update failed: the old version is still installed (see the log)".into(),
                            Kind::Install => "Install failed (see the log)".into(),
                            Kind::Warm => "Warm-up failed (see the log)".into(),
                            _ => "Failed".into(),
                        };
                        j.log.push(format!("ERROR: {e}"));
                    }
                }
            }
            if matches!(kind, Kind::Install | Kind::Uninstall) {
                let st = repo_state(&p);
                repos.lock().unwrap().insert(p.repo.clone(), st);
            }
            ctx.request_repaint();
        });
    }
}


fn stage(job: &Arc<Mutex<Job>>, ctx: &egui::Context, s: &str) {
    let mut j = job.lock().unwrap();
    j.stage = s.to_string();
    j.log.push(format!("== {s}"));
    ctx.request_repaint();
}

/// Downloads `p` (or its newest version) and builds it, once; it stays
/// installed until the player updates it.
fn install(job: &Arc<Mutex<Job>>, ctx: &egui::Context, p: &Project) -> Result<(), String> {
    let ws = workspace();
    std::fs::create_dir_all(ws.join("repos")).map_err(|e| format!("cannot make {}: {e}", ws.display()))?;
    let dir = repo_dir(&p.repo);
    let url = format!("https://github.com/{}.git", p.repo);
    if dir.join(".git").is_dir() {
        stage(job, ctx, "Downloading the new version");
        let refspec = p.branch.clone().unwrap_or_else(|| "HEAD".into());
        run(job, ctx, tools::git().args(["fetch", "--depth", "1", "origin", &refspec]).current_dir(&dir))?;
        run(job, ctx, tools::git().args(["reset", "--hard", "FETCH_HEAD"]).current_dir(&dir))?;
    } else {
        stage(job, ctx, "Downloading the project");
        let mut c = tools::git();
        c.args(["clone", "--depth", "1"]);
        if let Some(b) = &p.branch {
            c.args(["--branch", b]);
        }
        c.arg(&url).arg(&dir);
        run(job, ctx, &mut c)?;
    }
    stage(job, ctx, "Building (the first time takes several minutes)");
    let target = target_dir(&p.repo);
    let profile = detect_build_profile(&dir);
    let has_lockfile = dir.join("Cargo.lock").is_file();
    let mut cargo = tools::cargo();
    cargo.args(["build", "--profile", profile, "-p", "victory", "--target-dir"]);
    cargo.arg(&target);
    if has_lockfile {
        cargo.arg("--locked");
    }
    cargo.current_dir(&dir);
    run(job, ctx, &mut cargo)?;
    if !built_exe(&p.repo).is_file() {
        return Err(format!("the build made no {}", built_exe(&p.repo).display()));
    }
    let mut c = tools::git();
    c.args(["rev-parse", "HEAD"]).current_dir(&dir);
    hide_window(&mut c);
    if let Ok(out) = c.output() {
        let _ = std::fs::write(built_file(&p.repo), String::from_utf8_lossy(&out.stdout).trim());
    }
    Ok(())
}

/// Deletes `p`'s download and build (and its shader cache); its saves stay.
fn uninstall(job: &Arc<Mutex<Job>>, ctx: &egui::Context, p: &Project) -> Result<(), String> {
    if p.local_exe.is_some() {
        return Err("this build was not installed by Mandyhub".into());
    }
    stage(job, ctx, "Uninstalling");
    let ws = workspace();
    for dir in [repo_dir(&p.repo), target_dir(&p.repo), ws.join("cache").join(slug(&p.repo))] {
        if dir.exists() {
            job.lock().unwrap().log.push(format!("removing {}", dir.display()));
            ctx.request_repaint();
            std::fs::remove_dir_all(&dir).map_err(|e| format!("could not remove {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

/// The victory.exe `p` runs with: its build, or a build already on this PC.
fn project_exe(p: &Project) -> Result<PathBuf, String> {
    let exe = p.local_exe.as_ref().map(PathBuf::from).unwrap_or_else(|| built_exe(&p.repo));
    if exe.is_file() {
        Ok(exe)
    } else {
        Err("this project is not installed yet: press Install".into())
    }
}

/// The environment victory.exe runs with, the same for the warm-up and for
/// Play: the project's own env vars (from its scripts/play.ps1) first, then
/// the mode, its data and cache folders, and the player's switches.
fn victory_env(c: &mut Command, p: &Project, l: &Launch) {
    for (k, v) in detect_launch_env(&repo_dir(&p.repo)) {
        c.env(&k, &v);
    }
    if l.series_s {
        c.env("VICTORY_OWN_UPSCALER", "1");
        c.env("VICTORY_RENDER_SCALE", format!("{:.2}", l.scale));
    }
    let ws = workspace();
    let data = ws.join("data").join(slug(&p.repo));
    let _ = std::fs::create_dir_all(&data);
    c.env("VICTORY_DATA_DIR", &data);
    c.env("VICTORY_CACHE_DIR", ws.join("cache").join(slug(&p.repo)));
    for line in l.extra.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if k.starts_with("VICTORY_") {
                c.env(k, v.trim());
            }
        }
    }
}

fn victory_help(exe: &Path, subcommand: &str) -> Option<String> {
    let mut command = Command::new(exe);
    command.args([subcommand, "--help"]);
    hide_window(&mut command);
    let output = command.output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Fills the same shader cache as Play, using the commands the installed fork exposes.
fn warm(job: &Arc<Mutex<Job>>, ctx: &egui::Context, p: &Project, l: &Launch) -> Result<(), String> {
    let exe = project_exe(p)?;
    let files: Vec<PathBuf> = WARM_FILES.iter().map(|f| l.game_dir.join(f)).filter(|f| f.is_file()).collect();
    if files.is_empty() {
        return Err("the game files were not found: choose their folder in Settings".into());
    }
    let translate = victory_help(&exe, "translate");
    let supports_flag = |help: &str, flag: &str| help.split_whitespace().any(|word| word == flag);
    let translate_cache = translate.as_deref().filter(|help| supports_flag(help, "--cache"));
    let mut commands = Vec::new();
    let mut shaders = Command::new(&exe);
    if let Some(help) = translate_cache {
        shaders.args(["translate", "--cache"]);
        if l.pipelines && supports_flag(help, "--pipelines") {
            shaders.arg("--pipelines");
        }
    } else if victory_help(&exe, "shaders").is_some() {
        shaders.arg("shaders");
    } else {
        return Err("This build offers neither translate --cache nor shaders for shader warm-up.".into());
    }
    shaders.args(&files);
    commands.push(shaders);
    if l.pipelines && !translate_cache.is_some_and(|help| supports_flag(help, "--pipelines")) {
        if victory_help(&exe, "warm").is_some_and(|help| supports_flag(&help, "--used")) {
            let mut pipelines = Command::new(&exe);
            pipelines.args(["warm", "--used"]);
            commands.push(pipelines);
        } else {
            job.lock().unwrap().log.push("This build does not expose a supported pipeline warm-up. Warming shaders only.".into());
        }
    }
    stage(
        job,
        ctx,
        if l.pipelines {
            "Warming up shaders and pipelines (this can take hours)"
        } else {
            "Warming up shaders (the first time takes minutes)"
        },
    );
    for mut command in commands {
        victory_env(&mut command, p, l);
        job.lock().unwrap().stoppable = true;
        let result = run_with(job, ctx, &mut command, |pid| job.lock().unwrap().pid = Some(pid));
        let stopped = {
            let mut j = job.lock().unwrap();
            j.stoppable = false;
            j.pid = None;
            j.stopped
        };
        if stopped {
            return Err("stopped".into());
        }
        result?;
    }
    Ok(())
}

/// Starts the installed build of `p`, warming its shaders first when the player asked for that.
fn play(job: &Arc<Mutex<Job>>, ctx: &egui::Context, p: &Project, l: &Launch) -> Result<(), String> {
    let exe = project_exe(p)?;
    if l.warm_first {
        if let Err(e) = warm(job, ctx, p, l) {
            if job.lock().unwrap().stopped {
                return Err("stopped".into());
            }
            job.lock().unwrap().log.push(format!("Warm-up failed: {e}. Starting the game anyway."));
        }
    }
    stage(job, ctx, "Playing");
    let mut c = Command::new(&exe);
    c.arg("run").arg(l.game_dir.join(EXE_NAME)).arg("--execute");
    if l.series_s {
        c.args(["--console", "series-s"]);
    }
    // The player's own: more `run` flags, and after `--` the game's.
    c.args(&l.args);
    victory_env(&mut c, p, l);
    let fullscreen = l.fullscreen;
    run_with(job, ctx, &mut c, |pid| {
        if fullscreen {
            fullscreen::keep_borderless(pid);
        }
    })
}

/// Runs `c`, its output lines into the job's log.
fn run(job: &Arc<Mutex<Job>>, ctx: &egui::Context, c: &mut Command) -> Result<(), String> {
    run_with(job, ctx, c, |_| {})
}

/// [`run`], telling `spawned` the process id once it started.
fn run_with(job: &Arc<Mutex<Job>>, ctx: &egui::Context, c: &mut Command, spawned: impl FnOnce(u32)) -> Result<(), String> {
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_window(c);
    let mut child = c.spawn().map_err(|e| format!("could not start {:?}: {e}", c.get_program()))?;
    spawned(child.id());
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let pipe = |r: Box<dyn std::io::Read + Send>| {
        let (job, ctx) = (job.clone(), ctx.clone());
        std::thread::spawn(move || {
            for line in BufReader::new(r).lines().map_while(Result::ok) {
                let mut j = job.lock().unwrap();
                j.log.push(line);
                if j.log.len() > 4000 {
                    j.log.drain(..1000);
                }
                drop(j);
                ctx.request_repaint();
            }
        })
    };
    let a = out.map(|o| pipe(Box::new(o)));
    let b = err.map(|e| pipe(Box::new(e)));
    let status = child.wait().map_err(|e| e.to_string())?;
    for t in [a, b].into_iter().flatten() {
        let _ = t.join();
    }
    status.success().then_some(()).ok_or_else(|| format!("{:?} ended with {status}", c.get_program()))
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.saved);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.title_bar_done {
            self.title_bar_done = fullscreen::dark_title_bar(std::process::id(), theme::BG, theme::WHITE);
        }
        // A size remembered from a minimized window can come back tiny: then the normal size.
        if !self.size_checked {
            let (rect, minimized) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().minimized));
            if let Some(r) = rect {
                if minimized != Some(true) && (r.width() < 960.0 || r.height() < 620.0) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1280.0, 800.0)));
                }
                self.size_checked = true;
            }
        }
        self.draw(ctx);
    }
}

impl App {
    /// Installs what is missing with winget, then looks again.
    fn start_tools(&mut self, ctx: &egui::Context) {
        let job = self.job.clone();
        {
            let mut j = job.lock().unwrap();
            if j.running {
                return;
            }
            *j = Job { running: true, stage: "Installing tools (Windows will ask for permission)".into(), ..Default::default() };
        }
        self.show_log = true;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let have = tools::detect();
            let result = if !tools::has_winget() {
                Err("winget (the Windows package manager) is not on this PC. Install them by hand: git-scm.com, rustup.rs and the Visual Studio Build Tools (Desktop development with C++).".to_string())
            } else {
                let mut r = Ok(());
                for (what, args) in tools::install_steps(&have) {
                    stage(&job, &ctx, &format!("Installing {what} (this can take a while)"));
                    let mut c = Command::new("winget");
                    c.args(&args);
                    if let Err(e) = run(&job, &ctx, &mut c) {
                        r = Err(format!("{what}: {e}"));
                        break;
                    }
                }
                r
            };
            let now = tools::detect();
            let mut j = job.lock().unwrap();
            j.running = false;
            match result {
                Ok(()) if now.ready() => j.stage = "Tools installed: ready to install projects".into(),
                Ok(()) => {
                    j.failed = true;
                    j.stage = format!("Still missing: {}. A restart of Windows can help.", now.missing().join(", "));
                }
                Err(e) => {
                    j.failed = true;
                    j.stage = "Tools not installed (see the log)".into();
                    j.log.push(format!("ERROR: {e}"));
                }
            }
            ctx.request_repaint();
        });
    }

    /// A new install folder: asks first if projects are installed in the old one.
    fn choose_install_dir(&mut self, new: PathBuf) {
        let old = workspace();
        if new == old {
            return;
        }
        if has_projects(&old) {
            self.pending_move = Some((old, new));
        } else {
            self.apply_install_dir(&new);
        }
    }

    fn apply_install_dir(&mut self, new: &Path) {
        self.saved.install_dir = if new == default_workspace() { String::new() } else { new.display().to_string() };
        set_install_dir(&self.saved.install_dir);
        // What is installed where is looked at again.
        self.repos.lock().unwrap().clear();
        self.last_auto_check = -1.0;
    }

    /// Moves every installed project from `old` to `new`, then uses `new`.
    fn start_move(&mut self, ctx: &egui::Context, old: PathBuf, new: PathBuf) {
        let job = self.job.clone();
        {
            let mut j = job.lock().unwrap();
            if j.running {
                return;
            }
            *j = Job { running: true, stage: "Moving installed projects".into(), ..Default::default() };
        }
        let (ctx, repos) = (ctx.clone(), self.repos.clone());
        std::thread::spawn(move || {
            let log = |line: String| {
                job.lock().unwrap().log.push(line);
                ctx.request_repaint();
            };
            let mut result = Ok(());
            for part in PROJECT_PARTS {
                log(format!("moving {}", old.join(part).display()));
                if let Err(e) = move_dir(&old.join(part), &new.join(part), &log) {
                    result = Err(format!("could not move {}: {e}", old.join(part).display()));
                    break;
                }
            }
            let mut j = job.lock().unwrap();
            j.running = false;
            match result {
                Ok(()) => j.stage = "Moved: projects are in the new folder".into(),
                Err(e) => {
                    j.failed = true;
                    j.stage = "Failed".into();
                    j.log.push(format!("ERROR: {e}"));
                }
            }
            repos.lock().unwrap().clear();
            ctx.request_repaint();
        });
    }

    /// At start and every 15 minutes: every installed project is checked for
    /// a newer version online (no download; `git ls-remote`).
    fn auto_check(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let listed = self.projects.lock().unwrap().is_some();
        let due = if self.last_auto_check < 0.0 { listed || now > 10.0 } else { now - self.last_auto_check > AUTO_CHECK_SECS };
        if !due {
            ctx.request_repaint_after(std::time::Duration::from_secs(30));
            return;
        }
        self.last_auto_check = now;
        for p in self.all_projects() {
            if p.local_exe.is_none() && built_exe(&p.repo).is_file() {
                self.check_repo(ctx, &p);
            }
        }
    }
}
