// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Online verifier for `nika-catalog`.
//!
//! Probes every catalog entry against its upstream source of truth
//! (npm registry, `PyPI`, OCI registries, remote MCP endpoints) and
//! reports drift. Non-zero exit code on `NotFound`; network errors
//! are reported but do not fail the run.
//!
//! Usage:
//!   * `nika-catalog-verify` — full scan, human output
//!   * `nika-catalog-verify --json` — full scan, JSON to stdout
//!   * `nika-catalog-verify --concurrency 32` — bump parallelism (default 16)
//!
//! Layer: L4 tooling binary, not part of the L0 `nika-catalog` lib.
//! Workspace clippy rules designed for runtime library src/ are relaxed
//! here with explicit rationale: `println!` is the CLI output contract,
//! `.expect()` on the semaphore is provably infallible, and test-mod
//! assertions naturally use `unwrap()`.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::expect_used,
    clippy::struct_field_names
)]

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use futures::stream::{FuturesUnordered, StreamExt};
use nika_catalog::{all_embeddings, all_mcp_servers, McpServer, RegistryType};
use reqwest::Client;
use tokio::sync::Semaphore;

mod probe;
mod report;

use probe::{build_client, probe_cargo, probe_npm, probe_oci, probe_pypi, probe_remote};
use report::{Finding, FindingResult, Report};

#[derive(Parser, Debug)]
#[command(name = "nika-catalog-verify", version, about)]
struct Cli {
    /// Emit machine-readable JSON on stdout instead of a human summary.
    #[arg(long)]
    json: bool,
    /// Maximum number of probes in flight at any one time.
    #[arg(long, default_value_t = 16)]
    concurrency: usize,
    /// Skip network probes — exercise the report pipeline in CI without
    /// hitting the open internet.
    #[arg(long)]
    offline: bool,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .compact()
        .init();

    let cli = Cli::parse();

    let findings = if cli.offline {
        collect_offline_findings()
    } else {
        match collect_online_findings(cli.concurrency).await {
            Ok(f) => f,
            Err(e) => {
                eprintln!("fatal: could not build reqwest client: {e}");
                return ExitCode::from(2);
            }
        }
    };

    let report = Report::build(findings);

    if cli.json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("fatal: could not serialise report: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        report.print_human();
    }

    if report.should_fail() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Collect findings using `FindingResult::Ok` for everything — useful
/// for CI pipelines that want to exercise the report path without
/// actually hitting the network.
fn collect_offline_findings() -> Vec<Finding> {
    let mut out = Vec::new();
    for s in all_mcp_servers() {
        for target in targets_for_server(s) {
            out.push(Finding {
                entry: s.id.to_string(),
                kind: target.kind,
                target: target.target,
                result: FindingResult::Ok,
            });
        }
    }
    for e in all_embeddings() {
        out.push(Finding {
            entry: e.id.to_string(),
            kind: "embedding",
            target: format!("{}/{}", e.provider, e.model),
            result: FindingResult::Ok,
        });
    }
    out
}

/// Collect findings by actually probing every upstream.
async fn collect_online_findings(concurrency: usize) -> Result<Vec<Finding>, reqwest::Error> {
    let client = Arc::new(build_client()?);
    let sem = Arc::new(Semaphore::new(concurrency.max(1)));

    let mut tasks = FuturesUnordered::new();
    for s in all_mcp_servers() {
        for target in targets_for_server(s) {
            let client = client.clone();
            let sem = sem.clone();
            let entry = s.id.to_string();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.expect("semaphore never closed");
                let result = run_target(&client, &target).await;
                Finding {
                    entry,
                    kind: target.kind,
                    target: target.target,
                    result: result.into(),
                }
            }));
        }
    }

    let mut out = Vec::new();
    while let Some(join) = tasks.next().await {
        match join {
            Ok(f) => out.push(f),
            Err(e) => eprintln!("task panicked: {e}"),
        }
    }
    Ok(out)
}

/// Anatomy of one upstream probe. `kind` classifies it for the report,
/// `target` is the raw identifier sent to the probe function.
struct Target {
    kind: &'static str,
    target: String,
    variant: TargetKind,
}

enum TargetKind {
    Npm,
    Pypi,
    Cargo,
    Oci,
    Remote,
}

async fn run_target(client: &Client, t: &Target) -> Result<(), probe::ProbeError> {
    match t.variant {
        TargetKind::Npm => probe_npm(client, &t.target).await,
        TargetKind::Pypi => probe_pypi(client, &t.target).await,
        TargetKind::Cargo => probe_cargo(client, &t.target).await,
        TargetKind::Oci => probe_oci(client, &t.target).await,
        TargetKind::Remote => probe_remote(client, &t.target).await,
    }
}

fn targets_for_server(s: &McpServer) -> Vec<Target> {
    let mut v = Vec::new();
    for pkg in s.packages {
        let (kind, variant) = match pkg.registry_type {
            RegistryType::Npm => ("npm", TargetKind::Npm),
            RegistryType::Pypi => ("pypi", TargetKind::Pypi),
            RegistryType::Oci => ("oci", TargetKind::Oci),
            RegistryType::Cargo => ("cargo", TargetKind::Cargo),
            // mcpb has no single online registry — skip for now.
            _ => continue,
        };
        v.push(Target { kind, target: pkg.identifier.to_string(), variant });
    }
    for r in s.remotes {
        v.push(Target {
            kind: "remote",
            target: r.url.to_string(),
            variant: TargetKind::Remote,
        });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_mode_covers_every_server_and_embedding() {
        // Each MCP server contributes at least one target (packages +
        // remotes), plus every embedding contributes one.
        let findings = collect_offline_findings();
        assert!(findings.len() >= all_mcp_servers().len() + all_embeddings().len());
        assert!(findings.iter().all(|f| matches!(f.result, FindingResult::Ok)));
    }

    #[test]
    fn offline_report_should_not_fail() {
        let findings = collect_offline_findings();
        let report = Report::build(findings);
        assert!(!report.should_fail());
    }
}
