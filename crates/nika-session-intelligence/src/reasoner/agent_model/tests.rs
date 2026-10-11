// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_error::cost::Cost;
use nika_kernel::provider::ContentBlock;
use nika_providers::InferenceAdmission;
use nika_providers::model_choice::{ModelInventory, ModelRole};
use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
use nika_types::access::AccessClass;

use super::*;
use crate::reasoner::test_transport;
use crate::reasoner::wire::{Peer, response};

/// A metered vendor route whose key is present: no call, no network, only the probe's facts.
fn probes() -> Vec<ProviderProbe> {
    let readiness = ProviderReadiness::new(
        true,
        true,
        None,
        None,
        true,
        ExecutionLocus::Cloud,
        AccessClass::Api,
    );
    let probe = ProviderProbe::new(
        "deepseek",
        true,
        true,
        "DEEPSEEK_API_KEY",
        true,
        readiness,
        "https://api.deepseek.com",
    );
    vec![probe]
}

/// A model this machine offers carries its route, its billing and its list price as the
/// inventory states them; a model it does not offer carries nothing.
#[test]
fn a_model_choice_carries_the_inventorys_facts_and_an_unknown_one_none() {
    let probes = probes();
    let inventory = ModelInventory::from_probes(&probes);
    let offer = (inventory.offers(ModelRole::Run).iter())
        .find(|offer| offer.model.starts_with("deepseek/"))
        .expect("the probe's route offers the catalogue's models");
    let facts = model_facts(&probes, &offer.model).expect("an offered model has facts");
    assert_eq!(facts.role, "run");
    assert_eq!(facts.model, offer.model);
    assert_eq!(facts.via, offer.route.access);
    assert_eq!(facts.class, offer.route.class.as_str());
    assert_eq!(facts.billing, offer.route.billing.as_str());
    assert_eq!(facts.configured, offer.route.configured);
    assert_eq!(
        facts.output_usd_per_million.map(|price| price.0),
        offer.output_usd_per_million
    );
    assert_eq!(model_facts(&probes, "deepseek/not-a-real-model"), None);
}

/// A tool the conversation's run calls reaches a host as typed activity: started, then finished
/// with the time it took, its phase the tool's family, never its arguments.
#[test]
fn a_tool_step_reaches_the_host_as_typed_activity() {
    use std::sync::Mutex;

    use nika_onboard::activity::{Activity, Phase, ToolState};
    use nika_session_agent::Observed;
    use nika_session_change::tools::{SessionTools, ToolCall, ToolDef, ToolReply};

    struct Verify;
    impl SessionTools for Verify {
        fn tools(&self) -> Vec<ToolDef> {
            Vec::new()
        }
        fn call(&self, _: ToolCall) -> ToolReply {
            ToolReply::ok("ready")
        }
    }
    let seen = Arc::new(Mutex::new(Vec::<Activity>::new()));
    let kept = Arc::clone(&seen);
    let observed = Observed::new(Arc::new(Verify));
    observed.watch(Some(tool_steps(Arc::new(move |activity: &Activity| {
        kept.lock().expect("the sink's lock").push(activity.clone());
    }))));
    let args = serde_json::json!({"source": "nika: secret-workflow"});
    observed.call(ToolCall::new("verify", args).with_meta("toolu_v"));
    let seen = seen.lock().expect("the sink's lock").clone();
    let marks: Vec<_> = (seen.iter())
        .map(|a| {
            let mark = a.tool.clone().expect("a tool step");
            (mark.call, mark.name, mark.state, a.phase, a.done)
        })
        .collect();
    assert_eq!(
        marks,
        [
            (
                "toolu_v".to_owned(),
                "verify".to_owned(),
                ToolState::Started,
                Phase::Checking,
                false
            ),
            (
                "toolu_v".to_owned(),
                "verify".to_owned(),
                ToolState::Finished,
                Phase::Checking,
                true
            ),
        ]
    );
    assert!(seen.iter().all(|a| !a.note.contains("secret-workflow")));
}

/// One person's line as the conversation's request.
fn line() -> Request {
    Request::new(None, vec![Message::text(Role::User, "go")], Vec::new())
}

/// The turn's calls share one transport: the first call builds it, the next ones reuse it.
#[test]
fn the_calls_of_a_turn_share_one_transport() {
    let peer = Peer::start(vec![(200, response("Un.")), (200, response("Deux."))]);
    let _http = test_transport::install(&peer.url);
    let mut model = AgentModel::new("deepseek/deepseek-v4-pro");
    model
        .complete(&line(), &mut |_| {})
        .expect("the first call");
    let first = model.transport.clone().expect("built at the first call");
    model
        .complete(&line(), &mut |_| {})
        .expect("the second call");
    let kept = model.transport.as_ref().expect("kept for the turn");
    assert!(Arc::ptr_eq(&first, kept));
    assert_eq!(peer.bodies().len(), 2);
}

/// A provider's own "busy" answer, as the wire reads a 429 or a 503.
fn busy() -> serde_json::Value {
    serde_json::json!({"error": {"message": "the route is busy", "type": "rate_limit_error"}})
}

/// A transient rejection is asked again after the policy's wait; each ask is a new physical
/// call, and the answer then ends the call.
#[test]
fn a_busy_provider_is_asked_again_and_each_ask_is_counted() {
    let peer = Peer::start(vec![(429, busy()), (200, response("Voici."))]);
    let _http = test_transport::install(&peer.url);
    let account = InferenceAdmission::new(Cost::new(20_000_000_000)).expect("an account");
    let mut model = AgentModel::new("deepseek/deepseek-v4-pro").with_admission(account.clone());
    let reply = model
        .complete(&line(), &mut |_| {})
        .expect("answered after one wait");
    let said = (reply.content.iter()).find_map(|block| match block {
        ContentBlock::Text { text } => Some(text.as_str()),
        _ => None,
    });
    assert_eq!(said, Some("Voici."));
    assert_eq!(peer.bodies().len(), 2);
}

/// Busy at every ask, the call ends after asking again twice, and says so.
#[test]
fn a_provider_busy_at_every_ask_fails_the_call_said() {
    let peer = Peer::start(vec![(503, busy())]);
    let _http = test_transport::install(&peer.url);
    let mut model = AgentModel::new("deepseek/deepseek-v4-pro");
    let error = (model.complete(&line(), &mut |_| {})).expect_err("a busy provider fails");
    let said = matches!(&error, ModelError::Failed(why) if why.contains("asked again 2×"));
    assert!(said, "{error:?}");
    assert_eq!(peer.bodies().len(), 3);
}

/// A money account keeps a dispatched call the provider rejected as an unknown charge: the
/// call is not asked again, and the person reads why.
#[test]
fn a_busy_provider_is_not_asked_again_past_its_money_account() {
    let peer = Peer::start(vec![(429, busy()), (200, response("Voici."))]);
    let _http = test_transport::install(&peer.url);
    let account = InferenceAdmission::new(Cost::new(20_000_000_000)).expect("an account");
    let mut model = AgentModel::new("deepseek/deepseek-v4-pro").with_admission(account.clone());
    let error = (model.complete(&line(), &mut |_| {})).expect_err("not asked again");
    let said = matches!(&error, ModelError::Failed(why) if why.contains("did not ask again"));
    assert!(said, "{error:?}");
    assert_eq!(peer.bodies().len(), 1);
    assert_eq!(account.snapshot().expect("a receipt").attempts.len(), 1);
}
