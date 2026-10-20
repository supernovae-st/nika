// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Live host adapter over the actual Session runtime, a configured fixture route: the
//! deterministic compiler proposes, the fold carries the Session's identity and the exact
//! pending bytes, a revision of the proposal's money is a new identity over the same bytes,
//! a `yes` answers the identity on screen or none, and Save is never a Run. Hermetic: no
//! model, no socket, no run (the runners panic if anything asks them).

use std::path::{Path, PathBuf};

use nika_session::ScriptedReasoner;
use nika_session::change::{ProjectChange, ProjectChangeSet, RunRequest, Witness};
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

/// A live conversation over `root` with an injected fixture route: the facts and the
/// deterministic compiler answer, nothing reasons, nothing runs.
fn live(root: &Path) -> Live {
    let (census, selected) = crate::session::intelligence::fixture();
    let mut live = Live::new(
        root.to_path_buf(),
        census,
        Some(selected),
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
    // One workflow and no run request: the Session's own typed method admits
    // a `save & run` of exactly this proposal, and nothing ran or was saved.
    let review = candidate.review(false).expect("a consent can name it");
    assert!(review.runs(), "the one workflow it saves");
    let lines = review.lines(false, false, 76);
    let rows: Vec<String> = lines.iter().map(ToString::to_string).collect();
    let head = format!("creates {DEST} (");
    let created = rows.iter().find(|row| row.starts_with(&head));
    let counted = created.is_some_and(|row| row.ends_with(" lines)"));
    assert!(counted, "the count bound to its unit: {rows:?}");
    assert!(!room.0.join("out/copy.md").exists(), "nothing ran");
}

/// A set whose changes create `workflows` (and one supporting file), carrying
/// `run` as its own run request.
fn set(workflows: &[&str], run: Option<RunRequest>) -> ProjectChangeSet {
    let mut changes: Vec<ProjectChange> = (workflows.iter())
        .map(|path| ProjectChange::CreateWorkflow {
            path: PathBuf::from(path),
            content: "nika: x\n".to_owned(),
        })
        .collect();
    changes.push(ProjectChange::CreateSupportingFile {
        path: PathBuf::from("notes/brief.md"),
        content: "brief\n".to_owned(),
    });
    ProjectChangeSet {
        root: PathBuf::from("/project"),
        goal: "copy the brief".to_owned(),
        changes,
        run,
        repairs: Vec::new(),
        audits: Vec::new(),
    }
}

/// A run request over `workflow` binding `vars` under the ceiling `max`.
fn asked(workflow: &str, vars: &[&str], max: f64) -> RunRequest {
    RunRequest {
        workflow: PathBuf::from(workflow),
        vars: vars.iter().map(|var| (*var).to_owned()).collect(),
        max_cost_usd: max,
        access_pin: None,
        bytes: None,
        closure: None,
    }
}

/// What a `save & run` would run is folded from the Session's own typed
/// method, never from words or the disk: the one workflow saved; the run the
/// request carried, named by its workflow, its ceiling and the names of its
/// inputs, never a value; nothing where the method refuses (several
/// workflows and no request, no workflow, a ceiling that is no amount).
#[test]
fn a_save_and_run_is_folded_from_the_sessions_typed_method() {
    use crate::workspace::candidate::RunAfter;
    let english = crate::lang::Voice::default();
    let one = super::run_after(&set(&["one.nika"], None), english);
    assert_eq!(one, Some(RunAfter::Saved));
    assert_eq!(
        super::run_after(&set(&["a.nika", "b.nika"], None), english),
        None
    );
    assert_eq!(super::run_after(&set(&[], None), english), None);
    // The names alone: no value, no `=`, and no entry that names no input.
    let carried = asked("report.nika", &["region=north", "limit=5", "unnamed"], 0.25);
    let words = "asked run · report.nika once · ceiling $0.25 · inputs region, limit";
    let folded = super::run_after(&set(&["a.nika", "b.nika"], Some(carried)), english);
    assert_eq!(folded, Some(RunAfter::Asked(words.to_owned())));
    let bare = asked("report.nika", &[], 0.0);
    let words = "asked run · report.nika once · ceiling $0";
    let folded = super::run_after(&set(&["report.nika"], Some(bare)), english);
    assert_eq!(folded, Some(RunAfter::Asked(words.to_owned())));
    // A ceiling under a cent reads as it was set, never « $0.00 » (live gauntlet r09 B12).
    let bare = asked("report.nika", &[], 0.001);
    let words = "asked run · report.nika once · ceiling $0.001";
    let folded = super::run_after(&set(&["report.nika"], Some(bare)), english);
    assert_eq!(folded, Some(RunAfter::Asked(words.to_owned())));
    // In French the words this host builds follow; the workflow, the amount and the names stay.
    let french = crate::lang::Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    let carried = asked("report.nika", &["region=north", "limit=5"], 0.25);
    let folded = super::run_after(&set(&["report.nika"], Some(carried)), french);
    let words = "run demandé · report.nika une fois · plafond $0.25 · entrées region, limit";
    assert_eq!(folded, Some(RunAfter::Asked(words.to_owned())));
    let nowhere = asked("report.nika", &[], f64::NAN);
    let refused = super::run_after(&set(&["report.nika"], Some(nowhere)), english);
    assert_eq!(refused, None, "a ceiling that is no amount");
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

/// What the compiler did to the document is said in a person's words: the values, steps and
/// components it changed by their names, each component by its name, what the bytes show of
/// it and the values it was given, never its kind, its release or a node path (live gauntlet
/// 4: « révisé sur place · tasks.clean.invoke.args.expression », « composant ·
/// block:csv-clean-dedupe-report knowledge-0.123.0-r3.5-foundry-20261016 · expanded »); a
/// whole rewrite and a component no longer as bound are warnings.
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
        super::revised(&expanded, crate::lang::Voice::default()),
        vec![
            (
                "revised in place · value max_age_hours, component stale-filter-report".to_owned(),
                false
            ),
            (
                "component · stale-filter-report · reused as bound · max_age_hours = 72".to_owned(),
                false
            ),
        ]
    );
    let moved = DocumentRevision::of(&record, &["revised".to_owned()]).expect("revision");
    assert!(
        super::revised(&moved, crate::lang::Voice::default())[1].1,
        "no longer as bound: a warning"
    );
    let whole = serde_json::json!({"mode": "replaced", "candidate_sha256": "c"});
    let whole = DocumentRevision::of(&whole, &[]).expect("revision");
    assert_eq!(
        super::revised(&whole, crate::lang::Voice::default()),
        vec![(
            "rewritten whole · no preservation of the earlier bytes is claimed".to_owned(),
            true
        )]
    );
}

/// `expanded` and `invoked` are two admitted states of held reuse, each named by its own words;
/// only a witness that is neither (revised, absent, unreadable, unwitnessed) needs attention,
/// each said in words too.
#[test]
fn an_invoked_component_is_admitted_like_an_expanded_one_and_named_apart() {
    use nika_session::work::DocumentRevision;
    let record = serde_json::json!({"mode": "composed", "candidate_sha256": "c",
        "components": [{"component": {"id": "block:json-filter-records",
            "release": {"version": "1.0.0"}}}]});
    for (witness, said, attention) in [
        ("expanded", "reused as bound", false),
        ("invoked", "called as bound", false),
        ("revised", "reused, then changed", true),
        ("absent", "no longer in these bytes", true),
        ("unreadable", "unreadable in these bytes", true),
        ("unwitnessed", "not checked in these bytes", true),
    ] {
        let revision = DocumentRevision::of(&record, &[witness.to_owned()]).expect("revision");
        let words = format!("component · json-filter-records · {said}");
        assert_eq!(
            super::revised(&revision, crate::lang::Voice::default())[1],
            (words, attention),
            "{witness}"
        );
    }
    // A component the Session did not witness is `unwitnessed`: attention, never admitted.
    let unseen = DocumentRevision::of(&record, &[]).expect("revision");
    assert!(
        super::revised(&unseen, crate::lang::Voice::default())[1].1,
        "{:?}",
        super::revised(&unseen, crate::lang::Voice::default())
    );
}

/// The typed record stays with the fold only where it names the file it binds: the Session
/// binds it to one workflow's bytes by digest, so a set of one workflow names it, two do not.
#[test]
fn the_typed_record_is_kept_only_where_it_binds_the_one_workflow_shown() {
    use nika_session::work::DocumentRevision;
    let room = Room::new("bound");
    let at = |path: &str| {
        ProjectChangeSet::workflow_at(&room.0, COPY, path, format!("nika: {path}\n"))
            .expect("a workflow set")
    };
    let one = at("first.nika");
    let raw = serde_json::json!({"mode": "written", "candidate_sha256": "c"});
    let record = DocumentRevision::of(&raw, &[]);
    assert!(record.is_some());
    assert_eq!(super::bound(&one, record.clone()), record);
    assert_eq!(super::bound(&one, None), None);
    let mut two = one.clone();
    two.changes.extend(at("second.nika").changes);
    assert_eq!(
        super::bound(&two, record),
        None,
        "two workflows: the file is not named"
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
        title.trim_start().starts_with("draft · ") && !title.contains("proposal"),
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

/// Only the draft of the round that asks the question waiting is shown: a snapshot whose draft
/// belongs to questions that do not include the waiting key (a discarded or earlier round's,
/// kept beside another question; live gauntlet 5, r07 B4) shows no draft, so the saved file, or
/// nothing, stands in its place; the same draft under its own question still shows.
#[test]
fn a_draft_shows_only_under_a_question_its_round_asks() {
    use nika_session::work::Waiting;
    let room = Room::new("draft-stale");
    let mut live = live(&room.0);
    let _ = live.submit(ASKS);
    let work = live.runtime.as_ref().expect("runtime").work();
    let Waiting::Question { key, .. } = &work.waiting else {
        panic!("a question waits: {:?}", work.waiting);
    };
    let asked = (work.authoring.as_ref()).expect("the round that asks");
    assert!(
        asked.questions.contains(key),
        "{key} among {:?}",
        asked.questions
    );
    let draft = asked.draft.clone().expect("the compiler drafted");
    let shown = super::draft_of(&work, None).expect("its own question: the draft shows");
    assert_eq!(
        shown.witness(),
        Some(Witness::of(draft.as_bytes()).0.as_str())
    );
    // The same draft, kept from a round whose questions do not ask the key that waits.
    let mut stale = work.clone();
    let other = stale.authoring.as_mut().expect("the round");
    other.questions = vec!["const.earlier_round".to_owned()];
    assert!(
        super::draft_of(&stale, None).is_none(),
        "a stale draft must not show"
    );
    stale.authoring = None;
    assert!(
        super::draft_of(&stale, None).is_none(),
        "no round, no draft"
    );
}

/// Typed creation records have no earlier bytes; an unknown record cannot
/// inherit a replacement claim. These are projection fixtures, not a live CREATE.
#[test]
fn document_records_do_not_describe_a_creation_or_unknown_mode_as_replacement() {
    for (mode, base, changed, expected, attention) in [
        ("written", None, vec![], "created · written whole", false),
        (
            "composed",
            None,
            vec![],
            "created · composed from components",
            false,
        ),
        (
            "operations",
            Some("base"),
            vec![],
            "revised in place · no node changed",
            false,
        ),
        (
            "operations",
            Some("base"),
            vec![
                "tasks.read.invoke.args.path",
                "tasks.read.with.x",
                "permits.fs",
            ],
            "revised in place · step read, the permissions",
            false,
        ),
        (
            "replaced",
            Some("base"),
            vec![],
            "rewritten whole · no preservation of the earlier bytes is claimed",
            true,
        ),
        (
            "future-record",
            None,
            vec![],
            "document record · mode not described",
            true,
        ),
        (
            "written",
            Some("base"),
            vec![],
            "document record · mode not described",
            true,
        ),
    ] {
        let record = serde_json::json!({"mode": mode, "base_sha256": base,
            "candidate_sha256": "a".repeat(64), "changed": changed,
            "preservation": "", "components": []});
        let revision =
            nika_session::work::DocumentRevision::of(&record, &[]).expect("the typed record");
        assert_eq!(
            super::revised(&revision, crate::lang::Voice::default()),
            vec![(expected.to_owned(), attention)],
            "mode {mode}, base {base:?}"
        );
    }
}

/// A change reads in the person's language and the glyph column in use: what it
/// creates and how long, the count bound to its unit (French writes the
/// singular below two, English keeps its one unit); the path stays as written.
#[test]
fn a_change_reads_in_the_persons_language() {
    use crate::lang::{Lang, Voice};
    use nika_tui_view::workspace::text::KEEP;
    let french = Voice {
        lang: Lang::Fr,
        ascii: false,
    };
    let plain = Voice {
        ascii: true,
        ..french
    };
    let one = set(&["one.nika"], None);
    let created = &one.changes[0];
    assert_eq!(
        super::words(created, Voice::default()),
        format!("creates one.nika (1{KEEP}lines)")
    );
    assert_eq!(
        super::words(created, french),
        format!("crée one.nika (1{KEEP}ligne)")
    );
    assert_eq!(
        super::words(created, plain),
        format!("cree one.nika (1{KEEP}ligne)")
    );
}
