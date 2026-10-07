// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a door that holds no project (Serve) shares with `nika compile`, and what the caller's
//! engine prints for it: the `nika compile --observe-only` document (the host observation, and
//! the text of the files it read for a trial), the trial project and room built from it, and
//! the decision model it seats. The laws a door admits them by are `crate::compile::remote`.

use serde_json::{Value, json};

pub mod decision;
pub mod trial;

/// The version of the `nika compile --observe-only` document. Only a breaking change bumps it.
pub const OBSERVATION_VERSION: u32 = 1;

/// The `nika compile --observe-only` document: the host observation of what `intent` states
/// (`world`, null when nothing is stated) and the text of the files it read under `root`.
#[must_use]
pub fn document(root: &std::path::Path, intent: &str, world: Option<&Value>) -> Value {
    let trial = world.and_then(|world| trial::inputs(root, world));
    json!({
        "observation_version": OBSERVATION_VERSION,
        "intent_sha256": crate::compile::intent_sha256(intent),
        "observed_world": world,
        "trial_inputs": trial,
    })
}
