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
//! the fits predicate was private in `nika_schema::check::permits_fit`) so the
//! vocabulary lives in one dependency-light leaf that a future `nika-policy`
//! (or any capability-aware crate) can consume without pulling in the parser.

mod algebra;
mod effect;
mod fit;
mod permits;
mod policy;
mod shape;
mod trifecta;

// Public surface = the 4 capability types + their inherent methods (allows_* ·
// union · intersect · new). The glob/normalize helpers stay crate-internal:
// nothing outside consumes them yet, and an L0 API is frozen forever — widening
// pub(crate)→pub later is additive, narrowing pub→pub(crate) would break. So the
// surface starts minimal (Gate-11 rust-pro + spn-nika L1).
// The fine-grained builtin effect table (boundary checking · inference) —
// extracted from nika-schema's permits_fit under the same 15k pressure as
// shape.rs; the coarse policy table (EffectClass) lives beside it.
pub use effect::{BuiltinEffect, builtin_effect, chart_vl_sibling};
pub use permits::{ExecPermit, FsPermits, NetPermits, Permits};
// W4 « the authority » (spec 10) — the policy: vocabulary (closed at the
// type level) + the pure judge + the certificate's authority projection.
pub use policy::{
    Allow, CertEffects, EffectClass, Forbid, HUMAN_GATE_TOOL, Limits, Objective, POLICY_KEYS,
    Policy, PolicySubject, PolicyViolation, Prefer, ProviderPin, Require, policy_child_keys,
    policy_violations,
};
pub use shape::builtin_shape_findings;
// NEP-0002 · the lethal-trifecta judge (`NIKA-SEC-009`) — the pure
// leg-conjunction + path-dominance logic; the projection lives in
// `nika-schema::check::trifecta` (the PolicySubject / policy_violations split).
pub use trifecta::{TrifectaSubject, TrifectaViolation, trifecta_violations};
