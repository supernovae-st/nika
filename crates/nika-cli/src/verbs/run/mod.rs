// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika run <file>` — execute a CHECKED workflow through the real L3
//! runtime (spec §3 · exit 0 ok · 1 workflow failed · 2 file findings).
//!
//! The composer: this module is the L4 half that the runtime's seams
//! (`Stamper` · `EventSink`) and the verb crates' generics expect. It
//! wires PRODUCTION effects (real fs · http · clock · subprocess ·
//! provider registry with env-resolved keys) and the display fold.
//!
//! ```text
//! parse (Strict) → check → dirty? → render findings · exit 2
//!                        → clean  → runtime.run(prod seams) →
//!                  outcome.ok → exit 0 | exit 1 (the failure card)
//! ```

mod compose;
mod sink;
mod stamp;

pub use compose::{ProdRuntime, production_runtime};
pub use sink::{FoldSink, JsonSink};
pub use stamp::SystemStamper;
