//! Which Python builds the model's environment, and the private Python the app can fetch.
//!
//! The rules (also in `docs/app/models.md`):
//!
//! 1. `RBC_KNIGHTLM_PYTHON` wins when set (any CPython 3.10 or newer; the person chose it).
//! 2. Otherwise the app's own **private Python** under `<data>/knightlm/python/`, when it is
//!    already there. It is a pinned python-build-standalone CPython, so the model runs the same
//!    way on every machine.
//! 3. Otherwise a **system** Python that is CPython 3.10 to 3.13 (torch has no wheels for 3.14
//!    yet, and 3.9 is too old). A candidate is only believed after it prints its own version:
//!    the Windows Store stub `python.exe` (which opens the Store, or exits 9009) never does.
//! 4. Otherwise "Get the model" downloads the private Python for this OS and CPU (pinned URL,
//!    pinned sha256, see `knightlm_manifest.json`). It is also the fallback when a system
//!    Python cannot create a venv (Debian without `python3-venv`).
//!
//! Everything here that decides something is a pure function, so it is unit-tested with fake
//! probes (including a Store stub) and fixture archives, without a network.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

/// `(major, minor, micro)`.
pub type Version = (u32, u32, u32);

/// Where a candidate came from; the order of this enum is the order of preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `RBC_KNIGHTLM_PYTHON`.
    Override,
    /// The app's venv (only believed when its imports work; see `python_info`).
    Venv,
    /// The python-build-standalone the app downloaded.
    Private,
    /// `python3` / `python` / `py -3.x` found on the machine.
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub kind: Kind,
}

/// Parse `3.12.4`, `Python 3.12.4`, `Python 3.13.0rc1`, with or without a newline.
pub fn parse_version(text: &str) -> Option<Version> {
    let text = text.trim();
    let text = text.strip_prefix("Python").unwrap_or(text).trim();
    let mut parts = text.split('.');
    let major = parts.next()?.trim().parse().ok()?;
    let minor = parts.next()?.trim().parse().ok()?;
    let micro: String = parts
        .next()
        .unwrap_or("0")
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    Some((major, minor, micro.parse().unwrap_or(0)))
}

/// Whether a Python of this kind and version may build or run the model environment.
pub fn suitable(kind: Kind, version: Version) -> bool {
    match kind {
        // A person's explicit choice, and our own venv, only need to be new enough.
        Kind::Override | Kind::Venv => version >= (3, 10, 0),
        Kind::Private | Kind::System => version.0 == 3 && (10..=13).contains(&version.1),
    }
}

/// The first candidate whose probed version is suitable for its kind. `probe` returns `None`
/// for anything that did not report a version (missing file, Store stub, crash, timeout).
pub fn select(
    candidates: &[Candidate],
    probe: &dyn Fn(&Path) -> Option<Version>,
) -> Option<(Candidate, Version)> {
    candidates.iter().find_map(|candidate| {
        let version = probe(&candidate.path)?;
        suitable(candidate.kind, version).then(|| (candidate.clone(), version))
    })
}

/// Ask a Python for its version. It must exit 0 and print a parseable version within 10
/// seconds. The Windows Store stub fails all three ways: it prints a hint and exits 9009 (or
/// opens the Store), so it can never be selected.
pub fn probe_version(path: &Path) -> Option<Version> {
    let mut child = Command::new(path)
        .args(["-c", "import sys; print('%d.%d.%d' % sys.version_info[:3])"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut out = String::new();
    std::io::Read::read_to_string(child.stdout.as_mut()?, &mut out).ok()?;
    parse_version(&out)
}

/// The names to try on `PATH`. On Windows `py -3.x` resolves to a real path first (see
/// [`windows_launcher_paths`]); a bare `python.exe` may be the Store stub, which [`probe_version`]
/// rejects.
pub fn system_names() -> Vec<PathBuf> {
    [
        "python3.13",
        "python3.12",
        "python3.11",
        "python3.10",
        "python3",
        "python",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

/// Ask the Windows `py` launcher where its 3.13 to 3.10 interpreters are (no-op elsewhere).
pub fn windows_launcher_paths() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    ["3.13", "3.12", "3.11", "3.10"]
        .into_iter()
        .filter_map(|minor| {
            let out = Command::new("py")
                .arg(format!("-{minor}"))
                .args(["-c", "import sys; print(sys.executable)"])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
        .collect()
}

// --- the private Python on disk --------------------------------------------------------------

/// Where the interpreter is inside an unpacked python-build-standalone `install_only` tree:
/// `python/python.exe` on Windows, `python/bin/python3` elsewhere.
pub fn python_exe_in(unpacked: &Path, windows: bool) -> PathBuf {
    if windows {
        unpacked.join("python.exe")
    } else {
        unpacked.join("bin/python3")
    }
}

/// The interpreter inside a venv: `Scripts/python.exe` on Windows, `bin/python` elsewhere.
pub fn venv_python(venv: &Path, windows: bool) -> PathBuf {
    if windows {
        venv.join("Scripts/python.exe")
    } else {
        venv.join("bin/python")
    }
}

/// The manifest key for an OS and CPU, or `None` when no standalone Python is pinned for it.
pub fn target_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("macos", "aarch64") => Some("macos-aarch64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        _ => None,
    }
}

pub fn host_target() -> Option<&'static str> {
    target_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Unpack a python-build-standalone `install_only` tarball (`python/...`) into `dest`, which
/// must not exist yet. The archive is already hash-checked; this still refuses every entry
/// that is not under `python/`, has a `..`, or is an absolute path, and never writes outside
/// `dest` (`unpack_in`). Links inside the tree (`python3 -> python3.12`) are kept.
pub fn extract_python(tgz: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(tgz).with_context(|| format!("open {}", tgz.display()))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    archive.set_preserve_permissions(true);
    std::fs::create_dir_all(dest)?;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw = entry.path()?.into_owned();
        let mut first = true;
        for component in raw.components() {
            match component {
                std::path::Component::Normal(part) => {
                    if first {
                        ensure!(
                            part == "python",
                            "the Python archive has an entry outside python/: {}",
                            raw.display()
                        );
                        first = false;
                    }
                }
                std::path::Component::CurDir => {}
                _ => bail!("the Python archive has an unsafe path: {}", raw.display()),
            }
        }
        if first {
            continue; // "." or an empty name
        }
        ensure!(
            entry.unpack_in(dest)?,
            "the Python archive has an unsafe path: {}",
            raw.display()
        );
    }
    ensure!(dest.join("python").is_dir(), "the Python archive was empty");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(path: &str, kind: Kind) -> Candidate {
        Candidate {
            path: PathBuf::from(path),
            kind,
        }
    }

    #[test]
    fn versions_parse_in_the_shapes_pythons_print() {
        assert_eq!(parse_version("3.12.4\n"), Some((3, 12, 4)));
        assert_eq!(parse_version("Python 3.10.0"), Some((3, 10, 0)));
        assert_eq!(parse_version("Python 3.13.0rc1"), Some((3, 13, 0)));
        assert_eq!(parse_version("3.9"), Some((3, 9, 0)));
        // The Store stub's hint, an empty answer and noise are not versions.
        assert_eq!(
            parse_version(
                "Python was not found; run without arguments to install from the Microsoft Store"
            ),
            None
        );
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("hello"), None);
    }

    #[test]
    fn system_and_private_pythons_must_be_3_10_to_3_13() {
        for (v, ok) in [
            ((3, 9, 18), false),
            ((3, 10, 0), true),
            ((3, 13, 9), true),
            ((3, 14, 0), false),
            ((2, 7, 18), false),
        ] {
            assert_eq!(suitable(Kind::System, v), ok, "{v:?}");
            assert_eq!(suitable(Kind::Private, v), ok, "{v:?}");
        }
        // A person's own choice may be newer; it only has to be 3.10 or later.
        assert!(suitable(Kind::Override, (3, 14, 1)));
        assert!(!suitable(Kind::Override, (3, 9, 0)));
    }

    #[test]
    fn selection_prefers_override_then_private_then_system_in_list_order() {
        let list = [
            cand("override", Kind::Override),
            cand("private", Kind::Private),
            cand("sys-a", Kind::System),
        ];
        let all = |_: &Path| Some((3, 12, 1));
        assert_eq!(select(&list, &all).unwrap().0.kind, Kind::Override);
        let no_override = |p: &Path| (p != Path::new("override")).then_some((3, 12, 1));
        assert_eq!(select(&list, &no_override).unwrap().0.kind, Kind::Private);
        let only_system = |p: &Path| (p == Path::new("sys-a")).then_some((3, 11, 0));
        assert_eq!(
            select(&list, &only_system).unwrap().0.path,
            Path::new("sys-a")
        );
        assert!(select(&list, &|_: &Path| None).is_none());
    }

    #[test]
    fn a_store_stub_a_py39_and_a_py314_are_never_selected() {
        // `stub` models the Windows Store python.exe: it answers no version at all.
        let list = [
            cand("stub", Kind::System),
            cand("py39", Kind::System),
            cand("py314", Kind::System),
            cand("py312", Kind::System),
        ];
        let probe = |p: &Path| match p.to_str().unwrap() {
            "stub" => None,
            "py39" => Some((3, 9, 13)),
            "py314" => Some((3, 14, 0)),
            "py312" => Some((3, 12, 7)),
            _ => None,
        };
        let (found, version) = select(&list, &probe).unwrap();
        assert_eq!(found.path, Path::new("py312"));
        assert_eq!(version, (3, 12, 7));
        // With only the stub, nothing is found and the private Python is downloaded.
        assert!(select(&list[..1], &probe).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn probing_a_real_store_stub_script_finds_no_version() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("rbc-pystub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, body: &str| {
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        // What the Store stub does with arguments: a hint on stdout, exit 9009 (here: 127).
        let stub = write(
            "stub",
            "#!/bin/sh\necho 'Python was not found; run without arguments to install from the Microsoft Store'\nexit 127\n",
        );
        // Exit 0 but not a version is not a Python either.
        let chatty = write("chatty", "#!/bin/sh\necho 'Opening the Store'\n");
        let good = write("good", "#!/bin/sh\necho 3.12.2\n");
        assert_eq!(probe_version(&stub), None);
        assert_eq!(probe_version(&chatty), None);
        assert_eq!(probe_version(&good), Some((3, 12, 2)));
        assert_eq!(probe_version(&dir.join("missing")), None);
        let list = [
            Candidate {
                path: stub,
                kind: Kind::System,
            },
            Candidate {
                path: good.clone(),
                kind: Kind::System,
            },
        ];
        assert_eq!(select(&list, &probe_version).unwrap().0.path, good);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn layouts_follow_the_os() {
        let root = Path::new("/d/python");
        assert_eq!(
            python_exe_in(root, false),
            Path::new("/d/python/bin/python3")
        );
        assert_eq!(python_exe_in(root, true), Path::new("/d/python/python.exe"));
        let venv = Path::new("/d/venv");
        assert_eq!(venv_python(venv, false), Path::new("/d/venv/bin/python"));
        assert_eq!(
            venv_python(venv, true),
            Path::new("/d/venv/Scripts/python.exe")
        );
    }

    #[test]
    fn targets_map_each_pinned_platform_and_nothing_else() {
        assert_eq!(target_for("linux", "x86_64"), Some("linux-x86_64"));
        assert_eq!(target_for("windows", "x86_64"), Some("windows-x86_64"));
        assert_eq!(target_for("macos", "aarch64"), Some("macos-aarch64"));
        assert_eq!(target_for("macos", "x86_64"), Some("macos-x86_64"));
        assert_eq!(target_for("linux", "aarch64"), None);
        assert_eq!(target_for("windows", "aarch64"), None);
    }

    fn tgz_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            for (name, body) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append_data(&mut header, name, *body).unwrap();
            }
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar_bytes).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn the_python_archive_unpacks_under_python_and_refuses_strays() {
        let dir = std::env::temp_dir().join(format!("rbc-pyextract-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let good = dir.join("good.tgz");
        std::fs::write(
            &good,
            tgz_of(&[
                ("python/bin/python3", b"#!/bin/sh\n"),
                ("python/python.exe", b"MZ"),
            ]),
        )
        .unwrap();
        extract_python(&good, &dir.join("out")).unwrap();
        assert!(python_exe_in(&dir.join("out/python"), false).is_file());
        assert!(python_exe_in(&dir.join("out/python"), true).is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("out/python/bin/python3"))
                .unwrap()
                .permissions()
                .mode();
            assert!(
                mode & 0o111 != 0,
                "the interpreter keeps its executable bit"
            );
        }

        let stray = dir.join("stray.tgz");
        std::fs::write(&stray, tgz_of(&[("other/evil", b"x")])).unwrap();
        let error = extract_python(&stray, &dir.join("out2")).unwrap_err();
        assert!(
            format!("{error:#}").contains("outside python/"),
            "{error:#}"
        );

        let empty = dir.join("empty.tgz");
        std::fs::write(&empty, tgz_of(&[])).unwrap();
        assert!(extract_python(&empty, &dir.join("out3")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
