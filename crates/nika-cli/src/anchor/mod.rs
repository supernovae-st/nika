// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trace anchor (S3 · the verifiable-run wave) — the whole trust
//! plane (Rekor v2 contract · RFC 3161 · sidecar · offline
//! verification · the submit composer · the SEALED/ANCHORED tier
//! evaluations) DESCENDED to the trace-forensics plane
//! (`nika_dap::anchor` · 2026-07-20 · the 15k wall: compute descends,
//! render stays). Re-exported at the old path so the `trace anchor`
//! verb and the verify ladder read unchanged.

pub use nika_dap::anchor::*;
