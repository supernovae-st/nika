// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness access class client (D-2026-08-04-N1 · P3 · Lane A).
//!
//! A hand-rolled ACP **wire v1** JSON-lines client — the Client role's
//! narrow waist (`initialize` · `session/new` · `session/prompt` ·
//! `session/update` · `session/request_permission` · `session/cancel`)
//! implementing the kernel's `AgentBackend` seam. The official SDK is
//! NEVER linked here (its `serde_json/preserve_order` requirement
//! would flip the engine's byte-attested surfaces — spec §2bis): it
//! judges this client from the quarantined conformance workspace
//! (`crates/nika-acp`) over a process boundary instead.
//!
//! B3.1 ships the transport-generic driver ([`client::drive`]) proven
//! against scripted duplex dialogues; B3.2 adds the confined spawn
//! (kill-on-drop · version handshake · NO env scrub of the harness's
//! own auth store · A-3) and the `AgentBackend` impl over it.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod client;
pub mod declaration;
pub mod infer;
pub mod probe;
pub mod registry;
pub mod spawn;
pub mod wire;

pub use client::{IDLE_TIMEOUT_SECS, MAX_LINE_BYTES, drive, drive_with_idle};
pub use declaration::{
    declared_adapter_id, seat_from_env, seat_from_id, seat_from_lookup, seat_from_pin,
    seat_http_err,
};
pub use infer::{
    HarnessInferOutcome, HarnessInferRequest, InferGradeAttestation, InferGradeError,
    InferGradeSeat, StructuredOutputGrade, meet_infer_grade,
};
pub use probe::{
    AdapterProbeRow, PresenceFact, VersionPin, judge_version, parse_version, presence_facts,
    probe_adapters, probe_adapters_sync,
};
pub use registry::{AdapterRow, AuthProbe, DISABLE_ENV, registry, registry_with};
pub use spawn::{HarnessAdapter, SpawnedHarness, compose_env};
