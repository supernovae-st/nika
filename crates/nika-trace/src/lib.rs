// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-trace` — the flight-recorder reader: the trace plane of the
//! `nika-cli` unit (L4 · size-cap split per D-2026-07-09-N1 · the ADR-110
//! cli-host precedent), descended 2026-08-11 at the 15k prod-LOC wall.
//!
//! Everything here READS `.nika/traces/` — the NDJSON journals a run
//! records: `trace show|replay|outputs|peek|flow` (the fold's render),
//! `trace ls|rm` (the store's management), `trace verify` (tamper-evidence
//! chain · signature · anchor tiers), `trace anchor` (Rekor · RFC 3161
//! notary), `trace reproduce`, `trace export` (`OTel`), the `evidence` pack,
//! the `receipt` explainer, and the learned-truth `forecast` behind
//! `explain --forecast`. The compute (chain walk · anchor wire · recover ·
//! store scan) stays in `nika-dap` (the 2026-07-09 W0 descent); this
//! member is the render/routing half. `nika-cli` re-exports every public
//! item at its historical `verbs::` path, so call sites and the bin
//! dispatch read unchanged — one architectural unit, two members.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)
)]

// The root aliases `nika-cli`'s own lib carries (the nika-cli-host
// pattern), so the descended files keep their `crate::display::…` /
// `crate::demo` / `crate::anchor::…` / `crate::seal::…` paths byte-true.
pub(crate) use display::demo;
pub(crate) use display::render::{frame, frame_with_outputs};
pub(crate) use display::state::{RunView, TaskRow, TaskState};
pub use nika_cli_host::output::VerbOutput;
pub(crate) use nika_cli_host::output::{exit, linked_path};
pub(crate) use nika_cli_host::text;
pub(crate) use nika_dap::{anchor, seal};
pub use nika_display as display;

pub mod dispatch;
pub mod evidence;
pub mod forecast;
pub mod receipt;
pub mod trace;
pub mod trace_anchor;
pub mod trace_otel;
pub mod trace_reproduce;
pub mod trace_verify;

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode};

/// Read + strict-parse + ladder-check one workflow file — the
/// `trace flow` seam's twin of `nika-cli`'s `verbs::load_checked` (the
/// report rides along: `flow` discards it — the render judges edges,
/// not findings — but the judged-semantic stamp keeps the twin
/// byte-faithful to the seam every static verb shares). The Unix dash
/// (`-`) reads stdin.
///
/// Failure mapping per spec §4: unreadable = environment (`3`) · parse
/// error = a finding in the FILE (`2`).
pub(crate) fn load_checked(path: &str) -> Result<(RawWorkflow, CheckReport), VerbOutput> {
    let yaml = if path == "-" {
        use std::io::Read as _;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| VerbOutput::env(format!("cannot read stdin: {e}")))?;
        buf
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?
    };
    let wf = nika_schema::parse(&yaml, FileId::new(0), ParseMode::Strict)
        .map_err(|e| VerbOutput::file(format!("PARSE ✗  [{}] {e}", e.spec_code())))?;
    // The composed lane (spec 14): child targets resolve against the
    // file the operator named.
    let mut report = nika_check::check_composed(&wf, path, &mut |p| {
        std::fs::read_to_string(p).map_err(|e| e.to_string())
    });
    // The judged-vs-booted binding (F-P2), stamped even though `flow`
    // discards the report: the twin stays byte-faithful to the cli seam.
    report.workflow_semantic =
        nika_runtime::proof::ir::semantic_ir_hash(&wf).map(|h| h.as_hex().to_owned());
    Ok((wf, report))
}
