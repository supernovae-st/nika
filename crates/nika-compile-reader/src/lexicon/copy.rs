// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The copy family: « copy ./a.txt as is to ./out/b.txt », « copie ./a.txt tel quel, octet
//! pour octet, dans ./out/b.txt », « copia ./a.txt tal cual en ./out/b.txt » — one source
//! file read, one destination written with what was read, no language step. The identity
//! words between the two paths (as is, tel quel, byte for byte, unverändert) state what the
//! copy does by construction; they bind nothing else.
use super::super::paths::{self, PathShape};
use super::super::plan::{Binding, Effect, EffectPolicy, EffectVerb, Op, Step};
use super::Reading;

/// The copy heads, six languages, folded as the clause is (lowercase, accents kept).
const COPY_HEADS: &[&str] = &[
    "copy", "copie", "copiez", "copier", "copia", "copiate", "copiare", "kopiere", "kopier",
    "kopieren", "copie", "copiem",
];

/// A clause that copies one file to another: the copy head first, then exactly two file
/// paths, the source before the destination. Anything else is not a copy.
pub(super) fn read(text: &str, original: &str, reading: &mut Reading) -> bool {
    let head = text
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default();
    if !COPY_HEADS.contains(&head) {
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
    for path in [source, destination] {
        reading.plan.bindings.push(Binding {
            role: "path",
            literal: path.clone(),
        });
    }
    reading.plan.push_step(Step {
        op: Op::Read,
        evidence: original.to_owned(),
        detail: source.clone(),
        categories: Vec::new(),
    });
    super::push_effect(
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
    use super::super::super::plan::{EffectVerb, Op};
    use super::super::read as read_intent;

    #[test]
    fn a_copy_is_a_read_and_a_write_of_what_was_read_in_six_languages() {
        for intent in [
            "Copy ./fete/consignes.txt as is, byte for byte, to ./out/consignes-copie.txt",
            "copie ./fete/consignes.txt tel quel, octet pour octet, dans ./out/consignes-copie.txt",
            "Copia ./fete/consignes.txt tal cual en ./out/consignes-copie.txt",
            "Copia ./fete/consignes.txt così com'è in ./out/consignes-copie.txt",
            "Kopiere ./fete/consignes.txt unverändert nach ./out/consignes-copie.txt",
            "Copia ./fete/consignes.txt tal e qual para ./out/consignes-copie.txt",
        ] {
            let reading = read_intent(intent);
            let ops: Vec<Op> = reading.plan.steps.iter().map(|s| s.op).collect();
            assert_eq!(ops, [Op::Read], "{intent}: {:?}", reading.plan);
            assert_eq!(
                reading.plan.steps[0].detail, "./fete/consignes.txt",
                "{intent}"
            );
            assert_eq!(
                reading.plan.effects.len(),
                1,
                "{intent}: {:?}",
                reading.plan
            );
            assert_eq!(reading.plan.effects[0].verb, EffectVerb::Write);
            assert_eq!(reading.plan.effects[0].target, "./out/consignes-copie.txt");
            assert!(
                reading.unresolved.is_empty(),
                "{intent}: {:?}",
                reading.unresolved
            );
            assert!(
                reading.plan.constraints.is_empty(),
                "{intent}: {:?}",
                reading.plan.constraints
            );
        }
    }

    #[test]
    fn a_copy_with_one_path_or_a_folder_is_not_the_copy_family() {
        for intent in ["Copy ./a.txt somewhere safe", "Copy ./in/ to ./out/"] {
            let reading = read_intent(intent);
            assert!(
                reading
                    .plan
                    .effects
                    .iter()
                    .all(|e| e.verb != EffectVerb::Write)
                    || !reading.unresolved.is_empty()
                    || reading.plan.steps.is_empty(),
                "{intent}: {:?}",
                reading.plan
            );
        }
    }
}
