// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::workspace::text::own;

/// The keys a phrase names, with the word a French keyboard gives each: the
/// same key in the same number in the French, never dropped by a phrase.
const KEYS: &[(&str, &str)] = &[
    ("Enter", "Entrée"),
    ("Esc", "Échap"),
    ("Space", "Espace"),
    ("↑↓", "↑↓"),
    ("Ctrl+A", "Ctrl+A"),
    ("Ctrl+C", "Ctrl+C"),
    ("Ctrl+O", "Ctrl+O"),
    ("Alt+", "Alt+"),
    ("F2", "F2"),
    ("F6", "F6"),
    ("PgUp/PgDn", "PgUp/PgDn"),
    ("End", "End"),
];

/// The words the Session reads when typed, kept as typed in the French.
const TYPED: &[&str] = &[
    "/help",
    "/show",
    "/providers",
    "/intelligence",
    "save & run",
    "details",
];

/// The values a phrase sets in after it is said (`{act}`, `{place}`), in order.
fn values(phrase: &str) -> Vec<&str> {
    let mut values: Vec<&str> = (phrase.match_indices('{'))
        .filter_map(|(at, _)| {
            let end = phrase[at..].find('}')?;
            Some(&phrase[at..=at + end])
        })
        .collect();
    values.sort_unstable();
    values
}

/// Every French phrase says its own English: never a copy of it, the same
/// cues (` · ` and rows) so a narrow row gives way at the same places, every
/// key it names and every typed word the Session reads; and each English
/// phrase has one French.
#[test]
fn every_french_phrase_keeps_the_cues_keys_and_typed_words_of_its_english() {
    let mut seen = std::collections::BTreeSet::new();
    for &(en, fr) in FR {
        assert!(seen.insert(en), "one French per English: {en:?}");
        assert_ne!(en, fr, "a French phrase is no copy of its English");
        assert_eq!(
            en.matches(" · ").count(),
            fr.matches(" · ").count(),
            "{en:?} / {fr:?}"
        );
        assert_eq!(en.matches('\n').count(), fr.matches('\n').count(), "{en:?}");
        for &(key, french) in KEYS {
            assert_eq!(
                en.matches(key).count(),
                fr.matches(french).count(),
                "{key} in {en:?} / {fr:?}"
            );
        }
        for typed in TYPED {
            // A key cue naming what Enter opens is no typed `details`.
            let cue = *typed == "details" && en.contains("Enter details");
            assert_eq!(
                en.contains(typed) && !cue,
                fr.contains(typed),
                "{typed} in {en:?} / {fr:?}"
            );
        }
        assert_eq!(values(en), values(fr), "the values set in {en:?} / {fr:?}");
        // The Session reads `oui`, `non` and `annuler`: a phrase that names its
        // English twin names the French word.
        for (said, typed) in [("yes", "oui"), ("cancel", "annuler")] {
            // A state (`cancelled`) or a key's effect (`cancels`) is no typed word.
            if en.contains(said) && !en.contains("cancels") && !en.contains("cancelled") {
                assert!(fr.contains(typed), "{typed} for {said} in {fr:?}");
            }
        }
    }
}

/// English stays the default and the fallback: an English voice says every
/// phrase as written, and a French one says a phrase the table does not hold
/// unchanged, never another phrase.
#[test]
fn english_is_the_default_and_the_fallback() {
    let english = Voice::default();
    assert_eq!(english.lang, Lang::En);
    let french = Voice {
        lang: Lang::Fr,
        ascii: false,
    };
    for &(en, fr) in FR {
        assert_eq!(english.say(en), en);
        assert_eq!(
            Voice {
                ascii: true,
                ..english
            }
            .say(en),
            en
        );
        assert_eq!(french.say(en), fr);
    }
    for unknown in [
        "",
        "Draft ● · Saved ○",
        "« the person's words »",
        "claude-sonnet-5-5",
    ] {
        assert_eq!(french.say(unknown), unknown);
    }
}

/// Under the ASCII glyph column a French phrase loses its accents and the
/// renderer's marks take their twins: it is as plain as its English there
/// (a state glyph either keeps, such as a current phase's `●`, aside). A
/// notice stored in that column (its English twin) finds the same French.
#[test]
fn an_ascii_french_phrase_is_plain_ascii() {
    let plain = Voice {
        lang: Lang::Fr,
        ascii: true,
    };
    for &(en, fr) in FR {
        let shown = own(plain.say(en), true);
        let marks = |text: &str| text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        assert_eq!(marks(&shown), marks(&own(en, true)), "{shown:?}");
        assert_eq!(plain.say(en), fold(fr));
        assert_eq!(own(plain.say(&own(en, true)), true), shown, "{en:?}");
    }
    assert_eq!(
        fold("Échap à côté · «œuvre» çà"),
        "Echap a cote · \"oeuvre\" ca"
    );
}

/// A count reads as each language writes it: the English plural past one, the
/// French singular below two; a unit the table does not hold keeps the
/// English rule, and ASCII folds the French.
#[test]
fn a_count_reads_as_each_language_writes_it() {
    let english = Voice::default();
    let french = Voice {
        lang: Lang::Fr,
        ascii: false,
    };
    let plain = Voice {
        ascii: true,
        ..french
    };
    for (count, en, fr) in [
        (0, "0 rows", "0 ligne"),
        (1, "1 row", "1 ligne"),
        (3, "3 rows", "3 lignes"),
    ] {
        assert_eq!(english.count(count, "row"), en);
        assert_eq!(french.count(count, "row"), fr);
    }
    assert_eq!(english.count(1, "match"), "1 match");
    assert_eq!(english.count(17, "match"), "17 matches");
    assert_eq!(french.count(17, "match"), "17 résultats");
    assert_eq!(plain.count(1, "match"), "1 resultat");
    assert_eq!(french.count(2, "widget"), "2 widgets");
    assert_eq!(
        english.count(3, "more step not listed"),
        "3 more steps not listed"
    );
    assert_eq!(
        french.count(3, "more step not listed"),
        "3 étapes de plus non listées"
    );
    assert_eq!(english.count(2, "task ran"), "2 tasks ran");
    assert_eq!(french.count(1, "task ran"), "1 tâche exécutée");
    assert_eq!(english.count(2, "skipped"), "2 skipped");
    assert_eq!(french.count(2, "skipped"), "2 ignorées");
}
