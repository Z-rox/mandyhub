//! The tools a build needs: Git, Rust (cargo) and a C linker — found on the
//! PATH or in the folders their installers use (a PATH changed after the
//! session started does not reach programs started before). On Windows the
//! missing ones are installed with winget; on Linux the command of the
//! system's package manager is shown instead.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::platform;

/// What a missing linker is called in the interface.
#[cfg(windows)]
const LINKER: &str = "C++ build tools";
/// What a missing linker is called in the interface.
#[cfg(not(windows))]
const LINKER: &str = "C linker (cc)";

#[derive(Clone, Default, Debug)]
pub struct Tools {
    pub git: Option<PathBuf>,
    pub cargo: Option<PathBuf>,
    /// The C linker Rust links with: MSVC's on Windows, cc/gcc/clang on Linux.
    pub linker: bool,
}

impl Tools {
    pub fn ready(&self) -> bool {
        self.git.is_some() && self.cargo.is_some() && self.linker
    }

    /// What is missing, for people.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut m = Vec::new();
        if self.git.is_none() {
            m.push("Git");
        }
        if self.cargo.is_none() {
            m.push("Rust");
        }
        if !self.linker {
            m.push(LINKER);
        }
        m
    }
}

/// The last look, shared with the build commands.
static FOUND: Mutex<Option<Tools>> = Mutex::new(None);

pub fn current() -> Option<Tools> {
    FOUND.lock().unwrap().clone()
}

/// `name` on the PATH, else the first of `places` that exists and answers.
fn find(name: &str, places: &[Option<PathBuf>]) -> Option<PathBuf> {
    if platform::answers(Path::new(name)) {
        return Some(PathBuf::from(name));
    }
    places.iter().flatten().find(|p| p.is_file() && platform::answers(p)).cloned()
}

/// Whether the C++ tools of Visual Studio (MSVC) are installed: every Visual
/// Studio install (any edition, Build Tools too), from the installer's own
/// list, else the usual folders.
#[cfg(windows)]
fn msvc_installed() -> bool {
    use std::process::Command;

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from) {
        let vswhere = pf86.join("Microsoft Visual Studio").join("Installer").join("vswhere.exe");
        if vswhere.is_file() {
            let mut c = Command::new(vswhere);
            c.args(["-all", "-products", "*", "-property", "installationPath"]);
            crate::hide_window(&mut c);
            if let Ok(o) = c.output() {
                roots.extend(String::from_utf8_lossy(&o.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from));
            }
        }
    }
    for base in ["ProgramFiles", "ProgramFiles(x86)"].into_iter().filter_map(|v| std::env::var_os(v).map(PathBuf::from)) {
        let vs = base.join("Microsoft Visual Studio");
        for year in std::fs::read_dir(&vs).into_iter().flatten().flatten() {
            for edition in std::fs::read_dir(year.path()).into_iter().flatten().flatten() {
                roots.push(edition.path());
            }
        }
    }
    // The C++ tools are there if a version of them has its 64-bit linker.
    roots.iter().any(|root| {
        std::fs::read_dir(root.join("VC").join("Tools").join("MSVC"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|v| v.path().join("bin").join("Hostx64").join("x64").join("link.exe").is_file())
    })
}

/// Whether the Rust compiler has a linker to link with.
#[cfg(windows)]
fn linker_installed() -> bool {
    msvc_installed()
}

/// Whether the Rust compiler has a linker to link with.
#[cfg(not(windows))]
fn linker_installed() -> bool {
    platform::has_linker()
}

/// Looks for the tools now.
pub fn detect() -> Tools {
    let t = Tools {
        git: find("git", &platform::git_places()),
        cargo: find("cargo", &platform::cargo_places()),
        linker: linker_installed(),
    };
    *FOUND.lock().unwrap() = Some(t.clone());
    t
}

/// A command for a tool, by the path it was found at, with the tools'
/// folders first on its PATH (cargo runs rustc and git from there).
fn command(path: Option<PathBuf>, name: &str) -> std::process::Command {
    let program = path.unwrap_or_else(|| PathBuf::from(name));
    let mut c = std::process::Command::new(&program);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(t) = current() {
        for p in [t.git, t.cargo].into_iter().flatten() {
            if let Some(d) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
                dirs.push(d.to_path_buf());
            }
        }
    }
    if !dirs.is_empty() {
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            c.env("PATH", joined);
        }
    }
    c
}

pub fn git() -> std::process::Command {
    command(current().and_then(|t| t.git), "git")
}

pub fn cargo() -> std::process::Command {
    command(current().and_then(|t| t.cargo), "cargo")
}

/// Whether winget (the Windows package manager) is there.
#[cfg(windows)]
pub fn has_winget() -> bool {
    platform::answers(Path::new("winget"))
}

/// Whether winget (the Windows package manager) is there.
#[cfg(not(windows))]
pub fn has_winget() -> bool {
    false
}

/// The winget installs for what `t` lacks: (what, arguments).
#[cfg(windows)]
pub fn install_steps(t: &Tools) -> Vec<(&'static str, Vec<String>)> {
    let common = ["--exact", "--source", "winget", "--accept-package-agreements", "--accept-source-agreements"];
    let mut steps = Vec::new();
    let mut step = |what: &'static str, id: &str, extra: &[&str]| {
        let mut a: Vec<String> = vec!["install".into(), "--id".into(), id.into()];
        a.extend(common.iter().map(|s| s.to_string()));
        a.extend(extra.iter().map(|s| s.to_string()));
        steps.push((what, a));
    };
    if t.git.is_none() {
        step("Git", "Git.Git", &["--silent"]);
    }
    if !t.linker {
        step(
            "C++ build tools",
            "Microsoft.VisualStudio.2022.BuildTools",
            &["--override", "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"],
        );
    }
    if t.cargo.is_none() {
        step("Rust", "Rustlang.Rustup", &["--silent"]);
    }
    steps
}

/// The winget installs for what `t` lacks: (what, arguments).
#[cfg(not(windows))]
pub fn install_steps(_: &Tools) -> Vec<(&'static str, Vec<String>)> {
    Vec::new()
}

/// The command that installs what `t` lacks with the system's package
/// manager (`None` on Windows, where winget does it, or when no known
/// manager is there).
pub fn install_command(t: &Tools) -> Option<String> {
    platform::install_command(t.git.is_none(), t.cargo.is_none(), !t.linker)
}
