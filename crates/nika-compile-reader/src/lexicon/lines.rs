// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The clauses a line filter rides in. « ./rando/guide.md : extrais toutes les lignes de
//! titre markdown (celles qui commencent par un ou plusieurs #), telles quelles, dans
//! l'ordre, une par ligne → ./out/titres.txt » (sealed sv3-04) is three things at once: a
//! source led by its path and a colon, an extraction of whole lines by a stated pattern —
//! a line filter the compiler writes, never language work — and a destination behind an
//! arrow. « Read ./guide.md and write the lines that start with # to ./out/titles.txt »
//! names the same filter as the object of a write.
use super::super::objects;
use super::super::paths::{self, PathShape};
use super::super::plan::{Binding, Effect, EffectPolicy, EffectVerb, Op, Step};
use super::super::rules;
use super::{Reading, push_rule};

/// Connectors that introduce a source path inside an object (« de ./guide.md »).
const SOURCE_CONNECTORS: &[&str] = &[
    "de", "du", "from", "of", "in", "dans", "depuis", "desde", "da", "aus", "von", "em",
];

/// « ./rando/guide.md : extrais … »: the path before the colon is the source the clause
/// works on, read once; the clause after the colon is read on its own with that read in
/// place. Returns whether the clause was led by a source.
pub(super) fn leading_source(original: &str, reading: &mut Reading, money: &mut [String]) -> bool {
    let Some((head, rest)) = original.split_once(':') else {
        return false;
    };
    let path = head.trim();
    let rest = rest.trim();
    if path.contains(char::is_whitespace)
        || rest.is_empty()
        || rest.starts_with("//")
        || !matches!(paths::token(path), Some(PathShape::File(_)))
    {
        return false;
    }
    push_read(path, original, reading);
    let lower = rest.to_lowercase();
    super::read_clause(&lower, rest, reading, money);
    true
}

/// « extrais toutes les lignes … (celles qui commencent par …) → ./out/titres.txt »,
/// « extrais les lignes qui commencent par # de ./guide.md dans ./out/titres.txt »: the
/// object of an extract that states a line filter. The destination the object names is the
/// write of the kept lines, a source path inside it is the read. Returns whether the
/// clause was read as a line filter.
pub(super) fn read_extract(
    detail: &str,
    detail_lower: &str,
    original: &str,
    reading: &mut Reading,
) -> bool {
    // The rule keeps the whole clause, head included (« extrais toutes les lignes … »): the
    // cue is then covered by the rule it produced, as any stated computation's is.
    let clause_lower = original.to_lowercase();
    if detail.len() != detail_lower.len() || original.len() != clause_lower.len() {
        return false;
    }
    let mut object = original.to_owned();
    let mut destination = None;
    let mut source = None;
    for shape in paths::literals(original) {
        let PathShape::File(path) = shape else {
            continue;
        };
        let Some(at) = original.find(&path) else {
            continue;
        };
        if destination.is_none() && objects::destination_at(&clause_lower, at).is_some() {
            destination = Some(path);
        } else if source.is_none() {
            source = Some(path);
        }
    }
    if let Some(path) = &destination
        && let Some(at) = original.find(path.as_str())
        && let Some(cut) = objects::destination_at(&clause_lower, at)
    {
        object.truncate(cut);
    }
    if let Some(path) = &source {
        object = without_source(&object, path);
    }
    let Some(rule) = rules::line_filter(object.trim()) else {
        return false;
    };
    if let Some(path) = &source {
        push_read(path, original, reading);
    }
    push_rule(original, rule, reading);
    if let Some(path) = destination {
        push_write(&path, original, reading);
        // What follows the destination (« et signale-moi ») is read on its own.
        super::defer_residue(original, &path, reading);
    }
    true
}

/// « write the lines that start with # to ./out/titles.txt »: the object a write names is a
/// line filter over what was read; the write writes the kept lines. Returns whether the
/// object was read as a line filter.
pub(super) fn written_lines(object: &str, original: &str, reading: &mut Reading) -> bool {
    let Some(rule) = rules::line_filter(object.trim()) else {
        return false;
    };
    push_rule(original, rule, reading);
    true
}

/// The object without « de ./guide.md »: the path and the connector before it.
fn without_source(object: &str, path: &str) -> String {
    let Some(at) = object.find(path) else {
        return object.to_owned();
    };
    let before = object[..at].trim_end();
    let before = match before.rsplit_once(' ') {
        Some((head, word)) if SOURCE_CONNECTORS.contains(&word.to_lowercase().as_str()) => head,
        _ => before,
    };
    let after = object[at + path.len()..].trim_start();
    format!("{before} {after}").trim().to_owned()
}

fn push_read(path: &str, original: &str, reading: &mut Reading) {
    reading.plan.bindings.push(Binding {
        role: "path",
        literal: path.to_owned(),
    });
    reading.plan.push_step(Step {
        op: Op::Read,
        evidence: original.to_owned(),
        detail: paths::material(path),
        categories: Vec::new(),
    });
}

fn push_write(path: &str, original: &str, reading: &mut Reading) {
    reading.plan.bindings.push(Binding {
        role: "path",
        literal: path.to_owned(),
    });
    super::effects::push_effect(
        &mut reading.plan,
        Effect {
            verb: EffectVerb::Write,
            target: path.to_owned(),
            evidence: original.to_owned(),
            policy: EffectPolicy::Automatic,
            policy_literal: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::super::super::plan::Op;
    use super::super::read;

    const TITLES: &str = "./rando/guide.md : extrais toutes les lignes de titre markdown (celles qui commencent par un ou plusieurs #), telles quelles, dans l'ordre, une par ligne → ./out/titres.txt. Rien d'autre dans le fichier.";

    #[test]
    fn a_source_led_extraction_of_lines_is_a_read_a_line_filter_and_a_write() {
        let reading = read(TITLES);
        let ops: Vec<Op> = reading.plan.steps.iter().map(|s| s.op).collect();
        assert_eq!(ops, [Op::Read, Op::Compute], "{:?}", reading.plan.steps);
        assert_eq!(reading.plan.steps[0].detail, "./rando/guide.md");
        let rule = reading.plan.rules.first().expect("a line filter");
        assert!(rule.lines());
        assert!(rule.jq().contains("startswith(\"#\")"), "{}", rule.jq());
        // The arrow is the destination: the whole clause before it is the rule's text.
        assert!(
            rule.text().ends_with("une par ligne"),
            "the rule keeps the clause up to the arrow: {}",
            rule.text()
        );
        assert_eq!(reading.plan.effects.len(), 1, "{:?}", reading.plan.effects);
        assert_eq!(reading.plan.effects[0].target, "./out/titres.txt");
        assert!(reading.unresolved.is_empty(), "{:?}", reading.unresolved);
        assert!(reading.pending.is_empty(), "{:?}", reading.pending);
    }

    #[test]
    fn a_line_filter_inside_an_extract_or_a_write_object_is_the_computation() {
        for intent in [
            "Extrais toutes les lignes qui commencent par # de ./rando/guide.md dans ./out/titres.txt",
            "Lis ./rando/guide.md et écris dans ./out/titres.txt les lignes qui commencent par #, telles quelles, dans l'ordre.",
            "Read ./guide.md and write the lines that start with # to ./out/titles.txt, as they are, in order.",
        ] {
            let reading = read(intent);
            let ops: Vec<Op> = reading.plan.steps.iter().map(|s| s.op).collect();
            assert!(
                ops.contains(&Op::Read) && ops.contains(&Op::Compute),
                "{intent}: {ops:?}"
            );
            assert!(
                !ops.contains(&Op::Extract) && !ops.contains(&Op::Draft),
                "{intent}: {ops:?}"
            );
            assert_eq!(
                reading.plan.effects.len(),
                1,
                "{intent}: {:?}",
                reading.plan.effects
            );
            assert!(
                reading
                    .plan
                    .rules
                    .iter()
                    .any(super::super::super::rules::Rule::lines),
                "{intent}"
            );
            assert!(
                reading.unresolved.is_empty(),
                "{intent}: {:?}",
                reading.unresolved
            );
        }
    }
}
