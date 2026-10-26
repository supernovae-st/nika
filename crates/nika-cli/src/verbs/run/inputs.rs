// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--var KEY=VALUE` input seam, in `run`'s voice: the parse and the
//! origin law live in `nika_cli_host::var_inputs` (one door for `run` and
//! the golden test's cases); this file keeps the env-refusal wrap.

pub(crate) use nika_cli_host::var_inputs::{ValidatedInputs, parse_var_overrides};

use nika_schema::raw::RawWorkflow;
use nika_service_execution::inputs::{BindingFault, check_bindings};

use super::epilogue;

/// [`parse_var_overrides`] + the admission preflight (#603 · the runtime's
/// ONE constructor) — both ENV-class refusals (stderr + the envelope).
pub(super) fn validated_var_overrides(
    vars: &[String],
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<ValidatedInputs, u8> {
    let validated = parse_var_overrides(vars, wf)
        .map_err(|message| var_refusal(vars, wf, &message, output_json))?;
    if let Some(err) = nika_runtime::required_inputs_refusal(wf, &validated.values) {
        return Err(epilogue::env_refusal(&err.to_string(), output_json));
    }
    Ok(validated)
}

/// A `--var` refusal with the typed code the other input doors already use
/// (`--inputs-json`, the server): the same per-pair law judges each pair
/// alone, and the first faulted one names the code. The sentence and the
/// exit stay the bind's own.
fn var_refusal(vars: &[String], wf: &RawWorkflow, message: &str, machine: bool) -> u8 {
    let fault = check_bindings(vars, wf)
        .into_iter()
        .find_map(|check| check.fault);
    match fault {
        Some(fault) => epilogue::typed_env_refusal(var_fault_code(fault), message, machine),
        None => epilogue::env_refusal(message, machine),
    }
}

/// The wire slug of a `--var` fault: the shared names for an undeclared key
/// and a value its type refuses, the fault's own slug otherwise.
fn var_fault_code(fault: BindingFault) -> &'static str {
    match fault {
        BindingFault::UnknownInput => "unknown_input",
        BindingFault::TypeMismatch => "input_type_mismatch",
        other => other.as_str(),
    }
}

/// The input channel is selected once; resumed legs retain the same literal map.
#[derive(Clone, Copy)]
pub(crate) enum InputBindings<'a> {
    Operator(&'a [String]),
    Literal(&'a std::collections::BTreeMap<String, serde_json::Value>),
    /// A `nika try` rehearsal (#1633): the run's `--var` pairs, then the
    /// try verb's own re-invocation flags. Its staged room is discarded
    /// when the verb returns, so a taught line re-invokes `nika try` with
    /// the operator's flags, never `nika run` on the room's file.
    Rehearsal(&'a [String], &'a str),
}

impl InputBindings<'_> {
    pub(super) fn validate(self, wf: &RawWorkflow, machine: bool) -> Result<ValidatedInputs, u8> {
        match self {
            Self::Operator(vars) | Self::Rehearsal(vars, _) => {
                validated_var_overrides(vars, wf, machine)
            }
            Self::Literal(values) => super::literal_inputs::validate(values, wf)
                .map_err(|error| super::literal_inputs::refuse(&error, machine)),
        }
    }

    pub(super) fn resume_carry(self, model: Option<&str>) -> String {
        match self {
            Self::Operator(vars) => epilogue::resume_carry(vars, model),
            // Never place the payload in argv or reinterpret it as --var.
            // A separate resume invocation must resend the object on stdin.
            Self::Literal(_) => format!("{} --inputs-json -", epilogue::resume_carry(&[], model)),
            // The try verb's flags as the operator typed them (`--model self`
            // stays `self`): the resolved seat would read as the mock default.
            Self::Rehearsal(_, carry) => carry.to_owned(),
        }
    }
}
