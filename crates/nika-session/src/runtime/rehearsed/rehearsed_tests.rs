// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A rehearsed copy through the session's own doors: `turn`, `consent`, `consent_to`,
//! the run line, and the scripted route of a change. The reasoner is the counted no-intelligence
//! player; nothing calls a provider, a server or a peer.
//!
//! - **The protocol tests** rehearse over a double of the existing port: each room
//!   answers the copy exactly, or turns the text copy's CRLF into LF. They prove the session's
//!   protocol, never a real rehearsal.
//! - **The real-room test alone** runs the observed room, and prints one `room-call/1` line before
//!   and after each port call. It is ignored: it runs only when selected explicitly.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nika_event::source_id::sha256_hex;
use nika_onboard::compile::rehearse::{
    Attempt, Bounds, CopyReceipt, Digest, EffectCounts, FinalReceipt, FinalState, Held,
    LedgerFacts, Observation, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse, RoomEvidence,
    Spent,
};
use nika_onboard::compile::room::ObservedRoom;
use serde_json::{Value, json};

use super::super::authoring::DETERMINISTIC;
use crate::authoring::{AuthoringRound, compile_in};
use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::outcome::{ProposalId, Refusal, RefusalClass};
use crate::reasoner::{ReasonError, Reply, SessionReasoner};
use crate::runtime::{SessionRuntime, TurnOutcome};
use crate::turn::{
    RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext, TurnDecision,
};

const INTENT: &str = "Copy ./in/source.txt as is to ./out/copied.txt";
/// A new complete request of the same sentence, to another target.
const SECOND: &str = "Copy ./in/source.txt as is to ./out/second.txt";
const SOURCE: &str = "in/source.txt";
const USER: &str = "the user notes, line one\n";
/// Other bytes of the same length.
const EDITED: &str = "the user notes, line two\n";
/// Where the session lands a candidate in a root without `workflows/`.
const LANDED: &str = "compiled-workflow.nika";
/// The run line, with its ceiling.
const RUN: &str = "run it with a ceiling of 0.05";
/// A money-only amendment said at the consent prompt.
const BUDGET: &str = "budget 0.10 USD";

type Prompts = Arc<Mutex<Vec<String>>>;

/// The no-intelligence reasoner, counted.
struct Player {
    prompts: Prompts,
}

impl SessionReasoner for Player {
    fn name(&self) -> String {
        "none".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.prompts
            .lock()
            .expect("prompt record")
            .push(prompt.to_owned());
        Err(ReasonError::NoIntelligence)
    }
}

/// A room over one world's root that answers a copy as it would run: the target holds the
/// source's bytes, or the text copy's CRLF turned into LF when `rewrites`.
struct Room {
    root: PathBuf,
    rewrites: bool,
}

impl Rehearse for Room {
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }

    fn bound(&self) -> Duration {
        ObservedRoom::BOUND
    }

    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move { answer(&self.root, self.rewrites, candidate, inputs, targets) })
    }
}

/// What a room would observe of `candidate` over the world at `root`.
fn answer(
    root: &Path,
    rewrites: bool,
    candidate: &str,
    inputs: &[String],
    targets: &[String],
) -> RehearsalReport {
    let read = |path: &str| {
        std::fs::read(root.join(path.trim_start_matches("./"))).expect("an input the room copies")
    };
    let source = String::from_utf8(read(SOURCE)).expect("text");
    let result = if rewrites && !candidate.contains("binary: true") {
        source.replace("\r\n", "\n")
    } else {
        source
    };
    let mut observation = Observation::none();
    for input in inputs {
        let bytes = read(input);
        let digest = Digest::of(&bytes);
        let held = Held::of(&bytes, ObservedRoom::PREVIEW_BOUND);
        observation.copies.push(CopyReceipt::new(
            input.clone(),
            digest.clone(),
            Some(digest),
            held,
        ));
    }
    for target in targets {
        let state = FinalState::File {
            digest: Digest::of(result.as_bytes()),
            held: Held::of(result.as_bytes(), ObservedRoom::PREVIEW_BOUND),
        };
        observation
            .finals
            .push(FinalReceipt::new(target.clone(), state));
    }
    let written = targets
        .iter()
        .map(|target| target.trim_start_matches("./").to_owned())
        .collect();
    observation.ledger = LedgerFacts::clean(written);
    let copied = observation
        .copies
        .iter()
        .map(|copy| copy.source.bytes)
        .sum();
    let read_back = observation.finals.len() as u64 * result.len() as u64;
    observation.spent = Spent::new(copied, read_back);
    observation.bounds = Bounds::new(
        10_000,
        ObservedRoom::COPY_BOUND,
        ObservedRoom::PREVIEW_BOUND,
    );
    let sha = sha256_hex(candidate.as_bytes());
    RehearsalReport::new(
        Rehearsal::Passed {
            outputs: Vec::new(),
        },
        Attempt::Completed { elapsed_ms: 5 },
        EffectCounts::none(),
        sha.clone(),
    )
    .with_room(RoomEvidence::new(true, true))
    .with_admitted_digest(format!("admitted-{sha}"))
    .with_observation(observation)
}

/// A session over `root` with no intelligence, its prompts counted, and the rehearsal host
/// `host` builds over each world's root; the host calls counted.
fn open_with(
    root: &Path,
    host: impl Fn(&Path) -> Box<dyn Rehearse> + Send + Sync + 'static,
) -> (SessionRuntime, Prompts, Arc<AtomicUsize>) {
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let intelligence = ResolvedSessionIntelligence {
        kind: IntelligenceKind::None,
        model: None,
        locus: DataLocus::None,
        ready: true,
        why: None,
    };
    let player = Player {
        prompts: Arc::clone(&prompts),
    };
    let mut s = SessionRuntime::open(root, intelligence, Box::new(player));
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    s.with_rehearsal_host(Arc::new(move |world: &Path| -> Box<dyn Rehearse> {
        counted.fetch_add(1, Ordering::SeqCst);
        host(world)
    }));
    (s, prompts, calls)
}

/// A session over the double: exact copies, or the text copy rewriting CRLF.
fn open(root: &Path, rewrites: bool) -> (SessionRuntime, Prompts, Arc<AtomicUsize>) {
    open_with(root, move |world: &Path| -> Box<dyn Rehearse> {
        Box::new(Room {
            root: world.to_path_buf(),
            rewrites,
        })
    })
}

fn write(root: &Path, path: &str, text: &str) {
    let at = root.join(path);
    std::fs::create_dir_all(at.parent().expect("a parent")).expect("dirs");
    std::fs::write(at, text).expect("write");
}

/// A project whose source holds the user's text, and no target.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("project");
    write(dir.path(), SOURCE, USER);
    dir
}

fn proposal(outcome: TurnOutcome) -> (ProposalId, String) {
    match outcome {
        TurnOutcome::Proposal { id, preview } => (id, preview),
        other => panic!("expected a proposal, got {other:?}"),
    }
}

fn refused(outcome: TurnOutcome) -> Refusal {
    match outcome {
        TurnOutcome::Refusal(why) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn facts(outcome: TurnOutcome) -> String {
    match outcome {
        TurnOutcome::Facts(text) => text,
        other => panic!("expected facts, got {other:?}"),
    }
}

/// The exact bytes the pending proposal would land.
fn pending_bytes(s: &SessionRuntime) -> String {
    s.pending
        .as_ref()
        .and_then(|set| set.changes.first())
        .expect("a pending proposal")
        .content()
        .to_owned()
}

/// Nothing of a consent reached the project: no workflow, no output, no consent record.
fn untouched(root: &Path) {
    assert!(!root.join(LANDED).exists(), "no workflow written");
    assert!(!root.join("out").exists(), "no output");
    assert!(
        !root.join(".nika/consents.ndjson").exists(),
        "no consent recorded"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(SOURCE)).expect("source"),
        USER
    );
}

/// A revision that `propose` qualifies as the byte copy keeps that
/// selection as the reading awaiting consent, never the variant it was handed.
#[test]
fn a_revision_keeps_the_outcome_propose_selected() {
    let root = project();
    let (mut s, prompts, _) = open(root.path(), true);
    let round = AuthoringRound::new(INTENT);
    let out = compile_in(
        &DETERMINISTIC,
        &s.project_context(),
        &round.request(),
        INTENT,
    )
    .expect("the copy compiles");
    let handed = out.candidate.clone().expect("a Ready candidate");
    s.last_outcome = Some(out.clone());
    let (id, preview) = proposal(s.propose_revision(&round, &out));
    let kept = s
        .last_outcome
        .as_ref()
        .and_then(|reading| reading.candidate.clone())
        .expect("a reading");
    assert_ne!(kept, handed, "never the variant it was handed");
    assert!(kept.contains("binary: true"), "the byte copy, selected");
    assert_eq!(pending_bytes(&s), kept);
    assert!(s.rehearsed_pending(&id));
    assert!(
        preview.contains("Rehearsed once on a copy of your files"),
        "{preview}"
    );
    untouched(root.path());
    assert!(
        prompts.lock().expect("prompts").is_empty(),
        "no model asked"
    );
}

/// A yes saves the selected bytes only over the world they were rehearsed on: a source
/// edited to other bytes of the same length, or a target that appeared where the preview said
/// none was, withdraws the proposal with no effect.
#[test]
fn a_yes_saves_only_when_the_rehearsed_world_is_unchanged() {
    let root = project();
    let (mut s, prompts, _) = open(root.path(), false);
    let (_, preview) = proposal(s.turn(INTENT));
    assert!(
        preview.contains("`./out/copied.txt` did not exist"),
        "{preview}"
    );
    let bytes = pending_bytes(&s);
    let landed = facts(s.consent("yes"));
    let held = format!(
        "the rehearsed world holds: `./in/source.txt` the same {} B · `./out/copied.txt` still absent",
        USER.len()
    );
    assert!(landed.contains(&held), "{landed}");
    assert!(
        landed.contains("rehearsed on a copy of your files, nothing has run on them"),
        "{landed}"
    );
    let saved = std::fs::read_to_string(root.path().join(LANDED)).expect("saved");
    assert_eq!(saved, bytes, "exactly the selected bytes");
    assert!(
        !root.path().join("out").exists(),
        "a yes saves, it never copies"
    );
    assert!(prompts.lock().expect("prompts").is_empty());

    let root = project();
    let (mut s, _, _) = open(root.path(), false);
    let (id, _) = proposal(s.turn(INTENT));
    write(root.path(), SOURCE, EDITED);
    let why = refused(s.consent("yes"));
    assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
    assert!(
        why.text.contains("`./in/source.txt` changed"),
        "{}",
        why.text
    );
    assert!(why.text.contains("nothing was written"), "{}", why.text);
    assert!(s.pending_proposal().is_none(), "withdrawn, never pending");
    let late = refused(s.consent_to(&id, "yes"));
    assert_eq!(late.class, RefusalClass::WrongState, "{}", late.text);
    write(root.path(), SOURCE, USER);
    untouched(root.path());

    let root = project();
    let (mut s, _, _) = open(root.path(), false);
    proposal(s.turn(INTENT));
    write(root.path(), "out/copied.txt", "appeared\n");
    let why = refused(s.consent("yes"));
    assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
    assert!(
        why.text.contains("`./out/copied.txt` appeared"),
        "{}",
        why.text
    );
    assert!(!root.path().join(LANDED).exists(), "no workflow written");
}

/// A run line runs the saved copy only over the world and the bytes it was rehearsed on. A
/// source changed after the yes, or the workflow replaced by another program at its path, withdraws
/// the rehearsal and its authority: no run is requested, then or at the next run line.
#[test]
fn a_run_line_runs_only_the_rehearsed_world_and_workflow() {
    let root = project();
    let (mut s, _, _) = open(root.path(), false);
    proposal(s.turn(INTENT));
    facts(s.consent("yes"));
    let TurnOutcome::RunRequested { run, .. } = s.turn(RUN) else {
        panic!("the unchanged world requests the run");
    };
    assert_eq!(run.workflow, PathBuf::from(LANDED));
    assert!(run.vars.is_empty());
    assert!((run.max_cost_usd - 0.05).abs() < f64::EPSILON);

    let root = project();
    let (mut s, _, _) = open(root.path(), false);
    proposal(s.turn(INTENT));
    facts(s.consent("yes"));
    write(root.path(), SOURCE, EDITED);
    for _ in 0..2 {
        let why = refused(s.turn(RUN));
        assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
        assert!(why.text.contains("was withdrawn"), "{}", why.text);
        assert!(why.text.contains("nothing was run"), "{}", why.text);
    }

    let root = project();
    let (mut s, _, _) = open(root.path(), false);
    proposal(s.turn(INTENT));
    let bytes = pending_bytes(&s);
    facts(s.consent("yes"));
    let other = bytes.replace("./out/copied.txt", "./out/other.txt");
    assert_ne!(other, bytes, "another program");
    std::fs::write(root.path().join(LANDED), other).expect("replaced");
    let why = refused(s.turn(RUN));
    assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
    assert!(
        why.text.contains("is not the rehearsed one"),
        "{}",
        why.text
    );
}

/// The host's classifier: a line at a proposal changes it; any other is new work.
struct Changes;

impl TurnClassifier for Changes {
    fn classify(&mut self, context: &TurnContext, _raw: &str) -> TurnDecision {
        let act = if matches!(context.phase, SessionPhase::ProposalPending) {
            TurnAct::Modify
        } else {
            TurnAct::NewWork
        };
        TurnDecision::new(act, RoutingMethod::Model)
    }
}

/// Through the real route: a change at the rehearsed proposal is held, the original waiting
/// with its own bytes and proof and nothing rehearsed again; an explicit no discards it; a new
/// complete request to another target is rehearsed for that target, and the old identity saves
/// nothing.
#[test]
fn a_change_keeps_the_rehearsed_original_and_a_new_request_replaces_it() {
    let root = project();
    let (mut s, prompts, calls) = open(root.path(), false);
    s.with_classifier(Box::new(Changes));
    let (first, _) = proposal(s.turn(INTENT));
    let rehearsed = calls.load(Ordering::SeqCst);
    let bytes = pending_bytes(&s);
    let TurnOutcome::Held { id, preview } = s.consent("keep only the first line of it") else {
        panic!("a change at a rehearsed proposal is held");
    };
    assert_eq!(id, first);
    assert!(
        preview.contains("a change never carries a rehearsal"),
        "{preview}"
    );
    assert_eq!(s.pending_proposal(), Some(first.clone()));
    assert_eq!(pending_bytes(&s), bytes, "the original, unchanged");
    assert!(s.rehearsed_pending(&first), "with its own proof");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        rehearsed,
        "nothing rehearsed again"
    );
    let discarded = facts(s.consent("no"));
    assert!(discarded.contains("discarded"), "{discarded}");
    let (second, preview) = proposal(s.turn(SECOND));
    assert_ne!(second, first);
    assert!(preview.contains("`./out/second.txt`"), "{preview}");
    let both = calls.load(Ordering::SeqCst);
    assert!(both > rehearsed, "its own rehearsal");
    let waiting = pending_bytes(&s);
    // The old identity is stale while the second proposal waits: it saves nothing, and the second
    // still waits with its identity, its bytes and its proof, nothing rehearsed again.
    let late = refused(s.consent_to(&first, "yes"));
    assert_eq!(late.class, RefusalClass::StaleRevision, "{}", late.text);
    assert_eq!(s.pending_proposal(), Some(second.clone()));
    assert_eq!(pending_bytes(&s), waiting);
    assert!(s.rehearsed_pending(&second));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        both,
        "no call at the late yes"
    );
    untouched(root.path());
    assert!(
        prompts.lock().expect("prompts").is_empty(),
        "no model asked"
    );
}

/// The rehearsal lines a preview shows, from their first line to the worlds the copy held on.
fn rehearsal_block(preview: &str) -> String {
    let start = preview
        .find("Rehearsed once on a copy of your files")
        .expect("a rehearsal");
    let rest = &preview[start..];
    let worlds = rest.find("held on every world:").expect("its worlds");
    let end = rest[worlds..]
        .find('\n')
        .map_or(rest.len(), |at| worlds + at + 1);
    rest[..end].to_owned()
}

/// A money-only amendment at a rehearsed proposal: a new identity over the very same bytes, its
/// preview showing that same rehearsal beside the new budget, nothing rehearsed again and no model
/// asked. The old identity is stale; the yes of the new one lands with its proof, a run of the
/// unchanged world is requested, and a source changed after the yes withdraws the next run.
#[test]
fn a_money_only_amendment_keeps_the_rehearsal_of_the_same_bytes() {
    let root = project();
    let (mut s, prompts, calls) = open(root.path(), false);
    let (first, preview) = proposal(s.turn(INTENT));
    let rehearsed = calls.load(Ordering::SeqCst);
    let bytes = pending_bytes(&s);
    let (amended, again) = proposal(s.consent(BUDGET));
    assert_ne!(amended, first);
    assert_eq!(pending_bytes(&s), bytes, "the same bytes");
    assert!(again.contains(&rehearsal_block(&preview)), "{again}");
    assert!(again.contains("monetary input:"), "the new budget: {again}");
    assert!(s.rehearsed_pending(&amended) && !s.rehearsed_pending(&first));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        rehearsed,
        "nothing rehearsed again"
    );
    let late = refused(s.consent_to(&first, "yes"));
    assert_eq!(late.class, RefusalClass::StaleRevision, "{}", late.text);
    assert_eq!(s.pending_proposal(), Some(amended.clone()));
    let landed = facts(s.consent("yes"));
    assert!(landed.contains("the rehearsed world holds"), "{landed}");
    let saved = std::fs::read_to_string(root.path().join(LANDED)).expect("saved");
    assert_eq!(saved, bytes);
    let TurnOutcome::RunRequested { run, .. } = s.turn(RUN) else {
        panic!("the unchanged world requests the run");
    };
    assert_eq!(run.workflow, PathBuf::from(LANDED));
    write(root.path(), SOURCE, EDITED);
    let why = refused(s.turn(RUN));
    assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
    assert!(why.text.contains("was withdrawn"), "{}", why.text);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        rehearsed,
        "no call after the proposal"
    );
    assert!(
        prompts.lock().expect("prompts").is_empty(),
        "no model asked"
    );
}

/// After a money-only amendment the yes still judges the rehearsed world: a source edited to other
/// bytes, or a target that appeared where none was, withdraws the amended proposal and writes
/// nothing.
#[test]
fn a_money_only_amendment_never_saves_over_a_changed_world() {
    let changes = [
        (SOURCE, EDITED, "`./in/source.txt` changed"),
        (
            "out/copied.txt",
            "appeared\n",
            "`./out/copied.txt` appeared",
        ),
    ];
    for (path, text, said) in changes {
        let root = project();
        let (mut s, _, calls) = open(root.path(), false);
        proposal(s.turn(INTENT));
        proposal(s.consent(BUDGET));
        write(root.path(), path, text);
        let why = refused(s.consent("yes"));
        assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
        assert!(why.text.contains(said), "{}", why.text);
        assert!(why.text.contains("nothing was written"), "{}", why.text);
        assert!(!root.path().join(LANDED).exists(), "no workflow written");
        assert!(s.pending_proposal().is_none(), "withdrawn, never pending");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "the proposal's rehearsal only"
        );
    }
}

/// The real room, each port call printed as one `room-call/1` line before it and one after it,
/// before any assertion: safe fields only, never a text, a message or a path.
struct Logged {
    inner: ObservedRoom,
    test: &'static str,
    subrun: usize,
}

impl Rehearse for Logged {
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }

    fn bound(&self) -> Duration {
        self.inner.bound()
    }

    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            room_call(&json!({
                "schema": "room-call/1", "event": "BEGIN", "test": self.test,
                "subrun": self.subrun, "candidate_sha256": sha256_hex(candidate.as_bytes()),
                "candidate_bytes": candidate.len(),
            }));
            let report = self
                .inner
                .rehearse_reading(candidate, inputs, targets)
                .await;
            room_call(&returned(self.test, self.subrun, &report));
            report
        })
    }
}

#[allow(clippy::print_stdout, clippy::disallowed_macros)]
fn room_call(line: &Value) {
    println!("{line}");
}

/// The RETURN line of one port call: identities, the attempt, the outcome, the room, the denied
/// counts, the ledger, and the copies and finals by index as length, sha256 and coverage.
fn returned(test: &str, subrun: usize, report: &RehearsalReport) -> Value {
    let (attempt, elapsed_ms) = match report.attempt {
        Attempt::NeverAttempted => ("never", 0),
        Attempt::Completed { elapsed_ms } => ("completed", elapsed_ms),
        Attempt::Stopped { elapsed_ms } => ("stopped", elapsed_ms),
        _ => ("other", 0),
    };
    let (outcome, failed) = match &report.outcome {
        Rehearsal::Passed { .. } => ("passed", Value::Null),
        Rehearsal::Missing { .. } => ("missing", Value::Null),
        Rehearsal::Failed { code, task, .. } => ("failed", json!({"task": task, "code": code})),
        Rehearsal::NotRun { .. } => ("not_run", Value::Null),
        _ => ("other", Value::Null),
    };
    let coverage = |held: &Held| match held {
        Held::Whole(_) => "whole",
        Held::Preview(_) => "preview",
        Held::NotText => "not_text",
        _ => "other",
    };
    let copies: Vec<Value> = report
        .observation
        .copies
        .iter()
        .enumerate()
        .map(|(index, copy)| {
            json!({"index": index, "bytes": copy.source.bytes, "sha256": copy.source.sha256,
                "coverage": coverage(&copy.held)})
        })
        .collect();
    let finals: Vec<Value> = report
        .observation
        .finals
        .iter()
        .enumerate()
        .map(|(index, read)| match &read.state {
            FinalState::File { digest, held } => json!({"index": index, "state": "file",
                "bytes": digest.bytes, "sha256": digest.sha256, "coverage": coverage(held)}),
            FinalState::Absent => json!({"index": index, "state": "absent"}),
            FinalState::Directory => json!({"index": index, "state": "directory"}),
            _ => json!({"index": index, "state": "unreadable"}),
        })
        .collect();
    let (e, ledger) = (&report.effects, &report.observation.ledger);
    json!({
        "schema": "room-call/1", "event": "RETURN", "test": test, "subrun": subrun,
        "candidate_sha256": report.candidate_sha256, "admitted_digest": report.admitted_digest,
        "attempt": attempt, "elapsed_ms": elapsed_ms, "outcome": outcome, "failed": failed,
        "refusal": report.observation.refusal.map(|refusal| format!("{refusal:?}")),
        "room": {"prepared": report.room.prepared, "cleaned": report.room.cleaned,
            "late_refused": report.room.late_refused},
        "effects": {"network": e.network, "provider": e.provider, "spawn": e.spawn,
            "prompt": e.prompt, "secret": e.secret, "child": e.child},
        "ledger": {"written": ledger.written.len(), "late_refused": ledger.late_refused,
            "leftovers": ledger.leftovers, "panicked": ledger.panicked, "drained": ledger.drained},
        "copies": copies,
        "finals": finals,
    })
}

/// The real room. A user's own text, a stale target and a file the request never names. The
/// preview reads back the copy of that text, never a discriminating world's; the originals stay as
/// they were; and the yes saves the workflow only. Every port call leaves its BEGIN and RETURN
/// lines. Ignored: it runs only when selected explicitly.
#[test]
#[ignore = "runs the real rehearsal host: select it explicitly"]
fn a_copy_turn_previews_the_rehearsal_of_the_users_own_files() {
    const TEST: &str = "a_copy_turn_previews_the_rehearsal_of_the_users_own_files";
    const OWN: &str = "only this project holds these words\n";
    const WITNESS: &str = "a file the request never names\n";
    let root = tempfile::tempdir().expect("project");
    write(root.path(), SOURCE, OWN);
    write(root.path(), "out/copied.txt", "stale");
    write(root.path(), "notes.txt", WITNESS);
    let subruns = Arc::new(AtomicUsize::new(0));
    let (mut s, prompts, _) = open_with(root.path(), move |world: &Path| -> Box<dyn Rehearse> {
        Box::new(Logged {
            inner: ObservedRoom::new(world),
            test: TEST,
            subrun: subruns.fetch_add(1, Ordering::SeqCst),
        })
    });
    let (_, preview) = proposal(s.turn(INTENT));
    let read_back = format!(
        "read back · `./out/copied.txt` published by the run · {} B · sha256 {}",
        OWN.len(),
        &sha256_hex(OWN.as_bytes())[..12]
    );
    assert!(preview.contains(&read_back), "{preview}");
    assert!(
        preview.contains(r"« only this project holds these words\n »"),
        "{preview}"
    );
    assert!(
        preview.contains("replaces `./out/copied.txt` (5 B)"),
        "{preview}"
    );
    assert!(
        preview.contains("nothing ran on the originals"),
        "{preview}"
    );
    let file = |path: &str| std::fs::read_to_string(root.path().join(path)).ok();
    let originals = || (file(SOURCE), file("out/copied.txt"), file("notes.txt"));
    let as_they_were = (
        Some(OWN.to_owned()),
        Some("stale".to_owned()),
        Some(WITNESS.to_owned()),
    );
    assert_eq!(originals(), as_they_were, "nothing ran on the originals");
    assert!(
        !root.path().join(LANDED).exists(),
        "nothing saved before a yes"
    );
    let bytes = pending_bytes(&s);
    let landed = facts(s.consent("yes"));
    assert!(landed.contains("rehearsed on a copy"), "{landed}");
    assert_eq!(file(LANDED), Some(bytes), "the yes saves the workflow");
    assert_eq!(originals(), as_they_were, "a yes never copies");
    assert!(
        prompts.lock().expect("prompts").is_empty(),
        "no model asked"
    );
}
