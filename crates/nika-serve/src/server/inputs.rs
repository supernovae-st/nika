// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Literal HTTP input binding. No CLI coercion, environment lookup or evaluation.

use super::error::ApiError;
use hyper::StatusCode;
use nika_execution::InputRefusal;
use serde_json::Value;
use std::collections::BTreeMap;

/// The admitted workflow's literal input law (Execution's) answered in this
/// door's words, then the runtime's required-input refusal.
pub(super) fn validate(
    admitted: &nika_execution::AdmittedExecution,
    inputs: &BTreeMap<String, Value>,
) -> Result<(), ApiError> {
    admitted.check_inputs(inputs).map_err(|refusal| {
        let (code, message) = match refusal {
            InputRefusal::Undeclared => (
                "unknown_input",
                "input key is not declared by this workflow",
            ),
            InputRefusal::UnresolvedType => (
                "invalid_input_type",
                "declared input type could not be resolved",
            ),
            _ => (
                "input_type_mismatch",
                "input JSON value does not conform to its declared type",
            ),
        };
        ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    })?;
    if nika_runtime::required_inputs_refusal(admitted.workflow(), inputs).is_some() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "NIKA-1708",
            "required workflow inputs have no caller value or declared default",
        ));
    }
    Ok(())
}
