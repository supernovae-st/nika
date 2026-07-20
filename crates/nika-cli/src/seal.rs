// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trust plane's home is the trace-forensics crate — the run-key
//! custody + seal minting (`nika_dap::seal`) and the workflow
//! author-binding compute (`nika_dap::sign`) DESCENDED there
//! 2026-07-21 (the 15k wall: compute descends, render stays).
//! Re-exported at the old `crate::seal::…` paths so the key verb, the
//! sign verb and the run sink/gate read unchanged.

pub use nika_dap::seal::*;
pub use nika_dap::sign::*;
