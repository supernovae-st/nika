// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The observed spelling of a literal a clause states, held by a program a seat writes (R4 A11).
//! « …status is livré » (precomposed) over a status the host observed spelled e + U+0301: the
//! typed equality matches both spellings (R4 A5), while a seat's program comparing bytes was
//! READY and summed nothing. Where a column the program reads carries, among its host-observed
//! categorical values, a spelling canonically equivalent to a literal the clause states at exact
//! token boundaries, the program must treat both alike, or be repaired from that concrete
//! defect. A bounded law over the seat's own example rows and the host's bounded sample, never a
//! proof that two programs mean the same. The seat here is a double: it scripts a provider and
//! approves every verifier question to reach the assembly; it decides nothing.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data};
use jaq_json::{Val, read};
use nika_compile::surface::observed::{equivalent_spellings, stated_spellings};
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
/// every verifier question gets the approving verdict (the explicit approving double), or the
/// refusing one when the test names it, and the STATE it showed is kept.
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
    /// The same double refusing every verifier question: each clause missing, the request
    /// unfaithful, no part located.
    fn refusing(answers: Vec<String>) -> Self {
        Self {
            refuses: true,
            ..Self::new(answers)
        }
    }
    /// The last message of the authoring call `at`.
    fn said(&self, at: usize) -> String {
        self.said.lock().unwrap()[at].clone()
    }
    /// The STATE each verifier question showed the judge, in order.
    fn judged(&self) -> Vec<Value> {
        self.judged.lock().unwrap().clone()
    }
}

/// The text of a request's last message.
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
        let verifier = offers("faithful") || offers("carried") || offers("another_part");
        let text = if verifier {
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

/// The request stating `clause` over ./data/input.csv, and the seat's plan of it (a computation
/// the typed stages do not state: treatment B).
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

/// The clause of the live shape: a sum over the rows whose status is the precomposed « livré ».
fn livre() -> String {
    format!("sum qty over the rows where status is {LIVRE_NFC}")
}

/// A transform answer: `jq` reading `columns`, verified on the seat's own example rows (the
/// delivered row spelled as the request states it) and what it returns on them.
fn program(jq: &str, columns: &[&str], expected: &Value) -> String {
    let example = json!([
        {"id": "a1", "item": "x", "status": LIVRE_NFC, "qty": "40"},
        {"id": "a2", "item": "y", "status": "en attente", "qty": "15"}
    ]);
    json!({"jq": jq, "columns_read": columns, "example_input": example, "expected_output": expected})
        .to_string()
}

/// The seat program summing qty over the rows whose status is one of `spellings`.
fn summing(spellings: &[&str]) -> String {
    let test: Vec<String> = spellings
        .iter()
        .map(|s| format!(".status == {}", json!(s)))
        .collect();
    let jq = format!(
        ".records | map(select({}) | .qty | tonumber) | add // 0",
        test.join(" or ")
    );
    program(&jq, &["status", "qty"], &json!(40))
}

/// The observation of ./data/input.csv as a host builds it: its columns and the categorical
/// values of `status` and of `item`.
fn spelled(status: &[&str], item: &[&str]) -> Value {
    json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false,
        "kind": "csv", "columns": ["id", "item", "status", "qty"],
        "values": {"status": status, "item": item}}]})
}

/// Rows carrying `spelling`: two delivered rows (40 and 2) beside a pending one.
fn delivered(spelling: &str) -> Value {
    json!({"records": [
        {"id": "r1", "item": "x", "status": spelling, "qty": "40"},
        {"id": "r2", "item": "y", "status": "en attente", "qty": "15"},
        {"id": "r3", "item": "z", "status": spelling, "qty": "2"}
    ]})
}

/// The request of `clause` compiled COLD over `world` by `seat` under `repairs` repair rounds.
async fn compiled(seat: &Seat, clause: &str, world: Value, repairs: u32) -> CompileOutcome {
    let policy =
        AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(repairs);
    let request = CompileRequest::create(request_of(clause).0)
        .with_knowledge(world)
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    compile_with_provider(&request, seat).await.unwrap()
}

/// The compute program of a READY candidate.
fn compute(out: &CompileOutcome) -> String {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    doc["tasks"]["compute"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Whether the candidate's own Check preview is clean.
fn checked(out: &CompileOutcome) -> bool {
    out.check_preview
        .as_ref()
        .is_some_and(|preview| preview.report.is_clean())
}

/// The roles of the authoring calls the receipt journals, the judge's apart.
fn authored(out: &CompileOutcome) -> Vec<String> {
    let receipt = out.provenance.authoring.as_ref().unwrap();
    receipt
        .context
        .iter()
        .filter_map(|c| c["call"].as_str().map(str::to_owned))
        .filter(|role| !role.starts_with("judge_"))
        .collect()
}

/// The code points of `text`, as a refusal names them.
fn points(text: &str) -> String {
    let points: Vec<String> = text
        .chars()
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    points.join(" ")
}

/// RED on 00fa-era cognition (frozen, h1-red-a4077cc0c): the program the seat verified on its
/// own precomposed example was READY and, executed on rows carrying the observed spelling, summed
/// nothing. It is refused naming both spellings; the repaired program (its second answer)
/// compares both, the stated literal kept, and sums the delivered rows.
#[tokio::test]
async fn a_seat_program_literal_meets_the_observed_spelling() {
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        summing(&[LIVRE_NFC]),
        summing(&[LIVRE_NFC, LIVRE_NFD]),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    let program = compute(&out);
    assert_eq!(run(&program, &delivered(LIVRE_NFD)), json!(42), "{program}");
    assert!(checked(&out), "{out:#?}");
}

/// Control: the typed equality of the same literal over the same observation matches the
/// observed spelling exactly beside the stated one (R4 A5), the decision records it, and the
/// emitted program keeps the delivered rows.
#[test]
fn a_typed_equality_of_the_literal_matches_the_observed_spelling() {
    let text = format!(
        "Lis ./data/input.csv, garde seulement les lignes dont le status est {LIVRE_NFC} et écris-les dans ./out/livrees.csv"
    );
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = nika_compile::compile(&CompileRequest::create(text).with_knowledge(world)).unwrap();
    let program = compute(&out);
    let record = &out.provenance.decision.as_ref().unwrap()["spellings"][0];
    assert_eq!(record["spellings"], json!([LIVRE_NFD]), "{record}");
    let kept = run(&program, &delivered(LIVRE_NFD));
    assert_eq!(kept.as_array().map(Vec::len), Some(2), "{program}: {kept}");
}

/// Control: an observed spelling byte-identical to the stated literal leaves the seat's program
/// admitted as it is, READY, summing the delivered rows.
#[tokio::test]
async fn a_seat_program_literal_byte_identical_to_the_observation_is_admitted() {
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        summing(&[LIVRE_NFC]),
    ]);
    let world = spelled(&[LIVRE_NFC, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    let program = compute(&out);
    assert_eq!(run(&program, &delivered(LIVRE_NFC)), json!(42), "{program}");
    assert_eq!(authored(&out), ["plan", "transform"]);
}

/// Control: an equivalent spelling observed in a column the program never reads binds nothing;
/// the program is admitted as it is.
#[tokio::test]
async fn an_equivalent_spelling_in_an_uncompared_column_binds_nothing() {
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        summing(&[LIVRE_NFC]),
    ]);
    let world = spelled(&[LIVRE_NFC, "en attente"], &[LIVRE_NFD, "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    let program = compute(&out);
    assert_eq!(run(&program, &delivered(LIVRE_NFC)), json!(42), "{program}");
    assert_eq!(authored(&out), ["plan", "transform"]);
}

/// A program that echoes the value, or groups the rows by it, is not refused for echoing it:
/// its output on the observed spelling, read back as the stated one, is its output on the
/// stated spelling. Both are admitted as they are, with no repair.
#[tokio::test]
async fn a_program_echoing_or_grouping_the_value_is_admitted() {
    let echo = format!(
        ".records | map(select(.status == {} or .status == {})) | {{statuses: (map(.status) | unique), total: (map(.qty | tonumber) | add // 0)}}",
        json!(LIVRE_NFC),
        json!(LIVRE_NFD)
    );
    let group = ".records | group_by(.status) | map({status: .[0].status, total: (map(.qty | tonumber) | add)})";
    let answers = [
        program(
            &echo,
            &["status", "qty"],
            &json!({"statuses": [LIVRE_NFC], "total": 40}),
        ),
        program(
            group,
            &["status", "qty"],
            &json!([{"status": "en attente", "total": 15}, {"status": LIVRE_NFC, "total": 40}]),
        ),
    ];
    for answer in answers {
        let seat = Seat::new(vec![request_of(&livre()).1.to_string(), answer]);
        let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
        let out = compiled(&seat, &livre(), world, 1).await;
        assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
        assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
    }
}

/// An equivalent spelling observed in a column the program reads but never compares (the clause
/// names the item it lists) binds that column, and the program treats both spellings alike
/// there: it is admitted as it is.
#[tokio::test]
async fn an_equivalent_spelling_in_a_read_but_uncompared_column_is_admitted() {
    let clause = format!("{} and list their item", livre());
    let jq = format!(
        ".records | map(select(.status == {})) | {{items: map(.item), total: (map(.qty | tonumber) | add // 0)}}",
        json!(LIVRE_NFC)
    );
    let answer = program(
        &jq,
        &["status", "item", "qty"],
        &json!({"items": ["x"], "total": 40}),
    );
    let seat = Seat::new(vec![request_of(&clause).1.to_string(), answer]);
    let world = spelled(&[LIVRE_NFC, "en attente"], &[LIVRE_NFD, "y"]);
    let out = compiled(&seat, &clause, world, 1).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
}

/// A clause stating a similar word (« livrée ») binds nothing: the observed « livré » occurs
/// inside it, never at exact token boundaries. The program is admitted as it is.
#[tokio::test]
async fn a_similar_word_in_the_clause_binds_nothing() {
    let clause = format!("sum qty over the rows where status is {LIVRE_NFC}e");
    let jq = format!(
        ".records | map(select(.status == {}) | .qty | tonumber) | add // 0",
        json!(format!("{LIVRE_NFC}e"))
    );
    let example = json!([
        {"id": "a1", "status": format!("{LIVRE_NFC}e"), "qty": "40"},
        {"id": "a2", "status": "en attente", "qty": "15"}
    ]);
    let answer = json!({"jq": jq, "columns_read": ["status", "qty"], "example_input": example, "expected_output": 40})
        .to_string();
    let seat = Seat::new(vec![request_of(&clause).1.to_string(), answer]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &clause, world, 1).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
}

/// A program already comparing both the canonical and the decomposed spelling is admitted as it
/// is, with no repair, and sums the delivered rows whichever spelling the source holds.
#[tokio::test]
async fn a_program_accepting_both_spellings_is_admitted_as_it_is() {
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        summing(&[LIVRE_NFC, LIVRE_NFD]),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    let program = compute(&out);
    assert_eq!(authored(&out), ["plan", "transform"]);
    assert_eq!(run(&program, &delivered(LIVRE_NFD)), json!(42), "{program}");
    assert_eq!(run(&program, &delivered(LIVRE_NFC)), json!(42), "{program}");
    assert!(checked(&out), "{out:#?}");
}

/// The repair is told the concrete defect: the column, both spellings and their code points,
/// with the refused program; the attempt is recorded, and its answer is held to the same law.
#[tokio::test]
async fn the_repair_is_told_both_spellings() {
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        summing(&[LIVRE_NFC]),
        summing(&[LIVRE_NFC, LIVRE_NFD]),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform", "transform_repair"]);
    let told: Value = serde_json::from_str(&seat.said(2)).unwrap();
    let refused = told["verifier"]["refused"].as_str().unwrap();
    for part in ["`status`", &points(LIVRE_NFC), &points(LIVRE_NFD)] {
        assert!(refused.contains(part), "{part}: {refused}");
    }
    assert_eq!(
        told["observed_values"]["status"],
        json!([LIVRE_NFD, "en attente"])
    );
    let attempts = &out.provenance.decision.as_ref().unwrap()["transform_repairs"];
    assert_eq!(attempts[0]["call"], "answered", "{attempts:#}");
}

/// With no repair granted, the refused program is never emitted: the request stays INCOMPLETE,
/// naming the defect, and no repair call is made.
#[tokio::test]
async fn with_no_repair_the_refused_program_stays_incomplete() {
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        summing(&[LIVRE_NFC]),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 0).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"]);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "authoring_transform" && d.message.contains(&points(LIVRE_NFD))),
        "{out:#?}"
    );
    let transforms = &out.provenance.decision.as_ref().unwrap()["transforms"];
    assert_eq!(transforms[0]["accepted"], false, "{transforms:#}");
}

/// A negated literal compared by bytes (R4 A11, B21 T4): « …status is not livré » and
/// `.status != "livré"` keep the rows the file spells e + U+0301, which the clause excludes. The
/// program is refused, and the refusal says what it does in neutral words: it treats the observed
/// spelling as it treats a value the clause does not state, not as it treats the stated one. It
/// no longer claims the rows are dropped, nor asks to keep the stated spelling. The repair
/// excluding both spellings sums the one row the clause keeps.
#[tokio::test]
async fn a_negated_literal_compared_by_bytes_is_refused_in_neutral_words() {
    let clause = format!("sum qty over the rows where status is not {LIVRE_NFC}");
    let excluding = |spellings: &[&str]| {
        let test: Vec<String> = spellings
            .iter()
            .map(|s| format!(".status != {}", json!(s)))
            .collect();
        format!(
            ".records | map(select({}) | .qty | tonumber) | add // 0",
            test.join(" and ")
        )
    };
    let seat = Seat::new(vec![
        request_of(&clause).1.to_string(),
        program(&excluding(&[LIVRE_NFC]), &["status", "qty"], &json!(15)),
        program(
            &excluding(&[LIVRE_NFC, LIVRE_NFD]),
            &["status", "qty"],
            &json!(15),
        ),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &clause, world, 1).await;
    assert_eq!(
        authored(&out),
        ["plan", "transform", "transform_repair"],
        "{out:#?}"
    );
    let refused = told(&seat, 2);
    let neutral = "the program treats the observed spelling as it treats a value the clause does not state, not as it treats the stated spelling";
    assert!(refused.contains(neutral), "{refused}");
    for claim in ["drop", "keep the stated spelling"] {
        assert!(!refused.contains(claim), "{claim}: {refused}");
    }
    let program = compute(&out);
    assert_eq!(run(&program, &delivered(LIVRE_NFD)), json!(15), "{program}");
}

/// A program that returns a value on the stated spelling and fails on the observed one (the branch
/// the observed spelling reaches feeds its text to `tonumber`) escapes nothing: an asymmetric
/// error is a spelling difference. It is refused, repaired from that defect within the
/// allowance, and the repaired program sums the delivered rows.
#[tokio::test]
async fn an_observed_spelling_the_program_fails_on_is_refused() {
    let jq = format!(
        ".records | map(if .status == {} then (.qty | tonumber) elif (.status | length) > 6 then 0 else (.status | tonumber) end) | add // 0",
        json!(LIVRE_NFC)
    );
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        program(&jq, &["status", "qty"], &json!(40)),
        summing(&[LIVRE_NFC, LIVRE_NFD]),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    assert_eq!(
        authored(&out),
        ["plan", "transform", "transform_repair"],
        "{out:#?}"
    );
    let program = compute(&out);
    assert_eq!(run(&program, &delivered(LIVRE_NFD)), json!(42), "{program}");
}

/// A row the program fails on whatever the spelling (here an odd quantity) is no spelling
/// difference: the program is equally undefined for both, so the law compares nothing there and
/// the value laws and the run own that error. The other rows agree; the program is admitted as
/// it is.
#[tokio::test]
async fn a_row_failing_whatever_the_spelling_is_no_spelling_difference() {
    let jq = format!(
        ".records | map(select(.status == {} or .status == {}) | if (.qty | tonumber) % 2 == 1 then error(.qty) else (.qty | tonumber) end) | add // 0",
        json!(LIVRE_NFC),
        json!(LIVRE_NFD)
    );
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        program(&jq, &["status", "qty"], &json!(40)),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    let program = compute(&out);
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
    assert_eq!(run(&program, &delivered(LIVRE_NFD)), json!(42), "{program}");
}

/// A transform answer like [`program`] over the seat's own example rows, spelled `statuses`
/// (qty 40, then 15).
fn program_on(jq: &str, statuses: [&str; 2], expected: &Value) -> String {
    let example = json!([
        {"id": "a1", "item": "x", "status": statuses[0], "qty": "40"},
        {"id": "a2", "item": "y", "status": statuses[1], "qty": "15"}
    ]);
    json!({"jq": jq, "columns_read": ["status", "qty"], "example_input": example, "expected_output": expected})
        .to_string()
}

/// A program summing qty over the rows whose status is exactly `stated`, which stops with an error
/// on a row `condition` selects: the adversarial shape of B21 D1, which erred on its unmatched
/// probe.
fn erring(condition: &str, stated: &str) -> String {
    format!(
        ".records | map(if {condition} then error(\"?\") elif .status == {} then (.qty | tonumber) else 0 end) | add // 0",
        json!(stated)
    )
}

/// B21 D1's condition, byte for byte: a status holding no ASCII letter, as U+2400 is.
const NO_ASCII_LETTER: &str = "(.status | ascii_downcase) == (.status | ascii_upcase)";

/// The refusal the repair was told in the authoring call `at`.
fn told(seat: &Seat, at: usize) -> String {
    let told: Value = serde_json::from_str(&seat.said(at)).unwrap();
    told["verifier"]["refused"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// B21 D1, retained (R4 A11, B21 T1): a program that stops with an error on the unmatched probe
/// text and otherwise compares the stated bytes was READY and summed 0 on rows the source spells
/// e + U+0301, where 42 is due: the error left the law nothing to compare. The law now reads the
/// program's treatment of a value the clause does not state from a value the host observed in
/// the bound column (« en attente »): the observed spelling is treated as that value, not as the
/// stated one. The program is refused naming both spellings, repaired, and sums 42.
#[tokio::test]
async fn an_error_on_the_unmatched_probe_does_not_silence_the_law() {
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        program(
            &erring(NO_ASCII_LETTER, LIVRE_NFC),
            &["status", "qty"],
            &json!(40),
        ),
        summing(&[LIVRE_NFC, LIVRE_NFD]),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    let program = compute(&out);
    let sum = run(&program, &delivered(LIVRE_NFD));
    assert_eq!(
        authored(&out),
        ["plan", "transform", "transform_repair"],
        "READY with `{program}`, summing {sum} on rows spelled e + U+0301"
    );
    let refused = told(&seat, 2);
    for part in ["`status`", &points(LIVRE_NFC), &points(LIVRE_NFD)] {
        assert!(refused.contains(part), "{part}: {refused}");
    }
    assert_eq!(sum, json!(42), "{program}");
}

/// A fresh erring shape (R4 A11, B21 T1): the program stops with an error on a status shorter
/// than two characters, as U+2400 is, and compares the stated bytes otherwise. The observed
/// value probes its treatment of a value the clause does not state all the same: refused,
/// repaired, 42.
#[tokio::test]
async fn an_unmatched_probe_error_of_another_shape_is_probed_with_an_observed_value() {
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        program(
            &erring("(.status | length) < 2", LIVRE_NFC),
            &["status", "qty"],
            &json!(40),
        ),
        summing(&[LIVRE_NFC, LIVRE_NFD]),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    assert_eq!(
        authored(&out),
        ["plan", "transform", "transform_repair"],
        "{out:#?}"
    );
    let program = compute(&out);
    assert_eq!(run(&program, &delivered(LIVRE_NFD)), json!(42), "{program}");
}

/// « Chờ » as a request may state it, fully decomposed (o, then the horn U+031B and the grave
/// U+0300, two marks), and as a file may spell it, composed (U+1EDD): canonically equivalent.
const CHO_STATED: &str = "Cho\u{31b}\u{300}";
const CHO_OBSERVED: &str = "Ch\u{1edd}";

/// A fresh composition behind the same error (R4 A11, B21 T1): the clause states « Chờ » with two
/// combining marks, the file spells it composed, and the program stops with an error on its
/// unmatched probe. The observed value « en attente » reads its treatment of a value the clause
/// does not state: refused naming both spellings, repaired, 42.
#[tokio::test]
async fn a_decomposed_literal_behind_an_unmatched_probe_error_is_refused() {
    let clause = format!("sum qty over the rows where status is {CHO_STATED}");
    let both = format!(
        ".records | map(select(.status == {} or .status == {}) | .qty | tonumber) | add // 0",
        json!(CHO_STATED),
        json!(CHO_OBSERVED)
    );
    let example = [CHO_STATED, "en attente"];
    let seat = Seat::new(vec![
        request_of(&clause).1.to_string(),
        program_on(&erring(NO_ASCII_LETTER, CHO_STATED), example, &json!(40)),
        program_on(&both, example, &json!(40)),
    ]);
    let world = spelled(&[CHO_OBSERVED, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &clause, world, 1).await;
    assert_eq!(
        authored(&out),
        ["plan", "transform", "transform_repair"],
        "{out:#?}"
    );
    let refused = told(&seat, 2);
    for part in [&points(CHO_STATED), &points(CHO_OBSERVED)] {
        assert!(refused.contains(part.as_str()), "{part}: {refused}");
    }
    let program = compute(&out);
    assert_eq!(
        run(&program, &delivered(CHO_OBSERVED)),
        json!(42),
        "{program}"
    );
}

/// A program stopping with an error on U+2400 and on every value the host observed that the clause
/// does not state (a status holding a space, as « en attente » does), never on its own example.
fn unjudgeable() -> String {
    let jq = erring(
        &format!("{NO_ASCII_LETTER} or (.status | split(\" \") | length) > 1"),
        LIVRE_NFC,
    );
    program_on(&jq, [LIVRE_NFC, "pr\u{ea}t"], &json!(40))
}

/// The notes a verifier question carried in its STATE on a program the law could not judge.
fn notes(state: &Value) -> Vec<Value> {
    state["unjudged_spellings"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// A program whose treatment of every value the clause does not state is an error cannot be judged
/// by the spelling law (R4 A11, B21 T1): it is not refused on that ground alone, and it is not
/// READY silently. Its record and an applied finding say the law could not judge it, naming the
/// column, both spellings and their code points; the note rides the STATE of every verifier
/// question, so the clause and the whole request go to the judges with it. The approving double
/// makes it READY (a double certifies nothing); the refusing double keeps it INCOMPLETE. With no
/// observed value the clause does not state, U+2400 alone was tried, and the note says so.
#[tokio::test]
async fn a_program_the_law_cannot_judge_goes_to_the_judges_with_its_record() {
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let approving = Seat::new(vec![request_of(&livre()).1.to_string(), unjudgeable()]);
    let out = compiled(&approving, &livre(), world.clone(), 1).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
    let decision = out.provenance.decision.clone().unwrap();
    let record = &decision["transforms"][0];
    assert_eq!(record["accepted"], json!(true), "{record:#}");
    let note = record["unjudged"][0].clone();
    assert_eq!(note["column"], json!("status"), "{record:#}");
    assert_eq!(note["stated_code_points"], json!(points(LIVRE_NFC)));
    assert_eq!(note["observed_code_points"], json!(points(LIVRE_NFD)));
    assert_eq!(note["tried"], json!(["\u{2400}", "en attente"]), "{note:#}");
    let finding = out
        .diagnostics
        .iter()
        .find(|d| d.target == "authoring_transform" && d.message.contains("could not judge"));
    assert!(finding.is_some(), "{out:#?}");
    let judged = approving.judged();
    assert!(judged.len() >= 2, "{judged:#?}");
    for state in &judged {
        assert_eq!(notes(state), std::slice::from_ref(&note), "{state:#}");
    }
    let refusing = Seat::refusing(vec![request_of(&livre()).1.to_string(), unjudgeable()]);
    let refused = compiled(&refusing, &livre(), world, 0).await;
    assert_eq!(refused.status, CompileStatus::Incomplete, "{refused:#?}");
    assert!(
        refusing
            .judged()
            .iter()
            .all(|state| !notes(state).is_empty())
    );
    let alone = Seat::new(vec![request_of(&livre()).1.to_string(), unjudgeable()]);
    let only = spelled(&[LIVRE_NFD], &["x", "y"]);
    let out = compiled(&alone, &livre(), only, 1).await;
    let record = &out.provenance.decision.as_ref().unwrap()["transforms"][0];
    assert_eq!(
        record["unjudged"][0]["tried"],
        json!(["\u{2400}"]),
        "{record:#}"
    );
}

/// The field-answer regeneration is held to the same law and the same record (R4 A11, B21 T1): the
/// program regenerated once the human answered `qty` cannot be judged by the spelling law, so its
/// note rides the regeneration record and the STATE of the judges of its first candidate.
#[tokio::test]
async fn a_regenerated_program_the_law_cannot_judge_goes_to_the_judges_with_its_record() {
    let unread = format!(
        ".records | map(select(.status == {}) | .quantity | tonumber) | add // 0",
        json!(LIVRE_NFC)
    );
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let first = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        program(&unread, &["status", "quantity"], &json!(40)),
    ]);
    let pending = compiled(&first, &livre(), world.clone(), 1).await;
    let record = pending.provenance.plan.clone().unwrap();
    let key = pending
        .questions
        .iter()
        .find(|q| q.key.starts_with("const.rule_field"))
        .map(|q| q.key.clone())
        .unwrap();
    let seat = Seat::new(vec![unjudgeable()]);
    let policy =
        AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(1);
    let request = CompileRequest::create(request_of(&livre()).0)
        .with_plan(record)
        .with_knowledge(world)
        .with_authoring_policy(policy)
        .answer(&key, "\"qty\"");
    let out = compile_with_provider(&request, &seat).await.unwrap();
    let regeneration = &out.provenance.decision.as_ref().unwrap()["transform_regeneration"];
    assert_eq!(regeneration["accepted"], json!(true), "{out:#?}");
    assert_eq!(
        regeneration["unjudged"][0]["column"],
        json!("status"),
        "{regeneration:#}"
    );
    let judged = seat.judged();
    assert!(!judged.is_empty(), "{out:#?}");
    for state in &judged {
        assert_eq!(notes(state).len(), 1, "{state:#}");
    }
}

/// The request of the sum with each kept status also transformed as `suffix` states, compiled with
/// no repair granted over a status observed as e + U+0301; the seat's program keeps the rows of
/// `spellings` and applies `transform` to each kept status, as the request asks.
async fn requested(
    suffix: &str,
    spellings: &[&str],
    transform: &str,
    kept: &Value,
) -> CompileOutcome {
    let clause = format!("{}{suffix}", livre());
    let test: Vec<String> = spellings
        .iter()
        .map(|s| format!(".status == {}", json!(s)))
        .collect();
    let jq = format!(
        ".records | map(select({})) | {{kept: map(.status | {transform}), total: (map(.qty | tonumber) | add // 0)}}",
        test.join(" or ")
    );
    let answer = program(
        &jq,
        &["status", "qty"],
        &json!({"kept": [kept], "total": 40}),
    );
    let seat = Seat::new(vec![request_of(&clause).1.to_string(), answer]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    compiled(&seat, &clause, world, 0).await
}

/// The valid intents the law must not erase: each request asks for a text or byte transformation
/// of the kept status (a suffix label, ASCII uppercase, the code-point length, the percent
/// encoding of its UTF-8 bytes), which answers differently for the two spellings by its very
/// definition. The program keeps both spellings and transforms as asked: it is admitted with no
/// repair, and on rows carrying the observed spelling it keeps them and sums them.
const INTENTS: [(&str, &str, &str, &str); 4] = [
    (
        " and list each kept status with the suffix rows",
        ". + \" rows\"",
        "livr\u{e9} rows",
        "livre\u{301} rows",
    ),
    (
        " and list each kept status in ASCII uppercase",
        "ascii_upcase",
        "LIVR\u{e9}",
        "LIVRE\u{301}",
    ),
    (
        " and list each kept status percent-encoded",
        "@uri",
        "livr%C3%A9",
        "livre%CC%81",
    ),
    (
        " and list the code-point length of each kept status",
        "length",
        "5",
        "6",
    ),
];

/// The kept value of an intent's expected text: the code-point length is a number.
fn kept(text: &str) -> Value {
    text.parse::<u64>()
        .map_or_else(|_| json!(text), |n| json!(n))
}

#[tokio::test]
async fn a_requested_transformation_of_the_value_is_admitted() {
    let mut refused = Vec::new();
    for (suffix, transform, stated, observed) in INTENTS {
        let both = [LIVRE_NFC, LIVRE_NFD];
        let out = requested(suffix, &both, transform, &kept(stated)).await;
        if out.status != CompileStatus::Ready {
            let why: Vec<&str> = out
                .diagnostics
                .iter()
                .filter(|d| d.target == "authoring_transform")
                .map(|d| d.message.as_str())
                .collect();
            refused.push(format!("{suffix}: {:?} {why:?}", out.status));
            continue;
        }
        assert_eq!(authored(&out), ["plan", "transform"], "{suffix}");
        let program = compute(&out);
        let result = run(&program, &delivered(LIVRE_NFD));
        assert_eq!(result["total"], json!(42), "{suffix}: {program}");
        assert_eq!(
            result["kept"],
            json!([kept(observed), kept(observed)]),
            "{suffix}"
        );
    }
    assert!(refused.is_empty(), "{refused:#?}");
}

/// Control: the same requested transformations over a program that keeps only the stated
/// spelling drop the rows the source spells the other way: each is refused naming both
/// spellings, and with no repair granted the request stays INCOMPLETE.
#[tokio::test]
async fn a_requested_transformation_dropping_the_observed_spelling_is_refused() {
    for (suffix, transform, stated, _) in INTENTS {
        let out = requested(suffix, &[LIVRE_NFC], transform, &kept(stated)).await;
        assert_eq!(out.status, CompileStatus::Incomplete, "{suffix}: {out:#?}");
        assert!(
            out.diagnostics.iter().any(
                |d| d.target == "authoring_transform" && d.message.contains(&points(LIVRE_NFD))
            ),
            "{suffix}: {out:#?}"
        );
    }
}

/// A program reading the bound column without declaring it (a bracket read, its `columns_read`
/// naming only `qty`) is held to the law all the same: every bound column is probed, whatever the
/// seat declares. Comparing bytes there drops the observed spelling: it is refused, repaired from
/// that defect, and the repaired program sums the delivered rows.
#[tokio::test]
async fn an_undeclared_read_of_a_bound_column_is_probed() {
    let jq = format!(
        ".records | map(select(.[\"status\"] == {}) | .qty | tonumber) | add // 0",
        json!(LIVRE_NFC)
    );
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        program(&jq, &["qty"], &json!(40)),
        summing(&[LIVRE_NFC, LIVRE_NFD]),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    assert_eq!(
        authored(&out),
        ["plan", "transform", "transform_repair"],
        "{out:#?}"
    );
    let program = compute(&out);
    assert_eq!(run(&program, &delivered(LIVRE_NFD)), json!(42), "{program}");
}

/// A program that truly does not use the bound column (it sums every row, reading only `qty`) is
/// admitted as it is: the probes move none of its outputs. Whether it answers the request is the
/// verifier's to judge, never the spelling law's.
#[tokio::test]
async fn a_program_not_using_the_bound_column_is_admitted() {
    let jq = ".records | map(.qty | tonumber) | add // 0";
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        program(jq, &["qty"], &json!(55)),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
}

/// An undeclared read that treats both spellings alike is admitted as it is: the law refuses a
/// dropped spelling, never an undeclared column as such.
#[tokio::test]
async fn an_undeclared_read_honoring_both_spellings_is_admitted() {
    let jq = format!(
        ".records | map(select(.[\"status\"] == {} or .[\"status\"] == {}) | .qty | tonumber) | add // 0",
        json!(LIVRE_NFC),
        json!(LIVRE_NFD)
    );
    let seat = Seat::new(vec![
        request_of(&livre()).1.to_string(),
        program(&jq, &["qty"], &json!(40)),
    ]);
    let world = spelled(&[LIVRE_NFD, "en attente"], &["x", "y"]);
    let out = compiled(&seat, &livre(), world, 1).await;
    let program = compute(&out);
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
    assert_eq!(run(&program, &delivered(LIVRE_NFD)), json!(42), "{program}");
}

/// The binding and the law, as the core states them: a literal at exact token boundaries only,
/// never inside another word nor a column name, never a byte-identical spelling, and no case or
/// compatibility (NFKC) folding.
#[test]
fn the_binding_is_a_stated_token_and_the_law_is_canonical_equivalence_only() {
    let nfd = [LIVRE_NFD.to_owned()];
    let nfc = [LIVRE_NFC.to_owned()];
    let pair = |stated: &str, observed: &str| vec![(stated.to_owned(), observed.to_owned())];
    let stated = |clause: &str, observed: &[String]| stated_spellings(clause, observed, &[]);
    assert_eq!(
        stated(&format!("status is {LIVRE_NFC}."), &nfd),
        pair(LIVRE_NFC, LIVRE_NFD)
    );
    assert_eq!(
        stated(&format!("status is {LIVRE_NFD}"), &nfc),
        pair(LIVRE_NFD, LIVRE_NFC)
    );
    assert!(stated(&format!("status is {LIVRE_NFC}e"), &nfd).is_empty());
    assert!(stated(&format!("status is {LIVRE_NFC}"), &nfc).is_empty());
    assert!(stated("status is LIVR\u{c9}", &nfd).is_empty());
    let column = stated_spellings(&format!("sum the {LIVRE_NFC} column"), &nfd, &nfc);
    assert!(column.is_empty(), "{column:?}");
    assert!(stated("status is fine", &["\u{fb01}ne".to_owned()]).is_empty());
    let observed = [
        LIVRE_NFD.to_owned(),
        LIVRE_NFC.to_owned(),
        "LIVR\u{c9}".to_owned(),
    ];
    assert_eq!(
        equivalent_spellings(LIVRE_NFC, &observed),
        [LIVRE_NFD.to_owned()]
    );
}
