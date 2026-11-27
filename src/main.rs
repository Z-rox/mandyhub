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

#[allow(dead_code)]
mod design;
mod fullscreen;
mod look;
mod selfupdate;
mod slides;
mod tools;

use eframe::egui::{self, Color32, RichText, Rounding, Stroke, Vec2};
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

// Colours: a dark night with a sunset accent.
const CARD: Color32 = Color32::from_rgb(34, 28, 56);
const CARD_SEL: Color32 = Color32::from_rgb(70, 36, 86);
const PINK: Color32 = Color32::from_rgb(255, 72, 160);
const ORANGE: Color32 = Color32::from_rgb(255, 150, 60);
const TEAL: Color32 = Color32::from_rgb(60, 220, 210);
const TEXT: Color32 = Color32::from_rgb(235, 230, 245);
const DIM: Color32 = Color32::from_rgb(150, 140, 175);
const GOOD: Color32 = Color32::from_rgb(90, 220, 120);
const BAD: Color32 = Color32::from_rgb(255, 90, 90);

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
    /// Where the repo lives: `None` GitHub, else the base address of another git host (the repo hub).
    #[serde(default)]
    host: Option<String>,
}

impl Project {
    /// The address git clones and fetches from.
    fn git_url(&self) -> String {
        format!(
            "{}/{}.git",
            self.host.as_deref().unwrap_or("https://github.com"),
            self.repo
        )
    }
    /// The repo's page in a browser.
    fn web_url(&self) -> String {
        format!(
            "{}/{}",
            self.host.as_deref().unwrap_or("https://github.com"),
            self.repo
        )
    }
    /// What the details panel shows for the link (the hub's address is not shown).
    fn web_label(&self) -> String {
        match self.host {
            None => format!("github.com/{}", self.repo),
            Some(_) => format!("{} on the repo hub", self.repo),
        }
    }
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
    /// Where the project list comes from: 0 PortWatch, 1 the repo hub.
    source: usize,
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
}

struct App {
    saved: Saved,
    projects: Arc<Mutex<Option<Result<Vec<Project>, String>>>>,
    job: Arc<Mutex<Job>>,
    custom_link: String,
    custom_error: Option<String>,
    files: Vec<(&'static str, bool)>,
    files_for: String,
    tools: Option<tools::Tools>,
    /// A custom link waiting for "Add it": (host, owner/name), host `None` for GitHub.
    confirm_custom: Option<(Option<String>, String)>,
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
    show_settings: bool,
    show_log: bool,
    /// The moving pictures behind everything.
    slides: slides::Slides,
    /// A newer Mandyhub on GitHub, if one was found (selfupdate.rs).
    selfup: Arc<Mutex<selfupdate::SelfUpdate>>,
    /// Descriptions of custom repos, fetched from their sites (etch_about).
    about: Arc<Mutex<std::collections::HashMap<String, String>>>,
    /// When every installed project was last checked for updates (seconds).
    last_auto_check: f64,
    /// The project the update notification is about, while it shows.
    notice: Option<String>,
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
    // Another Rust project: the program its build made, as noted after the build.
    if let Ok(p) = std::fs::read_to_string(target_dir(repo).join("built-exe.txt")) {
        return PathBuf::from(p.trim());
    }
    let dir = repo_dir(repo);
    let profile = detect_build_profile(&dir);
    target_dir(repo).join(profile).join("victory.exe")
}

/// Whether a cloned repo is a VIctory project (has the `victory` program):
/// it is built with `-p victory` and played with the game; any other Rust repo
/// is built whole and its own program is started on its own.
fn is_victory(dir: &Path) -> bool {
    ["apps/victory/Cargo.toml", "crates/victory/Cargo.toml", "victory/Cargo.toml"]
        .iter()
        .any(|p| dir.join(p).is_file())
}

/// After a build of another Rust project: the program it made, in
/// `target/<profile>` (the one named like the repo, else the largest).
fn find_built_exe(out: &Path, repo: &str) -> Option<PathBuf> {
    let mut exes: Vec<PathBuf> = std::fs::read_dir(out)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        .collect();
    let want = repo.rsplit(['/', '-']).next().unwrap_or(repo).to_lowercase().replace(['-', '_'], "");
    if let Some(i) = exes.iter().position(|p| {
        p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.to_lowercase().replace(['-', '_'], "") == want)
    }) {
        return Some(exes.swap_remove(i));
    }
    exes.sort_by_key(|p| std::cmp::Reverse(p.metadata().map(|m| m.len()).unwrap_or(0)));
    exes.into_iter().next()
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
    let Ok(text) = std::fs::read_to_string(&script) else {
        // Some projects keep their tuned settings in a Windows batch file
        // (Play-*.cmd or .bat in the repo's top folder) instead.
        return detect_cmd_env(repo_dir);
    };
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
/// Reads `set "VICTORY_NAME=value"` lines from a project's `Play*.cmd` / `Play*.bat`
/// (as in `if not defined VICTORY_NAME set "VICTORY_NAME=1"`). Values that name
/// another variable (`%...%`) are skipped, as are the ones the launcher sets itself
/// (game folder, program to run, data and cache folders).
fn detect_cmd_env(repo_dir: &Path) -> Vec<(String, String)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(repo_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
            name.starts_with("play") && (name.ends_with(".cmd") || name.ends_with(".bat"))
        })
        .collect();
    files.sort();
    let mut vars: Vec<(String, String)> = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        vars.extend(parse_cmd_env(&text));
    }
    vars
}

fn parse_cmd_env(text: &str) -> Vec<(String, String)> {
    const OWN: [&str; 6] =
        ["VICTORY_GAME_DIR", "VICTORY_RUNNER", "VICTORY_DATA_DIR", "VICTORY_CACHE_DIR", "VICTORY_CONSOLE", "VICTORY_KEYS"];
    let mut vars: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.to_ascii_lowercase().starts_with("rem ") || line.starts_with("::") {
            continue;
        }
        // Everything after `set "` up to the closing quote.
        let lower = line.to_ascii_lowercase();
        let Some(at) = lower.find("set \"") else { continue };
        let rest = &line[at + 5..];
        let Some(end) = rest.rfind('"') else { continue };
        let Some((k, v)) = rest[..end].split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if k.starts_with("VICTORY_") && !k.contains(' ') && !v.contains('%') && !OWN.contains(&k) {
            // The last assignment of a name wins, as in the batch file.
            vars.retain(|(name, _)| name != k);
            vars.push((k.to_string(), v.to_string()));
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
    let url = p.git_url();
    let mut c = tools::git();
    c.arg("ls-remote").arg(&url);
    match &p.branch {
        Some(b) => c.arg(format!("refs/heads/{b}")),
        None => c.arg("HEAD"),
    };
    hide_window(&mut c);
    let out = c.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace()
        .next()
        .filter(|h| h.len() == 40)
        .map(str::to_string)
}

fn repo_state(p: &Project) -> RepoState {
    if let Some(exe) = &p.local_exe {
        return RepoState {
            checking: false,
            installed: Path::new(exe).is_file(),
            built: Some("local".into()),
            remote: None,
        };
    }
    RepoState {
        checking: false,
        installed: built_exe(&p.repo).is_file(),
        built: std::fs::read_to_string(built_file(&p.repo))
            .ok()
            .map(|s| s.trim().to_string()),
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
        let skip = [
            "windows",
            "program files",
            "program files (x86)",
            "programdata",
            "appdata",
        ];
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
            .with_inner_size([1280.0, 780.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    migrate_old_folders();
    eframe::run_native(
        "Mandyhub",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// Where the launcher keeps repos it downloads: `%LOCALAPPDATA%\Mandyhub`.
fn workspace() -> PathBuf {
    INSTALL_DIR
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(default_workspace)
}

/// `%LOCALAPPDATA%\Mandyhub`: where projects go unless the player chose
/// another folder (Settings: Install folder).
fn default_workspace() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
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
    std::fs::read_dir(dir.join("repos"))
        .map(|mut d| d.next().is_some())
        .unwrap_or(false)
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
        let Some(base) = std::env::var_os(var).map(PathBuf::from) else {
            continue;
        };
        let (old, new) = (base.join("VI Launcher"), base.join("Mandyhub"));
        if old.is_dir() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}

/// Words kept out of the source as text: the game's name in the ways other
/// sites write it, to replace or skip (`clean_text`, `names_the_game`), and
/// PortWatch's field name for scores. Each byte is XORed with 0x5A.
const NAMES: &[&[u8]] = &[
    &[
        29, 40, 59, 52, 62, 122, 14, 50, 63, 60, 46, 122, 27, 47, 46, 53, 122, 12, 19,
    ],
    &[
        29, 40, 59, 52, 62, 122, 14, 50, 63, 60, 46, 122, 27, 47, 46, 53, 122, 108,
    ],
    &[29, 14, 27, 122, 12, 19],
    &[29, 14, 27, 122, 108],
    &[29, 14, 27, 108],
    &[29, 14, 27, 12, 19],
    &[61, 46, 59, 108],
    &[61, 46, 59, 122, 108],
];
const KEYS: &[&[u8]] = &[
    &[61, 46, 59, 108],
    &[61, 46, 59, 44, 51],
    &[61, 40, 59, 52, 62, 46, 50, 63, 60, 46, 59, 47, 46, 53],
];
const FIELD: &[u8] = &[61, 46, 59];

fn hidden(words: &[&[u8]]) -> Vec<String> {
    words
        .iter()
        .map(|w| w.iter().map(|b| (b ^ 0x5A) as char).collect())
        .collect()
}

/// Text from other sites without the game's name: "the game" in its place.
fn clean_text(s: &str) -> String {
    let mut out = s.to_string();
    for pat in hidden(NAMES) {
        let pat = pat.as_str();
        for (from, to) in [
            (format!("{pat}'s"), "the game's".to_string()),
            (pat.to_string(), "the game".to_string()),
        ] {
            out = out.replace(&from, &to);
        }
    }
    out.replace(" PC port of the game", " PC port")
        .replace("the game PC", "the game on PC")
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
    let ok = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    (ok(owner) && ok(name)).then(|| format!("{owner}/{name}"))
}

/// A custom repo link from any git site: `(host, owner/name)`, host `None` for GitHub.
/// Takes `https://site/owner/name(.git)`, `site/owner/name`, `owner/name` (GitHub),
/// and deeper paths (GitLab groups: `site/group/sub/name`).
fn parse_link(link: &str) -> Option<(Option<String>, String)> {
    // A first part that is a site other than GitHub ("gitlab.com/...") is not a GitHub owner.
    let bare = link.trim().split_once("://").map_or(link.trim(), |(_, r)| r);
    let first = bare.split('/').next().unwrap_or("");
    let other_site = first.contains('.') && !first.eq_ignore_ascii_case("github.com");
    if !other_site {
        if let Some(r) = parse_repo(link) {
            return Some((None, r));
        }
    }
    let s = link.trim().trim_end_matches('/').trim_end_matches(".git");
    let (scheme, rest) = match s.split_once("://") {
        Some((sch, rest)) if sch == "https" || sch == "http" => (sch, rest),
        Some(_) => return None,
        None => ("https", s),
    };
    let (site, path) = rest.split_once('/')?;
    // A site name has a dot (or is localhost with a port); the path has owner/name at least.
    if !(site.contains('.') || site.starts_with("localhost")) || site.contains(['@', ' ']) {
        return None;
    }
    let path = path.split(['?', '#']).next()?.trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    let ok = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if parts.len() < 2 || !parts.iter().all(|p| ok(p)) {
        return None;
    }
    // GitLab's web pages put /-/tree/... after the repo: cut there.
    let parts: Vec<&str> = parts.into_iter().take_while(|p| *p != "-").collect();
    if parts.len() < 2 {
        return None;
    }
    Some((Some(format!("{scheme}://{site}")), parts.join("/")))
}

/// The public projects on PortWatch, best first.
fn fetch_portwatch() -> Result<Vec<Project>, String> {
    let body: serde_json::Value = ureq::get(PORTWATCH)
        .set("User-Agent", "mandyhub")
        .call()
        .map_err(|e| format!("PortWatch could not be reached: {e}"))?
        .into_json()
        .map_err(|e| format!("PortWatch sent something unreadable: {e}"))?;
    let efforts = body["efforts"]
        .as_array()
        .ok_or("PortWatch's list has no projects")?;
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
            if e["stats"]["missing"].as_bool() == Some(true)
                || e["stats"]["archived"].as_bool() == Some(true)
            {
                return None;
            }
            Some(Project {
                name: clean_text(e["name"].as_str().unwrap_or(&repo)),
                branch: e["branch"].as_str().map(str::to_string),
                gpu: e["gpu"]
                    .as_array()
                    .map(|g| {
                        g.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                fps: e[field.as_str()]["fps"].as_f64(),
                render: e[field.as_str()]["render"].as_f64(),
                verified: e["verification"].as_str() == Some("portwatch"),
                summary: clean_text(e["summary"].as_str().unwrap_or("")),
                custom: false,
                local_exe: None,
                host: None,
                repo,
            })
        })
        .collect();
    let score = |p: &Project| {
        p.render.unwrap_or(0.0) * 10.0 + p.fps.unwrap_or(0.0) + if p.verified { 5.0 } else { 0.0 }
    };
    out.sort_by(|a, b| score(b).total_cmp(&score(a)));
    Ok(out)
}

/// The repo hub's address (a Gitea server people mirror projects to). XORed with 0x5A like `NAMES`.
const HUB: &[u8] = &[
    50, 46, 46, 42, 41, 96, 117, 117, 61, 51, 46, 116, 61, 46, 59, 50, 47, 56, 116, 41, 46,
];

/// The Rust projects on the repo hub, newest first (Gitea's search API, up to 250 repos).
fn fetch_hub() -> Result<Vec<Project>, String> {
    let base = hidden(&[HUB]).remove(0);
    let mut out = Vec::new();
    for page in 1..=5 {
        let url =
            format!("{base}/api/v1/repos/search?limit=50&page={page}&sort=updated&order=desc");
        let body: serde_json::Value = ureq::get(&url)
            .set("User-Agent", "mandyhub")
            .call()
            .map_err(|e| format!("The repo hub could not be reached: {e}"))?
            .into_json()
            .map_err(|e| format!("The repo hub sent something unreadable: {e}"))?;
        let repos = body["data"]
            .as_array()
            .ok_or("The repo hub's list has no repos")?;
        for r in repos {
            let full = r["full_name"].as_str().unwrap_or_default().to_string();
            let name = r["name"].as_str().unwrap_or_default();
            // Only buildable Rust projects; not empty, archived, or named after the game.
            if r["language"].as_str() != Some("Rust")
                || r["empty"].as_bool() == Some(true)
                || r["archived"].as_bool() == Some(true)
                || full.is_empty()
                || names_the_game(&full)
            {
                continue;
            }
            // Mirrors are named owner--repo under a mirror account: shown as "repo (owner)".
            let shown = match name.split_once("--") {
                Some((owner, repo)) => format!("{repo} ({owner})"),
                None => format!("{name} ({})", full.split('/').next().unwrap_or("")),
            };
            out.push(Project {
                name: clean_text(&shown),
                repo: full,
                branch: r["default_branch"].as_str().map(str::to_string),
                gpu: Vec::new(),
                fps: None,
                render: None,
                verified: false,
                summary: clean_text(r["description"].as_str().unwrap_or("")),
                custom: false,
                local_exe: None,
                host: Some(base.clone()),
            });
        }
        if repos.len() < 50 {
            break;
        }
    }
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
        style(&cc.egui_ctx);
        let mut saved: Saved = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
            .unwrap_or_default();
        if saved.render_scale <= 0.0 {
            saved.render_scale = 0.5;
        }
        let mut app = App {
            saved,
            projects: Arc::new(Mutex::new(None)),
            job: Arc::new(Mutex::new(Job::default())),
            custom_link: String::new(),
            custom_error: None,
            files: Vec::new(),
            files_for: String::new(),
            tools: None,
            confirm_custom: None,
            confirm_uninstall: None,
            pending_move: None,
            empty: false,
            finding: Arc::new(Mutex::new(None)),
            repos: Arc::new(Mutex::new(std::collections::HashMap::new())),
            show_settings: false,
            show_log: false,
            slides: slides::Slides::new(),
            selfup: Arc::new(Mutex::new(selfupdate::SelfUpdate::default())),
            about: Arc::new(Mutex::new(std::collections::HashMap::new())),
            last_auto_check: -1.0,
            notice: None,
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
        selfupdate::check(app.selfup.clone(), cc.egui_ctx.clone());
        app.fetch_about(&cc.egui_ctx);
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
        let (slot, ctx, source) = (self.projects.clone(), ctx.clone(), self.saved.source);
        std::thread::spawn(move || {
            let r = if source == 1 {
                fetch_hub()
            } else {
                fetch_portwatch()
            };
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
            && std::fs::read_dir(&dir)
                .map(|mut d| d.next().is_none())
                .unwrap_or(true);
        self.files = REQUIRED
            .iter()
            .map(|f| (*f, !self.saved.game_dir.is_empty() && dir.join(f).is_file()))
            .collect();
        self.files_for = self.saved.game_dir.clone();
    }

    fn files_ok(&self) -> bool {
        !self.files.is_empty() && self.files.iter().all(|f| f.1)
    }

    fn all_projects(&self) -> Vec<Project> {
        let mut v: Vec<Project> = self.saved.custom.clone();
        let about = self.about.lock().unwrap();
        if let Some(Ok(list)) = &*self.projects.lock().unwrap() {
            // A custom repo that is also on the list shows the list's data (fps, GPU,
            // verified, description) and stays custom; the list's copy is not shown twice.
            let same = |a: &Project, b: &Project| a.repo.eq_ignore_ascii_case(&b.repo) && a.host == b.host;
            for c in v.iter_mut() {
                if let Some(l) = list.iter().find(|l| same(l, c)) {
                    c.gpu = l.gpu.clone();
                    c.fps = l.fps;
                    c.render = l.render;
                    c.verified = l.verified;
                    c.summary = l.summary.clone();
                    if c.branch.is_none() {
                        c.branch = l.branch.clone();
                    }
                }
            }
            let custom = v.clone();
            v.extend(list.iter().filter(|l| !custom.iter().any(|c| same(c, l))).cloned());
        }
        // Not on the list: the repo's own description from its site (fetched at start and on add).
        for c in v.iter_mut().filter(|p| p.custom && p.summary == "Added by link.") {
            if let Some(d) = about.get(&c.repo) {
                c.summary = d.clone();
            }
        }
        v
    }

    /// Fetches the description of each custom repo from its site (GitHub or a Gitea server).
    fn fetch_about(&self, ctx: &egui::Context) {
        let (about, ctx) = (self.about.clone(), ctx.clone());
        let repos: Vec<(Option<String>, String)> = self.saved.custom.iter().map(|p| (p.host.clone(), p.repo.clone())).collect();
        std::thread::spawn(move || {
            for (host, repo) in repos {
                if about.lock().unwrap().contains_key(&repo) {
                    continue;
                }
                let url = match &host {
                    None => format!("https://api.github.com/repos/{repo}"),
                    Some(h) => format!("{h}/api/v1/repos/{repo}"),
                };
                let Ok(r) = ureq::get(&url).set("User-Agent", "mandyhub").call() else { continue };
                let Ok(j) = r.into_json::<serde_json::Value>() else { continue };
                let d = clean_text(j["description"].as_str().unwrap_or("").trim());
                if !d.is_empty() {
                    about.lock().unwrap().insert(repo, d);
                    ctx.request_repaint();
                }
            }
        });
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
        let built = std::fs::read_to_string(built_file(&p.repo))
            .ok()
            .map(|s| s.trim().to_string());
        let newer = built.as_deref() != Some(remote.as_str());
        let later = self
            .saved
            .later
            .iter()
            .any(|(r, c)| r == &p.repo && c == &remote);
        (newer && !later).then_some(remote)
    }

    fn start(&mut self, ctx: &egui::Context, p: Project, kind: Kind) {
        let job = self.job.clone();
        {
            let mut j = job.lock().unwrap();
            if j.running {
                return;
            }
            *j = Job {
                running: true,
                stage: "Starting".into(),
                repo: Some(p.repo.clone()),
                ..Default::default()
            };
        }
        // An install over an installed project is an update.
        let updating = kind == Kind::Install && built_exe(&p.repo).is_file();
        let game_dir = PathBuf::from(&self.saved.game_dir);
        let series_s = self.saved.series_s;
        let fullscreen = self.saved.fullscreen;
        let scale = self.saved.render_scale;
        let mut extra = self.saved.extra_env.clone();
        if self.saved.fix_8gb {
            extra.push_str("\nVICTORY_TEXTURE_DROP_MIPS=2\nVICTORY_NULL_COLOR=grey");
        }
        if self.saved.bright_shadows {
            extra.push_str("\nVICTORY_AMBIENT=0.6");
        }
        let (ctx, repos) = (ctx.clone(), self.repos.clone());
        std::thread::spawn(move || {
            let r = match kind {
                Kind::Install => install(&job, &ctx, &p),
                Kind::Play => play(
                    &job, &ctx, &p, &game_dir, series_s, scale, &extra, fullscreen,
                ),
                Kind::Uninstall => uninstall(&job, &ctx, &p),
            };
            {
                let mut j = job.lock().unwrap();
                j.running = false;
                match r {
                    Ok(()) => {
                        j.stage = match kind {
                            Kind::Install if updating => "Updated: ready to play".into(),
                            Kind::Install => "Installed: ready to play".into(),
                            Kind::Play => "Game closed".into(),
                            Kind::Uninstall => "Uninstalled".into(),
                        }
                    }
                    Err(e) => {
                        j.failed = true;
                        j.stage = match kind {
                            Kind::Install if updating => {
                                "Update failed: the old version is still installed (see the log)"
                                    .into()
                            }
                            Kind::Install => "Install failed (see the log)".into(),
                            _ => "Failed".into(),
                        };
                        j.log.push(format!("ERROR: {e}"));
                    }
                }
            }
            if kind != Kind::Play {
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
    std::fs::create_dir_all(ws.join("repos"))
        .map_err(|e| format!("cannot make {}: {e}", ws.display()))?;
    let dir = repo_dir(&p.repo);
    let url = p.git_url();
    if dir.join(".git").is_dir() {
        stage(job, ctx, "Downloading the new version");
        let refspec = p.branch.clone().unwrap_or_else(|| "HEAD".into());
        run(
            job,
            ctx,
            tools::git()
                .args(["fetch", "--depth", "1", "origin", &refspec])
                .current_dir(&dir),
        )?;
        run(
            job,
            ctx,
            tools::git()
                .args(["reset", "--hard", "FETCH_HEAD"])
                .current_dir(&dir),
        )?;
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
    if !dir.join("Cargo.toml").is_file() {
        return Err("this repo is not a Rust project (no Cargo.toml): Mandyhub cannot build it".into());
    }
    let victory = is_victory(&dir);
    // Projects with a native engine built by a script first (Kelvaroq: scripts/build_native.py).
    let native = dir.join("scripts").join("build_native.py");
    if !victory && native.is_file() {
        stage(job, ctx, "Building the project's native engine (scripts/build_native.py)");
        let mut py = Command::new("python");
        py.arg(&native).current_dir(&dir);
        run(job, ctx, &mut py).map_err(|e| format!("{e} (this project needs Python: python.org, or 'winget install Python.Python.3.12')"))?;
    }
    let note = target.join("built-exe.txt");
    let _ = std::fs::remove_file(&note);
    let mut cargo = tools::cargo();
    cargo.args(["build", "--profile", profile]);
    if victory {
        cargo.args(["-p", "victory"]);
    } else {
        stage(job, ctx, "Building (not a VIctory project: the whole repo is built)");
    }
    cargo.arg("--target-dir").arg(&target);
    if has_lockfile {
        cargo.arg("--locked");
    }
    cargo.current_dir(&dir);
    run(job, ctx, &mut cargo)?;
    if !victory {
        let out = target.join(if profile == "dev" { "debug" } else { profile });
        let exe = find_built_exe(&out, &p.repo).ok_or_else(|| format!("the build made no program in {}", out.display()))?;
        let _ = std::fs::write(&note, exe.display().to_string());
    }
    if !built_exe(&p.repo).is_file() {
        return Err(format!(
            "the build made no {}",
            built_exe(&p.repo).display()
        ));
    }
    let mut c = tools::git();
    c.args(["rev-parse", "HEAD"]).current_dir(&dir);
    hide_window(&mut c);
    if let Ok(out) = c.output() {
        let _ = std::fs::write(
            built_file(&p.repo),
            String::from_utf8_lossy(&out.stdout).trim(),
        );
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
    for dir in [
        repo_dir(&p.repo),
        target_dir(&p.repo),
        ws.join("cache").join(slug(&p.repo)),
    ] {
        if dir.exists() {
            job.lock()
                .unwrap()
                .log
                .push(format!("removing {}", dir.display()));
            ctx.request_repaint();
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("could not remove {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

/// Starts the installed build of `p`.
fn play(
    job: &Arc<Mutex<Job>>,
    ctx: &egui::Context,
    p: &Project,
    game_dir: &Path,
    series_s: bool,
    scale: f32,
    extra: &str,
    fullscreen: bool,
) -> Result<(), String> {
    let exe = p
        .local_exe
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| built_exe(&p.repo));
    if !exe.is_file() {
        return Err("this project is not installed yet: press Install".into());
    }
    // Not a VIctory project: its own program, started on its own (no game arguments).
    if p.local_exe.is_none() && target_dir(&p.repo).join("built-exe.txt").is_file() {
        stage(job, ctx, "Running the project's program");
        Command::new(&exe)
            .current_dir(exe.parent().unwrap_or(Path::new(".")))
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", exe.display()))?;
        return Ok(());
    }
    stage(job, ctx, "Playing");
    let ws = workspace();
    let mut c = Command::new(&exe);
    c.arg("run").arg(game_dir.join(EXE_NAME)).arg("--execute");
    // The project's own env vars (from its scripts/play.ps1) go first;
    // the player's switches and extra lines can override them.
    let dir = repo_dir(&p.repo);
    for (k, v) in detect_launch_env(&dir) {
        c.env(&k, &v);
    }
    if series_s {
        c.args(["--console", "series-s"]);
        c.env("VICTORY_OWN_UPSCALER", "1");
        c.env("VICTORY_RENDER_SCALE", format!("{scale:.2}"));
    }
    let data = ws.join("data").join(slug(&p.repo));
    let _ = std::fs::create_dir_all(&data);
    c.env("VICTORY_DATA_DIR", &data);
    c.env("VICTORY_CACHE_DIR", ws.join("cache").join(slug(&p.repo)));
    for line in extra.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if k.starts_with("VICTORY_") {
                c.env(k, v.trim());
            }
        }
    }
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
fn run_with(
    job: &Arc<Mutex<Job>>,
    ctx: &egui::Context,
    c: &mut Command,
    spawned: impl FnOnce(u32),
) -> Result<(), String> {
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_window(c);
    let mut child = c
        .spawn()
        .map_err(|e| format!("could not start {:?}: {e}", c.get_program()))?;
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
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{:?} ended with {status}", c.get_program()))
}

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = Color32::from_rgba_unmultiplied(14, 8, 30, 245);
    v.window_rounding = Rounding::same(14.0);
    v.menu_rounding = Rounding::same(10.0);
    v.window_stroke = Stroke::new(1.5_f32, PINK);
    v.extreme_bg_color = Color32::from_rgb(10, 8, 18);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = PINK;
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.hovered.weak_bg_fill = CARD_SEL;
    v.widgets.active.weak_bg_fill = PINK;
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.rounding = Rounding::same(8.0);
    }
    v.widgets.noninteractive.rounding = Rounding::same(8.0);
    v.widgets.open.rounding = Rounding::same(8.0);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, PINK);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgba_unmultiplied(40, 20, 70, 220);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgba_unmultiplied(90, 30, 110, 235);
    ctx.set_visuals(v);
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = Vec2::new(10.0, 8.0);
    s.spacing.button_padding = Vec2::new(14.0, 8.0);
    ctx.set_style(s);
    design::text_sizes(ctx);
}

fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    look::tag(ui, text, color, false);
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.saved);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // What the last look found (a look runs at start, after "Check
        // again" and after the tools were installed).
        self.tools = tools::current();
        self.check_files();
        let playing = {
            let j = self.job.lock().unwrap();
            j.running && j.stage == "Playing"
        };
        // While a game runs the launcher stands still: the card is the game's.
        // The player's pictures, slowly moving; the neon scene without any.
        let bt = design::anim_time(ctx, !playing);
        if self.slides.paint(ctx, bt) {
            if !playing {
                ctx.request_repaint_after(std::time::Duration::from_millis(33));
            }
        } else {
            design::background(ctx, !playing);
        }
        let t = design::time(ctx);
        selfupdate::window(&self.selfup, ctx);

        // Header.
        let header = egui::TopBottomPanel::top("header")
            .frame(look::glass().inner_margin(egui::Margin::symmetric(20.0, 12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let _ = t;
                    look::gradient_title(ui, "MANDYHUB", 44.0, look::ORANGE, look::PINK);
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("COMMUNITY LAUNCHER")
                                .color(DIM)
                                .size(12.0)
                                .strong(),
                        );
                        ui.label(
                            RichText::new("by Zettrox")
                                .color(look::ORANGE)
                                .size(13.0)
                                .strong(),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let b = egui::Button::new(RichText::new("⚙  SETTINGS").strong().size(15.0))
                            .min_size(Vec2::new(130.0, 36.0));
                        if ui.add(b).clicked() {
                            self.show_settings = !self.show_settings;
                        }
                    });
                });
            });
        let page_top = header.response.rect.bottom();

        // Bottom: game folder, settings, play.
        egui::TopBottomPanel::bottom("play")
            .frame(look::glass().inner_margin(16.0))
            .show(ctx, |ui| {
                self.bottom(ui, ctx);
            });

        // Left: the list.
        egui::SidePanel::left("list")
            .exact_width((ctx.screen_rect().width() * 0.5).clamp(420.0, 700.0))
            .frame(look::glass().inner_margin(12.0))
            .show(ctx, |ui| {
                self.list(ui, ctx);
            });

        // Right: details and log.
        egui::CentralPanel::default()
            .frame(look::glass().inner_margin(egui::Margin {
                left: 18.0,
                right: 18.0,
                top: 14.0,
                bottom: 8.0,
            }))
            .show(ctx, |ui| {
                self.details(ui);
            });

        self.auto_check(ctx);
        self.update_notice(ctx);

        // Settings: a page of its own that slides in from the right over the
        // main page, and out again with Back.
        let a =
            ctx.animate_bool_with_time(egui::Id::new("settings-page"), self.show_settings, 0.38);
        if a > 0.001 {
            let eased = 1.0 - (1.0 - a).powi(3);
            let screen = ctx.screen_rect();
            let page = egui::Rect::from_min_max(egui::pos2(screen.left(), page_top), screen.max);
            let id = egui::Id::new("settings-area");
            // The main page dims as the settings page comes over it.
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new("dim"),
            ))
            .rect_filled(page, 0.0, Color32::from_black_alpha((150.0 * eased) as u8));
            egui::Area::new(id)
                .order(egui::Order::Foreground)
                .fixed_pos(page.min)
                .show(ctx, |ui| {
                    look::glass()
                        .fill(Color32::from_rgba_unmultiplied(14, 8, 24, 225))
                        .inner_margin(egui::Margin::symmetric(40.0, 24.0))
                        .show(ui, |ui| {
                            ui.set_min_size(page.size() - Vec2::new(80.0, 48.0));
                            ui.set_max_width(page.width() - 80.0);
                            ui.horizontal(|ui| {
                                if look::filter_pill(ui, "◀  Back", false).clicked() {
                                    self.show_settings = false;
                                }
                                ui.add_space(16.0);
                                look::gradient_title(
                                    ui,
                                    "Settings",
                                    40.0,
                                    look::ORANGE,
                                    look::PINK,
                                );
                            });
                            ui.add_space(18.0);
                            // The page grew past the window (backgrounds): it scrolls.
                            egui::ScrollArea::vertical()
                                .id_salt("settings-scroll")
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    self.settings(ui, ctx);
                                });
                        });
                });
            let slide = (1.0 - eased) * page.width();
            ctx.set_transform_layer(
                egui::LayerId::new(egui::Order::Foreground, id),
                egui::emath::TSTransform::from_translation(Vec2::new(slide, 0.0)),
            );
            if a < 1.0 {
                ctx.request_repaint();
            }
        }

        if let Some((old, new)) = self.pending_move.clone() {
            let mut open = true;
            egui::Window::new(RichText::new("Move installed projects?").strong().size(18.0))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(RichText::new(format!("New install folder:\n{}", new.display())).color(Color32::WHITE));
                    ui.label(RichText::new("Your installed projects (and their saves) are in the old folder. Move them to the new one? Moving to another drive can take a while.").color(DIM));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(RichText::new("Move them").color(Color32::WHITE)).fill(look::CORAL);
                        if ui.add(b).clicked() {
                            self.apply_install_dir(&new);
                            self.start_move(ctx, old.clone(), new.clone());
                            self.pending_move = None;
                        }
                        if ui.button("Don't move").clicked() {
                            self.apply_install_dir(&new);
                            self.pending_move = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_move = None;
                        }
                    });
                });
            if !open {
                self.pending_move = None;
            }
        }

        if let Some(repo) = self.confirm_uninstall.clone() {
            let name = self.all_projects().into_iter().find(|p| p.repo == repo);
            let mut open = true;
            egui::Window::new(RichText::new("Uninstall?").strong().size(18.0))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .open(&mut open)
                .show(ctx, |ui| {
                    let label = name.as_ref().map_or(repo.as_str(), |p| p.name.as_str());
                    ui.label(RichText::new(format!("Remove {label} from this PC?")).color(Color32::WHITE).size(16.0));
                    ui.label(RichText::new("Its download, its build and its shader cache are deleted. Your saves stay. You can install it again later.").color(DIM));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(RichText::new("Uninstall").color(Color32::WHITE)).fill(look::RED);
                        if ui.add(b).clicked() {
                            if let Some(p) = name.clone() {
                                self.start(ctx, p, Kind::Uninstall);
                            }
                            self.confirm_uninstall = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_uninstall = None;
                        }
                    });
                });
            if !open {
                self.confirm_uninstall = None;
            }
        }

        if let Some((host, repo)) = self.confirm_custom.clone() {
            let mut open = true;
            egui::Window::new("Add a custom repo?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(match &host {
                        None => format!("github.com/{repo}"),
                        Some(h) => format!("{}/{repo}", h.trim_start_matches("https://").trim_start_matches("http://")),
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Play downloads this repo, builds it and runs it on your PC.\nOnly add repos from people you trust: fake repos with malware exist.").color(ORANGE));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Add it").color(TEXT)).clicked() {
                            let name = repo.split('/').next_back().unwrap_or(&repo).to_string();
                            self.saved.custom.retain(|p| p.repo != repo);
                            self.saved.custom.insert(0, Project {
                                name,
                                repo: repo.clone(),
                                branch: None,
                                gpu: Vec::new(),
                                fps: None,
                                render: None,
                                verified: false,
                                summary: "Added by link.".into(),
                                custom: true,
                                local_exe: None,
                                host: host.clone(),
                            });
                            self.saved.selected = Some(repo.clone());
                            self.custom_link.clear();
                            self.confirm_custom = None;
                            self.fetch_about(ctx);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_custom = None;
                        }
                    });
                });
            if !open {
                self.confirm_custom = None;
            }
        }
    }
}

impl App {
    fn list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Projects")
                    .size(26.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⟳ Refresh").clicked() {
                    self.refresh_projects(ctx);
                }
            });
        });
        ui.horizontal(|ui| {
            for (i, name) in ["All", "NVIDIA", "AMD"].iter().enumerate() {
                if look::filter_pill(ui, name, self.saved.gpu_filter == i).clicked() {
                    self.saved.gpu_filter = i;
                }
            }
            // Where the list comes from: PortWatch or the repo hub.
            // A pill like the filters beside it; clicking it opens the two choices, as pills too.
            let names = ["From portwatch-vi.xyz", "From the repo hub"];
            let before = self.saved.source.min(1);
            let mut source = before;
            let popup = ui.make_persistent_id("source_popup");
            let open = ui.memory(|m| m.is_popup_open(popup));
            let pill = look::dropdown_pill(ui, names[before], open);
            if pill.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
            egui::popup::popup_below_widget(
                ui,
                popup,
                &pill,
                egui::PopupCloseBehavior::CloseOnClick,
                |ui| {
                    ui.set_min_width(pill.rect.width());
                    for (i, n) in names.iter().enumerate() {
                        if look::filter_pill(ui, n, i == before).clicked() {
                            source = i;
                        }
                        ui.add_space(4.0);
                    }
                },
            );
            if source != before {
                self.saved.source = source;
                self.refresh_projects(ctx);
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.custom_link)
                .hint_text("Custom repo link (any git site)")
                .desired_width(290.0);
            ui.add(edit);
            if ui.button(RichText::new("+ Add").color(TEAL)).clicked() {
                match parse_link(&self.custom_link) {
                    Some(r) => {
                        self.custom_error = None;
                        self.confirm_custom = Some(r);
                    }
                    None => self.custom_error = Some("That is not a git repo link (like https://site/owner/name).".into()),
                }
            }
        });
        if let Some(e) = &self.custom_error {
            ui.label(RichText::new(e).color(BAD));
        }
        ui.add_space(6.0);

        let state = match &*self.projects.lock().unwrap() {
            None => Some(
                if self.saved.source == 1 {
                    "Loading the list from the repo hub..."
                } else {
                    "Loading the list from PortWatch..."
                }
                .to_string(),
            ),
            Some(Err(e)) => Some(e.clone()),
            Some(Ok(_)) => None,
        };
        if let Some(s) = state {
            ui.label(RichText::new(s).color(DIM));
        }
        let filter = self.saved.gpu_filter;
        let projects: Vec<Project> = self
            .all_projects()
            .into_iter()
            .filter(|p| match filter {
                1 => p.custom || p.gpu.iter().any(|g| g.eq_ignore_ascii_case("nvidia")),
                2 => p.custom || p.gpu.iter().any(|g| g.eq_ignore_ascii_case("amd")),
                _ => true,
            })
            .collect();
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let mut remove = None;
                for pair in projects.chunks(2) {
                    ui.columns(2, |cols| {
                        for (col, p) in cols.iter_mut().zip(pair) {
                            let selected = self.saved.selected.as_deref() == Some(p.repo.as_str());
                            let mut remove_rect: Option<egui::Rect> = None;
                            let resp =
                                look::card(col, egui::Id::new(("card", &p.repo)), selected, |ui| {
                                    // Long names (the repo hub's) are cut with "…" so a card never grows past its column.
                                    ui.horizontal(|ui| {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if let Some(f) = p.fps {
                                                    ui.label(
                                                        RichText::new(format!("{f:.0} fps"))
                                                            .color(ORANGE)
                                                            .strong()
                                                            .size(16.0),
                                                    );
                                                }
                                                ui.with_layout(
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(&p.name)
                                                                    .size(17.0)
                                                                    .strong(),
                                                            )
                                                            .truncate(),
                                                        );
                                                    },
                                                );
                                            },
                                        );
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&p.repo).color(DIM).size(12.0),
                                        )
                                        .truncate(),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        for g in &p.gpu {
                                            chip(
                                                ui,
                                                g,
                                                if g.eq_ignore_ascii_case("nvidia") {
                                                    look::LIME
                                                } else {
                                                    look::RED
                                                },
                                            );
                                        }
                                        if p.verified {
                                            chip(ui, "verified", look::TEAL);
                                        }
                                        if p.custom {
                                            chip(ui, "custom", ORANGE);
                                            // The card's own click area lies over this button (look::card
                                            // registers it after the contents): its click is told apart below.
                                            remove_rect = Some(ui.small_button("remove").rect);
                                        }
                                    });
                                });
                            if resp.clicked() {
                                let on_remove = remove_rect
                                    .zip(resp.interact_pointer_pos())
                                    .is_some_and(|(r, at)| r.contains(at));
                                if on_remove {
                                    remove = Some(p.repo.clone());
                                } else {
                                    self.saved.selected = Some(p.repo.clone());
                                }
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
                if let Some(r) = remove {
                    self.saved.custom.retain(|p| p.repo != r);
                    if self.saved.selected.as_deref() == Some(r.as_str()) {
                        self.saved.selected = None;
                    }
                }
            });
    }

    /// When Git, Rust or the C++ build tools are missing: what is missing,
    /// "Install missing tools" (winget) and "Check again".
    fn tools_row(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(t) = self.tools.clone() else { return };
        if t.ready() {
            return;
        }
        let running = self.job.lock().unwrap().running;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("To install projects you need:")
                    .color(look::GOLD)
                    .strong(),
            );
            for m in t.missing() {
                chip(ui, m, BAD);
            }
            let b = egui::Button::new(
                RichText::new("Install missing tools for me").color(Color32::WHITE),
            )
            .fill(look::CORAL);
            if ui.add_enabled(!running, b).clicked() {
                self.start_tools(ctx);
            }
            if ui
                .add_enabled(!running, egui::Button::new("Check again"))
                .clicked()
            {
                self.tools = None;
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tools::detect();
                    ctx.request_repaint();
                });
            }
        });
        ui.add_space(4.0);
    }

    /// Installs what is missing with winget, then looks again.
    fn start_tools(&mut self, ctx: &egui::Context) {
        let job = self.job.clone();
        {
            let mut j = job.lock().unwrap();
            if j.running {
                return;
            }
            *j = Job {
                running: true,
                stage: "Installing tools (Windows will ask for permission)".into(),
                ..Default::default()
            };
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
                    stage(
                        &job,
                        &ctx,
                        &format!("Installing {what} (this can take a while)"),
                    );
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
                Ok(()) if now.ready() => {
                    j.stage = "Tools installed: ready to install projects".into()
                }
                Ok(()) => {
                    j.failed = true;
                    j.stage = format!(
                        "Still missing: {}. A restart of Windows can help.",
                        now.missing().join(", ")
                    );
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
        self.saved.install_dir = if new == default_workspace() {
            String::new()
        } else {
            new.display().to_string()
        };
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
            *j = Job {
                running: true,
                stage: "Moving installed projects".into(),
                ..Default::default()
            };
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
        let due = if self.last_auto_check < 0.0 {
            listed || now > 10.0
        } else {
            now - self.last_auto_check > AUTO_CHECK_SECS
        };
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

    /// A notification that slides down from the top when an installed project
    /// has an update, whichever project is selected: Update now or Later.
    fn update_notice(&mut self, ctx: &egui::Context) {
        if self.notice.as_ref().is_none_or(|r| {
            self.all_projects()
                .iter()
                .find(|p| &p.repo == r)
                .is_none_or(|p| self.update_for(p).is_none())
        }) {
            self.notice = self
                .all_projects()
                .into_iter()
                .find(|p| self.update_for(p).is_some())
                .map(|p| p.repo);
        }
        let shown = self.notice.is_some() && !self.job.lock().unwrap().running;
        let a = ctx.animate_bool_with_time(egui::Id::new("notice-anim"), shown, 0.4);
        if a < 0.001 {
            return;
        }
        let Some(p) = self
            .notice
            .as_ref()
            .and_then(|r| self.all_projects().into_iter().find(|p| &p.repo == r))
        else {
            return;
        };
        let eased = 1.0 - (1.0 - a).powi(3);
        let screen = ctx.screen_rect();
        let id = egui::Id::new("update-notice");
        let width = 560.0_f32.min(screen.width() - 40.0);
        let mut update = false;
        let mut later = false;
        egui::Area::new(id)
            .order(egui::Order::Tooltip)
            .fixed_pos(egui::pos2(
                screen.center().x - width / 2.0,
                screen.top() + 18.0,
            ))
            .show(ctx, |ui| {
                ui.set_width(width);
                let frame = egui::Frame::none()
                    .fill(Color32::from_rgba_unmultiplied(24, 12, 36, 240))
                    .stroke(Stroke::new(1.5_f32, look::ORANGE))
                    .rounding(Rounding::same(14.0))
                    .inner_margin(egui::Margin::symmetric(18.0, 14.0));
                let r = frame.show(ui, |ui| {
                    ui.set_width(width - 36.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("⬆  Update available")
                                    .strong()
                                    .size(18.0)
                                    .color(look::ORANGE),
                            );
                            ui.label(
                                RichText::new(format!("{} has a new version.", p.name))
                                    .color(Color32::WHITE),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if look::filter_pill(ui, "Later", false).clicked() {
                                later = true;
                            }
                            if look::filter_pill(ui, "Update now", true).clicked() {
                                update = true;
                            }
                        });
                    });
                });
                // The glow goes on a layer under the notification.
                let glow =
                    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, id.with("glow")));
                look::pill_glow(&glow, r.response.rect, look::ORANGE, 10.0, 0.35);
            });
        let slide =
            egui::emath::TSTransform::from_translation(Vec2::new(0.0, -(1.0 - eased) * 120.0));
        ctx.set_transform_layer(egui::LayerId::new(egui::Order::Tooltip, id), slide);
        ctx.set_transform_layer(
            egui::LayerId::new(egui::Order::Foreground, id.with("glow")),
            slide,
        );
        if a < 1.0 {
            ctx.request_repaint();
        }
        if let Some(remote) = self.update_for(&p) {
            if later {
                self.saved.later.retain(|(r, _)| r != &p.repo);
                self.saved.later.push((p.repo.clone(), remote));
                self.notice = None;
            }
            if update {
                self.saved.selected = Some(p.repo.clone());
                self.notice = None;
                self.start(ctx, p, Kind::Install);
            }
        }
    }

    /// Installed or not, and an update banner with Update now / Later.
    fn install_box(&mut self, ui: &mut egui::Ui, p: &Project) {
        let ctx = ui.ctx().clone();
        let st = match self.state_of(&p.repo) {
            Some(st) => st,
            None => {
                self.check_repo(&ctx, p);
                RepoState {
                    checking: true,
                    ..Default::default()
                }
            }
        };
        let running = self.job.lock().unwrap().running;
        ui.horizontal_wrapped(|ui| {
            if st.installed {
                let v = st
                    .built
                    .as_deref()
                    .map(|c| &c[..c.len().min(7)])
                    .unwrap_or("?");
                look::tag(ui, &format!("✔ Installed (version {v})"), look::GREEN, true);
            } else if st.checking {
                ui.label(RichText::new("checking…").color(DIM));
            } else {
                look::tag(ui, "Not installed yet", look::RED, true);
            }
            if p.local_exe.is_none()
                && !st.checking
                && look::link(ui, "Check for updates").clicked()
            {
                self.check_repo(&ctx, p);
            }
            if p.local_exe.is_none()
                && st.installed
                && ui
                    .add_enabled(!running, egui::Button::new("Reinstall"))
                    .clicked()
            {
                self.start(&ctx, p.clone(), Kind::Install);
            }
            if p.local_exe.is_none() && st.installed && !running {
                let b = egui::Button::new(RichText::new("Uninstall").color(look::RED))
                    .stroke(Stroke::new(1.0_f32, look::RED));
                if ui.add(b).clicked() {
                    self.confirm_uninstall = Some(p.repo.clone());
                }
            }
        });
        if let Some(remote) = self.update_for(p) {
            egui::Frame::none()
                .fill(ORANGE.gamma_multiply(0.15))
                .stroke(Stroke::new(1.5_f32, ORANGE))
                .rounding(Rounding::ZERO)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⬆ Update available").strong().color(ORANGE));
                        ui.label(RichText::new(format!("new version {}", &remote[..7])).color(DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Later").clicked() {
                                self.saved.later.retain(|(r, _)| r != &p.repo);
                                self.saved.later.push((p.repo.clone(), remote.clone()));
                            }
                            let b = egui::Button::new(
                                RichText::new("Update now").color(Color32::WHITE),
                            )
                            .fill(PINK);
                            if ui.add_enabled(!running, b).clicked() {
                                self.start(&ctx, p.clone(), Kind::Install);
                            }
                        });
                    });
                });
        }
    }

    /// The right side scrolls: a long description must not push the
    /// status and log out of sight.
    fn details(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("details-scroll")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .auto_shrink([false; 2])
            .show(ui, |ui| self.details_inner(ui));
    }

    fn details_inner(&mut self, ui: &mut egui::Ui) {
        match self.selected_project() {
            Some(p) => {
                look::gradient_title(ui, &p.name, 52.0, look::ORANGE, look::CORAL);
                ui.hyperlink_to(p.web_label(), p.web_url());
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if let Some(f) = p.fps {
                        look::tag(
                            ui,
                            &format!("ABOUT {f:.0} FPS (THEIR PC)"),
                            look::GOLD,
                            true,
                        );
                    }
                    if let Some(r) = p.render {
                        look::tag(ui, &format!("RENDERING {:.0}%", r * 100.0), look::SKY, true);
                    }
                });
                if p.gpu.iter().all(|g| !g.eq_ignore_ascii_case("nvidia")) && !p.gpu.is_empty() {
                    ui.label(
                        RichText::new("Made for AMD cards: it may not start on NVIDIA.").color(BAD),
                    );
                }
                ui.add_space(6.0);
                self.install_box(ui, &p);
                ui.add_space(6.0);
                ui.label(RichText::new(&p.summary).color(TEXT));
            }
            None => {
                ui.label(
                    RichText::new("Pick a project on the left")
                        .size(22.0)
                        .color(DIM),
                );
            }
        }
        ui.add_space(10.0);
        ui.separator();
        let mut show_log = self.show_log;
        let j = self.job.lock().unwrap();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Status").strong().size(18.0));
            let color = if j.failed {
                BAD
            } else if j.running {
                ORANGE
            } else {
                look::GREEN
            };
            ui.label(
                RichText::new(if j.stage.is_empty() {
                    "Ready"
                } else {
                    &j.stage
                })
                .color(color),
            );
            if j.running && !j.stage.is_empty() && j.stage != "Playing" {
                ui.add(egui::Spinner::new().color(design::PINK));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if show_log { "Hide log" } else { "Show log" };
                if ui.small_button(label).clicked() {
                    show_log = !show_log;
                }
            });
        });
        if show_log || j.failed {
            egui::ScrollArea::vertical()
                .id_salt("log-scroll")
                .max_height(260.0)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for line in j.log.iter().rev().take(400).rev() {
                        ui.label(RichText::new(line).monospace().size(11.5).color(DIM));
                    }
                });
        }
        drop(j);
        self.show_log = show_log;
    }

    fn settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // A drive search that finished.
        let done = {
            let mut f = self.finding.lock().unwrap();
            if matches!(&*f, Some(Some(_))) {
                f.take().flatten()
            } else {
                None
            }
        };
        if let Some(r) = done {
            match r {
                Ok(dir) => {
                    self.saved.game_dir = dir;
                    self.files.clear();
                }
                Err(e) => self.custom_error = Some(e),
            }
        }
        let searching = matches!(&*self.finding.lock().unwrap(), Some(None));
        ui.horizontal(|ui| {
            ui.label(RichText::new("Game folder").strong());
            let edit = egui::TextEdit::singleline(&mut self.saved.game_dir)
                .hint_text("the folder with game_scarlett_master.exe")
                .desired_width(300.0);
            if ui.add(edit).changed() {
                self.files.clear();
            }
            if ui.button("Browse…").clicked() {
                if let Some(d) = rfd::FileDialog::new()
                    .set_title("The folder of your game files")
                    .pick_folder()
                {
                    self.saved.game_dir = d.display().to_string();
                    self.files.clear();
                }
            }
            let label = if searching {
                "Searching…"
            } else {
                "🔍 Find it for me"
            };
            if ui
                .add_enabled(
                    !searching,
                    egui::Button::new(RichText::new(label).color(TEAL)),
                )
                .clicked()
            {
                *self.finding.lock().unwrap() = Some(None);
                let (slot, ctx) = (self.finding.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let r = search_drives();
                    *slot.lock().unwrap() = Some(Some(r));
                    ctx.request_repaint();
                });
            }
        });
        ui.horizontal_wrapped(|ui| {
            if self.saved.game_dir.is_empty() {
                ui.label(RichText::new("Choose the folder of your game files, or press Find it for me.").color(ORANGE));
            } else if self.files_ok() {
                ui.label(RichText::new("✔ Game files found").color(GOOD).strong());
            } else if self.empty {
                ui.label(RichText::new("✘ This folder is empty: the game files are not here, so the game can't start. Press Find it for me.").color(BAD).strong());
            } else {
                ui.label(RichText::new("✘ You don't have the game files here, so the game can't start. Missing:").color(BAD).strong());
                for (f, ok) in &self.files {
                    if !ok {
                        chip(ui, f, BAD);
                    }
                }
            }
            if self.tools.is_none() {
                ui.label(RichText::new("checking tools…").color(DIM));
            }
        });
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Graphics").strong().size(20.0));
            look::tag(ui, "BETA", look::GOLD, false);
        });
        ui.label(
            RichText::new("These graphics options are in beta: they may not work with every project or on older PCs. If a game will not start or looks wrong, switch back to Best quality and turn the fixes off.")
                .color(DIM)
                .size(13.0),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mode").strong());
            let modes = [
                ("Fast", true, 0.5_f32),
                ("Balanced", true, 0.75),
                ("Best quality", false, 1.0),
            ];
            for (name, s, scale) in modes {
                let on = self.saved.series_s == s
                    && (!s || (self.saved.render_scale - scale).abs() < 0.01);
                let b = egui::Button::new(RichText::new(name).color(if on {
                    Color32::WHITE
                } else {
                    TEXT
                }))
                .fill(if on { PINK } else { CARD })
                .min_size(Vec2::new(110.0, 30.0));
                if ui.add(b).clicked() {
                    self.saved.series_s = s;
                    self.saved.render_scale = scale;
                }
            }
            ui.add_space(14.0);
            ui.checkbox(&mut self.saved.fix_8gb, "8 GB graphics card fix (beta)");
            ui.checkbox(&mut self.saved.bright_shadows, "Brighter shadows (beta)");
        });
        ui.add_space(10.0);
        ui.checkbox(
            &mut self.saved.fullscreen,
            RichText::new("Fullscreen").strong(),
        );
        ui.add_space(10.0);
        let running = self.job.lock().unwrap().running;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Install folder").strong());
            ui.label(RichText::new(workspace().display().to_string()).color(DIM));
        });
        ui.horizontal(|ui| {
            let browse = ui.add_enabled(!running, egui::Button::new("Browse…"));
            if browse.clicked() {
                if let Some(d) = rfd::FileDialog::new()
                    .set_title("Where projects are downloaded and built")
                    .pick_folder()
                {
                    self.choose_install_dir(d.join("Mandyhub"));
                }
            }
            let default = ui.add_enabled(
                !running && !self.saved.install_dir.is_empty(),
                egui::Button::new("Default"),
            );
            if default.clicked() {
                self.choose_install_dir(default_workspace());
            }
            ui.label(
                RichText::new("Projects take several GB each: a big drive is best.")
                    .color(DIM)
                    .size(13.0),
            );
        });
        // Backgrounds: the player's own pictures (none ship with the launcher).
        ui.add_space(10.0);
        let mut changed = false;
        egui::CollapsingHeader::new(RichText::new("Backgrounds").strong()).show(ui, |ui| {
            let pics = slides::list();
            if pics.is_empty() {
                ui.label(
                    RichText::new("No pictures yet: add your own screenshots (jpg or png).")
                        .color(DIM)
                        .size(13.0),
                );
            }
            for f in &pics {
                ui.horizontal(|ui| {
                    let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                    ui.add(egui::Label::new(RichText::new(name).color(TEXT)).truncate());
                    if ui.small_button("remove").clicked() {
                        // A copy in Mandyhub's own folder (Add copies pictures there): the player's original stays.
                        if let Err(e) = std::fs::remove_file(f) {
                            self.notice = Some(format!("Could not remove {name}: {e}"));
                        }
                        changed = true;
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui.button("+ Add pictures…").clicked() {
                    if let Some(files) = rfd::FileDialog::new()
                        .set_title("Pictures for the background")
                        .add_filter("Pictures", &["jpg", "jpeg", "png"])
                        .pick_files()
                    {
                        for f in files {
                            if let Err(e) = slides::add(&f) {
                                self.notice = Some(format!("Could not add {}: {e}", f.display()));
                            }
                        }
                        changed = true;
                    }
                }
                if ui.button("Open folder").clicked() {
                    let _ = std::fs::create_dir_all(slides::folder());
                    let _ = Command::new("explorer").arg(slides::folder()).spawn();
                }
                ui.label(
                    RichText::new("They fade into each other behind the launcher.")
                        .color(DIM)
                        .size(13.0),
                );
            });
        });
        if changed {
            self.slides = slides::Slides::new();
        }
        egui::CollapsingHeader::new(RichText::new("Advanced").color(DIM)).show(ui, |ui| {
            ui.label(
                RichText::new("Extra switches for experts, one per line (VICTORY_NAME=value)")
                    .color(DIM),
            );
            ui.add(
                egui::TextEdit::multiline(&mut self.saved.extra_env)
                    .desired_rows(2)
                    .desired_width(500.0),
            );
        });
        ui.add_space(26.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(RichText::new("Credits").strong().size(20.0));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mandyhub by").color(Color32::WHITE));
            look::gradient_title(ui, "Zettrox", 22.0, look::ORANGE, look::PINK);
        });
        ui.label(RichText::new("Project list from PortWatch (portwatch-vi.xyz). The projects are their authors' work, built on VIctory by Marvisak and its contributors.").color(DIM).size(13.0));
        ui.label(
            RichText::new(
                "No game files are included or downloaded: you need your own copy of the game.",
            )
            .color(DIM)
            .size(13.0),
        );
    }

    fn bottom(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // The game folder in one line; changing it is in Settings.
        ui.horizontal(|ui| {
            if self.files_ok() {
                ui.label(RichText::new("✔ Game files found").color(GOOD).strong());
                ui.label(RichText::new(&self.saved.game_dir).color(DIM).size(12.0));
            } else {
                ui.label(
                    RichText::new("✘ Game files not found: the game can't start.")
                        .color(BAD)
                        .strong(),
                );
                if ui
                    .button(RichText::new("Choose the game folder").color(design::ORANGE))
                    .clicked()
                {
                    self.show_settings = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mode = if !self.saved.series_s {
                    "Best quality"
                } else if self.saved.render_scale < 0.6 {
                    "Fast"
                } else {
                    "Balanced"
                };
                ui.label(RichText::new(format!("Graphics: {mode}")).color(DIM));
            });
        });
        ui.add_space(6.0);
        self.tools_row(ui, ctx);
        let tools_ok = self.tools.as_ref().is_some_and(|t| t.ready());
        let running = self.job.lock().unwrap().running;
        let selected = self.selected_project();
        let installed = selected
            .as_ref()
            .and_then(|p| self.state_of(&p.repo))
            .is_some_and(|st| st.installed);
        // Playing needs the game files; installing needs Git and Rust.
        let can =
            selected.is_some() && !running && if installed { self.files_ok() } else { tools_ok };
        ui.horizontal(|ui| {
            let (icon, label) = if running {
                ("⏳", "RUNNING")
            } else if installed {
                ("▶", "PLAY")
            } else {
                ("⬇", "INSTALL")
            };
            if look::big_pill(ui, icon, label, can, running, design::time(ctx)).clicked() {
                if let Some(p) = selected.clone() {
                    self.start(ctx, p, if installed { Kind::Play } else { Kind::Install });
                }
            }
            ui.add_space(10.0);
            let why = if selected.is_none() {
                "Pick a project first."
            } else if installed && !self.files_ok() {
                "The game files are missing."
            } else if !installed && !tools_ok {
                "Installing a project needs Git, Rust and the C++ build tools: press Install missing tools."
            } else if installed {
                "Starts the installed build. No game files are downloaded."
            } else {
                "Downloads and builds the project once, on this PC (several minutes). It stays installed."
            };
            ui.label(RichText::new(why).color(DIM));
        });
    }
}

#[cfg(test)]
mod link_tests {
    use super::parse_link;

    #[test]
    fn links_from_any_git_site() {
        let gh = |r: &str| Some((None, r.to_string()));
        let at = |h: &str, r: &str| Some((Some(h.to_string()), r.to_string()));
        assert_eq!(parse_link("https://github.com/fxceless/victory-fork"), gh("fxceless/victory-fork"));
        assert_eq!(parse_link("Marvisak/VIctory"), gh("Marvisak/VIctory"));
        assert_eq!(parse_link("https://gitlab.com/group/sub/proj.git"), at("https://gitlab.com", "group/sub/proj"));
        assert_eq!(parse_link("gitlab.com/group/proj/-/tree/main"), at("https://gitlab.com", "group/proj"));
        assert_eq!(parse_link("https://codeberg.org/someone/thing/"), at("https://codeberg.org", "someone/thing"));
        assert_eq!(parse_link("http://192.168.1.5/me/repo"), at("http://192.168.1.5", "me/repo"));
        assert_eq!(parse_link("ftp://site.org/a/b"), None);
        assert_eq!(parse_link("not a link"), None);
        assert_eq!(parse_link("https://site.org/onlyone"), None);
    }
}

#[cfg(test)]
mod cmd_env_tests {
    use super::parse_cmd_env;

    #[test]
    fn reads_set_lines_and_skips_the_launcher_own_ones() {
        let lines = [
            "@echo off",
            r#"rem set "VICTORY_SKIPPED=1""#,
            r#"if not defined VICTORY_RELAY set "VICTORY_RELAY=2""#,
            r#"if not defined VICTORY_CACHE_DIR set "VICTORY_CACHE_DIR=%LOCALAPPDATA%\x""#,
            r#"if not defined VICTORY_GAME_DIR set "VICTORY_GAME_DIR=C:\game""#,
            r#"set "VICTORY_XMEM_SECTION_KB=20000-21000""#,
            r#"set "VICTORY_RELAY=3""#,
            r#"set "OTHER=1""#,
        ];
        let vars = parse_cmd_env(&lines.join("\r\n"));
        assert_eq!(
            vars,
            vec![
                ("VICTORY_XMEM_SECTION_KB".to_string(), "20000-21000".to_string()),
                ("VICTORY_RELAY".to_string(), "3".to_string()),
            ]
        );
    }
}
