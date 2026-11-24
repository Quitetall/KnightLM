//! Which chess engine the app uses, in one order:
//!
//! 1. `STOCKFISH_PATH`, when set and not empty (the person's explicit choice);
//! 2. the Stockfish the installer ships (see "Layouts" below);
//! 3. nothing: the server reports "no engine" and every engine feature answers with a plain
//!    message instead of failing. A path saved in Settings or chosen from the File menu still
//!    overrides this at request time (`api::effective`).
//!
//! The desktop shell (src-tauri) may additionally fall back to a `stockfish` on `PATH`; that
//! is its own, last, step and is not part of this order.
//!
//! # Layouts
//!
//! The release workflow places the official binary at `engine/stockfish` (`stockfish.exe` on
//! Windows) inside the Tauri resources (`src-tauri/bundled-engine/`). Where the resource
//! directory is depends on the package:
//!
//! | package            | executable                      | resources                          |
//! |--------------------|---------------------------------|------------------------------------|
//! | `.deb` / `.rpm`    | `/usr/bin/<bin>`                | `/usr/lib/RustyBrickChess/`        |
//! | AppImage           | `$APPDIR/usr/bin/<bin>`         | `$APPDIR/usr/lib/RustyBrickChess/` |
//! | macOS `.app`       | `Contents/MacOS/<bin>`          | `Contents/Resources/`              |
//! | Windows `.exe/.msi`| `<dir>\<bin>.exe`               | `<dir>\`                           |
//! | `install.sh`       | `<prefix>/bin/<bin>`            | `<prefix>/share/rustybrickchess/`  |
//!
//! The desktop shell passes Tauri's own `resource_dir()`; the server also derives the same
//! places from the executable's location, so a headless server inside an AppImage finds it.

use std::path::{Path, PathBuf};

/// Directory (inside a resource directory) that holds the engine.
pub const ENGINE_DIR: &str = "engine";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `STOCKFISH_PATH`.
    Env,
    /// The binary the installer ships.
    Bundled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    pub path: PathBuf,
    pub source: Source,
}

pub fn binary_name(windows: bool) -> &'static str {
    if windows { "stockfish.exe" } else { "stockfish" }
}

/// Directories that may hold the bundled engine, best first. `resource_dir` is Tauri's
/// resource directory when the caller has it.
pub fn bundled_dirs(exe_dir: Option<&Path>, resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(resources) = resource_dir {
        dirs.push(resources.join(ENGINE_DIR));
    }
    if let Some(exe) = exe_dir {
        // Windows and a flat layout: next to the executable.
        dirs.push(exe.join(ENGINE_DIR));
        // deb, rpm, AppImage: <prefix>/bin/.. = <prefix>, resources in <prefix>/lib/<name>.
        for name in ["RustyBrickChess", "rusty-brick-chess", "rustybrickchess"] {
            dirs.push(exe.join("../lib").join(name).join(ENGINE_DIR));
        }
        // macOS bundle.
        dirs.push(exe.join("../Resources").join(ENGINE_DIR));
        // install.sh.
        dirs.push(exe.join("../share/rustybrickchess").join(ENGINE_DIR));
    }
    dirs
}

/// The resolution order, with the environment and the filesystem passed in.
pub fn resolve(env_value: Option<&str>, dirs: &[PathBuf], windows: bool) -> Option<Engine> {
    if let Some(value) = env_value.map(str::trim).filter(|v| !v.is_empty()) {
        return Some(Engine {
            path: PathBuf::from(value),
            source: Source::Env,
        });
    }
    let name = binary_name(windows);
    dirs.iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .map(|path| Engine {
            path: clean(path),
            source: Source::Bundled,
        })
}

/// Collapse `bin/../lib` so the path shown in Settings is readable.
fn clean(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// Resolve for this process: the environment, then the places around the executable.
pub fn resolve_here(resource_dir: Option<&Path>) -> Option<Engine> {
    let exe = std::env::current_exe().ok();
    let dirs = bundled_dirs(exe.as_deref().and_then(Path::parent), resource_dir);
    resolve(
        std::env::var("STOCKFISH_PATH").ok().as_deref(),
        &dirs,
        cfg!(windows),
    )
}

/// What the person sees when there is no engine.
pub const NO_ENGINE: &str = "No chess engine found. The installer ships Stockfish; if it is missing, install Stockfish and set STOCKFISH_PATH, or choose an engine in Settings.";

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rbc-engine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn the_environment_wins_even_over_a_bundled_binary() {
        let root = tree("env");
        touch(&root.join("engine/stockfish"));
        let found = resolve(Some("/opt/mine/sf"), &[root.join("engine")], false).unwrap();
        assert_eq!(found.source, Source::Env);
        assert_eq!(found.path, Path::new("/opt/mine/sf"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_variable_counts_as_unset() {
        let root = tree("empty");
        touch(&root.join("engine/stockfish"));
        for blank in [Some(""), Some("   "), None] {
            let found = resolve(blank, &[root.join("engine")], false).unwrap();
            assert_eq!(found.source, Source::Bundled, "{blank:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_variable_and_no_bundle_is_none_not_an_error() {
        let root = tree("none");
        assert_eq!(resolve(None, &bundled_dirs(Some(&root), Some(&root)), false), None);
        assert_eq!(resolve(None, &[], true), None);
        // A directory with the right name is not a binary.
        std::fs::create_dir_all(root.join("engine/stockfish")).unwrap();
        assert_eq!(resolve(None, &[root.join("engine")], false), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_looks_for_the_exe() {
        let root = tree("win");
        touch(&root.join("engine/stockfish.exe"));
        assert!(resolve(None, &[root.join("engine")], true).is_some());
        assert!(resolve(None, &[root.join("engine")], false).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_deb_and_rpm_layout_is_found_from_the_executable() {
        let root = tree("deb");
        touch(&root.join("usr/lib/RustyBrickChess/engine/stockfish"));
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        let dirs = bundled_dirs(Some(&root.join("usr/bin")), None);
        let found = resolve(None, &dirs, false).unwrap();
        assert_eq!(found.source, Source::Bundled);
        assert!(found.path.ends_with("usr/lib/RustyBrickChess/engine/stockfish"), "{:?}", found.path);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_appimage_layout_is_found_from_the_executable_inside_the_mount() {
        // $APPDIR/usr/bin/<bin>, resources in $APPDIR/usr/lib/<product>/ (the mount path is
        // arbitrary and read-only; nothing is written).
        let root = tree("appimage");
        let appdir = root.join("tmp/.mount_RustyBabc123");
        touch(&appdir.join("usr/lib/RustyBrickChess/engine/stockfish"));
        std::fs::create_dir_all(appdir.join("usr/bin")).unwrap();
        let dirs = bundled_dirs(Some(&appdir.join("usr/bin")), None);
        assert!(resolve(None, &dirs, false).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_shells_resource_dir_is_tried_first_and_the_macos_and_install_layouts_work() {
        let root = tree("order");
        touch(&root.join("res/engine/stockfish"));
        touch(&root.join("app/Contents/Resources/engine/stockfish"));
        std::fs::create_dir_all(root.join("app/Contents/MacOS")).unwrap();
        let dirs = bundled_dirs(Some(&root.join("app/Contents/MacOS")), Some(&root.join("res")));
        let found = resolve(None, &dirs, false).unwrap();
        assert!(found.path.ends_with("res/engine/stockfish"), "{:?}", found.path);
        let mac = bundled_dirs(Some(&root.join("app/Contents/MacOS")), None);
        assert!(resolve(None, &mac, false).unwrap().path.ends_with("Resources/engine/stockfish"));

        touch(&root.join("prefix/share/rustybrickchess/engine/stockfish"));
        std::fs::create_dir_all(root.join("prefix/bin")).unwrap();
        let installed = bundled_dirs(Some(&root.join("prefix/bin")), None);
        assert!(resolve(None, &installed, false).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }
}
