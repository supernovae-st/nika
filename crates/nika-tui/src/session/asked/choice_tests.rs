// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::panic)]

use super::*;
use nika_session::outcome::Incarnation;
use std::sync::Arc;

fn snapshot(id: QuestionId, questions: Vec<work::AskedQuestion>) -> work::Work {
    let facts = nika_session::LifecycleFacts::new();
    let rail = work::Rail::from(&nika_session::Lifecycle::from_facts(&facts));
    work::Work::new(
        std::path::PathBuf::from("/fixture"),
        work::Request::default(),
        work::Waiting::Question {
            key: "sources".into(),
            id,
        },
        None,
        None,
        None,
        None,
        rail,
    )
    .with_conversation(Vec::new(), Vec::new(), questions)
}

#[test]
fn current_cooperative_question_preserves_options_multiplicity_and_free_text() {
    let incarnation = Arc::new(Incarnation);
    let id = QuestionId::new("q1".into(), &incarnation);
    for multiple in [false, true] {
        for free in [false, true] {
            let question = work::AskedQuestion::new(id.clone(), "sources", "Which sources?")
                .with_why("Sources for this digest")
                .with_options(
                    vec![
                        work::Offer::new("rss", "RSS feed", false),
                        work::Offer::new("web", "Website", true),
                    ],
                    free,
                    multiple,
                );
            let shown = capture(&snapshot(id.clone(), vec![question]), 7);
            let Waiting::QuestionDocument { key, asked } = shown else {
                panic!("typed choices missing");
            };
            assert_eq!(key, "sources");
            assert_eq!(asked.label, "Which sources?");
            assert_eq!(asked.why, "Sources for this digest");
            assert_eq!(asked.witness, witness(&id, 7));
            assert_eq!(asked.free_text, free);
            // The author's recommendation rides along, on its own offer only.
            let offers = vec![
                Offer::new("rss", "RSS feed"),
                Offer::new("web", "Website").recommended(),
            ];
            assert_eq!(
                asked.shape,
                if multiple {
                    Shape::Multiple(offers)
                } else {
                    Shape::Choice(offers)
                }
            );
        }
    }
}

#[test]
fn stale_foreign_closed_or_other_key_question_never_lends_its_options() {
    let incarnation = Arc::new(Incarnation);
    let id = QuestionId::new("q1".into(), &incarnation);
    for case in 0..4 {
        let mut question = work::AskedQuestion::new(id.clone(), "sources", "Which sources?")
            .with_options(vec![work::Offer::new("rss", "RSS", false)], true, true);
        match case {
            0 => question.id = QuestionId::new("q1".into(), &Arc::new(Incarnation)),
            1 => question.id = QuestionId::new("old".into(), &incarnation),
            2 => question.key = "other".into(),
            _ => question.state = work::AskedState::After,
        }
        assert_eq!(
            capture(&snapshot(id.clone(), vec![question]), 7),
            Waiting::Question {
                key: "sources".into()
            },
            "{case}"
        );
    }
}

#[test]
fn grouped_questions_show_native_label_only_choices_in_the_owners_order() {
    let incarnation = Arc::new(Incarnation);
    let first_id = QuestionId::new("q1".into(), &incarnation);
    let second_id = QuestionId::new("q2".into(), &incarnation);
    for multi in [false, true] {
        let first = work::AskedQuestion::new(first_id.clone(), "format", "Which format?")
            .with_options(vec![work::Offer::new("yes", "Markdown", true)], true, multi);
        let second = work::AskedQuestion::new(second_id.clone(), "delivery", "Which delivery?")
            .with_options(
                vec![work::Offer::new("yes", "Local file", false)],
                false,
                false,
            );
        // The Waiting owner supplies order; the document list is not authority for it.
        let mut work = snapshot(first_id.clone(), vec![second.clone(), first]);
        work.waiting = work::Waiting::Questions {
            ids: vec![first_id.clone(), second_id.clone()],
        };
        let Waiting::QuestionDocument { key, asked } = capture(&work, 7) else {
            panic!("grouped native question missing");
        };
        assert_eq!(key, "format");
        assert_eq!(asked.label, "Which format?");
        assert_eq!(asked.witness, witness(&first_id, 7));
        assert!(asked.free_text);
        let offers = vec![Offer::new("yes", "Markdown").recommended()];
        assert_eq!(
            asked.shape,
            if multi {
                Shape::Multiple(offers)
            } else {
                Shape::Choice(offers)
            }
        );

        // Answering removes the first question from the owner's open projection.
        work.questions = vec![second];
        let Waiting::QuestionDocument { key, asked: next } = capture(&work, 7) else {
            panic!("next grouped question missing");
        };
        assert_eq!(key, "delivery");
        assert_eq!(next.witness, witness(&second_id, 7));
        assert_ne!(next.witness, asked.witness);
        assert_eq!(
            next.shape,
            Shape::Choice(vec![Offer::new("yes", "Local file")])
        );
        assert!(!next.free_text);
    }
}

#[test]
fn grouped_questions_exclude_foreign_unlisted_and_dependent_documents() {
    let incarnation = Arc::new(Incarnation);
    let id = QuestionId::new("q1".into(), &incarnation);
    let offered = work::AskedQuestion::new(id.clone(), "format", "Which format?").with_options(
        vec![work::Offer::new("yes", "Markdown", false)],
        true,
        false,
    );
    let mut work = snapshot(id.clone(), vec![offered]);
    for ids in [
        vec![QuestionId::new("q1".into(), &Arc::new(Incarnation))],
        vec![QuestionId::new("other".into(), &incarnation)],
    ] {
        work.waiting = work::Waiting::Questions { ids };
        assert_eq!(capture(&work, 7), Waiting::Free);
    }
    work.waiting = work::Waiting::Questions { ids: vec![id] };
    work.questions[0].state = work::AskedState::After;
    assert_eq!(capture(&work, 7), Waiting::Free);
}

#[test]
fn proposal_choices_keep_both_identities_and_the_owners_answer_shape() {
    let incarnation = Arc::new(Incarnation);
    let id = QuestionId::new("proposal-question".into(), &incarnation);
    let proposal = ProposalId::of("candidate one");
    for multi in [false, true] {
        let q = work::AskedQuestion::new(id.clone(), "format", "Which format?").with_options(
            vec![
                work::Offer::new("yes", "Markdown", true),
                work::Offer::new("json", "JSON", false),
            ],
            true,
            multi,
        );
        let mut work = snapshot(id.clone(), Vec::new()).with_open_choices(vec![q]);
        work.waiting = work::Waiting::Consent {
            proposal: proposal.clone(),
        };
        let Waiting::ProposalChoice {
            proposal: shown,
            key,
            asked,
        } = capture(&work, 7)
        else {
            panic!("proposal choice")
        };
        assert_eq!(shown, proposal.as_str());
        assert_eq!(key, "format");
        assert_eq!(asked.label, "Which format?");
        assert!(asked.free_text);
        let offers = vec![
            Offer::new("yes", "Markdown").recommended(),
            Offer::new("json", "JSON"),
        ];
        assert_eq!(
            asked.shape,
            if multi {
                Shape::Multiple(offers)
            } else {
                Shape::Choice(offers)
            }
        );
        assert_eq!(asked.witness, proposal_witness(&proposal, &id, 7));
        assert_ne!(asked.witness, witness(&id, 7));
        work.waiting = work::Waiting::Consent {
            proposal: ProposalId::of("candidate two"),
        };
        let Waiting::ProposalChoice { asked: changed, .. } = capture(&work, 7) else {
            panic!("new proposal")
        };
        assert_ne!(changed.witness, asked.witness);
        work.open_choices.clear();
        assert_eq!(capture(&work, 7), Waiting::Proposal);
    }
}
