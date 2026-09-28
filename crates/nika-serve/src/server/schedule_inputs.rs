// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resident binds a schedule's declared `inputs` (#1370) on every fire
//! through the literal law the CLI edge shares (`nika_service_execution::inputs`:
//! the declared key, then the declared type's coercion, `--var` semantics),
//! then judges them by the literal admission validator `POST /v1/jobs` uses.
//! One law, two doors. Judged at `PUT` so an operator learns before the first
//! slot, and again at fire because the workflow file may have changed in between.

use std::collections::BTreeMap;

use nika_cadence::ScheduleDefinition;
use nika_execution::AdmittedExecution;
use nika_service_execution::inputs::{coerce_literal, declaration};
use serde_json::Value;

use super::inputs;

/// The declared environment channel of the CLI edge (`--var key=@env:VAR`).
/// The resident never reads its own process environment on a schedule's
/// behalf: a server's environment is not the operator.
const ENV_PREFIX: &str = "@env:";

/// A schedule input the resident cannot bind, named for the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScheduleInputsRefusal {
    pub(super) code: &'static str,
    pub(super) message: String,
}

impl ScheduleInputsRefusal {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Bind a schedule's `inputs` against the admitted workflow: coerce every
/// text by the declared type, refuse an unknown key with the declared set,
/// refuse the `@env:` channel, then apply the literal admission validator
/// (type fit and `required:` law).
///
/// # Errors
/// The first refusal, with a stable code: `unknown_input`,
/// `env_channel_unsupported`, `input_type_mismatch`, `invalid_input_type`
/// or `NIKA-1708`.
pub(super) fn bind(
    admitted: &AdmittedExecution,
    definition: &ScheduleDefinition,
) -> Result<BTreeMap<String, Value>, ScheduleInputsRefusal> {
    let mut bound = BTreeMap::new();
    for (key, text) in definition.inputs() {
        let declared = declaration(admitted.workflow(), key).map_err(|undeclared| {
            let message = format!("inputs.{key}: unknown input — {}", undeclared.teaching());
            ScheduleInputsRefusal::new("unknown_input", message)
        })?;
        if text.starts_with(ENV_PREFIX) {
            return Err(ScheduleInputsRefusal::new(
                "env_channel_unsupported",
                format!(
                    "inputs.{key}: the resident binds literal values only — the `@env:` channel is the CLI edge's (`nika arm fire`)"
                ),
            ));
        }
        let value = coerce_literal(declared, text).map_err(|misfit| {
            let message = format!("inputs.{key}: {}", misfit.why);
            ScheduleInputsRefusal::new("input_type_mismatch", message)
        })?;
        bound.insert(key.to_owned(), value);
    }
    inputs::validate(admitted, &bound)
        .map_err(|error| ScheduleInputsRefusal::new(error.code(), error.message()))?;
    Ok(bound)
}
