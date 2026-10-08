// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::json;

use super::{DocumentRevision, Edit, Origin, Path, Revision, RevisionRefusal};

const BASE: &str = "nika: window\n# the threshold\nconst:\n  window_hours: 48\ntasks:\n  t:\n    exec:\n      command: [\"echo\", \"${{ const.window_hours }}\"]\n";

fn hours(value: i64) -> Edit {
    Edit::set(Path::new(["const", "window_hours"]), json!(value))
}

#[test]
fn a_revision_is_the_sha256_of_its_exact_bytes() {
    let first = DocumentRevision::import(BASE).expect("import");
    assert_eq!(first.revision(), &Revision::of(BASE));
    assert_eq!(first.revision().as_str().len(), 64);
    assert_eq!(
        Revision::parse(first.revision().as_str()).as_ref(),
        Some(first.revision())
    );
    assert_eq!(Revision::parse("ABC"), None);
    assert_eq!(first.parent(), None);
    assert_eq!(first.origin(), &Origin::Imported);
}

#[test]
fn an_edit_names_its_base_and_links_its_parent() {
    let first = DocumentRevision::import(BASE).expect("import");
    let second = first.apply(first.revision(), &[hours(72)]).expect("72");
    assert_eq!(second.parent(), Some(first.revision()));
    assert_eq!(second.source(), BASE.replace("48", "72"));
    let Origin::Edited { changed, splices } = second.origin() else {
        panic!("an edit records its evidence");
    };
    assert_eq!(changed, &[Path::new(["const", "window_hours"])]);
    assert_eq!(splices.len(), 1);
    let same = second
        .apply(second.revision(), &[hours(72)])
        .expect("no-op");
    assert_eq!(same.revision(), second.revision());
    assert_eq!(same.parent(), Some(first.revision()));
}

#[test]
fn a_stale_edit_never_overwrites_a_newer_revision() {
    // The holder keeps the current revision; a late change aimed at the first.
    let first = DocumentRevision::import(BASE).expect("import");
    let head = first.apply(first.revision(), &[hours(72)]).expect("72");
    let late = head
        .apply(first.revision(), &[hours(96)])
        .expect_err("stale");
    assert!(
        matches!(&late, RevisionRefusal::Stale { base, current }
            if base == first.revision() && current == head.revision()),
        "{late}"
    );
    assert!(late.to_string().contains("was not applied"), "{late}");
    assert!(head.source().contains("window_hours: 72"));
    let replaced = head
        .replace(first.revision(), BASE.replace("48", "1"))
        .expect_err("a stale replacement is refused too");
    assert!(matches!(replaced, RevisionRefusal::Stale { .. }));
    let next = head
        .apply(head.revision(), &[hours(96)])
        .expect("on the head");
    assert_eq!(next.parent(), Some(head.revision()));
}

#[test]
fn a_whole_source_replacement_claims_no_byte_evidence() {
    let first = DocumentRevision::import(BASE).expect("import");
    let replaced = first
        .replace(first.revision(), BASE.replace("# the threshold\n", ""))
        .expect("replace");
    assert_eq!(replaced.origin(), &Origin::Replaced);
    assert_eq!(replaced.parent(), Some(first.revision()));
    let refused = first
        .replace(first.revision(), "nika: [")
        .expect_err("not a document");
    assert!(matches!(refused, RevisionRefusal::Document(_)), "{refused}");
}
