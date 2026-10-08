// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Live host adapter over the actual Session runtime, no intelligence chosen: the
//! deterministic compiler proposes, the fold carries the Session's identity and the exact
//! pending bytes, a revision of the proposal's money is a new identity over the same bytes,
//! a `yes` answers the identity on screen or none, and Save is never a Run. Hermetic: no
//! model, no socket, no run (the runners panic if anything asks them).

use std::path::{Path, PathBuf};

use nika_session::ScriptedReasoner;
use nika_session::change::Witness;
use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, UserIntelligencePreference,
};
use nika_tui_view::Face;

use crate::model::{Beat, Conversation, Kind};
use crate::session::{Live, Runners};

/// A Ready intent of the deterministic compiler: no question, no model.
const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
/// Where the Session lands COPY's candidate in a root without `workflows/`.
const DEST: &str = "compiled-workflow.nika";

/// A temporary project root, removed when the test ends.
struct Room(PathBuf);

impl Room {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "nika-tui-candidate-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(path.join("notes")).expect("room");
        std::fs::write(path.join("notes/brief.md"), "brief\n").expect("brief");
        Self(path)
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A live conversation over `root` with the kept choice « none »: the facts and the
/// deterministic compiler answer, nothing reasons, nothing runs.
fn live(root: &Path) -> Live {
    let none = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let mut live = Live::new(
        root.to_path_buf(),
        IntelligenceCensus::empty(),
        Some(none),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        Runners {
            run_once: Box::new(|_, _| panic!("nothing runs here")),
            run_resume: Box::new(|_, _, _, _| panic!("nothing resumes here")),
            run_tapped: None,
        },
    );
    let _ = live.open();
    live
}

fn words(beats: &[Beat]) -> String {
    beats
        .iter()
        .filter_map(|b| match b {
            Beat::Say(c) => Some(format!("{:?}: {}", c.kind, c.text)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The pending bytes the runtime holds, and the identity it waits for.
fn pending(live: &Live) -> (String, Option<nika_session::ProposalId>) {
    let runtime = live.runtime.as_ref().expect("runtime");
    let bytes = runtime
        .candidate()
        .map(|c| c.set.changes[0].content().to_owned())
        .unwrap_or_default();
    (bytes, runtime.pending_proposal())
}

#[test]
fn a_proposal_is_folded_with_the_sessions_identity_and_its_exact_bytes() {
    let room = Room::new("fold");
    let mut live = live(&room.0);
    assert!(live.candidate().is_none(), "nothing proposed yet");
    let turn = live.submit(COPY);
    assert!(
        turn.beats
            .iter()
            .any(|b| matches!(b, Beat::Say(c) if c.kind == Kind::Proposal)),
        "{}",
        words(&turn.beats)
    );
    let candidate = live.candidate().expect("the proposal is folded");
    let (bytes, waiting) = pending(&live);
    assert_eq!(
        Some(candidate.id()),
        waiting.as_ref(),
        "the consent identity"
    );
    assert_eq!(
        candidate.witness(),
        Some(Witness::of(bytes.as_bytes()).0.as_str()),
        "the witness of the exact pending bytes"
    );
    assert_eq!(candidate.path(), DEST);
    assert!(!candidate.aside());
    let (_, source) = candidate.face_lines(Face::Source, 100, false, false);
    let source: Vec<String> = source.iter().map(ToString::to_string).collect();
    let first = bytes.lines().next().expect("a first line");
    assert!(
        source.iter().any(|row| row.contains(first)),
        "the pending bytes, never a file: {source:?}"
    );
    assert!(
        source
            .iter()
            .any(|row| row.contains(&format!("creates {DEST}"))),
        "{source:?}"
    );
    assert!(!room.0.join(DEST).exists(), "nothing written");
}

#[test]
fn a_revision_of_the_money_is_a_new_identity_over_the_same_bytes() {
    let room = Room::new("money");
    let mut live = live(&room.0);
    let _ = live.submit(COPY);
    let a = live.candidate().expect("A");
    let turn = live.submit("budget 0.10 USD");
    let b = live.candidate().expect("B");
    assert_ne!(a.id(), b.id(), "{}", words(&turn.beats));
    assert_eq!(a.witness(), b.witness(), "the same bytes");
    assert_eq!(Some(b.id()), pending(&live).1.as_ref(), "B is what waits");
}

#[test]
fn a_yes_answers_the_identity_on_screen_or_none() {
    let room = Room::new("stale");
    let mut live = live(&room.0);
    let _ = live.submit(COPY);
    let a = live.candidate().expect("A");
    let _ = live.submit("budget 0.10 USD");
    let b = live.candidate().expect("B");
    // The screen still shows A (a stale fold): the yes is refused, nothing lands.
    live.candidate = Some(a);
    let turn = live.submit("yes");
    let said = words(&turn.beats);
    assert!(
        said.contains("Refusal") && said.contains("is not the one waiting"),
        "{said}"
    );
    assert!(!room.0.join(DEST).exists(), "a stale consent applied");
    assert_eq!(
        live.candidate().as_ref(),
        Some(&b),
        "the fold follows B again"
    );
    let turn = live.submit("yes");
    assert!(
        words(&turn.beats).contains("applied"),
        "{}",
        words(&turn.beats)
    );
    assert!(room.0.join(DEST).exists(), "B landed");
    assert!(
        live.candidate().is_none(),
        "a saved candidate is no longer one"
    );
    assert!(!room.0.join("out/copy.md").exists(), "Save is never a Run");
}

#[test]
fn a_discarded_candidate_leaves_and_writes_nothing() {
    let room = Room::new("no");
    let mut live = live(&room.0);
    let _ = live.submit(COPY);
    assert!(live.candidate().is_some());
    let turn = live.submit("no");
    assert!(
        words(&turn.beats).contains("discarded"),
        "{}",
        words(&turn.beats)
    );
    assert!(live.candidate().is_none());
    assert!(!room.0.join(DEST).exists());
}

/// A proposal waits but no candidate is on screen (the fold is gone): a
/// `yes` is refused and lands nothing; the next fold shows the candidate
/// again and the next `yes` answers its identity. Declining needs nothing on
/// screen: it applies nothing.
#[test]
fn a_yes_with_no_candidate_on_screen_lands_nothing() {
    let room = Room::new("none-shown");
    let mut live = live(&room.0);
    let _ = live.submit(COPY);
    let shown = live.candidate().expect("shown");
    live.candidate = None;
    let turn = live.submit("yes");
    let said = words(&turn.beats);
    assert!(
        said.contains("Refusal") && said.contains("no proposal is on screen"),
        "{said}"
    );
    assert!(!room.0.join(DEST).exists(), "nothing landed");
    assert_eq!(
        live.candidate().as_ref(),
        Some(&shown),
        "shown again after the line"
    );
    let turn = live.submit("yes");
    assert!(
        words(&turn.beats).contains("applied"),
        "{}",
        words(&turn.beats)
    );
    let _ = live.submit(COPY);
    live.candidate = None;
    let turn = live.submit("no");
    assert!(
        words(&turn.beats).contains("discarded"),
        "{}",
        words(&turn.beats)
    );
}

/// The face used to say what the workflow does when it runs but not where its bytes reach, so
/// a local contract server and a connected service read alike. It now states the Session's
/// declared reach of the exact pending bytes: a copy between project files reaches nothing
/// outside this machine.
#[test]
fn the_face_states_where_the_pending_bytes_reach_as_declared() {
    let room = Room::new("reach");
    let mut live = live(&room.0);
    let _ = live.submit(COPY);
    let candidate = live.candidate().expect("the proposal is folded");
    let (_, body) = candidate.face_lines(Face::Source, 120, false, false);
    let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
    let reach = (rows.iter())
        .find(|row| row.contains("reaches, as declared"))
        .unwrap_or_else(|| panic!("a reach row: {rows:?}"));
    assert!(reach.contains("local only"), "{reach}");
}

/// What the compiler did to the document is said as it is: the changed paths, each component
/// with its version, witness and bindings; a whole rewrite and a component no longer as bound
/// are warnings.
#[test]
fn a_revision_names_what_changed_and_warns_on_a_rewrite_or_a_moved_component() {
    use nika_session::work::DocumentRevision;
    let record = serde_json::json!({"mode": "operations", "candidate_sha256": "c",
        "changed": ["const.max_age_hours", "component block:stale-filter-report"],
        "components": [{"component": {"id": "block:stale-filter-report",
            "release": {"version": "r1"}},
            "bindings": [{"path": "const.max_age_hours", "bound": 72}]}]});
    let expanded = DocumentRevision::of(&record, &["expanded".to_owned()]).expect("revision");
    assert_eq!(
        super::revised(&expanded),
        vec![
            (
                "revised in place · const.max_age_hours, component block:stale-filter-report"
                    .to_owned(),
                false
            ),
            (
                "component · block:stale-filter-report r1 · expanded · const.max_age_hours = 72"
                    .to_owned(),
                false
            ),
        ]
    );
    let moved = DocumentRevision::of(&record, &["revised".to_owned()]).expect("revision");
    assert!(super::revised(&moved)[1].1, "no longer as bound: a warning");
    let whole = serde_json::json!({"mode": "replaced", "candidate_sha256": "c"});
    let whole = DocumentRevision::of(&whole, &[]).expect("revision");
    assert_eq!(
        super::revised(&whole),
        vec![(
            "rewritten whole · no preservation of the earlier bytes is claimed".to_owned(),
            true
        )]
    );
}

/// An exact skeleton the compiler drafts whole and then asks one value of before it proposes.
const ASKS: &str = "aggregate-by-key";

#[test]
fn the_draft_shows_while_its_question_waits_and_answers_no_consent() {
    let room = Room::new("draft");
    let mut live = live(&room.0);
    let turn = live.submit(ASKS);
    let draft = {
        let runtime = live.runtime.as_ref().expect("runtime");
        assert!(
            matches!(
                runtime.waiting(),
                nika_session::work::Waiting::Question { .. }
            ),
            "{}",
            words(&turn.beats)
        );
        assert!(runtime.pending_proposal().is_none(), "nothing is proposed");
        (runtime.work().authoring)
            .and_then(|authoring| authoring.draft)
            .expect("the compiler drafted before it asked")
    };
    let shown = live
        .candidate()
        .expect("the draft is folded while its question waits");
    assert!(
        shown.draft() && shown.aside(),
        "a draft is never consentable"
    );
    assert_eq!(
        shown.witness(),
        Some(Witness::of(draft.as_bytes()).0.as_str())
    );
    let (title, body) = shown.face_lines(Face::Source, 100, false, false);
    let title = title.to_string();
    assert!(
        title.contains(" draft · ") && !title.contains("proposal"),
        "{title}"
    );
    let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
    assert!(rows[0].starts_with("draft · "), "{rows:?}");
    let first = draft.lines().next().expect("a first line");
    assert!(rows.iter().any(|row| row.contains(first)), "{rows:?}");
    assert!(
        !rows.iter().any(|row| row.contains("what a yes answers")),
        "{rows:?}"
    );
    // The draft lives only while its question waits: dropping the question takes it away.
    let turn = live.submit("cancel");
    assert!(live.candidate().is_none(), "{}", words(&turn.beats));
    assert!(!room.0.join(DEST).exists(), "a draft writes nothing");
}
