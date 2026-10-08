// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Scripted ACP peers for the native reasoning effort: exact payloads and
//! order, the options re-read after a model change, the complete read-back,
//! and every refusal proven with ZERO prompts reaching the peer.

use std::pin::Pin;

use futures_core::Stream as _;
use nika_kernel::ai::harness::{
    HarnessError, HarnessEvent, HarnessEventStream, HarnessOutcome, HarnessRequest, ModelProvenance,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

type PeerReader = BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>;
type PeerWriter = tokio::io::WriteHalf<tokio::io::DuplexStream>;

/// One line from the client, `None` at its EOF (the client gave up).
async fn read(peer: &mut PeerReader) -> Option<Value> {
    let mut line = String::new();
    let size = tokio::time::timeout(std::time::Duration::from_secs(5), peer.read_line(&mut line))
        .await
        .expect("scripted dialogue must settle")
        .expect("read peer");
    (size != 0).then(|| serde_json::from_str(&line).expect("JSON request"))
}

async fn reply(peer: &mut PeerWriter, request: &Value, result: Value) {
    let mut bytes =
        serde_json::to_vec(&json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
            .expect("JSON");
    bytes.push(b'\n');
    peer.write_all(&bytes).await.expect("write peer");
    peer.flush().await.expect("flush peer");
}

fn select(id: &str, category: &str, current: &str, values: &[&str]) -> Value {
    json!({"id":id,"name":id,"category":category,"type":"select","currentValue":current,
        "options":values.iter().map(|v| json!({"value":v,"name":v})).collect::<Vec<_>>()})
}

/// The codex-acp shape: `model` plus `reasoning_effort` (category
/// `thought_level`) whose values belong to the CURRENT model.
fn codex(model: &str, effort: &str, efforts: &[&str]) -> Value {
    json!([
        select("model", "model", model, &["gpt-5.4", "gpt-5.5"]),
        select("reasoning_effort", "thought_level", effort, efforts)
    ])
}

/// Answer initialize and session/new with `config`; return the next request.
async fn open(r: &mut PeerReader, w: &mut PeerWriter, config: Value) -> Option<Value> {
    let init = read(r).await.expect("initialize");
    assert_eq!(init["method"], "initialize");
    reply(w, &init, json!({"protocolVersion":1})).await;
    let new = read(r).await.expect("session/new");
    assert_eq!(new["method"], "session/new");
    reply(
        w,
        &new,
        json!({"sessionId":"s-effort","configOptions":config}),
    )
    .await;
    read(r).await
}

/// Assert one `session/set_config_option` payload exactly, answer `config`.
async fn expect_set(
    r: &mut PeerReader,
    w: &mut PeerWriter,
    request: Option<Value>,
    (config_id, value): (&str, &str),
    config: Value,
) -> Option<Value> {
    let request = request.expect("a selection request");
    assert_eq!(request["method"], "session/set_config_option");
    assert_eq!(
        request["params"],
        json!({"sessionId":"s-effort","configId":config_id,"value":value}),
        "exact payload"
    );
    reply(w, &request, json!({"configOptions":config})).await;
    read(r).await
}

async fn finish(w: &mut PeerWriter, prompt: Option<Value>) {
    let prompt = prompt.expect("the prompt follows a confirmed configuration");
    assert_eq!(prompt["method"], "session/prompt");
    reply(w, &prompt, json!({"stopReason":"end_turn"})).await;
}

async fn result(mut stream: HarnessEventStream) -> Result<HarnessOutcome, HarnessError> {
    loop {
        match std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
            Some(Ok(HarnessEvent::Completed { outcome })) => return Ok(*outcome),
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(error),
            None => panic!("scripted turn must complete or refuse"),
        }
    }
}

/// Drive `request` against a scripted peer; the peer task returns the
/// method names it received (the zero-prompt witness).
async fn dialogue<F, Fut>(request: HarnessRequest, peer: F) -> Result<HarnessOutcome, HarnessError>
where
    F: FnOnce(PeerReader, PeerWriter) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (peer_read, peer_write) = tokio::io::split(theirs);
    let agent = tokio::spawn(peer(BufReader::new(peer_read), peer_write));
    let outcome = result(crate::client::drive(client_read, client_write, request)).await;
    agent.await.expect("scripted peer completes");
    outcome
}

fn ask(model: Option<&str>, effort: Option<&str>) -> HarnessRequest {
    let mut request =
        HarnessRequest::new("hello", "/tmp").with_requested_effort(effort.map(str::to_owned));
    if let Some(model) = model {
        request = request.with_requested_model(model);
    }
    request
}

/// Spec runtime fixture 006's shape: the default model offers low/medium,
/// the selected one offers high. The effort is judged on the answer to the
/// model selection, sent through `reasoning_effort` with the exact value,
/// and both dimensions are read back before the prompt.
#[tokio::test]
async fn the_effort_is_judged_on_the_options_refreshed_by_the_model_selection() {
    let outcome = dialogue(
        ask(Some("openai/gpt-5.5"), Some("high")),
        |mut r, mut w| async move {
            let next = open(
                &mut r,
                &mut w,
                codex("gpt-5.4", "medium", &["low", "medium"]),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("model", "gpt-5.5"),
                codex("gpt-5.5", "medium", &["medium", "high"]),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("reasoning_effort", "high"),
                codex("gpt-5.5", "high", &["medium", "high"]),
            )
            .await;
            finish(&mut w, next).await;
        },
    )
    .await
    .expect("configured exactly");
    assert_eq!(
        (
            outcome.observed_model.as_deref(),
            outcome.observed_model_source
        ),
        (Some("gpt-5.5"), Some(ModelProvenance::ConfirmedSelection))
    );
    let s = &outcome.selection;
    assert_eq!(s.transmitted_model.as_deref(), Some("gpt-5.5"));
    assert_eq!(s.effort_option.as_deref(), Some("reasoning_effort"));
    assert_eq!(s.transmitted_effort.as_deref(), Some("high"));
    assert_eq!(s.configured_effort.as_deref(), Some("high"));
    assert_eq!(
        s.configured_effort_source,
        Some(ModelProvenance::ConfirmedSelection)
    );
    assert!(s.changed_mid_turn.is_empty());
}

/// Spec fixture 009's shape: the DEFAULT model offers `xhigh` but the
/// selected one does not — a stale read of `session/new` would have let it
/// through. Refused after the model answer, before any prompt.
#[tokio::test]
async fn an_effort_only_the_previous_model_offered_refuses_before_the_prompt() {
    let err = dialogue(
        ask(Some("openai/gpt-5.4"), Some("xhigh")),
        |mut r, mut w| async move {
            let mut start = codex("gpt-5.5", "medium", &["medium", "high", "xhigh"]);
            start[0]["currentValue"] = json!("gpt-5.5");
            let next = open(&mut r, &mut w, start).await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("model", "gpt-5.4"),
                codex("gpt-5.4", "low", &["low", "medium"]),
            )
            .await;
            assert!(next.is_none(), "no effort request and no prompt: {next:?}");
        },
    )
    .await
    .expect_err("refused");
    let HarnessError::Refused { reason } = err else {
        panic!("a refusal, got {err:?}")
    };
    assert!(
        reason.contains("`xhigh`")
            && reason.contains("gpt-5.4")
            && reason.contains("low · medium")
            && reason.contains("run.reasoning.effort")
            && reason.contains("no prompt was sent"),
        "{reason}"
    );
}

/// No alias, no case folding: `High` is not `high`.
#[tokio::test]
async fn an_effort_must_equal_an_advertised_value_exactly() {
    let err = dialogue(ask(None, Some("High")), |mut r, mut w| async move {
        let next = open(
            &mut r,
            &mut w,
            codex("gpt-5.5", "medium", &["medium", "high"]),
        )
        .await;
        assert!(next.is_none(), "nothing after the refusal: {next:?}");
    })
    .await
    .expect_err("refused");
    assert!(err.to_string().contains("`High`"), "{err}");
    assert!(err.to_string().contains("medium · high"), "{err}");
}

/// Spec fixture 011's shape: the session acknowledges but its read-back
/// does not hold the value — refused before the prompt.
#[tokio::test]
async fn a_read_back_that_does_not_hold_the_effort_refuses() {
    let err = dialogue(ask(None, Some("high")), |mut r, mut w| async move {
        let next = open(
            &mut r,
            &mut w,
            codex("gpt-5.5", "medium", &["medium", "high"]),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("reasoning_effort", "high"),
            codex("gpt-5.5", "medium", &["medium", "high"]),
        )
        .await;
        assert!(
            next.is_none(),
            "no prompt after an unconfirmed effort: {next:?}"
        );
    })
    .await
    .expect_err("refused");
    let reason = err.to_string();
    assert!(
        reason.contains("did not confirm reasoning effort `high`") && reason.contains("`medium`"),
        "{reason}"
    );
}

/// The model selected first must still hold when the effort is set.
#[tokio::test]
async fn a_model_moved_by_the_effort_selection_refuses() {
    let err = dialogue(
        ask(Some("openai/gpt-5.5"), Some("high")),
        |mut r, mut w| async move {
            let next = open(&mut r, &mut w, codex("gpt-5.4", "low", &["low", "high"])).await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("model", "gpt-5.5"),
                codex("gpt-5.5", "low", &["low", "high"]),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("reasoning_effort", "high"),
                codex("gpt-5.4", "high", &["low", "high"]),
            )
            .await;
            assert!(next.is_none(), "no prompt on a moved model: {next:?}");
        },
    )
    .await
    .expect_err("refused");
    assert!(
        err.to_string()
            .contains("moved the model from `gpt-5.5` to `gpt-5.4`"),
        "{err}"
    );
}

/// No reasoning option (or no category): refused, naming what IS advertised.
#[tokio::test]
async fn a_session_without_a_reasoning_option_refuses_and_names_its_options() {
    let err = dialogue(ask(None, Some("high")), |mut r, mut w| async move {
        let config = json!([select("model", "model", "k3", &["k3"]),
            {"id":"depth","name":"Depth","type":"select","currentValue":"a",
             "options":[{"value":"a","name":"a"}]}]);
        let next = open(&mut r, &mut w, config).await;
        assert!(next.is_none(), "nothing after the refusal: {next:?}");
    })
    .await
    .expect_err("refused");
    let reason = err.to_string();
    assert!(
        reason.contains("offers no reasoning-effort option")
            && reason.contains("model (model) · depth"),
        "{reason}"
    );
}

/// Two `thought_level` options are ambiguous for a request; without one
/// the turn proceeds and records no effort (nothing depends on it).
#[tokio::test]
async fn two_reasoning_options_are_ambiguous_only_when_an_effort_is_asked() {
    let twin = || {
        json!([
            select("model", "model", "m", &["m"]),
            select("effort_a", "thought_level", "low", &["low"]),
            select("effort_b", "thought_level", "low", &["low"])
        ])
    };
    let err = dialogue(ask(None, Some("low")), move |mut r, mut w| async move {
        let next = open(&mut r, &mut w, twin()).await;
        assert!(next.is_none());
    })
    .await
    .expect_err("ambiguous");
    assert!(err.to_string().contains("effort_a · effort_b"), "{err}");
    let outcome = dialogue(ask(None, None), move |mut r, mut w| async move {
        let next = open(&mut r, &mut w, twin()).await;
        finish(&mut w, next).await;
    })
    .await
    .expect("nothing asked, nothing refused");
    assert_eq!(outcome.selection.configured_effort, None);
}

/// claude-agent-acp's shape: `effort` with a `default` row and grouped
/// values; the value is sent through ITS id, found inside a group.
#[tokio::test]
async fn a_grouped_effort_option_is_matched_and_sent_through_its_own_id() {
    let grouped = |current: &str| {
        json!([select("model", "model", "sonnet", &["sonnet", "opus"]),
            {"id":"effort","name":"Effort","category":"thought_level","type":"select",
             "currentValue":current,"options":[
                {"group":"plain","name":"Plain","options":[{"value":"default","name":"Default"}]},
                {"group":"levels","name":"Levels","options":[
                    {"value":"low","name":"Low"},{"value":"max","name":"Max"}]}]}])
    };
    let outcome = dialogue(ask(None, Some("max")), move |mut r, mut w| async move {
        let next = open(&mut r, &mut w, grouped("default")).await;
        let next = expect_set(&mut r, &mut w, next, ("effort", "max"), grouped("max")).await;
        finish(&mut w, next).await;
    })
    .await
    .expect("grouped value applied");
    assert_eq!(outcome.selection.effort_option.as_deref(), Some("effort"));
    assert_eq!(outcome.selection.configured_effort.as_deref(), Some("max"));
}

/// Without a requested effort nothing is sent; the session's own current
/// value is recorded as stated (`session_config`), never as applied.
#[tokio::test]
async fn without_a_request_the_session_effort_is_recorded_not_applied() {
    let outcome = dialogue(ask(None, None), |mut r, mut w| async move {
        let next = open(
            &mut r,
            &mut w,
            codex("gpt-5.5", "medium", &["medium", "high"]),
        )
        .await;
        finish(&mut w, next).await;
    })
    .await
    .expect("default session");
    let s = &outcome.selection;
    assert_eq!(
        (s.effort_option.as_deref(), s.transmitted_effort.as_deref()),
        (None, None)
    );
    assert_eq!(s.configured_effort.as_deref(), Some("medium"));
    assert_eq!(
        s.configured_effort_source,
        Some(ModelProvenance::SessionConfig)
    );
}

/// A legacy model list answers no configuration: an effort cannot be
/// judged after it, so it refuses before even the model request.
#[tokio::test]
async fn a_legacy_model_selection_with_an_effort_refuses() {
    let err = dialogue(ask(Some("claude-code/opus"), Some("high")), |mut r, mut w| async move {
        let init = read(&mut r).await.expect("initialize");
        reply(&mut w, &init, json!({"protocolVersion":1})).await;
        let new = read(&mut r).await.expect("session/new");
        reply(&mut w, &new, json!({"sessionId":"s-effort","models":{"currentModelId":"sonnet",
            "availableModels":[{"modelId":"sonnet","name":"Sonnet"},{"modelId":"opus","name":"Opus"}]}}))
        .await;
        assert!(read(&mut r).await.is_none(), "no set_model, no prompt");
    })
    .await
    .expect_err("refused");
    assert!(err.to_string().contains("legacy model list"), "{err}");
}

/// The agent moving its own model mid-turn is recorded on the outcome.
#[tokio::test]
async fn a_mid_turn_model_change_is_recorded() {
    let outcome = dialogue(ask(None, Some("high")), |mut r, mut w| async move {
        let next = open(&mut r, &mut w, codex("gpt-5.5", "medium", &["medium", "high"])).await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("reasoning_effort", "high"),
            codex("gpt-5.5", "high", &["medium", "high"]),
        )
        .await;
        let prompt = next.expect("prompt");
        let update = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-effort",
            "update":{"sessionUpdate":"config_option_update",
                "configOptions":codex("gpt-5.4", "low", &["low", "medium"])}}});
        let mut line = serde_json::to_vec(&update).expect("json");
        line.push(b'\n');
        w.write_all(&line).await.expect("write");
        reply(&mut w, &prompt, json!({"stopReason":"end_turn"})).await;
    })
    .await
    .expect("completed");
    assert_eq!(
        outcome.selection.changed_mid_turn,
        vec!["model=gpt-5.4".to_owned(), "effort=low".to_owned()]
    );
}
