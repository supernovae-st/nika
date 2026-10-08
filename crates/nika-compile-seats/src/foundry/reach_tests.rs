// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Whole-catalog reach: an admitted entry the lexical pack never held is asked by its
//! descriptor, resolved in full when the seat finds it applicable, then bound and expanded; a
//! distractor the seat judges unrelated stays out with its answer; an entry it cannot judge stays
//! a descriptor. The coverage account never calls a pack-only pass complete.

use std::sync::Mutex;

use nika_compile::surface::{initial, sha256};
use nika_compile::{AuthoringKnowledge, CompileRequest, KnowledgeReference};
use serde_json::{Value, json};

use super::component::{Component, ComponentCatalog, ComponentRef, Release, Unresolved, pinned};
use super::reach::{DESCRIPTOR, descriptor};
use super::{Binding, expand, instantiate, qualified_with, reused};
use crate::decide::{
    BatchFuture, ChoiceAnswer, ChoiceBatch, ChoiceFuture, ChoiceQuestion, DecisionError,
    DecisionSeat, NONE_OPTION, each_alone,
};

/// The request, in other words than any admitted entry's.
const INTENT: &str = "Liste les demandes en souffrance depuis plus de deux jours dans ./in/tickets.json et enregistre le bilan dans ./out/report.json";

const STALE: &str = "nika: p90-stale-filter-report\nconst:\n  records_path: ./data/tickets.json\n  report_path: ./out/stale.json\n  max_age_hours: { type: integer, value: 48 }\npermits:\n  fs: { read: [\"./data/tickets.json\"], write: [\"./out/stale.json\"] }\n  tools: [\"nika:read\", \"nika:jq\", \"nika:write\"]\ntasks:\n  read_records:\n    invoke: { tool: \"nika:read\", args: { path: \"${{ const.records_path }}\" } }\n  parse_records:\n    with: { raw: \"${{ tasks.read_records.output }}\" }\n    invoke: { tool: \"nika:jq\", args: { input: \"${{ with.raw }}\", expression: \"fromjson\" } }\n  stale:\n    with: { rows: \"${{ tasks.parse_records.output }}\", hours: \"${{ const.max_age_hours }}\" }\n    invoke: { tool: \"nika:jq\", args: { input: { rows: \"${{ with.rows }}\", hours: \"${{ with.hours }}\" }, expression: \"(.hours | tonumber) as $h | [.rows[] | select(.age_hours > $h)]\" } }\n  write_report:\n    with: { content: \"${{ tasks.stale.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"${{ const.report_path }}\", content: \"${{ with.content }}\" } }\noutputs:\n  stale: ${{ tasks.stale.output }}\n";

fn release() -> Release {
    Release::new(
        "fixture-reach-r1",
        "3".repeat(64),
        "nika-knowledge-release-profile/r1",
    )
}

/// The release's rows: the component nobody's words reach, a pattern, and a distractor that
/// shares the request's words.
fn rows() -> Vec<Value> {
    vec![
        json!({"id": "block:stale-filter-report", "kind": "block",
               "title": "Records older than a threshold, counted and written",
               "purpose": "Keep the records whose age_hours exceeds the threshold, write them.",
               "file": "blocks/stale-filter-report.nika", "file_sha256": sha256(STALE),
               "holes": [{"name": "const.records_path", "owner": "human"},
                         {"name": "const.report_path", "owner": "human"},
                         {"name": "const.max_age_hours", "owner": "human", "note": "hours"}],
               "effects": ["fs.read", "fs.write"], "interfaces": ["RecordTransformer"],
               "status": "EXPERIMENTAL", "proof_level": "CHECKED"}),
        json!({"id": "pattern:filter-by-age", "kind": "pattern",
               "title": "Filter by age", "purpose": "Compare an age field with a threshold."}),
        json!({"id": "block:demandes-bilan", "kind": "block",
               "title": "Liste les demandes et enregistre le bilan",
               "purpose": "demandes bilan enregistre liste", "file": "blocks/x.nika",
               "file_sha256": "0".repeat(64)}),
    ]
}

/// A release that lends every entry, the component's full text and the component itself.
struct Release1;

impl ComponentCatalog for Release1 {
    fn release(&self) -> Release {
        release()
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        reference.block_name()?;
        pinned(reference, &self.release())?;
        let row = rows()
            .into_iter()
            .find(|r| r["id"] == reference.id.as_str());
        let row = row.ok_or_else(|| Unresolved::Unknown(reference.id.clone()))?;
        Component::from_row(self.release(), &row, Some(STALE.as_bytes()))
    }
    fn entries(&self) -> Vec<Value> {
        rows()
    }
    fn reference(&self, id: &str) -> Option<KnowledgeReference> {
        (id == "block:stale-filter-report").then(|| KnowledgeReference {
            kind: "block".to_owned(),
            id: id.to_owned(),
            text: format!("Records older than a threshold\n```yaml\n{STALE}```"),
        })
    }
}

/// A seat that answers by reference id and keeps every question it was asked.
struct Judge(Mutex<Vec<ChoiceQuestion>>);

impl DecisionSeat for Judge {
    fn name(&self) -> &'static str {
        "test/judge"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.0.lock().unwrap().push(question.clone());
        let id = question.state["reference"]["id"]
            .as_str()
            .unwrap_or_default();
        let choice = match id {
            "block:stale-filter-report" | "skeleton:aggregate-by-key" => Ok("applies"),
            "pattern:filter-by-age" => Ok(NONE_OPTION),
            id if id == "block:demandes-bilan" || id.starts_with("skeleton:") => Ok("unrelated"),
            _ => Err("unscripted"),
        };
        Box::pin(async move {
            choice
                .map(|choice| ChoiceAnswer::new(choice, "test/judge"))
                .map_err(|why| DecisionError(why.to_owned()))
        })
    }
    fn choose_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchFuture<'a> {
        each_alone(batch, |question| self.choose(question))
    }
}

/// The lexical shortlist: only the distractor shares the request's words.
fn request() -> CompileRequest {
    let pack = AuthoringKnowledge {
        references: vec![KnowledgeReference {
            kind: "block".to_owned(),
            id: "block:demandes-bilan".to_owned(),
            text: "Liste les demandes et enregistre le bilan".to_owned(),
        }],
        ..AuthoringKnowledge::default()
    };
    CompileRequest::create(INTENT).with_authoring_knowledge(pack)
}

fn row<'a>(record: &'a Value, id: &str) -> &'a Value {
    let rows = record["references"].as_array().unwrap();
    rows.iter().find(|r| r["id"] == id).unwrap()
}

#[test]
fn a_descriptor_says_what_an_entry_is_for_and_never_carries_its_code() {
    let text = descriptor(&rows()[0]);
    assert!(text.starts_with(DESCRIPTOR), "{text}");
    assert!(text.contains("Records older than a threshold, counted and written — Keep"));
    assert!(text.contains("holes: const.records_path (human); const.report_path (human); const.max_age_hours (human: hours)"));
    assert!(text.contains("effects: fs.read, fs.write"));
    assert!(
        !text.contains("nika:jq") && !text.contains("tasks:"),
        "{text}"
    );
}

#[tokio::test]
async fn an_entry_no_word_reaches_is_asked_resolved_in_full_bound_and_witnessed() {
    let judge = Judge(Mutex::new(Vec::new()));
    let (shown, record) = qualified_with(INTENT, &request(), Some(&judge), Some(&Release1))
        .await
        .unwrap();
    let pack = shown.authoring_knowledge.as_ref().unwrap();
    // Asked in one batch, all by descriptor: the two entries the shortlist lacked, and the
    // recalled one the catalogue lists.
    let asked = judge.0.lock().unwrap().clone();
    let state = |id: &str| {
        let question = asked
            .iter()
            .find(|q| q.state["reference"]["id"] == id)
            .unwrap();
        question.state["reference"]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(state("block:stale-filter-report").starts_with(DESCRIPTOR));
    assert!(!state("block:stale-filter-report").contains("tasks:"));
    assert!(state("block:demandes-bilan").starts_with(DESCRIPTOR));
    // Shown: the applicable entry in full, the undecided one as its descriptor, not the distractor.
    let text = |id: &str| {
        pack.references
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.text.clone())
    };
    assert!(
        text("block:stale-filter-report")
            .unwrap()
            .contains("max_age_hours: { type: integer, value: 48 }")
    );
    assert!(
        text("pattern:filter-by-age")
            .unwrap()
            .starts_with(DESCRIPTOR)
    );
    assert_eq!(text("block:demandes-bilan"), None);
    let found = row(&record, "block:stale-filter-report");
    assert_eq!(
        (found["asked"].as_str(), found["shown"].as_str()),
        (Some("descriptor"), Some("full"))
    );
    let undecided = row(&record, "pattern:filter-by-age");
    assert_eq!(
        (undecided["verdict"].as_str(), undecided["shown"].as_str()),
        (Some("unqualified"), Some("descriptor"))
    );
    let distractor = row(&record, "block:demandes-bilan");
    assert_eq!(distractor["verdict"], "unrelated");
    assert_eq!(
        distractor["answer"]["choice"], "unrelated",
        "{distractor:#}"
    );
    assert_eq!(
        record["discarded_basis"]
            .as_str()
            .map(|b| b.contains("never a proven incompatibility")),
        Some(true)
    );
    let coverage = &record["coverage"];
    assert_eq!(coverage["complete"], true);
    assert_eq!(
        [
            &coverage["admitted_entries"],
            &coverage["already_in_pack"],
            &coverage["asked_by_descriptor"],
            &coverage["resolved_in_full"]
        ],
        [&json!(3), &json!(1), &json!(2), &json!(1)]
    );
    assert_eq!(coverage["recalled_asked_by_descriptor"], 1);
    assert_eq!(
        coverage["recalled_shown_in_full"], 0,
        "the distractor is discarded"
    );
    assert_eq!(record["questions"], json!(asked.len()));
    // The selected entry is an executable component: resolved, bound, expanded, witnessed.
    let component = Release1
        .resolve(&ComponentRef::new("block:stale-filter-report").at_version("fixture-reach-r1"))
        .unwrap();
    let bindings = [
        Binding::new("const.records_path", json!("./in/tickets.json")),
        Binding::new("const.report_path", json!("./out/report.json")),
        Binding::new("const.max_age_hours", json!(48)),
    ];
    let parent = "nika: demandes-en-souffrance\npermits:\n  fs: { read: [\"./in/tickets.json\"], write: [\"./out/report.json\"] }\n  tools: [\"nika:read\", \"nika:jq\", \"nika:write\"]\ntasks: {}\n";
    let expansion = expand(parent, &instantiate(&component, &bindings).unwrap()).unwrap();
    assert!(expansion.ready, "{:#}", expansion.receipt["check"]);
    let mut out = initial();
    out.candidate = Some(expansion.candidate.clone());
    reused(
        &shown,
        record,
        std::slice::from_ref(&expansion.receipt),
        &mut out,
    );
    let reuse = &out.provenance.decision.as_ref().unwrap()["knowledge_qualification"]["reuse"];
    assert_eq!(reuse["expanded"], 1, "{reuse:#}");
    let used = (reuse["references"].as_array().unwrap().iter())
        .find(|r| r["id"] == "block:stale-filter-report")
        .unwrap();
    assert_eq!(used["use"], "expanded");
}

#[tokio::test]
async fn every_embedded_template_is_reached_beside_the_release_never_only_the_top_hits() {
    let judge = Judge(Mutex::new(Vec::new()));
    let (shown, record) = qualified_with(INTENT, &request(), Some(&judge), Some(&Release1))
        .await
        .unwrap();
    let templates = &record["coverage"]["embedded_templates"];
    let listed = templates["admitted_entries"].as_u64().unwrap();
    assert_eq!(listed, nika_pack::template_names().len() as u64);
    assert_eq!(
        listed,
        templates["already_in_pack"].as_u64().unwrap()
            + templates["asked_by_descriptor"].as_u64().unwrap()
    );
    let pack = shown.authoring_knowledge.as_ref().unwrap();
    let text = |id: &str| (pack.references.iter().find(|r| r.id == id)).map(|r| r.text.clone());
    // The applicable template in full, an unrelated one out with the seat's answer.
    let aggregate = text("skeleton:aggregate-by-key").unwrap();
    assert!(
        aggregate.starts_with("nika: aggregate-by-key"),
        "{aggregate}"
    );
    assert_eq!(text("skeleton:deduplicate-records"), None);
    assert_eq!(
        row(&record, "skeleton:deduplicate-records")["verdict"],
        "unrelated"
    );
}

#[tokio::test]
async fn without_a_catalogue_or_a_seat_the_record_says_what_was_never_asked() {
    let judge = Judge(Mutex::new(Vec::new()));
    let (shown, record) = qualified_with(INTENT, &request(), Some(&judge), None)
        .await
        .unwrap();
    assert_eq!(record["coverage"]["complete"], false);
    assert!(
        record["coverage"]["why"]
            .as_str()
            .unwrap()
            .contains("never a candidate")
    );
    let pack = shown.authoring_knowledge.as_ref().unwrap();
    assert!(
        !pack
            .references
            .iter()
            .any(|r| r.id == "block:stale-filter-report")
    );
    // A catalogue without a seat: every entry shown, the widened ones as descriptors.
    let (shown, record) = qualified_with(INTENT, &request(), None, Some(&Release1))
        .await
        .unwrap();
    let pack = shown.authoring_knowledge.as_ref().unwrap();
    let component = pack
        .references
        .iter()
        .find(|r| r.id == "block:stale-filter-report")
        .unwrap();
    assert!(component.text.starts_with(DESCRIPTOR));
    assert_eq!(record["coverage"]["complete"], true);
    assert_eq!(record["coverage"]["resolved_in_full"], 0);
    assert_eq!(record["by"], Value::Null);
}

/// A recalled reference as the lexical pack presents it: its full text, never a descriptor.
fn recalled(id: &str, kind: &str) -> KnowledgeReference {
    KnowledgeReference {
        kind: kind.to_owned(),
        id: id.to_owned(),
        text: format!("{id} in full: the body the pack recalled"),
    }
}

#[tokio::test]
async fn a_recalled_entry_is_judged_by_its_descriptor_and_read_whole_unless_discarded() {
    // Recalled: an applicable entry, an undecided one, a discarded one, one the seat fails on
    // (absent from its script), and one the catalogue does not list.
    let pack = AuthoringKnowledge {
        references: vec![
            recalled("block:stale-filter-report", "block"),
            recalled("pattern:filter-by-age", "pattern"),
            recalled("block:demandes-bilan", "block"),
            recalled("example:elsewhere", "example"),
        ],
        ..AuthoringKnowledge::default()
    };
    let request = CompileRequest::create(INTENT).with_authoring_knowledge(pack.clone());
    let judge = Judge(Mutex::new(Vec::new()));
    let (shown, record) = qualified_with(INTENT, &request, Some(&judge), Some(&Release1))
        .await
        .unwrap();
    let asked = judge.0.lock().unwrap().clone();
    let state = |id: &str| {
        (asked.iter().find(|q| q.state["reference"]["id"] == id))
            .and_then(|q| q.state["reference"]["text"].as_str().map(str::to_owned))
            .unwrap()
    };
    for id in [
        "block:stale-filter-report",
        "pattern:filter-by-age",
        "block:demandes-bilan",
    ] {
        assert!(
            state(id).starts_with(DESCRIPTOR),
            "{id} asked by its descriptor"
        );
        assert!(!state(id).contains("the body the pack recalled"), "{id}");
    }
    assert_eq!(
        state("example:elsewhere"),
        "example:elsewhere in full: the body the pack recalled"
    );
    // Read: the applicable, the undecided and the failed one whole, exactly as recalled; the
    // discarded one out.
    let reader = shown.authoring_knowledge.as_ref().unwrap();
    let text = |id: &str| (reader.references.iter().find(|r| r.id == id)).map(|r| r.text.clone());
    for id in [
        "block:stale-filter-report",
        "pattern:filter-by-age",
        "example:elsewhere",
    ] {
        assert_eq!(
            text(id),
            Some(format!("{id} in full: the body the pack recalled")),
            "{id}"
        );
    }
    assert_eq!(text("block:demandes-bilan"), None);
    let shown_as = |id: &str| {
        let found = row(&record, id);
        (
            found["verdict"].clone(),
            found["asked"].clone(),
            found["shown"].clone(),
        )
    };
    assert_eq!(
        shown_as("block:stale-filter-report"),
        (json!("applies"), json!("descriptor"), json!("full"))
    );
    assert_eq!(
        shown_as("pattern:filter-by-age"),
        (json!("unqualified"), json!("descriptor"), json!("full"))
    );
    assert_eq!(
        shown_as("example:elsewhere"),
        (json!("unqualified"), json!("full"), json!("full"))
    );
    assert_eq!(row(&record, "block:demandes-bilan")["asked"], "descriptor");
    assert_eq!(record["coverage"]["recalled_asked_by_descriptor"], 3);
    assert_eq!(record["coverage"]["recalled_shown_in_full"], 2);
    // Without a seat nothing is judged, and every recalled entry is read whole as recalled.
    let (unjudged, _) = qualified_with(INTENT, &request, None, Some(&Release1))
        .await
        .unwrap();
    let unjudged = unjudged.authoring_knowledge.unwrap();
    for reference in &pack.references {
        let kept = unjudged
            .references
            .iter()
            .find(|r| r.id == reference.id)
            .unwrap();
        assert_eq!(kept.text, reference.text, "{}", reference.id);
    }
}
