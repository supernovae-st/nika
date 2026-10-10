// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_providers::model_choice::{ModelInventory, ModelRole};
use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
use nika_types::access::AccessClass;

use super::*;

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
