//! `materialize_corpus` through the engine's Stage trait, on the repository's own
//! corpus fixture (made by `model.corpus.test_pipeline.write_fixture`). Needs
//! python3; skipped, loudly, when it is missing.

use std::path::PathBuf;
use std::process::Command;

use blut::framework::stage::{Stage, StageContext};
use blut_cookbook_knightlm::runctx::{repo_root, sha256_file};
use blut_cookbook_knightlm::stages::MaterializeCorpus;
use blut_cookbook_knightlm::stages::materialize_corpus::Args;

fn fixture(dir: &std::path::Path, leak: Option<&str>) -> Option<PathBuf> {
    let corpus = dir.join("corpus");
    let code = match leak {
        Some(kind) => format!(
            "import pathlib; from model.corpus.test_pipeline import *; r=pathlib.Path({corpus:?}); write_fixture(r, 10); plant_leak(r, {kind:?})"
        ),
        None => format!(
            "import pathlib; from model.corpus.test_pipeline import *; write_fixture(pathlib.Path({corpus:?}), 10)"
        ),
    };
    let st = Command::new("python3")
        .args(["-c", &code])
        .current_dir(repo_root())
        .env("PYTHONPATH", repo_root())
        .status()
        .ok()?;
    st.success().then_some(corpus)
}

fn stage_args(corpus: &std::path::Path, run_dir: &std::path::Path) -> Args {
    Args {
        corpus_dir: corpus.to_path_buf(),
        corpus_manifest_sha256: sha256_file(&corpus.join("manifest.json")).unwrap(),
        run_dir: run_dir.to_path_buf(),
    }
}

#[tokio::test]
async fn converts_the_fixture_and_writes_shard_hashes_to_the_ledger() {
    let td = tempfile::tempdir().unwrap();
    let Some(corpus) = fixture(td.path(), None) else {
        eprintln!("SKIPPED: python3 / model.corpus.test_pipeline unavailable");
        return;
    };
    let stage_dir = td.path().join("stage");
    std::fs::create_dir_all(&stage_dir).unwrap();
    let ctx = StageContext::for_test(td.path().to_path_buf(), stage_dir.clone());
    let run_dir = td.path().join("run");
    let out = MaterializeCorpus
        .run(&ctx, (), &stage_args(&corpus, &run_dir))
        .await
        .unwrap();
    assert_eq!(out.n_examples, 25);
    let first = std::fs::read_to_string(&out.path).unwrap();
    let row: serde_json::Value = serde_json::from_str(first.lines().next().unwrap()).unwrap();
    assert!(
        row["prompt"]
            .as_str()
            .unwrap()
            .starts_with("Choose a legal chess move.")
    );
    assert_eq!(row["completion"], "e2e4");
    let ledger = std::fs::read_to_string(run_dir.join("ledger.jsonl")).unwrap();
    let rec: serde_json::Value = serde_json::from_str(ledger.lines().next().unwrap()).unwrap();
    assert_eq!(rec["stage"], "materialize_corpus");
    assert_eq!(rec["detail"]["shards"].as_array().unwrap().len(), 3);

    // Same stage directory again: the converter reuses the verified result.
    let again = MaterializeCorpus
        .run(&ctx, (), &stage_args(&corpus, &run_dir))
        .await
        .unwrap();
    assert_eq!(again.content_hash, out.content_hash);
    let ledger = std::fs::read_to_string(run_dir.join("ledger.jsonl")).unwrap();
    let last: serde_json::Value = serde_json::from_str(ledger.lines().last().unwrap()).unwrap();
    assert_eq!(last["detail"]["reused"], true);
}

#[tokio::test]
async fn a_planted_leak_refuses_the_stage_and_writes_no_dataset() {
    let td = tempfile::tempdir().unwrap();
    let Some(corpus) = fixture(td.path(), Some("game_id")) else {
        eprintln!("SKIPPED: python3 / model.corpus.test_pipeline unavailable");
        return;
    };
    let stage_dir = td.path().join("stage");
    std::fs::create_dir_all(&stage_dir).unwrap();
    let ctx = StageContext::for_test(td.path().to_path_buf(), stage_dir.clone());
    let err = MaterializeCorpus
        .run(&ctx, (), &stage_args(&corpus, &td.path().join("run")))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("corpus audit failed"), "{err}");
    assert!(!stage_dir.join("dataset.jsonl").exists());
}
