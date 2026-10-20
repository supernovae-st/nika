// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A person's approval that a `before` closes holds the write of the file named after it, typed
//! the way the live gauntlet's founder typed it (2026-10-15): « une validation humaine avant
//! d'ecrire jobs-<date>.md ». Only a gate reads a write from a file it names: a person turning
//! down a proposal (« non laisse tomber tout ca, je veux plutot … ecrire le total dans
//! ./out/total.txt ») bans nothing, and a request that never asks a person holds nothing.
use nika_compile_reader::lexicon;
use nika_compile_reader::plan::{EffectPolicy, EffectVerb};

/// The effects the reading of `intent` holds under `policy`.
fn effects(intent: &str, policy: EffectPolicy) -> Vec<EffectVerb> {
    let plan = lexicon::read(intent).plan;
    let held = plan.effects.iter().filter(|e| e.policy == policy);
    held.map(|e| e.verb).collect()
}

#[test]
fn an_approval_before_writing_a_named_file_holds_that_write() {
    for intent in [
        "oui, je veux une validation humaine avant d'ecrire jobs-<date>.md",
        "Oui, je valide chaque run (oui/non) avant l'écriture de jobs-<date>.md",
        "Un seul workflow : il lit profil.md et attend ton OK avant d'écrire jobs-<date>.md",
        "demande-moi avant d'écrire jobs.md",
    ] {
        assert_eq!(
            effects(intent, EffectPolicy::HumanFirst),
            [EffectVerb::Write],
            "{intent}"
        );
    }
}

#[test]
fn turning_down_a_proposal_bans_no_write() {
    for intent in [
        "non laisse tomber tout ca, je veux plutot compter les lignes de ./data/ventes.csv et ecrire le total dans ./out/total.txt",
        "non, je veux plutot ecrire le resume dans notes.md",
    ] {
        assert_eq!(
            effects(intent, EffectPolicy::Forbidden),
            Vec::<EffectVerb>::new(),
            "{intent}"
        );
        assert_eq!(
            effects(intent, EffectPolicy::HumanFirst),
            Vec::<EffectVerb>::new(),
            "{intent}"
        );
    }
}

/// The question an author asks for that approval names it too, its infinitive and the person
/// glued to the verb that asks it or doing it (« par un humain »): the reading a free « Oui » to
/// it rests on (live gauntlet q02). An infinitive no person performs approves nothing.
#[test]
fn an_approval_question_names_the_write_it_holds() {
    for asked in [
        "Dois-je faire valider par un humain chaque run avant d'écrire jobs-<date>.md ?",
        "Veux-tu valider chaque run avant que j'écrive jobs-<date>.md ?",
        "Voulez-vous approuver chaque run avant d'écrire jobs-<date>.md ?",
        "Should a human approve each run before it writes jobs-<date>.md?",
    ] {
        assert_eq!(
            effects(asked, EffectPolicy::HumanFirst),
            [EffectVerb::Write],
            "{asked}"
        );
    }
    for stated in [
        "Le workflow doit valider le JSON avant d'écrire jobs-<date>.md",
        "il faut confirmer le format avant d'écrire jobs-<date>.md",
    ] {
        assert_eq!(
            effects(stated, EffectPolicy::HumanFirst),
            Vec::<EffectVerb>::new(),
            "{stated}"
        );
    }
}
