// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversion family: « convert ./fleet/mileage.csv (columns vehicle, driver, km) into
//! ./out/mileage.json, a JSON array with one object per row using the column names as keys,
//! same row order », « convertis ./a.csv en ./out/a.json », « konvertiere ./a.json nach
//! ./out/a.csv » — one structured source read and parsed, its records written to a
//! destination of another structured format, no language step. The rows are the rows and
//! the format is the destination's own; the words after the destination (one object per
//! row, the column names as keys, the row order) say what the conversion does by itself.
use super::super::paths::{self, PathShape, Structured};
use super::super::plan::{Binding, Effect, EffectPolicy, EffectVerb, Op, Step};
use super::super::rules::{Junction, Rule, Shape};
use super::{Reading, push_rule};

/// The conversion heads, six languages, folded as the clause is.
const CONVERT_HEADS: &[&str] = &[
    "convert",
    "converts",
    "convertis",
    "convertissez",
    "convertir",
    "convierte",
    "convierta",
    "convertid",
    "converti",
    "convertite",
    "convertire",
    "konvertiere",
    "konvertieren",
    "konvertier",
    "wandle",
    "wandeln",
    "converta",
    "converte",
    "converter",
    "transform",
    "transforme",
    "transformez",
    "transforma",
    "transformiere",
    "export",
    "exporte",
    "exportez",
    "exporta",
    "exportiere",
];

/// A clause that converts one structured file into another: the head first, then exactly two
/// file paths of two structured formats, the source before the destination. Anything else is
/// not a conversion.
pub(super) fn read(text: &str, original: &str, reading: &mut Reading) -> bool {
    let head = text
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default();
    if !CONVERT_HEADS.contains(&head) {
        return false;
    }
    let files: Vec<String> = paths::literals(original)
        .into_iter()
        .filter_map(|shape| match shape {
            PathShape::File(path) => Some(path),
            _ => None,
        })
        .collect();
    let [source, destination] = files.as_slice() else {
        return false;
    };
    let (Some(from), Some(to)) = (Structured::of(source), Structured::of(destination)) else {
        return false;
    };
    if from == to {
        return false;
    }
    for path in [source, destination] {
        reading.plan.bindings.push(Binding {
            role: "path",
            literal: path.clone(),
        });
    }
    reading.plan.push_step(Step {
        op: Op::Read,
        evidence: original.to_owned(),
        detail: paths::material(source),
        categories: Vec::new(),
    });
    // The identity over the parsed records: the whole clause is the rule's text, so the
    // head is covered by the computation it produced and the format words ride with it.
    push_rule(
        original,
        Rule::typed(original.trim(), Vec::new(), Junction::And, Shape::default()),
        reading,
    );
    super::effects::push_effect(
        &mut reading.plan,
        Effect {
            verb: EffectVerb::Write,
            target: destination.clone(),
            evidence: original.to_owned(),
            policy: EffectPolicy::Automatic,
            policy_literal: None,
        },
    );
    true
}

#[cfg(test)]
mod tests {
    use super::super::super::plan::Op;
    use super::super::read;

    #[test]
    fn a_conversion_is_a_read_the_identity_over_the_records_and_a_write() {
        for intent in [
            "convert ./fleet/mileage.csv (columns vehicle, driver, km) into ./out/mileage.json, a JSON array with one object per row using the column names as keys, same row order",
            "Convertis ./ventes/mars.csv en ./out/mars.json, un tableau JSON avec un objet par ligne.",
            "Konvertiere ./daten/kunden.json nach ./out/kunden.csv, eine Zeile pro Objekt.",
        ] {
            let reading = read(intent);
            let ops: Vec<Op> = reading.plan.steps.iter().map(|s| s.op).collect();
            assert_eq!(
                ops,
                [Op::Read, Op::Compute],
                "{intent}: {:?}",
                reading.plan.steps
            );
            let rule = reading.plan.rules.first().expect("the identity rule");
            assert_eq!(rule.jq(), ".records", "{intent}");
            assert_eq!(
                reading.plan.effects.len(),
                1,
                "{intent}: {:?}",
                reading.plan.effects
            );
            assert!(
                reading.unresolved.is_empty(),
                "{intent}: {:?}",
                reading.unresolved
            );
            assert!(
                reading.pending.is_empty(),
                "{intent}: {:?}",
                reading.pending
            );
        }
    }

    #[test]
    fn a_conversion_needs_two_structured_files_of_two_formats() {
        for intent in [
            "convert ./a.csv into ./out/b.csv",
            "convert ./notes.md into ./out/notes.json",
            "convert ./a.csv into JSON",
        ] {
            let reading = read(intent);
            assert!(
                !reading.plan.steps.iter().any(|s| s.op == Op::Compute),
                "{intent}: {:?}",
                reading.plan.steps
            );
        }
    }
}
