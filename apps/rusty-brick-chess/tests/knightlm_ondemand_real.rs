//! The real on-demand install, end to end, against the real network. Not run by CI.
//!
//! ```sh
//! mkdir -p /mnt/4tb/tmp/knightlm-ondemand-test/empty-path
//! RBC_REAL_DATA=/mnt/4tb/tmp/knightlm-ondemand-test/data \
//!   PATH=/mnt/4tb/tmp/knightlm-ondemand-test/empty-path \
//!   cargo test --test knightlm_ondemand_real -- --ignored --nocapture
//! ```
//!
//! With an empty `PATH` no system Python is visible, so this downloads the pinned standalone
//! Python, builds the venv from it, pip-installs `model/serve/requirements.txt` (CPU torch) and
//! downloads the pinned weights. `RBC_REAL_SKIP_WEIGHTS=1` stops after the Python step (the
//! weights are about 1 GB). `RBC_REAL_START=1` also starts the server and asks `/v1/models`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rusty_brick_chess::model_runtime::{Config, Manifest, Phase, install_runtime};

#[test]
#[ignore = "downloads Python, about 1 GB of packages and about 1 GB of weights"]
fn the_real_install_works_with_no_system_python() {
    let Some(data) = std::env::var_os("RBC_REAL_DATA").map(PathBuf::from) else {
        eprintln!("RBC_REAL_DATA not set; nothing to do");
        return;
    };
    std::fs::create_dir_all(&data).unwrap();
    let mut manifest = Manifest::pinned();
    let skip_weights = std::env::var("RBC_REAL_SKIP_WEIGHTS").is_ok_and(|v| v == "1");
    if skip_weights {
        // Keep only the small files so the Python path is what is being exercised.
        manifest.base.files.retain(|f| f.size < 1_000_000);
        manifest.base.files.retain(|f| f.name != "tokenizer.json");
    }
    manifest.check_shape().unwrap();
    let config = Config {
        manifest,
        unpinned: skip_weights,
        fake: false,
        dev_base_dir: None,
        python: None,
        code_dir: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")),
    };
    let runtime = install_runtime(&data, config);
    let started = Instant::now();
    if !runtime.files_installed() {
        runtime.install().unwrap();
    }
    let mut last = String::new();
    loop {
        let phase = runtime.phase();
        let line = match &phase {
            Phase::Downloading { bytes, total, file } => {
                format!(
                    "downloading {file} {} / {} MB",
                    bytes / 1_000_000,
                    total / 1_000_000
                )
            }
            other => format!("{other:?}"),
        };
        // Print state changes, and download progress every ~100 MB.
        let key = match &phase {
            Phase::Downloading { bytes, file, .. } => format!("dl {file} {}", bytes / 100_000_000),
            other => format!("{other:?}"),
        };
        if key != last {
            eprintln!("[{:>5.0}s] {line}", started.elapsed().as_secs_f32());
            last = key;
        }
        match phase {
            Phase::NotInstalled | Phase::Ready => {
                if runtime.files_installed() {
                    break;
                }
            }
            Phase::Error { code, message } => panic!("{code}: {message}"),
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("installed in {:.0}s", started.elapsed().as_secs_f32());
    let python = runtime.python_info(true);
    eprintln!("python: {python:?}");
    assert!(
        python.ready,
        "the venv imports torch, transformers, peft..."
    );
    assert!(
        python.path.as_deref().is_some_and(|p| p.contains("/venv/")),
        "the venv Python is the one in use"
    );
    if std::env::var("RBC_REAL_START").is_ok_and(|v| v == "1") {
        let db = rbc_db::Db::open_memory().unwrap();
        let begun = Instant::now();
        let url = runtime.start(&db).expect("the real server starts");
        eprintln!(
            "server up at {url} after {:.0}s",
            begun.elapsed().as_secs_f32()
        );
        runtime.stop();
    }
}
