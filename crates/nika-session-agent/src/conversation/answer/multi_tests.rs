// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_session_change::outcome::{Incarnation, QuestionId};
use nika_session_change::work::OfferValue;
use std::sync::Arc;

fn question(role: ValueRole) -> AskedQuestion {
    let id = QuestionId::new("sources".into(), &Arc::new(Incarnation));
    let offers = [
        ("a", "https://a.example/feed"),
        ("b", "https://b.example/feed"),
    ]
    .into_iter()
    .map(|(key, value)| {
        Offer::new(key, key, false).with_values(vec![OfferValue::new(role, value, None)])
    })
    .collect();
    AskedQuestion::new(id, "sources", "Which sources?").with_options(offers, true, true)
}

#[test]
fn several_offers_bind_together_with_their_exact_provenance() {
    let mut conversation = Conversation::default();
    conversation.asked.push(question(ValueRole::ReadSource));
    let said = conversation.answered_by("u2", r#"["b","a"]"#, &BTreeMap::new());
    assert_eq!(
        said,
        ["`sources`: the person picked `a`, `b` with u2; Nika bound all selected values"]
    );
    let bindings = conversation.bindings();
    assert_eq!(bindings.len(), 2);
    for (binding, key, value) in [
        (&bindings[0], "a", "https://a.example/feed"),
        (&bindings[1], "b", "https://b.example/feed"),
    ] {
        assert_eq!(binding.role, ValueRole::ReadSource);
        assert_eq!(binding.value, value);
        assert_eq!(binding.key.as_deref(), Some("sources"));
        assert_eq!(
            binding.provenance,
            Provenance::new(ProvenanceKind::Offered, "u2").with_offer("sources", Some(key.into()))
        );
        let answer = &conversation.answered["sources"];
        assert!(answer.offers("u2", Some(key), (ValueRole::ReadSource, value)));
        assert!(!answer.offers("u1", Some(key), (ValueRole::ReadSource, value)));
    }
    assert!(!conversation.answered["sources"].offers(
        "u2",
        Some("c"),
        (ValueRole::ReadSource, "https://c.example/feed")
    ));
    let before = conversation.bindings().to_vec();
    assert_eq!(
        conversation.answered_by("u3", r#"["a"]"#, &BTreeMap::new()),
        Vec::<String>::new()
    );
    assert_eq!(conversation.bindings(), before);
}

#[test]
fn conflicting_single_values_are_rejected_atomically_without_offered_authority() {
    let mut conversation = Conversation::default();
    let prior = Binding::new(
        ValueRole::OutputPath,
        "kept.md",
        Provenance::new(ProvenanceKind::Named, "u1"),
    );
    conversation.bindings.push(prior.clone());
    conversation.asked.push(question(ValueRole::OutputPath));
    let said = conversation.answered_by("u2", r#"["a","b"]"#, &BTreeMap::new());
    assert_eq!(
        said,
        [
            "`sources`: the selected offers are invalid or contain conflicting single values; nothing is bound"
        ]
    );
    assert_eq!(conversation.bindings(), [prior]);
    assert!(!conversation.answered["sources"].offers(
        "u2",
        Some("a"),
        (ValueRole::OutputPath, "https://a.example/feed")
    ));
}

#[test]
fn invalid_native_sets_never_bind_even_when_a_reading_proposes_a_valid_offer() {
    for line in [
        r#"["a","unknown"]"#,
        r#"["a","a"]"#,
        "[]",
        "[broken]",
        "[1]",
    ] {
        let mut conversation = Conversation::default();
        conversation.asked.push(question(ValueRole::ReadSource));
        let read = BTreeMap::from([("sources".into(), Pick::Offer("a".into()))]);
        conversation.answered_by("u2", line, &read);
        assert_eq!(conversation.bindings(), [], "{line}");
        assert!(
            !conversation.answered["sources"].offers(
                "u2",
                Some("a"),
                (ValueRole::ReadSource, "https://a.example/feed")
            ),
            "{line}"
        );
    }
}
