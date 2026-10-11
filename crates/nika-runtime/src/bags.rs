// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's value authorities, threaded whole: every scope a lane reads a value in is built
//! from them.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::expr::Scope;
use crate::record::TaskRecord;

/// The three read-only value namespaces every lane threads whole (`inputs` · `const` ·
/// `secrets`: the value authorities are exactly three since `config:` died).
#[derive(Clone, Copy)]
pub(crate) struct ValueBags<'a> {
    pub(crate) inputs: &'a BTreeMap<String, Value>,
    pub(crate) consts: &'a BTreeMap<String, Value>,
    pub(crate) secrets: &'a BTreeMap<String, Value>,
}

impl<'a> ValueBags<'a> {
    /// The bags `scope` reads (a cleanup renders with its parent's).
    pub(crate) const fn of(scope: &Scope<'a>) -> Self {
        Self {
            inputs: scope.inputs(),
            consts: scope.consts(),
            secrets: scope.secrets(),
        }
    }

    /// The workflow-level scope over `records` that reads these bags.
    pub(crate) fn scope(self, records: &'a BTreeMap<String, TaskRecord>) -> Scope<'a> {
        Scope::workflow_with_value_authorities(records, self.inputs, self.consts, self.secrets)
    }
}
