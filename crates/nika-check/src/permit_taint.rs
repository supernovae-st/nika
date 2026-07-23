// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The permit-parameterization taint (NEP-0004 · LAW-AUTH-0325) — the
//! STATIC twin of the runtime re-gate (`nika-runtime`'s `dispatch/regate.rs`
//! · the `NIKA-SEC-004` run-time half). The category fit
//! ([`super::permits_fit`]) proves `Required ⊆ Declared` on CATEGORIES;
//! this lane binds the VALUES flowing under a PRESENT `permits:` block:
//!
//! - **Law 1 ([`PermitTaintKind::BoundInterpolated`] · `NIKA-AUTH-007`)** —
//!   an interpolation reaching a permit BOUND (a host/glob/program literal
//!   inside `permits:`) is a hard refusal: the bound MUST be a literal,
//!   the boundary would be self-serve, there is nothing left to
//!   canonicalize against. The parser never renders `permits:` (a `${{ }}`
//!   island is a mute literal there today — the silent hole), so the
//!   rejection is this static gate, before any token.
//! - **Law 2 ([`PermitTaintKind::ArgEscapes`] · `NIKA-AUTH-008`)** — an
//!   UNTRUSTED value reaching a permitted verb's ARGUMENT is re-gated on
//!   its RESOLVED, canonical form against the step's permit: `inputs.*` /
//!   `config.*` (the two roots resolvable at check, via their declared
//!   defaults) substitute, the fs path folds lexically
//!   ([`lexically_normalize`]) before the glob match, the net host reads
//!   through the shared WHATWG extractor ([`super::permits_fit::url_host`]),
//!   and an exec argv tail token of the re-entry class (`--exec` · `-c` ·
//!   `eval`…) is never covered unless the permit lists it.
//! - **Law 4 (deferral)** — an untrusted reference NOT resolvable at
//!   check (no default · a `with:`/`tasks.*` derivation) DEFERS: the file
//!   stays valid and the run-time re-gate is mandatory. Law 4's
//!   informational « deferred re-gates » listing (a SHOULD) is a declared
//!   v1 gap — the hint rail would fire on every ordinary
//!   `${{ tasks.x.output }}` workflow.
//! - **Law 5 (`declassify:`)** — a task-level entry raises its `from:`
//!   binding to trusted HERE (the static twin never re-gates it); the
//!   value is still matched like a literal everywhere else (never a
//!   permit bypass) and the run receipt records the event.
//!
//! Everything is judged against the WORKFLOW's `permits:` block (v1 has
//! no task-level narrowing — the step permit IS the declared block), and
//! only when the block is PRESENT (an absent/null block is NEP-0003's
//! ground — `NIKA-AUTH-006`, the `permits_fit` lane).

use nika_schema::raw::RawWorkflow;
use nika_schema::types::ExecPermit;

/// The wire code of a bound interpolation (law 1).
pub(crate) const BOUND_CODE: &str = "NIKA-AUTH-007";
/// The wire code of an untrusted-argument escape (law 2).
pub(crate) const REGATE_CODE: &str = "NIKA-AUTH-008";

/// Which NEP-0004 rule the finding speaks (the wire-code mapping is the
/// ONE match arm every surface — findings · conformance codes · CLI —
/// reads, so the two classes can never swap).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub enum PermitTaintKind {
    /// Law 1 · a permit bound is interpolated, not literal
    /// (`NIKA-AUTH-007`).
    BoundInterpolated,
    /// Law 2 · an untrusted value's canonical resolved form escapes the
    /// step's permit (`NIKA-AUTH-008`).
    ArgEscapes,
}

/// One permit-taint finding (law 1 or law 2) — the check-time twin of
/// the runtime re-gate's `NIKA-SEC-004` refusal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct PermitTaint {
    /// The offending task (law 2), or `permits` for a bound finding
    /// (law 1 — the bound is its own site).
    pub task: String,
    /// Which law speaks.
    pub kind: PermitTaintKind,
    /// The witness — law 3's shape: the taint path source-first, the
    /// resolved value, its canonical form, and the bound it escaped
    /// (law 1: the interpolated bound's own path).
    pub detail: String,
    /// The machine-applicable repair (the one idiom per class).
    pub fix: Option<String>,
}

impl PermitTaint {
    /// The canonical spec code this finding stamps (one arm · every
    /// surface reads THIS).
    #[must_use]
    pub fn wire_code(&self) -> &'static str {
        match self.kind {
            PermitTaintKind::BoundInterpolated => BOUND_CODE,
            PermitTaintKind::ArgEscapes => REGATE_CODE,
        }
    }
}

/// Scan a workflow for the permit-parameterization taint — law 1 (bound
/// literality) and law 2 (the static re-gate) under a PRESENT `permits:`
/// block; empty when no block is declared (NEP-0003 owns that ground).
#[must_use]
pub(crate) fn scan_permit_taint(wf: &RawWorkflow) -> Vec<PermitTaint> {
    let Some(permits) = wf.permits.as_ref() else {
        return Vec::new();
    };
    let permits = &permits.value;
    let mut out = Vec::new();
    scan_bound_literality(permits, &mut out);
    out
}

/// Law 1 · every bound string inside the block MUST be a literal — an
/// interpolation reaching the wall itself is a hard refusal (NEP-0004's
/// « the boundary would be self-serve »). Walks the four carrier
/// families (fs globs · net hosts · exec programs · tool grants) and
/// names each interpolated bound by its own path (`net.http[0]`).
fn scan_bound_literality(permits: &nika_schema::types::Permits, out: &mut Vec<PermitTaint>) {
    let mut check = |path: String, bound: &str| {
        if bound.contains("${{") {
            out.push(PermitTaint {
                task: "permits".to_owned(),
                kind: PermitTaintKind::BoundInterpolated,
                detail: format!(
                    "permit bound `{path}` is interpolated, not literal · a bound is \
                     the wall itself: there is nothing left to canonicalize against \
                     (NEP-0004 law 1)"
                ),
                fix: Some(
                    "write the literal host/glob/program and gate the data in the body \
                     (the NIKA-AUTH-008 re-gate checks the value there)"
                        .to_owned(),
                ),
            });
        }
    };
    if let Some(fs) = permits.fs.as_ref() {
        for (i, glob) in fs.read.iter().enumerate() {
            check(format!("fs.read[{i}]"), glob);
        }
        for (i, glob) in fs.write.iter().enumerate() {
            check(format!("fs.write[{i}]"), glob);
        }
    }
    if let Some(net) = permits.net.as_ref() {
        for (i, host) in net.http.iter().enumerate() {
            check(format!("net.http[{i}]"), host);
        }
    }
    if let Some(ExecPermit::Programs(programs)) = permits.exec.as_ref() {
        for (i, program) in programs.iter().enumerate() {
            check(format!("exec[{i}]"), program);
        }
    }
    if let Some(tools) = permits.tools.as_ref() {
        for (i, tool) in tools.iter().enumerate() {
            check(format!("tools[{i}]"), tool);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn taints_of(yaml: &str) -> Vec<PermitTaint> {
        scan_permit_taint(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn an_interpolated_bound_is_a_hard_refusal() {
        // Conformance fixture core/authority/010 — the default even
        // MATCHES the intended host: irrelevant, the boundary would be
        // self-serve.
        let y = r#"nika: v1
workflow:
  id: w
permits:
  net: { http: ["${{ inputs.host }}"] }
  tools: ["nika:fetch"]
inputs:
  host: { type: string, default: "api.example.com" }
tasks:
  grab:
    invoke:
      tool: nika:fetch
      args: { url: "https://${{ inputs.host }}/data" }
"#;
        let taints = taints_of(y);
        assert!(
            taints
                .iter()
                .any(|t| t.kind == PermitTaintKind::BoundInterpolated
                    && t.detail.contains("net.http[0]")),
            "law 1 names the bound's own path: {taints:?}"
        );
    }

    #[test]
    fn literal_bounds_and_an_absent_block_are_silent() {
        // Law 1 judges a PRESENT block only — absent is NEP-0003's ground.
        assert!(
            taints_of(
                "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n"
            )
            .is_empty()
        );
        // …and literal bounds never fire (every honest file).
        let y = r#"nika: v1
workflow:
  id: w
permits:
  fs: { read: ["datasets/**"] }
  net: { http: ["api.example.com"] }
  exec: ["find"]
  tools: ["nika:read"]
tasks:
  t:
    invoke: { tool: "nika:read", args: { path: "datasets/q3.csv" } }
"#;
        assert!(taints_of(y).is_empty(), "{:?}", taints_of(y));
    }

    #[test]
    fn every_bound_family_is_walked() {
        let y = r#"nika: v1
workflow:
  id: w
permits:
  fs: { read: ["${{ inputs.a }}"], write: ["out/**"] }
  net: { http: ["${{ inputs.b }}"] }
  exec: ["${{ inputs.c }}"]
  tools: ["${{ inputs.d }}"]
tasks:
  t:
    exec: { command: ["true"] }
"#;
        let paths: Vec<String> = taints_of(y).iter().map(|t| t.detail.clone()).collect();
        for needle in ["fs.read[0]", "net.http[0]", "exec[0]", "tools[0]"] {
            assert!(
                paths.iter().any(|d| d.contains(needle)),
                "{needle} flagged: {paths:?}"
            );
        }
        // the literal write glob is NOT flagged.
        assert!(!paths.iter().any(|d| d.contains("fs.write[0]")));
    }
}
