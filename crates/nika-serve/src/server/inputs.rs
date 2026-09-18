// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Literal HTTP input binding. No CLI coercion, environment lookup or evaluation.

use super::error::ApiError;
use hyper::StatusCode;
use nika_types::types::{fits, parse_type};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(
    admitted: &nika_execution::AdmittedExecution,
    inputs: &BTreeMap<String, Value>,
) -> Result<(), ApiError> {
    let workflow = admitted.workflow();
    for (name, value) in inputs {
        let Some((_, declaration)) = workflow.inputs.iter().find(|(key, _)| key.value == *name)
        else {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unknown_input",
                "input key is not declared by this workflow",
            ));
        };
        if let nika_vocab::VarDecl::Typed { r#type, .. } = declaration {
            let ty = parse_type(&r#type.value, &BTreeSet::new(), "inputs").map_err(|_| {
                ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_input_type",
                    "declared input type could not be resolved",
                )
            })?;
            if !fits(value, &ty, &BTreeMap::new()) {
                return Err(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "input_type_mismatch",
                    "input JSON value does not conform to its declared type",
                ));
            }
        }
    }
    if nika_runtime::required_inputs_refusal(workflow, inputs).is_some() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "NIKA-1708",
            "required workflow inputs have no caller value or declared default",
        ));
    }
    Ok(())
}
