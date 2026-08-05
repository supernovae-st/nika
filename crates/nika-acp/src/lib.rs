// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness access class (D-2026-08-04-N1 · P3) — WIP skeleton.
//!
//! `agent:` tasks on the user's OWN authenticated harness, driven
//! through the official Agent Client Protocol (Client role). The
//! harness owns auth; nika never holds a credential (A-3). Spec:
//! `docs/crate-specs/nika-acp.md` (Gate 1). Admission is the P3.2
//! ceremony — this skeleton exists so the MOCK AGENT (the
//! load-bearing instrument · spec §5) lands tests-first.

#![forbid(unsafe_code)]
