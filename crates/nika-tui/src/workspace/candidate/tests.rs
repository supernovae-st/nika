// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate's faces say whose bytes they are: the identity a yes answers, what it
//! creates or replaces, what it reaches, its rehearsal, the witness of the pending bytes, in
//! every face, at every width, in both glyph columns. A set aside one is never offered.

use super::*;
use nika_session::change::Witness;

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
        assert!(title.contains("proposal · copy-brief"), "{title}");
        assert!(title.contains(&format!("[{}]", face.label())), "{title}");
        let body = rows(&body);
        let text = body.join("\n");
        for needle in [
            format!("proposal {id} · what a yes answers · not saved"),
            "creates copy-brief.nika".to_owned(),
            "when it runs · reads ./notes/brief.md".to_owned(),
            "when it runs · writes ./out/copy.md".to_owned(),
            "rehearsal · no proof is bound to this identity".to_owned(),
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
    assert!(!text.contains("no proof is bound"), "{text}");
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
