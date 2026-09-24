// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The live conversation over the actual Session runtime: its one-time
//! unknown-cost choice is a fresh spending question, and a retained Run review
//! keeps its contract. Hermetic mechanics only: the stub route never answers
//! and no socket opens, so no model, billing or UX is qualified here.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use nika_cli_host::lane::{ChildSlot, drive_reviewed_child};
use nika_session::intelligence::IntelligenceKind;
use nika_session::{CostHostEvidence, ReasonError, Reply, ScriptedReasoner, SessionReasoner};
use std::sync::atomic::{AtomicUsize, Ordering};

const MODEL: &str = "deepseek/s90-unpriced-fixture";
const SOURCE: &str = "nika: fresh-run\nmodel: deepseek/s90-unpriced-fixture\ntasks:\n  answer:\n    infer: { prompt: Say OK, max_tokens: 32 }\noutputs:\n  result: ${{ tasks.answer.output }}\n";
/// One challenge frame as the child's host review writes it (every identity
/// is a token no sentence of the copy contains).
const FRAME: &str = r#"{"schema":"nika/run-cost-challenge@1","nonce":"nonce-s90","candidate":"cand-s90","invocation":"inv-s90","route":{"provider":"deepseek","model":"s90-unpriced-fixture","endpoint":"https://api.deepseek.com/v1"},"source_sha256":"src-s90","inputs_sha256":"in-s90","question":"USD cost is unknown; a charge is possible on deepseek/s90-unpriced-fixture.\nAt most 1 requests; each at most 8192 output tokens and 120 seconds (at most 120 seconds of model wait). No automatic retry.\nOverrides only the shown defaults (invocation: $0.250000; project: none); no hard cap is overridden.\nContinue once? yes / no","native_price":"price and invoice unknown","review_details":"candidate cand-s90 · invocation inv-s90 · endpoint https://api.deepseek.com/v1 · host fixture"}"#;

/// A temporary project root, removed when the test ends.
struct Room(PathBuf);

impl Room {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path =
            std::env::temp_dir().join(format!("nika-tui-s90-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("room");
        Self(path)
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The selected unpriced route: it opts into admission, so the Session asks
/// its cost question, but carries no admission seam of its own. It counts
/// every unmetered call, which must never happen.
struct Route(Arc<AtomicUsize>);

impl SessionReasoner for Route {
    fn name(&self) -> String {
        "s90 route".to_owned()
    }
    fn reason(&mut self, _prompt: &str) -> Result<Reply, ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(ReasonError::Provider(
            "unmetered call on an unknown-cost route".to_owned(),
        ))
    }
    fn supports_admission(&self) -> bool {
        true
    }
    fn authoring_model(&self) -> Option<String> {
        Some(MODEL.to_owned())
    }
}

fn runners() -> Runners {
    Runners {
        run_once: Box::new(|_, _| panic!("no plain Run in this fixture")),
        run_resume: Box::new(|_, _, _, _| panic!("no resume in this fixture")),
        run_tapped: None,
    }
}

fn session_live(room: &Room, unmetered: &Arc<AtomicUsize>) -> Live {
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("deepseek".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Api {
            provider: "deepseek".to_owned(),
        },
        Some(MODEL.to_owned()),
    );
    let calls = Arc::clone(unmetered);
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(move |_| Box::new(Route(Arc::clone(&calls)))),
        runners(),
    )
    .with_cost_host_evidence(CostHostEvidence::unmanaged_interactive_local());
    let _ = live.open();
    live
}

/// A Run review whose child asks `FRAME` and copies its one-use reply pipe to
/// `reply.json` until EOF.
fn run_live(room: &Room) -> Live {
    std::fs::write(room.0.join("one.nika"), SOURCE).expect("workflow");
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        None,
    );
    let slot: ChildSlot = Arc::default();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        runners(),
    )
    .with_run_review(Box::new(move |root, _, busy| {
        let args = vec![
            "-c".to_owned(),
            "printf '%s\\n' \"$1\"; cat > reply.json".to_owned(),
            "fixture".to_owned(),
            FRAME.to_owned(),
        ];
        drive_reviewed_child(Path::new("/bin/sh"), &args, root, busy, &slot)
    }));
    let _ = live.open();
    live
}

fn said(beats: &[Beat]) -> Vec<(Kind, String)> {
    beats
        .iter()
        .filter_map(|beat| match beat {
            Beat::Say(block) => Some((block.kind, block.text.clone())),
            _ => None,
        })
        .collect()
}

fn question(beats: &[Beat]) -> String {
    said(beats)
        .into_iter()
        .find_map(|(kind, text)| (kind == Kind::Question).then_some(text))
        .unwrap_or_else(|| panic!("no question in {beats:?}"))
}

fn joined(beats: &[Beat]) -> String {
    said(beats)
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn waits(beats: &[Beat]) -> Option<Waiting> {
    beats.iter().rev().find_map(|beat| match beat {
        Beat::Wait(waiting) => Some(waiting.clone()),
        _ => None,
    })
}

#[test]
fn the_session_cost_question_is_fresh_and_its_details_answer_nothing() {
    let room = Room::new("details");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    assert!(!live.fresh_input_required());
    let asked = live.submit("hello").beats;
    let first = question(&asked);
    assert!(
        first.starts_with("Fresh authoring cost decision · this request only;"),
        "{first}"
    );
    for fact in [
        "USD cost is unknown",
        "At most 3 requests",
        "8192 output tokens",
        "120 seconds",
        "no hard cap is overridden",
    ] {
        assert!(first.contains(fact), "{fact}: {first}");
    }
    assert!(
        first.ends_with("\nContinue once? yes / no / details"),
        "{first}"
    );
    assert_eq!(first.matches("Continue once?").count(), 1, "{first}");
    assert!(!first.contains("candidate"), "{first}");
    assert_eq!(
        waits(&asked),
        Some(Waiting::Question {
            key: "unknown_cost".to_owned()
        })
    );
    assert!(live.fresh_input_required());
    assert_eq!(
        live.busy_label("yes").as_deref(),
        Some("answering the fresh authoring cost question")
    );
    let details = question(&live.submit("details").beats);
    for evidence in [
        "candidate ",
        "invocation session:",
        "endpoint https://",
        "host ",
    ] {
        assert!(details.contains(evidence), "{evidence}: {details}");
    }
    assert_eq!(
        question(&live.submit(" DETAILS ").beats),
        details,
        "the same review, unchanged"
    );
    assert!(live.fresh_input_required(), "reading the details answered");
    assert_eq!(unmetered.load(Ordering::SeqCst), 0);
}

#[test]
fn an_interruption_cancels_the_session_choice_and_sends_nothing() {
    let room = Room::new("cancel");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    let _ = live.submit("hello");
    assert!(live.fresh_input_required());
    let cancelled = live.cancel_pending();
    let text = joined(&cancelled);
    assert!(text.contains("cancelled; nothing sent"), "{text}");
    assert_eq!(waits(&cancelled), Some(Waiting::Free));
    assert!(!live.fresh_input_required());
    assert!(
        live.cancel_pending().is_empty(),
        "a second interruption finds nothing to cancel"
    );
    let late = joined(&live.submit("yes").beats);
    assert!(late.contains("nothing waits for a yes or a no"), "{late}");
    assert!(!live.fresh_input_required());
    assert_eq!(unmetered.load(Ordering::SeqCst), 0);
}

#[test]
fn only_a_fresh_yes_after_the_question_reaches_the_admission_seam_once() {
    let room = Room::new("yes");
    let unmetered = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &unmetered);
    let _ = live.submit("hello");
    // This route has no seam of its own: the Session's default seam refuses
    // the one admitted call, so reaching it is the evidence of the approval.
    let answered = joined(&live.submit("yes").beats);
    assert!(answered.contains("no catalog admission seam"), "{answered}");
    assert!(!live.fresh_input_required());
    let again = joined(&live.submit("yes").beats);
    assert!(again.contains("nothing waits for a yes or a no"), "{again}");
    assert_eq!(
        unmetered.load(Ordering::SeqCst),
        0,
        "an approval never falls back to an unmetered call"
    );
}

#[test]
fn a_run_review_keeps_its_contract_and_details_leave_the_challenge_untouched() {
    let room = Room::new("run");
    let mut live = run_live(&room);
    let asked = live.submit("run one.nika").beats;
    let first = question(&asked);
    assert!(
        first.starts_with(
            "Fresh Run cost decision · deepseek/s90-unpriced-fixture at https://api.deepseek.com\n"
        ),
        "{first}"
    );
    assert!(
        first.ends_with("\nContinue once? yes / no / details"),
        "{first}"
    );
    for identity in [
        "nonce-s90",
        "cand-s90",
        "inv-s90",
        "src-s90",
        "in-s90",
        "/v1",
    ] {
        assert!(!first.contains(identity), "{identity}: {first}");
    }
    assert_eq!(
        waits(&asked),
        Some(Waiting::Question {
            key: "run_cost".to_owned()
        })
    );
    assert!(live.fresh_input_required());
    let details = question(&live.submit("details").beats);
    for evidence in [
        "challenge nonce-s90",
        "Endpoint: https://api.deepseek.com/v1\n",
        "Source SHA-256: src-s90\n",
        "Input SHA-256: in-s90\n",
        "Candidate: cand-s90\n",
        "Invocation: inv-s90\n",
        "Host and cap evidence: candidate cand-s90",
    ] {
        assert!(details.contains(evidence), "{evidence}: {details}");
    }
    let reply = room.0.join("reply.json");
    assert!(
        std::fs::read(&reply).unwrap_or_default().is_empty(),
        "reading the details answered the child"
    );
    assert!(live.fresh_input_required());
    let _ = live.submit("yes");
    let sent = std::fs::read_to_string(&reply).expect("the one reply");
    assert!(sent.contains(r#""nonce":"nonce-s90""#), "{sent}");
    assert!(sent.ends_with(r#""yes":true}"#), "{sent}");
    assert_eq!(
        sent.matches("nika/run-cost-response@1").count(),
        1,
        "{sent}"
    );
    assert!(!live.fresh_input_required());
}

#[test]
fn an_interruption_ends_a_run_review_without_a_reply() {
    let room = Room::new("run-cancel");
    let mut live = run_live(&room);
    let _ = live.submit("run one.nika");
    assert!(live.fresh_input_required());
    let text = joined(&live.cancel_pending());
    assert!(
        text.contains("Run cost decision cancelled; nothing sent"),
        "{text}"
    );
    assert!(!live.fresh_input_required());
    assert!(live.cancel_pending().is_empty());
    let reply = std::fs::read(room.0.join("reply.json")).unwrap_or_default();
    assert!(reply.is_empty(), "a cancelled review answered: {reply:?}");
}

#[test]
fn the_authoring_projection_keeps_any_other_wording_whole() {
    assert_eq!(
        authoring_cost_question("A sentence the Session wrote."),
        "Fresh authoring cost decision · this request only; approving it never saves or runs anything\nA sentence the Session wrote.\nContinue once? yes / no / details"
    );
}
