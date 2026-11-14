//! Stage `register_corpus`: the standard `register_dataset`, made rerunnable.
//!
//! `register_dataset` inserts a row and fails with "already registered" when the
//! name exists, so a restarted or resumed run (same corpus, same dataset name)
//! would die at this stage. Here an existing row with the same sha256 is
//! accepted as is, a row with the same name and a different sha256 is refused
//! (the name pins corpus and prompt hashes, so that means the converter changed
//! its output without its pins changing), and only a missing row is delegated to
//! the standard stage.

use async_trait::async_trait;
use blut::artifacts::DatasetJsonl;
use blut::datasets_db;
use blut::framework::compat::Compatible;
use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};
use blut_backends::HfTrainerBackend;
use blut_backends::stages::register_dataset::{Args, RegisterDataset};

pub struct RegisterCorpus;
impl Compatible<HfTrainerBackend> for RegisterCorpus {}

fn backend(e: impl std::fmt::Display) -> StageError {
    StageError::Backend(anyhow::anyhow!("{e}"))
}

#[async_trait]
impl Stage for RegisterCorpus {
    const NAME: &'static str = "register_corpus";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Disk];
    type Input = DatasetJsonl;
    type Output = DatasetJsonl;
    type Args = Args;

    async fn run(
        &self,
        ctx: &StageContext,
        input: DatasetJsonl,
        args: &Args,
    ) -> Result<DatasetJsonl, StageError> {
        let (name, path) = (args.name.clone(), input.path.clone());
        let existing =
            tokio::task::spawn_blocking(move || -> Result<Option<(String, String)>, String> {
                let conn = datasets_db::open().map_err(|e| e.to_string())?;
                let found = datasets_db::get_by_name(&conn, &name).map_err(|e| e.to_string())?;
                match found {
                    None => Ok(None),
                    Some(rec) => {
                        let mine = datasets_db::record_from_jsonl(name, &path, "sft", None)
                            .map_err(|e| e.to_string())?;
                        Ok(Some((rec.sha256, mine.sha256)))
                    }
                }
            })
            .await
            .map_err(backend)?
            .map_err(backend)?;
        match existing {
            None => RegisterDataset.run(ctx, input, args).await,
            Some((registered, mine)) if registered == mine => Ok(input),
            Some((registered, mine)) => Err(StageError::BadInput(format!(
                "dataset '{}' is registered with sha256 {registered} but this materialization is {mine}",
                args.name
            ))),
        }
    }
}
