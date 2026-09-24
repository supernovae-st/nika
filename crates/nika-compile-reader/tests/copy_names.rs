// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A copy's files are their exact literals. A quoted or capitalized multiword name is taken
//! whole; a copy that names no destination, names its source again, or leaves a lowercase
//! multiword name open asks for the exact file; nothing is ever truncated to its last word.
use nika_compile_reader::{
    hot, lexicon,
    paths::{self, PathShape},
    plan::{EffectPolicy, EffectVerb, Op},
};

/// The one read and the one automatic write of an admitted copy reading.
fn copy(intent: &str) -> (String, String, Vec<String>) {
    let reading = lexicon::read(intent);
    let why = hot::rejections(intent, &reading);
    assert!(
        reading.hot_rejections().is_empty() && why.is_empty(),
        "{intent}: {why:?}"
    );
    assert_eq!(reading.plan.steps.len(), 1, "{intent}: {reading:#?}");
    assert_eq!(reading.plan.steps[0].op, Op::Read, "{intent}");
    assert_eq!(reading.plan.effects.len(), 1, "{intent}: {reading:#?}");
    assert_eq!(reading.plan.effects[0].verb, EffectVerb::Write, "{intent}");
    assert_eq!(reading.plan.effects[0].policy, EffectPolicy::Automatic);
    let mut bound: Vec<String> = reading
        .plan
        .bindings
        .iter()
        .filter(|b| b.role == "path")
        .map(|b| b.literal.clone())
        .collect();
    bound.dedup();
    (
        reading.plan.steps[0].detail.clone(),
        reading.plan.effects[0].target.clone(),
        bound,
    )
}

fn file(path: &str) -> PathShape {
    PathShape::File(path.to_owned())
}

#[test]
fn the_french_request_and_its_quoted_forms_keep_both_whole_names() {
    for intent in [
        "Copie exactement le fichier Notes équipe.txt dans un nouveau fichier Copie équipe.txt",
        "Copie \"Notes équipe.txt\" dans \"Copie équipe.txt\".",
        "Copie « Notes équipe.txt » dans « Copie équipe.txt ».",
        "Copie Notes équipe.txt à l'identique dans le fichier Copie équipe.txt.",
    ] {
        let (detail, target, bound) = copy(intent);
        assert_eq!(
            paths::literals(&detail),
            vec![file("Notes équipe.txt")],
            "{intent}"
        );
        assert_eq!(
            paths::single_file(&target).as_deref(),
            Some("Copie équipe.txt"),
            "{intent}"
        );
        assert_eq!(bound, ["Notes équipe.txt", "Copie équipe.txt"], "{intent}");
    }
}

#[test]
fn english_german_and_paraphrased_copies_keep_whole_names() {
    for (intent, source, destination) in [
        (
            "Copy `Team notes.txt` to “Team copy.txt”",
            "Team notes.txt",
            "Team copy.txt",
        ),
        (
            "Copy the file Team notes.txt to the new file Team copy.txt",
            "Team notes.txt",
            "Team copy.txt",
        ),
        (
            "Copy \"Notes de réunion.txt\" to \"Compte rendu.docx\"",
            "Notes de réunion.txt",
            "Compte rendu.docx",
        ),
        (
            "Prepare a copy of \"Q3 report.csv\" to \"Q3 backup.csv\".",
            "Q3 report.csv",
            "Q3 backup.csv",
        ),
        (
            "Kopiere \"Über uns.md\" nach \"Über uns Kopie.md\"",
            "Über uns.md",
            "Über uns Kopie.md",
        ),
        (
            "Copie entree.txt dans sortie.txt.",
            "entree.txt",
            "sortie.txt",
        ),
    ] {
        let (detail, target, bound) = copy(intent);
        assert_eq!(paths::literals(&detail), vec![file(source)], "{intent}");
        assert_eq!(paths::single_file(&target).as_deref(), Some(destination));
        assert_eq!(bound, [source, destination], "{intent}");
    }
}

#[test]
fn a_copy_without_a_distinct_destination_asks_for_the_exact_file() {
    for (intent, source, asked) in [
        ("Fais une copie de entree.txt.", "entree.txt", "une copie"),
        (
            "Faites une copie du fichier entree.txt",
            "entree.txt",
            "une copie",
        ),
        ("Copie entree.txt.", "entree.txt", "Copie"),
        ("Copie le fichier entree.txt.", "entree.txt", "Copie"),
        ("Make a copy of input.txt.", "input.txt", "a copy"),
        ("Copy input.txt", "input.txt", "Copy"),
        ("Please copy input.txt", "input.txt", "copy"),
        ("Copie « Notes équipe.txt ».", "Notes équipe.txt", "Copie"),
        // The source named again is never overwritten: its destination is asked.
        ("Copie entree.txt dans entree.txt", "entree.txt", "Copie"),
        ("Copy ./a.txt to a.txt", "./a.txt", "Copy"),
    ] {
        let (detail, target, bound) = copy(intent);
        assert_eq!(paths::literals(&detail), vec![file(source)], "{intent}");
        assert_eq!(target, asked, "{intent}");
        assert!(paths::literals(&target).is_empty(), "{intent}: {target}");
        assert_eq!(paths::single_file(&target), None, "{intent}");
        assert!(bound.iter().all(|b| b == source), "{intent}: {bound:?}");
    }
}

#[test]
fn a_lowercase_multiword_name_stays_open_and_is_asked() {
    let (detail, target, bound) =
        copy("Copie le fichier notes équipe.txt dans le fichier copie équipe.txt");
    assert_eq!(
        paths::literals(&detail),
        vec![PathShape::Placeholder("notes équipe.txt".to_owned())]
    );
    assert!(paths::literals(&target).is_empty(), "{target}");
    assert!(bound.is_empty(), "{bound:?}");
}

#[test]
fn no_reading_of_a_spaced_name_binds_its_truncated_tail() {
    for intent in [
        "Copie exactement le fichier Notes équipe.txt dans un nouveau fichier Copie équipe.txt",
        "Écris dans ce dossier un workflow copie-equipe.nika qui lit Notes équipe.txt et écrit \
         son contenu à l'identique dans Copie équipe.txt.",
        "Non, ce n'est pas le bon fichier : la source est \"Notes équipe.txt\" et la \
         destination est \"Copie équipe.txt\" (avec l'espace).",
        "Copie notes équipe.txt dans copie équipe.txt",
    ] {
        let reading = lexicon::read(intent);
        let truncated = Some("équipe.txt".to_owned());
        for step in &reading.plan.steps {
            assert!(
                !paths::literals(&step.detail).contains(&file("équipe.txt")),
                "{intent}: {step:?}"
            );
        }
        for effect in &reading.plan.effects {
            assert_ne!(paths::single_file(&effect.target), truncated, "{intent}");
        }
        assert!(
            reading
                .plan
                .bindings
                .iter()
                .all(|b| b.literal != "équipe.txt"),
            "{intent}: {:?}",
            reading.plan.bindings
        );
    }
}

#[test]
fn extra_work_folders_and_unknown_heads_are_not_the_copy_family() {
    for intent in [
        "Copy ./a.txt somewhere safe",
        "Copy ./in/ to ./out/",
        "Copy ./a.txt ./b.txt",
        "Copy ./a.txt to ./b.txt or ./c.txt",
        "Copie exactement le fichier Notes équipe.txt dans Copie équipe.txt puis traduis-le",
    ] {
        let reading = lexicon::read(intent);
        let identity_copy = reading.plan.steps.len() == 1
            && reading.plan.has(Op::Read)
            && reading.plan.effects.len() == 1
            && reading.hot_rejections().is_empty()
            && hot::rejections(intent, &reading).is_empty();
        assert!(!identity_copy, "{intent}: {reading:#?}");
    }
}
