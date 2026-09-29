// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The spelling law's relation-canonical confirmation (B24). A confirmed unmatched pair shows a
//! dropped spelling; a private copy of the program in which every string relation compares
//! canonical forms, every value keeping its bytes, decides its cause. A drop that disappears there
//! is a byte comparison, refused however the compared text is written; a drop that persists is a
//! value the program computes, recorded for the judges and never a pass. The seat is a double: it
//! scripts a provider and approves (or, when named, refuses) every verifier question; it decides
//! nothing.
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

/// « livré » as the request states it (precomposed) and as a file may spell it (e + U+0301).
const LIVRE_NFC: &str = "livr\u{e9}";
const LIVRE_NFD: &str = "livre\u{301}";

/// An injected seat: its authoring calls are answered in order and their last message kept;
/// every verifier question gets the approving verdict, or the refusing one when the test names
/// it, and the STATE it showed is kept.
struct Seat {
    answers: Vec<String>,
    said: Mutex<Vec<String>>,
    judged: Mutex<Vec<Value>>,
    refuses: bool,
}

impl Seat {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers,
            said: Mutex::new(Vec::new()),
            judged: Mutex::new(Vec::new()),
            refuses: false,
        }
    }
    fn refusing(answers: Vec<String>) -> Self {
        Self {
            refuses: true,
            ..Self::new(answers)
        }
    }
    fn said(&self, at: usize) -> String {
        self.said.lock().unwrap()[at].clone()
    }
    fn judged(&self) -> Vec<Value> {
        self.judged.lock().unwrap().clone()
    }
}

fn last_text(request: &InferRequest) -> String {
    request
        .messages
        .last()
        .and_then(|message| {
            message.content.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .unwrap_or_default()
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
        let offers = |key: &str| keys.iter().any(|k| k == key);
        let text = if offers("faithful") || offers("carried") || offers("another_part") {
            let said = last_text(&request);
            let state = said
                .strip_prefix("STATE:\n")
                .and_then(|rest| rest.split("\n\nOPTIONS:").next())
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or(Value::Null);
            self.judged.lock().unwrap().push(state);
            let choice = match (self.refuses, offers("faithful"), offers("carried")) {
                (false, true, _) => "faithful",
                (false, false, true) => "carried",
                (true, true, _) => "unfaithful",
                (true, false, true) => "missing",
                _ => "another_part",
            };
            json!({"choice": choice}).to_string()
        } else {
            let mut said = self.said.lock().unwrap();
            said.push(last_text(&request));
            self.answers[(said.len() - 1).min(self.answers.len() - 1)].clone()
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The one value `program` emits over `input`, with the capability-filtered jaq stack.
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
    let modules = Loader::new(defs).load(&arena, file).expect("parses");
    let filter = Compiler::default()
        .with_funs(funs)
        .with_global_vars(std::iter::once(nika_cap::JQ_RUN_START_VAR))
        .compile(modules)
        .expect("compiles");
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

/// The request stating `clause` over ./data/input.csv, and the seat's plan of it.
fn request_of(clause: &str) -> (String, Value) {
    let write = "write the sum to ./out/result.json";
    let text = format!("read ./data/input.csv, {clause}, {write}");
    let proposal = json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": "read ./data/input.csv"},
            {"op": "compute", "detail": clause, "evidence": clause, "computation": {"present": false}}
        ],
        "effects": [{"verb": "write", "target": "./out/result.json", "policy": "automatic", "evidence": write}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "read ./data/input.csv,", "role": "operation"},
            {"text": format!("{clause},"), "role": "operation"},
            {"text": write, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    (text, proposal)
}

fn livre() -> String {
    format!("sum qty over the rows where status is {LIVRE_NFC}")
}

/// A transform answer verified on the seat's own example rows (spelled as the request states).
fn program(jq: &str, columns: &[&str], expected: &Value) -> String {
    let example = json!([
        {"id": "a1", "item": "x", "status": LIVRE_NFC, "qty": "40"},
        {"id": "a2", "item": "y", "status": "en attente", "qty": "15"}
    ]);
    json!({"jq": jq, "columns_read": columns, "example_input": example, "expected_output": expected})
        .to_string()
}

/// The repair: qty summed over the rows spelled either way.
fn summing() -> String {
    let jq = format!(
        ".records | map(select(.status == {} or .status == {}) | .qty | tonumber) | add // 0",
        json!(LIVRE_NFC),
        json!(LIVRE_NFD)
    );
    program(&jq, &["status", "qty"], &json!(40))
}

fn world() -> Value {
    json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false,
        "kind": "csv", "columns": ["id", "item", "status", "qty"],
        "values": {"status": [LIVRE_NFD, "en attente"], "item": ["x", "y"]}}]})
}

/// Rows spelled as the file spells them: two delivered (40 and 2) beside a pending one.
fn delivered() -> Value {
    json!({"records": [
        {"id": "r1", "item": "x", "status": LIVRE_NFD, "qty": "40"},
        {"id": "r2", "item": "y", "status": "en attente", "qty": "15"},
        {"id": "r3", "item": "z", "status": LIVRE_NFD, "qty": "2"}
    ]})
}

async fn compiled(seat: &Seat, clause: &str, repairs: u32) -> CompileOutcome {
    let policy =
        AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(repairs);
    let request = CompileRequest::create(request_of(clause).0)
        .with_knowledge(world())
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    compile_with_provider(&request, seat).await.unwrap()
}

fn compute(out: &CompileOutcome) -> String {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    doc["tasks"]["compute"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn authored(out: &CompileOutcome) -> Vec<String> {
    let receipt = out.provenance.authoring.as_ref().unwrap();
    receipt
        .context
        .iter()
        .filter_map(|c| c["call"].as_str().map(str::to_owned))
        .filter(|role| !role.starts_with("judge_"))
        .collect()
}

fn points(text: &str) -> String {
    let points: Vec<String> = text
        .chars()
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    points.join(" ")
}

/// The reason the law recorded for the program it could not judge, if any.
fn reason(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["transforms"][0]["unjudged"][0]["reason"].clone()
}

/// A byte comparison is refused however the compared text is written: one literal bound to a
/// variable that also inspects its own bytes (B24 S8, which the literal exchange could not
/// confirm), a fragment of the spelling, and an unparenthesized concatenation. Each is told both
/// code-point lists, repaired, and the repaired program sums the delivered rows.
#[tokio::test]
async fn a_byte_comparison_is_refused_however_the_compared_text_is_written() {
    let stated = json!(LIVRE_NFC);
    let programs = [
        format!(
            ".records | map({stated} as $l | select(.status == $l and ($l | length) == 5) | .qty | tonumber) | add // 0"
        ),
        format!(
            ".records | map({stated} as $l | select(.status == $l and ($l | length) < 6) | .qty | tonumber) | add // 0"
        ),
        ".records | map(select(.status | contains(\"\u{e9}\")) | .qty | tonumber) | add // 0"
            .to_owned(),
        ".records | map(select(.status == \"liv\" + \"r\u{e9}\") | .qty | tonumber) | add // 0"
            .to_owned(),
    ];
    for jq in programs {
        let seat = Seat::new(vec![
            request_of(&livre()).1.to_string(),
            program(&jq, &["status", "qty"], &json!(40)),
            summing(),
        ]);
        let out = compiled(&seat, &livre(), 1).await;
        assert_eq!(
            authored(&out),
            ["plan", "transform", "transform_repair"],
            "{jq}"
        );
        let told = seat.said(2);
        assert!(
            told.contains(&points(LIVRE_NFC)) && told.contains(&points(LIVRE_NFD)),
            "{jq}"
        );
        assert_eq!(run(&compute(&out), &delivered()), json!(42), "{jq}");
    }
}

/// A requested value computed from the literal is not a byte comparison: a threshold on its
/// code-point length (reviewer E's case, refused by the literal exchange) and an explicit
/// comparison of percent-encodings. Canonical relations leave the difference in place, so it is
/// recorded for the judges (`relation_unconfirmed`) and the program is admitted with no repair.
#[tokio::test]
async fn a_requested_value_of_the_literal_is_left_to_the_judges() {
    let stated = json!(LIVRE_NFC);
    let cases = [
        (
            format!("say whether each status has as many code points as {LIVRE_NFC}"),
            format!(".records | map((.status | length) == ({stated} | length))"),
            vec!["status"],
            json!([true, false]),
            json!([false, false, false]),
        ),
        (
            format!(
                "sum qty over the rows where the percent-encoding of status is the percent-encoding of {LIVRE_NFC} as typed here"
            ),
            format!(
                ".records | map(select((.status | @uri) == ({stated} | @uri)) | .qty | tonumber) | add // 0"
            ),
            vec!["status", "qty"],
            json!(40),
            json!(0),
        ),
    ];
    for (clause, jq, columns, on_example, on_file) in cases {
        let seat = Seat::new(vec![
            request_of(&clause).1.to_string(),
            program(&jq, &columns, &on_example),
        ]);
        let out = compiled(&seat, &clause, 1).await;
        assert_eq!(authored(&out), ["plan", "transform"], "{jq}: {out:#?}");
        assert_eq!(reason(&out), json!("relation_unconfirmed"), "{jq}");
        assert_eq!(run(&compute(&out), &delivered()), on_file, "{jq}");
    }
}

/// A length used as a proxy for equality with the stated literal (0 on the file where 42 is due)
/// is no byte comparison the law can prove: it is never a silent pass. The approving double
/// reaches READY only with the note, which every verifier question shows the judges; the
/// refusing double keeps it INCOMPLETE.
#[tokio::test]
async fn a_value_used_as_a_proxy_for_equality_reaches_the_judges() {
    let jq = format!(
        ".records | map(select((.status | length) == ({} | length)) | .qty | tonumber) | add // 0",
        json!(LIVRE_NFC)
    );
    let answers = || {
        vec![
            request_of(&livre()).1.to_string(),
            program(&jq, &["status", "qty"], &json!(40)),
        ]
    };
    let approving = Seat::new(answers());
    let out = compiled(&approving, &livre(), 1).await;
    assert_eq!(reason(&out), json!("relation_unconfirmed"), "{out:#?}");
    assert!(!approving.judged().is_empty());
    for state in approving.judged() {
        let noted = state["unjudged_spellings"]
            .as_array()
            .is_some_and(|notes| notes.iter().any(|n| n["reason"] == "relation_unconfirmed"));
        assert!(noted, "{state:#}");
    }
    let refusing = Seat::refusing(answers());
    let refused = compiled(&refusing, &livre(), 1).await;
    assert_eq!(refused.status, CompileStatus::Incomplete, "{refused:#?}");
}
