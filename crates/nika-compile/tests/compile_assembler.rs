// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The deterministic assembler on fresh-intent plans: a compiled workflow must produce the
//! artefact the intent asked for. Every case is a plan an authoring model actually proposed
//! for a fresh intent (2026-09-20 clean-shell gate), fed through a hermetic provider double;
//! the assertions read the emitted candidate, never a model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, TriggerKind,
    TriggerStatus, compile_with_provider, outcome_document,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::time::Duration;

struct Provider(String);
impl ProviderInferDyn for Provider {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: self.0.clone(),
            }],
            TokenUsage::new(100, 100),
            StopReason::EndTurn,
        ))
    }
}

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2))
}

async fn compile(intent: &str, plan: &Value, answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(intent).with_authoring_policy(policy());
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    compile_with_provider(&request, &Provider(plan.to_string()))
        .await
        .unwrap()
}

/// The answer-round door the CLI control uses: a trusted recorded plan replayed for its
/// intent, zero provider calls.
fn replay(intent: &str, record: &Value, answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(intent).with_plan(record.clone());
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    nika_compile::compile(&request).unwrap()
}

fn keys(out: &CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

fn document(out: &CompileOutcome) -> Value {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.check_preview.as_ref().unwrap().report.is_clean(),
        "{out:#?}"
    );
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

fn label(out: &CompileOutcome, key: &str) -> String {
    match out.questions.iter().find(|q| q.key == key) {
        Some(question) => question.label.clone(),
        None => panic!("no question {key}: {out:#?}"),
    }
}

fn tasks(doc: &Value) -> &serde_json::Map<String, Value> {
    doc["tasks"].as_object().unwrap()
}

// ── the fresh intents and the plans the authoring seat proposed for them ──────────

const STOCK: &str = "Lee el archivo ./inventario/stock.json, encuentra los productos con menos de 10 unidades y escribe una lista de reposición en ./salida/reposicion.md agrupada por proveedor. Si algún producto tiene 0 unidades, márcalo como URGENTE al principio de su línea.";
fn stock_plan() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./inventario/stock.json","evidence":"Lee el archivo ./inventario/stock.json"},
        {"op":"extract","detail":"productos con menos de 10 unidades","evidence":"encuentra los productos con menos de 10 unidades"},
        {"op":"draft","detail":"lista de reposición agrupada por proveedor","evidence":"escribe una lista de reposición en ./salida/reposicion.md agrupada por proveedor"}],
      "effects":[{"verb":"write","target":"./salida/reposicion.md","policy":"automatic","evidence":"escribe una lista de reposición en ./salida/reposicion.md agrupada por proveedor"}],
      "obligations":[],"constraints":["Si algún producto tiene 0 unidades, márcalo como URGENTE al principio de su línea"],"unknowns":[]})
}

const RFC: &str = "Fetch https://www.rfc-editor.org/rfc/rfc2324.txt and pull out the numbered section titles. Then write a plain-English brief of under 150 words explaining what the protocol does and why it is a joke, as 5 bullets, to ./out/rfc2324-brief.md.";
fn rfc_plan() -> Value {
    json!({"steps":[
        {"op":"fetch","detail":"https://www.rfc-editor.org/rfc/rfc2324.txt","evidence":"Fetch https://www.rfc-editor.org/rfc/rfc2324.txt and pull out the numbered section titles"}],
      "effects":[{"verb":"write","target":"./out/rfc2324-brief.md","policy":"automatic","evidence":"Then write a plain-English brief of under 150 words explaining what the protocol does and why it is a joke, as 5 bullets, to ./out/rfc2324-brief.md"}],
      "obligations":[],"constraints":[],"unknowns":[]})
}

const CATALOG: &str = "For each of these four product slugs - solar-lamp, wind-chime, rain-barrel, compost-bin - read ./catalog/<slug>.md and draft one two-sentence marketing blurb in a warm, down-to-earth tone. Process at most 2 products at a time, then merge all four blurbs in the listed order into a single ./out/catalog-blurbs.md with the product name as a heading above each blurb.";
fn catalog_plan() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./catalog/solar-lamp.md ; ./catalog/wind-chime.md ; ./catalog/rain-barrel.md ; ./catalog/compost-bin.md","evidence":"read ./catalog/<slug>.md"},
        {"op":"draft","detail":"one two-sentence marketing blurb in a warm, down-to-earth tone for solar-lamp ; one two-sentence marketing blurb in a warm, down-to-earth tone for wind-chime ; one two-sentence marketing blurb in a warm, down-to-earth tone for rain-barrel ; one two-sentence marketing blurb in a warm, down-to-earth tone for compost-bin","evidence":"draft one two-sentence marketing blurb in a warm, down-to-earth tone"}],
      "effects":[
        {"verb":"merge","target":"all four blurbs in the listed order into a single ./out/catalog-blurbs.md with the product name as a heading above each blurb","policy":"automatic","evidence":"merge all four blurbs in the listed order into a single ./out/catalog-blurbs.md with the product name as a heading above each blurb"},
        {"verb":"write","target":"./out/catalog-blurbs.md","policy":"automatic","evidence":"then merge all four blurbs in the listed order into a single ./out/catalog-blurbs.md with the product name as a heading above each blurb"}],
      "obligations":[],"constraints":["Process at most 2 products at a time"],"unknowns":[]})
}

const ORDERS: &str = "Read the CSV at ./data/orders-2026-09.csv and keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON to ./out/shipped-by-country.json, then write a short Markdown note naming the top 3 countries to ./out/summary.md.";
/// The xai proposal: the JSON write folded into the compute step, one write effect.
fn orders_plan() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./data/orders-2026-09.csv","evidence":"Read the CSV at ./data/orders-2026-09.csv"},
        {"op":"compute","detail":"keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON","evidence":"keep only the rows whose status is \"shipped\" and whose total_eur is above 120"},
        {"op":"draft","detail":"short Markdown note naming the top 3 countries","evidence":"write a short Markdown note naming the top 3 countries"}],
      "effects":[{"verb":"write","target":"./out/summary.md","policy":"automatic","evidence":"write a short Markdown note naming the top 3 countries to ./out/summary.md"}],
      "obligations":[],"constraints":[],"unknowns":[]})
}
/// The gemini proposal: the write target is the whole prose phrase.
fn orders_prose_plan() -> Value {
    let mut plan = orders_plan();
    plan["effects"][0]["target"] =
        json!("a short Markdown note naming the top 3 countries to ./out/summary.md");
    plan["steps"][0]["detail"] = json!("CSV at ./data/orders-2026-09.csv");
    plan
}

const NOTES: &str = "bon alors jai un dossier ./notes avec plein de fichiers .md de reunion de la semaine faut que tu me fasse un resumé de chaque en 3 lignes max et que tu me mette tout ca dans ./out/recap-semaine.md avec le nom du fichier en titre stp pas de blabla juste les decisions et les trucs a faire merci";
fn notes_plan() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./notes avec plein de fichiers .md","evidence":"jai un dossier ./notes avec plein de fichiers .md de reunion de la semaine"},
        {"op":"draft","detail":"resumé de chaque en 3 lignes max","evidence":"faut que tu me fasse un resumé de chaque en 3 lignes max"}],
      "effects":[{"verb":"write","target":"./out/recap-semaine.md","policy":"automatic","evidence":"que tu me mette tout ca dans ./out/recap-semaine.md"}],
      "obligations":[],"constraints":["3 lignes max","pas de blabla juste les decisions et les trucs a faire"],"unknowns":[]})
}

const BIG_ORDERS: &str = "Read ./data/orders.csv, keep only the rows whose amount is strictly greater than 100, and write those rows to ./out/big_orders.csv. Then write ./out/summary.md with one line stating how many rows were kept and the total of their amounts.";
/// The trusted plan of the fidelity control (case a): the threshold is a compute step.
fn big_orders_plan() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./data/orders.csv","evidence":"Read ./data/orders.csv"},
        {"op":"compute","detail":"keep only the rows whose amount is strictly greater than 100","evidence":"keep only the rows whose amount is strictly greater than 100"},
        {"op":"draft","detail":"one line stating how many rows were kept and the total of their amounts","evidence":"one line stating how many rows were kept and the total of their amounts"}],
      "effects":[
        {"verb":"write","target":"./out/big_orders.csv","policy":"automatic","evidence":"write those rows to ./out/big_orders.csv"},
        {"verb":"write","target":"./out/summary.md","policy":"automatic","evidence":"write ./out/summary.md with one line stating how many rows were kept and the total of their amounts"}],
      "obligations":[],"constraints":[],"unknowns":[]})
}
/// The same proposal with the threshold demoted to a prompt instruction.
fn demoted_plan() -> Value {
    let mut plan = big_orders_plan();
    plan["steps"].as_array_mut().unwrap().remove(1);
    plan["constraints"] = json!(["keep only the rows whose amount is strictly greater than 100"]);
    plan
}
const RULE: (&str, &str) = (
    "const.rule_expression",
    r#"".records | map(select((.amount | tonumber) > 100))""#,
);

const MODEL: (&str, &str) = ("model", r#""mock/echo""#);

const CHAPTERS: &str = "For each of the four files ./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md, at most 2 at a time, write a two-sentence summary. Then merge the summaries in exactly that order into ./out/digest.md, with one heading per file named after the file.";
const CHAPTER_FILES: [&str; 4] = [
    "./chapters/01-intro.md",
    "./chapters/02-method.md",
    "./chapters/03-results.md",
    "./chapters/04-limits.md",
];
/// The trusted recorded plan of the fidelity control (case c).
fn chapters_record() -> Value {
    let mut bindings: Vec<Value> = CHAPTER_FILES
        .iter()
        .map(|p| json!({"role": "path", "literal": p}))
        .collect();
    bindings.push(json!({"role": "path", "literal": "./out/digest.md"}));
    json!({"operations":[
        {"op":"read","detail":CHAPTER_FILES.join(" ; "),"evidence":"./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md","categories":[]},
        {"op":"draft","detail":"a two-sentence summary","evidence":"For each of the four files ./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md, at most 2 at a time, write a two-sentence summary","categories":[]}],
      "effects":[{"verb":"write","target":"./out/digest.md","policy":"automatic","evidence":"merge the summaries in exactly that order into ./out/digest.md, with one heading per file named after the file","policy_literal":null}],
      "obligations":[],"bindings":bindings,
      "constraints":["at most 2 at a time","in exactly that order","with one heading per file named after the file"],
      "unknowns":[],"trigger":"For each of the four files","strategy":"cold"})
}

const TICKET: &str = "Look up ticket T-4471 in ./data/tickets.json, classify it as billing, shipping or other, and draft a reply to the customer. If it is a refund request, a human must approve before the refund amount is posted to http://127.0.0.1:18471/refunds and the reply is sent to http://127.0.0.1:18471/replies.";
/// The trusted recorded plan of the fidelity control (case d).
fn ticket_record() -> Value {
    json!({"operations":[
        {"op":"lookup","detail":"ticket T-4471 in ./data/tickets.json","evidence":"Look up ticket T-4471 in ./data/tickets.json","categories":[]},
        {"op":"classify","detail":"it","evidence":"classify it as billing, shipping or other","categories":["billing","shipping","other"]},
        {"op":"draft","detail":"a reply to the customer","evidence":"draft a reply to the customer","categories":[]}],
      "effects":[
        {"verb":"refund","target":"the refund amount is posted to http://127.0.0.1:18471/refunds","policy":"human_first","evidence":"a human must approve before the refund amount is posted to http://127.0.0.1:18471/refunds","policy_literal":null},
        {"verb":"send","target":"the reply is sent to http://127.0.0.1:18471/replies","policy":"human_first","evidence":"the reply is sent to http://127.0.0.1:18471/replies","policy_literal":null}],
      "obligations":[],
      "bindings":[{"role":"path","literal":"./data/tickets.json"},{"role":"url","literal":"http://127.0.0.1:18471/refunds"},{"role":"url","literal":"http://127.0.0.1:18471/replies"}],
      "constraints":["If it is a refund request, a human must approve before the refund amount is posted to http://127.0.0.1:18471/refunds and the reply is sent to http://127.0.0.1:18471/replies"],
      "unknowns":[],"trigger":null,"strategy":"cold"})
}
const TICKET_ANSWERS: [(&str, &str); 4] = [
    MODEL,
    (
        "const.refund_endpoint",
        r#""http://127.0.0.1:18471/refunds""#,
    ),
    ("const.send_endpoint", r#""http://127.0.0.1:18471/replies""#),
    (
        "const.refund_policy",
        r#"{"cap":100,"currency":"EUR","eligibility":"duplicate charge or damaged item reported within 30 days"}"#,
    ),
];

// ── a lookup by identifier selects one record, never the whole file ─────────────
// The control (case d) asked a directory question for a file the request already named,
// declared phantom `inputs.item` and `inputs.record_id`, then died on
// `cannot index [array] with "T-4471"`. A lookup detail naming one JSON file and an
// identifier binds the file, keeps the identifier as a constant, asks only which field
// holds it, and selects the one record so later steps see the record alone.
#[test]
fn a_lookup_by_identifier_in_a_json_file_selects_the_one_record() {
    let asked = replay(TICKET, &ticket_record(), &TICKET_ANSWERS);
    assert_eq!(keys(&asked), ["const.ticket_id_field"], "{asked:#?}");
    let text = label(&asked, "const.ticket_id_field");
    assert!(text.contains("./data/tickets.json"), "{text}");
    assert!(text.contains("T-4471"), "{text}");
    let mut answers = TICKET_ANSWERS.to_vec();
    answers.push(("const.ticket_id_field", r#""id""#));
    let out = replay(TICKET, &ticket_record(), &answers);
    let doc = document(&out);
    assert_eq!(
        doc["const"]["ticket_directory"], "./data/tickets.json",
        "{doc:#}"
    );
    assert_eq!(doc["const"]["ticket_id"], "T-4471");
    assert_eq!(doc["const"]["ticket_id_field"], "id");
    assert!(doc["const"].get("ticket_t_4471_directory").is_none());
    assert!(
        doc.get("inputs").is_none(),
        "a literal lookup is the corpus: no item, no record_id: {doc:#}"
    );
    assert_eq!(doc["permits"]["fs"]["read"], json!(["./data/tickets.json"]));
    let record = &tasks(&doc)["lookup_record"];
    let input = &record["invoke"]["args"]["input"];
    assert_eq!(input["id"], "${{ const.ticket_id }}");
    assert_eq!(input["field"], "${{ const.ticket_id_field }}");
    let expression = record["invoke"]["args"]["expression"].as_str().unwrap();
    assert!(expression.contains(r#"if type == "array""#), "{expression}");
    assert!(expression.contains(".[$l.field] == $l.id"), "{expression}");
    // Classification, the draft and the payloads see the record, never the file.
    let classify = tasks(&doc)["classify"]["infer"]["prompt"].as_str().unwrap();
    assert!(
        classify.contains("record: ${{ with.record }}"),
        "{classify}"
    );
    assert!(!classify.contains("Item:"), "{classify}");
    let payload = &tasks(&doc)["refund_payload"]["invoke"]["args"]["input"];
    assert!(payload.get("record").is_some(), "{payload:#}");
    assert!(payload.get("item").is_none(), "{payload:#}");
    assert!(!out.candidate.as_deref().unwrap().contains("inputs."));
    // The gates still dominate both POSTs.
    assert_eq!(
        tasks(&doc)["refund"]["when"],
        "${{ with.approved == true }}"
    );
    assert_eq!(tasks(&doc)["send"]["when"], "${{ with.approved == true }}");
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["shape"]["word"],
        "human_gated"
    );
}

fn operations(out: &CompileOutcome, op: &str) -> Vec<Value> {
    out.provenance.plan.as_ref().unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["op"] == op)
        .cloned()
        .collect()
}

// ── a numeric rule is an operation, never prompt guidance ───────────────────────
// The control (2026-09-20, case a) showed a model reading "strictly greater than 100" as
// a constraint: the draft was then asked to filter rows in prose. A digit beside a
// comparison cue is a code rule; the compiler promotes it to a compute stage anchored in
// the request, right after the sources, so the draft sees the computed rows.
#[tokio::test]
async fn a_numeric_filter_demoted_to_a_constraint_is_promoted_to_a_compute_stage() {
    let out = compile(BIG_ORDERS, &demoted_plan(), &[MODEL]).await;
    // The promoted rule states its threshold in words: no rule question is asked.
    assert_eq!(keys(&out), Vec::<&str>::new(), "{out:#?}");
    let ops: Vec<&str> = out.provenance.plan.as_ref().unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["read", "compute", "draft"], "{out:#?}");
    let compute = &operations(&out, "compute")[0];
    assert_eq!(
        compute["evidence"],
        "keep only the rows whose amount is strictly greater than 100"
    );
    assert_eq!(
        out.provenance.plan.as_ref().unwrap()["constraints"],
        json!([]),
        "the promoted rule leaves the prompt guidance"
    );
    let doc = document(&out);
    let compute = &tasks(&doc)["compute"];
    assert_eq!(compute["invoke"]["tool"], "nika:jq");
    assert_eq!(
        compute["with"]["records"],
        "${{ tasks.parse_source.output }}"
    );
    let summary = &tasks(&doc)["compute_summary"];
    assert_eq!(summary["invoke"]["tool"], "nika:jq", "{doc:#}");
    assert_eq!(summary["with"]["computed"], "${{ tasks.compute.output }}");
    let expression = summary["invoke"]["args"]["expression"].as_str().unwrap();
    assert!(expression.contains("count:"), "{expression}");
    assert!(expression.contains("totals:"), "{expression}");
    let draft = &tasks(&doc)["draft"];
    assert_eq!(draft["with"]["computed"], "${{ tasks.compute.output }}");
    assert_eq!(
        draft["with"]["summary"],
        "${{ tasks.compute_summary.output }}"
    );
    let prompt = draft["infer"]["prompt"].as_str().unwrap();
    assert!(prompt.contains("summary: ${{ with.summary }}"), "{prompt}");
    assert!(
        !prompt.contains("Instruction from the requester: keep only"),
        "{prompt}"
    );
    // Idempotent: a plan that already carries the compute step gains no second one.
    let again = compile(BIG_ORDERS, &big_orders_plan(), &[MODEL, RULE]).await;
    let computes = operations(&again, "compute");
    assert_eq!(computes.len(), 1, "{computes:#?}");
    assert_eq!(
        computes[0]["detail"],
        "keep only the rows whose amount is strictly greater than 100"
    );
}

// ── a rule the request states in words is the jq the workflow runs ──────────────
// The tournament (2026-09-20, 40 sealed seeds) asked `const.rule_expression` for rules
// the intent already stated (13 of 40). A numeric or equality rule over a parsed source
// is synthesized deterministically: a guard asserts the referenced columns exist on the
// first record, the filter runs as code, and the question is not asked.
#[tokio::test]
async fn a_numeric_rule_stated_in_the_request_needs_no_rule_question() {
    let out = compile(BIG_ORDERS, &big_orders_plan(), &[MODEL]).await;
    assert_eq!(keys(&out), Vec::<&str>::new(), "{out:#?}");
    let doc = document(&out);
    assert!(doc["const"].get("rule_expression").is_none(), "{doc:#}");
    let compute = &tasks(&doc)["compute"];
    assert_eq!(compute["invoke"]["tool"], "nika:jq");
    assert_eq!(
        compute["invoke"]["args"]["expression"],
        "[.records[] | select((.amount | tonumber) > 100)]",
        "{doc:#}"
    );
    assert_eq!(
        compute["invoke"]["args"]["input"],
        json!({"records": "${{ with.records }}"})
    );
    assert_eq!(
        compute["with"]["records"],
        "${{ tasks.parse_source.output }}"
    );
    assert_eq!(compute["after"], json!({"compute_admit": "success"}));
    let guard = &tasks(&doc)["compute_guard"];
    assert_eq!(guard["invoke"]["tool"], "nika:jq", "{doc:#}");
    let expression = guard["invoke"]["args"]["expression"].as_str().unwrap();
    assert!(expression.contains(r#"has("amount")"#), "{expression}");
    assert!(expression.contains("length) == 0 or"), "{expression}");
    let admit = &tasks(&doc)["compute_admit"];
    assert_eq!(admit["invoke"]["tool"], "nika:assert");
    assert_eq!(admit["invoke"]["args"]["condition"], "${{ with.ok }}");
    assert_eq!(admit["with"]["ok"], "${{ tasks.compute_guard.output }}");
    let message = admit["invoke"]["args"]["message"].as_str().unwrap();
    assert!(message.contains("`amount`"), "{message}");
    // The summary and the CSV conversion keep reading the filtered rows.
    assert_eq!(
        tasks(&doc)["compute_summary"]["with"]["computed"],
        "${{ tasks.compute.output }}"
    );
    assert_eq!(
        tasks(&doc)["big_orders_csv"]["with"]["data"],
        "${{ tasks.compute.output }}"
    );
    let rule = &out.provenance.decision.as_ref().unwrap()["rule"];
    assert_eq!(rule["synthesized"], true, "{rule:#}");
    assert_eq!(
        rule["text"],
        "keep only the rows whose amount is strictly greater than 100"
    );
    assert_eq!(rule["fields"], json!(["amount"]));
    assert_eq!(rule["field"], "amount");
    assert_eq!(rule["comparator"], ">");
    assert_eq!(rule["value"], "100");
    assert_eq!(
        rule["jq"],
        "[.records[] | select((.amount | tonumber) > 100)]"
    );
    // An explicit answer still wins over the synthesis: the human's expression runs.
    let answered = compile(BIG_ORDERS, &big_orders_plan(), &[MODEL, RULE]).await;
    let doc = document(&answered);
    assert_eq!(
        tasks(&doc)["compute"]["invoke"]["args"]["expression"],
        "${{ const.rule_expression }}"
    );
    assert!(tasks(&doc).get("compute_guard").is_none(), "{doc:#}");
    assert!(
        answered.provenance.decision.as_ref().unwrap()["rule"].is_null(),
        "{answered:#?}"
    );
}

const REFUNDED: &str = "Read ./data/orders.csv (columns order_id,customer,amount,status), keep only the rows whose status is refunded, and write those rows to ./out/refunded.csv.";
/// A trusted recorded plan (the hot reader drops this filter; the record is the control).
fn filter_record(source: &str, rule: &str, target: &str) -> Value {
    json!({"operations":[
        {"op":"read","detail":source,"evidence":format!("Read {source}"),"categories":[]},
        {"op":"compute","detail":rule,"evidence":rule,"categories":[]}],
      "effects":[{"verb":"write","target":target,"policy":"automatic","evidence":format!("to {target}"),"policy_literal":null}],
      "obligations":[],
      "bindings":[{"role":"path","literal":source},{"role":"path","literal":target}],
      "constraints":[],"unknowns":[],"trigger":null,"strategy":"cold"})
}

#[test]
fn an_equality_rule_on_a_status_column_is_synthesized() {
    let record = filter_record(
        "./data/orders.csv",
        "keep only the rows whose status is refunded",
        "./out/refunded.csv",
    );
    let out = replay(REFUNDED, &record, &[]);
    assert_eq!(keys(&out), Vec::<&str>::new(), "{out:#?}");
    let doc = document(&out);
    assert_eq!(
        tasks(&doc)["compute"]["invoke"]["args"]["expression"],
        r#"[.records[] | select(.status == "refunded")]"#,
        "{doc:#}"
    );
    let guard = tasks(&doc)["compute_guard"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap();
    assert!(guard.contains(r#"has("status")"#), "{guard}");
    assert_eq!(
        tasks(&doc)["refunded_csv"]["with"]["data"],
        "${{ tasks.compute.output }}"
    );
    assert!(tasks(&doc).get("compute_summary").is_none(), "{doc:#}");
    let rule = &out.provenance.decision.as_ref().unwrap()["rule"];
    assert_eq!(rule["comparator"], "==", "{rule:#}");
    assert_eq!(rule["value"], "refunded");
    // Two clauses joined by `and`, a quoted value kept in its exact case.
    let intent = "Read ./data/orders.csv, keep only the rows whose status is \"Shipped\" and whose amount_eur is at least 120, and write those rows to ./out/shipped.csv.";
    let record = filter_record(
        "./data/orders.csv",
        "keep only the rows whose status is \"Shipped\" and whose amount_eur is at least 120",
        "./out/shipped.csv",
    );
    let out = replay(intent, &record, &[]);
    let doc = document(&out);
    assert_eq!(
        tasks(&doc)["compute"]["invoke"]["args"]["expression"],
        r#"[.records[] | select(.status == "Shipped" and (.amount_eur | tonumber) >= 120)]"#,
        "{doc:#}"
    );
    let rule = &out.provenance.decision.as_ref().unwrap()["rule"];
    assert_eq!(rule["fields"], json!(["status", "amount_eur"]), "{rule:#}");
    assert_eq!(rule["clauses"].as_array().unwrap().len(), 2);
    assert!(rule.get("field").is_none(), "{rule:#}");
}

const FOLDED: &str = "Read ./data/orders.csv (columns order_id,customer,amount,status), keep only the rows whose amount is strictly greater than 100, and write those rows to ./out/big.csv. Then write ./out/note.md with one line stating how many rows were kept and the total of their amounts.";

// The live authoring seat (xai/grok-3-mini-fast, 2026-09-20) folded the note's claims into
// the compute detail and proposed no draft step. The trailing count-and-total request is
// what the summary stage computes: the rule is still synthesized, the summary becomes an
// output, and no question is asked. The one-line note itself needs the draft the seat
// dropped; that is the reader's fidelity, not a rule question.
#[test]
fn a_count_and_total_folded_into_the_rule_is_the_summary_stage() {
    let mut record = filter_record(
        "./data/orders.csv",
        "keep only the rows whose amount is strictly greater than 100 and how many rows were kept and the total of their amounts",
        "./out/big.csv",
    );
    record["operations"][1]["evidence"] =
        json!("keep only the rows whose amount is strictly greater than 100");
    record["effects"].as_array_mut().unwrap().push(json!({"verb":"write","target":"./out/note.md","policy":"automatic","evidence":"write ./out/note.md with one line stating how many rows were kept and the total of their amounts","policy_literal":null}));
    record["bindings"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"path","literal":"./out/note.md"}));
    let out = replay(FOLDED, &record, &[]);
    assert_eq!(keys(&out), Vec::<&str>::new(), "{out:#?}");
    let doc = document(&out);
    assert_eq!(
        tasks(&doc)["compute"]["invoke"]["args"]["expression"],
        "[.records[] | select((.amount | tonumber) > 100)]",
        "{doc:#}"
    );
    assert_eq!(
        tasks(&doc)["compute_summary"]["with"]["computed"],
        "${{ tasks.compute.output }}",
        "{doc:#}"
    );
    assert_eq!(
        doc["outputs"]["summary"],
        "${{ tasks.compute_summary.output }}"
    );
    assert_eq!(
        tasks(&doc)["big_csv"]["with"]["data"],
        "${{ tasks.compute.output }}"
    );
    // The prose note takes the count-and-totals summary, the nearest result of its kind;
    // the one-line wording itself needs the draft step the seat dropped.
    assert_eq!(
        tasks(&doc)["write_note"]["with"]["content"],
        "${{ tasks.compute_summary.output }}",
        "{doc:#}"
    );
    let rule = &out.provenance.decision.as_ref().unwrap()["rule"];
    assert_eq!(rule["summary"], true, "{rule:#}");
    assert_eq!(
        rule["jq"],
        "[.records[] | select((.amount | tonumber) > 100)]"
    );
    // Without the fold, the summary stage is not emitted for a rule nothing later reads.
    let plain = filter_record(
        "./data/orders.csv",
        "keep only the rows whose amount is strictly greater than 100",
        "./out/big.csv",
    );
    let doc = document(&replay(FOLDED, &plain, &[]));
    assert!(tasks(&doc).get("compute_summary").is_none(), "{doc:#}");
}

#[tokio::test]
async fn an_unresolvable_rule_still_asks_for_the_expression() {
    // The compute detail carries a grouping the grammar does not cover: the question
    // stays, naming the parsed input shape.
    let out = compile(ORDERS, &orders_plan(), &[MODEL]).await;
    assert!(keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    assert!(
        label(&out, "const.rule_expression").contains("{document, records}"),
        "{out:#?}"
    );
    // No resolvable field: "units" names no column of the request.
    let intent = "Read ./data/stock.csv, keep only the products with fewer than 10 units, and write them to ./out/low.csv.";
    let record = filter_record(
        "./data/stock.csv",
        "keep only the products with fewer than 10 units",
        "./out/low.csv",
    );
    let out = replay(intent, &record, &[]);
    assert_eq!(keys(&out), ["const.rule_expression"], "{out:#?}");
    assert!(
        out.provenance
            .decision
            .as_ref()
            .is_none_or(|d| d["rule"].is_null()),
        "{out:#?}"
    );
}

// ── per-item work fans out and folds back with one heading per file ─────────────
// The control (case c) produced one aggregate draft over the folded corpus: no headings,
// chapters missing. A request that distributes its draft over the files is a for_each
// draft per item, a per-item law, and a fold in item order; the order and heading
// instructions are structure now, not prompt text.
#[test]
fn per_file_summaries_become_a_for_each_draft_folded_with_one_heading_per_file() {
    let out = replay(CHAPTERS, &chapters_record(), &[MODEL]);
    let doc = document(&out);
    assert_eq!(
        doc["const"]["source_paths"],
        json!(CHAPTER_FILES),
        "{doc:#}"
    );
    assert!(doc.get("inputs").is_none());
    let read = &tasks(&doc)["read_source"];
    assert_eq!(read["for_each"]["max_parallel"], 2, "{read:#}");
    let items = &tasks(&doc)["draft_items"];
    assert_eq!(items["invoke"]["tool"], "nika:jq", "{doc:#}");
    assert_eq!(items["with"]["texts"], "${{ tasks.read_source.output }}");
    assert_eq!(
        items["invoke"]["args"]["input"]["paths"],
        "${{ const.source_paths }}"
    );
    assert!(
        items["invoke"]["args"]["expression"]
            .as_str()
            .unwrap()
            .contains("{path: $r.paths[$i], text: $r.texts[$i]}")
    );
    assert!(
        tasks(&doc).get("documents").is_none(),
        "nothing else reads the whole corpus: {doc:#}"
    );
    let draft = &tasks(&doc)["draft"];
    assert_eq!(draft["for_each"]["items"], "${{ with.items }}", "{draft:#}");
    assert_eq!(draft["for_each"]["max_parallel"], 2);
    assert_eq!(draft["for_each"]["fail_fast"], true);
    assert_eq!(draft["with"]["items"], "${{ tasks.draft_items.output }}");
    let prompt = draft["infer"]["prompt"].as_str().unwrap();
    assert!(prompt.contains("${{ item.text }}"), "{prompt}");
    assert!(prompt.contains("a two-sentence summary"), "{prompt}");
    assert!(!prompt.contains("with.document"), "{prompt}");
    for structural in ["in exactly that order", "heading", "at most 2"] {
        assert!(!prompt.contains(structural), "{structural}: {prompt}");
    }
    let law = tasks(&doc)["draft_anchors"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap();
    assert!(law.contains("$r.items[$i].text"), "{law}");
    assert!(law.contains(r#"gsub("\\s+"; " ")"#), "{law}");
    assert!(
        law.contains("($r.drafts | length) == ($r.items | length)"),
        "{law}"
    );
    let fold = &tasks(&doc)["draft_fold"];
    assert_eq!(fold["with"]["drafts"], "${{ tasks.draft.output }}");
    let expression = fold["invoke"]["args"]["expression"].as_str().unwrap();
    assert!(
        expression.contains(r###""## \($r.items[$i].path | split("/") | last)"###),
        "{expression}"
    );
    let write = &tasks(&doc)["write_output"];
    assert_eq!(write["with"]["content"], "${{ tasks.draft_fold.output }}");
    assert_eq!(
        write["after"],
        json!({"draft_admit": "success"}),
        "{write:#}"
    );
    assert_eq!(doc["outputs"]["draft"], "${{ tasks.draft_fold.output }}");
    let shape = &out.provenance.decision.as_ref().unwrap()["shape"];
    assert_eq!(
        shape,
        &json!({"word": "fan_out_fan_in", "fan_out": true, "per_item": true, "outputs": 1, "gated": false}),
        "{shape:#}"
    );
}

#[test]
fn a_per_item_request_with_placeholder_outputs_is_refused_not_lowered() {
    let intent = "For each of the four files ./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md, write a two-sentence summary to ./out/<name>.md.";
    let record = json!({"operations":[
        {"op":"read","detail":CHAPTER_FILES.join(" ; "),"evidence":"./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md","categories":[]},
        {"op":"draft","detail":"a two-sentence summary","evidence":"For each of the four files ./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md, write a two-sentence summary to ./out/<name>.md","categories":[]}],
      "effects":[{"verb":"write","target":"./out/<name>.md","policy":"automatic","evidence":"write a two-sentence summary to ./out/<name>.md","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":"For each of the four files","strategy":"cold"});
    let out = replay(intent, &record, &[MODEL]);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        !keys(&out).contains(&"const.output_path"),
        "a placeholder is never lowered to one guessed path: {out:#?}"
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown && d.message.contains("one file per item")),
        "{out:#?}"
    );
}

// ── a body whose keys the request states is exactly those keys over produced values ─
// The sealed till seed computed `{tickets, total_cents}` correctly, then posted the generic
// `{action, target, facts}` envelope: run green, wrong body. A brace list in the effect's
// own words is the payload's shape; a key nothing produces is asked, never invented.
const TILL: &str = "Compute the day's sales total from ./till.csv (columns ticket,time,amount_cents): the number of tickets and the sum of amount_cents. Send that summary in one POST to http://127.0.0.1:18471/hooks/till with the JSON body {tickets, total_cents} and write the same object to ./out/till.json.";
const TILL_PLAIN: &str = "Compute the day's sales total from ./till.csv (columns ticket,time,amount_cents): the number of tickets and the sum of amount_cents. Send that summary in one POST to http://127.0.0.1:18471/hooks/till with the JSON body of the summary and write the same object to ./out/till.json.";
const TILL_CASHIER: &str = "Compute the day's sales total from ./till.csv (columns ticket,time,amount_cents): the number of tickets and the sum of amount_cents. Send that summary in one POST to http://127.0.0.1:18471/hooks/till with the JSON body {tickets, cashier} and write the same object to ./out/till.json.";
fn till_record(body: &str) -> Value {
    let compute = "the number of tickets and the sum of amount_cents";
    json!({"operations":[
        {"op":"read","detail":"./till.csv","evidence":"Compute the day's sales total from ./till.csv (columns ticket,time,amount_cents)","categories":[]},
        {"op":"compute","detail":compute,"evidence":compute,"categories":[]}],
      "effects":[
        {"verb":"send","target":format!("one POST to http://127.0.0.1:18471/hooks/till with the JSON body {body}"),"policy":"automatic","evidence":format!("Send that summary in one POST to http://127.0.0.1:18471/hooks/till with the JSON body {body}"),"policy_literal":null},
        {"verb":"write","target":"./out/till.json","policy":"automatic","evidence":"write the same object to ./out/till.json","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./till.csv"},{"role":"url","literal":"http://127.0.0.1:18471/hooks/till"},{"role":"path","literal":"./out/till.json"}],
      "constraints":[],"unknowns":[],"trigger":null,"strategy":"cold",
      "rules":[{"text":compute,"clauses":[],"junction":"and","summary":false,
                "shape":{"group_by":null,
                         "aggregations":[{"field":null,"op":"count","name":"tickets","round":null},
                                         {"field":"amount_cents","op":"sum","name":"total_cents","round":null}],
                         "sort_by":null,"descending":false,"columns":[],"derived":[]}}]})
}

#[test]
fn a_body_whose_keys_the_request_states_is_those_keys_over_produced_values() {
    let out = replay(TILL, &till_record("{tickets, total_cents}"), &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    assert_eq!(
        tasks(&doc)["send_payload"]["invoke"]["args"]["expression"],
        r#"{"tickets": .computed["tickets"], "total_cents": .computed["total_cents"]}"#,
        "{doc:#}"
    );
    assert_eq!(
        tasks(&doc)["send_payload"]["with"]["computed"],
        "${{ tasks.compute.output }}"
    );
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}"
    );
    assert_eq!(
        doc["outputs"]["total_cents"],
        "${{ tasks.compute.output.total_cents }}"
    );
    // No stated keys: the payload names the action, its target and every fact, as before.
    let out = replay(TILL_PLAIN, &till_record("of the summary"), &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let expression = document(&out)["tasks"]["send_payload"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        expression.starts_with(r#"{action: "send", target: "#) && expression.ends_with("facts: .}"),
        "{expression}"
    );
    // A key nothing produces is asked, never filled with an invented value.
    let out = replay(TILL_CASHIER, &till_record("{tickets, cashier}"), &[]);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown
                && d.target == "send"
                && d.message.contains("`cashier`")),
        "{out:#?}"
    );
    // One key left and one drafted text: the body the request named after its content.
    let intent = "Make a one-paragraph digest of ./notes.md and POST it to http://127.0.0.1:18471/hooks/digest with the JSON body {digest}.";
    let record = json!({"operations":[
        {"op":"read","detail":"./notes.md","evidence":"Make a one-paragraph digest of ./notes.md","categories":[]},
        {"op":"draft","detail":"a one-paragraph digest","evidence":"Make a one-paragraph digest of ./notes.md","categories":[]}],
      "effects":[{"verb":"send","target":"POST it to http://127.0.0.1:18471/hooks/digest with the JSON body {digest}","policy":"automatic","evidence":"POST it to http://127.0.0.1:18471/hooks/digest with the JSON body {digest}","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./notes.md"},{"role":"url","literal":"http://127.0.0.1:18471/hooks/digest"}],
      "constraints":[],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        document(&out)["tasks"]["send_payload"]["invoke"]["args"]["expression"],
        r#"{"digest": .draft}"#
    );
}

// ── the obligation ledger: every stated duty is carried, or nothing is READY ───────
// The ledger is the typed truth a product projects: one duty per stated demand with its
// kind, its state and the element carrying it. A READY candidate has no unresolved duty;
// a constraint no step can carry ends INCOMPLETE naming it, never a green run on a dropped
// instruction.
#[test]
fn the_ledger_names_the_carrier_of_every_stated_duty_and_refuses_a_silent_one() {
    let out = replay(CHAPTERS, &chapters_record(), &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let record = out.provenance.decision.clone().unwrap();
    let ledger = record["ledger"].as_array().unwrap();
    let duties: Vec<(&str, &str, &str)> = ledger
        .iter()
        .map(|d| {
            (
                d["kind"].as_str().unwrap(),
                d["state"].as_str().unwrap(),
                d["realized_by"].as_str().unwrap_or("-"),
            )
        })
        .collect();
    assert_eq!(
        duties,
        [
            ("transformation", "realized", "draft"),
            ("effect", "realized", "write_output"),
            ("format", "realized", "for_each"),
            ("identity", "realized", "draft_fold"),
            ("identity", "realized", "draft_fold"),
            ("cardinality", "realized", "draft"),
        ],
        "{record:#}"
    );
    assert!(
        ledger.iter().all(|d| d["state"] != "unresolved"),
        "{record:#}"
    );
    // The plan record stays the replayable identity of the plan: no ledger inside it.
    assert!(
        out.provenance
            .plan
            .as_ref()
            .unwrap()
            .get("ledger")
            .is_none()
    );
    // A gated write carries its gate; a prompt-bound cardinality says it is not verified.
    let out = replay(HEADLINE, &headline_record(None), &[MODEL]);
    let record = out.provenance.decision.clone().unwrap();
    assert_eq!(record["ledger"][0]["realized_by"], "draft", "{record:#}");
    let mut gated = headline_record(None);
    gated["effects"][0]["policy"] = json!("human_first");
    gated["constraints"] = json!(["a single line, under 90 characters"]);
    let out = replay(HEADLINE, &gated, &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let record = out.provenance.decision.clone().unwrap();
    let by_kind: Vec<(&str, &str, &str)> = record["ledger"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["kind"].as_str().unwrap(),
                d["realized_by"].as_str().unwrap_or("-"),
                d["note"].as_str().unwrap_or("-"),
            )
        })
        .collect();
    assert!(
        by_kind.contains(&("gate", "write_output_review", "-")),
        "{record:#}"
    );
    assert!(
        by_kind.contains(&(
            "cardinality",
            "draft",
            "prompt guidance; not verified at run"
        )),
        "{record:#}"
    );
    // A constraint with no step to carry it: INCOMPLETE naming the instruction, no candidate.
    let intent = "Read ./draft.md and write it to ./final.md in a warm tone.";
    let record = json!({"operations":[
        {"op":"read","detail":"./draft.md","evidence":"Read ./draft.md","categories":[]}],
      "effects":[{"verb":"write","target":"./final.md","policy":"automatic","evidence":"write it to ./final.md","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":["in a warm tone"],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[]);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown
                && d.target == "format"
                && d.message.contains("in a warm tone")
                && d.message.contains("silent obligation")),
        "{out:#?}"
    );
    let ledger = out.provenance.decision.clone().unwrap()["ledger"].clone();
    assert_eq!(ledger[0]["kind"], "effect", "{ledger:#}");
    assert_eq!(ledger[0]["state"], "realized");
    assert_eq!(ledger[1]["kind"], "format");
    assert_eq!(ledger[1]["state"], "unresolved");
    // Metamorphic: the same request without the tone instruction is READY.
    let mut plain = record;
    plain["constraints"] = json!([]);
    let out = replay("Read ./draft.md and write it to ./final.md.", &plain, &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
}

// ── a cadence or an event is a requirement beside the candidate, never a dropped clause ─
// nika#1720: the portable program bytes carry no cadence, hook id or secret; the outcome
// states the trigger requirement next to the candidate, the ledger records the clause as
// carried by that requirement, and a sequencing head ("once …") states none.
const MORNING: &str = "Every morning, read ./tickets.json, draft a short digest of the open tickets and write it to ./out/digest.md.";
fn morning_record(trigger: &str) -> Value {
    json!({"operations":[
        {"op":"read","detail":"./tickets.json","evidence":"read ./tickets.json","categories":[]},
        {"op":"draft","detail":"a short digest of the open tickets","evidence":"draft a short digest of the open tickets","categories":[]}],
      "effects":[{"verb":"write","target":"./out/digest.md","policy":"automatic","evidence":"write it to ./out/digest.md","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./tickets.json"},{"role":"path","literal":"./out/digest.md"}],
      "constraints":[],"unknowns":[],"trigger":trigger,"strategy":"cold"})
}

#[test]
fn a_cadence_or_an_event_is_a_trigger_requirement_beside_the_candidate() {
    let out = replay(MORNING, &morning_record("Every morning"), &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let requirement = out.requested_trigger.as_ref().expect("a cadence is stated");
    assert_eq!(requirement.kind, TriggerKind::Schedule);
    assert_eq!(requirement.status, TriggerStatus::RequiresBinding);
    assert_eq!(requirement.source_hint.as_deref(), Some("Every morning"));
    assert!(requirement.payload_input.is_none(), "{requirement:#?}");
    let doc = document(&out);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    assert!(
        !doc.to_string().to_lowercase().contains("morning"),
        "the bytes carry no cadence: {doc:#}"
    );
    let wire = outcome_document(&out);
    assert_eq!(wire["requested_trigger"]["kind"], "schedule", "{wire:#}");
    assert_eq!(wire["requested_trigger"]["status"], "requires_binding");
    assert_eq!(wire["requested_trigger"]["source_hint"], "Every morning");
    let ledger = out.provenance.decision.clone().unwrap()["ledger"].clone();
    assert!(
        ledger
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "trigger"
                && d["state"] == "realized"
                && d["realized_by"] == "requested_trigger"),
        "{ledger:#}"
    );
    // An outside event, in another language, with no material of its own: the item is the
    // payload the trigger supplies.
    let intent =
        "Dès qu'un ticket arrive, rédige un accusé de réception et écris-le dans ./out/accuse.md.";
    let record = json!({"operations":[
        {"op":"draft","detail":"un accusé de réception","evidence":"rédige un accusé de réception","categories":[]}],
      "effects":[{"verb":"write","target":"./out/accuse.md","policy":"automatic","evidence":"écris-le dans ./out/accuse.md","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":"Dès qu'un ticket arrive","strategy":"cold"});
    let out = replay(intent, &record, &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let requirement = out.requested_trigger.as_ref().expect("an event is stated");
    assert_eq!(requirement.kind, TriggerKind::Event);
    assert_eq!(requirement.payload_input.as_deref(), Some("item"));
    // A sequencing head and a plain request state no requirement.
    let out = replay(
        HEADLINE,
        &headline_record(Some("once the brief is read")),
        &[MODEL],
    );
    assert!(out.requested_trigger.is_none(), "{out:#?}");
    assert!(outcome_document(&out)["requested_trigger"].is_null());
    let out = replay(
        MORNING,
        &morning_record("Every morning").tap_trigger_null(),
        &[MODEL],
    );
    assert!(out.requested_trigger.is_none(), "{out:#?}");
}

trait TapTriggerNull {
    fn tap_trigger_null(self) -> Self;
}
impl TapTriggerNull for Value {
    fn tap_trigger_null(mut self) -> Self {
        self["trigger"] = Value::Null;
        self
    }
}

// ── contradictory bounds on the produced content are refused, never run ──────────
#[test]
fn contradictory_bounds_on_the_produced_content_are_refused_never_run() {
    let intent = "Read ./notes.md and write ./out/report.md: the report must be exactly 5 lines and at least 12 lines long, both are mandatory.";
    let record = |constraints: Value| {
        json!({"operations":[
            {"op":"read","detail":"./notes.md","evidence":"Read ./notes.md","categories":[]},
            {"op":"draft","detail":"the report","evidence":"write ./out/report.md: the report","categories":[]}],
          "effects":[{"verb":"write","target":"./out/report.md","policy":"automatic","evidence":"write ./out/report.md","policy_literal":null}],
          "obligations":[],"bindings":[],"constraints":constraints,"unknowns":[],"trigger":null,"strategy":"cold"})
    };
    let out = replay(
        intent,
        &record(json!(["exactly 5 lines", "at least 12 lines long"])),
        &[MODEL],
    );
    assert_eq!(out.status, CompileStatus::Refused, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::RequiresHuman
                && d.message.contains("exactly 5 lines")
                && d.message.contains("at least 12 lines long")),
        "{out:#?}"
    );
    assert!(keys(&out).contains(&"intent.clarification"));
    let ledger = out.provenance.decision.clone().unwrap()["ledger"].clone();
    let contradicted = ledger
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["kind"] == "cardinality" && d["state"] == "contradicted")
        .count();
    assert_eq!(contradicted, 2, "{ledger:#}");
    // Compatible bounds are carried by the draft's prompt and the request is READY.
    let out = replay(
        intent,
        &record(json!(["at least 3 lines", "5 lines"])),
        &[MODEL],
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
}

// ── a trigger over the request's own material never declares an item ────────────
// Two sealed seeds compiled READY and died at run time on a missing `inputs.item`: "once
// all three are done" (a sequencing trigger over a read brief) and "pour chaque ligne de
// niveau critique" (a per-row trigger over a read CSV) both declared an input no run could
// supply. The item is the material of an invocation only when the request supplies none.
const HEADLINE: &str = "Take the product brief in ./brief.md and write a formal headline (a single line, under 90 characters) to ./out/headline.txt. Once the brief is read, nothing else runs.";
fn headline_record(trigger: Option<&str>) -> Value {
    json!({"operations":[
        {"op":"read","detail":"./brief.md","evidence":"Take the product brief in ./brief.md","categories":[]},
        {"op":"draft","detail":"a formal headline (a single line, under 90 characters)","evidence":"write a formal headline (a single line, under 90 characters) to ./out/headline.txt","categories":[]}],
      "effects":[{"verb":"write","target":"./out/headline.txt","policy":"automatic","evidence":"write a formal headline (a single line, under 90 characters) to ./out/headline.txt","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":trigger,"strategy":"cold"})
}

#[test]
fn a_trigger_over_the_request_s_own_material_never_declares_an_item() {
    let sequenced = replay(
        HEADLINE,
        &headline_record(Some("once the brief is read")),
        &[MODEL],
    );
    assert_eq!(sequenced.status, CompileStatus::Ready, "{sequenced:#?}");
    let doc = document(&sequenced);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    let prompt = tasks(&doc)["draft"]["infer"]["prompt"].as_str().unwrap();
    assert!(!prompt.contains("inputs.item"), "{prompt}");
    // Metamorphic pair: the sequencing trigger changes nothing in the emitted source.
    let plain = replay(HEADLINE, &headline_record(None), &[MODEL]);
    assert_eq!(sequenced.candidate, plain.candidate);
    // No material of its own: the item IS the material, as before.
    let intent = "For each incoming brief, write a formal headline to ./out/headline.txt.";
    let record = json!({"operations":[
        {"op":"draft","detail":"a formal headline","evidence":"write a formal headline to ./out/headline.txt","categories":[]}],
      "effects":[{"verb":"write","target":"./out/headline.txt","policy":"automatic","evidence":"write a formal headline to ./out/headline.txt","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":"For each incoming brief","strategy":"cold"});
    let per_item = replay(intent, &record, &[MODEL]);
    assert_eq!(per_item.status, CompileStatus::Ready, "{per_item:#?}");
    let doc = document(&per_item);
    assert_eq!(doc["inputs"]["item"]["required"], true, "{doc:#}");
}

// ── an outbound effect repeated per item is asked, never sent once in silence ───
const ALERTS: &str = "Read ./alerts.csv (columns id,level,message). For each critical row, send a POST to http://127.0.0.1:18471/hooks with a JSON body {id, message}, one request per alert, nothing for the other levels. At the end write ./out/sent.json: the array of the ids sent, in file order.";
const ALERTS_FOLD: &str = "Read ./alerts.csv (columns id,level,message). For each row, compute whether it is critical. Then send one POST to http://127.0.0.1:18471/hooks with the critical rows as a JSON body and write ./out/sent.json with their ids.";
const RULE_CRITICAL: (&str, &str) = (
    "const.rule_expression",
    r#"".records | map(select(.level == \"critical\"))""#,
);
fn alerts_record(intent: &str, compute: &str, send: &str, trigger: &str) -> Value {
    json!({"operations":[
        {"op":"read","detail":"./alerts.csv","evidence":"Read ./alerts.csv (columns id,level,message)","categories":[]},
        {"op":"compute","detail":"critical","evidence":compute,"categories":[]}],
      "effects":[
        {"verb":"send","target":"a POST to http://127.0.0.1:18471/hooks","policy":"automatic","evidence":send,"policy_literal":null},
        {"verb":"write","target":"./out/sent.json","policy":"automatic","evidence":"write ./out/sent.json","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./alerts.csv"},{"role":"url","literal":"http://127.0.0.1:18471/hooks"},{"role":"path","literal":"./out/sent.json"}],
      "constraints":[],"unknowns":[],"trigger":trigger,"strategy":"cold",
      "intent_check": intent.contains(send)})
}

#[test]
fn an_outbound_effect_repeated_per_item_of_a_read_source_is_asked_never_sent_once() {
    let record = alerts_record(
        ALERTS,
        "For each critical row",
        "send a POST to http://127.0.0.1:18471/hooks with a JSON body {id, message}, one request per alert",
        "For each critical row",
    );
    let out = replay(ALERTS, &record, &[RULE_CRITICAL]);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown
                && d.target == "send"
                && d.message.contains("once per item")
                && d.message.contains("For each critical row")),
        "{out:#?}"
    );
    // The effect a later sentence states applies to the whole result: one POST, no item,
    // no clarification. The distributive trigger over the rows stays a trigger.
    let folded = alerts_record(
        ALERTS_FOLD,
        "For each row, compute whether it is critical",
        "send one POST to http://127.0.0.1:18471/hooks with the critical rows as a JSON body",
        "For each row",
    );
    let out = replay(ALERTS_FOLD, &folded, &[RULE_CRITICAL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    assert_eq!(
        tasks(&doc)["send"]["invoke"]["tool"],
        "nika:fetch",
        "{doc:#}"
    );
    assert_eq!(tasks(&doc)["send"]["invoke"]["args"]["method"], "POST");
    assert_eq!(
        doc["const"]["send_endpoint"],
        "http://127.0.0.1:18471/hooks"
    );
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}"
    );
}

// ── distinct files bound to one drafted text are asked, never duplicated ────────
// The sealed "three headline variants, one per tone" seed drafted once and wrote the same
// body to three files; the run would have been green on wrong content.
const VARIANTS: &str = "Take the product brief in ./brief.md and write three headline variants, one per tone: formal, playful and urgent, each to its own file ./out/headline-formal.txt, ./out/headline-playful.txt and ./out/headline-urgent.txt.";
fn variants_record() -> Value {
    json!({"operations":[
        {"op":"read","detail":"./brief.md","evidence":"Take the product brief in ./brief.md","categories":[]},
        {"op":"draft","detail":"three headline variants, one per tone: formal, playful and urgent","evidence":"write three headline variants, one per tone: formal, playful and urgent","categories":[]}],
      "effects":[
        {"verb":"write","target":"./out/headline-formal.txt","policy":"automatic","evidence":"each to its own file ./out/headline-formal.txt","policy_literal":null},
        {"verb":"write","target":"./out/headline-playful.txt","policy":"automatic","evidence":"./out/headline-playful.txt","policy_literal":null},
        {"verb":"write","target":"./out/headline-urgent.txt","policy":"automatic","evidence":"./out/headline-urgent.txt","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":null,"strategy":"cold"})
}

#[test]
fn distinct_files_bound_to_one_drafted_text_are_asked_never_duplicated() {
    let out = replay(VARIANTS, &variants_record(), &[MODEL]);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown
                && d.message.contains("./out/headline-formal.txt")
                && d.message.contains("./out/headline-playful.txt")
                && d.message.contains("same drafted text")),
        "{out:#?}"
    );
    // Two renderings of one computed result (JSON and CSV) are not a duplicate.
    let intent = "Read ./data/orders.csv, keep only the rows whose amount is strictly greater than 100, and write those rows to ./out/big.json and to ./out/big.csv.";
    let record = json!({"operations":[
        {"op":"read","detail":"./data/orders.csv","evidence":"Read ./data/orders.csv","categories":[]},
        {"op":"compute","detail":"keep only the rows whose amount is strictly greater than 100","evidence":"keep only the rows whose amount is strictly greater than 100","categories":[]}],
      "effects":[
        {"verb":"write","target":"./out/big.json","policy":"automatic","evidence":"write those rows to ./out/big.json","policy_literal":null},
        {"verb":"write","target":"./out/big.csv","policy":"automatic","evidence":"to ./out/big.csv","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[RULE]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}",
        "{doc:#}"
    );
    assert_eq!(
        tasks(&doc)["write_big"]["with"]["content"],
        "${{ tasks.big_csv.output }}"
    );
}

// ── a structured destination receives its format, not JSON ─────────────────────
// The control (case a) wrote the computed JSON array into ./out/big_orders.csv. A .csv,
// .yaml or .toml destination whose content is data gets a nika:convert stage feeding
// the write; .json stays JSON (see two_write_clauses_yield_two_write_tasks_with_typed_content).
#[tokio::test]
async fn a_csv_destination_receives_csv_from_the_computed_rows() {
    let out = compile(BIG_ORDERS, &big_orders_plan(), &[MODEL, RULE]).await;
    let doc = document(&out);
    assert_eq!(
        doc["const"]["output_path"], "./out/big_orders.csv",
        "{doc:#}"
    );
    assert_eq!(doc["const"]["summary_path"], "./out/summary.md");
    let convert = &tasks(&doc)["big_orders_csv"];
    assert_eq!(convert["invoke"]["tool"], "nika:convert", "{doc:#}");
    assert_eq!(convert["invoke"]["args"]["from"], "json");
    assert_eq!(convert["invoke"]["args"]["to"], "csv");
    assert_eq!(convert["invoke"]["args"]["input"], "${{ with.data }}");
    assert_eq!(convert["with"]["data"], "${{ tasks.compute.output }}");
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.big_orders_csv.output }}"
    );
    assert_eq!(
        tasks(&doc)["write_summary"]["with"]["content"],
        "${{ tasks.draft.output.body }}"
    );
    assert!(
        doc["permits"]["tools"]
            .as_array()
            .unwrap()
            .contains(&json!("nika:convert"))
    );
    // The CSV source is parsed because the compute consumes the rows.
    assert_eq!(tasks(&doc)["parse_source"]["invoke"]["args"]["from"], "csv");
}

// ── #1666 · a CSV written back from a CSV source keeps the source's column order ──
// The engine never preserves JSON key order, so `big_orders_csv` would sort the header
// (`amount,customer,order_id`) while the requester read `order_id,customer,amount`. The
// order is read from the source text itself (`source_columns`, a jq over the read
// document) and handed to the convert stage as `columns`; a source that is not a CSV has
// no order to keep.
#[tokio::test]
async fn a_csv_destination_from_a_csv_source_keeps_the_source_column_order() {
    let out = compile(BIG_ORDERS, &big_orders_plan(), &[MODEL, RULE]).await;
    let doc = document(&out);
    let columns = &tasks(&doc)["source_columns"];
    assert_eq!(columns["invoke"]["tool"], "nika:jq", "{doc:#}");
    assert_eq!(columns["invoke"]["args"]["input"], "${{ with.document }}");
    assert_eq!(
        columns["with"]["document"],
        "${{ tasks.read_source.output }}"
    );
    let expression = columns["invoke"]["args"]["expression"].as_str().unwrap();
    assert!(
        expression.starts_with(r#"split("\n") | .[0]"#),
        "{expression}"
    );
    assert!(expression.contains(r#"rtrimstr("\r")"#), "{expression}");
    assert!(expression.contains(r#"split(",")"#), "{expression}");
    assert!(
        columns.get("after").is_none(),
        "a data edge, never a control edge: {columns:#}"
    );
    let convert = &tasks(&doc)["big_orders_csv"];
    assert_eq!(
        convert["invoke"]["args"]["columns"], "${{ with.columns }}",
        "{doc:#}"
    );
    assert_eq!(
        convert["with"]["columns"],
        "${{ tasks.source_columns.output }}"
    );
    assert_eq!(convert["with"]["data"], "${{ tasks.compute.output }}");
    // The control chain is unchanged: nothing follows the column read.
    for (id, task) in tasks(&doc) {
        assert!(
            task["after"].get("source_columns").is_none(),
            "{id} chains on the column read: {doc:#}"
        );
    }
}

#[tokio::test]
async fn a_csv_destination_from_a_json_source_has_no_column_order_to_keep() {
    let intent = BIG_ORDERS.replace("./data/orders.csv", "./data/orders.json");
    let mut plan = big_orders_plan();
    plan["steps"][0] =
        json!({"op":"read","detail":"./data/orders.json","evidence":"Read ./data/orders.json"});
    let out = compile(&intent, &plan, &[MODEL, RULE]).await;
    let doc = document(&out);
    assert_eq!(
        tasks(&doc)["parse_source"]["invoke"]["tool"],
        "nika:jq",
        "{doc:#}"
    );
    assert!(tasks(&doc).get("source_columns").is_none(), "{doc:#}");
    let convert = &tasks(&doc)["big_orders_csv"];
    assert_eq!(convert["invoke"]["args"]["to"], "csv", "{doc:#}");
    assert!(
        convert["invoke"]["args"].get("columns").is_none(),
        "{doc:#}"
    );
    assert!(convert["with"].get("columns").is_none(), "{doc:#}");
}

// ── D5 · the anchor law judges the corpus the step consumed ──────────────────────
#[tokio::test]
async fn extract_anchors_are_checked_against_the_read_document_not_a_phantom_item() {
    let out = compile(STOCK, &stock_plan(), &[MODEL]).await;
    let doc = document(&out);
    let law = &tasks(&doc)["extract_anchors"]["invoke"]["args"];
    let input = law["input"].as_object().unwrap();
    assert!(input.contains_key("document"), "{law:#}");
    assert!(input.contains_key("fields"), "{law:#}");
    assert!(!input.contains_key("item"), "{law:#}");
    let expression = law["expression"].as_str().unwrap();
    assert!(
        expression.contains("any($corpus[]; contains($f.anchor | gsub("),
        "{expression}"
    );
    assert!(!expression.contains("$root.item"), "{expression}");
    assert_eq!(
        tasks(&doc)["extract_anchors"]["with"]["document"],
        "${{ tasks.read_source.output }}"
    );
    // The draft law reads the same corpus plus the extracted fields, consistently.
    let draft_law = &tasks(&doc)["draft_anchors"]["invoke"]["args"];
    let input = draft_law["input"].as_object().unwrap();
    assert!(
        input.contains_key("document") && input.contains_key("fields"),
        "{draft_law:#}"
    );
    assert!(!input.contains_key("item"));
    assert!(
        draft_law["expression"]
            .as_str()
            .unwrap()
            .contains("any($corpus[]; contains($f.anchor | gsub(")
    );
}

// ── the draft law judges the body it is given and folds whitespace ──────────────
// wave16 (19319653) prefixed the law with `(.body | length) > 0` while the law's input
// carried no `body`: every draft was refused at run time (control 2026-09-20, cases a and
// c). The body must be bound, excluded from the corpus, and anchors must survive a
// line wrap.
#[tokio::test]
async fn the_draft_law_reads_the_body_it_judges_and_folds_whitespace() {
    let out = compile(STOCK, &stock_plan(), &[MODEL]).await;
    let doc = document(&out);
    let anchors = &tasks(&doc)["draft_anchors"];
    let law = &anchors["invoke"]["args"];
    let input = law["input"].as_object().unwrap();
    assert_eq!(input["body"], "${{ with.body }}", "{law:#}");
    assert_eq!(
        anchors["with"]["body"], "${{ tasks.draft.output.body }}",
        "{anchors:#}"
    );
    let expression = law["expression"].as_str().unwrap();
    assert!(
        expression.contains("del(.facts_used, .body)"),
        "the body is never its own anchor corpus: {expression}"
    );
    assert!(
        expression.contains("($root.body | length) > 0"),
        "{expression}"
    );
    assert!(
        expression.contains(r#"gsub("\\s+"; " ")"#),
        "anchors are compared after folding runs of whitespace: {expression}"
    );
    // The corpus carries each fact as the prompt renders it (`category: billing`) as well as
    // its bare value, and JSON punctuation spacing folds on both sides: an anchor copied from
    // the prompt line or from a re-serialized record is found (control 2026-09-21, case d).
    assert!(
        expression.contains(r"\(.key): \($v)"),
        "the corpus must hold the `name: value` rendering the prompt shows: {expression}"
    );
    assert!(
        expression.contains(r#"gsub("\\s*:\\s*"; ":")"#),
        "spaces around JSON punctuation must fold on both sides: {expression}"
    );
    let extract = tasks(&doc)["extract_anchors"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap();
    assert!(extract.contains(r#"gsub("\\s+"; " ")"#), "{extract}");
}

// ── D2 · no phantom `item` for a file → transform → write workflow ────────────────
#[tokio::test]
async fn a_read_transform_write_workflow_declares_no_incoming_item() {
    let out = compile(STOCK, &stock_plan(), &[MODEL]).await;
    let doc = document(&out);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    let source = out.candidate.as_deref().unwrap();
    assert!(!source.contains("inputs.item"), "{source}");
    assert!(!source.contains("Item:"), "{source}");
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.draft.output.body }}"
    );
    assert_eq!(doc["const"]["source_path"], "./inventario/stock.json");
    assert_eq!(doc["const"]["output_path"], "./salida/reposicion.md");
    // A JSON source is parsed for code only when a code rule, an endpoint payload or a
    // structured write consumes it; here nothing does, so no parsed copy is emitted.
    assert!(tasks(&doc).get("parse_source").is_none(), "{doc:#}");
    let prompt = tasks(&doc)["extract"]["infer"]["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("document: ${{ with.document }}"),
        "{prompt}"
    );
    assert!(!prompt.contains("records:"), "{prompt}");
}

#[tokio::test]
async fn a_per_item_request_without_material_keeps_its_incoming_item() {
    let intent = "Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse.";
    let plan = json!({"steps":[
        {"op":"lookup","detail":"le client","evidence":"consulte le client"},
        {"op":"classify","detail":"le problème","evidence":"classe le problème"},
        {"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
      "effects":[],"obligations":[],"constraints":[],"unknowns":[]});
    let out = compile(
        intent,
        &plan,
        &[MODEL, ("const.customer_directory", r#""./customers.json""#)],
    )
    .await;
    let doc = document(&out);
    assert_eq!(doc["inputs"]["item"]["required"], true, "{doc:#}");
    assert!(
        tasks(&doc)["draft_anchors"]["invoke"]["args"]["input"]
            .as_object()
            .unwrap()
            .contains_key("item")
    );
}

// ── D1 · a write whose named content nothing produces is never a copy of the page ────
// The gate showed the raw RFC written as the "brief". A proposal that kept the write and
// dropped the draft is not feasible: the compiler asks instead of inventing the content.
#[tokio::test]
async fn a_write_after_a_fetch_with_no_draft_is_a_question_not_a_copy() {
    let out = compile(RFC, &rfc_plan(), &[]).await;
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("names content no step produces")),
        "{out:#?}"
    );
    // With the brief drafted, the write binds the draft and never an incoming item.
    let mut drafted = rfc_plan();
    drafted["steps"].as_array_mut().unwrap().push(json!({
        "op": "draft",
        "detail": "a plain-English brief of under 150 words explaining what the protocol does and why it is a joke, as 5 bullets",
        "evidence": "write a plain-English brief of under 150 words explaining what the protocol does and why it is a joke, as 5 bullets"
    }));
    let out = compile(RFC, &drafted, &[MODEL]).await;
    let doc = document(&out);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.draft.output.body }}"
    );
    assert_eq!(doc["const"]["output_path"], "./out/rfc2324-brief.md");
    assert_eq!(doc["permits"]["net"]["http"], json!(["www.rfc-editor.org"]));
}

#[tokio::test]
async fn a_write_with_nothing_upstream_is_a_finding_not_an_invented_input() {
    let intent = "Write the result to ./out/result.md.";
    let plan = json!({"steps":[],
      "effects":[{"verb":"write","target":"./out/result.md","policy":"automatic","evidence":"Write the result to ./out/result.md"}],
      "obligations":[],"constraints":[],"unknowns":[]});
    let out = compile(intent, &plan, &[]).await;
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown && d.message.contains("nothing to write")),
        "{out:#?}"
    );
}

// ── D3 + D13 · several read paths are a fan-out, never one joined literal ────────
#[tokio::test]
async fn several_read_paths_become_a_bounded_fan_out_with_one_permit_per_path() {
    let out = compile(CATALOG, &catalog_plan(), &[MODEL]).await;
    let doc = document(&out);
    let paths = json!([
        "./catalog/solar-lamp.md",
        "./catalog/wind-chime.md",
        "./catalog/rain-barrel.md",
        "./catalog/compost-bin.md"
    ]);
    assert_eq!(doc["const"]["source_paths"], paths, "{doc:#}");
    assert_eq!(doc["permits"]["fs"]["read"], paths);
    assert!(doc["const"].get("source_path").is_none(), "{doc:#}");
    assert!(
        doc["permits"]["fs"]["read"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| !p.as_str().unwrap().contains(';'))
    );
    let read = &tasks(&doc)["read_source"];
    assert_eq!(read["for_each"]["items"], "${{ const.source_paths }}");
    assert_eq!(read["for_each"]["max_parallel"], 2, "{read:#}");
    assert_eq!(read["for_each"]["fail_fast"], true);
    assert_eq!(read["invoke"]["args"]["path"], "${{ item }}");
    // "a heading above each blurb": one blurb per file, zipped, drafted per item, folded.
    let items = &tasks(&doc)["draft_items"];
    assert_eq!(items["with"]["texts"], "${{ tasks.read_source.output }}");
    assert_eq!(
        items["invoke"]["args"]["input"]["paths"],
        "${{ const.source_paths }}"
    );
    assert!(tasks(&doc).get("documents").is_none(), "{doc:#}");
    let draft = &tasks(&doc)["draft"];
    assert_eq!(draft["with"]["items"], "${{ tasks.draft_items.output }}");
    assert_eq!(draft["for_each"]["max_parallel"], 2, "{draft:#}");
    // The concurrency bound is structure now, not prompt text.
    let prompt = draft["infer"]["prompt"].as_str().unwrap();
    assert!(!prompt.contains("Process at most 2"), "{prompt}");
    assert!(prompt.contains("${{ item.text }}"), "{prompt}");
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.draft_fold.output }}"
    );
    assert!(
        doc.get("inputs").is_none(),
        "a fan-out over named files has no incoming item"
    );
}

// ── D8 · "merge … into a single file" is a write to that file, never a POST ──────
#[tokio::test]
async fn an_effect_whose_target_names_a_local_file_is_a_write_not_an_endpoint() {
    let asked = compile(CATALOG, &catalog_plan(), &[]).await;
    assert_eq!(keys(&asked), ["model"], "{asked:#?}");
    let out = compile(CATALOG, &catalog_plan(), &[MODEL]).await;
    let doc = document(&out);
    assert!(doc["const"].get("merge_endpoint").is_none(), "{doc:#}");
    assert!(!out.candidate.as_deref().unwrap().contains("nika:fetch"));
    assert_eq!(doc["const"]["output_path"], "./out/catalog-blurbs.md");
    assert_eq!(
        doc["permits"]["fs"]["write"],
        json!(["./out/catalog-blurbs.md"])
    );
    let writes: Vec<_> = tasks(&doc)
        .iter()
        .filter(|(_, t)| t["invoke"]["tool"] == "nika:write")
        .map(|(id, _)| id.clone())
        .collect();
    assert_eq!(writes, ["write_output"], "one file, one write");
}

// ── D7 · prose never becomes a path literal ──────────────────────────────────────
#[tokio::test]
async fn a_prose_write_target_yields_exactly_its_one_path_token() {
    let out = compile(
        ORDERS,
        &orders_prose_plan(),
        &[MODEL, ("const.rule_expression", r#"".records""#)],
    )
    .await;
    let doc = document(&out);
    assert_eq!(doc["const"]["summary_path"], "./out/summary.md", "{doc:#}");
    assert_eq!(doc["const"]["source_path"], "./data/orders-2026-09.csv");
    assert_eq!(
        doc["permits"]["fs"]["read"],
        json!(["./data/orders-2026-09.csv"])
    );
    for (name, value) in doc["const"].as_object().unwrap() {
        if name.ends_with("_path") {
            assert!(!value.as_str().unwrap().contains(' '), "{name}: {value}");
        }
    }
    for path in doc["permits"]["fs"]["write"].as_array().unwrap() {
        assert!(!path.as_str().unwrap().contains(' '), "{path}");
    }
}

#[tokio::test]
async fn a_directory_is_never_read_as_one_file_it_asks_for_a_glob_then_fans_out() {
    let asked = compile(NOTES, &notes_plan(), &[MODEL]).await;
    assert!(asked.candidate.is_none(), "{asked:#?}");
    assert_eq!(keys(&asked), ["const.source_glob"], "{asked:#?}");
    assert!(label(&asked, "const.source_glob").contains("./notes/*.md"));
    let bare = compile(
        NOTES,
        &notes_plan(),
        &[MODEL, ("const.source_glob", r#""./notes""#)],
    )
    .await;
    assert!(bare.candidate.is_none());
    assert_eq!(keys(&bare), ["const.source_glob"], "{bare:#?}");
    let out = compile(
        NOTES,
        &notes_plan(),
        &[MODEL, ("const.source_glob", r#""./notes/*.md""#)],
    )
    .await;
    let doc = document(&out);
    assert_eq!(doc["const"]["source_glob"], "./notes/*.md");
    assert_eq!(doc["permits"]["fs"]["read"], json!(["./notes/**"]));
    assert_eq!(tasks(&doc)["glob_source"]["invoke"]["tool"], "nika:glob");
    let read = &tasks(&doc)["read_source"];
    assert_eq!(read["with"]["paths"], "${{ tasks.glob_source.output }}");
    assert_eq!(read["for_each"]["items"], "${{ with.paths }}");
    // "un resumé de chaque … avec le nom du fichier en titre": one summary per file.
    let items = &tasks(&doc)["draft_items"];
    assert_eq!(items["with"]["paths"], "${{ tasks.glob_source.output }}");
    assert_eq!(
        items["invoke"]["args"]["input"]["paths"],
        "${{ with.paths }}"
    );
    let draft = &tasks(&doc)["draft"];
    assert_eq!(draft["for_each"]["items"], "${{ with.items }}", "{draft:#}");
    assert!(draft["for_each"].get("max_parallel").is_none());
    let prompt = draft["infer"]["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("3 lignes max"),
        "a per-item cap stays: {prompt}"
    );
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.draft_fold.output }}"
    );
    assert_eq!(doc["const"]["output_path"], "./out/recap-semaine.md");
    assert!(doc.get("inputs").is_none());
}

#[tokio::test]
async fn a_placeholder_path_asks_for_the_exact_files() {
    // Explicit enough for the deterministic reader: zero model calls, one placeholder.
    let intent = "Read ./catalog/<slug>.md and write the text to ./out/blurbs.md.";
    let plan = json!({"steps":[
        {"op":"read","detail":"./catalog/<slug>.md","evidence":"Read ./catalog/<slug>.md"}],
      "effects":[{"verb":"write","target":"./out/blurbs.md","policy":"automatic","evidence":"write the text to ./out/blurbs.md"}],
      "obligations":[],"constraints":[],"unknowns":[]});
    let asked = compile(intent, &plan, &[]).await;
    assert_eq!(keys(&asked), ["const.source_paths"], "{asked:#?}");
    let rejected = compile(intent, &plan, &[("const.source_paths", r#"["./catalog"]"#)]).await;
    assert_eq!(keys(&rejected), ["const.source_paths"], "{rejected:#?}");
    let out = compile(
        intent,
        &plan,
        &[(
            "const.source_paths",
            r#"["./catalog/a.md", "./catalog/b.md"]"#,
        )],
    )
    .await;
    let doc = document(&out);
    assert_eq!(
        doc["const"]["source_paths"],
        json!(["./catalog/a.md", "./catalog/b.md"])
    );
    assert!(
        tasks(&doc)["read_source"]["for_each"]
            .get("max_parallel")
            .is_none()
    );
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.documents.output }}"
    );
}

// ── D6 · the rule question describes the exact input the rule receives ───────────
#[tokio::test]
async fn the_rule_question_names_the_parsed_input_shape_and_the_rule_receives_it() {
    let asked = compile(ORDERS, &orders_plan(), &[MODEL]).await;
    let text = label(&asked, "const.rule_expression");
    assert!(text.contains("{document, records}"), "{text}");
    assert!(
        text.contains("`.records` is the rows of ./data/orders-2026-09.csv"),
        "{text}"
    );
    assert!(!text.contains("{item, record, fields}"), "{text}");
    let out = compile(
        ORDERS,
        &orders_plan(),
        &[
            MODEL,
            ("const.rule_expression", r#"".records | map(select(.status == \"shipped\" and (.total_eur | tonumber) > 120)) | group_by(.country) | map({(.[0].country): length}) | add""#),
        ],
    )
    .await;
    let doc = document(&out);
    let parse = &tasks(&doc)["parse_source"];
    assert_eq!(parse["invoke"]["tool"], "nika:convert");
    assert_eq!(parse["invoke"]["args"]["from"], "csv");
    assert_eq!(parse["invoke"]["args"]["to"], "json");
    let compute = &tasks(&doc)["compute"];
    let input = compute["invoke"]["args"]["input"].as_object().unwrap();
    assert_eq!(input["document"], "${{ with.document }}");
    assert_eq!(input["records"], "${{ with.records }}");
    assert!(!input.contains_key("item"), "{compute:#}");
    assert_eq!(
        compute["with"]["records"],
        "${{ tasks.parse_source.output }}"
    );
    // The draft prompt sees the document and the computed result, not the parsed copy.
    let prompt = tasks(&doc)["draft"]["infer"]["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("computed: ${{ with.computed }}"),
        "{prompt}"
    );
    assert!(!prompt.contains("records:"), "{prompt}");
}

// ── D4 · a named output file is never dropped in silence ─────────────────────────
#[tokio::test]
async fn two_write_clauses_yield_two_write_tasks_with_typed_content() {
    // The xai proposal carried ONE write effect; the reader's own write floor carries the
    // other file, and two writes to two files are two effects, never one overwritten target.
    let out = compile(
        ORDERS,
        &orders_plan(),
        &[MODEL, ("const.rule_expression", r#"".records""#)],
    )
    .await;
    let doc = document(&out);
    assert_eq!(
        doc["const"]["output_path"], "./out/shipped-by-country.json",
        "{doc:#}"
    );
    assert_eq!(doc["const"]["summary_path"], "./out/summary.md");
    assert_eq!(
        doc["permits"]["fs"]["write"],
        json!(["./out/shipped-by-country.json", "./out/summary.md"])
    );
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}",
        "a JSON target takes the computed result"
    );
    let summary = &tasks(&doc)["write_summary"];
    assert_eq!(summary["invoke"]["tool"], "nika:write");
    assert_eq!(
        summary["invoke"]["args"]["path"],
        "${{ const.summary_path }}"
    );
    assert_eq!(summary["with"]["content"], "${{ tasks.draft.output.body }}");
    assert_eq!(
        doc["outputs"]["write_summary_status"],
        "${{ tasks.write_summary.status }}"
    );
}

#[tokio::test]
async fn a_named_path_no_effect_writes_is_a_question_then_its_own_write() {
    // A clause the deterministic reader cannot consume ("Harmonise") keeps this COLD.
    let intent = "Read ./data/orders.csv. Harmonise the totals per country; the JSON belongs in ./out/totals.json. Write a note to ./out/note.md.";
    let plan = json!({"steps":[
        {"op":"read","detail":"./data/orders.csv","evidence":"Read ./data/orders.csv"},
        {"op":"compute","detail":"the totals per country","evidence":"Harmonise the totals per country"},
        {"op":"draft","detail":"a note","evidence":"Write a note to ./out/note.md"}],
      "effects":[{"verb":"write","target":"./out/note.md","policy":"automatic","evidence":"Write a note to ./out/note.md"}],
      "obligations":[],"constraints":[],"unknowns":[]});
    let rule = ("const.rule_expression", r#"".records""#);
    let asked = compile(intent, &plan, &[MODEL, rule]).await;
    assert!(asked.candidate.is_none(), "{asked:#?}");
    assert_eq!(keys(&asked), ["effect.write_totals.include"], "{asked:#?}");
    assert!(label(&asked, "effect.write_totals.include").contains("./out/totals.json"));
    let both = compile(
        intent,
        &plan,
        &[MODEL, rule, ("effect.write_totals.include", "true")],
    )
    .await;
    let doc = document(&both);
    assert_eq!(doc["const"]["output_path"], "./out/note.md");
    assert_eq!(doc["const"]["totals_path"], "./out/totals.json");
    assert_eq!(
        doc["permits"]["fs"]["write"],
        json!(["./out/note.md", "./out/totals.json"])
    );
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.draft.output.body }}"
    );
    let second = &tasks(&doc)["write_totals"];
    assert_eq!(second["invoke"]["tool"], "nika:write");
    assert_eq!(second["invoke"]["args"]["path"], "${{ const.totals_path }}");
    assert_eq!(second["with"]["content"], "${{ tasks.compute.output }}");
    assert_eq!(
        doc["outputs"]["write_totals_status"],
        "${{ tasks.write_totals.status }}"
    );
    let one = compile(
        intent,
        &plan,
        &[MODEL, rule, ("effect.write_totals.include", "false")],
    )
    .await;
    let doc = document(&one);
    assert!(tasks(&doc).get("write_totals").is_none());
    assert!(
        one.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Applied && d.message.contains("./out/totals.json")),
        "{one:#?}"
    );
}

#[tokio::test]
async fn two_explicit_write_effects_are_two_write_tasks() {
    let intent = "Read ./data/orders.csv, compute the totals, write the totals as JSON to ./out/totals.json and write a note to ./out/note.md.";
    let plan = json!({"steps":[
        {"op":"read","detail":"./data/orders.csv","evidence":"Read ./data/orders.csv"},
        {"op":"compute","detail":"the totals","evidence":"compute the totals"},
        {"op":"draft","detail":"a note","evidence":"write a note to ./out/note.md"}],
      "effects":[
        {"verb":"write","target":"./out/totals.json","policy":"automatic","evidence":"write the totals as JSON to ./out/totals.json"},
        {"verb":"write","target":"./out/note.md","policy":"automatic","evidence":"write a note to ./out/note.md"}],
      "obligations":[],"constraints":[],"unknowns":[]});
    let out = compile(
        intent,
        &plan,
        &[MODEL, ("const.rule_expression", r#"".records""#)],
    )
    .await;
    let doc = document(&out);
    assert_eq!(doc["const"]["output_path"], "./out/totals.json");
    assert_eq!(doc["const"]["note_path"], "./out/note.md");
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}"
    );
    assert_eq!(
        tasks(&doc)["write_note"]["with"]["content"],
        "${{ tasks.draft.output.body }}"
    );
    assert_eq!(
        doc["permits"]["fs"]["write"],
        json!(["./out/totals.json", "./out/note.md"])
    );
}
