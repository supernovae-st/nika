// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Conversational revisions of a base no semantic record binds preserve the full base. One
//! destination is replaced in place: the seat states the typed link (the original clause that
//! states the destination, the change clause that states its new path) and the additions, the
//! compiler writes every byte and proves the complete document: only that destination's parsed
//! slots change, every task, calculation, option and other destination survives, and the author's
//! formatting stays. Additions, invented destinations, a wrong or missing link and a genuine
//! ambiguity never inherit that proof. The answer round replays the revision with zero calls on
//! the very base it revised; the next revision binds the revised bytes.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, NativeMode,
    compile,
};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;

mod common;
use common::{Judged, Rotating, held_for_its_judge};

const ORIGINAL: &str = "Copie entree.txt dans a.txt.";
const CHANGE: &str = "Finalement, utilise b.txt.";
/// The reader's clauses of [`ORIGINAL`] and [`CHANGE`].
const COPY: &str = "Copie entree.txt dans a.txt";
const USE_B: &str = "Finalement, utilise b.txt";
/// The accepted base of the campaign's DIALOG-06, verbatim.
const BASE: &str = r#"nika: copie-entree-vers-a
inputs: {}
const: {}
secrets: {}
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["entree.txt"]
    write: ["a.txt"]
run: {}
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args:
        path: "entree.txt"
  write_dest:
    with:
      content: "${{ tasks.read_source.output }}"
    invoke:
      tool: "nika:write"
      args:
        path: "a.txt"
        content: "${{ with.content }}"
        overwrite: true
        create_dirs: false
outputs: {}
"#;

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Only)
        .with_repairs(2)
}

/// The seat's only answer: one typed link and the additions.
fn links(replaces: &str, by: &str, adds: &[&str]) -> String {
    json!({"supersedes": [{"replaces": replaces, "by": by}], "adds": adds, "notes": "revised"})
        .to_string()
}

fn revise(change: &str) -> CompileRequest {
    CompileRequest::edit(BASE, change)
        .with_original_intent(ORIGINAL)
        .with_authoring_policy(policy())
}

fn rounds(out: &CompileOutcome) -> Vec<Value> {
    out.provenance.decision.as_ref().unwrap()["native"]["rounds"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// A revision the compiler did not keep: no candidate, no record, the reason named.
fn refused(out: &CompileOutcome, needle: &str) {
    assert_ne!(out.status, CompileStatus::Ready, "{needle}: {out:#?}");
    assert!(out.candidate.is_none(), "{needle}: {out:#?}");
    assert!(
        out.provenance
            .plan
            .as_ref()
            .is_none_or(|p| p.get("superseded").is_none()),
        "{needle}: {out:#?}"
    );
    assert!(
        (out.diagnostics.iter()).any(|d| d.message.contains(needle)),
        "{needle}: {out:#?}"
    );
}

#[tokio::test]
async fn the_campaign_revision_replaces_the_destination_and_supersedes_the_old_path() {
    let provider = Rotating::new(vec![links(COPY, USE_B, &[])]);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&revise(CHANGE), &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1, "{out:#?}");
    // The compiler wrote the bytes: the base with only the destination's slots changed, its
    // formatting, options and every other field kept.
    let candidate = out.candidate.as_deref().unwrap();
    assert_eq!(candidate, BASE.replace("a.txt", "b.txt"), "{candidate}");
    let rounds = rounds(&out);
    assert_eq!(rounds[0]["diagnostics"], json!([]), "{rounds:#?}");
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(record["gaps"], json!([]), "{record:#}");
    assert_eq!(record["superseded"][0]["path"], "a.txt");
    assert_eq!(record["superseded"][0]["by"], "b.txt");
    assert_eq!(
        record["source_revision"]["base_sha256"],
        nika_compile::surface::sha256(BASE)
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Applied
                && d.target == "revision"
                && d.message.contains("`a.txt` is replaced by `b.txt`")),
        "{out:#?}"
    );
    // The answer round replays the record with zero calls: the same candidate, then
    // held for the round's judge (R4 A11, step 2): this keyless round permits none.
    let answered = CompileRequest::edit(BASE, CHANGE)
        .with_original_intent(ORIGINAL)
        .with_plan(record);
    let replayed = compile(&answered).unwrap();
    let revised = nika_compile::revise_intent(&answered).unwrap();
    assert!(held_for_its_judge(&replayed, &revised), "{replayed:#?}");
    assert_eq!(replayed.candidate, out.candidate);
}

#[tokio::test]
async fn an_english_replacement_of_a_rooted_path_is_proven_the_same_way() {
    let base = "nika: copy-source\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"./in/source.txt\"]\n    write: [\"./out/first.txt\"]\ntasks:\n  read_source:\n    invoke:\n      tool: \"nika:read\"\n      args:\n        path: \"./in/source.txt\"\n  write_copy:\n    with:\n      content: \"${{ tasks.read_source.output }}\"\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"./out/first.txt\"\n        content: \"${{ with.content }}\"\n        overwrite: true\n        create_dirs: true\n";
    let original = "Copy ./in/source.txt to ./out/first.txt";
    let provider = Rotating::new(vec![links(original, "Use ./out/second.txt instead", &[])]);
    let request = CompileRequest::edit(base, "Use ./out/second.txt instead.")
        .with_original_intent(original)
        .with_authoring_policy(policy());
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        out.candidate.as_deref(),
        Some(base.replace("first", "second").as_str())
    );
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(
        record["superseded"][0]["path"], "./out/first.txt",
        "{record:#}"
    );
    assert_eq!(
        record["superseded"][0]["by"], "./out/second.txt",
        "{record:#}"
    );
}

/// Short of a proof the base is kept: an addition the seat misread as a replacement is held by
/// the round's independent judge, never silently READY; a change that names no new path is kept
/// with its limitation and no call; a link to a clause the request never states is refused.
#[tokio::test]
async fn short_of_proof_the_base_is_kept_and_nothing_is_ready() {
    // An addition the seat misread as a replacement: the substitution is proven, the request it
    // answers is not, and this round's judge (no approving double) does not carry it.
    let provider = Rotating::new(vec![links(COPY, "Écris aussi dans b.txt", &[])]);
    let out = compile_with_provider(&revise("Écris aussi dans b.txt."), &provider)
        .await
        .unwrap();
    assert_ne!(
        out.status,
        CompileStatus::Ready,
        "an addition is never a replacement: {out:#?}"
    );
    // A change unrelated to any path, the seat linking it to the copy anyway: one typed reading
    // call (choice A), a link the change does not state proves nothing, the base kept.
    let provider = Rotating::new(vec![links(COPY, USE_B, &[])]);
    let out = compile_with_provider(&revise("Finalement, utilise un ton formel."), &provider)
        .await
        .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        (out.diagnostics.iter()).any(|d| d.message.contains("is not a clause the change states")),
        "{out:#?}"
    );
}

#[tokio::test]
async fn unproven_links_and_creation_gaps_do_not_waive_paths() {
    // A link whose original clause the request never states proves nothing.
    let provider = Rotating::new(vec![links("Copie entree.txt dans b.txt", USE_B, &[])]);
    let out = compile_with_provider(&revise(CHANGE), &provider)
        .await
        .unwrap();
    refused(&out, "is not a clause the original request states once");
    // A missing link: the change's clause is neither a supersession nor an addition.
    let provider = Rotating::new(vec![
        json!({"supersedes": [], "adds": [], "notes": "none"}).to_string(),
    ]);
    let out = compile_with_provider(&revise(CHANGE), &provider)
        .await
        .unwrap();
    refused(&out, "neither supersedes a clause with it nor adds it");
    // A creation opens every stated path; a gap waives none. Fresh CREATE is semantic: the
    // sketch reads the source, writes nothing and reports the write as a gap.
    let sketch = json!({"name": "copie-entree", "tasks": [
        {"id": "read_source", "verb": "invoke", "tool": "nika:read", "reads": ["entree.txt"], "purpose": "the source"}
    ], "questions": [], "gaps": ["`a.txt` cannot be written."], "notes": "no write"});
    let created = Rotating::new(vec![
        sketch.to_string(),
        json!({"fills": [], "notes": "none"}).to_string(),
    ]);
    let policy = AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(0);
    let out = compile_with_provider(
        &CompileRequest::create(ORIGINAL).with_authoring_policy(policy),
        &created,
    )
    .await
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let judged = rounds(&out);
    assert!(!judged.is_empty(), "{out:#?}");
    assert!(
        judged.iter().any(|r| {
            let said = r["diagnostics"].to_string();
            said.contains("UNREALIZED PATH") && said.contains("a.txt")
        }),
        "the owed write is named: {out:#?}"
    );
}

const FILTER_TOTAL: &str = include_str!("fixtures/compile/revision-filter-total.nika");
const FILTER_INTENT: &str = "Lis commandes.csv, écris uniquement les commandes confirmées dans commandes-confirmees.csv et leur montant total sous forme de nombre dans total.txt.";
const FILTER_CHANGE: &str = "Écris finalement les commandes dans commandes-finales.csv ; conserve le filtre et le total dans total.txt.";
/// The reader's clauses of [`FILTER_INTENT`] and [`FILTER_CHANGE`].
const WRITES: &str = "écris uniquement les commandes confirmées dans commandes-confirmees.csv et leur montant total sous forme de nombre dans total.txt";
const FINAL: &str = "Écris finalement les commandes dans commandes-finales.csv";
const KEEP_TOTAL: &str = "conserve le filtre et le total dans total.txt";

fn filter_revision(change: &str) -> CompileRequest {
    CompileRequest::edit(FILTER_TOTAL, change)
        .with_original_intent(FILTER_INTENT)
        .with_authoring_policy(policy())
}

#[tokio::test]
async fn a_destination_replacement_preserves_the_recalled_total_and_full_computation() {
    let revised = FILTER_TOTAL.replace("commandes-confirmees.csv", "commandes-finales.csv");
    let provider = Rotating::new(vec![links(WRITES, FINAL, &[KEEP_TOTAL])]);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(
        &filter_revision(FILTER_CHANGE),
        &Judged::approving(&provider),
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    // Every task, calculation, option and the other destination survive, byte for byte.
    assert_eq!(out.candidate.as_deref(), Some(revised.as_str()));
    let emitted =
        nika_compile::surface::literal_projection(out.candidate.as_deref().unwrap()).unwrap();
    let expected = nika_compile::surface::literal_projection(&revised).unwrap();
    assert_eq!(
        emitted, expected,
        "every task, calculation and other destination survives"
    );
    let record = out.provenance.plan.as_ref().unwrap();
    assert_eq!(record["superseded"][0]["by"], "commandes-finales.csv");
    assert_eq!(
        record["source_revision"]["slots"],
        json!(["/const/confirmed_path", "/permits/fs/write/0"])
    );
}

#[tokio::test]
async fn a_replacement_cannot_use_its_path_proof_to_change_a_calculation_or_add_a_destination() {
    // The seat cannot write a calculation: it names clauses only. An invented destination is no
    // clause the change states, and a clause left unaccounted for proves nothing.
    for (stated, needle) in [
        (
            links(
                WRITES,
                "Écris finalement les commandes dans invented.csv",
                &[KEEP_TOTAL],
            ),
            "is not a clause the change states",
        ),
        (
            links(WRITES, FINAL, &[]),
            "neither supersedes a clause with it nor adds it",
        ),
    ] {
        let provider = Rotating::new(vec![stated]);
        let out = compile_with_provider(&filter_revision(FILTER_CHANGE), &provider)
            .await
            .unwrap();
        refused(&out, needle);
    }
}

#[tokio::test]
async fn a_recalled_path_cannot_be_silently_replaced_instead_of_the_requested_destination() {
    // Linking the kept total as the change clause names no new destination; it never replaces
    // the requested one.
    let provider = Rotating::new(vec![links(WRITES, KEEP_TOTAL, &[FINAL])]);
    let out = compile_with_provider(&filter_revision(FILTER_CHANGE), &provider)
        .await
        .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.provenance
            .plan
            .as_ref()
            .is_none_or(|p| p.get("superseded").is_none())
    );
}

/// A genuine ambiguity: the replaced clause states two destinations and the change keeps neither
/// by name, so which one it replaces is not stated: the revision asks ONE bounded choice among
/// those exact paths (no candidate, no guess); its answer round decides with zero calls, held for
/// its judge.
#[tokio::test]
async fn a_clause_with_two_destinations_and_no_kept_one_asks_which_never_guesses() {
    let change = "Écris finalement les commandes dans commandes-finales.csv.";
    let provider = Rotating::new(vec![links(WRITES, FINAL, &[])]);
    let out = compile_with_provider(&filter_revision(change), &provider)
        .await
        .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(out.questions.len(), 1, "one bounded question: {out:#?}");
    let question = &out.questions[0];
    assert_eq!(question.key, "revision.destination");
    let offered: Vec<&str> = question.options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(offered, ["commandes-confirmees.csv", "total.txt"]);
    let record = out
        .provenance
        .plan
        .clone()
        .expect("the pending question record");
    // The answer round: the human chooses; nothing is asked of a seat.
    let answered = CompileRequest::edit(FILTER_TOTAL, change)
        .with_original_intent(FILTER_INTENT)
        .with_plan(record.clone())
        .answer("revision.destination", r#""commandes-confirmees.csv""#);
    let replayed = compile(&answered).unwrap();
    let revised = nika_compile::revise_intent(&answered).unwrap();
    assert!(held_for_its_judge(&replayed, &revised), "{replayed:#?}");
    assert_eq!(
        replayed.candidate.as_deref(),
        Some(
            FILTER_TOTAL
                .replace("commandes-confirmees.csv", "commandes-finales.csv")
                .as_str()
        )
    );
    // An answer outside the offered paths, or another base, never decides.
    let wrong = CompileRequest::edit(FILTER_TOTAL, change)
        .with_original_intent(FILTER_INTENT)
        .with_plan(record.clone())
        .answer("revision.destination", r#""commandes.csv""#);
    assert!(compile(&wrong).unwrap().candidate.is_none());
    let moved = CompileRequest::edit(FILTER_TOTAL.replace("add // 0", "add"), change)
        .with_original_intent(FILTER_INTENT)
        .with_plan(record)
        .answer("revision.destination", r#""commandes-confirmees.csv""#);
    assert!(compile(&moved).unwrap().candidate.is_none());
}

#[tokio::test]
async fn a_replacement_never_duplicates_the_write_and_keeps_every_other_destination() {
    let faithful = FILTER_TOTAL.replace("commandes-confirmees.csv", "commandes-finales.csv");
    let provider = Rotating::new(vec![links(WRITES, FINAL, &[KEEP_TOTAL])]);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(
        &filter_revision(FILTER_CHANGE),
        &Judged::approving(&provider),
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let candidate = out.candidate.as_deref().unwrap();
    assert_eq!(candidate, faithful);
    assert!(!candidate.contains("commandes-confirmees.csv"));
    assert_eq!(
        candidate.matches("nika:write\"").count(),
        FILTER_TOTAL.matches("nika:write\"").count()
    );
}

/// An ADDITION of a destination keeps both writes: the seat states the destination whose written
/// content the new one receives (`like`); the compiler copies that write under a new id with the
/// new path and appends its permit, every calculation and other duty proven unchanged.
#[tokio::test]
async fn an_addition_keeps_both_destinations() {
    let change =
        "Écris aussi les commandes dans commandes-finales.csv ; conserve le total dans total.txt.";
    let stated = json!({"supersedes": [],
        "adds": ["Écris aussi les commandes dans commandes-finales.csv", "conserve le total dans total.txt"],
        "like": "commandes-confirmees.csv", "notes": "addition"})
    .to_string();
    let provider = Rotating::new(vec![stated]);
    let out = compile_with_provider(&filter_revision(change), &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.as_deref().unwrap();
    assert!(
        candidate.contains("commandes-confirmees.csv")
            && candidate.contains("commandes-finales.csv"),
        "{candidate}"
    );
    // Exactly the base plus the copied write and its permit: every other field unchanged.
    let base = nika_compile::surface::literal_projection(FILTER_TOTAL).unwrap();
    let revised = nika_compile::surface::literal_projection(candidate).unwrap();
    let mut expected = base.clone();
    let mut copy = base["tasks"]["write_confirmed"].clone();
    copy["invoke"]["args"]["path"] = json!("commandes-finales.csv");
    expected["tasks"]["write_confirmed_2"] = copy;
    expected["permits"]["fs"]["write"]
        .as_array_mut()
        .unwrap()
        .push(json!("commandes-finales.csv"));
    assert_eq!(revised, expected, "{candidate}");
    let record = out.provenance.plan.as_ref().unwrap();
    assert_eq!(record["source_revision"]["edit"], "add");
    assert_eq!(record["superseded"], json!([]));
}

/// An addition whose copied write the facts leave open (two destinations, no `like`) asks which
/// one, bounded to the base's destinations; on a base writing one destination the facts settle it.
#[tokio::test]
async fn an_addition_asks_which_write_it_copies_only_when_the_facts_leave_it_open() {
    let change =
        "Écris aussi les commandes dans commandes-finales.csv ; conserve le total dans total.txt.";
    let stated = json!({"supersedes": [],
        "adds": ["Écris aussi les commandes dans commandes-finales.csv", "conserve le total dans total.txt"],
        "notes": "addition"})
    .to_string();
    let provider = Rotating::new(vec![stated]);
    let out = compile_with_provider(&filter_revision(change), &provider)
        .await
        .unwrap();
    assert!(out.candidate.is_none(), "{out:#?}");
    let keys: Vec<&str> = out.questions.iter().map(|q| q.key.as_str()).collect();
    assert_eq!(keys, ["revision.like"], "{out:#?}");
    let answered = CompileRequest::edit(FILTER_TOTAL, change)
        .with_original_intent(FILTER_INTENT)
        .with_plan(out.provenance.plan.clone().unwrap())
        .answer("revision.like", r#""commandes-confirmees.csv""#);
    let replayed = compile(&answered).unwrap();
    let revised = nika_compile::revise_intent(&answered).unwrap();
    assert!(held_for_its_judge(&replayed, &revised), "{replayed:#?}");
    assert!(
        replayed
            .candidate
            .unwrap()
            .contains("commandes-finales.csv")
    );
    // One destination: nothing to ask.
    let change = "Écris aussi dans b.txt.";
    let provider = Rotating::new(vec![
        json!({"supersedes": [], "adds": ["Écris aussi dans b.txt"], "notes": "addition"})
            .to_string(),
    ]);
    let out = compile_with_provider(&revise(change), &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.as_deref().unwrap();
    assert!(
        candidate.contains("a.txt") && candidate.contains("b.txt"),
        "{candidate}"
    );
}

/// An explicit « Remplace X par Y » is replaced on its typed link and the complete-document
/// proof; without that link the base is kept (the old path is never dropped on words alone).
#[tokio::test]
async fn an_explicit_old_path_is_replaced_only_on_its_typed_link_and_proof() {
    let change = "Remplace commandes-confirmees.csv par commandes-finales.csv ; conserve le total dans total.txt.";
    let replace = "Remplace commandes-confirmees.csv par commandes-finales.csv";
    let keep = "conserve le total dans total.txt";
    let provider = Rotating::new(vec![links(WRITES, replace, &[keep])]);
    let out = compile_with_provider(&filter_revision(change), &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(record["superseded"][0]["path"], "commandes-confirmees.csv");
    // The answer round replays it, held for the round's judge (R4 A11, step 2).
    let answered = CompileRequest::edit(FILTER_TOTAL, change)
        .with_original_intent(FILTER_INTENT)
        .with_plan(record);
    let resumed = compile(&answered).unwrap();
    let revised = nika_compile::revise_intent(&answered).unwrap();
    assert!(held_for_its_judge(&resumed, &revised), "{resumed:#?}");
    assert!(
        !resumed
            .candidate
            .unwrap()
            .contains("commandes-confirmees.csv")
    );
    // No typed link: the « Remplace » clause stated as an addition replaces nothing.
    let provider = Rotating::new(vec![
        json!({"supersedes": [], "adds": [replace, keep], "notes": "no link"}).to_string(),
    ]);
    let out = compile_with_provider(&filter_revision(change), &provider)
        .await
        .unwrap();
    refused(&out, "replaces and adds no destination");
}

/// A recorded revision replays only on the base it revised: an altered base, or a record whose
/// candidate is not the substitution of its base, is refused; the next revision binds the
/// revised bytes and the words they answer.
#[tokio::test]
async fn a_stale_base_is_refused_and_the_next_revision_binds_the_revised_bytes() {
    let provider = Rotating::new(vec![links(COPY, USE_B, &[])]);
    let out = compile_with_provider(&revise(CHANGE), &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let record = out.provenance.plan.clone().unwrap();
    let revised = out.candidate.clone().unwrap();
    // An answer round on another base: refused, never the recorded candidate.
    let altered = BASE.replace("overwrite: true", "overwrite: false");
    let stale = CompileRequest::edit(altered, CHANGE)
        .with_original_intent(ORIGINAL)
        .with_plan(record.clone());
    let replayed = compile(&stale).unwrap();
    assert!(replayed.candidate.is_none(), "{replayed:#?}");
    assert!(
        (replayed.diagnostics.iter()).any(|d| d.message.contains("does not bind this base")),
        "{replayed:#?}"
    );
    // A tampered record: its candidate is not the substitution of its base.
    let mut tampered = record.clone();
    tampered["source"] = json!(revised.replace("overwrite: true", "overwrite: false"));
    let request = CompileRequest::edit(BASE, CHANGE)
        .with_original_intent(ORIGINAL)
        .with_plan(tampered);
    let replayed = compile(&request).unwrap();
    assert!(replayed.candidate.is_none(), "{replayed:#?}");
    // The next revision: the revised bytes with their record, a new change in words.
    let again = "Finalement, utilise c.txt.";
    let provider = Rotating::new(vec![links(USE_B, "Finalement, utilise c.txt", &[])]);
    let next = CompileRequest::edit(revised.as_str(), again)
        .with_original_intent(ORIGINAL)
        .with_plan(record.clone())
        .with_authoring_policy(policy());
    let out = compile_with_provider(&next, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        out.candidate.as_deref(),
        Some(BASE.replace("a.txt", "c.txt").as_str())
    );
    // The same next change on bytes the record did not write: refused, no call.
    let provider = Rotating::new(vec![links(USE_B, "Finalement, utilise c.txt", &[])]);
    let foreign = CompileRequest::edit(BASE, again)
        .with_original_intent(ORIGINAL)
        .with_plan(record)
        .with_authoring_policy(policy());
    let out = compile_with_provider(&foreign, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
}

struct RevisionContext(Rotating);
impl nika_kernel::ai::provider::ProviderInferDyn for RevisionContext {
    async fn infer(
        &self,
        request: nika_kernel::ai::provider::InferRequest,
    ) -> Result<nika_kernel::ai::provider::InferResponse, nika_kernel::ai::provider::ProviderError>
    {
        let messages = format!("{:?}", request.messages);
        assert!(messages.contains("reader_observations_before_applying_change"));
        assert!(!messages.contains("facts_the_compiler_holds_you_to"));
        assert!(messages.contains("base_candidate") && messages.contains("change"));
        self.0.infer(request).await
    }
}

#[tokio::test]
async fn the_first_revision_call_carries_observations_without_imposing_superseded_destinations() {
    let provider = RevisionContext(Rotating::new(vec![links(WRITES, FINAL, &[KEEP_TOTAL])]));
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(
        &filter_revision(FILTER_CHANGE),
        &Judged::approving(&provider),
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
}

/// A change the reader reads as no effect (« keep a copy of the result in ./c.md ») is a
/// destination addition only when the seat types it so, naming the write it copies (`like`): the
/// typed answer decides, the compiler proves; without it nothing is guessed.
#[tokio::test]
async fn a_typed_like_makes_an_addition_of_a_clause_the_reader_reads_as_no_effect() {
    let change = "also keep a copy of the result in ./c.md";
    let typed = json!({"supersedes": [], "adds": [change], "like": "a.txt", "notes": "copy"});
    let provider = Rotating::new(vec![typed.to_string()]);
    let out = compile_with_provider(&revise(change), &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.as_deref().unwrap();
    assert!(
        // The new path in the base's own style (`c.md` beside `a.txt`): the same relative path.
        candidate.contains("path: \"a.txt\"") && candidate.contains("path: \"c.md\""),
        "{candidate}"
    );
    let untyped = json!({"supersedes": [], "adds": [change], "notes": "copy"});
    let provider = Rotating::new(vec![untyped.to_string()]);
    let out = compile_with_provider(&revise(change), &provider)
        .await
        .unwrap();
    refused(&out, "replaces and adds no destination");
}

/// A change that targets a destination but names no new path: the typed link replaces the
/// written destination, so the revision asks the new path (a bounded text question), never
/// invents one; its answer, a new relative path, is proven with zero calls and held for its
/// judge. An escaping, absolute, already named or empty answer, or another base, never decides.
#[tokio::test]
async fn a_destination_change_without_a_path_asks_the_path_and_proves_the_answer() {
    let change = "Change the destination file.";
    let provider = Rotating::new(vec![links(COPY, "Change the destination file", &[])]);
    let out = compile_with_provider(&revise(change), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(out.questions.len(), 1, "{out:#?}");
    let question = &out.questions[0];
    assert_eq!(question.key, "revision.path");
    assert_eq!(question.answer_type, nika_compile::QuestionType::Text);
    assert!(question.options.is_empty());
    let record = out
        .provenance
        .plan
        .clone()
        .expect("the pending question record");
    let answered = |base: &str, path: &str| {
        let request = CompileRequest::edit(base, change)
            .with_original_intent(ORIGINAL)
            .with_plan(record.clone())
            .answer("revision.path", json!(path).to_string());
        compile(&request).unwrap()
    };
    let replayed = answered(BASE, "b.txt");
    let request = CompileRequest::edit(BASE, change).with_original_intent(ORIGINAL);
    let revised = nika_compile::revise_intent(&request).unwrap();
    assert!(held_for_its_judge(&replayed, &revised), "{replayed:#?}");
    assert_eq!(
        replayed.candidate.as_deref(),
        Some(BASE.replace("a.txt", "b.txt").as_str())
    );
    for wrong in ["../b.txt", "/tmp/b.txt", "entree.txt", "", "two words.txt"] {
        assert!(
            answered(BASE, wrong).candidate.is_none(),
            "`{wrong}` never decides"
        );
    }
    let moved = BASE.replace("overwrite: true", "overwrite: false");
    assert!(
        answered(&moved, "b.txt").candidate.is_none(),
        "another base never decides"
    );
}

/// A path absent from the change is supplied by the human, retained in the resolved request,
/// and can be replaced in a later revision. Both revisions are judged and keep every other
/// byte; an old conversation goal cannot replace the first revision's recorded meaning.
#[tokio::test]
async fn a_second_revision_uses_the_path_answered_when_the_first_change_named_none() {
    let change = "Change the destination file.";
    let provider = Rotating::new(vec![links(COPY, "Change the destination file", &[])]);
    let asked = compile_with_provider(&revise(change), &provider)
        .await
        .unwrap();
    assert!(asked.candidate.is_none(), "{asked:#?}");
    assert_eq!(asked.questions[0].key, "revision.path");
    let answered = revise(change)
        .with_plan(asked.provenance.plan.unwrap())
        .answer("revision.path", json!("b.txt").to_string());
    let first = compile_with_provider(&answered, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(first.status, CompileStatus::Ready, "{first:#?}");
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "answer did not re-author"
    );
    let record = first.provenance.plan.clone().unwrap();
    let resolved = record["source_revision"]["resolved"].as_str().unwrap();
    assert_eq!(
        resolved,
        "Change the destination file (destination: b.txt)."
    );
    assert_eq!(
        record["source_revision"]["change"], change,
        "human words retained"
    );
    assert_eq!(
        first.candidate.as_deref(),
        Some(BASE.replace("a.txt", "b.txt").as_str())
    );
    let next = "Use c.txt instead.";
    let provider = Rotating::new(vec![links(
        resolved.trim_end_matches('.'),
        "Use c.txt instead",
        &[],
    )]);
    let request = CompileRequest::edit(first.candidate.unwrap(), next)
        .with_original_intent(ORIGINAL)
        .with_plan(record)
        .with_authoring_policy(policy());
    let second = compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(second.status, CompileStatus::Ready, "{second:#?}");
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "one typed link revision"
    );
    assert_eq!(
        second.candidate.as_deref(),
        Some(BASE.replace("a.txt", "c.txt").as_str())
    );
}

/// An unreadable base is kept and no seat is asked. A readable base that writes nothing has no
/// destination link to state: it is revised over its complete document (one seat call), and an
/// answer stated as destination links there is refused, with no candidate. With no seat at all,
/// nothing is revised.
#[tokio::test]
async fn a_base_writing_nothing_is_revised_over_its_document_and_an_unreadable_one_asks_no_seat() {
    let read_only = "nika: lire\npermits:\n  tools: [\"nika:read\"]\n  fs:\n    read: [\"entree.txt\"]\ntasks:\n  read_source:\n    invoke:\n      tool: \"nika:read\"\n      args:\n        path: \"entree.txt\"\n";
    for (base, calls) in [(read_only, 1), ("nika: [broken", 0)] {
        let provider = Rotating::new(vec![links(COPY, USE_B, &[])]);
        let request = CompileRequest::edit(base, CHANGE)
            .with_original_intent(ORIGINAL)
            .with_authoring_policy(policy());
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert_eq!(
            provider.calls.load(Ordering::SeqCst),
            calls,
            "{base}: {out:#?}"
        );
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        // A readable base is kept with no candidate, the links refused where no destination
        // edit applies; an unreadable one is the core's own finding.
        if base == read_only {
            assert!(out.candidate.is_none(), "{out:#?}");
            assert!(
                (out.diagnostics.iter()).any(|d| d.message.contains("no destination edit applies")),
                "{out:#?}"
            );
        }
    }
    // No seat: nothing is revised (the core keeps its own deterministic outcome, the base
    // unchanged when it carries one).
    let unseated =
        compile(&CompileRequest::edit(BASE, CHANGE).with_original_intent(ORIGINAL)).unwrap();
    assert_ne!(unseated.status, CompileStatus::Ready, "{unseated:#?}");
    assert!(
        unseated.candidate.is_none() || unseated.candidate.as_deref() == Some(BASE),
        "{unseated:#?}"
    );
}
