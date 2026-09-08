// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Durable-session acceptance tests over the runtime's public doors. The
//! process fixture uses only scripted reasoning, local files and a real kill;
//! no provider, workflow execution or paid request is involved.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::outcome::{Refusal, RefusalClass};
use crate::reasoner::{ReasonError, Reply, ScriptedReasoner, SessionReasoner};
use crate::runtime::{SessionRuntime, TurnOutcome};

const GOAL: &str = "Compose a short poem about a violet comet.";
const ANSWER: &str = "A violet comet crosses the quiet sky.";
const PROPOSAL: &str = "Here it is.\n\n```yaml path=daily.nika.yaml\nnika: daily\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\noutputs:\n  said: ${{ tasks.t.output }}\n```\n";

type Seen = Arc<Mutex<Vec<String>>>;

struct Player {
    inner: ScriptedReasoner,
    seen: Seen,
}

impl SessionReasoner for Player {
    fn name(&self) -> String {
        "durability-fixture".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.seen
            .lock()
            .expect("prompt record")
            .push(prompt.to_owned());
        self.inner.reason(prompt)
    }
}

fn intelligence() -> ResolvedSessionIntelligence {
    ResolvedSessionIntelligence {
        kind: IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        model: None,
        locus: DataLocus::Local,
        ready: true,
        why: None,
    }
}

fn open(root: &Path, replies: &[&str]) -> (SessionRuntime, Seen) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let player = Player {
        inner: ScriptedReasoner::new(replies.iter().map(|s| (*s).to_owned()).collect()),
        seen: Arc::clone(&seen),
    };
    (
        SessionRuntime::open(root, intelligence(), Box::new(player)),
        seen,
    )
}

fn history_dir(home: &Path, root: &Path) -> PathBuf {
    let canonical = root.canonicalize().expect("canonical fixture root");
    let identity = blake3::hash(canonical.to_str().expect("UTF-8 root").as_bytes());
    home.join(".nika/sessions").join(identity.to_hex().as_str())
}

fn refused(outcome: TurnOutcome) -> Refusal {
    match outcome {
        TurnOutcome::Refusal(why) => why,
        other => panic!("expected refusal, got {other:?}"),
    }
}

fn assert_uncertain(notice: &str) {
    let lower = notice.to_ascii_lowercase();
    assert!(
        [
            "uncertain",
            "interrupted",
            "unfinished",
            "in flight",
            "in-flight",
            "unknown",
            "may have"
        ]
        .iter()
        .any(|word| lower.contains(word)),
        "recovery must disclose the incomplete operation: {notice}"
    );
}

#[test]
fn reopening_restores_the_goal_and_dialogue_without_calling_the_reasoner() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(root.path(), &[ANSWER]);
    assert!(
        first
            .enable_history(home.path())
            .expect("fresh history")
            .is_none()
    );
    first.intent.decisions.push("Use a calm tone.".to_owned());
    first
        .intent
        .unresolved
        .push("Choose the closing line.".to_owned());
    assert!(matches!(first.turn(GOAL), TurnOutcome::Reply(_)));
    drop(first);

    let (mut resumed, seen) = open(root.path(), &["A gentler ending."]);
    assert!(
        resumed
            .enable_history(home.path())
            .expect("resume")
            .is_some()
    );
    assert_eq!(resumed.intent.goal.as_deref(), Some(GOAL));
    assert_eq!(resumed.intent.decisions, ["Use a calm tone."]);
    assert_eq!(resumed.intent.unresolved, ["Choose the closing line."]);
    assert!(
        seen.lock().expect("record").is_empty(),
        "opening is observation"
    );
    assert!(matches!(
        resumed.turn("Make its ending gentler."),
        TurnOutcome::Reply(_)
    ));
    let prompts = seen.lock().expect("record");
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains(GOAL) && prompts[0].contains(ANSWER));
    assert!(
        !root.path().join(".nika").exists(),
        "private history lives outside the project"
    );
}

#[test]
fn persisted_history_redacts_recognized_values_in_every_intent_field_and_dialogue() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(
        root.path(),
        &["A quiet orbit. password=FAKE_ASSISTANT_PASSWORD_VALUE"],
    );
    first.enable_history(home.path()).expect("fresh history");
    first
        .intent
        .decisions
        .push("token=FAKE_DECISION_TOKEN_VALUE".to_owned());
    first
        .intent
        .unresolved
        .push("secret=FAKE_UNRESOLVED_SECRET_VALUE".to_owned());
    assert!(matches!(
        first.turn("Compose a sonnet. token=FAKE_USER_TOKEN_VALUE"),
        TurnOutcome::Reply(_)
    ));
    drop(first);
    let journal =
        std::fs::read_to_string(history_dir(home.path(), root.path()).join("events.ndjson"))
            .expect("history bytes");
    for secret in [
        "FAKE_ASSISTANT_PASSWORD_VALUE",
        "FAKE_DECISION_TOKEN_VALUE",
        "FAKE_UNRESOLVED_SECRET_VALUE",
        "FAKE_USER_TOKEN_VALUE",
    ] {
        assert!(
            !journal.contains(secret),
            "recognized value persisted: {secret}"
        );
    }
    let (mut resumed, seen) = open(root.path(), &[ANSWER]);
    resumed.enable_history(home.path()).expect("resume");
    assert!(matches!(
        resumed.turn("Continue the sonnet."),
        TurnOutcome::Reply(_)
    ));
    let prompt = seen.lock().expect("record")[0].clone();
    assert!(!prompt.contains("FAKE_USER_TOKEN_VALUE"));
    assert!(!prompt.contains("FAKE_ASSISTANT_PASSWORD_VALUE"));
}

#[test]
fn reopening_does_not_restore_the_authority_of_a_pending_proposal() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(root.path(), &[PROPOSAL]);
    first.enable_history(home.path()).expect("fresh history");
    let TurnOutcome::Proposal { id, .. } = first.turn("Write me a daily digest workflow.") else {
        panic!("proposal expected");
    };
    drop(first);
    let (mut resumed, seen) = open(root.path(), &[ANSWER]);
    resumed.enable_history(home.path()).expect("resume");
    assert!(resumed.pending_proposal().is_none());
    assert!(resumed.waiting_gate().is_none());
    assert_eq!(
        refused(resumed.consent_to(&id, "yes")).class,
        RefusalClass::WrongState
    );
    assert_eq!(
        refused(resumed.consent("yes")).class,
        RefusalClass::WrongState
    );
    assert!(!root.path().join("daily.nika.yaml").exists());
    assert!(seen.lock().expect("record").is_empty());
}

#[test]
fn a_run_request_without_an_observation_remains_uncertain_and_is_not_replayed() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(root.path(), &[PROPOSAL]);
    first.enable_history(home.path()).expect("fresh history");
    let TurnOutcome::Proposal { id, .. } =
        first.turn("Write me a daily digest workflow and run it once.")
    else {
        panic!("proposal expected");
    };
    assert!(matches!(
        first.consent_to(&id, "yes"),
        TurnOutcome::RunRequested { .. }
    ));
    let landed = std::fs::read(root.path().join("daily.nika.yaml")).expect("landed file");
    drop(first);

    let (mut resumed, seen) = open(root.path(), &[ANSWER]);
    let notice = resumed
        .enable_history(home.path())
        .expect("resume")
        .expect("recovery notice");
    assert_uncertain(&notice);
    assert!(seen.lock().expect("record").is_empty());
    assert_eq!(
        refused(resumed.consent_to(&id, "yes")).class,
        RefusalClass::WrongState
    );
    assert_eq!(
        std::fs::read(root.path().join("daily.nika.yaml")).expect("same file"),
        landed
    );
    assert!(!root.path().join(".nika/traces").exists());
}

#[test]
fn an_append_failure_prevents_consent_and_poisoning_survives_filesystem_repair() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut session, seen) = open(root.path(), &[PROPOSAL]);
    session.enable_history(home.path()).expect("fresh history");
    let TurnOutcome::Proposal { id, .. } = session.turn("Write me a daily digest workflow.") else {
        panic!("proposal expected");
    };
    let journal = history_dir(home.path(), root.path()).join("events.ndjson");
    let saved = journal.with_extension("saved");
    std::fs::rename(&journal, &saved).expect("preserve journal");
    std::fs::create_dir(&journal).expect("block append with a directory");
    assert_eq!(
        refused(session.consent_to(&id, "yes")).class,
        RefusalClass::Io
    );
    assert!(!root.path().join("daily.nika.yaml").exists());
    std::fs::remove_dir(&journal).expect("remove obstruction");
    std::fs::rename(&saved, &journal).expect("restore original journal");
    let before = seen.lock().expect("record").len();
    let why = refused(session.turn("Compose a second poem."));
    assert!(matches!(
        why.class,
        RefusalClass::Io | RefusalClass::WrongState
    ));
    assert_eq!(
        seen.lock().expect("record").len(),
        before,
        "poisoned runtime cannot call a model"
    );
    assert!(!root.path().join("daily.nika.yaml").exists());
}

#[test]
fn failed_history_enable_cannot_silently_fall_back_to_an_ephemeral_session() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    std::fs::write(home.path().join(".nika"), b"not a directory").expect("obstruction");
    let (mut session, seen) = open(root.path(), &[ANSWER]);
    assert!(session.enable_history(home.path()).is_err());
    std::fs::remove_file(home.path().join(".nika")).expect("repair home");
    let why = refused(session.turn(GOAL));
    assert!(matches!(
        why.class,
        RefusalClass::Io | RefusalClass::WrongState
    ));
    assert!(seen.lock().expect("record").is_empty());
}

#[test]
fn corrupt_and_truncated_journals_are_refused_without_changing_their_bytes() {
    for suffix in [b"not-json\n".as_slice(), b"{\"unfinished\":"] {
        let root = tempfile::tempdir().expect("project");
        let home = tempfile::tempdir().expect("home");
        let (mut first, _) = open(root.path(), &[ANSWER]);
        first.enable_history(home.path()).expect("fresh history");
        assert!(matches!(first.turn(GOAL), TurnOutcome::Reply(_)));
        drop(first);
        let journal = history_dir(home.path(), root.path()).join("events.ndjson");
        let mut bytes = std::fs::read(&journal).expect("original history");
        bytes.extend_from_slice(suffix);
        std::fs::write(&journal, &bytes).expect("corrupt tail");
        let (mut resumed, seen) = open(root.path(), &[ANSWER]);
        assert!(resumed.enable_history(home.path()).is_err());
        assert_eq!(std::fs::read(&journal).expect("retained history"), bytes);
        assert!(matches!(resumed.turn(GOAL), TurnOutcome::Refusal(_)));
        assert!(seen.lock().expect("record").is_empty());
    }
}

#[test]
fn an_oversized_journal_and_oversized_input_are_refused_before_reasoning() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut first, seen) = open(root.path(), &[ANSWER]);
    first.enable_history(home.path()).expect("fresh history");
    assert!(matches!(
        first.turn(&"x".repeat(64 * 1024 + 1)),
        TurnOutcome::Refusal(_)
    ));
    assert!(seen.lock().expect("record").is_empty());
    drop(first);
    let journal = history_dir(home.path(), root.path()).join("events.ndjson");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&journal)
        .expect("journal");
    file.set_len(16 * 1024 * 1024 + 1)
        .expect("oversized sparse fixture");
    drop(file);
    let (mut resumed, _) = open(root.path(), &[ANSWER]);
    assert!(resumed.enable_history(home.path()).is_err());
    assert_eq!(
        std::fs::metadata(&journal).expect("retained journal").len(),
        16 * 1024 * 1024 + 1
    );
}

#[cfg(unix)]
#[test]
fn symlinked_journal_or_lock_is_refused_without_touching_the_target() {
    use std::os::unix::fs::symlink;
    for name in ["events.ndjson", "session.lock"] {
        let root = tempfile::tempdir().expect("project");
        let home = tempfile::tempdir().expect("home");
        let outside = tempfile::tempdir().expect("outside");
        let (mut first, _) = open(root.path(), &[ANSWER]);
        first.enable_history(home.path()).expect("fresh history");
        drop(first);
        let target = outside.path().join("untouched");
        std::fs::write(&target, b"OUTSIDE-WITNESS").expect("outside bytes");
        let path = history_dir(home.path(), root.path()).join(name);
        std::fs::remove_file(&path).expect("remove owned fixture child");
        symlink(&target, &path).expect("redirect fixture child");
        let (mut resumed, seen) = open(root.path(), &[ANSWER]);
        assert!(
            resumed.enable_history(home.path()).is_err(),
            "must refuse {name}"
        );
        assert_eq!(
            std::fs::read(&target).expect("outside stays"),
            b"OUTSIDE-WITNESS"
        );
        assert!(seen.lock().expect("record").is_empty());
    }
}

#[cfg(unix)]
// This controller must launch and kill a separate OS process. Kernel shell
// execution would test workflow effects instead of the session's crash boundary.
// Environment reads below carry only temporary fixture paths and a mode tag.
#[allow(clippy::disallowed_types, clippy::disallowed_methods)]
mod processes {
    use super::*;
    use std::io::Write as _;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    const HELPER: &str = "runtime::durable_tests::processes::durable_process_helper";

    struct ChildGuard(Child);

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn child(root: &Path, home: &Path, marker: &Path, mode: &str) -> ChildGuard {
        ChildGuard(
            Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "--exact",
                    HELPER,
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("NIKA_DURABLE_FIXTURE_ROOT", root)
                .env("NIKA_DURABLE_FIXTURE_HOME", home)
                .env("NIKA_DURABLE_FIXTURE_MARKER", marker)
                .env("NIKA_DURABLE_FIXTURE_MODE", mode)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("start fixture host"),
        )
    }

    fn await_marker(child: &mut ChildGuard, marker: &Path) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !marker.exists() {
            if let Some(status) = child.0.try_wait().expect("fixture state") {
                assert!(
                    status.success() && marker.exists(),
                    "fixture exited before its marker: {status}"
                );
                return;
            }
            assert!(
                Instant::now() < deadline,
                "fixture did not reach the reasoner"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn mark(path: &Path) {
        let mut file = std::fs::File::create(path).expect("fixture marker");
        file.write_all(b"READY\n").expect("write fixture marker");
        file.sync_all().expect("sync fixture marker");
    }

    struct BlockedReasoner(PathBuf);

    impl SessionReasoner for BlockedReasoner {
        fn name(&self) -> String {
            "blocked-local-fixture".to_owned()
        }

        fn reason(&mut self, _prompt: &str) -> Result<Reply, ReasonError> {
            mark(&self.0);
            loop {
                std::thread::park();
            }
        }
    }

    #[test]
    #[ignore = "child-process fixture, launched by the durable-session tests"]
    fn durable_process_helper() {
        let root =
            PathBuf::from(std::env::var_os("NIKA_DURABLE_FIXTURE_ROOT").expect("fixture root"));
        let home =
            PathBuf::from(std::env::var_os("NIKA_DURABLE_FIXTURE_HOME").expect("fixture home"));
        let marker =
            PathBuf::from(std::env::var_os("NIKA_DURABLE_FIXTURE_MARKER").expect("fixture marker"));
        if std::env::var("NIKA_DURABLE_FIXTURE_MODE").expect("fixture mode") == "blocked" {
            let mut session =
                SessionRuntime::open(&root, intelligence(), Box::new(BlockedReasoner(marker)));
            session.enable_history(&home).expect("durable child");
            let _ = session.turn(GOAL);
            panic!("the blocked reasoner unexpectedly returned");
        }
        let (mut session, _) = open(&root, &[ANSWER]);
        session.enable_history(&home).expect("durable child");
        assert!(matches!(session.turn(GOAL), TurnOutcome::Reply(_)));
        drop(session);
        mark(&marker);
    }

    #[test]
    fn a_second_process_recovers_the_completed_conversation() {
        let root = tempfile::tempdir().expect("project");
        let home = tempfile::tempdir().expect("home");
        let markers = tempfile::tempdir().expect("markers");
        let marker = markers.path().join("complete");
        let mut first = child(root.path(), home.path(), &marker, "complete");
        await_marker(&mut first, &marker);
        assert!(first.0.wait().expect("child exit").success());
        let (mut resumed, seen) = open(root.path(), &["A softer closing line."]);
        assert!(
            resumed
                .enable_history(home.path())
                .expect("second process opens")
                .is_some()
        );
        assert_eq!(resumed.intent.goal.as_deref(), Some(GOAL));
        assert!(seen.lock().expect("record").is_empty());
        assert!(matches!(
            resumed.turn("Make its ending gentler."),
            TurnOutcome::Reply(_)
        ));
        assert!(seen.lock().expect("record")[0].contains(ANSWER));
    }

    #[test]
    fn an_active_process_excludes_another_writer_and_sigkill_leaves_uncertainty() {
        let root = tempfile::tempdir().expect("project");
        let home = tempfile::tempdir().expect("home");
        let markers = tempfile::tempdir().expect("markers");
        let marker = markers.path().join("reasoning");
        let mut first = child(root.path(), home.path(), &marker, "blocked");
        await_marker(&mut first, &marker);
        let (mut concurrent, seen) = open(root.path(), &[ANSWER]);
        let before = Instant::now();
        assert!(
            concurrent.enable_history(home.path()).is_err(),
            "one writer per project history"
        );
        assert!(
            before.elapsed() < Duration::from_secs(2),
            "lease acquisition must not block"
        );
        assert!(seen.lock().expect("record").is_empty());
        drop(concurrent);
        first.0.kill().expect("SIGKILL the fixture on Unix");
        assert!(!first.0.wait().expect("reap killed fixture").success());
        let (mut resumed, seen) = open(root.path(), &[ANSWER]);
        let notice = resumed
            .enable_history(home.path())
            .expect("lease released by death")
            .expect("recovery notice");
        assert_uncertain(&notice);
        assert!(
            seen.lock().expect("record").is_empty(),
            "recovery never replays the interrupted turn"
        );
        assert!(resumed.pending_proposal().is_none());
        assert!(resumed.waiting_gate().is_none());
        assert!(
            !root.path().join(".nika").exists(),
            "no workflow or trace was created"
        );
    }
}

// Append inside runtime::durable_tests, whose existing helpers provide
// open/history_dir/refused/assert_uncertain and the scripted fixtures.
// These are injected late errors after real local writes, not simulated
// hardware failures or claims that a real fsync error occurred.

#[test]
fn closing_expires_a_pending_proposal_in_ephemeral_and_durable_sessions() {
    for durable in [false, true] {
        let root = tempfile::tempdir().expect("project");
        let home = tempfile::tempdir().expect("home");
        let (mut session, _) = open(root.path(), &[PROPOSAL]);
        if durable {
            session.enable_history(home.path()).expect("fresh history");
        }
        let TurnOutcome::Proposal { id, .. } = session.turn("Write me a daily digest workflow.")
        else {
            panic!("proposal expected");
        };
        assert!(matches!(session.turn("/quit"), TurnOutcome::Quit));
        assert!(session.pending_proposal().is_none());
        assert_eq!(
            refused(session.consent_to(&id, "yes")).class,
            RefusalClass::WrongState
        );
        assert!(!root.path().join("daily.nika.yaml").exists());
    }
}

#[test]
fn closing_expires_a_pending_gate_without_preparing_a_resume() {
    let root = tempfile::tempdir().expect("project");
    let (mut session, _) = open(root.path(), &[ANSWER]);
    // Seed a gate directly: this test concerns closing the existing machine
    // state, not tracing or executing a workflow to manufacture that state.
    let trace = root.path().join("paused.ndjson");
    session.pending_gate = Some(crate::change::PendingGate {
        workflow: PathBuf::from("daily.nika.yaml"),
        trace: trace.clone(),
        task: "approval".to_owned(),
        message: "Continue?".to_owned(),
        mode: "confirm".to_owned(),
    });
    let id = session.waiting_gate().expect("fixture gate");
    assert!(matches!(session.turn("/exit"), TurnOutcome::Quit));
    assert!(session.waiting_gate().is_none());
    assert_eq!(
        refused(session.answer_gate_for(&id, "yes")).class,
        RefusalClass::WrongState
    );
    assert!(
        !trace.exists(),
        "closing never executes or manufactures a trace"
    );
}

#[test]
fn a_late_history_activation_error_also_blocks_the_ephemeral_runtime() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut session, seen) = open(root.path(), &[ANSWER]);
    assert!(matches!(session.turn(GOAL), TurnOutcome::Reply(_)));
    assert_eq!(seen.lock().expect("record").len(), 1);
    assert_eq!(
        session
            .enable_history(home.path())
            .expect_err("activation is too late")
            .class,
        RefusalClass::WrongState
    );
    assert!(matches!(
        session.turn("Compose a second poem."),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(
        seen.lock().expect("record").len(),
        1,
        "no silent ephemeral fallback"
    );
    assert!(!home.path().join(".nika/sessions").exists());
    assert!(matches!(session.turn("/quit"), TurnOutcome::Quit));
}

#[test]
fn a_completed_io_refusal_preserves_effect_uncertainty_and_its_context() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let path = root.path().join("partial.nika.yaml");
    let (mut session, _) = open(root.path(), &[ANSWER]);
    session.enable_history(home.path()).expect("fresh history");
    let detail = "partial.nika.yaml may have changed: injected error after replacement";
    let outcome = session.recorded(super::history::Operation::Consent, "yes", |_| {
        nika_fs::OwnedDir::open(root.path())
            .expect("project capability")
            .write_atomic("partial.nika.yaml", "nika: partially-observed\n")
            .expect("real completed replacement before the injected error");
        TurnOutcome::Refusal(Refusal::new(RefusalClass::Io, detail))
    });
    assert_eq!(refused(outcome).class, RefusalClass::Io);
    assert!(
        path.exists(),
        "the injected refusal follows an actual file effect"
    );
    // A subsequent successful conversation turn is not reconciliation of the
    // earlier effect and must not erase its uncertainty on the next replay.
    assert!(matches!(session.turn(GOAL), TurnOutcome::Reply(_)));
    drop(session);
    let (mut resumed, seen) = open(root.path(), &[ANSWER]);
    let notice = resumed
        .enable_history(home.path())
        .expect("resume")
        .expect("recovery notice");
    assert_uncertain(&notice);
    assert!(seen.lock().expect("record").is_empty());
    assert!(matches!(
        resumed.turn("Describe the current situation."),
        TurnOutcome::Reply(_)
    ));
    let prompt = seen.lock().expect("record")[0].clone();
    assert!(
        prompt.contains(detail),
        "the earlier refusal remains conversational evidence"
    );
    assert_uncertain(&prompt);
    assert_eq!(
        std::fs::read_to_string(path).expect("unchanged effect"),
        "nika: partially-observed\n"
    );
}

#[test]
fn a_failed_final_journal_barrier_withholds_the_run_request_after_real_apply() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut session, seen) = open(root.path(), &[PROPOSAL]);
    session.enable_history(home.path()).expect("fresh history");
    let TurnOutcome::Proposal { id, .. } =
        session.turn("Write me a daily digest workflow and run it once.")
    else {
        panic!("proposal expected");
    };
    let journal = history_dir(home.path(), root.path()).join("events.ndjson");
    let saved = journal.with_extension("before-completion");
    let outcome = session.recorded(super::history::Operation::Consent, "yes", |runtime| {
        // This invokes the real apply/check/run-request path. The fixture
        // obstructs only the subsequent journal append, before the host is
        // allowed to see and execute the returned RunRequested.
        let prepared = runtime.consent_unrecorded("yes");
        assert!(matches!(prepared, TurnOutcome::RunRequested { .. }));
        std::fs::rename(&journal, &saved).expect("preserve started journal");
        std::fs::create_dir(&journal).expect("prevent completion append");
        prepared
    });
    assert_eq!(refused(outcome).class, RefusalClass::Io);
    let landed = std::fs::read(root.path().join("daily.nika.yaml")).expect("actual applied file");
    let calls = seen.lock().expect("record").len();
    assert!(matches!(
        session.turn("Compose a second poem."),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(seen.lock().expect("record").len(), calls);
    drop(session);

    std::fs::remove_dir(&journal).expect("remove obstruction");
    std::fs::rename(&saved, &journal).expect("restore interrupted journal");
    let (mut resumed, seen) = open(root.path(), &[ANSWER]);
    let notice = resumed
        .enable_history(home.path())
        .expect("resume")
        .expect("recovery notice");
    assert_uncertain(&notice);
    assert_eq!(
        refused(resumed.consent_to(&id, "yes")).class,
        RefusalClass::WrongState
    );
    assert!(
        seen.lock().expect("record").is_empty(),
        "replay does not call a model"
    );
    assert!(matches!(
        resumed.turn("Describe the current situation."),
        TurnOutcome::Reply(_)
    ));
    assert_uncertain(&seen.lock().expect("record")[0]);
    assert_eq!(
        std::fs::read(root.path().join("daily.nika.yaml")).expect("same applied bytes"),
        landed
    );
    assert!(
        !root.path().join(".nika/traces").exists(),
        "the withheld request was never executed"
    );
}

// Append inside runtime::durable_tests. The fake value is intentionally
// recognized by the line-oriented redactor in the raw reply: formatting
// Debug first must not flatten the two lines and let it escape via outcome.

#[test]
fn diagnostic_history_cannot_leak_a_value_masked_in_the_raw_reply() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let reply = "token: ${{ secrets.reference }}\ntoken: FAKE_PRIVATE_VALUE_FROM_SECOND_LINE";
    assert!(
        !crate::broker::redact(reply)
            .0
            .contains("FAKE_PRIVATE_VALUE_FROM_SECOND_LINE"),
        "the fixture is a recognized secret in raw text"
    );
    let (mut session, _) = open(root.path(), &[reply]);
    session.enable_history(home.path()).expect("fresh history");
    assert!(matches!(session.turn(GOAL), TurnOutcome::Reply(_)));
    drop(session);
    let journal =
        std::fs::read_to_string(history_dir(home.path(), root.path()).join("events.ndjson"))
            .expect("journal");
    assert!(
        !journal.contains("FAKE_PRIVATE_VALUE_FROM_SECOND_LINE"),
        "an outcome diagnostic persisted a value already masked in the saved conversation"
    );
}

#[cfg(unix)]
#[allow(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "bounded process fixture: a blocked FIFO opener must be killable by its parent"
)]
mod fifo_history_regression {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct KillOnDrop(Child);

    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    #[ignore = "process fixture for FIFO refusal; never opened by the parent"]
    fn open_fifo_history_child() {
        let root = PathBuf::from(
            std::env::var_os("NIKA_FIFO_HISTORY_FIXTURE_ROOT").expect("fixture root"),
        );
        let home = PathBuf::from(
            std::env::var_os("NIKA_FIFO_HISTORY_FIXTURE_HOME").expect("fixture home"),
        );
        let marker = PathBuf::from(
            std::env::var_os("NIKA_FIFO_HISTORY_FIXTURE_MARKER").expect("fixture marker"),
        );
        let (mut session, seen) = open(&root, &[ANSWER]);
        std::fs::write(marker, b"about to open history\n").expect("fixture progress");
        assert!(
            session.enable_history(&home).is_err(),
            "a FIFO is not a regular history file"
        );
        assert!(seen.lock().expect("record").is_empty());
    }

    #[test]
    fn a_fifo_history_is_refused_without_blocking_the_host() {
        use std::os::unix::fs::FileTypeExt as _;

        let root = tempfile::tempdir().expect("project");
        let home = tempfile::tempdir().expect("home");
        let markers = tempfile::tempdir().expect("markers");
        let (mut first, _) = open(root.path(), &[ANSWER]);
        first
            .enable_history(home.path())
            .expect("fresh regular history");
        drop(first);
        let journal = history_dir(home.path(), root.path()).join("events.ndjson");
        std::fs::remove_file(&journal).expect("remove regular fixture journal");
        assert!(
            Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&journal)
                .status()
                .expect("Unix mkfifo fixture command")
                .success()
        );
        assert!(
            std::fs::symlink_metadata(&journal)
                .expect("FIFO metadata")
                .file_type()
                .is_fifo()
        );
        let marker = markers.path().join("opening");
        let mut child = KillOnDrop(
            Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "--exact",
                    "runtime::durable_tests::fifo_history_regression::open_fifo_history_child",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("NIKA_FIFO_HISTORY_FIXTURE_ROOT", root.path())
                .env("NIKA_FIFO_HISTORY_FIXTURE_HOME", home.path())
                .env("NIKA_FIFO_HISTORY_FIXTURE_MARKER", &marker)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("start FIFO fixture host"),
        );
        let started = Instant::now();
        let mut opening = None;
        let status = loop {
            if marker.exists() {
                opening.get_or_insert_with(Instant::now);
            }
            if let Some(status) = child.0.try_wait().expect("fixture state") {
                break status;
            }
            if let Some(opening) = opening {
                assert!(
                    opening.elapsed() < Duration::from_secs(5),
                    "opening a FIFO blocked the host; the child is killed on unwind"
                );
            } else {
                assert!(
                    started.elapsed() < Duration::from_secs(15),
                    "fixture did not start"
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            marker.exists(),
            "the child actually attempted to enable history"
        );
        assert!(
            status.success(),
            "the history was not cleanly refused: {status}"
        );
        assert!(
            std::fs::symlink_metadata(&journal)
                .expect("preserved FIFO")
                .file_type()
                .is_fifo()
        );
    }
}
