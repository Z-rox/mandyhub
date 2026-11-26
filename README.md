# Mandyhub

A community launcher for VIctory-based projects.

- **Projects from PortWatch:** the list of public projects, live, with their reported frame rate, the graphics cards they were made for, and whether PortWatch has verified them.
- **Custom repo links:** add any GitHub repo by its link.
- **Install once, play many times:** a project is downloaded and built on your PC once and stays installed.
- **Updates:** installed projects are checked for new versions at start and every 15 minutes; an update can be installed now or later.
- **Install folder:** choose where projects are downloaded and built (a big drive is best); installed projects can be moved there.
- **Uninstall:** removes a project's download, build and shader cache (saves stay).
- **Game files check:** Play stays off until the folder with your own game files is chosen.
- **Fullscreen:** optional borderless fullscreen for the game window (Windows).

No game files are included or downloaded. You need your own copy of the game.

## Requirements

**Windows 10 or 11, 64-bit** — where the projects are built and played:

- Git, Rust and the C++ build tools. Mandyhub finds them where their installers put them, and **Install missing tools for me** installs what is missing with winget (Windows asks for permission).

**Linux** — the launcher runs and works the same way:

- Git, Rust (`cargo`) and a C compiler (`cc`, `gcc` or `clang`); the usual packages, e.g. `git rust base-devel` on Arch, `git rustup build-essential` on Debian/Ubuntu.
- When one is missing, the launcher shows the install command of your distribution's package manager, ready to copy (installing needs your password, so it is not run for you).
- Borderless fullscreen is a Windows feature: on Linux the game window is left to the window manager.

## Build

```powershell
cargo build --release
```

The launcher is `target\release\mandyhub.exe` on Windows and `target/release/mandyhub` on Linux.

## Where files go

| | Windows | Linux |
|---|---|---|
| Launcher folder | `%LOCALAPPDATA%\Mandyhub` | `$XDG_DATA_HOME/mandyhub` (`~/.local/share/mandyhub`) |
| Backgrounds | `…\Mandyhub\backgrounds` | `…/mandyhub/backgrounds` |
| Projects | `…\Mandyhub\repos`, `target`, `cache`, `data` | same, under the folder above |

## Backgrounds

Put your own `.jpg` or `.png` pictures in the `backgrounds` folder above. They slowly move and fade into each other behind the launcher. Without pictures, an animated scene is shown.

## Credits

Mandyhub by **Zettrox**. Project list from [PortWatch](https://portwatch-vi.xyz). The projects are their authors' work, built on VIctory by Marvisak and its contributors.
