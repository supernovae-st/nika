// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A transport caller's input provenance, the sibling of the `--var` seam in
//! [`crate::inputs`] (descended from Serve's resident door, C6): the keys a
//! caller supplied carry the caller's origin, and each declared input left
//! unbound keeps what the runtime derives with no CLI channel (a default is
//! the file's). Nothing is inferred from the executing process: no person, CI
//! context or environment read.

use std::collections::{BTreeMap, BTreeSet};

use nika_runtime::InputOrigin;
use nika_schema::raw::RawWorkflow;
use serde_json::Value;

use crate::ServiceExecutionDriver;

impl ServiceExecutionDriver {
    /// The provenance of a transport caller's literal `inputs`, already
    /// checked against this workflow: each supplied key is `origin`, and each
    /// declared input left unbound keeps what [`nika_runtime::input_origins`]
    /// derives with no CLI channel (a default is the file's; an input with no
    /// default has no entry).
    #[must_use]
    pub fn caller_origins(
        &self,
        inputs: &BTreeMap<String, Value>,
        origin: InputOrigin,
    ) -> BTreeMap<String, InputOrigin> {
        caller_origins(&self.workflow, inputs, origin)
    }
}

/// The law behind [`ServiceExecutionDriver::caller_origins`].
pub(crate) fn caller_origins(
    workflow: &RawWorkflow,
    inputs: &BTreeMap<String, Value>,
    origin: InputOrigin,
) -> BTreeMap<String, InputOrigin> {
    let (none, no_env) = (BTreeMap::new(), BTreeSet::new());
    let mut origins = nika_runtime::input_origins(workflow, &none, &no_env, false);
    origins.extend(inputs.keys().map(|name| (name.clone(), origin)));
    origins
}
