// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Law 25: date-times with offsets are compared as instants, never as text. Text order is time
//! order only between date-times that share one offset and one form: `2026-09-01T02:30:00+02:00`
//! (00:30 UTC) sorts after the upper bound `2026-09-01T01:00:00` of the hour it lies inside
//! (a first-hour total was 0 instead of 8), and a boundary row
//! written `+00:00` sorts before a `Z` bound of the same instant, so `Z` and `+00:00` are two
//! offsets. An order on one record (the scope of `record_scope`) is a finding when the host saw
//! the field's values in several offsets or forms, or when a string bound is a date-time in
//! another offset or form. The evidence is the host's: the instants it recorded in the kinds
//! entry of the path, else the categorical values of the observed row when every one is a
//! date-time. A date-only bound, a bound that is no date-time and a converted field
//! (`.at | fromdateiso8601`) are never judged.

use super::Diagnostic;
use super::record_scope::{Records, listed};
use nika_compile_behavior::instant_shape;
use std::collections::BTreeSet;

/// An order over a record field, as the walk found it.
pub(super) struct Order<'a> {
    /// The field the order reads on one record.
    pub(super) field: &'a str,
    /// `<` · `<=` · `>` · `>=` · `sort_by` · `min_by` · `max_by`.
    pub(super) op: &'static str,
    /// The other operand, when it is a string literal.
    pub(super) bound: Option<&'a str>,
    /// The observed records the field belongs to.
    pub(super) records: Records<'a>,
}

/// The offsets and forms the host observed for one field.
struct Shapes {
    offsets: BTreeSet<String>,
    forms: BTreeSet<String>,
}

/// The shapes of `field` in the records' evidence: the instants the host recorded in the kinds
/// entry (`{"instants": {field: {"offsets": […], "forms": […]}}}`), else the categorical values
/// of the observed row when every one is a date-time.
fn observed(records: &Records<'_>, field: &str) -> Option<Shapes> {
    let recorded = records
        .kinds
        .and_then(|kinds| kinds.get("instants"))
        .and_then(|instants| instants.get(field));
    let shapes = if let Some(entry) = recorded {
        let set = |key: &str| -> Option<BTreeSet<String>> {
            entry
                .get(key)?
                .as_array()?
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect()
        };
        Shapes {
            offsets: set("offsets")?,
            forms: set("forms")?,
        }
    } else {
        let mut shapes = Shapes {
            offsets: BTreeSet::new(),
            forms: BTreeSet::new(),
        };
        for value in records.row.get("values")?.get(field)?.as_array()? {
            let (form, offset) = instant_shape(value.as_str()?)?;
            shapes.forms.insert(form);
            shapes.offsets.insert(offset);
        }
        shapes
    };
    (!shapes.offsets.is_empty() && !shapes.forms.is_empty()).then_some(shapes)
}

/// Why text order is not time order for values of these shapes against `bound`, if it is not.
fn discord(shapes: &Shapes, bound: Option<&str>, path: &str) -> Option<String> {
    let shown = |offset: &str| {
        if offset.is_empty() {
            "(no offset)".to_owned()
        } else {
            offset.to_owned()
        }
    };
    if shapes.offsets.len() > 1 {
        let offsets: Vec<String> = shapes.offsets.iter().map(|o| shown(o)).collect();
        let offsets: Vec<&str> = offsets.iter().map(String::as_str).collect();
        let offsets = listed(&offsets, "", "");
        return Some(format!(
            "its values observed in `{path}` carry the offsets {offsets}"
        ));
    }
    let forms: Vec<&str> = shapes.forms.iter().map(String::as_str).collect();
    if forms.len() > 1 {
        let forms = listed(&forms, "", "");
        return Some(format!(
            "its values observed in `{path}` are written in the forms {forms}"
        ));
    }
    let bound = bound?;
    let (form, offset) = instant_shape(bound)?;
    let (theirs, their_form) = (shapes.offsets.first()?, shapes.forms.first()?);
    let zone = |offset: &str| {
        if offset.is_empty() {
            "no offset".to_owned()
        } else {
            format!("the offset {offset}")
        }
    };
    if offset != *theirs {
        let (ours, theirs) = (zone(&offset), zone(theirs));
        return Some(format!(
            "the bound `\"{bound}\"` carries {ours} while its values observed in `{path}` carry {theirs}"
        ));
    }
    (form != *their_form).then(|| format!("the bound `\"{bound}\"` is written in the form {form} while its values observed in `{path}` are written in the form {their_form}"))
}

/// Law 25 over the orders one task's expression makes: one finding per field whose observed
/// instants text order cannot compare.
pub(super) fn orders(task: &str, orders: &[Order<'_>], out: &mut Vec<Diagnostic>) {
    let fields: BTreeSet<&str> = orders.iter().map(|order| order.field).collect();
    for field in fields {
        let mine: Vec<&Order<'_>> = orders.iter().filter(|o| o.field == field).collect();
        let Some(first) = mine.first() else {
            continue;
        };
        let (records, path) = (first.records, first.records.path());
        let Some(shapes) = observed(&records, field) else {
            continue;
        };
        let Some(reason) = mine.iter().find_map(|o| discord(&shapes, o.bound, path)) else {
            continue;
        };
        let mut ops: Vec<&str> = Vec::new();
        for order in &mine {
            if !ops.contains(&order.op) {
                ops.push(order.op);
            }
        }
        let ops = listed(&ops, "`", "`");
        out.push(Diagnostic { kind: "records", message: format!("TEXT ORDER ON INSTANTS: the task `{task}` compares `.{field}` as text ({ops}), but {reason}: text order is not time order (2026-09-01T02:30:00+02:00 sorts after 2026-09-01T01:00:00Z though it is 00:30 UTC). Compare instants: `(.{field} | fromdateiso8601)` against a bound converted the same way and written with the offset the request means (`\"2026-09-01T00:00:00Z\" | fromdateiso8601` for UTC); `fromdateiso8601` reads a date-time with its offset and refuses one without.") });
    }
}
