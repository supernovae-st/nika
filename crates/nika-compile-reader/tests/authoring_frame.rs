// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Authoring the workflow and the effects its program must perform have different scopes.
use nika_compile_reader::lexicon;
use nika_compile_reader::plan::{EffectPolicy, EffectVerb};

#[test]
fn an_authoring_opening_is_kept_whole_without_inventing_a_program_effect() {
    for intent in [
        "Create a new workflow named config-values",
        "Create a workflow",
        "Please create a workflow called report",
        "Crée un nouveau workflow nommé valeurs-config",
        "Créez un workflow appelé rapport",
    ] {
        let reading = lexicon::read(intent);
        assert_eq!(
            reading.plan.effects.len(),
            0,
            "{intent}: {:?}",
            reading.plan
        );
        assert_eq!(reading.unresolved, [intent]);
        assert_eq!(reading.seen, [intent]);
        assert!(
            !reading.complete(),
            "framing alone proves no program: {intent}"
        );
    }
}

#[test]
fn a_relative_body_keeps_its_own_create_effect_and_verbatim_evidence() {
    for (intent, body, target) in [
        (
            "Create a workflow that creates an invoice",
            "creates an invoice",
            "an invoice",
        ),
        (
            "Create a workflow named invoices to create an invoice",
            "create an invoice",
            "an invoice",
        ),
        (
            "Crée un workflow qui crée une facture",
            "crée une facture",
            "une facture",
        ),
        (
            "Crée un nouveau workflow nommé factures pour créer une facture",
            "créer une facture",
            "une facture",
        ),
    ] {
        let reading = lexicon::read(intent);
        assert_eq!(
            reading.plan.effects.len(),
            1,
            "{intent}: {:?}",
            reading.plan
        );
        let effect = &reading.plan.effects[0];
        assert_eq!(effect.verb, EffectVerb::Create);
        assert_eq!(effect.policy, EffectPolicy::Automatic);
        assert_eq!(effect.target, target);
        assert_eq!(effect.evidence, body);
        assert_eq!(reading.unresolved, [intent]);
        assert_eq!(reading.seen, [intent, body]);
        assert!(!reading.complete());
    }
}

#[test]
fn ordinary_created_objects_are_still_business_effects() {
    for (intent, target) in [
        ("Create an invoice", "an invoice"),
        ("Create a user", "a user"),
        ("Create a file", "a file"),
        ("Crée une facture", "une facture"),
        ("Crée un utilisateur", "un utilisateur"),
        ("Crée un fichier", "un fichier"),
        ("Create a workflow in n8n", "a workflow in n8n"),
        (
            "Create a workflow at https://api.example.test/workflows",
            "a workflow at https://api.example.test/workflows",
        ),
        ("Create a workflow-template", "a workflow-template"),
    ] {
        let reading = lexicon::read(intent);
        assert_eq!(
            reading.plan.effects.len(),
            1,
            "{intent}: {:?}",
            reading.plan
        );
        assert_eq!(reading.plan.effects[0].verb, EffectVerb::Create, "{intent}");
        assert_eq!(
            reading.plan.effects[0].policy,
            EffectPolicy::Automatic,
            "{intent}"
        );
        assert_eq!(reading.plan.effects[0].target, target, "{intent}");
        assert_eq!(reading.plan.effects[0].evidence, intent, "{intent}");
    }
}

#[test]
fn a_later_or_scheduled_workflow_creation_stays_a_program_effect() {
    for intent in [
        "Read ./input.json, then create a workflow named child",
        "Every Monday, create a workflow named child",
        "Create a workflow named child every Monday",
    ] {
        let reading = lexicon::read(intent);
        assert_eq!(
            reading.plan.effects.len(),
            1,
            "{intent}: {:?}",
            reading.plan
        );
        assert_eq!(reading.plan.effects[0].verb, EffectVerb::Create, "{intent}");
        assert_eq!(
            reading.plan.effects[0].policy,
            EffectPolicy::Automatic,
            "{intent}"
        );
    }
}

#[test]
fn an_unknown_body_is_not_swallowed_or_claimed_complete() {
    let intent = "Create a workflow that frobnicate the records";
    let reading = lexicon::read(intent);
    assert_eq!(reading.plan.effects.len(), 0);
    assert_eq!(reading.unresolved, [intent, "frobnicate the records"]);
    assert_eq!(reading.seen, [intent, "frobnicate the records"]);
    assert!(!reading.complete());
}

#[test]
fn a_relative_prohibition_does_not_turn_into_a_requested_creation() {
    for intent in [
        "Create a workflow that never creates an invoice",
        "Crée un workflow qui ne crée pas de facture",
    ] {
        let reading = lexicon::read(intent);
        assert_eq!(
            reading.plan.effects.len(),
            1,
            "{intent}: {:?}",
            reading.plan
        );
        assert_eq!(reading.plan.effects[0].verb, EffectVerb::Create, "{intent}");
        assert_eq!(
            reading.plan.effects[0].policy,
            EffectPolicy::Forbidden,
            "{intent}"
        );
        assert!(!reading.complete());
    }
}
