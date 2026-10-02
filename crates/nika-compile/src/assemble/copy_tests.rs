// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One copy of one text file to another, lowered two ways from the same plan: its text read
//! and written back, or its bytes read as an opaque envelope the write decodes. Pure: the plan
//! is the reader's own, every candidate stays in memory, nothing reads or writes a file and no
//! provider is called. That the two programs copy the same bytes is a run's to show, the
//! host's; here each is shown to be its own candidate, crossing its own laws and Check.

use nika_compile_fidelity::candidate::plan_of_document;
use serde_json::{Value, json};

use super::read::{copy_document, copy_plan};
use super::{CopyLowering, Doc, assemble_judged, assemble_lowered};
use crate::plan::{Binding, Effect, EffectPolicy, EffectVerb, Op, Plan, Step};
use crate::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, HotPolicy, compile, gates,
    initial, lexicon, outcome_document, rules, shape,
};

const INTENT: &str = "Copy ./in/source.txt as is to ./out/copied.txt";
const SOURCE: &str = "./in/source.txt";
const TARGET: &str = "./out/copied.txt";
const COUNT: &str = "read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json";

/// The plan the deterministic door assembles for `intent` (the reader's, with the approval
/// backstop and the stated rules promoted), the folded intent, and whether the strict HOT door
/// admits it.
fn reader_plan(intent: &str) -> (Plan, String, bool) {
    let folded = lexicon::fold_apostrophes(intent);
    let mut reading = lexicon::read(&folded);
    gates::backstop(&folded, &mut reading.plan);
    shape::promote_stated_rules(&mut reading.plan, &folded);
    let admitted = crate::doors::admit_hot(&folded, &reading, HotPolicy::Strict).is_ok();
    (reading.plan, folded, admitted)
}

/// One assembly of `plan` under `lowering`, on its own outcome.
fn assembled_as(plan: &Plan, intent: &str, lowering: CopyLowering) -> CompileOutcome {
    let mut out = initial();
    let request = CompileRequest::create(intent);
    assemble_lowered(plan, intent, &request, &[], false, lowering, &mut out).expect("assembles");
    out
}

/// The candidate an outcome holds, as a document.
fn document(out: &CompileOutcome) -> Value {
    let candidate = out.candidate.as_deref().expect("a candidate");
    serde_yaml_bw::from_str(candidate).expect("the candidate is YAML")
}

/// The byte lowering refused: no candidate, a `lowering` refusal, the status refused.
fn assert_refused(out: &CompileOutcome, what: &str) {
    assert_eq!(
        out.status,
        CompileStatus::Refused,
        "{what}: {:?}",
        out.diagnostics
    );
    assert!(out.candidate.is_none(), "{what}");
    assert!(
        out.diagnostics
            .iter()
            .any(|finding| finding.kind == DiagnosticKind::Refused && finding.target == "lowering"),
        "{what}: {:?}",
        out.diagnostics
    );
}

/// The byte lowering's document before the laws, built by hand: the binary read of `source`
/// and the one write of `target` fed that whole read, with their paths and tools.
fn copy_doc(source: &str, target: &str) -> Doc {
    let mut d = Doc::new("compiled-workflow", false);
    d.root["const"] = json!({"source_path": source, "output_path": target});
    d.root["tasks"]["read_source"] = json!({"invoke": {
        "tool": "nika:read",
        "args": {"path": "${{ const.source_path }}", "binary": true},
    }});
    d.root["tasks"]["write_output"] = json!({
        "after": {"read_source": "success"},
        "with": {"content": "${{ tasks.read_source.output }}"},
        "invoke": {"tool": "nika:write", "args": {
            "path": "${{ const.output_path }}",
            "content": "${{ with.content }}",
            "create_dirs": true,
            "overwrite": true,
        }},
    });
    d.root["outputs"] = json!({"write_status": "${{ tasks.write_output.status }}"});
    d.tools.extend(["nika:read", "nika:write"]);
    d.reads.push(json!(source));
    d.writes.push(json!(target));
    d
}

#[test]
fn the_byte_lowering_reads_the_same_copy_as_bytes_and_crosses_its_own_check() {
    let (plan, intent, admitted) = reader_plan(INTENT);
    assert!(admitted, "the strict door admits the copy");
    assert!(copy_plan(&plan), "{plan:?}");
    let text = assembled_as(&plan, &intent, CopyLowering::Text);
    let bytes = assembled_as(&plan, &intent, CopyLowering::Bytes);
    for out in [&text, &bytes] {
        assert_eq!(out.status, CompileStatus::Ready, "{:?}", out.diagnostics);
        let preview = out
            .check_preview
            .as_ref()
            .expect("the candidate was checked");
        assert!(preview.report.is_clean(), "{:?}", out.diagnostics);
    }
    let (a, b) = (document(&text), document(&bytes));
    // The read differs, and so do the bytes and the identity of each candidate.
    assert_eq!(
        a["tasks"]["read_source"]["invoke"]["args"],
        json!({"path": "${{ const.source_path }}"})
    );
    assert_eq!(
        b["tasks"]["read_source"]["invoke"]["args"],
        json!({"path": "${{ const.source_path }}", "binary": true})
    );
    let (one, other) = (
        text.candidate.as_deref().unwrap_or_default(),
        bytes.candidate.as_deref().unwrap_or_default(),
    );
    assert_ne!(one, other);
    assert_ne!(crate::surface::sha256(one), crate::surface::sha256(other));
    // Everything else is the same program: tasks, paths, permits, write, outputs.
    let mut unbinary = b.clone();
    let args = unbinary["tasks"]["read_source"]["invoke"]["args"]
        .as_object_mut()
        .expect("the read's arguments");
    assert_eq!(args.remove("binary"), Some(json!(true)));
    assert_eq!(a, unbinary);
    assert_eq!(
        a["permits"],
        json!({"tools": ["nika:read", "nika:write"], "fs": {"read": [SOURCE], "write": [TARGET]}})
    );
    assert_eq!(
        b["tasks"]["write_output"]["with"],
        json!({"content": "${{ tasks.read_source.output }}"})
    );
    // The plan each candidate states by its structure is the same plan.
    assert_eq!(plan_of_document(&a), plan_of_document(&b));
}

#[test]
fn the_text_lowering_is_the_existing_assembly() {
    assert_eq!(CopyLowering::default(), CopyLowering::Text);
    for intent in [INTENT, COUNT] {
        let (plan, folded, _) = reader_plan(intent);
        let request = CompileRequest::create(folded.as_str());
        let mut judged = initial();
        assemble_judged(&plan, &folded, &request, &[], false, &mut judged).expect("assembles");
        let text = assembled_as(&plan, &folded, CopyLowering::Text);
        assert_eq!(
            outcome_document(&text),
            outcome_document(&judged),
            "{intent}"
        );
    }
    // The compile door's own candidate is the text lowering's, byte for byte.
    let compiled = compile(&CompileRequest::create(INTENT)).expect("compiles");
    assert_eq!(
        compiled.status,
        CompileStatus::Ready,
        "{:?}",
        compiled.diagnostics
    );
    let (plan, folded, _) = reader_plan(INTENT);
    let text = assembled_as(&plan, &folded, CopyLowering::Text);
    assert_eq!(compiled.candidate, text.candidate);
}

#[test]
fn the_byte_lowering_is_refused_outside_an_exact_text_copy() {
    // A count, a copy of a file that is no text by its suffix, a copy into JSON.
    for intent in [
        COUNT,
        "Copy ./in/logo.png as is to ./out/logo.png",
        "Copy ./in/source.json as is to ./out/copied.json",
    ] {
        let (plan, folded, _) = reader_plan(intent);
        assert_refused(&assembled_as(&plan, &folded, CopyLowering::Bytes), intent);
    }
    // A copy whose plan carries anything else is refused on its plan alone, before anything is
    // assembled: no ledger, no question.
    let (plan, folded, _) = reader_plan(INTENT);
    let mut gated = plan.clone();
    gated.effects[0].policy = EffectPolicy::HumanFirst;
    let mut constrained = plan;
    constrained.constraints.push("keep it short".to_owned());
    for (what, other) in [("a gate", gated), ("a constraint", constrained)] {
        let out = assembled_as(&other, &folded, CopyLowering::Bytes);
        assert_refused(&out, what);
        assert!(
            out.provenance.decision.is_none(),
            "{what}: nothing assembled"
        );
        assert!(out.questions.is_empty(), "{what}");
    }
}

#[test]
fn the_plan_predicate_holds_only_one_read_and_its_one_automatic_write() {
    let (plan, _, _) = reader_plan(INTENT);
    assert!(copy_plan(&plan), "{plan:?}");
    let mut gated = plan.clone();
    gated.effects[0].policy = EffectPolicy::HumanFirst;
    let mut alone = plan.clone();
    alone.effects[0].alone = true;
    let mut twice = plan.clone();
    twice.effects.push(Effect::new(
        EffectVerb::Write,
        "./out/again.txt",
        INTENT,
        EffectPolicy::Automatic,
    ));
    let mut computed = plan.clone();
    computed
        .steps
        .push(Step::new(Op::Compute, INTENT, "a summary", Vec::new()));
    let mut ruled = plan.clone();
    ruled
        .rules
        .push(rules::synthesize("keep the rows where status is paid", &[]).expect("a rule"));
    let mut bound = plan.clone();
    bound.bindings.push(Binding::new("content", "hello"));
    let mut triggered = plan.clone();
    triggered.trigger = Some("every morning".to_owned());
    let mut unknown = plan;
    unknown.unknowns.push("and archive the old one".to_owned());
    for (what, other) in [
        ("a gate", gated),
        ("a value alone", alone),
        ("a second write", twice),
        ("a computation", computed),
        ("a rule", ruled),
        ("a content binding", bound),
        ("a trigger", triggered),
        ("an unknown", unknown),
    ] {
        assert!(!copy_plan(&other), "{what}");
    }
}

#[test]
fn the_document_predicate_holds_only_the_whole_envelope_between_two_text_files() {
    assert!(copy_document(&copy_doc(SOURCE, TARGET)));
    let mut split = copy_doc(SOURCE, TARGET);
    split.root["tasks"]["write_output"]["with"]["content"] =
        json!("${{ tasks.read_source.output.bytes_base64 }}");
    let mut staged = copy_doc(SOURCE, TARGET);
    staged.root["tasks"]["decode"] = json!({"invoke": {"tool": "nika:jq", "args": {}}});
    let mut extra = copy_doc(SOURCE, TARGET);
    extra.root["const"]["note"] = json!("kept");
    let mut text_read = copy_doc(SOURCE, TARGET);
    text_read.root["tasks"]["read_source"]["invoke"]["args"] =
        json!({"path": "${{ const.source_path }}"});
    let mut modelled = copy_doc(SOURCE, TARGET);
    modelled.root["model"] = json!("mock/echo");
    let mut fetching = copy_doc(SOURCE, TARGET);
    fetching.hosts.push("example.com".to_owned());
    for (what, d) in [
        ("an envelope split", split),
        ("a stage between", staged),
        ("a third constant", extra),
        ("a text read", text_read),
        ("a model", modelled),
        ("a host", fetching),
        ("one file twice", copy_doc(SOURCE, "in/source.txt")),
        (
            "a file no text",
            copy_doc("./in/logo.png", "./out/logo.png"),
        ),
        ("a JSON file", copy_doc("./in/a.json", "./out/b.json")),
    ] {
        assert!(!copy_document(&d), "{what}");
    }
}

#[test]
fn each_lowering_records_its_own_ledger_over_the_same_paths() {
    // The same plan under each lowering asks nothing and records its own ledger.
    let (plan, folded, _) = reader_plan(INTENT);
    for lowering in [CopyLowering::Text, CopyLowering::Bytes] {
        let out = assembled_as(&plan, &folded, lowering);
        assert!(out.questions.is_empty(), "{lowering:?}");
        let decision = out.provenance.decision.as_ref().expect("a decision");
        assert!(decision.get("ledger").is_some(), "{lowering:?}: {decision}");
        assert_eq!(
            document(&out)["const"],
            json!({"source_path": SOURCE, "output_path": TARGET}),
            "{lowering:?}"
        );
    }
}
