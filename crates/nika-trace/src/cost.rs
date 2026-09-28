// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika trace cost` — P4: this project's unknown cost exposures, read as
//! data, and the one supported door to resolve one Run's exposure. The law
//! and the documents belong to the journal's owner
//! (`nika_dap::cost_journal::reconcile`); this verb supplies what only the
//! host has (the launch directory, the OS account, the clock, a fresh
//! execution id) and renders. It never prompts: a resolution is an explicit
//! command with every field named, and it authorizes no Run.

// The verb owns its streams: a machine document always goes to stdout (a host
// reads it whatever the exit); a human refusal from the environment goes to
// stderr, like every `trace` verb's.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::path::Path;

use clap::{Args, Subcommand, ValueEnum};
use nika_dap::cost_journal::Writer;
use nika_dap::cost_journal::reconcile::{
    self, Evidence, Inspection, Principal, Receipt, Refusal, Request, Resolution, RunReport,
};
use nika_dap::escape_tty;

use crate::exit;

/// `nika trace cost` — this project's unknown cost exposures.
#[derive(Args, Debug)]
#[non_exhaustive]
pub struct CostArgs {
    /// Resolve one Run's exposure instead of inspecting.
    #[command(subcommand)]
    pub action: Option<CostAction>,
    /// One JSON document (`cost_inspect_version: 1`) instead of the report.
    #[arg(long)]
    pub json: bool,
}

/// The cost door's one gesture.
#[derive(Subcommand, Debug)]
#[non_exhaustive]
pub enum CostAction {
    /// Append one resolution of one Run's unknown exposure: an operator
    /// attestation, never verified billing. It authorizes no Run.
    Reconcile(ReconcileArgs),
}

/// `nika trace cost reconcile` — every field explicit, no prompt.
#[derive(Args, Debug)]
#[non_exhaustive]
pub struct ReconcileArgs {
    /// The Run's invocation (`exe-…`), as `nika trace cost` names it.
    pub invocation: String,
    /// The binding `nika trace cost` reported for this project.
    #[arg(long)]
    pub project: String,
    /// The sha256 of the Run's latest row, as `nika trace cost` names it.
    #[arg(long)]
    pub prior: String,
    /// `still-unknown` records your finding and keeps blocking.
    #[arg(long, value_enum)]
    pub resolution: ResolutionArg,
    /// Where you looked (an invoice line, a dashboard entry, a ticket): 1 to
    /// 512 characters.
    #[arg(long)]
    pub reference: String,
    /// The evidence class. The one supported class is your own attestation,
    /// recorded unverified.
    #[arg(long, value_enum, default_value = "operator-attestation")]
    pub evidence: EvidenceArg,
    /// One JSON document (`cost_reconcile_version: 1`) instead of the receipt.
    #[arg(long)]
    pub json: bool,
}

/// A resolution as typed.
#[derive(ValueEnum, Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ResolutionArg {
    /// The provider billed it.
    Billed,
    /// The provider did not bill it.
    NotBilled,
    /// Still unknown: recorded, and the Run keeps blocking.
    StillUnknown,
}

/// The supported evidence classes.
#[derive(ValueEnum, Clone, Copy, Debug)]
#[non_exhaustive]
pub enum EvidenceArg {
    /// Your own attestation: it ends the exposure on your word, and Nika
    /// verifies no billing.
    OperatorAttestation,
}

/// `nika trace cost [reconcile …]` from the launch directory (the one root
/// the `.nika/` convention anchors at): print, return the exit code.
#[must_use]
pub fn run(args: CostArgs) -> u8 {
    let root = match std::env::current_dir() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("nika: cannot read the launch directory: {error}");
            return exit::ENV;
        }
    };
    let writer = Writer::this_process();
    match args.action {
        None => inspect(&root, &writer, args.json),
        Some(CostAction::Reconcile(reconcile)) => resolve(&root, &writer, &reconcile),
    }
}

fn inspect(root: &Path, writer: &Writer, json: bool) -> u8 {
    let observer = nika_types::id::ExecutionId::generate().to_string();
    match reconcile::inspect(root, writer, &observer) {
        Ok(seen) => {
            if json {
                println!("{}", seen.to_value());
            } else {
                println!("{}", render_inspection(&seen));
            }
            exit::OK
        }
        Err(Refusal::NoJournal) => {
            if json {
                let none = serde_json::json!({"cost_inspect_version": 1, "journal": null,
                    "clear": true, "derived": [], "runs": [], "torn": [], "conflicts": []});
                println!("{none}");
            } else {
                println!("no cost journal here: nothing blocks an unknown-cost Run");
            }
            exit::OK
        }
        Err(refusal) => refused("cost_inspect_version", &refusal, json),
    }
}

fn resolve(root: &Path, writer: &Writer, args: &ReconcileArgs) -> u8 {
    let (uid, name) = nika_cli_host::probe::operator_account();
    let resolution = match args.resolution {
        ResolutionArg::Billed => Resolution::Billed,
        ResolutionArg::NotBilled => Resolution::NotBilled,
        ResolutionArg::StillUnknown => Resolution::StillUnknown,
    };
    let evidence = match args.evidence {
        EvidenceArg::OperatorAttestation => Evidence::operator_attestation(args.reference.clone()),
    };
    let request = Request::new(
        args.project.clone(),
        args.invocation.clone(),
        args.prior.clone(),
        resolution,
        evidence,
        Principal::new(uid, name),
        std::time::SystemTime::now(),
    );
    match reconcile::submit(root, writer, &request) {
        Ok(receipt) => {
            if args.json {
                println!("{}", receipt.to_value());
            } else {
                println!("{}", render_receipt(&receipt));
            }
            exit::OK
        }
        Err(refusal) => refused("cost_reconcile_version", &refusal, args.json),
    }
}

/// A refusal: the request's own (`2`) or the environment's (`3`).
/// Preflight refusals preserve bytes; append I/O failures retain uncertainty.
fn refused(version: &str, refusal: &Refusal, json: bool) -> u8 {
    let code = if refusal.is_environment() {
        exit::ENV
    } else {
        exit::FILE
    };
    if json {
        let mut document = serde_json::json!({"refused": refusal.to_value()});
        document[version] = serde_json::json!(1);
        println!("{document}");
    } else if code == exit::ENV {
        eprintln!("nika: {refusal}");
    } else {
        println!("{refusal}");
    }
    code
}

fn render_inspection(seen: &Inspection) -> String {
    let mut out = vec![
        format!(
            "cost exposures · .nika/inference-cost-observations.ndjson · {} bytes · sha256 {}",
            seen.bytes, seen.sha256
        ),
        format!(
            "binding {} · host-local (this .nika directory), not an authenticated project identity",
            seen.project
        ),
    ];
    for (invocation, sha256) in &seen.derived {
        out.push(format!(
            "recorded unknown for Run {} (row sha256 {sha256}): the cost lease proves its writer gone",
            escape_tty(invocation)
        ));
    }
    for run in &seen.runs {
        out.extend(render_run(run, &seen.project));
    }
    for sha256 in &seen.torn {
        out.push(format!(
            "a row was cut mid-write (sha256 {sha256}): not reconcilable here, it keeps blocking"
        ));
    }
    for conflict in &seen.conflicts {
        out.push(format!(
            "a refused row for Run {} (sha256 {}) {}: not reconcilable here, it keeps blocking",
            escape_tty(&conflict.invocation),
            conflict.sha256,
            conflict.reason
        ));
    }
    out.push("evidence · operator attestation: it ends an exposure on your word; Nika verifies no billing, and a new unknown-cost Run still meets its own fresh review".to_owned());
    out.push(if seen.is_clear() {
        "clear · nothing blocks: a new unknown-cost Run still asks its own fresh question"
            .to_owned()
    } else {
        "clear · no: a new unknown-cost Run in this project waits".to_owned()
    });
    out.join("\n")
}

fn render_run(run: &RunReport, project: &str) -> Vec<String> {
    let invocation = escape_tty(&run.invocation);
    let state = match run.state {
        reconcile::RunState::Uncertain => {
            "uncertain: a sent request's charge is unknown".to_owned()
        }
        reconcile::RunState::Unknown => {
            "unknown: its writer let the cost lease go unsettled".to_owned()
        }
        reconcile::RunState::StillUnknown => "reconciled as still unknown".to_owned(),
        reconcile::RunState::Unjudged => "admitted and never settled".to_owned(),
        reconcile::RunState::Reconciled(resolution) => {
            format!("reconciled as {}", resolution.as_str().replace('_', " "))
        }
        _ => run.state.as_str().to_owned(),
    };
    let blocks = if run.state.blocks() {
        "blocks"
    } else {
        "clear"
    };
    let mut out = vec![
        String::new(),
        format!("Run {invocation} · {state} · {blocks}"),
    ];
    let facts = &run.facts;
    if facts["route"].is_object() {
        out.push(format!(
            "  route · {}/{} at {}",
            escape_tty(facts["route"]["provider"].as_str().unwrap_or("?")),
            escape_tty(facts["route"]["model"].as_str().unwrap_or("?")),
            escape_tty(facts["route"]["endpoint"].as_str().unwrap_or("?"))
        ));
    }
    let ids: Vec<String> = facts["provider_request_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_str().map(escape_tty))
        .collect();
    out.push(format!(
        "  provider request ids · {}",
        if ids.is_empty() {
            "none known".to_owned()
        } else {
            ids.join(", ")
        }
    ));
    let window = &facts["window"];
    out.push(format!(
        "  window · {} → {} (the execution ids' own times, not measured request times)",
        window["not_before"].as_str().unwrap_or("unknown"),
        window["not_after"].as_str().unwrap_or("unknown")
    ));
    if let Some(writer) = &run.writer {
        out.push(format!(
            "  writer · process {} on {}",
            writer["pid"],
            escape_tty(writer["host"].as_str().unwrap_or("?"))
        ));
    }
    if let Some(trace) = &run.trace {
        out.push(format!("  trace · .nika/traces/{}", escape_tty(trace)));
    }
    out.push(format!("  latest row · sha256 {}", run.head_sha256));
    match run.why_not {
        None => out.push(format!(
            "  resolve · nika trace cost reconcile {} --project {project} --prior {} --resolution billed|not-billed|still-unknown --reference \"<where you looked>\"",
            shell_quoted(&invocation),
            run.head_sha256
        )),
        Some(why) => out.push(format!("  not reconcilable here · {why}")),
    }
    out
}

/// One shell word, single-quoted: an earlier engine recorded invocations as
/// `ExecutionId { uuid: … }`, so the command to copy must survive spaces.
fn shell_quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

fn render_receipt(receipt: &Receipt) -> String {
    let claim = &receipt.event["reconciliation"];
    let resolution = claim["resolution"]
        .as_str()
        .unwrap_or("?")
        .replace('_', " ");
    let principal = &claim["principal"];
    let name = principal["name"]
        .as_str()
        .map_or_else(String::new, |name| format!(" ({})", escape_tty(name)));
    let outcome = if receipt.still_blocks {
        "still unknown: the Run keeps blocking"
    } else {
        "the exposure ended: a new unknown-cost Run still meets its own fresh review"
    };
    [
        format!(
            "reconciled Run {} as {resolution} · operator attestation, not verified by Nika",
            escape_tty(receipt.event["invocation"].as_str().unwrap_or("?"))
        ),
        format!(
            "  event · sha256 {} · names the Run's latest row {}",
            receipt.event_sha256,
            claim["prior_sha256"].as_str().unwrap_or("?")
        ),
        format!(
            "  principal · uid {}{name} · observed at {}",
            principal["uid"],
            claim["observed_at"].as_str().unwrap_or("?")
        ),
        format!(
            "  journal · {} → {} bytes · {}",
            receipt.before.0,
            receipt.after.0,
            if receipt.prefix_preserved {
                "every earlier byte kept"
            } else {
                "EARLIER BYTES CHANGED"
            }
        ),
        format!("  {outcome}"),
        format!("  clear · {}", if receipt.clear { "yes" } else { "no" }),
    ]
    .join("\n")
}
