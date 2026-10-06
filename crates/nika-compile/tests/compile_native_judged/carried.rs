// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A rejection carried across compiles (R6), end to end through the sketch door. A host that
//! authors again after a held round (the Session's rewrite, a fresh CLI or Serve round) may emit
//! the very bytes the judge rejected: the request carries that round's verdicts
//! (`CompileRequest::with_declined`), and a judge of the new compile is never asked again on
//! bytes it rejected. The verdict is repeated with no call, recorded `carried`, and the
//! candidate is held as before: a second vote never outvotes the first. An abstention is not a
//! rejection: it is not carried, and the new round asks its judge, who may decide it.
use super::observed::Judging;
use super::*;
use nika_compile_cognition::decide::DecisionSeat;
use nika_compile_cognition::{Cognition, compile_with_cognition};

/// The faithful program of [`TICKETS`]: the open tickets kept.
const OPEN: &str = "fromjson | map(select(.status == \"open\"))";

/// The judge's doubt that locates nothing: the request unfaithful, its two parts carried, no task
/// doing more than asked.
const REJECTED: [(&str, &str); 4] = [
    ("verify-request", "unfaithful"),
    ("verify-part-0", "carried"),
    ("verify-part-1", "carried"),
    ("verify-extra", "only_requested"),
];

/// The same doubt as an abstention: the judge chooses none on the whole request.
const ABSTAINED: [(&str, &str); 4] = [
    ("verify-request", "none"),
    ("verify-part-0", "carried"),
    ("verify-part-1", "carried"),
    ("verify-extra", "only_requested"),
];

/// The route step of a verdict a host carried from an earlier round.
const CARRIED: &str = "verify: same bytes, rejected in an earlier round";

/// What an abstention holds, as the verifier states it.
const HELD_ABSTAINED: &str = "The verifier read the candidate and abstained: it neither accepted nor rejected it, and located no defect. It is shown, never offered, and nothing was written; it is not asked again on these bytes in this compile. A correction of the request, another verifier, or a new round that authors again can decide it.";

/// One compile of [`TICKETS`] through the sketch door, judged by `judge`: the author answers the
/// same sketch and fill every time (the same bytes), and the request carries `declined`, the
/// verification attempts the host kept from earlier rounds.
async fn round(judge: &Judging, declined: Vec<Value>) -> CompileOutcome {
    let author = Rotating::new(vec![sketch(), fills(OPEN)]);
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(judge as &dyn DecisionSeat),
    };
    let request = CompileRequest::create(TICKETS)
        .with_authoring_policy(policy(NativeMode::Sketch))
        .with_declined(declined);
    let out = compile_with_cognition(&request, cognition).await.unwrap();
    assert_eq!(author.calls.load(Ordering::SeqCst), 2, "{out:#?}");
    out
}

/// The flags of a verification attempt: declined, rejected, settled, carried and the attempt it
/// repeats in its own compile.
fn flags(attempt: &Value) -> [&Value; 5] {
    [
        "declined",
        "rejected",
        "settled",
        "carried",
        "same_bytes_as",
    ]
    .map(|key| &attempt[key])
}

/// The same request again after a held rejection: the host carries the round's attempts, the
/// author writes the same bytes, and the judge, whose rejection they carry, is asked nothing. The
/// attempt repeats its verdict with no call, `carried` and repeating no attempt of this compile,
/// and the candidate is held again with the same findings: never READY on a second vote.
#[tokio::test]
async fn a_rejection_carried_from_an_earlier_round_is_repeated_with_no_call() {
    let first_judge = Judging::new(&REJECTED);
    let first = round(&first_judge, Vec::new()).await;
    assert_held(&first);
    assert_eq!(first_judge.left(), 0, "{first:#?}");
    let attempts = verification(&first);
    assert_eq!(attempts.len(), 1, "{attempts:#?}");
    let judged = &attempts[0];
    let rejected = [
        json!(true),
        json!(true),
        json!(false),
        json!(false),
        Value::Null,
    ];
    assert_eq!(flags(judged), rejected.each_ref(), "{judged:#}");
    assert_eq!(judged["attempted"], json!(4), "{judged:#}");
    let told = findings(&first, "semantic_verification");
    assert_eq!(told, [contested_whole(NO_TRIAL)], "{told:?}");
    // The next compile of the same request, carrying the round's verdicts.
    let second_judge = Judging::new(&[]);
    let second = round(&second_judge, attempts.clone()).await;
    assert_held(&second);
    assert_eq!(
        second.candidate, first.candidate,
        "the same bytes: {second:#?}"
    );
    assert_eq!(second_judge.ids(), Vec::<String>::new(), "{second:#?}");
    let repeated = verification(&second);
    assert_eq!(repeated.len(), 1, "{repeated:#?}");
    let mut carried = judged.clone();
    let spent = json!({"calls": 0, "input_tokens": 0, "output_tokens": 0, "complete": true});
    for (key, value) in [
        ("questions", json!([])),
        ("attempted", json!(0)),
        ("returned", json!(0)),
        ("consumed", json!(0)),
        ("usage", spent),
        ("carried", json!(true)),
    ] {
        carried[key] = value;
    }
    assert_eq!(repeated[0], carried, "{repeated:#?}");
    assert!(route(&second).contains(CARRIED), "{second:#?}");
    assert_eq!(findings(&second, "semantic_verification"), told);
}

/// An abstention held in an earlier round is not carried: the same bytes again are asked of the
/// judge, who decides them this time; the request then carries the verdict READY.
#[tokio::test]
async fn an_abstention_from_an_earlier_round_is_asked_again() {
    let first_judge = Judging::new(&ABSTAINED);
    let first = round(&first_judge, Vec::new()).await;
    assert_eq!(first.status, CompileStatus::Incomplete, "{first:#?}");
    assert_eq!(first_judge.left(), 0, "{first:#?}");
    assert!(first.provenance.plan.is_none(), "{first:#?}");
    let held = findings(&first, "verify_held");
    assert_eq!(held, [HELD_ABSTAINED], "{first:#?}");
    let attempts = verification(&first);
    let abstained = [
        json!(true),
        json!(false),
        json!(false),
        json!(false),
        Value::Null,
    ];
    assert_eq!(flags(&attempts[0]), abstained.each_ref(), "{attempts:#?}");
    assert_eq!(attempts[0]["unknown"], json!([TICKETS]), "{attempts:#?}");
    assert_eq!(attempts[0]["doubt"], json!(["none"]), "{attempts:#?}");
    let second_judge = Judging::new(&[("verify-request", "faithful")]);
    let second = round(&second_judge, attempts).await;
    assert_eq!(second.status, CompileStatus::Ready, "{second:#?}");
    assert_eq!(
        second.candidate, first.candidate,
        "the same bytes: {second:#?}"
    );
    assert_eq!(second_judge.ids(), ["verify-request"], "{second:#?}");
    let decided = verification(&second);
    assert_eq!(decided.len(), 1, "{decided:#?}");
    let carried = (&decided[0]["carried"], &decided[0]["settled_by"]);
    assert_eq!(carried, (&json!(false), &json!("verify-request")));
    let asked = (&decided[0]["attempted"], &decided[0]["same_bytes_as"]);
    assert_eq!(asked, (&json!(1), &Value::Null), "{decided:#?}");
    assert!(!route(&second).contains(CARRIED), "{second:#?}");
}
