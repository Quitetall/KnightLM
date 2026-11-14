//! The binary lists its recipe and refuses a bad base before doing any work.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_blut-knightlm"))
}

#[test]
fn recipe_list_shows_knightlm_lora() {
    let out = bin().args(["recipe", "list"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("knightlm_lora"), "{text}");
    assert!(text.contains("hf_trainer"), "{text}");
}

#[test]
fn unknown_base_is_refused_at_compile_time() {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("manifest.json"), "{}").unwrap();
    let args = serde_json::json!({"corpus_dir": td.path(), "run_root": td.path().join("runs"), "base": "nope"});
    let out = bin()
        .args([
            "recipe",
            "run",
            "knightlm_lora",
            "--args",
            &args.to_string(),
        ])
        .env("LAMU_TRAIN_JOBS_DIR", td.path().join("jobs"))
        .env("BLUT_REGISTRY_DB", td.path().join("r.db"))
        .env("XDG_DATA_HOME", td.path().join("xdg"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown base 'nope'"), "{err}");
}
