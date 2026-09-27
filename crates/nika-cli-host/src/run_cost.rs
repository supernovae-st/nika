// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Fresh local Run confirmation, independent of Session authoring approval.

// Host interaction/protocol projection is this module's effect boundary.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use nika_providers::InferenceAdmission;
use nika_providers::admission::{CostHostEvidence, CostReview, CostRoute, monetary_default};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
mod exchange;
mod lease;
mod readiness;
mod shape;
mod unsettled;
pub use exchange::ReviewChannel;
pub(crate) use readiness::readiness;

/// A fresh Run decision and its observation journal; never recovered authority.
#[non_exhaustive]
pub struct RunCost {
    pub account: InferenceAdmission,
    pub config: nika_runtime::RuntimeConfig,
    journal: Journal,
}
impl RunCost {
    /// Append observation only; no callable authority is serialized.
    /// # Errors
    /// Unreadable account or unwritable descriptor-rooted journal.
    pub fn observe(&self, phase: &str) -> Result<(), String> {
        self.journal.observe(phase)
    }
    /// Close live authority and persist the final observation, including uncertainty.
    /// # Errors
    /// Account closure or journal failure; the caller must report possible billing.
    pub fn finish(&self) -> Result<(), String> {
        self.journal.settle()
    }
}

/// The Run's side of the journal. It holds the writer lease from before the
/// `prepared` row until after the `settled` one, and every row it writes names
/// that writer. A Run that ends without `finish` (an early return, an unwinding
/// panic) still settles what its account observed when this drops; only a
/// process that dies leaves `prepared` behind, and its released lease lets the
/// next review record that Run as unknown.
struct Journal {
    account: InferenceAdmission,
    root: std::path::PathBuf,
    invocation: String,
    writer: lease::Writer,
    settled: AtomicBool,
    _lease: lease::Lease,
}
impl Journal {
    fn observe(&self, phase: &str) -> Result<(), String> {
        let receipt = self.account.snapshot().map_err(|e| e.to_string())?;
        let row = serde_json::json!({"schema":"nika/run-cost-observation@1", "invocation":self.invocation,
            "phase":phase, "observation":receipt.observation(), "lease":self.writer.json()});
        nika_fs::OwnedDir::open(&self.root)
            .and_then(|d| d.create_below(&[".nika"]))
            .and_then(|d| unsettled::append_row(&d, &row.to_string()))
            .map_err(|e| e.to_string())
    }
    fn settle(&self) -> Result<(), String> {
        self.account
            .close("Run ended; fresh decision required")
            .map_err(|e| e.to_string())?;
        self.observe("settled")?;
        self.settled.store(true, Ordering::SeqCst);
        Ok(())
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        if !self.settled.load(Ordering::SeqCst) {
            // Best effort while the lease is still held: a failure leaves the
            // `prepared` row, which the next review records as unknown.
            let _ = self.settle();
        }
    }
}

/// Take the writer lease and read what earlier Runs left, before any question:
/// a live Run that has not settled, or an exposure not yet reconciled, refuses.
fn clear_exposure(root: &Path, observer: &str) -> Result<(lease::Lease, lease::Writer), String> {
    let nika = nika_fs::OwnedDir::open(root)
        .and_then(|d| d.create_below(&[".nika"]))
        .map_err(|e| e.to_string())?;
    let writer = lease::Writer::this_process();
    let held = match lease::take(&nika, &writer)? {
        lease::Taken::Held(held) => held,
        lease::Taken::Busy { pid } => {
            let holder = pid.map_or_else(
                || "an unnamed process holds its cost lease".to_owned(),
                |pid| format!("process {pid} holds its cost lease"),
            );
            return Err(format!(
                "another unknown-cost Run in this project has not settled yet ({holder}) · no second Run, no automatic retry"
            ));
        }
    };
    let exposures = unsettled::fold(&nika, &writer.host, observer)?;
    if !exposures.is_clear() {
        return Err(unsettled::refusal(&exposures));
    }
    Ok((held, writer))
}
/// Unsupported hosts and workflow shapes never borrow this approval.
/// # Errors
/// Unsupported or changed scope, unreadable evidence, decline, or journal failure.
#[allow(clippy::too_many_arguments)]
pub fn review(
    root: &Path,
    file: &str,
    source: &str,
    invocation: String,
    wf: &nika_schema::raw::RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    inputs: &std::collections::BTreeMap<String, serde_json::Value>,
    invocation_default: Option<f64>,
    channel: impl Into<ReviewChannel>,
) -> Result<Option<RunCost>, String> {
    let config = nika_runtime::compose::config_from_env();
    let mut unknown = readiness::unknown_routes(plan, &config)?;
    if unknown.is_empty() {
        return Ok(None);
    }
    let channel = channel.into();
    if !channel.available() {
        return Err("price unknown: this host cannot obtain a fresh one-time choice; use an interactive local `nika run` or a host with explicit cap evidence and confirmation".into());
    }
    // The host's own review stays a message: the typed reason renders its words.
    let bound = nika_service_execution::run_cost::request_bound(wf, plan, unknown.len())
        .map_err(|e| e.to_string())?;
    let files = shape::read_witness(root, wf)?;
    if invocation_default == Some(0.0) {
        return Err("zero invocation ceiling refuses unknown spend before HTTP".into());
    }
    let (held, writer) = clear_exposure(root, &invocation)?;
    let config_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let project =
        nika_vocab::project::discover_reachable(&config_root).map_err(|e| e.to_string())?;
    if project.unreachable.is_some() {
        return Err("project monetary configuration is unreadable".into());
    }
    let project_default = project.found.as_ref().and_then(|(_, p)| p.ceiling);
    let (model, route) = unknown.remove(0);
    let candidate = witness(source, inputs, &(&project.found, &files));
    let review = CostReview::new(
        candidate.clone(),
        invocation.clone(),
        route,
        CostHostEvidence::unmanaged_interactive_local(),
        monetary_default(invocation_default)?,
        monetary_default(project_default)?,
    )?
    .for_run(bound)?;
    let pending = nika_providers::admission::PendingCostReview::new(
        review,
        nika_event::source_id::sha256_hex(source.as_bytes()),
        nika_event::source_id::sha256_hex(format!("{inputs:?}:{files:?}").as_bytes()),
    );
    let answer = channel.ask(pending.challenge())?;
    let actual_source = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    if actual_source != source {
        return Err("workflow changed during review; Run not started".into());
    }
    let project_now =
        nika_vocab::project::discover_reachable(&config_root).map_err(|e| e.to_string())?;
    if project_now.unreachable.is_some() {
        return Err("project configuration became unreadable".into());
    }
    let files_now = shape::read_witness(root, wf)?;
    let actual_candidate = witness(source, inputs, &(&project_now.found, &files_now));
    let actual_route = CostRoute::observe(&model, nika_runtime::compose::config_from_env())?;
    let account = pending.confirm(&answer, &actual_candidate, &actual_route)?;
    let config = nika_runtime::RuntimeConfig::new(None, 0)
        .with_inference_admission(&account, &actual_candidate, &invocation)
        .map_err(|e| e.to_string())?;
    let journal = Journal {
        account: account.clone(),
        root: root.into(),
        invocation,
        writer,
        settled: AtomicBool::new(false),
        _lease: held,
    };
    let choice = RunCost {
        account,
        config,
        journal,
    };
    choice.observe("prepared")?;
    Ok(Some(choice))
}

fn witness(
    source: &str,
    inputs: &std::collections::BTreeMap<String, serde_json::Value>,
    project: &impl std::fmt::Debug,
) -> String {
    nika_event::source_id::sha256_hex(format!("{source:?}:{inputs:?}:{project:?}").as_bytes())
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types
)]
mod tests {
    use super::unsettled::JOURNAL;
    use super::*;
    use std::io::BufRead as _;
    fn cost(root: &Path) -> RunCost {
        let route = CostRoute::observe(
            "deepseek/deepseek-v4-pro",
            nika_providers::ProvidersConfig::new(),
        )
        .unwrap();
        let account = CostReview::new(
            "candidate".into(),
            "run-1".into(),
            route.clone(),
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .unwrap()
        .confirm("candidate", &route)
        .unwrap();
        let config = nika_runtime::RuntimeConfig::new(None, 0)
            .with_inference_admission(&account, "candidate", "run-1")
            .unwrap();
        let nika = nika_fs::OwnedDir::open(root)
            .unwrap()
            .create_below(&[".nika"])
            .unwrap();
        let writer = lease::Writer::this_process();
        let lease::Taken::Held(held) = lease::take(&nika, &writer).unwrap() else {
            panic!("a fresh project's cost lease is free");
        };
        let journal = Journal {
            account: account.clone(),
            root: root.into(),
            invocation: "run-1".into(),
            writer,
            settled: AtomicBool::new(false),
            _lease: held,
        };
        RunCost {
            account,
            config,
            journal,
        }
    }
    fn rows(root: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(root.join(".nika").join(JOURNAL))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    #[test]
    fn prepared_run_is_not_replayed_and_settled_observation_cannot_restore_authority() {
        let root = tempfile::tempdir().unwrap();
        let cost = cost(root.path());
        cost.observe("prepared").unwrap();
        let live = clear_exposure(root.path(), "run-2").unwrap_err();
        assert!(live.contains("has not settled yet"), "{live}");
        cost.finish().unwrap();
        let rows_now = rows(root.path());
        let row = rows_now.last().unwrap();
        assert_eq!(row["phase"], "settled");
        assert_eq!(row["observation"]["known_subtotal_nano_usd"], "0");
        assert_eq!(row["observation"]["unknown_calls"], 0);
        assert!(row["observation"]["limit_nano_usd"].is_null());
        assert_eq!(row["lease"]["pid"], std::process::id());
        assert_eq!(
            cost.account.snapshot().unwrap().state,
            nika_providers::AdmissionState::Closed
        );
        drop(cost);
        assert!(clear_exposure(root.path(), "run-2").is_ok());
        assert_eq!(rows(root.path()).len(), 2, "a settled Run settles once");
    }
    /// Control · an early return after `prepared` (a pre-dispatch failure before
    /// the runtime starts) is no death: dropping the unfinished Run settles what
    /// its account observed — closed, nothing sent — and never reads unknown.
    #[test]
    fn a_run_that_ends_without_finish_settles_what_its_account_observed() {
        let root = tempfile::tempdir().unwrap();
        let cost = cost(root.path());
        cost.observe("prepared").unwrap();
        drop(cost);
        let rows = rows(root.path());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["phase"], "settled");
        assert_eq!(rows[1]["observation"]["state"], "Closed");
        assert_eq!(rows[1]["observation"]["unknown_calls"], 0);
        assert!(clear_exposure(root.path(), "run-2").is_ok());
    }
    /// The private journal (lease · settle-on-drop) keeps the public type's
    /// auto traits: a host may still move a reviewed Run across threads.
    #[test]
    fn a_reviewed_run_stays_send_and_sync() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<RunCost>();
    }
    #[test]
    fn missing_or_corrupt_observation_fields_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let dir = nika_fs::OwnedDir::open(root.path())
            .unwrap()
            .create_below(&[".nika"])
            .unwrap();
        dir.append_line(
            JOURNAL,
            r#"{"schema":"nika/run-cost-observation@1","invocation":"run-1","phase":"settled"}"#,
        )
        .unwrap();
        assert!(clear_exposure(root.path(), "run-2").is_err());
    }
    /// The killed writer: prepares under the lease, names itself, then waits to
    /// be killed. Only the parent test below enters it (the marker file).
    #[test]
    fn killed_writer_fixture_child() {
        if !Path::new(".p3-killed-writer-fixture").exists() {
            return;
        }
        let cost = cost(Path::new("."));
        cost.observe("prepared").unwrap();
        println!("PREPARED {}", std::process::id());
        std::thread::sleep(std::time::Duration::from_secs(30));
        drop(cost);
    }
    /// P3 · a writer killed after `prepared` runs no handler. While it lives its
    /// lease refuses a second review and nothing is judged; once the kernel has
    /// released it, the next review records THAT Run as unknown — once, appended
    /// after the untouched row it was derived from — and still refuses.
    #[test]
    fn a_killed_writer_is_recorded_unknown_by_the_next_review_and_still_blocks() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".p3-killed-writer-fixture"), "test only").unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "run_cost::tests::killed_writer_fixture_child",
                "--nocapture",
                "--quiet",
            ])
            .current_dir(root.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let pid: u64 = loop {
            let mut line = String::new();
            assert!(out.read_line(&mut line).unwrap() > 0, "the child prepared");
            if let Some(pid) = line.trim().strip_prefix("PREPARED ") {
                break pid.parse().unwrap();
            }
        };
        let live = clear_exposure(root.path(), "run-2").unwrap_err();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            live.contains(&format!("process {pid} holds its cost lease")),
            "{live}"
        );
        assert_eq!(rows(root.path()).len(), 1, "a live writer is never judged");
        let refused = clear_exposure(root.path(), "run-2").unwrap_err();
        assert!(
            refused.contains(&format!(
                "Run run-1 ended without a settlement and its process {pid} is gone"
            )),
            "{refused}"
        );
        let text = std::fs::read_to_string(root.path().join(".nika").join(JOURNAL)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "one durable unknown, appended");
        let derived: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(derived["invocation"], "run-1");
        assert_eq!(derived["phase"], "unknown");
        assert_eq!(derived["unsettled"]["writer"]["pid"], pid);
        assert_eq!(
            derived["unsettled"]["prior_sha256"],
            nika_event::source_id::sha256_hex(lines[0].as_bytes())
        );
        assert_eq!(derived["unsettled"]["observed_by"], "run-2");
        assert!(clear_exposure(root.path(), "run-3").is_err(), "no retry");
        assert_eq!(rows(root.path()).len(), 2, "recorded once");
    }
}
