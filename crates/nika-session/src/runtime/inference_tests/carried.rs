// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A verdict that rejected candidate bytes is handed to every later compile of the same goal
//! (R6 across compiles), through whole Session turns on loopback: a compile of the same request
//! that authors those bytes again (the stronger seat's retry) never asks the same verifier on
//! them; it repeats the verdict with no call (`carried`), and the bytes stay held. A correction
//! is another request: the verdict binds to the request it judged, so the corrected request is
//! asked again. New work carries none. No provider qualification, no paid call.
use super::unjudged::{DOUBTED, HELD, asked, held_findings};
use super::*;
use crate::authoring::decision::tests::{KEY, Peer as SystemOne, Reply, SEAT};
use crate::authoring::{AuthoringContext, AuthoringSeat, DecisionSetup};
use nika_cli_host::compile::config::AuthoringSettings;
use nika_event::source_id::sha256_hex;

/// Why a doubt with no defect stays undecided when no run of the doubted bytes exists.
const NO_TRIAL: &str = "no trial run of these exact bytes exists in this compile";
/// The route step of a verdict carried from an earlier round of the goal.
const CARRIED: &str = "verify: same bytes, rejected in an earlier round";

/// [`WORK`] is new work; any other line changes it.
struct Correcting;
impl TurnClassifier for Correcting {
    fn classify(&mut self, _: &TurnContext, line: &str) -> TurnDecision {
        let act = if line == WORK {
            TurnAct::NewWork
        } else {
            TurnAct::Modify
        };
        TurnDecision::new(act, crate::turn::RoutingMethod::Model)
    }
}

/// A System One answer to the question `id`: `choice`.
fn verdict(id: &str, choice: &str) -> Reply {
    Reply::Json(
        200,
        json!({"model": "jev-test", "answers": {id: {"type": "choice", "choice": choice}},
            "usage": {"input_tokens": 40, "output_tokens": 2}}),
    )
}

/// The verifier's doubt of [`WORK`]'s candidate with no defect located: the whole request not
/// carried, its one part carried, no task doing what the request does not ask.
const DOUBT: [(&str, &str); 3] = [
    ("verify-request", "unfaithful"),
    ("verify-part-0", "carried"),
    ("verify-extra", "only_requested"),
];

/// The session under the provider's unnamed default (a reading it cannot settle is read once
/// more by the stronger model), its verifier the operator-selected decision seat at `base`.
fn judged_by_seat(root: &Path, home: &Path, base: &str) -> SessionRuntime {
    std::fs::write(root.join("entree.txt"), "A\n").expect("input");
    let selected = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        model: None,
        locus: DataLocus::Metered {
            provider: "deepseek".into(),
        },
        ready: true,
        why: None,
    };
    let reasoner = || ProviderReasoner {
        model: "deepseek/deepseek-flash".into(),
        label: "DeepSeek".into(),
    };
    let mut s = SessionRuntime::open(root, selected, Box::new(reasoner()));
    s.factory = Some(Box::new(move |_| Box::new(reasoner())));
    let none = AuthoringSettings::none();
    // The verifier's carry is under test, not the knowledge release: none is opened, so no
    // catalogue is lent and the seat is asked the verifier's questions only.
    let off = AuthoringSettings::none().with_knowledge_off();
    let seat = DecisionSetup::with_key(SEAT, Some(KEY.to_owned()), Some(base));
    s.set_authoring_context(AuthoringContext::from_settings(&off, &none).with_decision(Some(seat)));
    s.enable_continuous_preparation();
    s.with_classifier(Box::new(Correcting));
    s.enable_history(home).unwrap();
    s
}

/// The verification attempts of the outcome the session keeps.
fn attempts(s: &SessionRuntime) -> Vec<Value> {
    let out = s.last_outcome.as_ref().expect("the outcome");
    let decision = out.provenance.decision.as_ref().expect("decision");
    (decision["semantic_verification"].as_array().cloned()).unwrap_or_default()
}

/// The verification steps of the decision route of the outcome the session keeps, in order.
fn verify_steps(s: &SessionRuntime) -> Vec<String> {
    let out = s.last_outcome.as_ref().expect("the outcome");
    let decision = out.provenance.decision.as_ref().expect("decision");
    (decision["route"].as_array().into_iter().flatten())
        .filter_map(|step| step.as_str().map(str::to_owned))
        .filter(|step| step.starts_with("verify:"))
        .collect()
}

/// The doubted verdict the held candidate's verifier gave with no defect located, as the
/// carried attempt reads it back.
fn assert_doubt_read_back(carried: &Value, judge: &Value, sha: &str) {
    assert_eq!(carried["judge"], *judge);
    assert_eq!(carried["candidate_sha256"], sha);
    assert_eq!(
        (
            &carried["carried"],
            &carried["same_bytes_as"],
            &carried["questions"]
        ),
        (&json!(true), &Value::Null, &json!([]))
    );
    for count in ["attempted", "returned", "consumed"] {
        assert_eq!(carried[count], 0, "{count}");
    }
    assert_eq!(carried["doubt"], json!(["unfaithful"]));
    assert_eq!(carried["contested"], json!([WORK]));
    assert_eq!(carried["unsettled"], json!([NO_TRIAL]));
    assert_eq!(
        (
            &carried["declined"],
            &carried["rejected"],
            &carried["settled"]
        ),
        (&json!(true), &json!(true), &json!(false))
    );
}

/// The decision seat doubts the default model's candidate with no defect located: held. The
/// stronger model's retry authors the very same bytes: the verdict is carried into its compile,
/// the decision seat is asked nothing more, and the bytes stay held (`carried`), never proposed
/// nor written. The goal keeps the one verdict, the original attempt.
#[test]
fn the_stronger_retry_that_authors_the_held_bytes_again_never_asks_their_verifier() {
    let jev = SystemOne::start(
        DOUBT
            .iter()
            .map(|(id, choice)| verdict(id, choice))
            .collect(),
    );
    let mut script: Vec<_> = semantic_create()
        .iter()
        .map(|t| (200, response(t)))
        .collect();
    script.extend(semantic_create().iter().map(|t| (200, response(t))));
    let peer = Peer::start(script);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = judged_by_seat(dir.path(), home.path(), &jev.base);
    let stronger = AuthoringSeat::Provider {
        model: "deepseek/deepseek-v4-pro".to_owned(),
    };
    assert_eq!(s.stronger_seat(), Some(stronger));
    let out = s.turn(WORK);
    assert!(
        matches!(&out, TurnOutcome::Facts(words) if words == DOUBTED),
        "{out:?}"
    );
    // The verifier was asked the held bytes' three questions, once.
    let questions: Vec<Vec<String>> = (jev.requests().iter())
        .map(|(_, _, body)| body["questions"].as_object().into_iter().flatten())
        .map(|asked| asked.map(|(id, _)| id.clone()).collect())
        .collect();
    let ids = DOUBT.map(|(id, _)| vec![id.to_owned()]);
    assert_eq!(questions, ids);
    // The author: the default's document, then the stronger model's, never a closed choice.
    let bodies = peer.bodies();
    let models: Vec<&Value> = bodies.iter().map(|body| &body["model"]).collect();
    let (flash, pro) = (json!("deepseek-flash"), json!("deepseek-v4-pro"));
    assert_eq!(models, [&flash, &pro]);
    assert!(bodies.iter().all(|body| asked(body).is_none()));
    // The retry's one attempt repeats the verdict with no call, on the same bytes.
    let held = s.last_outcome.as_ref().expect("the held outcome");
    let candidate = held.candidate.as_deref().expect("shown as the preview");
    let sha = sha256_hex(candidate.as_bytes());
    let attempts = attempts(&s);
    assert_eq!(attempts.len(), 1, "{attempts:#?}");
    let judge = json!({"seat": SEAT, "kind": "decision_seat"});
    assert_doubt_read_back(&attempts[0], &judge, &sha);
    let kept = s.declined.carried(s.intent.goal.as_ref());
    assert_eq!(kept.len(), 1, "{kept:#?}");
    assert_eq!(kept[0]["candidate_sha256"], sha.as_str());
    assert_eq!(kept[0]["carried"], false, "the attempt that judged them");
    let asked_roles: Vec<&Value> = (kept[0]["questions"].as_array().into_iter().flatten())
        .map(|question| &question["role"])
        .collect();
    assert_eq!(asked_roles, ["judge_request", "judge_part", "judge_extra"]);
    let held = s.last_outcome.as_ref().expect("the held outcome");
    assert_eq!(held_findings(held), [HELD]);
    assert_eq!(
        verify_steps(&s),
        [CARRIED, "verify: not ready, candidate held"]
    );
    assert!(s.pending_proposal().is_none());
    assert!(!dir.path().join("sortie.txt").exists(), "never run");
    assert!(!dir.path().join("compiled-workflow.nika").exists());
}

/// The session under its named API model: no stronger retry, the model is its own verifier.
fn named(root: &Path, home: &Path) -> SessionRuntime {
    let mut s = open(root);
    s.enable_continuous_preparation();
    s.with_classifier(Box::new(Correcting));
    s.enable_history(home).unwrap();
    s
}

/// A correction of a held request restates it: the verdict that rejected the held bytes judged
/// the earlier request, so the correction's compile is handed it but never carries it (a carried
/// rejection binds to the request it judged). When the correction authors those very bytes
/// again, the verifier is asked on them against the corrected request, and its approval proposes
/// them. The kept verdict follows the restated goal.
#[test]
fn a_correction_of_a_held_request_asks_its_verifier_again_on_the_same_bytes() {
    let mut script: Vec<_> = semantic_create()
        .iter()
        .map(|t| (200, response(t)))
        .collect();
    let doubt = DOUBT.map(|(_, choice)| (200, response(&json!({ "choice": choice }).to_string())));
    script.extend(doubt);
    script.extend(semantic_create().iter().map(|t| (200, response(t))));
    script.push((200, response(JUDGE_APPROVES)));
    let peer = Peer::start(script);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = named(dir.path(), home.path());
    let out = s.turn(WORK);
    assert!(
        matches!(&out, TurnOutcome::Facts(words) if words == DOUBTED),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 2);
    let held = attempts(&s)[0].clone();
    let sha = held["candidate_sha256"]
        .as_str()
        .expect("the held bytes")
        .to_owned();
    assert_eq!(held["request"], WORK);
    let correction = "Les octets doivent rester exactement les mêmes.";
    let out = s.turn(correction);
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let restated = format!(
        "Original request:\n{WORK}\nCorrection (it takes precedence over the original where they differ; every other requirement stands):\n{correction}"
    );
    assert_eq!(s.intent.goal.as_deref(), Some(restated.as_str()));
    // The correction's document, then the corrected request judged on the same bytes.
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), CREATE_CALLS + 2 + CREATE_CALLS);
    let fresh = &bodies[CREATE_CALLS + 2..];
    let authored = CREATE_CALLS - 1;
    assert!(fresh[..authored].iter().all(|body| asked(body).is_none()));
    let (state, options) = asked(&fresh[authored]).expect("the corrected request judged");
    assert_eq!(state["request"], restated.as_str());
    let judged = state["candidate_nika"].as_str().expect("the judged bytes");
    assert_eq!(sha256_hex(judged.as_bytes()), sha, "the held bytes again");
    assert_eq!(options, ["faithful", "unfaithful", "none"]);
    let attempts = attempts(&s);
    assert_eq!(attempts.len(), 1, "{attempts:#?}");
    assert_eq!(attempts[0]["candidate_sha256"], sha.as_str());
    assert_eq!(attempts[0]["request"], restated.as_str());
    assert_eq!(attempts[0]["carried"], false);
    assert_eq!(attempts[0]["settled_by"], "verify-request");
    assert_eq!(verify_steps(&s), ["verify: judged (authoring_provider)"]);
    // The verdict on the earlier request follows the restated goal, handed to every compile of
    // it; the core carries it only into a compile of the request it judged.
    assert_eq!(s.declined.carried(Some(&restated)), [held]);
    assert_eq!(
        s.declined.carried(Some(&WORK.to_owned())),
        Vec::<Value>::new()
    );
    assert!(
        !dir.path().join("sortie.txt").exists(),
        "proposed, never run"
    );
}
