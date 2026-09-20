// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The controlled episode — a black-box fixture over the crate's public
//! seams (the runtime · the ONE compiler · the scripted reasoner · the
//! proposal identity · the witness), driving one whole authoring episode
//! the way a host does: an explicit intent read deterministically by the
//! compiler, proposed as exact bytes at a destination the session
//! chooses, previewed, consented, applied, then run on an explicit line —
//! and the causal events that must leave stale bytes without effect: the
//! destination's preimage appears after the preview; a host that
//! restarts; a grant that goes missing on disk before the run; a
//! destination another hand holds (a foreign file · an unreadable file ·
//! a symlink out of the root). Native and keyless: no model, no network,
//! no run — the run stays data the door would execute.
//!
//! The laws pinned here are the owners' (ADR-126 · ADR-133 · product
//! convergence wave 1): the bytes landed are the bytes the compiler
//! returns for the same request (the parity law); the preview is built
//! from the exact bytes the apply consumes; a consent names the proposal
//! it answers and is never a run; every witness is checked before the
//! first write; a stale apply leaves the proposal undecided (never
//! « already consumed »); a file the human did not name is never
//! witnessed nor replaced; the reasoner sees only the bundle and its
//! reply never becomes a file.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nika_onboard::compile::{CompileRequest, compile};

use crate::change::{ChangeError, ProjectChange, ProjectChangeSet, Witness};
use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::outcome::{ProposalId, Refusal, RefusalClass};
use crate::reasoner::{ReasonError, Reply, ScriptedReasoner, SessionReasoner};
use crate::runtime::{SessionRuntime, TurnOutcome};

/// The intent the episode compiles — explicit operations and literals the
/// deterministic reader settles at once (zero calls · no question): a
/// copy of the brief under `out/`, inside the project, as the hidden
/// constraint demands.
const ASK: &str = "Read ./notes/brief.md and write it to ./out/copy.md";

/// A second explicit intent, Ready at once: the second candidate of a
/// session lands beside the first, never over it.
const ASK_FR: &str = "Lis ./notes/brief.md et écris-le dans ./out/copie.md";

/// Where the session lands a fresh candidate in a root without
/// `workflows/`: the candidate's own id.
const DEST: &str = "compiled-workflow.nika";

/// The numbered twin the session chooses when [`DEST`] is taken.
const TWIN: &str = "compiled-workflow-2.nika";

/// Bytes another hand lands at a destination after the preview.
const FOREIGN: &str = "nika: written-by-another-hand\n";

/// The brief's body: bytes the human never named, which must ride
/// neither a prompt nor a preview.
const BRIEF_BODY: &str = "# Brief\n\nBRIEF-BODY-ROW · the launch moves to October.\n";

/// The hidden constraint the oracle holds, outside the root: the token
/// that must never reach the player nor the human's screen.
const ORACLE_MARK: &str = "ORACLE-ONLY destination law: the copy stays under out/";

/// The candidate the compiler returns for `intent` — the bytes every
/// consent must land byte for byte (adapter parity: the session is a
/// door to the compiler, never a second author).
fn candidate(intent: &str) -> String {
    compile(&CompileRequest::create(intent))
        .expect("the compiler represents the candidate")
        .candidate
        .expect("an explicit intent is Ready at once")
}

/// The world: a project root with the brief the intent names, fixture
/// pages the human never names (one of them large), and a private oracle
/// OUTSIDE the root.
struct World {
    root: tempfile::TempDir,
    oracle: tempfile::TempDir,
}

impl World {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("root");
        let notes = root.path().join("notes");
        std::fs::create_dir_all(&notes).expect("notes");
        std::fs::write(notes.join("brief.md"), BRIEF_BODY).expect("brief");
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

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.at(rel)).expect("readable")
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

/// The tree outside the session's own evidence under `.nika/`.
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    listing(root)
        .into_iter()
        .filter(|(path, _)| !path.starts_with(".nika"))
        .collect()
}

/// What the player was handed, readable after the runtime boxed it.
type Seen = Arc<Mutex<Vec<String>>>;

/// The player: the crate's own scripted reasoner, with every prompt kept.
/// It names no authoring model: the compiler reads deterministically and
/// the player reasons in words only.
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

/// Exactly the compiled copy more in the tree, and under `.nika/` exactly
/// the session's own durable files — nothing else was written.
fn assert_landed_beside_evidence(world: &World, before: Vec<(PathBuf, Vec<u8>)>, own: &[&str]) {
    let mut expected = before;
    expected.push((PathBuf::from(DEST), candidate(ASK).into_bytes()));
    expected.sort();
    let (evidence, tree): (Vec<_>, Vec<_>) = listing(world.root())
        .into_iter()
        .partition(|(path, _)| path.starts_with(".nika"));
    assert_eq!(
        tree, expected,
        "exactly one file more outside the session's own evidence"
    );
    assert_eq!(
        evidence
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        own.iter().map(PathBuf::from).collect::<Vec<_>>(),
        "the session's own durable files, nothing else"
    );
}

/// The consent's durable evidence (#1465): one journal line naming the
/// proposal, the witness of the exact bytes the human saw — the
/// compiler's own — what landed, and no run: a consent is never one.
fn assert_consent_evidence(world: &World, id: &ProposalId) {
    let consents = crate::consent::ConsentRecord::read_all(world.root()).expect("the journal");
    assert_eq!(consents.len(), 1, "one consent, one line");
    assert_eq!(consents[0].proposal, id.as_str());
    assert_eq!(
        consents[0].decision,
        crate::consent::ConsentDecision::Applied
    );
    assert_eq!(consents[0].written, [PathBuf::from(DEST)]);
    assert_eq!(consents[0].witnesses.len(), 1);
    assert_eq!(
        consents[0].witnesses[0].before, None,
        "a create witnessed absence"
    );
    assert_eq!(
        consents[0].witnesses[0].after,
        Witness::of(candidate(ASK).as_bytes()).0,
        "the exact bytes the human saw are the compiler's"
    );
    assert_eq!(consents[0].run, None, "a consent is never a run");
}

/// The information suffices: the copy is proposed from the compiler's
/// exact bytes without a question and without the player, previewed with
/// its effects, landed on a consent that names it, checked clean — and
/// the consent is never a run: `out/` stays absent, a second consent is
/// a refusal, the evidence is the session's own.
#[test]
fn the_copy_lands_on_a_named_consent_that_is_never_a_run() {
    let world = World::new();
    let (mut s, seen) = open(&world, &[]);
    let before = listing(world.root());
    let (id, preview) = propose(&mut s, ASK);
    let bytes = candidate(ASK);
    for line in bytes.lines() {
        assert!(
            preview.contains(&format!("│ {line}")),
            "the exact bytes: {line}"
        );
    }
    for row in [
        "Nika proposes `compiled-workflow.nika`:",
        "read_source · nika:read",
        "write_output · nika:write",
        "external effects · none",
        "human approval at run · none",
        "creates `compiled-workflow.nika`",
        "check of these bytes · `compiled-workflow.nika` · clean ✔",
        "reads ./notes/brief.md",
        "writes ./out/copy.md",
    ] {
        assert!(preview.contains(row), "{row}: {preview}");
    }
    assert!(
        seen.lock().expect("seen").is_empty(),
        "an explicit intent never reaches the player"
    );
    assert_eq!(
        listing(world.root()),
        before,
        "nothing is written before the consent"
    );
    assert_eq!(s.pending_proposal().as_ref(), Some(&id));
    let TurnOutcome::Facts(report) = s.consent_to(&id, "yes") else {
        panic!("consent lands the set and reports the check — never a run");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow.nika`")
            && report.contains("check · `compiled-workflow.nika` · clean ✔")
            && report.contains("say « run it »"),
        "{report}"
    );
    assert_eq!(world.read(DEST), bytes, "byte for byte: the parity law");
    assert!(
        !world.at("out").exists(),
        "the session never runs: the door would, on an explicit line"
    );
    assert_landed_beside_evidence(
        &world,
        before,
        &[".nika/consents.ndjson", ".nika/session-state.json"],
    );
    assert_consent_evidence(&world, &id);
    let state = crate::state::SessionState::load(world.root())
        .expect("readable")
        .expect("written at the consent (#1464)");
    assert!(
        state.decisions.iter().any(|d| {
            d.starts_with(&format!("applied proposal {id}")) && d.contains("compiled-workflow.nika")
        }),
        "{:?}",
        state.decisions
    );
    assert_eq!(state.pending, None, "nothing waits after the consent");
    assert_eq!(
        refused(s.consent_to(&id, "yes")).class,
        RefusalClass::AlreadyConsumed
    );
    assert!(
        seen.lock().expect("seen").is_empty(),
        "the whole episode asked the player nothing"
    );
}

/// The run is an explicit line after the consent, handed back as data
/// with the ceiling the human named — the session never executes; the
/// observation of the run is a fact.
#[test]
fn the_run_is_an_explicit_line_and_stays_data() {
    let world = World::new();
    let (mut s, seen) = open(&world, &[]);
    let (id, _) = propose(&mut s, ASK);
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    let TurnOutcome::RunRequested { report, run } = s.turn("run it with a ceiling of 0.05") else {
        panic!("a clean check on disk requests the run of the accepted workflow");
    };
    assert!(report.contains("clean ✔"), "{report}");
    assert_eq!(run.workflow, PathBuf::from(DEST));
    assert!(run.vars.is_empty());
    assert!((run.max_cost_usd - 0.05).abs() < f64::EPSILON);
    assert!(
        !world.at("out").exists(),
        "the session never runs: the door would"
    );
    let TurnOutcome::Facts(observed) =
        s.observe_run(0, Some(Path::new(".nika/traces/copy.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(observed.contains("exit 0 · succeeded"), "{observed}");
    assert!(s.snapshot.workflows.iter().any(|w| w.path.ends_with(DEST)));
    assert!(
        seen.lock().expect("seen").is_empty(),
        "neither the run line nor the observation reaches the player"
    );
}

/// The player sees the bundle and nothing else: the identity core, the
/// project facts, the goal — never the oracle outside the root, never a
/// page or brief body the human did not name, never the environment; its
/// reply is words, never a file, even when it carries a fenced workflow;
/// and between the preview and the consent the player is not consulted
/// at all.
#[test]
fn the_player_sees_the_bundle_and_never_the_oracle_and_its_reply_is_words() {
    let world = World::new();
    let fenced = "Here you go.\n\n```yaml path=evil.nika\nnika: evil\npermits: {}\ntasks:\n  t:\n    invoke: { tool: \"nika:log\", args: { message: hi } }\n```\n";
    let (mut s, seen) = open(&world, &[fenced.to_owned()]);
    let before = listing(world.root());
    let TurnOutcome::Reply(text) = s.turn("hello there, how are you today?") else {
        panic!("a line that reads as no work is the conversation's");
    };
    assert!(text.contains("evil.nika"), "the words are shown as words");
    assert!(
        s.pending_proposal().is_none(),
        "a reply is never a proposal"
    );
    assert_eq!(listing(world.root()), before, "a reply writes nothing");
    let prompts = seen.lock().expect("seen").clone();
    assert_eq!(prompts.len(), 1);
    let prompt = &prompts[0];
    assert!(
        prompt.contains("Never invent Nika syntax") && prompt.contains("root: "),
        "the grounding and the facts ride: {prompt}"
    );
    let (id, preview) = propose(&mut s, ASK);
    assert_eq!(
        seen.lock().expect("seen").len(),
        1,
        "an explicit intent never reaches the player"
    );
    for private in [
        ORACLE_MARK,
        "PAGE-ONE-BODY",
        "BIG-PAGE-ROW",
        "BRIEF-BODY-ROW",
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
    assert!(!world.at("evil.nika").exists());
}

/// The destination's preimage appears after the preview: the consent
/// that names the proposal is refused as stale, not one byte of the
/// world moves (no file, no temp file), the proposal is left undecided —
/// and the resolution the law allows is the same intent again, at a
/// twin the other hand does not hold: a file the human did not name is
/// never replaced.
#[test]
fn a_destination_that_appears_after_the_preview_leaves_the_world_untouched() {
    let world = World::new();
    let (mut s, _seen) = open(&world, &[]);
    let (first, _) = propose(&mut s, ASK);
    assert!(
        matches!(
            s.consent_to(&first, "what does it write?"),
            TurnOutcome::Held { .. }
        ),
        "a question holds it"
    );
    // the causal event: another hand lands bytes at the destination
    std::fs::write(world.at(DEST), FOREIGN).expect("the event");
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
    let (second, preview) = propose(&mut s, ASK);
    assert_ne!(second, first, "a new preview is a new identity");
    assert!(
        preview.contains("creates `compiled-workflow-2.nika`") && !preview.contains("replaces"),
        "the twin, never a replacement: {preview}"
    );
    let TurnOutcome::Facts(report) = s.consent_to(&second, "yes") else {
        panic!("applied");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow-2.nika`"),
        "{report}"
    );
    assert_eq!(world.read(TWIN), candidate(ASK));
    assert_eq!(
        world.read(DEST),
        FOREIGN,
        "the other hand's file is untouched"
    );
}

/// A file the human did not name is never witnessed nor replaced: the
/// session steps aside to a numbered twin at the preview, and what that
/// file becomes between the preview and the consent — changed, then gone
/// — is not the set's business. A name that frees itself is simply free
/// again for the next candidate.
#[test]
fn a_file_the_human_did_not_name_is_never_witnessed_nor_replaced() {
    let world = World::new();
    std::fs::write(world.at(DEST), "nika: old\n").expect("seed");
    let (mut s, _seen) = open(&world, &[]);
    let (id, preview) = propose(&mut s, ASK);
    assert!(
        preview.contains("creates `compiled-workflow-2.nika`"),
        "{preview}"
    );
    assert!(
        !preview.contains("replaces") && !preview.contains(Witness::of(b"nika: old\n").short()),
        "no replacement is previewed, no witness of an unnamed file rides: {preview}"
    );
    std::fs::write(world.at(DEST), "nika: changed-meanwhile\n").expect("the change");
    let TurnOutcome::Facts(report) = s.consent_to(&id, "yes") else {
        panic!("the set never witnessed that file: the consent lands beside it");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow-2.nika`"),
        "{report}"
    );
    assert_eq!(world.read(TWIN), candidate(ASK));
    assert_eq!(
        world.read(DEST),
        "nika: changed-meanwhile\n",
        "the other hand's file is untouched"
    );
    std::fs::remove_file(world.at(DEST)).expect("the disappearance");
    let (second, preview) = propose(&mut s, ASK_FR);
    assert!(
        preview.contains("creates `compiled-workflow.nika`"),
        "a freed name is free again: {preview}"
    );
    let TurnOutcome::Facts(report) = s.consent_to(&second, "yes") else {
        panic!("applied");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow.nika`"),
        "{report}"
    );
    assert_eq!(world.read(DEST), candidate(ASK_FR));
    assert_eq!(
        world.read(TWIN),
        candidate(ASK),
        "the first candidate stays where it landed"
    );
}

/// The reduced counter-example, at the owner: two compiler candidates in
/// one set, one stale witness — `apply` refuses before its first write,
/// so the fresh first file is not written either.
#[test]
fn reduced_two_candidates_one_stale_witness_apply_writes_nothing() {
    let world = World::new();
    let set = ProjectChangeSet {
        root: world.root().to_path_buf(),
        goal: "the copy, twice".to_owned(),
        changes: vec![
            ProjectChange::CreateWorkflow {
                path: PathBuf::from(DEST),
                content: candidate(ASK),
            },
            ProjectChange::CreateWorkflow {
                path: PathBuf::from(TWIN),
                content: candidate(ASK_FR),
            },
        ],
        run: None,
        repairs: Vec::new(),
        audits: Vec::new(),
    };
    assert!(
        set.preview().contains("creates `compiled-workflow.nika`")
            && set.preview().contains("creates `compiled-workflow-2.nika`")
    );
    std::fs::write(world.at(TWIN), FOREIGN).expect("the event");
    let before = listing(world.root());
    let err = set.apply().expect_err("stale");
    assert!(
        matches!(err, ChangeError::Stale(ref p) if p == TWIN),
        "the refusal names the stale target: {err}"
    );
    assert!(
        !world.at(DEST).exists(),
        "the fresh first file is not written either"
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
    let (mut first, _seen) = open(&world, &[]);
    let (id, _) = propose(&mut first, ASK);
    let before = listing(world.root());
    drop(first); // the interruption
    let (mut restarted, _seen) = open(&world, &[]);
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

/// The candidate without its `permits:` block — what another hand leaves
/// on disk when it strips the grant.
fn without_permits(source: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in source.lines() {
        if line.starts_with("permits:") {
            inside = true;
            continue;
        }
        if inside && line.starts_with(char::is_whitespace) {
            continue;
        }
        inside = false;
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The grant goes missing on disk after the consent: the preparation
/// landed clean (the human saw the exact bytes and the verdict), and the
/// run the human asks for is not started — refused at the point of
/// effect from the checker's own finding, never silently, and the file on
/// disk is repaired by no one.
#[test]
fn a_grant_that_goes_missing_on_disk_stops_the_run_at_the_point_of_effect() {
    let world = World::new();
    let (mut s, _seen) = open(&world, &[]);
    let (id, _) = propose(&mut s, ASK);
    let TurnOutcome::Facts(report) = s.consent_to(&id, "yes") else {
        panic!("landed");
    };
    assert!(report.contains("clean ✔"), "{report}");
    let landed = world.read(DEST);
    let ungranted = without_permits(&landed);
    assert!(
        landed.contains("permits:") && !ungranted.contains("permits:"),
        "the grant is gone from the bytes"
    );
    // the causal event: another hand strips the grant from the landed file
    std::fs::write(world.at(DEST), &ungranted).expect("the event");
    let TurnOutcome::Facts(text) = s.turn("run it") else {
        panic!("findings stop a run before it starts");
    };
    assert!(
        text.contains("check · `compiled-workflow.nika` · findings ✖ — the run was not started")
            && text.contains("NIKA-AUTH-006"),
        "the checker's finding names the missing grant: {text}"
    );
    assert!(!world.at("out").exists());
    assert_eq!(
        world.read(DEST),
        ungranted,
        "the file on disk is the human's: the session repairs nothing"
    );
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

/// Public seam: an existing destination the process cannot read is never
/// promised as a create — the session steps aside to a twin, previews
/// neither the unreadable bytes nor their witness, and the unreadable
/// preimage stays. Skipped when this process can still read a 0o000 file
/// (root).
#[cfg(unix)]
#[test]
fn an_unreadable_destination_is_never_promised_as_a_create() {
    use std::os::unix::fs::PermissionsExt as _;
    const SECRET: &str = "nika: secret-on-disk\n";
    let world = World::new();
    let dest = world.at(DEST);
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
    let (mut s, _seen) = open(&world, &[]);
    let (id, preview) = propose(&mut s, ASK);
    assert!(
        !preview.contains("creates `compiled-workflow.nika`")
            && preview.contains("creates `compiled-workflow-2.nika`"),
        "no create is promised over bytes the session never read: {preview}"
    );
    assert!(
        !preview.contains(SECRET.trim())
            && !preview.contains(Witness::of(SECRET.as_bytes()).short()),
        "the unread bytes and their hash stay out of the preview: {preview}"
    );
    let TurnOutcome::Facts(report) = s.consent_to(&id, "yes") else {
        panic!("the twin lands");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow-2.nika`"),
        "{report}"
    );
    assert_eq!(world.read(TWIN), candidate(ASK));
    drop(restore);
    assert_eq!(world.read(DEST), SECRET, "the unreadable preimage stays");
}

/// The reduced counter-example, at the owner: the first write of a
/// two-candidate set lands, the second is refused by the file system.
/// The refusal names the file that landed from the apply's own written
/// list — it does not claim that nothing was written. Distinct from the
/// stale-second-target fixture, which writes nothing at all.
#[cfg(unix)]
#[test]
fn reduced_partial_apply_names_the_file_that_landed() {
    use std::os::unix::fs::PermissionsExt as _;
    let world = World::new();
    let locked = world.at("locked");
    std::fs::create_dir(&locked).expect("locked");
    let set = ProjectChangeSet {
        root: world.root().to_path_buf(),
        goal: "the copy and a locked twin".to_owned(),
        changes: vec![
            ProjectChange::CreateWorkflow {
                path: PathBuf::from(DEST),
                content: candidate(ASK),
            },
            ProjectChange::CreateWorkflow {
                path: PathBuf::from("locked/compiled-workflow.nika"),
                content: candidate(ASK_FR),
            },
        ],
        run: None,
        repairs: Vec::new(),
        audits: Vec::new(),
    };
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("ro");
    let restore = RestorePerms {
        path: locked.clone(),
        mode: 0o755,
    };
    let Err(attempt) = set.apply_attempt() else {
        note_coverage_limit(
            "coverage limitation: apply succeeded after 0o555 on the second parent; mid-set Io law not proven",
        );
        return;
    };
    if !matches!(attempt.error, ChangeError::Io(..)) {
        note_coverage_limit(&format!(
            "coverage limitation: apply refused with {} after 0o555; mid-set Io law not proven",
            attempt.error
        ));
        return;
    }
    assert_eq!(attempt.written, [PathBuf::from(DEST)]);
    assert_eq!(world.read(DEST), candidate(ASK), "the first landed");
    assert!(!world.at("locked/compiled-workflow.nika").exists());
    let text = attempt.refusal_text(&set);
    assert!(
        text.contains(DEST) && (text.contains("written before") || text.contains("kept")),
        "the refusal names what landed: {text}"
    );
    assert!(
        !text.contains("nothing else was written"),
        "the baked suffix claims a total no-write: {text}"
    );
    drop(restore);
}

/// Public seam: the file system refuses the only write of the set. The
/// refusal names that no earlier file was confirmed, nothing lands (no
/// file, no temp file), no consent is journaled, and the proposal stays
/// undecided — never « already consumed ».
#[cfg(unix)]
#[test]
fn an_io_refusal_at_the_only_write_leaves_the_proposal_undecided() {
    use std::os::unix::fs::PermissionsExt as _;
    let world = World::new();
    let workflows = world.at("workflows");
    std::fs::create_dir(&workflows).expect("workflows");
    let (mut s, _seen) = open(&world, &[]);
    let (id, preview) = propose(&mut s, ASK);
    assert!(
        preview.contains("creates `workflows/compiled-workflow.nika`"),
        "a project that keeps workflows/ receives the candidate there: {preview}"
    );
    std::fs::set_permissions(&workflows, std::fs::Permissions::from_mode(0o555)).expect("ro");
    let restore = RestorePerms {
        path: workflows.clone(),
        mode: 0o755,
    };
    let before = tree(world.root());
    let TurnOutcome::Refusal(io) = s.consent_to(&id, "yes") else {
        note_coverage_limit(
            "coverage limitation: consent was not a refusal after 0o555 on the destination's parent; the Io law is not proven on this process",
        );
        return;
    };
    if io.class != RefusalClass::Io {
        note_coverage_limit(&format!(
            "coverage limitation: consent class is {} after 0o555; the Io law is not proven on this process",
            io.class.as_str()
        ));
        return;
    }
    assert!(
        io.text.contains("no earlier file was confirmed written"),
        "{io}"
    );
    assert_eq!(
        tree(world.root()),
        before,
        "nothing landed — no file, no temp file"
    );
    assert!(
        crate::consent::ConsentRecord::read_all(world.root())
            .expect("the journal")
            .is_empty(),
        "no consent was journaled"
    );
    assert!(s.pending_proposal().is_none());
    assert_eq!(
        refused(s.consent_to(&id, "yes")).class,
        RefusalClass::WrongState,
        "undecided, never consumed"
    );
    drop(restore);
}

/// Public seam: a destination that is a symlink out of the root is never
/// witnessed — a live link is stepped aside from (the twin lands, the
/// outside bytes and their hash stay out of the preview), and a dangling
/// link, which the destination chooser cannot see, is refused at the
/// apply as a target that is not the absence the preview promised. The
/// outside file is untouched either way; no link is followed or written.
#[cfg(unix)]
#[test]
fn a_symlinked_destination_is_never_witnessed_nor_written_through() {
    const SECRET: &str = "OUTSIDE-SECRET-BYTES-do-not-hash\n";
    let world = World::new();
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("secret.nika");
    std::fs::write(&target, SECRET).expect("outside");
    std::os::unix::fs::symlink(&target, world.at(DEST)).expect("final symlink");
    let leaked = Witness::of(SECRET.as_bytes());
    let (mut s, _seen) = open(&world, &[]);
    let (id, preview) = propose(&mut s, ASK);
    assert!(
        preview.contains("creates `compiled-workflow-2.nika`")
            && !preview.contains("replaces")
            && !preview.contains(leaked.short())
            && !preview.contains(SECRET.trim()),
        "the outside hash and bytes stay out of the preview: {preview}"
    );
    let TurnOutcome::Facts(report) = s.consent_to(&id, "yes") else {
        panic!("the twin lands");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow-2.nika`"),
        "{report}"
    );
    assert_eq!(world.read(TWIN), candidate(ASK));
    assert_eq!(std::fs::read_to_string(&target).expect("untouched"), SECRET);
    assert!(
        std::fs::symlink_metadata(world.at(DEST))
            .expect("the link")
            .file_type()
            .is_symlink(),
        "the link itself is not replaced"
    );
    // the dangling link: invisible to `exists()`, the chooser still steps
    // aside from it — a candidate never lands on a link of any kind
    let gone = outside.path().join("gone.nika");
    std::os::unix::fs::symlink(&gone, world.at("compiled-workflow-3.nika")).expect("dangling");
    let (third, preview) = propose(&mut s, ASK_FR);
    assert!(
        preview.contains("creates `compiled-workflow-4.nika`"),
        "the dangling link is a taken name, never a destination: {preview}"
    );
    let TurnOutcome::Facts(report) = s.consent_to(&third, "yes") else {
        panic!("the fourth name lands");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow-4.nika`"),
        "{report}"
    );
    assert!(!gone.exists(), "nothing was written through the link");
    assert!(
        std::fs::symlink_metadata(world.at("compiled-workflow-3.nika"))
            .expect("the dangling link")
            .file_type()
            .is_symlink(),
        "the dangling link itself is untouched"
    );
    assert_eq!(
        refused(s.consent_to(&third, "yes")).class,
        RefusalClass::AlreadyConsumed,
        "decided once"
    );
}
