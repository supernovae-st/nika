// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-cap` — the declared capability boundary as pure data.
//!
//! The canonical home for the `permits:` block (spec `01-envelope.md`
//! §permits): a workflow's entire declared blast radius — filesystem, network,
//! shell, tools — as value types, plus the pure **fits** predicate over them
//! (`allows_exec` / `allows_program` / `allows_tool` / `allows_host` /
//! `allows_path`) and the **set-algebra** lattice (`union` / `intersect`).
//!
//! Layer **L0** — zero I/O, zero async. The static, lexical half of the
//! capability model: `nika check` reads it to flag escapes before a run; the
//! runtime (`nika-builtin`, `nika-http`) enforces the I/O-resolved half
//! (symlink + `..` canonicalization) separately. Both agree because the host
//! matcher is shared (`nika_types::net::host_glob_matches`).
//!
//! Extracted from `nika-schema` (the types were `nika_schema::types::permits`;
//! the fits predicate was private in its check module — `nika_check::permits_fit`
//! since the 2026-07-21 judgment split) so the
//! vocabulary lives in one dependency-light leaf that a future `nika-policy`
//! (or any capability-aware crate) can consume without pulling in the parser.

mod algebra;
mod effect;
mod effect_vocab;
pub mod env;
mod expr;
mod fit;
mod harness_gate;
mod integrity;
mod permits;
mod shape;
pub mod sink;
mod trifecta;
mod witness;

// Public surface = the 4 capability types + their inherent methods (allows_* ·
// union · intersect · new) + the ONE lexical canonicalization (F-O1 PR-2 · the
// runtime re-gate's refusal messages print it). The glob matcher stays
// crate-internal: an L0 API is frozen forever — widening pub(crate)→pub later
// is additive, narrowing pub→pub(crate) would break. So the surface starts
// minimal (Gate-11 rust-pro + spn-nika L1).
// The fine-grained builtin effect table (boundary checking · inference) —
// extracted from nika-schema's permits_fit under the same 15k pressure as
// shape.rs; the coarse policy table (EffectClass) lives beside it.
pub use effect::{
    BuiltinEffect, PURE_INTERNAL_TOOLS, builtin_effect, builtin_egresses, chart_vl_sibling,
    glob_walk_root, is_pure_internal, is_pure_internal_call,
};
pub use env::{
    DANGEROUS_ENV_VARS, RUNNER_FLOOR_ENV_VARS, compose_child_env, is_dangerous_env_name,
};
// D-2026-08-11-N26 · the expression boundary — the natives every jq seam
// withholds, so « an expression sees only its input » is a property of the
// function set the compiler receives rather than a sentence in a document.
// NOT a permit route: `env` above governs a CHILD PROCESS, never an expression.
pub use expr::{
    WITHHELD_JQ_NATIVES, WithheldNative, is_withheld_jq_native, withheld_jq_native,
    withheld_jq_reason,
};
pub use fit::{glob_admits, lexically_normalize};
// P3 B5 · the harness permission-bridge judge (the pure half — the
// wire facts' translation into the declared boundary's verdict).
pub use harness_gate::{HarnessAskFacts, HarnessGate, judge_harness_ask};
pub use sink::code_bearing_path_class;
// F-O1 PR-1 · the runtime integrity label (the Integ axis of RS-06's
// trifecta Value) + the shared untrusted-ingress source predicates
// (check≡run by construction).
pub use integrity::{Integrity, invoke_tool_is_ingress, tool_grant_admits_ingress};
pub use permits::{ExecPermit, FsPermits, NetPermits, Permits, glob_matches};
// W4 « the authority » (spec 10) — the effect vocabulary. The `policy:`
// block and its judge died with the 9-key envelope (2026-08-13); the
// three words below outlived it because the trifecta, the consent law,
// the approval tickets and the certificate all read them without ever
// reading a declaration.
pub use effect_vocab::{CertEffects, EffectClass, HUMAN_GATE_TOOL};
pub use shape::builtin_shape_findings;
// NEP-0002 · the lethal-trifecta judge (`NIKA-SEC-009`) — the pure
// leg-conjunction + path-dominance logic; the projection lives in
// `nika-schema::check::trifecta` (the PolicySubject / policy_violations split).
pub use trifecta::{
    TaintWitness, TrifectaMitigation, TrifectaSubject, TrifectaVerdict, TrifectaViolation,
    trifecta_verdict,
};
// NEP-0007 · the permit-decision witness (descended from nika-runtime at
// the 15k wall · P3 B5 — the decision over a Permits boundary is
// capability-boundary data).
pub use witness::{PermitDecision, PermitWitness};
