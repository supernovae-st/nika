// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A construction obligation through the document door (R5 · A5): a clause that asks for an
//! admitted component is judged on how the document is built, never on a runtime task. SCRIPTED
//! seats over the real compile entry: the author's answers in order, and a judge that decides
//! from the engine facts each question shows (the offer, each component's construction status on
//! the judged bytes, what the bytes compose). These tests establish the door's mechanics, never a
//! model's behaviour.

use super::{
    ENVELOPE, SNAPSHOT, STALE_INTENT, Shelf, calls, compose, door, expanded, stale_request,
    state_of,
};
use crate::rehearse::{
    Attempt, Bounds, CopyReceipt, Digest, EffectCounts, FinalReceipt, FinalState, Held,
    LedgerFacts, Observation, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
    RehearsedOutput, RoomEvidence, Spent,
};
use nika_compile::surface::sha256;
use nika_compile_seats::foundry::component::pinned;
use nika_compile_seats::foundry::{
    Component, ComponentCatalog, ComponentRef, Hole, Release, Unresolved,
};
use serde_json::{Value, json};

/// Words of the request's conditional clause: how the scripted judge recognizes the part it is
/// asked about (test scaffolding, never an engine rule).
const FOUNDRY: &str = "admitted Foundry component";

/// The judge's contextual decision over the stale-filter offer: it fits the request.
const STALE: &[&str] = &["block:stale-filter-report"];

/// The same work the stale-filter component does, written whole by the author: other task and
/// constant names, the same paths, threshold, order, report and named output.
const HAND_WRITTEN: &str = r#"nika: stale-tickets-report
const:
  tickets_path: ./in/tickets.json
  report_path: ./out/report.json
permits:
  fs: { read: ["./in/tickets.json"], write: ["./out/report.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks:
  load_tickets:
    invoke: { tool: "nika:read", args: { path: "${{ const.tickets_path }}" } }
  keep_stale:
    with: { raw: "${{ tasks.load_tickets.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.raw }}", expression: "fromjson | [.[] | select(.age_hours > 48)]" } }
  summarize:
    with: { stale: "${{ tasks.keep_stale.output }}" }
    invoke: { tool: "nika:jq", args: { input: { stale: "${{ with.stale }}" }, expression: "{count: (.stale | length), ids: [.stale[].id]} | tojson" } }
  save_report:
    with: { content: "${{ tasks.summarize.output }}" }
    invoke: { tool: "nika:write", args: { path: "${{ const.report_path }}", content: "${{ with.content }}" } }
outputs:
  stale: ${{ tasks.keep_stale.output }}
"#;

/// How the scripted judge answers the whole request.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Whole {
    /// From the facts: unfaithful when an offered component could be held and none is held.
    Facts,
    /// Always unfaithful, whatever the facts.
    Doubting,
    /// Unfaithful as `Doubting`; over a trial run it names the request's report write as a part
    /// the run did not carry.
    RunMissesWrite,
}

/// What the scripted judge answers when it is asked why a part is missing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Why {
    /// The first offered component it may name, else `no_task`.
    Component,
    /// `no_fit` when offered, else `no_task`.
    NoFit,
    /// NONE: it makes no choice.
    Abstain,
}

/// A SCRIPTED seat: the author's answers in order, and a judge reading the engine facts. Which
/// offered components fit the request (`fits`) is the judge's contextual decision, stated here.
struct Builder {
    answers: std::sync::Mutex<Vec<String>>,
    whole: Whole,
    why: Why,
    fits: &'static [&'static str],
    /// The last message of each authoring call: the opening, then each repair.
    told: std::sync::Mutex<Vec<String>>,
    /// The option keys of each question asking why a part is missing.
    pointed: std::sync::Mutex<Vec<Vec<String>>>,
    /// The engine facts each verifier question showed.
    facts: std::sync::Mutex<Vec<Value>>,
}

impl Builder {
    fn new(answers: Vec<String>, whole: Whole, why: Why, fits: &'static [&'static str]) -> Self {
        Self {
            answers: std::sync::Mutex::new(answers),
            whole,
            why,
            fits,
            told: std::sync::Mutex::new(Vec::new()),
            pointed: std::sync::Mutex::new(Vec::new()),
            facts: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The facts the first verifier question showed.
    fn first_facts(&self) -> Value {
        let facts = self.facts.lock().expect("facts");
        facts.first().cloned().unwrap_or(Value::Null)
    }

    fn told(&self) -> Vec<String> {
        self.told.lock().expect("told").clone()
    }

    fn pointed(&self) -> Vec<Vec<String>> {
        self.pointed.lock().expect("pointed").clone()
    }

    /// The facts the last verifier question showed.
    fn last_facts(&self) -> Value {
        let facts = self.facts.lock().expect("facts");
        facts.last().cloned().unwrap_or(Value::Null)
    }

    /// Whether the offered row fits the request, as this judge decides.
    fn fitting(&self, row: &Value) -> bool {
        (row["component"]["id"].as_str()).is_some_and(|id| self.fits.contains(&id))
    }

    /// Whether the facts leave the clause's component wanting: an offered component that fits,
    /// that the catalogue resolves and that the bytes do not hold as admitted, while no receipt
    /// is witnessed as held. A fact recorded with no construction status reads as the bare offer.
    fn wanting(&self, facts: &Value) -> bool {
        let rows = facts["offered"]["components"].as_array().cloned();
        let held = (facts["composed"].as_array().into_iter().flatten())
            .any(|seen| seen["verdict"] == "expanded" || seen["verdict"] == "invoked");
        let open = |row: &Value| {
            let status = &row["construction"];
            status.is_null()
                || (status["unresolved"].is_null()
                    && status["held"] != "expanded"
                    && status["held"] != "invoked")
        };
        !held && (rows.into_iter().flatten()).any(|row| self.fitting(&row) && open(&row))
    }

    fn judged(&self, keys: &[String], state: &Value, prompt: &str) -> String {
        let offered = |key: &str| keys.iter().any(|k| k == key);
        let facts = &state["authoring"];
        let doubted = self.whole != Whole::Facts || self.wanting(facts);
        if offered("unfaithful") {
            return if doubted { "unfaithful" } else { "faithful" }.to_owned();
        }
        if offered("no_task") {
            self.pointed.lock().expect("pointed").push(keys.to_vec());
            let rows = facts["offered"]["components"].as_array().cloned();
            let rows = rows.unwrap_or_default();
            let named = keys.iter().find(|key| {
                let at = key.strip_prefix("component-").and_then(|k| k.parse().ok());
                at.and_then(|k: usize| rows.get(k))
                    .is_some_and(|row| self.fitting(row))
            });
            return match (self.why, named) {
                (Why::Component, Some(key)) => key.clone(),
                (Why::NoFit, _) if offered("no_fit") => "no_fit".to_owned(),
                (Why::Abstain, _) => "none".to_owned(),
                _ => "no_task".to_owned(),
            };
        }
        if offered("only_requested") {
            return "only_requested".to_owned();
        }
        if offered("consistent") {
            let write = |key: &&String| {
                (prompt.lines()).any(|line| {
                    line.starts_with(&format!("- {key}: ")) && line.contains("./out/report.json")
                })
            };
            let missed = (self.whole == Whole::RunMissesWrite)
                .then(|| keys.iter().filter(|k| k.starts_with("part-")).find(write))
                .flatten();
            return missed.map_or_else(|| "consistent".to_owned(), Clone::clone);
        }
        let foundry = (state["clause"]["text"].as_str()).is_some_and(|text| text.contains(FOUNDRY));
        if offered("missing") && foundry && doubted && !offered("unexercised") {
            return "missing".to_owned();
        }
        "carried".to_owned()
    }
}

impl nika_kernel::ai::provider::ProviderInferDyn for Builder {
    async fn infer(
        &self,
        request: nika_kernel::ai::provider::InferRequest,
    ) -> Result<nika_kernel::ai::provider::InferResponse, nika_kernel::ai::provider::ProviderError>
    {
        use nika_kernel::ai::provider::{
            ContentBlock, InferResponse, ProviderError, ResponseFormat, StopReason, TokenUsage,
        };
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            return Err(ProviderError::Other {
                reason: "a structured answer was expected".to_owned(),
            });
        };
        let text = if schema["properties"]["operations"].is_object() {
            let last = (request.messages.last()).map_or_else(String::new, |m| format!("{m:?}"));
            self.told.lock().expect("told").push(last);
            let mut answers = self.answers.lock().expect("answers");
            (!answers.is_empty())
                .then(|| answers.remove(0))
                .ok_or_else(|| ProviderError::Other {
                    reason: "the scripted author has no further answer".to_owned(),
                })?
        } else {
            let keys: Vec<String> = (schema["properties"]["choice"]["enum"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|key| key.as_str().map(str::to_owned))
                .collect();
            let state = state_of(&request).unwrap_or(Value::Null);
            self.facts
                .lock()
                .expect("facts")
                .push(state["authoring"].clone());
            let prompt: Vec<&str> = (request.messages.iter())
                .flat_map(|message| message.content.iter())
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            json!({"choice": self.judged(&keys, &state, &prompt.join("\n"))}).to_string()
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(10, 5),
            StopReason::EndTurn,
        ))
    }
}

/// The compile of `request` by `seat`, with `host`'s room and `catalog` lent.
async fn compiled(
    request: &crate::CompileRequest,
    seat: &Builder,
    host: Option<&dyn Rehearse>,
    catalog: &dyn ComponentCatalog,
) -> crate::CompileOutcome {
    let cognition = crate::Cognition {
        provider: Some(seat),
        seat: None,
    };
    crate::compile_with_cognition_composed(request, cognition, host, Some(catalog))
        .await
        .expect("compiles")
}

/// The verification attempts the outcome recorded, in order.
fn attempts(out: &crate::CompileOutcome) -> Vec<Value> {
    (out.provenance.decision.as_ref())
        .and_then(|decision| decision["semantic_verification"].as_array().cloned())
        .unwrap_or_default()
}

/// The roles of the document door's own authoring calls, in order.
fn authored(out: &crate::CompileOutcome) -> Vec<String> {
    calls(out)
        .into_iter()
        .filter(|call| call.starts_with("document"))
        .collect()
}

/// The first question a verification attempt asked in `role`.
fn question<'a>(attempt: &'a Value, role: &str) -> Option<&'a Value> {
    (attempt["questions"].as_array().into_iter().flatten()).find(|q| q["role"] == role)
}

/// The one defect a verdict located, when it is the conditional clause: its text and note.
fn located(attempt: &Value) -> (String, String) {
    let defects = attempt["defects"].as_array().cloned().unwrap_or_default();
    assert_eq!(defects.len(), 1, "{attempt:#}");
    let defect = defects[0].as_str().unwrap_or_default().to_owned();
    assert!(defect.contains(FOUNDRY), "{defect}");
    let note = (attempt["notes"].as_array().into_iter().flatten())
        .find(|n| n["defect"] == defect.as_str())
        .and_then(|n| n["note"].as_str())
        .unwrap_or_default()
        .to_owned();
    (defect, note)
}

/// The written document under a catalogue whose admitted component fits the request: the judge
/// finds the conditional clause missing and locates it in the construction, on the component the
/// bytes do not hold (never a runtime task). The defect names that component; the door's repair
/// composes it by reference with the request's bindings, and the new bytes hold it as admitted:
/// READY after one repair, the receipt and the current witness exact.
#[tokio::test]
async fn a_missing_construction_is_located_on_the_component_and_repaired_by_composition() {
    let author = Builder::new(
        vec![door(HAND_WRITTEN, &[]), door(ENVELOPE, &[compose(48)])],
        Whole::Facts,
        Why::Component,
        STALE,
    );
    let out = compiled(&stale_request(), &author, None, &Shelf).await;
    assert_eq!(
        out.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        out.diagnostics
    );
    assert_eq!(authored(&out), ["document", "document-repair"]);
    // The written bytes held no receipt: the offer stood open on them.
    let before = &author.first_facts()["offered"]["components"][0]["construction"];
    assert_eq!(before["held"], Value::Null, "{before:#}");
    assert!(before["unresolved"].is_null(), "{before:#}");
    // The first verdict located the clause in the construction: the component, by identity.
    let attempts = attempts(&out);
    let first = &attempts[0];
    let (_, note) = located(first);
    assert!(note.contains("`block:stale-filter-report`"), "{note}");
    assert!(note.contains("fixture-document-r1"), "{note}");
    let point = question(first, "judge_point").expect("the clause was located");
    assert_eq!(point["choice"], "component-0", "{point:#}");
    let keys = &author.pointed()[0];
    for key in ["component-0", "no_fit", "no_task"] {
        assert!(keys.iter().any(|k| k == key), "{key}: {keys:?}");
    }
    // The repair the author was told names the component, never a task.
    let repair = &author.told()[1];
    assert!(repair.contains("block:stale-filter-report"), "{repair}");
    let last = attempts.last().expect("a last verdict");
    assert_eq!(last["settled"], true, "{last:#}");
    // The final bytes are the composition, its receipt and witness exact.
    let candidate = out.candidate.clone().expect("a candidate");
    assert_eq!(candidate, expanded().await);
    let decision = out.provenance.decision.as_ref().expect("decision");
    let created = &decision["document_create"];
    assert_eq!(created["mode"], "composed", "{created:#}");
    assert_eq!(created["reuse"]["expanded"], 1, "{created:#}");
    let receipt = &created["components"][0];
    assert_eq!(receipt["component"]["id"], "block:stale-filter-report");
    assert_eq!(
        receipt["component"]["release"]["version"],
        "fixture-document-r1"
    );
    assert_eq!(receipt["candidate_sha256"], json!(sha256(&candidate)));
    let mut bound: Vec<(&str, &Value)> = (receipt["bindings"].as_array().into_iter().flatten())
        .map(|b| (b["path"].as_str().unwrap_or_default(), &b["bound"]))
        .collect();
    bound.sort_by_key(|(path, _)| *path);
    assert_eq!(
        bound,
        [
            ("const.max_age_hours", &json!(48)),
            ("const.records_path", &json!("./in/tickets.json")),
            ("const.report_path", &json!("./out/report.json")),
        ]
    );
    // The judge read the current witness on those bytes: held as admitted.
    let facts = author.last_facts();
    assert_eq!(facts["composed"][0]["verdict"], "expanded", "{facts:#}");
    let status = &facts["offered"]["components"][0]["construction"];
    assert_eq!(status["held"], "expanded", "{facts:#}");
    // Every other clause kept: the author's boundary, the named output, the request's literals.
    let read = nika_compile::surface::literal_projection(&candidate).expect("literal");
    let boundary = json!({"fs": {"read": ["./in/tickets.json"], "write": ["./out/report.json"]},
        "tools": ["nika:read", "nika:jq", "nika:write"]});
    assert_eq!(read["permits"], boundary);
    assert_eq!(read["outputs"]["stale"], "${{ tasks.stale.output }}");
    assert_eq!(read["const"]["max_age_hours"]["value"], 48);
}

/// A release offering two admitted components that resolve but serve another purpose: a jq-only
/// constant doubler (no tool beyond the request's), and a drafted recap posted to an endpoint
/// (a model call and a network send the request never asks).
struct Unrelated;

/// The jq-only component: it doubles a constant.
const DOUBLE: &str = r#"nika: probe-double
const:
  n: 2
permits:
  tools: ["nika:jq"]
tasks:
  doubled:
    invoke: { tool: "nika:jq", args: { input: "${{ const.n }}", expression: ". * 2" } }
outputs:
  doubled: ${{ tasks.doubled.output }}
"#;

/// The recap component: a model draft from a read file, posted.
const RECAP: &str = r#"nika: probe-recap
model: mock/echo
const:
  source_path: ./data/notes.json
  recap_endpoint: https://hooks.example.test/recap
permits:
  fs: { read: ["./data/notes.json"] }
  net: { http: ["hooks.example.test"] }
  tools: ["nika:read", "nika:fetch"]
tasks:
  notes:
    invoke: { tool: "nika:read", args: { path: "${{ const.source_path }}" } }
  draft:
    with: { notes: "${{ tasks.notes.output }}" }
    infer: { prompt: "Summarize: ${{ with.notes }}" }
  post:
    with: { recap: "${{ tasks.draft.output }}" }
    invoke: { tool: "nika:fetch", args: { url: "${{ const.recap_endpoint }}", method: "POST", body: "${{ with.recap }}" } }
"#;

impl ComponentCatalog for Unrelated {
    fn release(&self) -> Release {
        Release::new(
            "fixture-unrelated-r1",
            SNAPSHOT,
            "nika-knowledge-release-profile/r1",
        )
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        reference.block_name()?;
        pinned(reference, &self.release())?;
        let (source, file, callables): (&str, &str, &[&str]) = match reference.id.as_str() {
            "block:double-constant" => (DOUBLE, "blocks/double-constant.nika", &["nika:jq"]),
            "block:draft-and-post" => (
                RECAP,
                "blocks/draft-and-post.nika",
                &["nika:read", "nika:fetch"],
            ),
            _ => return Err(Unresolved::Unknown(reference.id.clone())),
        };
        let release = self.release();
        let mut component =
            Component::new(reference.id.clone(), release, file, sha256(source), source);
        component.callables = callables.iter().map(|c| (*c).to_owned()).collect();
        if source == RECAP {
            component.holes = vec![
                Hole::new("const.source_path", "human", None),
                Hole::new("const.recap_endpoint", "human", None),
            ];
        }
        Ok(component)
    }
    fn entries(&self) -> Vec<Value> {
        vec![
            json!({"id": "block:double-constant", "kind": "block",
                "title": "A constant doubled with jq", "purpose": "Double the constant n.",
                "holes": [], "effects": []}),
            json!({"id": "block:draft-and-post", "kind": "block",
                "title": "A drafted recap posted to an endpoint",
                "purpose": "Read notes, draft a recap with a model, POST it.",
                "holes": [{"name": "const.source_path", "owner": "human"},
                          {"name": "const.recap_endpoint", "owner": "human"}],
                "effects": ["fs.read", "model", "net.read"]}),
        ]
    }
}

/// A release that lists the stale-filter component but resolves no admitted bytes for it.
struct Unresolvable;

impl ComponentCatalog for Unresolvable {
    fn release(&self) -> Release {
        Shelf.release()
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        Err(Unresolved::Integrity(reference.id.clone()))
    }
    fn entries(&self) -> Vec<Value> {
        Shelf.entries()
    }
}

/// The tickets the synthetic room copies in, and the report the request asks of them: the rows
/// whose `age_hours` is strictly greater than 48, in their input order, counted.
const TICKETS: &str = r#"[{"id":"fresh-10","age_hours":10},{"id":"stale-60","age_hours":60},{"id":"boundary-48","age_hours":48},{"id":"stale-90","age_hours":90}]"#;
const REPORT: &str = r#"{"count":2,"ids":["stale-60","stale-90"]}"#;

/// A synthetic room (a `Rehearse` double): it records each candidate it is asked to rehearse and
/// answers with one fixed receipt of a completed run of it ([`ran`]). It executes nothing.
#[derive(Default)]
struct Room {
    shown: std::sync::Mutex<Vec<String>>,
}

impl Rehearse for Room {
    fn bound(&self) -> std::time::Duration {
        std::time::Duration::from_secs(10)
    }
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        _inputs: &'a [String],
        _targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            self.shown.lock().expect("shown").push(candidate.to_owned());
            ran(candidate)
        })
    }
}

/// The synthetic receipt of one completed run of `candidate`: the tickets copied in, the report
/// written, bound to the candidate's digest. No candidate is executed to produce it.
fn ran(candidate: &str) -> RehearsalReport {
    let (source, target) = ("./in/tickets.json", "./out/report.json");
    let input = Digest::of(TICKETS.as_bytes());
    let mut observed = Observation::none();
    observed.bounds = Bounds::new(10_000, 1_048_576, 65_536);
    let held = Held::Whole(TICKETS.to_owned());
    observed.copies = vec![CopyReceipt::new(source, input.clone(), Some(input), held)];
    let state = FinalState::File {
        digest: Digest::of(REPORT.as_bytes()),
        held: Held::Whole(REPORT.to_owned()),
    };
    observed.finals = vec![FinalReceipt::new(target, state)];
    observed.ledger = LedgerFacts::clean(vec![target.into()]);
    observed.spent = Spent::new(TICKETS.len() as u64, REPORT.len() as u64);
    RehearsalReport::new(
        Rehearsal::Passed {
            outputs: vec![RehearsedOutput::new(target, REPORT)],
        },
        Attempt::Completed { elapsed_ms: 2 },
        EffectCounts::none(),
        sha256(candidate),
    )
    .with_admitted_digest("synthetic-admission")
    .with_room(RoomEvidence::new(true, true))
    .with_observation(observed)
}

/// Nonempty but unrelated offers force nothing: the judge's contextual decision is that neither
/// fits (their contracts shown: the doubler's jq, the recap's model call and network send), the
/// author's complete document stands as written, READY after one call, with no composition, no
/// repair and the author's own boundary (no network, no model) untouched.
#[tokio::test]
async fn unrelated_offers_force_no_reuse_and_the_written_document_stands() {
    let author = Builder::new(vec![door(HAND_WRITTEN, &[])], Whole::Facts, Why::NoFit, &[]);
    let out = compiled(&stale_request(), &author, None, &Unrelated).await;
    assert_eq!(
        out.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        out.diagnostics
    );
    assert_eq!(authored(&out), ["document"]);
    assert_eq!(out.candidate.as_deref(), Some(HAND_WRITTEN));
    let created = &out.provenance.decision.as_ref().expect("decision")["document_create"];
    assert_eq!(created["mode"], "written", "{created:#}");
    assert_eq!(created["reuse"]["expanded"], 0, "{created:#}");
    let facts = author.last_facts();
    assert_eq!(facts["offered"]["total"], 2, "{facts:#}");
    let statuses: Vec<&Value> = (facts["offered"]["components"].as_array().into_iter())
        .flatten()
        .map(|row| &row["construction"])
        .collect();
    assert_eq!(
        statuses,
        [
            &json!({"held": null, "callables": ["nika:jq"]}),
            &json!({"held": null, "callables": ["nika:read", "nika:fetch"]}),
        ],
        "each contract examinable, nothing held"
    );
    let read = nika_compile::surface::literal_projection(HAND_WRITTEN).expect("literal");
    assert!(read["permits"].get("net").is_none());
}

/// A judge that doubts the whole request and finds the conditional clause missing, then decides
/// over the examinable offers that none fits: that clause alone is settled, its own alternative
/// standing (the record keeps the basis), with no defect and no repair. The whole-request doubt
/// stays: without a run nothing decides it (held after one call); a whole trial run of the same
/// bytes the judge finds consistent makes them READY, still with no composition.
#[tokio::test]
async fn a_contextual_no_fit_settles_only_the_conditional_clause() {
    for room in [None, Some(Room::default())] {
        let author = Builder::new(
            vec![door(HAND_WRITTEN, &[])],
            Whole::Doubting,
            Why::NoFit,
            &[],
        );
        let host = room.as_ref().map(|room| room as &dyn Rehearse);
        let out = compiled(&stale_request(), &author, host, &Unrelated).await;
        assert_eq!(authored(&out), ["document"], "no repair");
        let attempts = attempts(&out);
        let first = &attempts[0];
        assert_eq!(first["defects"], json!([]), "{first:#}");
        assert_eq!(first["unknown"], json!([]), "{first:#}");
        let point = question(first, "judge_point").expect("the clause was located");
        assert_eq!(point["choice"], "no_fit");
        let clause = point["clause"]["text"].as_str().unwrap_or_default();
        assert!(clause.contains(FOUNDRY), "{clause}");
        let basis = &point["construction"]["no_fit"];
        assert_eq!(basis["alternative_stands"], true, "{point:#}");
        assert_eq!(
            basis["offered"].as_array().map(Vec::len),
            Some(2),
            "{point:#}"
        );
        assert_eq!(
            author.pointed()[0],
            [
                "task-load_tickets",
                "task-keep_stale",
                "task-summarize",
                "task-save_report",
                "omitted",
                "component-0",
                "component-1",
                "no_fit",
                "no_task",
                "none",
            ]
        );
        match room {
            None => {
                assert_ne!(out.status, crate::CompileStatus::Ready);
                // Only the whole request stays contested, never the conditional clause.
                assert_eq!(first["contested"], json!([STALE_INTENT]), "{first:#}");
            }
            Some(room) => {
                assert_eq!(
                    out.status,
                    crate::CompileStatus::Ready,
                    "{:#?}",
                    out.diagnostics
                );
                assert_eq!(out.candidate.as_deref(), Some(HAND_WRITTEN));
                let shown = room.shown.lock().expect("shown").clone();
                assert_eq!(shown, vec![HAND_WRITTEN.to_owned()]);
                let observed = question(first, "judge_observed").expect("the run was judged");
                assert_eq!(observed["choice"], "consistent");
            }
        }
    }
}

/// The discriminator of a conditional fallback: `no_fit` settles only a clause that asks for an
/// admitted component when one applies. When the judge names the request's ordinary report write
/// as a part the trial run did not carry and then answers `no_fit`, nothing excuses the write:
/// the bytes stay held, the part unsettled, and no repair or composition follows.
#[tokio::test]
async fn a_no_fit_never_excuses_a_required_write_the_run_misses() {
    let room = Room::default();
    let author = Builder::new(
        vec![door(HAND_WRITTEN, &[])],
        Whole::RunMissesWrite,
        Why::NoFit,
        &[],
    );
    let host = Some(&room as &dyn Rehearse);
    let out = compiled(&stale_request(), &author, host, &Unrelated).await;
    assert_ne!(
        out.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        out.diagnostics
    );
    assert_eq!(authored(&out), ["document"], "no repair, no composition");
    let attempts = attempts(&out);
    let first = &attempts[0];
    let write = "and write a JSON object containing count and ids to ./out/report.json";
    let observed = question(first, "judge_observed").expect("the run was judged");
    let over_run = (first["questions"].as_array().into_iter().flatten())
        .find(|q| {
            (q["question"].as_str()).is_some_and(|id| id.starts_with("verify-observed-point-"))
        })
        .expect("the named part was localized over the run");
    assert_eq!(over_run["clause"]["text"], write, "{first:#}");
    let k = (over_run["question"].as_str()).and_then(|id| id.rsplit('-').next());
    assert_eq!(observed["choice"], json!(k.map(|k| format!("part-{k}"))));
    assert_eq!(over_run["choice"], "no_fit");
    assert_eq!(first["defects"], json!([]), "{first:#}");
    assert_eq!(
        first["unsettled"],
        json!(["the judge named a part in the trial run, then no offer fitting it"]),
        "{first:#}"
    );
}

/// An offer the catalogue cannot resolve cannot be examined: the judge's `no_fit` decides
/// nothing there (the clause stays unknown, its basis recorded, no alternative standing), and a
/// judge that makes no choice leaves it unknown too. Neither is a defect, a fallback or READY.
#[tokio::test]
async fn an_unexaminable_offer_or_no_choice_keeps_the_fit_unknown() {
    let cases: [(&dyn ComponentCatalog, Why, Option<bool>); 2] = [
        (&Unresolvable, Why::NoFit, Some(false)),
        (&Shelf, Why::Abstain, None),
    ];
    for (catalog, why, stands) in cases {
        let author = Builder::new(vec![door(HAND_WRITTEN, &[])], Whole::Doubting, why, STALE);
        let out = compiled(&stale_request(), &author, None, catalog).await;
        assert_ne!(out.status, crate::CompileStatus::Ready);
        assert_eq!(authored(&out), ["document"], "no repair");
        let first = &attempts(&out)[0];
        assert_eq!(first["defects"], json!([]), "{first:#}");
        let unknown = first["unknown"].as_array().cloned().unwrap_or_default();
        let open = |u: &Value| u.as_str().is_some_and(|u| u.contains(FOUNDRY));
        assert!(unknown.iter().any(open), "{first:#}");
        let point = question(first, "judge_point").expect("the clause was located");
        let recorded = &point["construction"]["no_fit"]["alternative_stands"];
        assert_eq!(recorded.as_bool(), stands, "{point:#}");
    }
}

/// Text copied from the component by hand is no composition: the repair rewrites the expansion's
/// exact bytes without composing, no receipt names them, the judge locates the same component
/// again, and the door stops on no progress, the copy kept, never READY.
#[tokio::test]
async fn a_hand_copy_of_the_component_is_never_its_composition() {
    let copy = expanded().await;
    let author = Builder::new(
        vec![door(HAND_WRITTEN, &[]), door(&copy, &[])],
        Whole::Facts,
        Why::Component,
        STALE,
    );
    let out = compiled(&stale_request(), &author, None, &Shelf).await;
    assert_ne!(out.status, crate::CompileStatus::Ready);
    assert_eq!(authored(&out), ["document", "document-repair"]);
    assert_eq!(out.candidate.as_deref(), Some(copy.as_str()));
    let facts = author.last_facts();
    assert_eq!(facts["composed"], json!([]), "{facts:#}");
    let status = &facts["offered"]["components"][0]["construction"];
    assert_eq!(status["held"], Value::Null, "{facts:#}");
    let attempts = attempts(&out);
    let (first, _) = located(&attempts[0]);
    let (again, note) = located(attempts.last().expect("a last verdict"));
    assert_eq!(first, again, "the same clause located again");
    assert!(note.contains("`block:stale-filter-report`"), "{note}");
    let route = out.provenance.decision.as_ref().expect("decision")["route"].to_string();
    assert!(route.contains("native: no progress"), "{route}");
}

/// A judge that names an offer whose contract reaches beyond what the request allows (a model
/// draft and a network send) only opens a repair: the composition it asks for still faces the
/// strict parser, Check and the fidelity laws, which refuse the ungranted send and the endpoint
/// no word of the request states. The receipt grants nothing, and nothing with that extra effect
/// is READY.
#[tokio::test]
async fn a_named_component_with_unasked_effects_is_refused_before_ready() {
    let post = json!({"op": "compose", "component": "block:draft-and-post",
        "version": "fixture-unrelated-r1",
        "bindings_json": "{\"const.source_path\": \"./in/tickets.json\", \"const.recap_endpoint\": \"https://hooks.example.test/recap\"}"});
    let author = Builder::new(
        vec![door(HAND_WRITTEN, &[]), door(ENVELOPE, &[post])],
        Whole::Facts,
        Why::Component,
        &["block:draft-and-post"],
    );
    let out = compiled(&stale_request(), &author, None, &Unrelated).await;
    assert_ne!(out.status, crate::CompileStatus::Ready);
    let (_, note) = located(&attempts(&out)[0]);
    assert!(note.contains("`block:draft-and-post`"), "{note}");
    assert!(note.contains("fs.read, model, net.read"), "{note}");
    let native = &out.provenance.decision.as_ref().expect("decision")["native"];
    let rounds = native["rounds"].as_array().cloned().unwrap_or_default();
    let composed = (rounds.iter())
        .find(|round| round["components"] == json!(["block:draft-and-post"]))
        .expect("the composition was made and judged");
    let refused = composed["diagnostics"].as_array().map_or(0, Vec::len);
    assert!(refused > 0, "the laws refused it: {composed:#}");
    let ready_with_send = (out.status == crate::CompileStatus::Ready)
        && out
            .candidate
            .as_deref()
            .is_some_and(|c| c.contains("nika:fetch"));
    assert!(!ready_with_send);
}
