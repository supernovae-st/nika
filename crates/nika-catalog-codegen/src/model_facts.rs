// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sourced model facts (`nika/llm-providers@1.2`): where a model row's
//! token limits were read, and the retirement floor its provider publishes.
//!
//! The facts ride on the `[[providers.models]]` rows that carry them and
//! are emitted beside `ALL_PROVIDERS` as one `MODEL_FACTS` table keyed by
//! provider id and wire id, so a row without facts keeps its exact shape.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::emit::{opt_rstr, rstr};
use crate::error::CodegenError;
use crate::pricing::is_iso_date;
use crate::providers::{LifecycleEntry, ProviderEntry, ProviderModelEntry};

/// One row's sourced facts, as the codegen compares and emits them.
#[derive(PartialEq, Eq)]
struct Facts<'a> {
    limits: Option<(&'a str, &'a str)>,
    lifecycle: Option<&'a LifecycleEntry>,
}

fn facts(m: &ProviderModelEntry) -> Facts<'_> {
    Facts {
        limits: m.limits_source.as_deref().zip(m.as_of.as_deref()),
        lifecycle: m.lifecycle.as_ref(),
    }
}

/// A row's facts are whole and sourced: `limits_source` and `as_of` arrive
/// together, every source is `https://`, every date `YYYY-MM-DD`, and a
/// lifecycle successor is another wire of the same provider. Two rows that
/// name one wire carry the same facts.
pub(crate) fn validate(p: &ProviderEntry, ctx: &str) -> Result<(), CodegenError> {
    let wires: Vec<&str> = p.models.iter().map(|m| m.model.as_str()).collect();
    let mut seen: HashMap<&str, Facts<'_>> = HashMap::new();
    for m in &p.models {
        let refuse = |reason: String| {
            CodegenError::schema_validation(
                ctx,
                format!("provider {:?}: model {:?}: {reason}", p.id, m.id),
            )
        };
        match (m.limits_source.as_deref(), m.as_of.as_deref()) {
            (None, None) => {}
            (Some(source), Some(as_of)) => {
                if !source.starts_with("https://") || !is_iso_date(as_of) {
                    return Err(refuse(
                        "limits_source must be https:// and as_of YYYY-MM-DD".into(),
                    ));
                }
            }
            _ => return Err(refuse("limits_source and as_of arrive together".into())),
        }
        if let Some(life) = &m.lifecycle {
            if !is_iso_date(&life.retires_not_before)
                || !is_iso_date(&life.as_of)
                || !life.source.starts_with("https://")
            {
                return Err(refuse(
                    "lifecycle dates must be YYYY-MM-DD and its source https://".into(),
                ));
            }
            if life.successor == m.model || !wires.contains(&life.successor.as_str()) {
                return Err(refuse(format!(
                    "lifecycle successor {:?} must be another wire of this provider",
                    life.successor
                )));
            }
        }
        let these = facts(m);
        match seen.get(m.model.as_str()) {
            Some(earlier) if !earlier.is_silent() && !these.is_silent() && *earlier != these => {
                return Err(refuse(format!(
                    "wire {:?} carries different facts on another row",
                    m.model
                )));
            }
            Some(earlier) if !earlier.is_silent() => {}
            _ => {
                seen.insert(m.model.as_str(), these);
            }
        }
    }
    Ok(())
}

impl Facts<'_> {
    fn is_silent(&self) -> bool {
        self.limits.is_none() && self.lifecycle.is_none()
    }
}

/// `MODEL_FACTS`: one entry per provider wire that carries any fact, in
/// catalogue order.
pub(crate) fn emit(out: &mut String, providers: &[ProviderEntry]) {
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "pub static MODEL_FACTS: &[crate::types::ModelFacts] = &["
    );
    for p in providers {
        let mut emitted: Vec<&str> = Vec::new();
        for m in &p.models {
            let f = facts(m);
            if f.is_silent() || emitted.contains(&m.model.as_str()) {
                continue;
            }
            emitted.push(m.model.as_str());
            let lifecycle = f.lifecycle.map_or_else(
                || "None".to_owned(),
                |life| {
                    format!(
                        "Some(crate::types::ModelLifecycle {{ retires_not_before: {}, \
                         successor: {}, source: {}, as_of: {} }})",
                        rstr(&life.retires_not_before),
                        rstr(&life.successor),
                        rstr(&life.source),
                        rstr(&life.as_of),
                    )
                },
            );
            let _ = writeln!(out, "    crate::types::ModelFacts {{");
            let _ = writeln!(out, "        provider: {},", rstr(&p.id));
            let _ = writeln!(out, "        model: {},", rstr(&m.model));
            let _ = writeln!(
                out,
                "        limits_source: {},",
                opt_rstr(f.limits.map(|(source, _)| source))
            );
            let _ = writeln!(
                out,
                "        limits_as_of: {},",
                opt_rstr(f.limits.map(|(_, as_of)| as_of))
            );
            let _ = writeln!(out, "        lifecycle: {lifecycle},");
            let _ = writeln!(out, "    }},");
        }
    }
    let _ = writeln!(out, "];");
}
