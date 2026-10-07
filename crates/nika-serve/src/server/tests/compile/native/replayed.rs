// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An answer round judged by the operator's seat (R4 A11, R6): `cognition: explicitProvider`
//! with the token of a kept round replays that round's plan with this round's answers, and the
//! seat is asked only to judge the replayed bytes, never to author again. A candidate the judge
//! carries is READY, generation 2. A candidate it does not accept is held and its token
//! forgotten: those bytes are never put to the same judge again through it.

use super::authority::{answered, roles};
use super::*;

/// A judged answer round of the kept round `token` of [`INTENT`], with `fields` over it.
fn judged_replay(token: &str, fields: &Value) -> String {
    let mut body: Value = serde_json::from_str(&replay(token, fields)).expect("a replay body");
    body["cognition"] = json!("explicitProvider");
    body.to_string()
}

/// The question round is kept; its judged answer round asks the seat one question, the whole
/// request over the replayed bytes, and no authoring call: the answer is baked into the kept
/// plan's candidate, which the judge carries, READY.
#[tokio::test(flavor = "multi_thread")]
async fn a_judged_answer_round_replays_the_kept_plan_and_asks_the_seat_only_to_judge() {
    let world = TestWorld::new();
    let mut script = question_round();
    script.push(Reply::Text(JUDGE_APPROVES.to_owned()));
    let seat = Seat::start(script);
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _backend) = start_native(&world, compile_limits(), operator).await;
    let first = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(first.status, 200, "{}", first.body);
    let token = token_of(&first);
    assert_eq!(seat.calls(), 3, "the plan, the sketch and its fill");
    let judged = server
        .request(&compile_request(&judged_replay(&token, &answered())))
        .await;
    assert_eq!(judged.status, 200, "{}", judged.body);
    assert_eq!(judged.header("cache-control"), Some("no-store"));
    let document = judged.json();
    assert_eq!(document["compile_version"], 2, "a judge call happened");
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(seat.calls(), 4, "one judge question, no authoring call");
    let asked: Vec<String> = (roles(&document).into_iter())
        .map(|(role, _)| role)
        .collect();
    assert_eq!(asked, ["judge_request"], "{document:#}");
    let candidate = document["candidate"].as_str().expect("a candidate");
    assert!(
        candidate.contains(&format!("model: {RUN_MODEL}")),
        "{candidate}"
    );
    server.stop().await.expect("clean stop");
}

/// A judged answer round whose judge does not accept the replayed bytes holds them: INCOMPLETE
/// with the `verify_held` finding, no new token, and the kept round's token forgotten, so the
/// same bytes are never put to the same judge again through it.
#[tokio::test(flavor = "multi_thread")]
async fn a_judged_answer_round_the_judge_declines_holds_and_forgets_its_token() {
    let world = TestWorld::new();
    let mut script = question_round();
    // The whole request rejected; every part then carried, the last reply repeating.
    script.push(Reply::Text(json!({"choice": "unfaithful"}).to_string()));
    script.push(Reply::Text(json!({"choice": "carried"}).to_string()));
    let seat = Seat::start(script);
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _backend) = start_native(&world, compile_limits(), operator).await;
    let first = server.request(&compile_request(&fresh(&json!({})))).await;
    let token = token_of(&first);
    let judged = server
        .request(&compile_request(&judged_replay(&token, &answered())))
        .await;
    assert_eq!(judged.status, 200, "{}", judged.body);
    assert!(judged.header("nika-compile-replay").is_none(), "no token");
    let document = judged.json();
    assert_eq!(document["status"], "incomplete", "{document:#}");
    let held = (document["diagnostics"].as_array().into_iter().flatten())
        .any(|d| d["target"] == "verify_held");
    assert!(held, "{document:#}");
    let asked: Vec<String> = (roles(&document).into_iter())
        .map(|(role, _)| role)
        .collect();
    assert!(
        asked.iter().all(|role| role.starts_with("judge_")),
        "{asked:?}"
    );
    let calls = seat.calls();
    let again = server
        .request(&compile_request(&judged_replay(&token, &answered())))
        .await;
    assert_eq!(again.status, 409, "{}", again.body);
    assert_eq!(again.json()["error"]["code"], "compile_replay_unavailable");
    assert_eq!(seat.calls(), calls, "the judge is not asked again");
    server.stop().await.expect("clean stop");
}
