//! What differs between Windows and Linux: the launcher's own folder, the
//! usual places of the tools a build needs, where to look for the game, and
//! how the tools that are missing get installed.
//!
//! The projects themselves are made for Windows. On Linux the launcher lists
//! them, downloads and builds them the same way — a native build only needs
//! a C linker — and starts the built binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Whether `program` runs and answers `--version`.
pub fn answers(program: &Path) -> bool {
    let mut c = Command::new(program);
    c.arg("--version").stdout(Stdio::null()).stderr(Stdio::null());
    crate::hide_window(&mut c);
    c.status().map(|s| s.success()).unwrap_or(false)
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

// ------------------------------------------------------------- own folder

/// The launcher's own folder: settings, downloads, builds, pictures.
#[cfg(windows)]
pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Mandyhub")
}

/// `$XDG_DATA_HOME/mandyhub`, else `~/.local/share/mandyhub`.
#[cfg(not(windows))]
pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("mandyhub")
}

/// The user's home folder.
#[cfg(windows)]
pub fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

/// The user's home folder.
#[cfg(not(windows))]
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// ------------------------------------------------------------------ tools

/// Where Git is when it is not on the PATH.
#[cfg(windows)]
pub fn git_places() -> Vec<Option<PathBuf>> {
    let pf = env_path("ProgramFiles");
    let pf86 = env_path("ProgramFiles(x86)");
    let local = env_path("LOCALAPPDATA");
    vec![
        pf.map(|p| p.join("Git").join("cmd").join("git.exe")),
        pf86.map(|p| p.join("Git").join("cmd").join("git.exe")),
        local.map(|p| p.join("Programs").join("Git").join("cmd").join("git.exe")),
    ]
}

/// Where Git is when it is not on the PATH.
#[cfg(not(windows))]
pub fn git_places() -> Vec<Option<PathBuf>> {
    vec![
        Some(PathBuf::from("/usr/bin/git")),
        Some(PathBuf::from("/usr/local/bin/git")),
        home().map(|h| h.join(".local").join("bin").join("git")),
    ]
}

/// Where cargo (Rust) is when it is not on the PATH.
#[cfg(windows)]
pub fn cargo_places() -> Vec<Option<PathBuf>> {
    vec![
        env_path("CARGO_HOME").map(|p| p.join("bin").join("cargo.exe")),
        home().map(|p| p.join(".cargo").join("bin").join("cargo.exe")),
    ]
}

/// Where cargo (Rust) is when it is not on the PATH.
#[cfg(not(windows))]
pub fn cargo_places() -> Vec<Option<PathBuf>> {
    vec![
        env_path("CARGO_HOME").map(|p| p.join("bin").join("cargo")),
        home().map(|p| p.join(".cargo").join("bin").join("cargo")),
    ]
}

/// Whether a C linker the Rust compiler can use is on the PATH.
#[cfg(not(windows))]
pub fn has_linker() -> bool {
    ["cc", "gcc", "clang"].iter().any(|p| answers(Path::new(p)))
}

// ------------------------------------------------------------ game search

/// The folders the game is looked for: every existing drive.
#[cfg(windows)]
pub fn search_roots() -> Vec<PathBuf> {
    (b'C'..=b'Z')
        .map(|l| PathBuf::from(format!("{}:\\", l as char)))
        .filter(|p| p.is_dir())
        .collect()
}

/// The folders the game is looked for: the home folder, and the drives and
/// shares mounted under /media and /mnt.
#[cfg(not(windows))]
pub fn search_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = home().into_iter().collect();
    let user = std::env::var_os("USER").map(|u| u.to_string_lossy().to_string()).unwrap_or_default();
    for base in ["/media", "/mnt", "/run/media"] {
        let base = PathBuf::from(base);
        if !base.is_dir() {
            continue;
        }
        // /run/media/<user>/<drive>: the user's own folder is searched.
        let own = base.join(&user);
        if !user.is_empty() && own.is_dir() {
            roots.push(own);
        } else {
            roots.push(base);
        }
    }
    roots
}

/// How many levels down the game's exe is looked for.
pub fn search_depth() -> u32 {
    if cfg!(windows) {
        3
    } else {
        4
    }
}

/// Folders never worth walking into while looking for the game.
#[cfg(windows)]
pub fn skip_dirs() -> &'static [&'static str] {
    &["windows", "program files", "program files (x86)", "programdata", "appdata"]
}

/// Folders never worth walking into while looking for the game.
#[cfg(not(windows))]
pub fn skip_dirs() -> &'static [&'static str] {
    &[
        "proc", "sys", "dev", "boot", "usr", "var", "lib", "lib64", "bin", "sbin", "etc", "opt", "srv",
        "run", "tmp", "lost+found", "snap", "node_modules", "target",
    ]
}

/// What to tell when no game folder was found.
#[cfg(windows)]
pub fn not_found() -> &'static str {
    "The game was not found on any drive. Choose its folder with Browse."
}

/// What to tell when no game folder was found.
#[cfg(not(windows))]
pub fn not_found() -> &'static str {
    "The game was not found in your home folder, /media or /mnt. Choose its folder with Browse."
}

// -------------------------------------------------------- tool installation

/// Whether the launcher installs the missing tools itself (winget, Windows
/// only): on Linux the system's package manager needs a password, so the
/// command is shown instead.
pub fn can_auto_install() -> bool {
    cfg!(windows)
}

/// Whether `program` is in one of the usual binary folders.
#[cfg(not(windows))]
fn on_path(program: &str) -> bool {
    ["/usr/bin", "/usr/local/bin", "/bin", "/usr/sbin", "/sbin"]
        .iter()
        .any(|d| Path::new(d).join(program).is_file())
}

/// The package manager's command that installs what a build lacks.
#[cfg(windows)]
pub fn install_command(_git: bool, _rust: bool, _linker: bool) -> Option<String> {
    None
}

/// The package manager's command that installs what a build lacks, by the
/// distribution the launcher is on (`None` when no known manager is there).
#[cfg(not(windows))]
pub fn install_command(git: bool, rust: bool, linker: bool) -> Option<String> {
    // program, its install command, Git, Rust, C/C++ tools
    const MANAGERS: &[(&str, &str, &str, &str, &str)] = &[
        ("pacman", "sudo pacman -S --needed", "git", "rust", "base-devel"),
        ("apt-get", "sudo apt install", "git", "rustup", "build-essential"),
        ("dnf", "sudo dnf install", "git", "rust", "gcc gcc-c++"),
        ("zypper", "sudo zypper install", "git", "rust", "gcc gcc-c++"),
        ("apk", "sudo apk add", "git", "rust", "gcc musl-dev"),
    ];
    let (_, run, git_pkg, rust_pkg, cc_pkgs) = *MANAGERS.iter().find(|(program, ..)| on_path(program))?;
    let mut packages: Vec<&str> = Vec::new();
    if git {
        packages.push(git_pkg);
    }
    if rust {
        packages.push(rust_pkg);
    }
    if linker {
        packages.extend(cc_pkgs.split(' '));
    }
    if packages.is_empty() {
        return None;
    }
    let mut command = format!("{run} {}", packages.join(" "));
    // rustup has to pick a toolchain once before cargo works.
    if rust && rust_pkg == "rustup" {
        command.push_str("\nrustup default stable");
    }
    Some(command)
}
