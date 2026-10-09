// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::AuthoringRound;

#[test]
fn restatement_preserves_only_unchanged_money_text_across_utf8_and_length_changes()
-> Result<(), &'static str> {
    let clause = "an unclear business rule";
    for original in [
        format!("Budget: $0. Préfixe résumé. {clause}."),
        format!("Préfixe résumé. {clause}. Budget: $0."),
    ] {
        for answer in ["x", "une règle métier beaucoup plus longue", "Budget: $99."] {
            let mut old = AuthoringRound::new(&original);
            let start = original
                .find("Budget: $0.")
                .ok_or("missing fixture directive")?;
            old.money.push(start..start + "Budget: $0.".len());
            old.answers.insert("old-key".into(), "true".into());
            old.continuation = Some(serde_json::json!({"old": "plan"}));
            let next = old.restate_clause(clause, answer);
            let recorded: Vec<_> = next
                .money
                .iter()
                .map(|span| &next.intent[span.clone()])
                .collect();
            assert_eq!(recorded, ["Budget: $0."]);
            assert!(next.intent.contains(answer));
            assert!(next.answers.is_empty());
            assert!(next.continuation.is_none());
            assert_eq!(next.restatements, 1);
        }
    }
    Ok(())
}

#[test]
fn replacing_admitted_text_cannot_transfer_its_admission_to_new_words() {
    let mut old = AuthoringRound::new("Budget: $0. Keep the business result.");
    old.money.push(0..11);
    for clause in ["Budget: $0.", "$0"] {
        let next = old.restate_clause(clause, "Budget: $99.");
        assert!(next.money.is_empty());
    }
}
