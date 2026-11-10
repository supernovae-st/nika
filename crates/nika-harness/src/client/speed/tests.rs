// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Scripted ACP peers for the service tier: the exact payloads and their
//! order (model, effort, tier, then the mode), the offer re-read after the
//! model selection, the complete read-back, and every refusal proven with
//! ZERO prompts reaching the peer.

use std::pin::Pin;

use futures_core::Stream as _;
use nika_kernel::ai::harness::{
    HarnessConfinement, HarnessError, HarnessEvent, HarnessEventStream, HarnessOutcome,
    HarnessRequest, ModelProvenance,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

type PeerReader = BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>;
type PeerWriter = tokio::io::WriteHalf<tokio::io::DuplexStream>;

/// The name codex-acp reports on `initialize`: the key of its measured rows.
const CODEX_ACP: &str = "@agentclientprotocol/codex-acp";

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

/// codex-acp 1.13.1's speed option as its sessions advertise it without a
/// boolean capability: a select under `model_config`, `off` listed before `on`.
fn fast_mode(current: &str) -> Value {
    select("fast-mode", "model_config", current, &["off", "on"])
}

/// The codex-acp shape: `mode`, `model`, `reasoning_effort` and, only for a
/// model that supports a fast tier, `fast-mode`.
fn codex(mode: &str, model: &str, effort: &str, fast: Option<&str>) -> Value {
    let mut options = vec![
        select("mode", "mode", mode, &["read-only", "agent"]),
        select("model", "model", model, &["gpt-5.4", "gpt-5.5"]),
        select(
            "reasoning_effort",
            "thought_level",
            effort,
            &["low", "medium"],
        ),
    ];
    options.extend(fast.map(fast_mode));
    Value::Array(options)
}

/// Answer `initialize` with `agent_info` (the adapter's self-report, when it
/// makes one) and `session/new` with `config`; return the next request.
async fn open_as(
    r: &mut PeerReader,
    w: &mut PeerWriter,
    agent_info: Option<Value>,
    config: Value,
) -> Option<Value> {
    let init = read(r).await.expect("initialize");
    assert_eq!(init["method"], "initialize");
    let mut answer = json!({"protocolVersion":1});
    if let Some(agent_info) = agent_info {
        answer["agentInfo"] = agent_info;
    }
    reply(w, &init, answer).await;
    let new = read(r).await.expect("session/new");
    assert_eq!(new["method"], "session/new");
    reply(
        w,
        &new,
        json!({"sessionId":"s-tier","configOptions":config}),
    )
    .await;
    read(r).await
}

/// The measured adapter at its measured release.
async fn open(r: &mut PeerReader, w: &mut PeerWriter, config: Value) -> Option<Value> {
    let measured = json!({"name":CODEX_ACP,"version":"1.13.1"});
    open_as(r, w, Some(measured), config).await
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
        json!({"sessionId":"s-tier","configId":config_id,"value":value}),
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

/// Drive `request` against a scripted peer. The peer task is awaited, so an
/// assertion it makes (a request it must not receive) fails the test.
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

fn ask(model: Option<&str>, effort: Option<&str>, tier: Option<&str>) -> HarnessRequest {
    let mut request = HarnessRequest::new("hello", "/tmp")
        .with_requested_effort(effort.map(str::to_owned))
        .with_requested_service_tier(tier.map(str::to_owned));
    if let Some(model) = model {
        request = request.with_requested_model(model);
    }
    request
}

fn selection_refusal(err: HarnessError) -> String {
    let HarnessError::Selection { reason } = err else {
        panic!("a selection refusal, got {err:?}")
    };
    assert!(reason.contains("no prompt was sent"), "{reason}");
    reason
}

/// The default model offers no speed option and the selected one does: the
/// tier is judged on the answer to the model selection, sent through
/// `fast-mode` with the adapter's native value after the effort, and the
/// model, the effort and the tier are read back together before the prompt.
#[tokio::test]
async fn the_tier_is_applied_after_the_effort_and_read_back_before_the_prompt() {
    let outcome = dialogue(
        ask(Some("openai/gpt-5.5"), Some("low"), Some("fast")),
        |mut r, mut w| async move {
            let next = open(&mut r, &mut w, codex("agent", "gpt-5.4", "medium", None)).await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("model", "gpt-5.5"),
                codex("agent", "gpt-5.5", "medium", Some("off")),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("reasoning_effort", "low"),
                codex("agent", "gpt-5.5", "low", Some("off")),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("fast-mode", "on"),
                codex("agent", "gpt-5.5", "low", Some("on")),
            )
            .await;
            finish(&mut w, next).await;
        },
    )
    .await
    .expect("configured exactly");
    let s = &outcome.selection;
    assert_eq!(s.transmitted_model.as_deref(), Some("gpt-5.5"));
    assert_eq!(s.transmitted_effort.as_deref(), Some("low"));
    assert_eq!(s.configured_effort.as_deref(), Some("low"));
    assert_eq!(s.service_tier_option.as_deref(), Some("fast-mode"));
    assert_eq!(
        s.offered_service_tier,
        Some(vec!["off".to_owned(), "on".to_owned()]),
        "the offer as the session listed it, in its own order"
    );
    assert_eq!(s.transmitted_service_tier.as_deref(), Some("on"));
    assert_eq!(s.configured_service_tier.as_deref(), Some("on"));
    assert_eq!(
        s.configured_service_tier_source,
        Some(ModelProvenance::ConfirmedSelection)
    );
    assert!(s.changed_mid_turn.is_empty());
    assert!(s.moved_refusal().is_none());
}

/// An explicit `standard` is a selection too: the adapter's `off` is sent
/// even when the session already reports it, so the receipt holds a sent and
/// a confirmed value, never an assumed one.
#[tokio::test]
async fn standard_sends_the_native_off_and_reads_it_back() {
    let outcome = dialogue(
        ask(None, None, Some("standard")),
        |mut r, mut w| async move {
            let next = open(
                &mut r,
                &mut w,
                codex("agent", "gpt-5.5", "medium", Some("off")),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("fast-mode", "off"),
                codex("agent", "gpt-5.5", "medium", Some("off")),
            )
            .await;
            finish(&mut w, next).await;
        },
    )
    .await
    .expect("configured exactly");
    let s = &outcome.selection;
    assert_eq!(s.transmitted_service_tier.as_deref(), Some("off"));
    assert_eq!(s.configured_service_tier.as_deref(), Some("off"));
    assert_eq!(
        s.configured_service_tier_source,
        Some(ModelProvenance::ConfirmedSelection)
    );
    // No effort was asked: the session's own value is recorded as stated, as before.
    assert_eq!(s.transmitted_effort, None);
    assert_eq!(s.configured_effort.as_deref(), Some("medium"));
}

/// No tier asked: nothing is sent and nothing is recorded, even on a session
/// that offers the option and reports it on.
#[tokio::test]
async fn without_a_request_no_tier_is_sent_and_none_is_recorded() {
    let outcome = dialogue(ask(None, None, None), |mut r, mut w| async move {
        let next = open(
            &mut r,
            &mut w,
            codex("agent", "gpt-5.5", "medium", Some("on")),
        )
        .await;
        finish(&mut w, next).await;
    })
    .await
    .expect("default session");
    let s = &outcome.selection;
    assert_eq!(s.service_tier_option, None);
    assert_eq!(s.offered_service_tier, None);
    assert_eq!(s.transmitted_service_tier, None);
    assert_eq!(s.configured_service_tier, None);
    assert_eq!(s.configured_service_tier_source, None);
}

/// The DEFAULT model offers the option and the selected one does not: a
/// stale read of `session/new` would have let the tier through. Refused
/// after the model answer, with no tier request and no prompt.
#[tokio::test]
async fn a_tier_only_the_previous_model_offered_refuses_before_the_prompt() {
    let err = dialogue(
        ask(Some("openai/gpt-5.4"), None, Some("fast")),
        |mut r, mut w| async move {
            let next = open(
                &mut r,
                &mut w,
                codex("agent", "gpt-5.5", "medium", Some("off")),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("model", "gpt-5.4"),
                codex("agent", "gpt-5.4", "medium", None),
            )
            .await;
            assert!(next.is_none(), "no tier request and no prompt: {next:?}");
        },
    )
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(
        reason.contains("no `fast-mode` option")
            && reason.contains("gpt-5.4")
            && reason.contains("run.access.service_tier: fast")
            && reason.contains("reasoning_effort (thought_level)"),
        "{reason}"
    );
}

/// The session acknowledges the setter but its read-back still says `off`:
/// an accepted request is not a configuration.
#[tokio::test]
async fn a_read_back_that_does_not_hold_the_tier_refuses() {
    let err = dialogue(ask(None, None, Some("fast")), |mut r, mut w| async move {
        let next = open(
            &mut r,
            &mut w,
            codex("agent", "gpt-5.5", "medium", Some("off")),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("fast-mode", "on"),
            codex("agent", "gpt-5.5", "medium", Some("off")),
        )
        .await;
        assert!(
            next.is_none(),
            "no prompt after an unconfirmed tier: {next:?}"
        );
    })
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(
        reason.contains("`fast-mode` = `on`") && reason.contains("it reports `off`"),
        "{reason}"
    );
}

/// The answer to the tier setter no longer lists the option at all (the
/// adapter dropped it): the tier is not held.
#[tokio::test]
async fn an_answer_that_drops_the_option_refuses() {
    let err = dialogue(ask(None, None, Some("fast")), |mut r, mut w| async move {
        let next = open(
            &mut r,
            &mut w,
            codex("agent", "gpt-5.5", "medium", Some("off")),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("fast-mode", "on"),
            codex("agent", "gpt-5.5", "medium", None),
        )
        .await;
        assert!(next.is_none(), "no prompt: {next:?}");
    })
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(reason.contains("it reports none"), "{reason}");
}

/// The tier setter's answer moved the effort that was applied before it: the
/// whole selection must hold on one configuration.
#[tokio::test]
async fn an_effort_moved_by_the_tier_selection_refuses() {
    let err = dialogue(
        ask(None, Some("low"), Some("fast")),
        |mut r, mut w| async move {
            let next = open(
                &mut r,
                &mut w,
                codex("agent", "gpt-5.5", "medium", Some("off")),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("reasoning_effort", "low"),
                codex("agent", "gpt-5.5", "low", Some("off")),
            )
            .await;
            let next = expect_set(
                &mut r,
                &mut w,
                next,
                ("fast-mode", "on"),
                codex("agent", "gpt-5.5", "medium", Some("on")),
            )
            .await;
            assert!(next.is_none(), "no prompt: {next:?}");
        },
    )
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(
        reason.contains("reasoning effort `low`") && reason.contains("`medium`"),
        "{reason}"
    );
}

/// A later selection resets the tier: the mode setter's answer, the last
/// complete configuration before the prompt, reports `off` again. Refused,
/// with the mode request sent and no prompt.
#[tokio::test]
async fn a_tier_reset_by_the_mode_selection_refuses() {
    let request = ask(None, None, Some("fast")).with_confinement(HarnessConfinement::ReadOnly);
    let err = dialogue(request, |mut r, mut w| async move {
        let next = open(
            &mut r,
            &mut w,
            codex("agent", "gpt-5.5", "medium", Some("off")),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("fast-mode", "on"),
            codex("agent", "gpt-5.5", "medium", Some("on")),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("mode", "read-only"),
            codex("read-only", "gpt-5.5", "medium", Some("off")),
        )
        .await;
        assert!(next.is_none(), "no prompt after a reset tier: {next:?}");
    })
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(
        reason.contains("`fast-mode` = `on`") && reason.contains("it reports `off`"),
        "{reason}"
    );
}

/// The mode setter's answer still holds the tier: the confined session
/// prompts, in the order model, effort, tier, mode.
#[tokio::test]
async fn the_mode_selection_re_asserts_the_tier_before_the_prompt() {
    let request = ask(Some("openai/gpt-5.5"), Some("low"), Some("fast"))
        .with_confinement(HarnessConfinement::ReadOnly);
    let outcome = dialogue(request, |mut r, mut w| async move {
        let next = open(&mut r, &mut w, codex("agent", "gpt-5.4", "medium", None)).await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("model", "gpt-5.5"),
            codex("agent", "gpt-5.5", "medium", Some("off")),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("reasoning_effort", "low"),
            codex("agent", "gpt-5.5", "low", Some("off")),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("fast-mode", "on"),
            codex("agent", "gpt-5.5", "low", Some("on")),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("mode", "read-only"),
            codex("read-only", "gpt-5.5", "low", Some("on")),
        )
        .await;
        finish(&mut w, next).await;
    })
    .await
    .expect("confined and configured");
    assert_eq!(
        outcome.selection.configured_service_tier.as_deref(),
        Some("on")
    );
    assert_eq!(outcome.selection.configured_effort.as_deref(), Some("low"));
}

/// The option arrives as a boolean, a form this client never negotiated:
/// refused before any setter, never coerced.
#[tokio::test]
async fn a_boolean_option_this_client_did_not_negotiate_refuses() {
    let err = dialogue(ask(None, None, Some("fast")), |mut r, mut w| async move {
        let mut config = codex("agent", "gpt-5.5", "medium", None);
        config.as_array_mut().expect("options").push(json!({
            "id":"fast-mode","name":"Fast mode","category":"model_config","type":"boolean",
            "currentValue":false}));
        let next = open(&mut r, &mut w, config).await;
        assert!(next.is_none(), "no setter and no prompt: {next:?}");
    })
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(
        reason.contains("a `boolean`") && reason.contains("did not negotiate"),
        "{reason}"
    );
}

/// The measured option is `fast-mode` under `model_config`. The same id under
/// another category, or twice, is not that control.
#[tokio::test]
async fn the_option_must_be_the_measured_one() {
    for (extra, needle) in [
        (
            vec![select("fast-mode", "experimental", "off", &["off", "on"])],
            "under `experimental`",
        ),
        (vec![fast_mode("off"), fast_mode("off")], "2 options"),
    ] {
        let err = dialogue(
            ask(None, None, Some("fast")),
            move |mut r, mut w| async move {
                let mut config = codex("agent", "gpt-5.5", "medium", None);
                config.as_array_mut().expect("options").extend(extra);
                let next = open(&mut r, &mut w, config).await;
                assert!(next.is_none(), "no setter and no prompt: {next:?}");
            },
        )
        .await
        .expect_err("refused");
        let reason = selection_refusal(err);
        assert!(reason.contains(needle), "{needle}: {reason}");
    }
}

/// The option lists no `on`: `fast` is refused with the values it does list,
/// never served as `standard`.
#[tokio::test]
async fn a_native_value_the_option_does_not_list_refuses() {
    let err = dialogue(ask(None, None, Some("fast")), |mut r, mut w| async move {
        let mut config = codex("agent", "gpt-5.5", "medium", None);
        config.as_array_mut().expect("options").push(select(
            "fast-mode",
            "model_config",
            "off",
            &["off"],
        ));
        let next = open(&mut r, &mut w, config).await;
        assert!(next.is_none(), "no setter and no prompt: {next:?}");
    })
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(
        reason.contains("no `on` on `fast-mode`") && reason.contains("it offers: off"),
        "{reason}"
    );
}

/// An adapter with no measured speed control cannot honor a tier, whatever
/// its session advertises: the binding is keyed by its own name.
#[tokio::test]
async fn an_adapter_without_a_measured_binding_refuses() {
    for agent in [Some("@agentclientprotocol/claude-agent-acp"), None] {
        let err = dialogue(
            ask(None, None, Some("fast")),
            move |mut r, mut w| async move {
                let next = open_as(
                    &mut r,
                    &mut w,
                    agent.map(|name| json!({"name":name,"version":"0.81.1"})),
                    codex("agent", "gpt-5.5", "medium", Some("off")),
                )
                .await;
                assert!(next.is_none(), "no setter and no prompt: {next:?}");
            },
        )
        .await
        .expect_err("refused");
        let reason = selection_refusal(err);
        assert!(
            reason.contains("no speed control is measured")
                && reason.contains(agent.unwrap_or("``")),
            "{reason}"
        );
    }
}

/// The measurement holds for the adapter's exact release: another release, or
/// an `initialize` that reports none, cannot honor a tier even when its session
/// lists the same option. What `on` becomes on a turn belongs to the release
/// and the agent it bundles, so nothing is sent.
#[tokio::test]
async fn another_release_of_the_measured_adapter_refuses() {
    for (version, needle) in [
        (Some("2.6.0"), "release `2.6.0`"),
        (Some("1.13.10"), "release `1.13.10`"),
        (None, "no release"),
    ] {
        let err = dialogue(
            ask(None, None, Some("fast")),
            move |mut r, mut w| async move {
                let mut reported = json!({"name":CODEX_ACP});
                if let Some(version) = version {
                    reported["version"] = json!(version);
                }
                let next = open_as(
                    &mut r,
                    &mut w,
                    Some(reported),
                    codex("agent", "gpt-5.5", "medium", Some("off")),
                )
                .await;
                assert!(next.is_none(), "no setter and no prompt: {next:?}");
            },
        )
        .await
        .expect_err("refused");
        let reason = selection_refusal(err);
        assert!(
            reason.contains("is measured on codex-acp 1.13.1")
                && reason.contains(needle)
                && reason.contains("run.access.service_tier: fast"),
            "{reason}"
        );
    }
}

/// A legacy model list answers no configuration: the option listed before
/// the selection says nothing about the model now selected.
#[tokio::test]
async fn a_legacy_model_selection_cannot_carry_a_tier() {
    let err = dialogue(
        ask(Some("codex/opus"), None, Some("fast")),
        |mut r, mut w| async move {
            let init = read(&mut r).await.expect("initialize");
            reply(
                &mut w,
                &init,
                json!({"protocolVersion":1,"agentInfo":{"name":CODEX_ACP,"version":"1.13.1"}}),
            )
            .await;
            let new = read(&mut r).await.expect("session/new");
            reply(&mut w, &new, json!({"sessionId":"s-tier","models":{"currentModelId":"sonnet",
                "availableModels":[{"modelId":"sonnet","name":"Sonnet"},{"modelId":"opus","name":"Opus"}]}}))
            .await;
            let set = read(&mut r).await.expect("the legacy model request");
            assert_eq!(set["method"], "session/set_model");
            reply(&mut w, &set, json!({})).await;
            assert!(read(&mut r).await.is_none(), "no tier request, no prompt");
        },
    )
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(reason.contains("legacy model list"), "{reason}");
}

/// A word that is not a tier never reaches the session.
#[tokio::test]
async fn a_word_that_is_not_a_tier_refuses() {
    let err = dialogue(
        ask(None, None, Some("priority")),
        |mut r, mut w| async move {
            let next = open(
                &mut r,
                &mut w,
                codex("agent", "gpt-5.5", "medium", Some("off")),
            )
            .await;
            assert!(next.is_none(), "no setter and no prompt: {next:?}");
        },
    )
    .await
    .expect_err("refused");
    let reason = selection_refusal(err);
    assert!(
        reason.contains("run.access.service_tier: priority"),
        "{reason}"
    );
}

/// The agent moving its own tier mid-turn, or dropping the option, is
/// recorded on the outcome, and an exact selection then owes a refusal.
#[tokio::test]
async fn a_mid_turn_tier_change_is_recorded_and_refuses_the_answer() {
    for (after, change) in [
        (Some("off"), "service_tier=off"),
        (None, "service_tier=unoffered"),
    ] {
        let outcome = dialogue(
            ask(None, None, Some("fast")),
            move |mut r, mut w| async move {
                let next = open(
                    &mut r,
                    &mut w,
                    codex("agent", "gpt-5.5", "medium", Some("off")),
                )
                .await;
                let next = expect_set(
                    &mut r,
                    &mut w,
                    next,
                    ("fast-mode", "on"),
                    codex("agent", "gpt-5.5", "medium", Some("on")),
                )
                .await;
                let prompt = next.expect("prompt");
                let update = json!({"jsonrpc":"2.0","method":"session/update","params":{
                "sessionId":"s-tier","update":{"sessionUpdate":"config_option_update",
                    "configOptions":codex("agent", "gpt-5.5", "medium", after)}}});
                let mut line = serde_json::to_vec(&update).expect("json");
                line.push(b'\n');
                w.write_all(&line).await.expect("write");
                reply(&mut w, &prompt, json!({"stopReason":"end_turn"})).await;
            },
        )
        .await
        .expect("completed");
        assert_eq!(
            outcome.selection.changed_mid_turn,
            vec![change.to_owned()],
            "{change}"
        );
        let Some(HarnessError::Selection { reason }) = outcome.selection.moved_refusal() else {
            panic!("a moved tier owes a refusal")
        };
        assert!(reason.contains(change), "{reason}");
    }
}

/// An update that restates the configured tier moves nothing.
#[tokio::test]
async fn a_mid_turn_update_that_keeps_the_tier_records_nothing() {
    let outcome = dialogue(ask(None, None, Some("fast")), |mut r, mut w| async move {
        let next = open(
            &mut r,
            &mut w,
            codex("agent", "gpt-5.5", "medium", Some("off")),
        )
        .await;
        let next = expect_set(
            &mut r,
            &mut w,
            next,
            ("fast-mode", "on"),
            codex("agent", "gpt-5.5", "medium", Some("on")),
        )
        .await;
        let prompt = next.expect("prompt");
        let update = json!({"jsonrpc":"2.0","method":"session/update","params":{
            "sessionId":"s-tier","update":{"sessionUpdate":"config_option_update",
                "configOptions":codex("agent", "gpt-5.5", "medium", Some("on"))}}});
        let mut line = serde_json::to_vec(&update).expect("json");
        line.push(b'\n');
        w.write_all(&line).await.expect("write");
        reply(&mut w, &prompt, json!({"stopReason":"end_turn"})).await;
    })
    .await
    .expect("completed");
    assert!(outcome.selection.changed_mid_turn.is_empty());
    assert!(outcome.selection.moved_refusal().is_none());
}

/// Codex applies a turn's service tier only while its `fast_mode` feature is
/// on. If the tool-free profile ever listed it, the session would still offer
/// the speed option and confirm the selection while Codex dropped the tier: a
/// receipt that reads `configured` for a request with no effect.
#[test]
fn the_tool_free_profile_leaves_the_speed_feature_on() {
    assert!(
        !crate::infer::tool_free::DISABLED_FEATURES.contains(&"fast_mode"),
        "disabling `fast_mode` makes a confirmed service tier a silent no-op"
    );
}
