// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A retrieval clause keeps its own words. An identifier stated before a structured source
//! selects part of it, which a read cannot carry: the plan keeps it as unknown work (which a
//! replay of that plan refuses too) while the source stays opened. A clause continued past
//! its source after a comma re-enters as a clause of its own instead of riding inside the
//! retrieval's object. A clause with neither keeps its reading.

use super::read;
use crate::plan::Op;

fn reads(reading: &super::Reading, file: &str) -> bool {
    reading
        .plan
        .steps
        .iter()
        .any(|s| s.op == Op::Read && s.detail == file)
}

#[test]
fn an_identifier_before_a_structured_source_is_unknown_work_and_the_source_still_opened() {
    for (intent, clause, id, file) in [
        (
            "Read the stock of item Z-31 from ./inventory.json and write it to ./out/stock.txt.",
            "Read the stock of item Z-31 from ./inventory.json",
            "Z-31",
            "./inventory.json",
        ),
        (
            "Leggi la scheda M-8 da ./schede.csv e scrivila in ./out/scheda.txt.",
            "Leggi la scheda M-8 da ./schede.csv",
            "M-8",
            "./schede.csv",
        ),
    ] {
        let reading = read(intent);
        let [unknown] = reading.plan.unknowns.as_slice() else {
            panic!("{intent}: {reading:#?}");
        };
        for words in [clause, id, file] {
            assert!(unknown.contains(words), "{intent}: {unknown}");
        }
        assert!(reads(&reading, file), "{intent}: {reading:#?}");
        assert!(!reading.hot_rejections().is_empty(), "{intent}");
    }
}

#[test]
fn a_bare_number_or_a_text_source_keeps_its_plain_read() {
    for (intent, file) in [
        (
            "Read ./prices.json and write it to ./out/copy.json.",
            "./prices.json",
        ),
        (
            "Read the Q3 notes in ./q3.md and write them to ./out/q3.md.",
            "./q3.md",
        ),
        (
            "Read the 2026 totals from ./totals.json and write them to ./out/t.json.",
            "./totals.json",
        ),
    ] {
        let reading = read(intent);
        assert!(reading.unresolved.is_empty(), "{intent}: {reading:#?}");
        assert!(reads(&reading, file), "{intent}: {reading:#?}");
    }
}

#[test]
fn a_retrieval_continued_past_its_source_defers_the_continuation() {
    // Ambiguous (« entry », « voce ») or settled by a cue (« record »), the retrieval's object
    // ends at its source either way.
    for (intent, kept, rest) in [
        (
            "Find entry Q-12 in ./clients.json, keep only its phone field, and write it to ./out/phone.txt.",
            "entry Q-12 in ./clients.json",
            "keep only its phone field",
        ),
        (
            "Find record Q-12 in ./clients.json, keep only its phone field, and write it to ./out/phone.txt.",
            "record Q-12 in ./clients.json",
            "keep only its phone field",
        ),
        (
            "Trova la voce M-8 in ./schede.json, tieni solo il suo campo stato e scrivilo in ./out/stato.txt.",
            "la voce M-8 in ./schede.json",
            "tieni solo il suo campo stato",
        ),
    ] {
        let reading = read(intent);
        let retrievals: Vec<&String> = reading
            .plan
            .steps
            .iter()
            .filter(|s| matches!(s.op, Op::Search | Op::Lookup))
            .map(|s| &s.detail)
            .chain(reading.ambiguous.iter().map(|a| &a.detail))
            .collect();
        assert_eq!(retrievals, [kept], "{intent}: {reading:#?}");
        assert!(
            reading.seen.iter().any(|c| c == rest),
            "{intent}: {reading:#?}"
        );
        // A bounded seat may settle a reading only when nothing but the ambiguity remains.
        let mut why = reading.hot_rejections();
        why.extend(crate::hot::rejections(intent, &reading));
        assert!(
            !why.iter().all(|w| w.contains("ambiguous clause")),
            "{intent}: {why:?}"
        );
    }
}

#[test]
fn a_retrieval_whose_object_follows_its_source_keeps_it() {
    let intent = "Cherche dans ./notes.json les lignes qui mentionnent rollback et écris-les dans ./out/r.txt.";
    let reading = read(intent);
    let details = reading
        .plan
        .steps
        .iter()
        .map(|s| &s.detail)
        .chain(reading.ambiguous.iter().map(|a| &a.detail));
    assert!(
        details.into_iter().any(|d| d.contains("rollback")),
        "{reading:#?}"
    );
    assert!(
        !reading.seen.iter().any(|c| c.starts_with("les lignes")),
        "{reading:#?}"
    );
}
