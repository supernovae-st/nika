// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Bytes a judge declined are never asked of it again (R6), end to end through the sketch door.
//! The judge declines candidate X with a located defect (its filter keeps the closed tickets,
//! and the judge names the task doing so); the door reopens the sketch, and the author answers
//! the same sketch and fill, so the compiler emits X again, byte for byte. The judge is not
//! asked a second time: each later attempt repeats the earlier verdict with no call and names
//! the attempt it repeats. The door stops: past its round count it withdraws the candidate
//! naming the defect; under no count the repeat is no progress, the stall opens the source
//! recovery, and a recovered source of the same bytes is not judged again either.
use super::*;

/// The program that keeps the closed tickets: the filter [`OPEN_ONLY`] asks, inverted.
const CLOSED: &str = "fromjson | map(select(.status != \"open\"))";

/// The judge's one verdict over the bytes of [`CLOSED`]: the request unfaithful, the read
/// carried, the filter missing, and the task keeping the wrong tickets named as its reason.
const DECLINED: [&str; 4] = ["unfaithful", "carried", "missing", "task-keep_open"];

/// The route step of an attempt that repeats an earlier verdict on the same bytes.
const SAME_BYTES: &str = "verify: same bytes, earlier verdict stands";

/// The judge's calls over X: its one verdict, never asked again.
const ONCE: [&str; 4] = ["judge_request", "judge_part", "judge_part", "judge_point"];

/// The author's sketch and its fill keeping the closed tickets, the judge's answers over them,
/// then the same sketch and fill again: a repair that writes the very bytes the judge declined.
fn declined_then_the_same() -> Vec<String> {
    let mut replies = vec![sketch(), fills(CLOSED)];
    replies.extend(DECLINED.iter().map(|c| json!({"choice": c}).to_string()));
    replies.extend([sketch(), fills(CLOSED)]);
    replies
}

/// The bytes the sketch door emits from the sketch and the fill of [`CLOSED`], as an approving
/// judge leaves them READY.
async fn emitted() -> String {
    let author = Rotating::new(vec![sketch(), fills(CLOSED)]);
    let judged = Judged::approving(&author);
    let request = CompileRequest::create(TICKETS).with_authoring_policy(policy(NativeMode::Sketch));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    out.candidate.unwrap()
}

/// The defect the judge located, as the door hands it back to the author with its reason.
fn located() -> String {
    format!(
        "the judge compared the whole request with the candidate's bytes: it does not carry « {OPEN_ONLY} » · the judge's reason: {}",
        pointed_to("keep_open")
    )
}

/// The author (and the judge it answers for) of [`declined_then_the_same`], keeping the text of
/// the last user turn of every call it receives, in call order.
struct Recording {
    author: Rotating,
    told: std::sync::Mutex<Vec<String>>,
}

impl Recording {
    fn new(replies: Vec<String>) -> Self {
        Self {
            author: Rotating::new(replies),
            told: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn told(&self) -> Vec<String> {
        self.told.lock().unwrap().clone()
    }
}

impl ProviderInferDyn for Recording {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        use nika_kernel::ai::provider::{ContentBlock, Role};
        let user = (request.messages.iter().rev())
            .find(|message| message.role == Role::User)
            .map(|message| {
                (message.content.iter())
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>()
            })
            .unwrap_or_default();
        self.told.lock().unwrap().push(user);
        self.author.infer(request).await
    }
}

/// The verdict of the first attempt as a repeated attempt states it again, with no call:
/// everything but what the call itself spent and asked, the attempt it repeats, and its own
/// `attempt` (the verdicts this compile recorded before it).
fn repeated_verdict(first: &Value, attempt: u64) -> Value {
    let mut verdict = first.clone();
    let spent = json!({"calls": 0, "input_tokens": 0, "output_tokens": 0, "complete": true});
    for (key, value) in [
        ("attempt", json!(attempt)),
        ("questions", json!([])),
        ("attempted", json!(0)),
        ("returned", json!(0)),
        ("consumed", json!(0)),
        ("usage", spent),
        ("same_bytes_as", json!(0)),
    ] {
        verdict[key] = value;
    }
    verdict
}

/// The first attempt judged X itself: its digest, its located defect and its reason, declined
/// and rejected, repeating nothing.
async fn assert_judged_once(attempt: &Value) {
    let sha = nika_compile::surface::sha256(&emitted().await);
    assert_eq!(attempt["candidate_sha256"], json!(sha), "{attempt:#}");
    assert_eq!(attempt["same_bytes_as"], Value::Null, "{attempt:#}");
    assert_eq!(attempt["defects"], json!([OPEN_ONLY]), "{attempt:#}");
    let noted = json!([{"defect": OPEN_ONLY, "note": pointed_to("keep_open")}]);
    assert_eq!(attempt["notes"], noted, "{attempt:#}");
    let flags = (
        &attempt["declined"],
        &attempt["rejected"],
        &attempt["attempted"],
    );
    assert_eq!(
        flags,
        (&json!(true), &json!(true), &json!(4)),
        "{attempt:#}"
    );
}

/// The steps of the route, in order, that repeated or stopped the candidate.
fn stops(out: &CompileOutcome) -> Vec<String> {
    let route = &out.provenance.decision.as_ref().unwrap()["route"];
    (route.as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .filter(|step| {
            step.starts_with("verify:")
                || *step == "native: no progress"
                || step.contains("recovery")
        })
        .map(str::to_owned)
        .collect()
}

/// Under a repair count whose last round the repair spends, the repair that writes the declined
/// bytes again asks the judge nothing: the second attempt repeats the first verdict on the same
/// digest with no call, and the door, its rounds spent, withdraws the candidate naming the
/// defect and the judge's reason.
#[tokio::test]
async fn a_repair_writing_the_declined_bytes_again_is_never_judged_again() {
    let provider = Recording::new(declined_then_the_same());
    let request =
        CompileRequest::create(TICKETS).with_authoring_policy(allowing(NativeMode::Sketch, 2));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(out.provenance.plan.is_none(), "{out:#?}");
    assert_eq!(provider.author.calls.load(Ordering::SeqCst), 8, "{out:#?}");
    let mut asked = vec!["sketch", "fill"];
    asked.extend(ONCE);
    asked.extend(["sketch-repair", "fill"]);
    assert_eq!(calls(&out), asked, "{out:#?}");
    // The reopened sketch is handed the defect the judge located, with its reason.
    let told = provider.told();
    assert_eq!(told.len(), 8, "{told:#?}");
    assert!(told[6].contains(&located()), "{}", told[6]);
    let attempts = verification(&out);
    assert_eq!(attempts.len(), 2, "{attempts:#?}");
    assert_judged_once(&attempts[0]).await;
    assert_eq!(
        attempts[1],
        repeated_verdict(&attempts[0], 1),
        "{attempts:#?}"
    );
    // The same bytes again are no progress, whatever round the count has left.
    let steps = [SAME_BYTES, "verify: not ready", "native: no progress"];
    assert_eq!(stops(&out), steps, "{out:#?}");
    let note = pointed_to("keep_open");
    let told = findings(&out, "semantic_verification");
    // One repair was made from the judge's defect before its bytes came back.
    assert_eq!(told, [not_carried(OPEN_ONLY, &note, 1)], "{told:?}");
    assert_eq!(findings(&out, "verify_held"), Vec::<String>::new());
}

/// Under a repair count with rounds left, a repair that writes the declined bytes again is no
/// progress (P-SAME-BYTES-COUNTED, R6): the reused verdict withdraws the candidate at once, the
/// judge asked once in all, and the stop is named as the judge's, after the one repair it made.
#[tokio::test]
async fn a_counted_door_stops_at_the_first_repeat_of_the_declined_bytes() {
    let provider = Rotating::new(declined_then_the_same());
    let request =
        CompileRequest::create(TICKETS).with_authoring_policy(allowing(NativeMode::Sketch, 4));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 8, "{out:#?}");
    assert_eq!(judge_calls(&out), ONCE);
    let attempts = verification(&out);
    assert_eq!(attempts.len(), 2, "{attempts:#?}");
    assert_eq!(
        attempts[1],
        repeated_verdict(&attempts[0], 1),
        "{attempts:#?}"
    );
    assert!(route(&out).contains("native: no progress"), "{out:#?}");
    assert_eq!(
        findings(&out, "rehearsal"),
        Vec::<String>::new(),
        "{out:#?}"
    );
    let note = pointed_to("keep_open");
    let told = findings(&out, "semantic_verification");
    assert_eq!(told, [not_carried(OPEN_ONLY, &note, 1)], "{told:?}");
}

/// Under no repair count, the repeat of the declined bytes is no progress: the door stops and
/// opens the source recovery, whose whole source, the same bytes again, is not judged either:
/// one more attempt repeats the first verdict, the recovery stops on the same findings, and the
/// judge was asked once in all.
#[tokio::test]
async fn a_recovered_source_of_the_declined_bytes_is_never_judged_again() {
    let bytes = emitted().await;
    let mut replies = declined_then_the_same();
    replies.push(answer(&bytes));
    let provider = Rotating::new(replies);
    let request = CompileRequest::create(TICKETS).with_authoring_policy(
        AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Sketch)
            .with_unbounded_repairs(),
    );
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 9, "{out:#?}");
    let mut asked = vec!["sketch", "fill"];
    asked.extend(ONCE);
    asked.extend(["sketch-repair", "fill", "source-recovery"]);
    assert_eq!(calls(&out), asked, "{out:#?}");
    let attempts = verification(&out);
    assert_eq!(attempts.len(), 3, "{attempts:#?}");
    assert_judged_once(&attempts[0]).await;
    // Each repeat is its own attempt: one repair, then two, came before it.
    assert_eq!(
        attempts[1],
        repeated_verdict(&attempts[0], 1),
        "{attempts:#?}"
    );
    assert_eq!(
        attempts[2],
        repeated_verdict(&attempts[0], 2),
        "{attempts:#?}"
    );
    // The structured doors' same-bytes stop, the recovery it opened, then the recovery's own
    // repeat of the declined bytes, in order: withdrawn, never judged again, no progress.
    let steps = [
        SAME_BYTES,
        "verify: not ready",
        "native: no progress",
        "native: source recovery after structured exhaustion",
        SAME_BYTES,
        "verify: not ready",
        "native: no progress",
    ];
    assert_eq!(stops(&out), steps, "{out:#?}");
    // The stop is the judge's, named as such: no evidence refused these bytes.
    assert_eq!(
        findings(&out, "rehearsal"),
        Vec::<String>::new(),
        "{out:#?}"
    );
    let note = pointed_to("keep_open");
    let told = findings(&out, "semantic_verification");
    // Each stop names the repairs its door made before the bytes came back.
    let stopped = [
        not_carried(OPEN_ONLY, &note, 1),
        not_carried(OPEN_ONLY, &note, 2),
    ];
    assert_eq!(told, stopped, "{told:?}");
    let opened = "Source recovery opened: the structured doors made no further progress under a policy with no repair count, and a whole-source request to the same seat was attempted under the same authority. Only a READY outcome states that a recovered source passed every check.";
    assert_eq!(findings(&out, "authoring_recovery"), [opened], "{out:#?}");
}
