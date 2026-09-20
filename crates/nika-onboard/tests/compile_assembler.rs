// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The deterministic assembler on fresh-intent plans: a compiled workflow must produce the
//! artefact the intent asked for. Every case is a plan an authoring model actually proposed
//! for a fresh intent (2026-09-20 clean-shell gate), fed through a hermetic provider double;
//! the assertions read the emitted candidate, never a model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use nika_onboard::compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind,
    compile_with_provider,
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
    nika_onboard::compile::compile(&request).unwrap()
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
    let asked = compile(BIG_ORDERS, &demoted_plan(), &[MODEL]).await;
    assert!(
        keys(&asked).contains(&"const.rule_expression"),
        "{asked:#?}"
    );
    let text = label(&asked, "const.rule_expression");
    assert!(
        text.contains("keep only the rows whose amount is strictly greater than 100"),
        "{text}"
    );
    assert!(text.contains("{document, records}"), "{text}");
    let ops: Vec<&str> = asked.provenance.plan.as_ref().unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["read", "compute", "draft"], "{asked:#?}");
    let compute = &operations(&asked, "compute")[0];
    assert_eq!(
        compute["evidence"],
        "keep only the rows whose amount is strictly greater than 100"
    );
    assert_eq!(
        asked.provenance.plan.as_ref().unwrap()["constraints"],
        json!([]),
        "the promoted rule leaves the prompt guidance"
    );
    let out = compile(BIG_ORDERS, &demoted_plan(), &[MODEL, RULE]).await;
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
