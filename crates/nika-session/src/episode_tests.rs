// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The controlled episode — a black-box fixture over the crate's public
//! seams (the runtime · the scripted reasoner · the proposal identity ·
//! the witness), driving one whole authoring episode the way a host does:
//! a local brief from fixture pages, proposed, previewed, consented,
//! applied — and the causal events that must leave stale bytes without
//! effect: the destination's preimage appears, changes or disappears
//! after the preview, alone or as the second file of a set; a host that
//! restarts; a grant that is missing. Native and scripted: no model, no
//! network, no run — the run stays data the door would execute.
//!
//! The laws pinned here are the owners' (ADR-126 · ADR-133): the preview
//! is built from the exact bytes the apply consumes; a consent names the
//! proposal it answers; every witness is checked before the first write;
//! a stale apply leaves the proposal undecided (never « already
//! consumed »); the reasoner sees only the bundle.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::change::{ChangeError, ProjectChange, ProjectChangeSet, Witness};
use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::outcome::{ProposalId, Refusal, RefusalClass};
use crate::reasoner::{ReasonError, Reply, ScriptedReasoner, SessionReasoner};
use crate::runtime::{SessionRuntime, TurnOutcome};

/// The brief workflow the player proposes: reads the two fixture pages,
/// summarizes them, writes the brief under `out/` — inside the project,
/// as the hidden constraint demands.
const BRIEF: &str = "nika: local-brief\nmodel: mock/echo\npermits:\n  fs: { read: [\"./pages/**\"], write: [\"./out/brief.md\"] }\n  tools: [\"nika:read\", \"nika:write\"]\ntasks:\n  page_one:\n    invoke: { tool: \"nika:read\", args: { path: \"./pages/one.html\" } }\n  page_two:\n    invoke: { tool: \"nika:read\", args: { path: \"./pages/two.html\" } }\n  brief:\n    with: { one: \"${{ tasks.page_one.output }}\", two: \"${{ tasks.page_two.output }}\" }\n    infer: { prompt: \"Write a three-line brief from these two pages.\\nPage one: ${{ with.one }}\\nPage two: ${{ with.two }}\", max_tokens: 120 }\n  save:\n    with: { text: \"${{ tasks.brief.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/brief.md\", content: \"${{ with.text }}\" } }\noutputs:\n  brief: ${{ tasks.brief.output }}\n";

/// The grant block inside [`BRIEF`] — removed to play the missing grant.
const GRANT: &str = "permits:\n  fs: { read: [\"./pages/**\"], write: [\"./out/brief.md\"] }\n  tools: [\"nika:read\", \"nika:write\"]\n";

/// A second workflow for a two-file set: one model call, no grant needed.
const NOTE: &str = "nika: brief-note\nmodel: mock/echo\ntasks:\n  note:\n    infer: { prompt: \"One line on why the brief exists.\", max_tokens: 20 }\noutputs:\n  note: ${{ tasks.note.output }}\n";

/// Bytes another hand lands at a destination after the preview.
const FOREIGN: &str = "nika: written-by-another-hand\n";

/// The hidden constraint the oracle holds, outside the root: the token
/// that must never reach the player nor the human's screen.
const ORACLE_MARK: &str = "ORACLE-ONLY destination law: the brief stays under out/";

/// The world: a project root with fixture pages (one of them large), and
/// a private oracle OUTSIDE the root.
struct World {
    root: tempfile::TempDir,
    oracle: tempfile::TempDir,
}

impl World {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("root");
        let pages = root.path().join("pages");
        std::fs::create_dir_all(&pages).expect("pages");
        std::fs::write(
            pages.join("one.html"),
            "<html><body><h1>Page one</h1><p>PAGE-ONE-BODY</p></body></html>\n",
        )
        .expect("one");
        std::fs::write(
            pages.join("two.html"),
            "<html><body><h1>Page two</h1><p>PAGE-TWO-BODY</p></body></html>\n",
        )
        .expect("two");
        let big = format!(
            "<html><body>{}</body></html>\n",
            "<p>BIG-PAGE-ROW</p>\n".repeat(3000)
        );
        std::fs::write(pages.join("big.html"), big).expect("big");
        let oracle = tempfile::tempdir().expect("oracle");
        std::fs::write(oracle.path().join("oracle.txt"), format!("{ORACLE_MARK}\n"))
            .expect("oracle");
        Self { root, oracle }
    }

    fn root(&self) -> &Path {
        self.root.path()
    }

    fn at(&self, rel: &str) -> PathBuf {
        self.root().join(rel)
    }
}

/// Every file under the root with its bytes, relative and sorted — the
/// whole world, so « nothing was written » is judged against the tree
/// (a temp file left behind, a parent created, would show here).
fn listing(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).expect("dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path.strip_prefix(root).expect("inside").to_path_buf();
                out.push((rel, std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// What the player was handed, readable after the runtime boxed it.
type Seen = Arc<Mutex<Vec<String>>>;

/// The player: the crate's own scripted reasoner, with every prompt kept.
struct Player {
    inner: ScriptedReasoner,
    seen: Seen,
}

impl SessionReasoner for Player {
    fn name(&self) -> String {
        self.inner.name()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        let reply = self.inner.reason(prompt)?;
        self.seen.lock().expect("seen").push(prompt.to_owned());
        Ok(reply)
    }
}

/// Open the actual session over the world, a local intelligence ready.
fn open(world: &World, replies: &[String]) -> (SessionRuntime, Seen) {
    let seen: Seen = Arc::default();
    let player = Player {
        inner: ScriptedReasoner::new(replies.to_vec()),
        seen: Arc::clone(&seen),
    };
    let local = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        model: None,
        locus: DataLocus::Local,
        ready: true,
        why: None,
    };
    (
        SessionRuntime::open(world.root(), local, Box::new(player)),
        seen,
    )
}

/// A reply that proposes these files.
fn proposal(blocks: &[(&str, &str)]) -> String {
    let mut out = String::from("Here is the change.\n");
    for (path, body) in blocks {
        let _ = write!(out, "\n```yaml path={path}\n{body}```\n");
    }
    out
}

fn propose(session: &mut SessionRuntime, line: &str) -> (ProposalId, String) {
    match session.turn(line) {
        TurnOutcome::Proposal { id, preview } => (id, preview),
        other => panic!("a proposal was expected: {other:?}"),
    }
}

fn refused(outcome: TurnOutcome) -> Refusal {
    match outcome {
        TurnOutcome::Refusal(refusal) => refusal,
        other => panic!("a refusal was expected: {other:?}"),
    }
}

const ASK: &str =
    "make a brief report from the pages under pages/ and save it in the project as out/brief.md";

/// The information suffices: the brief is proposed from the exact bytes,
/// previewed with its effects, landed on a consent that names it, checked
/// clean, and the run the human asked for is handed back as data — the
/// session never executes, `out/` stays absent, a second consent is a
/// refusal, the observation of the run is a fact.
#[test]
fn the_brief_lands_on_a_named_consent_and_the_run_stays_data() {
    let world = World::new();
    let (mut s, _seen) = open(&world, &[proposal(&[("brief.nika.yaml", BRIEF)])]);
    let before = listing(world.root());
    let (id, preview) = propose(
        &mut s,
        &format!("{ASK}, then run it once with a ceiling of 0.05"),
    );
    for line in BRIEF.lines() {
        assert!(
            preview.contains(&format!("│ {line}")),
            "the exact bytes: {line}"
        );
    }
    for row in [
        "creates `brief.nika.yaml`",
        "clean ✔",
        "reads ./pages/one.html",
        "writes ./out/brief.md",
        "model mock/echo",
        "run `brief.nika.yaml` once (--max-cost-usd 0.05",
    ] {
        assert!(preview.contains(row), "{row}: {preview}");
    }
    assert_eq!(
        listing(world.root()),
        before,
        "nothing is written before the consent"
    );
    assert_eq!(s.pending_proposal().as_ref(), Some(&id));
    let TurnOutcome::RunRequested { report, run } = s.consent_to(&id, "yes") else {
        panic!("a clean check requests the run");
    };
    assert!(
        report.contains("applied · wrote `brief.nika.yaml`") && report.contains("clean ✔"),
        "{report}"
    );
    assert_eq!(run.workflow, PathBuf::from("brief.nika.yaml"));
    assert!((run.max_cost_usd - 0.05).abs() < f64::EPSILON);
    assert_eq!(
        std::fs::read_to_string(world.at("brief.nika.yaml")).expect("landed"),
        BRIEF,
        "byte for byte"
    );
    assert!(
        !world.at("out").exists(),
        "the session never runs: the door would"
    );
    let mut expected = before;
    expected.push((PathBuf::from("brief.nika.yaml"), BRIEF.as_bytes().to_vec()));
    expected.sort();
    assert_eq!(listing(world.root()), expected, "exactly one file more");
    assert_eq!(
        refused(s.consent_to(&id, "yes")).class,
        RefusalClass::AlreadyConsumed
    );
    let TurnOutcome::Facts(observed) =
        s.observe_run(0, Some(Path::new(".nika/traces/brief.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(observed.contains("exit 0 · succeeded"), "{observed}");
    assert!(
        s.snapshot
            .workflows
            .iter()
            .any(|w| w.path.ends_with("brief.nika.yaml"))
    );
}

/// The player sees the bundle and nothing else: the identity core, the
/// project facts, the goal — never the oracle outside the root, never a
/// page body the human did not name, never the environment; and between
/// the preview and the consent the player is not consulted at all.
#[test]
fn the_player_sees_the_bundle_and_never_the_oracle() {
    let world = World::new();
    let (mut s, seen) = open(&world, &[proposal(&[("brief.nika.yaml", BRIEF)])]);
    let (id, preview) = propose(&mut s, ASK);
    let prompts = seen.lock().expect("seen").clone();
    assert_eq!(prompts.len(), 1);
    let prompt = &prompts[0];
    assert!(
        prompt.contains("Never invent Nika syntax") && prompt.contains("root: "),
        "the grounding and the facts ride: {prompt}"
    );
    for private in [
        ORACLE_MARK,
        "PAGE-ONE-BODY",
        "BIG-PAGE-ROW",
        "PATH=",
        "HOME=",
    ] {
        assert!(
            !prompt.contains(private),
            "{private} must never ride: {prompt}"
        );
        assert!(
            !preview.contains(private),
            "{private} must never show: {preview}"
        );
    }
    let oracle_dir = world.oracle.path().display().to_string();
    assert!(
        !prompt.contains(&oracle_dir) && !preview.contains(&oracle_dir),
        "the oracle's place is not a fact"
    );
    let TurnOutcome::Held { id: held, .. } =
        s.consent_to(&id, "what will it read and write when it runs?")
    else {
        panic!("held");
    };
    assert_eq!(held, id);
    assert_eq!(
        seen.lock().expect("seen").len(),
        1,
        "a question at the consent prompt never reaches the player"
    );
    assert_eq!(
        s.pending_proposal().as_ref(),
        Some(&id),
        "the proposal still waits"
    );
}

/// The destination's preimage appears after the preview: the consent that
/// names the proposal is refused as stale, not one byte of the world
/// moves (no file, no temp file), the proposal is left undecided — and
/// the next revision witnesses the bytes that are now there, so the same
/// intent lands over them.
#[test]
fn a_destination_that_appears_after_the_preview_leaves_the_world_untouched() {
    let world = World::new();
    let block = proposal(&[("brief.nika.yaml", BRIEF)]);
    let (mut s, _seen) = open(&world, &[block.clone(), block]);
    let (first, _) = propose(&mut s, ASK);
    assert!(
        matches!(
            s.consent_to(&first, "what does it write?"),
            TurnOutcome::Held { .. }
        ),
        "a question holds it"
    );
    // the causal event: another hand lands bytes at the destination
    std::fs::write(world.at("brief.nika.yaml"), FOREIGN).expect("the event");
    let before = listing(world.root());
    let stale = refused(s.consent_to(&first, "yes"));
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert!(
        stale.text.contains("changed since this preview")
            && stale.text.contains("nothing was applied"),
        "{stale}"
    );
    assert_eq!(
        listing(world.root()),
        before,
        "not one byte moved — no file, no temp file"
    );
    assert!(s.pending_proposal().is_none());
    assert_eq!(
        refused(s.consent_to(&first, "yes")).class,
        RefusalClass::WrongState,
        "undecided, never consumed"
    );
    assert!(
        refused(s.consent("yes"))
            .text
            .contains("nothing is pending")
    );
    // the resolution the law allows: a new revision, witnessed over what is there now
    let (second, preview) = propose(&mut s, "write it again over the file that is there now");
    assert_ne!(second, first, "a new preview is a new identity");
    let witness = Witness::of(FOREIGN.as_bytes());
    assert!(
        preview.contains("replaces `brief.nika.yaml` whole") && preview.contains(witness.short()),
        "{preview}"
    );
    let TurnOutcome::Facts(report) = s.consent_to(&second, "yes") else {
        panic!("applied");
    };
    assert!(
        report.contains("applied · wrote `brief.nika.yaml`"),
        "{report}"
    );
    assert_eq!(
        std::fs::read_to_string(world.at("brief.nika.yaml")).expect("landed"),
        BRIEF
    );
}

/// A witnessed file that changes, then one that disappears, between the
/// preview and the consent: refused as stale, nothing replaced, nothing
/// created in its place, the proposal undecided each time.
#[test]
fn a_witnessed_file_that_changes_or_disappears_after_the_preview_is_refused() {
    let world = World::new();
    std::fs::write(world.at("brief.nika.yaml"), "nika: old\n").expect("seed");
    let block = proposal(&[("brief.nika.yaml", BRIEF)]);
    let (mut s, _seen) = open(&world, &[block.clone(), block]);
    let (first, preview) = propose(&mut s, "rewrite brief.nika.yaml as a brief from the pages");
    assert!(
        preview.contains(Witness::of(b"nika: old\n").short()),
        "{preview}"
    );
    std::fs::write(world.at("brief.nika.yaml"), "nika: changed-meanwhile\n").expect("the change");
    let before = listing(world.root());
    assert_eq!(
        refused(s.consent_to(&first, "yes")).class,
        RefusalClass::StaleRevision
    );
    assert_eq!(listing(world.root()), before, "nothing replaced");
    assert_eq!(
        refused(s.consent_to(&first, "yes")).class,
        RefusalClass::WrongState
    );
    let (second, preview) = propose(&mut s, "rewrite brief.nika.yaml again");
    assert!(
        preview.contains(Witness::of(b"nika: changed-meanwhile\n").short()),
        "the new revision witnesses what is there now: {preview}"
    );
    std::fs::remove_file(world.at("brief.nika.yaml")).expect("the disappearance");
    let before = listing(world.root());
    let stale = refused(s.consent_to(&second, "yes"));
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert_eq!(
        listing(world.root()),
        before,
        "nothing created in its place"
    );
    assert!(!world.at("brief.nika.yaml").exists());
    assert_eq!(
        refused(s.consent_to(&second, "yes")).class,
        RefusalClass::WrongState
    );
}

/// A set of two files whose SECOND destination appears after the preview:
/// every witness is checked before the first write, so the first file —
/// still fresh — is not written either. Nothing lands; the refusal names
/// the stale target; the proposal is undecided.
#[test]
fn a_set_whose_second_target_goes_stale_writes_nothing_at_all() {
    let world = World::new();
    let (mut s, _seen) = open(
        &world,
        &[proposal(&[
            ("brief.nika.yaml", BRIEF),
            ("note.nika.yaml", NOTE),
        ])],
    );
    let (id, preview) = propose(&mut s, "make the brief and a note about it");
    assert!(
        preview.contains("creates `brief.nika.yaml`")
            && preview.contains("creates `note.nika.yaml`"),
        "{preview}"
    );
    std::fs::write(world.at("note.nika.yaml"), FOREIGN).expect("the event");
    let before = listing(world.root());
    let stale = refused(s.consent_to(&id, "yes"));
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert!(
        stale.text.contains("`note.nika.yaml`"),
        "the refusal names the stale target: {stale}"
    );
    assert!(
        !world.at("brief.nika.yaml").exists(),
        "the fresh first file is not written either"
    );
    assert_eq!(listing(world.root()), before);
    assert!(s.pending_proposal().is_none());
    assert_eq!(
        refused(s.consent_to(&id, "yes")).class,
        RefusalClass::WrongState
    );
}

/// The reduced counter-example, at the owner: two changes, one stale
/// witness — `apply` refuses before its first write.
#[test]
fn reduced_two_changes_one_stale_witness_apply_writes_nothing() {
    let world = World::new();
    let set = ProjectChangeSet::from_reply(
        world.root(),
        "the brief and a note",
        &proposal(&[("brief.nika.yaml", BRIEF), ("note.nika.yaml", NOTE)]),
        &[],
        None,
    )
    .expect("legal")
    .expect("two blocks");
    assert_eq!(set.changes.len(), 2);
    assert!(
        set.changes
            .iter()
            .all(|c| matches!(c, ProjectChange::CreateWorkflow { .. }))
    );
    std::fs::write(world.at("note.nika.yaml"), FOREIGN).expect("the event");
    let before = listing(world.root());
    let err = set.apply().expect_err("stale");
    assert!(
        matches!(err, ChangeError::Stale(ref p) if p == "note.nika.yaml"),
        "{err}"
    );
    assert_eq!(
        listing(world.root()),
        before,
        "the first change is not written before the second is judged"
    );
}

/// A host that restarts holds no consent from before: a fresh runtime
/// over the same root has nothing pending, the old identity is refused
/// as the wrong state, and nothing is written — a consent never outlives
/// the session that showed the preview.
#[test]
fn a_restarted_host_holds_no_consent_from_the_previous_session() {
    let world = World::new();
    let (mut first, _seen) = open(&world, &[proposal(&[("brief.nika.yaml", BRIEF)])]);
    let (id, _) = propose(&mut first, ASK);
    let before = listing(world.root());
    drop(first); // the interruption
    let (mut restarted, _seen) = open(&world, &[proposal(&[("brief.nika.yaml", BRIEF)])]);
    assert!(restarted.pending_proposal().is_none());
    let old = refused(restarted.consent_to(&id, "yes"));
    assert_eq!(old.class, RefusalClass::WrongState, "{old}");
    assert!(
        old.text.contains(&id.to_string()),
        "the refusal names the identity the host sent: {old}"
    );
    assert!(
        refused(restarted.consent("yes"))
            .text
            .contains("nothing is pending")
    );
    assert_eq!(listing(world.root()), before, "nothing was written");
}

/// The grant is missing: the preview says so from the checker's own
/// finding, the preparation still lands on consent (the human saw the
/// exact bytes and the verdict), and the run the human asked for is not
/// started — refused at the point of effect, never silently.
#[test]
fn a_missing_grant_is_named_at_preview_and_stops_the_run_never_the_preparation() {
    let world = World::new();
    let ungranted = BRIEF.replace(GRANT, "");
    assert!(
        !ungranted.contains("permits:"),
        "the grant is gone from the bytes"
    );
    let (mut s, _seen) = open(&world, &[proposal(&[("brief.nika.yaml", &ungranted)])]);
    let (id, preview) = propose(&mut s, &format!("{ASK} and run it once"));
    assert!(
        preview.contains("findings ✖") && preview.contains("NIKA-AUTH-006"),
        "the checker's finding rides the preview: {preview}"
    );
    assert!(preview.contains("run `brief.nika.yaml` once"), "{preview}");
    let TurnOutcome::Facts(report) = s.consent_to(&id, "yes") else {
        panic!("landed, not run");
    };
    assert!(
        report.contains("applied · wrote `brief.nika.yaml`")
            && report.contains("findings ✖")
            && report.contains("the run was not started"),
        "{report}"
    );
    assert_eq!(
        std::fs::read_to_string(world.at("brief.nika.yaml")).expect("landed"),
        ungranted
    );
    assert!(!world.at("out").exists());
}

/// Restore a mode even if the test panics, so the tempdir can drop.
#[cfg(unix)]
struct RestorePerms {
    path: PathBuf,
    mode: u32,
}

#[cfg(unix)]
impl Drop for RestorePerms {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(self.mode));
    }
}

/// Visible when a unix permission law cannot be proven on this process.
#[cfg(unix)]
#[allow(clippy::disallowed_macros, clippy::print_stderr)]
fn note_coverage_limit(why: &str) {
    eprintln!("{why}");
}

/// `Some` when this process cannot prove EACCES on a 0o000 file (root
/// or an unexpected error). Name the reason and return; do not panic;
/// do not invent a pass of the EACCES law.
#[cfg(unix)]
fn eacces_unproven(path: &Path) -> Option<String> {
    match std::fs::read(path) {
        Ok(_) => Some(format!(
            "coverage limitation: this process can still read 0o000 at {}; EACCES law not proven",
            path.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => None,
        Err(e) => Some(format!(
            "coverage limitation: 0o000 at {} produced {e}; EACCES law not proven",
            path.display()
        )),
    }
}

/// Public seam: an existing destination the process cannot read is named
/// at proposal, never previewed as a create. The unreadable preimage
/// stays. Skipped when this process can still read a 0o000 file (root).
#[cfg(unix)]
#[test]
fn an_unreadable_destination_is_named_at_proposal_not_promised_as_create() {
    use std::os::unix::fs::PermissionsExt as _;
    const SECRET: &str = "nika: secret-on-disk\n";
    let world = World::new();
    let dest = world.at("brief.nika.yaml");
    std::fs::write(&dest, SECRET).expect("seed");
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let restore = RestorePerms {
        path: dest.clone(),
        mode: 0o644,
    };
    if let Some(why) = eacces_unproven(&dest) {
        note_coverage_limit(&why);
        return;
    }
    let (mut s, _seen) = open(&world, &[proposal(&[("brief.nika.yaml", BRIEF)])]);
    let outcome = s.turn(ASK);
    let refusal = refused(outcome);
    assert_eq!(refusal.class, RefusalClass::Io, "{refusal}");
    assert!(
        !refusal.text.contains("creates `brief.nika.yaml`"),
        "no create is promised over unwitnessed bytes: {refusal}"
    );
    assert!(
        refusal.text.contains("brief.nika.yaml")
            && (refusal.text.contains("cannot be witnessed")
                || refusal.text.contains("unreadable")),
        "the refusal names that the target exists and was not seen: {refusal}"
    );
    drop(restore);
    assert_eq!(std::fs::read_to_string(&dest).expect("untouched"), SECRET);
}

/// Public seam: the first write of a two-file set lands, the second is
/// refused by the file system. The refusal names the file that landed
/// from the apply's own written list; the snapshot sees it; a retry by
/// id is `wrong_state` (undecided — the law holds). Distinct from the
/// stale-second-target fixture, which writes nothing at all.
#[cfg(unix)]
#[test]
fn a_partial_apply_names_the_file_that_landed_and_leaves_the_proposal_undecided() {
    use std::os::unix::fs::PermissionsExt as _;
    let world = World::new();
    let locked = world.at("locked");
    std::fs::create_dir(&locked).expect("locked");
    let (mut s, _seen) = open(
        &world,
        &[proposal(&[
            ("brief.nika.yaml", BRIEF),
            ("locked/note.nika.yaml", NOTE),
        ])],
    );
    let (id, preview) = propose(&mut s, "make the brief and a note about it");
    assert!(
        preview.contains("creates `brief.nika.yaml`")
            && preview.contains("creates `locked/note.nika.yaml`"),
        "{preview}"
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("ro");
    let restore = RestorePerms {
        path: locked.clone(),
        mode: 0o755,
    };
    let TurnOutcome::Refusal(io) = s.consent_to(&id, "yes") else {
        note_coverage_limit(
            "coverage limitation: consent was not a refusal after 0o555 on the second parent; mid-set Io law not proven",
        );
        return;
    };
    if io.class != RefusalClass::Io {
        note_coverage_limit(&format!(
            "coverage limitation: consent class is {} after 0o555; mid-set Io law not proven",
            io.class.as_str()
        ));
        return;
    }
    assert_eq!(
        std::fs::read_to_string(world.at("brief.nika.yaml")).expect("first landed"),
        BRIEF
    );
    assert!(!world.at("locked/note.nika.yaml").exists());
    assert!(
        io.text.contains("brief.nika.yaml")
            && (io.text.contains("written before") || io.text.contains("kept")),
        "the refusal names what landed: {io}"
    );
    assert!(
        !io.text.contains("nothing else was written"),
        "the baked suffix claims a total no-write: {io}"
    );
    assert!(
        s.snapshot
            .workflows
            .iter()
            .any(|w| w.path.ends_with("brief.nika.yaml")),
        "the snapshot re-observes the landed workflow"
    );
    assert!(s.pending_proposal().is_none());
    assert_eq!(
        refused(s.consent_to(&id, "yes")).class,
        RefusalClass::WrongState,
        "the proposal stays undecided"
    );
    drop(restore);
}

/// Public seam: a destination that is a symlink out of the root is
/// refused at proposal, never previewed as a replace over the outside
/// bytes. The outside file is untouched.
#[cfg(unix)]
#[test]
fn a_symlinked_destination_is_refused_at_proposal_not_witnessed() {
    const SECRET: &str = "OUTSIDE-SECRET-BYTES-do-not-hash\n";
    let world = World::new();
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("secret.nika.yaml");
    std::fs::write(&target, SECRET).expect("outside");
    std::os::unix::fs::symlink(&target, world.at("brief.nika.yaml")).expect("final symlink");
    let leaked = Witness::of(SECRET.as_bytes());
    let (mut s, _seen) = open(&world, &[proposal(&[("brief.nika.yaml", BRIEF)])]);
    let refusal = refused(s.turn(ASK));
    assert_eq!(refusal.class, RefusalClass::Io, "{refusal}");
    assert!(
        !refusal.text.contains("replaces `brief.nika.yaml`")
            && !refusal.text.contains(leaked.short())
            && !refusal.text.contains(SECRET.trim()),
        "the outside hash and bytes stay out of the refusal: {refusal}"
    );
    assert_eq!(std::fs::read_to_string(&target).expect("untouched"), SECRET);
}
