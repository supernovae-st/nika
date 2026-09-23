// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A prepared literal file copy is identity work; its other clauses remain obligations.
use nika_compile_reader::{
    hot, lexicon,
    plan::{EffectPolicy, EffectVerb, Op},
};

fn literal_copy(intent: &str, source: &str, destination: &str) {
    let reading = lexicon::read(intent);
    assert_eq!(reading.plan.steps.len(), 1, "{intent}: {reading:#?}");
    assert_eq!(reading.plan.steps[0].op, Op::Read, "{intent}");
    assert_eq!(reading.plan.steps[0].detail, source, "{intent}");
    assert_eq!(reading.plan.effects.len(), 1, "{intent}: {reading:#?}");
    assert_eq!(reading.plan.effects[0].verb, EffectVerb::Write);
    assert_eq!(reading.plan.effects[0].target, destination);
    assert_eq!(reading.plan.effects[0].policy, EffectPolicy::Automatic);
    for evidence in [
        &reading.plan.steps[0].evidence,
        &reading.plan.effects[0].evidence,
    ] {
        assert_eq!(evidence, intent.trim_end_matches('.'), "{intent}");
    }
    let paths: Vec<_> = reading
        .plan
        .bindings
        .iter()
        .filter(|b| b.role == "path")
        .map(|b| b.literal.as_str())
        .collect();
    assert!(
        paths.contains(&source) && paths.contains(&destination),
        "{intent}"
    );
    assert!(
        paths.iter().all(|p| [source, destination].contains(p)),
        "{intent}"
    );
    assert!(
        reading.hot_rejections().is_empty(),
        "{intent}: {reading:#?}"
    );
    assert!(
        hot::rejections(intent, &reading).is_empty(),
        "{intent}: {:?}",
        hot::rejections(intent, &reading)
    );
}

#[test]
fn french_prepared_file_copies_keep_both_literals() {
    for intent in [
        "Prépare la copie de entree.txt dans sortie.txt.",
        "Préparez une copie de entree.txt vers sortie.txt.",
        "Préparer la copie du fichier entree.txt dans le fichier sortie.txt.",
        "Prépare une copie de entree.txt telle quelle dans sortie.txt.",
    ] {
        literal_copy(intent, "entree.txt", "sortie.txt");
    }
}

#[test]
fn english_prepared_file_copies_keep_both_literals() {
    for intent in [
        "Prepare a copy of Source.txt to Result.txt.",
        "Prepare the copy from Source.txt into Result.txt.",
        "Prepare a file copy of Source.txt in Result.txt.",
        "Prepare a copy of the file Source.txt as is, byte for byte, to the file Result.txt.",
    ] {
        literal_copy(intent, "Source.txt", "Result.txt");
    }
}

#[test]
fn quoted_paths_keep_their_case_and_copy_words_are_only_literal_data() {
    literal_copy(
        "Prepare a copy of `./Docs/Translate.txt` to \"./Out/Marketing.txt\".",
        "./Docs/Translate.txt",
        "./Out/Marketing.txt",
    );
}

#[test]
fn a_copy_noun_at_the_head_has_the_same_literal_structure() {
    for intent in [
        "Copy of ./a.txt to ./b.txt",
        "Copie de ./a.txt dans ./b.txt",
    ] {
        literal_copy(intent, "./a.txt", "./b.txt");
    }
}

#[test]
fn language_copy_and_summaries_still_request_language_work() {
    for intent in [
        "Prepare marketing copy",
        "Prepare marketing copy from ./a.txt to ./b.txt",
        "Prepare a rewritten copy of ./a.txt to ./b.txt",
        "Prepare a translated copy of ./a.txt to ./b.txt",
        "Prepare a summary of ./a.txt to ./b.txt",
        "Prépare une copie traduite de ./a.txt dans ./b.txt",
        "Prépare un résumé de ./a.txt dans ./b.txt",
    ] {
        let reading = lexicon::read(intent);
        assert!(reading.plan.has(Op::Draft), "{intent}: {reading:#?}");
    }
}

#[test]
fn a_separate_transformation_is_carried_and_unknown_work_stays_unresolved() {
    let intent = "Prepare a copy of ./a.txt to ./b.txt and summarize it";
    let reading = lexicon::read(intent);
    assert!(reading.plan.has(Op::Read), "{reading:#?}");
    assert!(
        reading
            .plan
            .steps
            .iter()
            .any(|step| { step.op == Op::Draft && step.evidence == "summarize it" }),
        "{reading:#?}"
    );
    let intent = "Prépare la copie de ./a.txt dans ./b.txt puis harmonise le ton";
    let reading = lexicon::read(intent);
    assert!(reading.plan.has(Op::Read), "{reading:#?}");
    assert!(
        reading
            .unresolved
            .iter()
            .any(|clause| clause == "harmonise le ton"),
        "{reading:#?}"
    );
    assert!(!reading.hot_rejections().is_empty());
}

#[test]
fn extra_work_and_ambiguous_paths_never_become_an_automatic_identity_copy() {
    for prefix in ["Prepare a copy of", "Prépare la copie de", "Copy", "Copie"] {
        for detail in [
            "./a.txt to ./b.txt with spelling corrected",
            "./a.txt to ./b.txt translated into French",
            "./a.txt to ./b.txt and polish the tone",
            "./a.txt to ./b.txt then frobnicate the result",
            "./a.txt to ./b.txt after checking my approval",
            "./a.txt to ./b.txt with ./extra/",
            "./a.txt ./b.txt",
            "./a.txt from ./b.txt",
            "./a.txt to ./b.txt or ./c.txt",
            "./a.txt to ./out/",
            "./a.txt to ./out/{name}.txt",
            "./a.txt to ./out/*.txt",
        ] {
            let intent = format!("{prefix} {detail}");
            let reading = lexicon::read(&intent);
            let automatic_identity = reading.plan.steps.len() == 1
                && reading.plan.has(Op::Read)
                && reading.plan.effects.len() == 1
                && reading.plan.effects[0].verb == EffectVerb::Write
                && reading.plan.effects[0].policy == EffectPolicy::Automatic
                && reading.hot_rejections().is_empty()
                && hot::rejections(&intent, &reading).is_empty();
            assert!(!automatic_identity, "{intent}: {reading:#?}");
        }
    }
}

#[test]
fn approval_on_a_prepared_copy_stays_on_the_write() {
    for intent in [
        "Prepare a copy of ./a.txt to ./b.txt only after my approval",
        "Prépare la copie de ./a.txt dans ./b.txt seulement après ma validation",
    ] {
        let reading = lexicon::read(intent);
        assert!(reading.plan.has(Op::Read), "{intent}: {reading:#?}");
        assert!(
            reading.plan.effects.iter().any(|e| {
                e.verb == EffectVerb::Write
                    && e.target == "./b.txt"
                    && e.policy == EffectPolicy::HumanFirst
            }),
            "{intent}: {reading:#?}"
        );
        assert!(!reading.plan.has(Op::Draft), "{intent}");
    }
}
