//! Stage `claim_run`: the recipe-level crash gate.
//!
//! BLUT's resume policy (`blut::framework::resume`) decides from a run-state
//! marker whether a starting run may RESUME a crashed predecessor, start FRESH,
//! or must REFUSE because another run is alive. The standard trainer stage does
//! not write that marker, so this stage does, in `<run_dir>/state.json`, before
//! training starts:
//!
//! * no marker: fresh start;
//! * marker `running`, same job id: an in-process retry, resume;
//! * marker `running`, another job id, heartbeat older than `stale_after_secs`:
//!   that run crashed, resume from its newest checkpoint;
//! * marker `running`, another job id, fresh heartbeat, but its pid no longer
//!   exists on this host: that run died (a failed plan leaves its marker
//!   `running`); resume without waiting for the heartbeat to go stale;
//! * marker `running`, another job id, fresh heartbeat, pid alive: refuse, one
//!   trainer per run directory;
//! * marker `finished`: refuse. A finished run is never extended; start a new
//!   run name (the previous run directory stays untouched).
//!
//! A background task keeps the heartbeat fresh while this process lives, so a
//! killed process goes stale and a live one never does. The trainer resumes
//! from the newest checkpoint in the run's stable `checkpoints/` directory
//! (`resume = true` is always set); `register_run` marks the marker `finished`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use blut::artifacts::DatasetJsonl;
use blut::framework::compat::Compatible;
use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::resume::{
    DEFAULT_STALE_AFTER_SECS, HEARTBEAT_INTERVAL_SECS, ResumeDecision, ResumeState, decide_resume,
};
use blut::framework::stage::{Stage, StageContext};
use blut_backends::HfTrainerBackend;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::runctx::{io_err, ledger_append, now_unix};

pub struct ClaimRun;
impl Compatible<HfTrainerBackend> for ClaimRun {}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    pub run_dir: PathBuf,
    pub run: String,
    /// Weights the trainer will load; checked to exist before any work starts.
    pub base_weights: PathBuf,
    #[serde(default)]
    pub stale_after_secs: Option<u64>,
    #[serde(default)]
    pub heartbeat_secs: Option<u64>,
}

/// Whether `pid` is a live process on this host (Linux `/proc`; elsewhere
/// assume alive, which only makes the gate more conservative).
pub fn pid_alive(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        Path::new("/proc").join(pid.to_string()).exists()
    } else {
        true
    }
}

pub fn state_path(run_dir: &Path) -> PathBuf {
    run_dir.join("state.json")
}

pub fn write_state(run_dir: &Path, state: &ResumeState) -> Result<(), StageError> {
    let path = state_path(run_dir);
    let tmp = run_dir.join("state.json.tmp");
    let body = serde_json::to_vec_pretty(state).map_err(|e| StageError::Backend(e.into()))?;
    std::fs::write(&tmp, body).map_err(|e| io_err(&tmp, e))?;
    std::fs::rename(&tmp, &path).map_err(|e| io_err(&path, e))
}

/// Mark the run finished (called by `register_run`).
pub fn mark_finished(run_dir: &Path, run_id: &str) -> Result<(), StageError> {
    let mut state = ResumeState::read(run_dir).unwrap_or_default();
    state.status = "finished".into();
    state.run_id = run_id.into();
    state.heartbeat_unix = now_unix();
    write_state(run_dir, &state)
}

/// Pure decision plus the refusal wording, separated for tests.
pub fn gate(
    state: Option<&ResumeState>,
    job_id: &str,
    now: u64,
    stale_after: u64,
    run: &str,
    pid_alive: impl Fn(u32) -> bool,
) -> Result<ResumeDecision, String> {
    if let Some(s) = state
        && s.status == "finished"
    {
        return Err(format!(
            "run '{run}' already finished (job {}); runs are not extended. Choose a new run name; \
             the finished run directory is left untouched",
            s.run_id
        ));
    }
    match decide_resume(state, job_id, now, stale_after) {
        ResumeDecision::RefuseConcurrent => {
            let s = state.expect("refusal implies a marker");
            if s.pid != 0 && !pid_alive(s.pid) {
                return Ok(ResumeDecision::Resume);
            }
            Err(format!(
                "run '{run}' is owned by live job {} (pid {}, heartbeat {}s ago, stale after \
                 {stale_after}s); refusing to start a second trainer on the same run directory",
                s.run_id,
                s.pid,
                now.saturating_sub(s.heartbeat_unix)
            ))
        }
        other => Ok(other),
    }
}

#[async_trait]
impl Stage for ClaimRun {
    const NAME: &'static str = "claim_run";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Disk];
    // Never cached: the claim must happen on every start, not only the first.
    const DETERMINISTIC: bool = false;
    type Input = DatasetJsonl;
    type Output = DatasetJsonl;
    type Args = Args;

    async fn preflight(&self, args: &Args) -> Result<(), StageError> {
        if !args.base_weights.is_file() {
            return Err(StageError::BadInput(format!(
                "base weights not found: {}",
                args.base_weights.display()
            )));
        }
        Ok(())
    }

    async fn run(
        &self,
        ctx: &StageContext,
        input: DatasetJsonl,
        args: &Args,
    ) -> Result<DatasetJsonl, StageError> {
        std::fs::create_dir_all(&args.run_dir).map_err(|e| io_err(&args.run_dir, e))?;
        let job_id = ctx
            .job_dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let stale = args.stale_after_secs.unwrap_or(DEFAULT_STALE_AFTER_SECS);
        let existing = ResumeState::read(&args.run_dir);
        let decision = gate(
            existing.as_ref(),
            &job_id,
            now_unix(),
            stale,
            &args.run,
            pid_alive,
        )
        .map_err(StageError::BadInput)?;
        let state = ResumeState {
            status: "running".into(),
            run_id: job_id.clone(),
            pid: std::process::id(),
            heartbeat_unix: now_unix(),
            ..Default::default()
        };
        write_state(&args.run_dir, &state)?;
        ledger_append(
            &args.run_dir,
            "claim_run",
            match decision {
                ResumeDecision::Resume => "resume",
                _ => "fresh",
            },
            json!({
                "job_id": job_id,
                "pid": state.pid,
                "previous_job": existing.as_ref().map(|s| s.run_id.clone()),
                "stale_after_secs": stale,
            }),
        )?;
        // Heartbeat for as long as this process lives; stops when the marker
        // is finished or taken over by another job.
        let every = Duration::from_secs(
            args.heartbeat_secs
                .unwrap_or(HEARTBEAT_INTERVAL_SECS)
                .max(1),
        );
        let (dir, id, pid) = (args.run_dir.clone(), job_id, state.pid);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                match ResumeState::read(&dir) {
                    Some(s) if s.status == "running" && s.run_id == id => {
                        let beat = ResumeState {
                            heartbeat_unix: now_unix(),
                            pid,
                            ..s
                        };
                        if write_state(&dir, &beat).is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        });
        Ok(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(status: &str, run_id: &str, hb: u64) -> ResumeState {
        ResumeState {
            status: status.into(),
            run_id: run_id.into(),
            pid: 7,
            heartbeat_unix: hb,
            ..Default::default()
        }
    }

    #[test]
    fn no_marker_is_fresh() {
        assert_eq!(
            gate(None, "j1", 1000, 180, "r", |_| true).unwrap(),
            ResumeDecision::Fresh
        );
    }

    #[test]
    fn same_job_resumes() {
        let s = st("running", "j1", 1000);
        assert_eq!(
            gate(Some(&s), "j1", 1001, 180, "r", |_| true).unwrap(),
            ResumeDecision::Resume
        );
    }

    #[test]
    fn crashed_foreign_run_is_taken_over() {
        let s = st("running", "j0", 1000);
        assert_eq!(
            gate(Some(&s), "j1", 1181, 180, "r", |_| true).unwrap(),
            ResumeDecision::Resume
        );
    }

    #[test]
    fn live_foreign_run_is_refused() {
        let s = st("running", "j0", 1000);
        let e = gate(Some(&s), "j1", 1100, 180, "r", |_| true).unwrap_err();
        assert!(e.contains("live job j0"), "{e}");
    }

    #[test]
    fn fresh_heartbeat_with_a_dead_pid_is_taken_over() {
        let s = st("running", "j0", 1000);
        assert_eq!(
            gate(Some(&s), "j1", 1010, 180, "r", |_| false).unwrap(),
            ResumeDecision::Resume
        );
    }

    #[test]
    fn pid_alive_sees_this_process_and_not_a_reaped_child() {
        assert!(pid_alive(std::process::id()));
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!pid_alive(pid));
    }

    #[test]
    fn finished_run_is_refused_not_extended() {
        let s = st("finished", "j0", 1000);
        let e = gate(Some(&s), "j1", 9999, 180, "r", |_| true).unwrap_err();
        assert!(
            e.contains("already finished") && e.contains("new run name"),
            "{e}"
        );
    }

    #[test]
    fn mark_finished_round_trips() {
        let td = tempfile::tempdir().unwrap();
        write_state(td.path(), &st("running", "j1", 5)).unwrap();
        mark_finished(td.path(), "j1").unwrap();
        assert_eq!(ResumeState::read(td.path()).unwrap().status, "finished");
    }
}
