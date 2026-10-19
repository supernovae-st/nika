// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::expect_used)]

use super::*;
use crate::workspace::text::own;

/// The values a phrase sets in after it is said, in order.
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

/// Every French phrase of the view says its own English: never a copy, the
/// same cues and values set in, the same typed words and keys; each English
/// phrase has one French; English stays the default and the fallback; under
/// ASCII a phrase is as plain as its English.
#[test]
fn every_view_phrase_keeps_its_english_cues_values_and_typed_words() {
    let mut seen = std::collections::BTreeSet::new();
    for &(en, fr) in FR {
        assert!(seen.insert(en), "one French per English: {en:?}");
        assert_ne!(en, fr);
        assert_eq!(
            en.matches(" · ").count(),
            fr.matches(" · ").count(),
            "{en:?}"
        );
        assert_eq!(values(en), values(fr), "{en:?} / {fr:?}");
        for typed in ["`/show`", "F2"] {
            assert_eq!(en.contains(typed), fr.contains(typed), "{typed} in {fr:?}");
        }
        assert_eq!(say(Lang::En, false, en), en);
        assert_eq!(say(Lang::Fr, false, en), fr);
        let marks = |text: &str| text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let plain = own(say(Lang::Fr, true, en), true);
        assert_eq!(marks(&plain), marks(&own(en, true)), "{plain:?}");
    }
    assert_eq!(
        say(Lang::Fr, false, "reads ./notes/brief.md"),
        "reads ./notes/brief.md"
    );
}

/// A proposal's review in French: its headings and the identity on its border
/// are the view's French, the Session's facts after each heading keep their
/// words; under ASCII the French loses its accents; English keeps English.
#[test]
fn a_french_review_names_its_headings_and_keeps_the_sessions_facts() {
    use crate::workspace::candidate::Proposed;
    use crate::workspace::inspect::Inspected;
    use nika_session::ProposalId;
    let source = "nika: copy\n".to_owned();
    let witness = "51835c93e564aaaabbbbccccddddeeeeffff0000".to_owned();
    let look = Inspected::unjudged("copy.nika", witness, source.clone());
    let id = ProposalId::of(&source);
    let shown = |lang: Lang, ascii: bool| {
        let proposed = (Proposed::new(id.clone(), false, look.clone()))
            .reaching(Some(vec!["reads ./notes/brief.md".to_owned()]))
            .speaking(lang);
        let review = proposed.review(ascii).expect("a consent can name it");
        let rows: Vec<String> = (review.lines(false, ascii, 80).iter())
            .map(ToString::to_string)
            .collect();
        (rows, review.foot(80, ascii))
    };
    let (rows, foot) = shown(Lang::En, false);
    assert_eq!(
        rows,
        [
            "when it runs · reads ./notes/brief.md",
            "rehearsal · none bound to this identity"
        ]
    );
    assert_eq!(foot, "proposal copy.nika · F2: whole words");
    let (rows, foot) = shown(Lang::Fr, false);
    assert_eq!(
        rows,
        [
            "à l'exécution · reads ./notes/brief.md",
            "répétition · aucune liée à cette identité"
        ]
    );
    assert_eq!(foot, "proposition copy.nika · F2 : tous les mots");
    let (rows, foot) = shown(Lang::Fr, true);
    assert_eq!(
        rows,
        [
            "a l'execution - reads ./notes/brief.md",
            "repetition - aucune liee a cette identite"
        ]
    );
    assert_eq!(foot, "proposition copy.nika - F2 : tous les mots");
}

/// A declaration's review says its schedule in the Session's words under the view's own
/// heading, its missed run and its ceiling hung under the cadence, and no « when it runs » row
/// for a file that runs nothing itself; the heading follows the person's language.
#[test]
fn a_declarations_review_says_its_schedule_under_the_views_heading() {
    use crate::workspace::candidate::Proposed;
    use crate::workspace::inspect::Inspected;
    use nika_session::ProposalId;
    use nika_session::work::ScheduleWords;
    let source = "arm:\n  - workflow: digest.nika\n".to_owned();
    let witness = "61835c93e564aaaabbbbccccddddeeeeffff0000".to_owned();
    let look = Inspected::unjudged("nika.yaml", witness, source.clone());
    let schedule = |lang: Lang| {
        let (cadence, missed, ceiling) = match lang {
            Lang::Fr => (
                "tous les jours à 8h (heure de Paris)",
                "si le Mac est éteint : le rattraper une fois",
                "plafond 0,20 $ par run",
            ),
            _ => (
                "every day at 08:00 (Paris time)",
                "if the Mac is off: catch up once",
                "ceiling $0.20 per run",
            ),
        };
        ScheduleWords::new(
            "digest.nika".into(),
            cadence.into(),
            missed.into(),
            ceiling.into(),
        )
    };
    let rows = |lang: Lang| -> Vec<String> {
        let proposed = (Proposed::new(ProposalId::of(&source), false, look.clone()))
            .changing(vec!["creates nika.yaml (2 lines)".to_owned()])
            .scheduling(Some(schedule(lang)))
            .speaking(lang);
        let review = proposed.review(false).expect("a consent can name it");
        (review.lines(false, false, 100).iter())
            .map(ToString::to_string)
            .collect()
    };
    assert_eq!(
        rows(Lang::En),
        [
            "creates nika.yaml (2 lines)",
            "schedule · digest.nika · every day at 08:00 (Paris time)",
            "  if the Mac is off: catch up once",
            "  ceiling $0.20 per run",
            "rehearsal · none bound to this identity",
        ]
    );
    assert_eq!(
        rows(Lang::Fr)[1..4],
        [
            "planification · digest.nika · tous les jours à 8h (heure de Paris)",
            "  si le Mac est éteint : le rattraper une fois",
            "  plafond 0,20 $ par run",
        ]
    );
}

/// The aside and the header say their own words in the person's language: the
/// region, the projections, the headings, the verdicts and the footer; what a
/// location is and its facts; the selected intelligence's label and that none
/// is selected. Names, paths, hosts and the entries' own words keep theirs;
/// under ASCII the French loses its accents; English keeps English.
#[test]
fn the_aside_and_the_header_say_their_own_words_in_french() {
    use crate::visual::icon::Icon;
    use crate::workspace::aside::{self, Aside, Entry, Tab, Verdict};
    use crate::workspace::header::{self, Place};
    use crate::workspace::masthead;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    let entries = vec![
        Entry::new(Icon::Conversation, "cette conversation"),
        Entry::new(Icon::Workflow, "enrich.nika").judged(Verdict::Findings(2)),
        Entry::new(Icon::Memory, "notes"),
    ];
    let shown = |lang: Lang, ascii: bool| -> Vec<String> {
        let aside = Aside::new("studio", Tab::Nika, entries.clone(), false).speaking(lang);
        (aside::lines(&aside, 40, 14, ascii, false).iter())
            .map(|line| line.to_string().trim_end().to_owned())
            .collect()
    };
    let english = shown(Lang::En, false);
    assert_eq!(english[0], "Project");
    assert_eq!(english[1], "Nika · Files");
    assert!(english.iter().any(|row| row == "MEMORY"), "{english:#?}");
    assert!(
        english.iter().any(|row| row.ends_with("2 findings")),
        "{english:#?}"
    );
    assert!(
        english.iter().any(|row| row == "partial listing"),
        "{english:#?}"
    );
    let french = shown(Lang::Fr, false);
    assert_eq!(french[0], "Projet");
    assert_eq!(french[1], "Nika · Fichiers");
    assert!(french.iter().any(|row| row == "MÉMOIRE"), "{french:#?}");
    assert!(
        french.iter().any(|row| row.ends_with("2 constats")),
        "{french:#?}"
    );
    assert!(
        french.iter().any(|row| row == "liste partielle"),
        "{french:#?}"
    );
    assert!(
        french.iter().any(|row| row.contains("enrich.nika")),
        "{french:#?}"
    );
    let plain = shown(Lang::Fr, true);
    assert!(plain.iter().any(|row| row == "MEMOIRE"), "{plain:#?}");
    // The header: what the location is and its facts; the host and the path as they are.
    let place = Place::on("local")
        .with_project("studio", "~/studio")
        .observed(false, false);
    let rows = |place: &Place| -> Vec<String> {
        (header::lines(place, 80, 2, false, false).iter())
            .map(|line| line.to_string().trim_end().to_owned())
            .collect()
    };
    let english = rows(&place);
    assert!(
        english[0].ends_with("studio  local project"),
        "{english:#?}"
    );
    assert_eq!(english[1], "  ~/studio · no git · no nika.yaml");
    let french = rows(&place.clone().speaking(Lang::Fr));
    assert!(french[0].ends_with("studio  projet local"), "{french:#?}");
    assert_eq!(french[1], "  ~/studio · pas de git · pas de nika.yaml");
    let none = rows(&Place::on("local").speaking(Lang::Fr));
    assert!(none[0].contains("aucun projet"), "{none:#?}");
    // The masthead: the label of the intelligence selected, or that none is.
    let painted = |place: &Place, seat: Option<&str>| -> String {
        let area = Rect::new(0, 0, 120, 2);
        let mut buf = Buffer::empty(area);
        masthead::render((place, seat), area, None, &mut buf, (false, false));
        (0..120).map(|x| buf[(x, 0)].symbol()).collect::<String>()
    };
    let french = place.clone().speaking(Lang::Fr);
    assert!(painted(&french, Some("vllm/s03-seat")).contains("Préparer avec : vllm/s03-seat"));
    assert!(painted(&french, None).contains("Préparer avec : non choisie · /intelligence"));
    assert!(painted(&place, Some("vllm/s03-seat")).contains("Prepare with: vllm/s03-seat"));
}

/// A workflow's face says the viewer's own words in the canvas's language: the
/// faces' names, the facts above the face (the format, the face, the size and
/// the line count), the rows around it, a note about what was cut and the
/// check's reasons; the bytes keep theirs. Counts take their French forms (one
/// for 0 and 1); under ASCII the French loses its accents; English keeps
/// English, and a noun the units do not hold keeps the English rule.
#[test]
fn a_workflow_face_says_its_own_words_in_french() {
    use crate::workspace::inspect::Inspected;
    use crate::{Canvas, Face, Note};
    let witness = "51835c93e564aaaabbbbccccddddeeeeffff0000".to_owned();
    let look = Inspected::unjudged("copy.nika", witness, "nika: copy\n".to_owned());
    let rows = |lang: Lang, ascii: bool, face: Face| {
        let (title, body) = look
            .clone()
            .speaking(lang)
            .face_lines(face, 120, ascii, false);
        let mut rows = vec![title.to_string()];
        rows.extend(body.iter().map(ToString::to_string));
        rows
    };
    let english = rows(Lang::En, false, Face::Source);
    assert_eq!(english[0], "copy.nika · [source] plan graph check");
    assert_eq!(english[1], "Nika workflow · source · 11 bytes · 1 line");
    assert_eq!(
        english[2],
        "this file's bytes, as last read · r reads it again · Left/Right change the face"
    );
    let french = rows(Lang::Fr, false, Face::Source);
    assert_eq!(french[0], "copy.nika · [source] plan graphe vérification");
    assert_eq!(french[1], "workflow Nika · source · 11 octets · 1 ligne");
    assert_eq!(
        french[2],
        "les octets de ce fichier, tels que lus en dernier · r le relit · Gauche/Droite changent la face"
    );
    assert!(
        french.iter().any(|row| row.ends_with("nika: copy")),
        "{french:#?}"
    );
    let check = rows(Lang::Fr, false, Face::Check).join("\n");
    assert!(
        check.contains("VALID        non jugé : rien n'a encore vérifié ces octets"),
        "{check}"
    );
    let plain = rows(Lang::Fr, true, Face::Check);
    assert_eq!(plain[0], "copy.nika - source plan graphe [verification]");
    assert_eq!(
        plain[1],
        "workflow Nika - verification - 11 octets - 0 constat"
    );
    assert!(plain.iter().all(|row| row.is_ascii()), "{plain:#?}");
    let cut = Note::WidthCut {
        lines: 7,
        width: 65,
    };
    let french = Canvas::new(80, false, false).speaking(Lang::Fr);
    assert_eq!(
        cut.text(false),
        "7 lines wider than 65 columns, cut at the edge"
    );
    assert_eq!(
        cut.said(french),
        "7 lignes plus larges que 65 colonnes, coupées au bord"
    );
    let one = Note::WidthCut {
        lines: 1,
        width: 65,
    };
    assert_eq!(
        one.said(french),
        "1 ligne plus large que 65 colonnes, coupée au bord"
    );
    assert_eq!(count(Lang::Fr, false, 4, "task"), "4 tâches");
    assert_eq!(count(Lang::Fr, false, 0, "finding"), "0 constat");
    assert_eq!(count(Lang::Fr, true, 3, "edge"), "3 aretes");
    assert_eq!(count(Lang::En, false, 3, "edge"), "3 edges");
    assert_eq!(count(Lang::Fr, false, 2, "operation"), "2 operations");
}

/// The conversation panel names the next message's recipient and says what it
/// carries in the thread's language; names and references keep theirs, and
/// under ASCII the French loses its accents.
#[test]
fn the_panel_names_the_recipient_and_what_is_attached_in_french() {
    use crate::workspace::conversation::{self, Thread};
    let thread = Thread::new("studio", "notes du lundi")
        .viewing("release.nika")
        .attaching("brief.md");
    assert_eq!(
        conversation::placeholder(&thread),
        "Message to studio / notes du lundi"
    );
    let text = |thread: &Thread, ascii: bool| {
        (conversation::context(thread, 80, ascii, false).spans.iter())
            .map(|span| span.content.to_string())
            .collect::<String>()
    };
    assert_eq!(
        text(&thread, false),
        "Attached: brief.md · viewing release.nika"
    );
    let french = thread.speaking(Lang::Fr);
    assert_eq!(
        conversation::placeholder(&french),
        "Message pour studio / notes du lundi"
    );
    assert_eq!(
        text(&french, false),
        "Joint : brief.md · à l'écran release.nika"
    );
    assert_eq!(
        text(&french, true),
        "Joint : brief.md - a l'ecran release.nika"
    );
    let alone = Thread::new("", "cette conversation").speaking(Lang::Fr);
    assert_eq!(
        conversation::placeholder(&alone),
        "Message pour cette conversation"
    );
}

/// A face's key cues say the keys that work from where the person types: the
/// object holding the keys names them (`r`, `Left/Right`), the box names the key
/// that brings them first (`F6 then …`), in both languages; the object holds
/// them by default.
#[test]
fn a_faces_key_cues_name_the_keys_from_where_they_are() {
    use crate::Face;
    use crate::workspace::candidate::Proposed;
    use crate::workspace::inspect::Inspected;
    use nika_session::ProposalId;
    let witness = "51835c93e564aaaabbbbccccddddeeeeffff0000".to_owned();
    let look = Inspected::unjudged("copy.nika", witness, "nika: copy\n".to_owned());
    let row = |look: Inspected| {
        let (_, body) = look.face_lines(Face::Source, 140, false, false);
        body[1].to_string()
    };
    let here = "this file's bytes, as last read · r reads it again · Left/Right change the face";
    assert_eq!(row(look.clone()), here);
    assert_eq!(row(look.clone().keys_here(true)), here);
    assert_eq!(
        row(look.clone().keys_here(false)),
        "this file's bytes, as last read · F6 then r reads it again · Left/Right change the face"
    );
    assert_eq!(
        row(look.clone().speaking(Lang::Fr).keys_here(false)),
        "les octets de ce fichier, tels que lus en dernier · F6 puis r le relit · Gauche/Droite changent la face"
    );
    let proposed = Proposed::new(ProposalId::of("nika: copy\n"), false, look);
    let cue = |proposed: Proposed| {
        let (_, body) = proposed.reviewed_face_lines(Face::Source, 140, false, false, false);
        body[0].to_string()
    };
    assert_eq!(
        cue(proposed.clone()),
        "these bytes, the proposal's own · Left/Right change the face"
    );
    assert_eq!(
        cue(proposed.clone().keys_here(false)),
        "these bytes, the proposal's own · F6 then Left/Right change the face"
    );
    assert_eq!(
        cue(proposed.speaking(Lang::Fr).keys_here(false)),
        "ces octets, ceux de la proposition · F6 puis Gauche/Droite changent la face"
    );
}

/// Where a face is narrower than the row saying whose bytes it shows (the object at about 120
/// columns of the terminal), the row wraps at its words rather than cut the keys at its end:
/// the cue that turns or reads the face again stays whole, every row within the face.
#[test]
fn a_face_cue_wraps_whole_where_the_face_is_narrow() {
    use crate::Face;
    use crate::workspace::inspect::Inspected;
    use unicode_width::UnicodeWidthStr;
    let witness = "51835c93e564aaaabbbbccccddddeeeeffff0000".to_owned();
    let look =
        Inspected::unjudged("copy.nika", witness, "nika: copy\n".to_owned()).keys_here(false);
    let cue =
        "this file's bytes, as last read · F6 then r reads it again · Left/Right change the face";
    for face in [Face::Source, Face::Plan, Face::Check] {
        let (_, body) = look.face_lines(face, 52, false, false);
        let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
        assert!(
            rows.iter().all(|row| row.width() <= 52),
            "{face:?}: {rows:#?}"
        );
        let read = rows
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(read.contains(cue), "{face:?}: {rows:#?}");
    }
}
