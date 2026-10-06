// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An output the request defines is generated, never asked as a source field. A live plan put
//! « a master id equal to the smallest `contact_id` in their group » among a typed projection's
//! columns; the grounding then asked which observed field of the source that output means, a
//! question no answer settles. A projected output the computation's own clause names, that no
//! observed key spells and that the computation reads nowhere, is no typed rule: the existing
//! verified transform states it from the observed keys. The contrasts keep every missing input
//! asked: a typed filter on a key the source lacks, and a transform program reading one. The seat
//! is a double: it scripts a provider and approves every verifier question; it decides nothing.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data};
use jaq_json::{Val, read};
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, HotPolicy};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;

mod common;
use common::{approval, verifier};

/// The source every request reads and the keys the host observed in it, in every record.
const SOURCE: &str = "./data/tickets.json";
const OBSERVED: [&str; 3] = ["email", "ticket_id", "title"];

/// An output the clause defines over observed keys.
const DEFINED: &str = "give each ticket a root id equal to the smallest ticket_id among the tickets sharing its email";
/// The same definition over `owner`, a key the source does not carry.
const OVER_UNKNOWN: &str = "give each ticket a root id equal to the smallest ticket_id among the tickets sharing its owner";
/// The seat's detail of a compute step, in its own words.
const DETAIL: &str = "Partition the tickets into classes of those sharing the key the clause names; each ticket keeps the smallest ticket of its class as its root.";
/// The write of the defined output beside a copied key.
const WRITE_ROOTS: &str =
    "save ./out/roots.json as a list of {ticket_id, root_id} objects sorted by ticket_id";

/// An injected seat: its authoring calls are answered in order; every verifier question gets the
/// approving verdict.
struct Seat {
    answers: Vec<String>,
    said: Mutex<usize>,
}

impl Seat {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers,
            said: Mutex::new(0),
        }
    }
}

impl ProviderInferDyn for Seat {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let keys: Vec<String> = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema["properties"]["choice"]["enum"]
                .as_array()
                .map(|keys| {
                    keys.iter()
                        .filter_map(|k| k.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let text = if verifier(&keys) {
            json!({"choice": approval(&keys)}).to_string()
        } else {
            let mut said = self.said.lock().unwrap();
            *said += 1;
            self.answers[(*said - 1).min(self.answers.len() - 1)].clone()
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The one value `program` emits over `input`, with the jaq definitions and natives the
/// runtime's capability filter installs.
fn run(program: &str, input: &Value) -> Value {
    let defs = jaq_core::defs()
        .chain(jaq_std::defs().filter(|d| nika_cap::install_jq_definition(d.name)))
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| nika_cap::install_jq_native(f.0));
    let arena = Arena::default();
    let file = File {
        code: program,
        path: (),
    };
    let modules = Loader::new(defs)
        .load(&arena, file)
        .expect("the program parses");
    let filter = Compiler::default()
        .with_funs(funs)
        .with_global_vars(std::iter::once(nika_cap::JQ_RUN_START_VAR))
        .compile(modules)
        .expect("the program compiles");
    let vars = Vars::new(std::iter::once(Val::from(1_700_000_000_isize)));
    let ctx = Ctx::<jaq_data::JustLut<Val>>::new(&filter.lut, vars);
    let val = read::parse_single(&serde_json::to_vec(input).unwrap()).unwrap();
    let out: Vec<Val> = filter
        .id
        .run((ctx, val))
        .map(|r| r.expect("runs"))
        .collect();
    assert_eq!(out.len(), 1, "one value");
    serde_json::from_str(&out[0].to_string()).unwrap()
}

/// The request reading the source, stating `clause` and `write`, and the seat's plan of it: the
/// clause a compute step carrying `computation` (the seat's typed projection), its detail the
/// seat's own prose, which the closed grammar does not read.
fn request_of(clause: &str, write: &str, computation: &Value) -> (String, Value) {
    let text = format!("read {SOURCE}, {clause}, {write}");
    let target = write
        .split_whitespace()
        .find(|word| word.starts_with("./out/"))
        .unwrap();
    let proposal = json!({
        "steps": [
            {"op": "read", "detail": SOURCE, "evidence": format!("read {SOURCE}")},
            {"op": "compute", "detail": DETAIL, "evidence": clause, "computation": computation}
        ],
        "effects": [{"verb": "write", "target": target, "policy": "automatic", "evidence": write}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": format!("read {SOURCE},"), "role": "operation"},
            {"text": format!("{clause},"), "role": "operation"},
            {"text": write, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    (text, proposal)
}

/// The seat's typed projection of the defined output: the copied key and the defined one, sorted
/// by the copied key, nothing filtered, grouped or totalled.
fn projection() -> Value {
    json!({"present": true, "polarity": "keep", "join": "and", "clauses": [], "group_by": "",
        "aggregations": [], "sort_by": "ticket_id", "order": "asc", "ties": "",
        "columns": ["ticket_id", "root_id"], "numbers": [], "derived": [], "limit": "",
        "renames": [], "distinct_by": []})
}

/// A transform answer: the program grouping the tickets by `key`, each given the smallest
/// `ticket_id` of its group, verified on the seat's own example and what it returns on it.
fn program(key: &str) -> String {
    let jq = format!(
        ".records | (group_by(.{key}) | map((map(.ticket_id) | sort | .[0]) as $root | map({{ticket_id: .ticket_id, root_id: $root}})) | add // []) | sort_by(.ticket_id)"
    );
    let example = json!([
        {"ticket_id": "T-3", key: "a", "title": "x"},
        {"ticket_id": "T-1", key: "a", "title": "y"},
        {"ticket_id": "T-2", key: "b", "title": "z"}
    ]);
    let expected = json!([
        {"ticket_id": "T-1", "root_id": "T-1"},
        {"ticket_id": "T-2", "root_id": "T-2"},
        {"ticket_id": "T-3", "root_id": "T-1"}
    ]);
    json!({"jq": jq, "columns_read": ["ticket_id", key], "example_input": example,
        "expected_output": expected})
    .to_string()
}

/// The host's observation of the source: every key in every record, nothing else stated.
fn world() -> Value {
    json!({"observed": [{"path": SOURCE, "state": "observed", "complete": true, "kind": "json",
        "columns": OBSERVED, "common_columns": OBSERVED}]})
}

/// The request compiled COLD over the observation by `seat`, one repair allowed.
async fn compiled(seat: &Seat, text: String) -> CompileOutcome {
    let policy =
        AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(1);
    let request = CompileRequest::create(text)
        .with_knowledge(world())
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    compile_with_provider(&request, seat).await.unwrap()
}

/// The roles of the authoring calls the receipt journals, the judges' apart.
fn authored(out: &CompileOutcome) -> Vec<String> {
    let receipt = out.provenance.authoring.as_ref().unwrap();
    receipt
        .context
        .iter()
        .filter_map(|c| c["call"].as_str().map(str::to_owned))
        .filter(|role| !role.starts_with("judge_"))
        .collect()
}

/// The field question of `out`, with its offered keys sorted.
fn field_question(out: &CompileOutcome) -> Option<(String, Vec<String>)> {
    let question = out
        .questions
        .iter()
        .find(|q| q.key.starts_with("const.rule_field"))?;
    let mut offered: Vec<String> = question.options.iter().map(|o| o.key.clone()).collect();
    offered.sort();
    Some((question.label.clone(), offered))
}

/// RED before the fix: the typed projection was admitted with `root_id` read from the source and
/// the compile asked which observed field `root_id` means. Now the defined output reaches the
/// transform, whose verified program writes it from observed keys: READY, no field question.
#[tokio::test]
async fn a_defined_output_is_generated_by_the_transform_never_asked_as_a_source_field() {
    let (text, plan) = request_of(DEFINED, WRITE_ROOTS, &projection());
    let seat = Seat::new(vec![plan.to_string(), program("email")]);
    let out = compiled(&seat, text).await;
    assert_eq!(field_question(&out), None, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
    let grounding = &out.provenance.decision.as_ref().unwrap()["grounding"];
    let mut grounded = grounding.as_array().into_iter().flatten();
    assert!(grounded.all(|e| e["field"] != "root_id"), "{grounding:#}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    let expression = doc["tasks"]["compute"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap();
    let rows = json!({"records": [
        {"ticket_id": "T-4", "email": "ben@x.test", "title": "d"},
        {"ticket_id": "T-3", "email": "ana@x.test", "title": "c"},
        {"ticket_id": "T-1", "email": "ana@x.test", "title": "a"},
        {"ticket_id": "T-2", "email": "ben@x.test", "title": "b"}
    ]});
    let roots = json!([
        {"ticket_id": "T-1", "root_id": "T-1"},
        {"ticket_id": "T-2", "root_id": "T-2"},
        {"ticket_id": "T-3", "root_id": "T-1"},
        {"ticket_id": "T-4", "root_id": "T-2"}
    ]);
    assert_eq!(run(expression, &rows), roots, "{expression}");
}

/// Contrast: a typed filter on `state`, a key the source does not carry, is a missing input. Its
/// rule stands with no transform call, and the compile asks which observed key `state` means, a
/// closed choice of exactly the observed keys.
#[tokio::test]
async fn a_typed_filter_on_an_unknown_source_field_still_asks_for_it() {
    let filter = json!({"present": true, "polarity": "keep", "join": "and",
        "clauses": [{"field": "state", "op": "eq", "value": "open", "value_field": ""}],
        "group_by": "", "aggregations": [], "sort_by": "", "order": "", "ties": "",
        "columns": ["ticket_id", "title"], "numbers": [], "derived": [], "limit": "",
        "renames": [], "distinct_by": []});
    let write = "save ./out/open.json as a list of {ticket_id, title} objects";
    let (text, plan) = request_of("keep the tickets whose state is open", write, &filter);
    let seat = Seat::new(vec![plan.to_string()]);
    let out = compiled(&seat, text).await;
    let (label, offered) = field_question(&out).expect("a field question");
    assert!(label.contains("`state`"), "{label}");
    assert_eq!(offered, OBSERVED, "{out:#?}");
    assert_eq!(authored(&out), ["plan"], "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
}

/// Contrast: the same defined output over `owner`, a key the source does not carry. The output
/// still reaches the transform, and the program reading `owner` is held until a human maps it:
/// the compile asks which observed key `owner` means, never a question about `root_id`.
#[tokio::test]
async fn a_defined_output_over_an_unknown_source_field_still_asks_for_that_field() {
    let (text, plan) = request_of(OVER_UNKNOWN, WRITE_ROOTS, &projection());
    let seat = Seat::new(vec![plan.to_string(), program("owner")]);
    let out = compiled(&seat, text).await;
    let (label, offered) = field_question(&out).expect("a field question");
    assert!(label.contains("`owner`"), "{label}");
    assert!(!label.contains("`root_id`"), "{label}");
    assert_eq!(offered, OBSERVED, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
}

/// A source field that is explicitly copied remains a human field choice. The transformer
/// below offers a different, observed field as its value; approving meaning doubles must not
/// turn that substitution into authority to map the missing input.
#[tokio::test]
async fn an_absent_copied_input_cannot_be_replaced_by_an_observed_sibling() {
    let clause =
        "give each ticket its ticket_id, email and owner exactly as recorded in the source";
    let write =
        "save ./out/owners.json as a list of {ticket_id, email, owner} objects sorted by ticket_id";
    let mut computation = projection();
    computation["columns"] = json!(["ticket_id", "email", "owner"]);
    let (text, mut plan) = request_of(clause, write, &computation);
    plan["steps"][1]["detail"] = json!(
        "Produce one output object per source row, carrying the three requested values exactly."
    );
    let example = json!([
        {"ticket_id": "T-1", "email": "a@x.test", "title": "one"},
        {"ticket_id": "T-2", "email": "b@x.test", "title": "two"}
    ]);
    let substitute = json!({
        "jq": ".records | map({ticket_id: .ticket_id, email: .email, owner: .email}) | sort_by(.ticket_id)",
        "columns_read": ["ticket_id", "email"],
        "example_input": example,
        "expected_output": [
            {"ticket_id": "T-1", "email": "a@x.test", "owner": "a@x.test"},
            {"ticket_id": "T-2", "email": "b@x.test", "owner": "b@x.test"}
        ]
    });
    let seat = Seat::new(vec![plan.to_string(), substitute.to_string()]);
    let out = compiled(&seat, text).await;
    let (label, offered) = field_question(&out).expect("the absent copied input stays asked");
    assert!(label.contains("`owner`"), "{out:#?}");
    assert_eq!(offered, OBSERVED, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
}
