// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Fresh local Run confirmation, independent of Session authoring approval.

// Host interaction/protocol projection is this module's effect boundary.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use nika_providers::InferenceAdmission;
use nika_providers::admission::{CostHostEvidence, CostReview, CostRoute, monetary_default};
use std::io::Read as _;
use std::path::Path;
mod exchange;
mod readiness;
mod shape;
pub use exchange::ReviewChannel;
pub(crate) use readiness::readiness;
const JOURNAL: &str = "inference-cost-observations.ndjson";

/// A fresh Run decision and its observation journal; never recovered authority.
#[non_exhaustive]
pub struct RunCost {
    pub account: InferenceAdmission,
    pub config: nika_runtime::RuntimeConfig,
    root: std::path::PathBuf,
    invocation: String,
}
impl RunCost {
    /// Append observation only; no callable authority is serialized.
    /// # Errors
    /// Unreadable account or unwritable descriptor-rooted journal.
    pub fn observe(&self, phase: &str) -> Result<(), String> {
        let receipt = self.account.snapshot().map_err(|e| e.to_string())?;
        let row = serde_json::json!({"schema":"nika/run-cost-observation@1", "invocation":self.invocation,
            "phase":phase, "observation":receipt.observation()});
        nika_fs::OwnedDir::open(&self.root)
            .and_then(|d| d.create_below(&[".nika"]))
            .and_then(|d| d.append_line(JOURNAL, &row.to_string()))
            .map_err(|e| e.to_string())
    }
    /// Close live authority and persist the final observation, including uncertainty.
    /// # Errors
    /// Account closure or journal failure; the caller must report possible billing.
    pub fn finish(&self) -> Result<(), String> {
        self.account
            .close("Run ended; fresh decision required")
            .map_err(|e| e.to_string())?;
        self.observe("settled")
    }
}
fn exposure_clear(root: &Path) -> Result<(), String> {
    let file = nika_fs::OwnedDir::open(root)
        .and_then(|d| d.open_below(&[".nika"]))
        .and_then(|d| d.open_relative(Path::new(JOURNAL)));
    let mut text = String::new();
    match file {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
        Ok(f) => {
            f.take(1_048_577)
                .read_to_string(&mut text)
                .map_err(|e| e.to_string())?;
        }
    }
    if text.len() > 1_048_576 {
        return Err("cost observation journal exceeds the read bound".into());
    }
    let mut latest = std::collections::BTreeMap::new();
    for line in text.lines() {
        let row: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let id = row["invocation"]
            .as_str()
            .ok_or("unreadable cost invocation")?
            .to_owned();
        if row["schema"] != "nika/run-cost-observation@1" {
            return Err("unrecognized cost observation".into());
        }
        let observation = &row["observation"];
        if observation["schema"] != "nika/inference-cost-observation@1"
            || observation["known_subtotal_nano_usd"]
                .as_str()
                .and_then(|v| v.parse::<i128>().ok())
                .is_none()
            || observation["unknown_calls"].as_u64().is_none()
            || !matches!(
                observation["state"].as_str(),
                Some("Open" | "Closed" | "Uncertain")
            )
        {
            return Err("unreadable cost observation; prior exposure is unknown".into());
        }
        latest.insert(id, row);
    }
    if latest
        .values()
        .any(|r| r["phase"] != "settled" || r["observation"]["state"] == "Uncertain")
    {
        return Err("an earlier dispatch has uncertain billing; inspect/reconcile its observation before a new Run, no automatic retry".into());
    }
    Ok(())
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
    let bound = nika_service_execution::run_cost::request_bound(wf, plan, unknown.len())?;
    let files = shape::read_witness(root, wf)?;
    if invocation_default == Some(0.0) {
        return Err("zero invocation ceiling refuses unknown spend before HTTP".into());
    }
    exposure_clear(root)?;
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
    let choice = RunCost {
        account,
        config,
        root: root.into(),
        invocation,
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
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
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
        RunCost {
            account,
            config,
            root: root.into(),
            invocation: "run-1".into(),
        }
    }
    #[test]
    fn prepared_run_is_not_replayed_and_settled_observation_cannot_restore_authority() {
        let root = tempfile::tempdir().unwrap();
        let cost = cost(root.path());
        cost.observe("prepared").unwrap();
        assert!(exposure_clear(root.path()).is_err());
        cost.finish().unwrap();
        assert!(exposure_clear(root.path()).is_ok());
        let text = std::fs::read_to_string(root.path().join(".nika").join(JOURNAL)).unwrap();
        let row: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(row["observation"]["known_subtotal_nano_usd"], "0");
        assert_eq!(row["observation"]["unknown_calls"], 0);
        assert!(row["observation"]["limit_nano_usd"].is_null());
        assert_eq!(
            cost.account.snapshot().unwrap().state,
            nika_providers::AdmissionState::Closed
        );
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
        assert!(exposure_clear(root.path()).is_err());
    }
}
