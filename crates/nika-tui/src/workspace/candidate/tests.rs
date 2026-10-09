// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate's faces say whose bytes they are: the identity a yes answers, what it
//! creates or replaces, what it reaches, its rehearsal, the witness of the pending bytes, in
//! every face, at every width, in both glyph columns. A set aside one is never offered. Its
//! review keeps only what changes the decision, each continuation hung under its fact.

use super::*;
use nika_session::change::Witness;
use unicode_width::UnicodeWidthStr;

const SOURCE: &str = "nika: copy-brief\npermits:\n  fs: { read: [\"./notes/brief.md\"], write: [\"./out/copy.md\"] }\n  tools: [\"nika:read\", \"nika:write\"]\ntasks:\n  read_source:\n    invoke: { tool: \"nika:read\", args: { path: \"./notes/brief.md\" } }\n  write_output:\n    with: { text: \"${{ tasks.read_source.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/copy.md\", content: \"${{ with.text }}\" } }\n";

fn fold(preview: &str, aside: bool, rehearsed: Option<&str>) -> Proposed {
    let witness = Witness::of(SOURCE.as_bytes()).0;
    let look = crate::session::judge_for_tests("copy-brief.nika", witness, SOURCE);
    Proposed::new(ProposalId::of(preview), aside, look)
        .changing(vec!["creates copy-brief.nika · 10 lines · new".to_owned()])
        .reaching(Some(vec![
            "reads ./notes/brief.md".to_owned(),
            "writes ./out/copy.md".to_owned(),
        ]))
        .rehearsed(rehearsed.map(str::to_owned))
}

fn rows(lines: &[Line<'static>]) -> Vec<String> {
    lines.iter().map(ToString::to_string).collect()
}

#[test]
fn every_face_names_the_identity_the_changes_the_reach_and_the_bytes() {
    let candidate = fold("preview A", false, None);
    let id = ProposalId::of("preview A").to_string();
    let short: String = Witness::of(SOURCE.as_bytes()).0.chars().take(12).collect();
    for face in Face::ALL {
        let (title, body) = candidate.face_lines(face, 100, false, false);
        let title = title.to_string();
        // The title names the workflow; the first row says what it is.
        assert!(title.starts_with("copy-brief"), "{title}");
        assert!(!title.contains("proposal"), "{title}");
        assert!(title.contains(&format!("[{}]", face.label())), "{title}");
        let body = rows(&body);
        let text = body.join("\n");
        assert!(body[0].starts_with(&format!("proposal {id} · ")), "{text}");
        for needle in [
            format!("proposal {id} · what a yes answers · not saved"),
            "creates copy-brief.nika".to_owned(),
            "when it runs · reads ./notes/brief.md".to_owned(),
            "when it runs · writes ./out/copy.md".to_owned(),
            "rehearsal · none bound to this identity".to_owned(),
            format!("these bytes {short}, the proposal's own"),
        ] {
            assert!(text.contains(&needle), "{face:?}: {needle}\n{text}");
        }
        assert!(
            !text.contains("r reads it again"),
            "no file to read: {text}"
        );
    }
    let (_, source) = candidate.face_lines(Face::Source, 100, false, false);
    assert!(rows(&source).join("\n").contains("nika: copy-brief"));
    let (_, check) = candidate.face_lines(Face::Check, 100, false, false);
    let check = rows(&check).join("\n");
    assert!(check.contains("IMPORTS"), "judged alone: {check}");
    assert!(check.contains("RUN READY"), "{check}");
    assert!(!check.contains("nothing known blocks a run"), "{check}");
}

/// The review holds only what changes the decision: every change and the
/// changes no face shows, one `when it runs` heading with each further effect
/// two cells in, the rehearsal. The standing (the status row's) and the
/// identity (the card's foot) take no row of their own; the foot names the
/// exact identity a consent names and the key of the Session's whole words,
/// whole or in a shorter whole form. Every fact once, each with its role; a
/// draft or set aside candidate has no review.
#[test]
fn the_review_holds_only_what_changes_the_decision() {
    let candidate = fold("preview A", false, None).unshown(1);
    let id = ProposalId::of("preview A").to_string();
    let review = candidate.review(false).expect("a consent can name it");
    let unshown =
        "1 more change(s) whose bytes these faces do not show · `/show` prints every byte";
    let expected = [
        "creates copy-brief.nika · 10 lines · new",
        unshown,
        "when it runs · reads ./notes/brief.md",
        "  writes ./out/copy.md",
        "rehearsal · none bound to this identity",
    ];
    assert_eq!(rows(&review.lines(false, false, 100)), expected);
    let styled = review.lines(true, false, 100);
    for (at, tone) in [
        (0, Role::Accent),
        (1, Role::Warn),
        (2, Role::Dim),
        (3, Role::Dim),
        (4, Role::Dim),
    ] {
        assert_eq!(styled[at].style, role::style(tone, true), "row {at}");
    }
    let whole = format!("proposal {id} · F2: whole words");
    assert_eq!(review.foot(usize::MAX, false), whole);
    assert_eq!(review.foot(whole.width(), false), whole);
    let short = format!("proposal {id} · F2");
    assert_eq!(review.foot(whole.width() - 1, false), short);
    assert_eq!(review.foot(short.width() - 1, false), format!("{id} · F2"));
    assert_eq!(review.foot(0, true), format!("{id} - F2"));
    let twin = candidate.review(true).expect("a review");
    let ascii = rows(&twin.lines(false, true, 100));
    assert!(ascii.iter().all(|row| row.is_ascii()), "{ascii:#?}");
    assert_eq!(ascii[2], "when it runs - reads ./notes/brief.md");
    let twinned = format!("proposal {id} - F2: whole words");
    assert_eq!(twin.foot(80, true), twinned);
    for review in [&review, &twin] {
        let text = rows(&review.lines(false, false, 100)).join("\n");
        let gone = ["what a yes answers", "not saved", "Full proposal", "F2"];
        for gone in gone.into_iter().chain([id.as_str()]) {
            assert!(!text.contains(gone), "{gone}: {text}");
        }
    }
    assert!(!review.runs(), "no save & run admitted");
    let aside = fold("preview A", true, None);
    assert!(aside.review(false).is_none(), "set aside");
    let draft = fold("preview A", false, None).drafted();
    assert!(draft.review(false).is_none(), "a draft");
}

/// Each fact starts at the margin, each further `when it runs` effect two
/// cells in and every wrapped continuation four cells in, so a continuation
/// never reads as a fact or an effect of its own; a count keeps its unit, no
/// word is cut and every row holds in its cells, at the card's words of 36,
/// 40, 56, 64 and 76 cells, in both glyph columns.
#[test]
fn every_review_row_hangs_its_continuations_within_the_card() {
    let look = crate::session::judge_for_tests("aggregate-by-key.nika", "w".repeat(64), SOURCE);
    let facts = [
        "creates aggregate-by-key.nika (61\u{a0}lines)",
        "tools nika:assert · nika:jq · nika:validate",
        "model output estimate · $0 · no direct model task in these checked bytes",
        "local only · nothing outside the process",
    ];
    let candidate = Proposed::new(ProposalId::of("preview H"), false, look)
        .changing(vec![facts[0].to_owned()])
        .reaching(Some(vec![facts[1].to_owned(), facts[2].to_owned()]))
        .declaring(vec![(facts[3].to_owned(), false)]);
    for width in [36_u16, 40, 56, 64, 76] {
        for ascii in [false, true] {
            let at = format!("{width} ascii={ascii}");
            let review = candidate.review(ascii).expect("a review");
            let shown = rows(&review.lines(false, ascii, width));
            let starts = |words: &str| {
                let words = twins(words, ascii);
                (shown.iter()).position(|row| row.trim_start().starts_with(words.as_str()))
            };
            for row in &shown {
                assert!(row.width() <= usize::from(width), "{at}: {row}");
                let indent = row.len() - row.trim_start().len();
                assert!([0, 2, 4].contains(&indent), "{at}: {row}");
                assert!(!ascii || row.is_ascii(), "{at}: {row}");
            }
            let indent = |at: usize| shown[at].len() - shown[at].trim_start().len();
            for head in ["creates", "when it runs", "reaches,", "rehearsal"] {
                let row = starts(head).unwrap_or_else(|| panic!("{at}: {head}\n{shown:#?}"));
                assert_eq!(indent(row), 0, "{at}: {head}");
            }
            let member = starts("model output estimate").expect("the second effect");
            assert_eq!(indent(member), 2, "{at}: {shown:#?}");
            assert!(shown.iter().any(|row| row.contains("(61 lines)")), "{at}");
            let read = shown
                .iter()
                .map(|row| row.trim())
                .collect::<Vec<_>>()
                .join(" ");
            for words in [facts[1], facts[2]] {
                assert!(read.contains(&twins(words, ascii)), "{at}: {words}\n{read}");
            }
        }
    }
}

/// While the conversation reviews it, the object's header is the one home of
/// its identity: the title names the workflow, the first row says whose exact
/// bytes these are (the key that turns the face where it fits whole, never
/// cut), and no second witness row follows, in every face; the face the
/// review does not cover keeps its facts and its own witness row.
#[test]
fn the_reviewed_face_leads_with_its_own_bytes_once() {
    let candidate = fold("preview A", false, None);
    let short: String = Witness::of(SOURCE.as_bytes()).0.chars().take(12).collect();
    let own = format!("these bytes {short}, the proposal's own");
    for face in Face::ALL {
        for (width, first) in [(100, format!("{own} · {FACES}")), (50, own.clone())] {
            let (title, body) = candidate.reviewed_face_lines(face, width, false, false, false);
            let (title, body) = (title.to_string(), rows(&body));
            let at = format!("{face:?} {width}");
            assert!(title.starts_with("copy-brief"), "{at}: {title}");
            assert_eq!(body[0], first, "{at}");
            let bytes = format!("these bytes {short}");
            let witnesses = body.iter().filter(|row| row.contains(&bytes)).count();
            assert_eq!(witnesses, 1, "{at}: {body:#?}");
            assert!(!body.join("\n").contains("what a yes answers"), "{at}");
        }
        let (_, unreviewed) = candidate.face_lines(face, 100, false, false);
        let unreviewed = rows(&unreviewed);
        assert!(unreviewed[0].starts_with("proposal "), "{face:?}");
        let said = format!("{own} · {FACES}");
        assert!(unreviewed.contains(&said), "{face:?}: {unreviewed:#?}");
    }
}

/// A `save & run` the Session's typed method admits is carried with the
/// candidate: the run its own request asked names its run once, as a change
/// does, before the effects; the one workflow it saves adds no row; a method
/// that refuses leaves no such word for the decision row.
#[test]
fn a_carried_run_is_named_once_before_what_it_reaches() {
    let asked = "asked run · copy-brief.nika once · ceiling $0.25 · inputs region";
    let carried = fold("preview A", false, None).running(Some(RunAfter::Asked(asked.to_owned())));
    let review = carried.review(false).expect("a review");
    assert!(review.runs());
    let shown = rows(&review.lines(false, false, 100));
    let row = shown.iter().position(|row| row == asked);
    let row = row.expect("the carried run");
    let effects = (shown.iter()).position(|row| row.starts_with("when it runs"));
    assert!(effects.is_some_and(|effects| row < effects), "{shown:#?}");
    let styled = review.lines(true, false, 100);
    assert_eq!(styled[row].style, role::style(Role::Accent, true));
    let saved = fold("preview A", false, None).running(Some(RunAfter::Saved));
    let review = saved.review(false).expect("a review");
    assert!(review.runs());
    let text = rows(&review.lines(false, false, 100)).join("\n");
    assert!(!text.contains("asked run"), "{text}");
    assert_ne!(saved, fold("preview A", false, None), "another fact");
    assert_ne!(saved, carried, "another run");
}

#[test]
fn a_set_aside_candidate_is_never_offered_to_a_yes() {
    let candidate = fold("preview A", true, None);
    let (_, body) = candidate.face_lines(Face::Plan, 100, false, false);
    let text = rows(&body).join("\n");
    assert!(
        text.contains("set aside while the revision's question waits · not consentable now"),
        "{text}"
    );
    assert!(!text.contains("what a yes answers"), "{text}");
}

#[test]
fn the_rehearsal_words_bound_to_the_identity_are_shown_whole() {
    let words = "Rehearsed on a copy of your files · read back ./out/copy.md · 6 bytes\nnothing ran on the originals\n";
    let candidate = fold("preview A", false, Some(words));
    let (_, body) = candidate.face_lines(Face::Source, 60, false, false);
    let text = rows(&body).join(" ");
    assert!(
        text.contains("rehearsal · Rehearsed on a copy of your files"),
        "{text}"
    );
    assert!(text.contains("nothing ran on the originals"), "{text}");
    assert!(!text.contains("none bound to this identity"), "{text}");
}

#[test]
fn the_head_wraps_within_the_width_and_takes_the_ascii_twins() {
    let candidate = fold("preview A", false, Some("Rehearsed on a copy · « kept »"));
    for width in [40_u16, 60, 82] {
        for face in Face::ALL {
            let (title, body) = candidate.face_lines(face, width, true, false);
            for line in std::iter::once(&title).chain(body.iter()) {
                assert!(line.width() <= usize::from(width), "{width}: {line}");
            }
            let head: Vec<String> = rows(&body).into_iter().take(6).collect();
            assert!(head.iter().all(|row| row.is_ascii()), "{width}: {head:?}");
        }
    }
}

#[test]
fn a_fold_is_the_same_candidate_only_when_every_fact_is() {
    let a = fold("preview A", false, None);
    assert_eq!(a, fold("preview A", false, None));
    assert_ne!(a, fold("preview B", false, None), "another identity");
    assert_ne!(a, fold("preview A", true, None), "another standing");
    assert_ne!(
        a,
        fold("preview A", false, Some("rehearsed")),
        "another rehearsal"
    );
    assert_ne!(a, a.clone().reaching(None), "another reach");
    let runs = a.clone().running(Some(RunAfter::Saved));
    assert_ne!(a, runs, "another save & run");
    assert_eq!(a.path(), "copy-brief.nika");
    assert_eq!(a.label(), "proposal copy-brief.nika");
}

/// A proposal that lands more than the shown workflow says how many of its
/// changes have bytes the faces do not show.
#[test]
fn changes_whose_bytes_are_not_shown_are_counted() {
    let candidate = fold("preview A", false, None).unshown(2);
    let (_, body) = candidate.face_lines(Face::Source, 100, false, false);
    let text = rows(&body).join("\n");
    assert!(
        text.contains("2 more change(s) whose bytes these faces do not show"),
        "{text}"
    );
    assert_ne!(candidate, fold("preview A", false, None));
}

#[test]
fn the_first_preview_uses_only_an_observed_graph_and_keeps_the_faces_visible() {
    let candidate = fold("audited preview", false, None);
    assert_eq!(candidate.initial_face(), Face::Graph);
    for face in Face::ALL {
        let (title, _) = candidate.face_lines(face, 37, false, true);
        let text = title.to_string();
        for tab in Face::ALL {
            assert!(text.contains(tab.label()), "{text}");
        }
        assert!(text.contains(&format!("[{}]", face.label())), "{text}");
        assert!(title.width() <= 37);
    }
    for width in [0, 1, 8, 20] {
        let (title, _) = candidate.face_lines(Face::Graph, width, true, false);
        assert!(title.width() <= usize::from(width));
    }
    let unseen = Proposed::new(
        ProposalId::of("not audited"),
        false,
        Inspected::unjudged("draft.nika", "witness".to_owned(), "nika: draft".to_owned()),
    );
    assert_eq!(unseen.initial_face(), Face::Source);
}

/// A revision over the complete document is stated above every face with its components, and a
/// component no longer witnessed as bound is a warning row.
#[test]
fn a_document_revision_is_stated_with_each_component_and_its_witness() {
    let candidate = fold("preview R", false, None).revising(vec![
        ("revised in place · const.max_age_hours".to_owned(), false),
        (
            "component · block:stale-filter-report r1 · revised · const.max_age_hours = 72"
                .to_owned(),
            true,
        ),
    ]);
    for face in Face::ALL {
        let (_, body) = candidate.face_lines(face, 120, false, false);
        let text = rows(&body).join("\n");
        assert!(
            text.contains("revised in place · const.max_age_hours"),
            "{text}"
        );
        assert!(
            text.contains("component · block:stale-filter-report r1 · revised"),
            "{text}"
        );
    }
    let (_, ascii) = candidate.face_lines(Face::Source, 120, true, false);
    assert!(
        rows(&ascii)
            .join("\n")
            .contains("revised in place - const.max_age_hours"),
        "the ASCII column twins the separator"
    );
}

/// A draft lands nowhere yet: its label takes the title its object shows,
/// never the path it holds until a proposal says where, and says it is
/// unsaved without one; a proposal's label names where it lands.
#[test]
fn a_draft_takes_its_object_title_never_its_placeholder_path() {
    let witness = Witness::of(SOURCE.as_bytes()).0;
    let named = crate::session::judge_for_tests("draft.nika", witness, SOURCE);
    let draft = Proposed::new(ProposalId::of("draft"), false, named).drafted();
    assert_eq!(draft.label(), "draft copy-brief");
    let bare = Inspected::unjudged("draft.nika", "w".to_owned(), "nika: draft".to_owned());
    let unsaved = Proposed::new(ProposalId::of("draft"), false, bare.clone()).drafted();
    assert_eq!(unsaved.label(), "draft (unsaved)");
    let proposal = Proposed::new(ProposalId::of("p"), false, bare);
    assert_eq!(proposal.label(), "proposal draft.nika");
}
