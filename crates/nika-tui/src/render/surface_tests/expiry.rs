// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

#[test]
fn compact_expiry_notice_keeps_the_question_f2_and_every_literal_choice_visible() {
    const NOTICE: &str = "Expired: a fresh question is needed before approval.";
    const PROMPT: &str = "Choose the exact report label";
    for key in ["gate", "gate_choice"] {
        for (long, ascii) in [(false, false), (true, false), (false, true), (true, true)] {
            let prompt = if long {
                format!("{PROMPT}: {}", "exact spelling matters. ".repeat(8))
            } else {
                PROMPT.into()
            };
            let offers = compact_gate_offers(key);
            let labels: Vec<_> = offers.iter().map(|offer| offer.label.clone()).collect();
            let mut asked = Asked::new(
                &prompt,
                NOTICE,
                true,
                Shape::Choice(offers),
                "action:expiry",
                1,
            )
            .retaining(Retained::new(Some("A retained request".into()), vec![]));
            asked.free_text = false;
            let mut state = state(
                Presentation::Workspace,
                (80, 24),
                Waiting::Action {
                    key: key.into(),
                    asked,
                },
                ascii,
            );
            let context = format!("run observed: exit 4\n{prompt}\nFull gate context remains here");
            state.transcript.push(Committed::new(Kind::Gate, &context));
            let mut composer = composer(&state.waiting, "kept draft");
            let (buffer, band) = gate_workspace_frame(&state, &composer);
            let question = row(&buffer, band.0.y + 1);
            assert!(question.starts_with(PROMPT), "{question}");
            assert!(question.contains("F2"), "{question}");
            assert_eq!(row(&buffer, band.0.y + 2), NOTICE);
            for (index, label) in labels.iter().enumerate() {
                let y = (band.0.y..band.0.bottom())
                    .find(|y| row(&buffer, *y) == format!("  ( ) {label}").trim_end())
                    .expect("offer");
                assert_eq!(
                    hit(&state, &composer, band, Position::new(band.0.x + 2, y)),
                    Some(Hit::Item(index))
                );
            }
            assert_eq!(composer.offer_selected(&state.waiting), None);
            select(&mut composer, &state.waiting, labels.len() - 1);
            assert_eq!(
                composer.offer_selected(&state.waiting),
                Some(labels.len() - 1)
            );
            assert_eq!(composer.text(), "kept draft");
            let owner = super::super::question::asked_block(&state).expect("F2 gate context");
            assert_eq!(state.transcript[owner].text, context);
        }
    }
}
