//! Base-model profiles: the pinned identity of each model the recipe can train.
//!
//! The recipe takes `base = "<key>"`; everything model-specific lives here so
//! adding a model is one entry. `lora_regex = None` leaves the choice to the
//! trainer's per-`model_type` default (which already covers both models below);
//! the recipe arg `lora_target_regex` overrides either.

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct BaseProfile {
    pub key: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    /// Default local snapshot directory (artifact store; override with `base_dir`).
    pub dir: &'static str,
    /// Weights file name inside `dir`.
    pub weights_file: &'static str,
    /// sha256 of the weights file.
    pub weights_sha256: &'static str,
    /// Whether the trainer's own `base_sha256` check can run: it reads
    /// `<dir>/model.safetensors`, so a sharded or differently named file cannot
    /// use it. The recipe still records the digest in the run manifest.
    pub trainer_checks_sha: bool,
    pub lora_regex: Option<&'static str>,
    /// Whether `model.serve.inference.Predictor` (and so the evaluation
    /// harness) can load this base. It pins the SmolVLM digest.
    pub evaluable: bool,
    /// A vocabulary extension built into `dir` before training (`python -m
    /// model.train.ctok build-base`). `weights_sha256` stays the pinned STOCK
    /// weights: the extension record must name them, and the extended file's
    /// own digest is checked against that record by [`BaseProfile::verify_extension`].
    pub extension: Option<Extension>,
}

/// The extended-vocabulary base the recipe may train new rows of.
#[derive(Debug)]
pub struct Extension {
    pub spec_version: &'static str,
    /// First new row: the host vocabulary size.
    pub base: u32,
    /// Number of new rows (ctok/1: 2,496).
    pub size: u32,
}

/// An extended base after verification: the digest the trainer must check.
#[derive(Debug, PartialEq)]
pub struct VerifiedExtension {
    pub extended_weights_sha256: String,
    pub vocab_sha256: String,
    pub seed: u64,
}

pub const PROFILES: &[BaseProfile] = &[
    BaseProfile {
        key: "smolvlm-500m",
        repository: "HuggingFaceTB/SmolVLM-500M-Instruct",
        revision: "a7da5b986cb59b408707209984f360a5f4ad7e47",
        dir: "/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98",
        weights_file: "model.safetensors",
        weights_sha256: "d05b567eeaf534e83d375551f068ed57b5f52d37c657197f644af5ef9db091a2",
        trainer_checks_sha: true,
        lora_regex: None,
        evaluable: true,
        extension: None,
    },
    // The same stock weights with the ctok/1 vocabulary appended (arms T1/T3 of the S0 probe).
    // The serving predictor pins the stock digest and has not been run on an extended base, so
    // evaluation is skipped; the snapshot is built once by `python -m model.train.ctok build-base`.
    BaseProfile {
        key: "smolvlm-500m-ctok1",
        repository: "HuggingFaceTB/SmolVLM-500M-Instruct",
        revision: "a7da5b986cb59b408707209984f360a5f4ad7e47",
        dir: "/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-ctok1",
        weights_file: "model.safetensors",
        weights_sha256: "d05b567eeaf534e83d375551f068ed57b5f52d37c657197f644af5ef9db091a2",
        trainer_checks_sha: true,
        lora_regex: None,
        evaluable: false,
        extension: Some(Extension {
            spec_version: "ctok/1",
            base: 49280,
            size: 2496,
        }),
    },
    BaseProfile {
        key: "qwen3_5-0.8b",
        repository: "Qwen/Qwen3.5-0.8B-Base",
        revision: "dc7cdfe2ee4154fa7e30f5b51ca41bfa40174e68",
        dir: "/mnt/4tb/models/qwen3.5-0.8b-base",
        weights_file: "model.safetensors-00001-of-00001.safetensors",
        // The Hugging Face LFS etag of that file, which is its sha256.
        weights_sha256: "c2b1e5a17d9c1e27685d92ed9b382911ebb99955ecd89052d1721241adfbab6c",
        trainer_checks_sha: false,
        lora_regex: None,
        evaluable: false,
        extension: None,
    },
];

pub fn find(key: &str) -> Option<&'static BaseProfile> {
    PROFILES.iter().find(|p| p.key == key)
}

pub fn keys() -> Vec<&'static str> {
    PROFILES.iter().map(|p| p.key).collect()
}

impl BaseProfile {
    pub fn dir_or(&self, over: Option<&Path>) -> PathBuf {
        over.map_or_else(|| PathBuf::from(self.dir), Path::to_path_buf)
    }
}

impl BaseProfile {
    /// Check `<dir>/ctok-extension.json` against this profile and the weights file on disk.
    /// The record must be the one `build-base` writes, for this spec and these row counts, built
    /// from the pinned stock weights, and the extended weights must hash to what it recorded.
    /// (The vocabulary hash is checked by the preflight, which can import the vocabulary.)
    pub fn verify_extension(&self, dir: &Path) -> Result<VerifiedExtension, String> {
        let ext = self
            .extension
            .as_ref()
            .ok_or_else(|| format!("base '{}' has no vocabulary extension", self.key))?;
        let record_path = dir.join("ctok-extension.json");
        let text = std::fs::read_to_string(&record_path).map_err(|e| {
            format!(
                "{}: {e} (build it: python -m model.train.ctok build-base)",
                record_path.display()
            )
        })?;
        let record: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", record_path.display()))?;
        let want = |field: &str, ok: bool| {
            if ok {
                Ok(())
            } else {
                Err(format!(
                    "{}: '{field}' does not match the '{}' profile",
                    record_path.display(),
                    self.key
                ))
            }
        };
        want("format", record["format"] == "knightlm-ctok-extension/1")?;
        want("spec_version", record["spec_version"] == ext.spec_version)?;
        want("base", record["base"] == ext.base)?;
        want("vocab_size", record["vocab_size"] == ext.size)?;
        want("head_untied", record["head_untied"] == true)?;
        want(
            "stock_weights_sha256",
            record["stock_weights_sha256"] == self.weights_sha256,
        )?;
        let recorded = record["extended_weights_sha256"]
            .as_str()
            .ok_or("extended_weights_sha256 missing from the extension record")?;
        let weights = dir.join(self.weights_file);
        let actual = crate::runctx::sha256_file(&weights)
            .map_err(|e| format!("{}: {e}", weights.display()))?;
        if actual != recorded {
            return Err(format!(
                "extended weights sha256 {actual} != recorded {recorded} ({})",
                weights.display()
            ));
        }
        if actual == self.weights_sha256 {
            return Err("the weights are the stock file: the base was not extended".into());
        }
        Ok(VerifiedExtension {
            extended_weights_sha256: actual,
            vocab_sha256: record["vocab_sha256"].as_str().unwrap_or_default().into(),
            seed: record["seed"].as_u64().unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctok() -> &'static BaseProfile {
        find("smolvlm-500m-ctok1").unwrap()
    }

    fn snapshot(td: &tempfile::TempDir, edit: impl Fn(&mut serde_json::Value)) -> VerifyDir {
        let weights = td.path().join("model.safetensors");
        std::fs::write(&weights, b"extended weights").unwrap();
        let sha = crate::runctx::sha256_file(&weights).unwrap();
        let mut record = json!({
            "format": "knightlm-ctok-extension/1", "spec_version": "ctok/1", "vocab_sha256": "v",
            "vocab_size": 2496, "base": 49280, "seed": 0, "head_untied": true,
            "stock_weights_sha256": ctok().weights_sha256, "extended_weights_sha256": sha,
        });
        edit(&mut record);
        std::fs::write(td.path().join("ctok-extension.json"), record.to_string()).unwrap();
        VerifyDir(td.path().to_path_buf(), sha)
    }

    struct VerifyDir(PathBuf, String);

    #[test]
    fn a_matching_extension_is_verified_and_names_the_extended_digest() {
        let td = tempfile::tempdir().unwrap();
        let dir = snapshot(&td, |_| {});
        let ok = ctok().verify_extension(&dir.0).unwrap();
        assert_eq!(ok.extended_weights_sha256, dir.1);
        assert_ne!(ok.extended_weights_sha256, ctok().weights_sha256);
    }

    #[test]
    fn a_wrong_record_or_tampered_weights_are_refused() {
        for (field, value) in [
            ("spec_version", json!("ctok/2")),
            ("base", json!(49281)),
            ("vocab_size", json!(2495)),
            ("head_untied", json!(false)),
            ("stock_weights_sha256", json!("00")),
            ("extended_weights_sha256", json!("00")),
        ] {
            let td = tempfile::tempdir().unwrap();
            let dir = snapshot(&td, |r| r[field] = value.clone());
            assert!(ctok().verify_extension(&dir.0).is_err(), "{field}");
        }
        let td = tempfile::tempdir().unwrap();
        let dir = snapshot(&td, |_| {});
        std::fs::write(dir.0.join("model.safetensors"), b"changed").unwrap();
        assert!(
            ctok()
                .verify_extension(&dir.0)
                .unwrap_err()
                .contains("!= recorded")
        );
        let empty = tempfile::tempdir().unwrap();
        assert!(
            ctok()
                .verify_extension(empty.path())
                .unwrap_err()
                .contains("build-base")
        );
    }

    #[test]
    fn the_stock_weights_are_not_an_extension_and_stock_profiles_have_none() {
        assert!(
            find("smolvlm-500m")
                .unwrap()
                .verify_extension(Path::new("/x"))
                .is_err()
        );
        assert!(find("qwen3_5-0.8b").unwrap().extension.is_none());
    }
}
