// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Finite Run inputs come from the checked declaration, never its question text.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use nika_schema::raw::RawWorkflow;
use nika_schema::types::VarDecl;
use nika_types::types::{NikaType, Primitive, parse_type};
use serde_json::Value;

use super::{Refusal, RefusalClass, RunInputs, SessionRuntime, Witness};

// A process-local request namespace. Repainting preserves an admission; cancel
// followed by an otherwise identical request must never revive its old control.
static NEXT_ADMISSION: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_admission() -> u64 {
    NEXT_ADMISSION.fetch_add(1, Ordering::Relaxed)
}

impl SessionRuntime {
    /// The active required bool or string-enum input, its exact typed values and
    /// the identity of this admission and its waiting Run request. Only the
    /// still-checked source bytes supply the declaration. A changed or unavailable
    /// source yields None.
    /// This read-only presentation grants no input, consent or Run authority;
    /// consumers compare the whole tuple again before the existing submit door.
    #[must_use]
    pub fn waiting_input_choices(&self) -> Option<(String, Vec<Value>, String)> {
        let inputs = self.run_inputs.as_ref()?;
        let name = inputs.first_needed()?;
        if self.waiting()
            != (crate::work::Waiting::Input {
                name: name.to_owned(),
            })
        {
            return None;
        }
        let workflow = inputs.checked_workflow(&self.snapshot.root)?;
        let values = values(&workflow, name)?;
        let identity = Witness::of(
            format!("waiting-input 1 admission {}\n{inputs:?}", inputs.admission).as_bytes(),
        )
        .0;
        Some((name.to_owned(), values, identity))
    }
}

impl RunInputs {
    fn checked_workflow(&self, root: &Path) -> Option<RawWorkflow> {
        let source = std::fs::read_to_string(root.join(&self.workflow)).ok()?;
        if self.checked.0.as_ref()? != &Witness::of(source.as_bytes()) {
            return None;
        }
        nika_schema::parse(
            &source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .ok()
    }

    /// The canonical launch parser reads enum strings as JSON before fitting the
    /// declaration. Retain quotes so "true", "1" and "@env:NAME" remain strings.
    pub(super) fn literal(&self, root: &Path, text: &str) -> Result<String, Refusal> {
        let choices = self
            .checked_workflow(root)
            .and_then(|wf| values(&wf, self.first_needed()?));
        let Some(choices) = choices else {
            return Ok(serde_json::from_str::<String>(text).unwrap_or_else(|_| text.to_owned()));
        };
        let value =
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_owned()));
        if !choices.contains(&value) {
            return Err(Refusal::new(
                RefusalClass::WrongState,
                "the value does not match this input's declared choices — the input still waits",
            ));
        }
        Ok(value.to_string())
    }
}

fn values(workflow: &RawWorkflow, name: &str) -> Option<Vec<Value>> {
    let (
        _,
        VarDecl::Typed {
            r#type,
            required: true,
            default: None,
            ..
        },
    ) = workflow.inputs.iter().find(|(key, _)| key.value == name)?
    else {
        return None;
    };
    match parse_type(&r#type.value, &BTreeSet::new(), "inputs").ok()? {
        NikaType::Prim(Primitive::Bool) => Some(vec![Value::Bool(true), Value::Bool(false)]),
        NikaType::Enum(values) => Some(values.into_iter().map(Value::String).collect()),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
