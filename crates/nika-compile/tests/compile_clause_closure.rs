// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! S0 clause closure (R4 95): a material clause of the request never disappears.
//! - A contradiction between a requested effect and a prohibition of it stays unresolved until
//!   a human chooses: no candidate, no effect, no seat call, whatever the authoring strategy
//!   (the 2026-09-27 black-box audit A07 « write 'hello' to ./a.txt but do not write anything »
//!   was resolved by a seat into a write, BUG-U5).
//! - A ban scoped to another object, or to what the content says, is no contradiction.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, NativeMode,
    compile,
};
use nika_compile_cognition::compile_with_provider;
use nika_compile_reader::lexicon;
use nika_compile_reader::plan::{EffectPolicy, EffectVerb};
use std::sync::atomic::Ordering;
use std::time::Duration;

mod common;
use common::Rotating;

/// The A07 contradiction and its neighbours: a requested write and a ban of every write.
const CONTRADICTIONS: &[&str] = &[
    "write 'hello' to ./a.txt but do not write anything",
    "Write 'hello' to ./a.txt, but don't write anything at all.",
    "write \"hello\" to ./a.txt and never write anything",
    "write 'hello' to ./a.txt but write nothing",
    "écris « bonjour » dans ./a.txt mais n'écris rien",
    "Écris 'bonjour' dans ./a.txt, mais ne rien écrire.",
];

/// A request over the E14 fixtures as the CLI observes them (R4 S1: an unobserved key is asked).
fn observing(intent: &str) -> CompileRequest {
    CompileRequest::create(intent).with_knowledge(common::e14_world())
}

/// The write effects the reader states for a request, with their policies.
fn writes(intent: &str) -> Vec<EffectPolicy> {
    lexicon::read(&lexicon::fold_apostrophes(intent))
        .plan
        .effects
        .iter()
        .filter(|e| e.verb == EffectVerb::Write)
        .map(|e| e.policy)
        .collect()
}

/// The contradiction is refused and stated: no candidate, the conflict names the write and both
/// clauses, and nothing asks for a model, an endpoint or a path to hide it (R4 95 forbids an
/// unrelated question).
fn refused_as_contradiction(intent: &str, out: &CompileOutcome) {
    assert_eq!(out.status, CompileStatus::Refused, "{intent}: {out:#?}");
    assert!(out.candidate.is_none(), "{intent}: {out:#?}");
    let stated = out.diagnostics.iter().find(|d| {
        d.message
            .starts_with("Contradictory instructions for `write`")
    });
    assert!(
        stated.is_some(),
        "{intent}: the contradiction is stated: {out:#?}"
    );
    let stated = stated.unwrap();
    assert_eq!(stated.kind, DiagnosticKind::RequiresHuman, "{intent}");
    // Both sides stay visible as exact excerpts of the request: the request and its ban.
    let sides = stated
        .message
        .split_once("prohibited (")
        .and_then(|(_, rest)| rest.split_once("). The contradiction"))
        .map(|(evidence, _)| evidence.split(" / ").collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(sides.len(), 2, "{intent}: {}", stated.message);
    for side in sides {
        assert!(
            !side.trim().is_empty() && lexicon::fold_apostrophes(intent).contains(side),
            "{intent}: `{side}` is no excerpt of the request"
        );
    }
    for key in out.questions.iter().map(|q| q.key.as_str()) {
        assert!(
            !matches!(key, "model" | "const.endpoint_url" | "const.output_path"),
            "{intent}: an unrelated question hides the conflict: {key}"
        );
    }
}

#[test]
fn a_requested_write_beside_a_ban_of_every_write_is_a_contradiction() {
    for intent in CONTRADICTIONS {
        assert_eq!(writes(intent), [EffectPolicy::Conflict], "{intent}");
        let out = compile(&CompileRequest::create(*intent)).unwrap();
        refused_as_contradiction(intent, &out);
    }
}

#[test]
fn a_ban_scoped_to_another_object_or_to_the_content_is_no_contradiction() {
    // What the content says is a content instruction, never a ban of the write.
    let content =
        writes("Summarize ./notes.md into ./summary.md; do not write anything about salaries.");
    assert!(!content.contains(&EffectPolicy::Conflict), "{content:?}");
    assert!(content.contains(&EffectPolicy::Automatic), "{content:?}");
    // « nothing else » bounds the workflow's shape; it bans no requested write.
    let shape = writes("write 'hello' to ./a.txt and do not write anything else");
    assert!(!shape.contains(&EffectPolicy::Conflict), "{shape:?}");
    // Another file banned beside the requested one: a request and a targeted ban.
    let other = writes("write 'hello' to ./a.txt and never write to ./b.txt");
    assert!(!other.contains(&EffectPolicy::Conflict), "{other:?}");
    assert!(
        other.contains(&EffectPolicy::Automatic) && other.contains(&EffectPolicy::Forbidden),
        "{other:?}"
    );
    // A ban alone, nothing requested: a refused effect, never a contradiction.
    let alone = writes("Read ./notes.md and do not write anything.");
    assert_eq!(alone, [EffectPolicy::Forbidden], "{alone:?}");
}

/// Every effect policy the reader states for a request, whatever the verb.
fn policies(intent: &str) -> Vec<EffectPolicy> {
    lexicon::read(&lexicon::fold_apostrophes(intent))
        .plan
        .effects
        .iter()
        .map(|e| e.policy)
        .collect()
}

/// A negation inside quoted content is what the workflow writes, never an instruction to it:
/// neither a ban nor a contradiction, whatever the quotes, the language, or a connector or a
/// full stop inside the quoted text.
/// Each is paired with its metamorphic neighbour: the same request with neutral content reads
/// the same effects.
const QUOTED_NEGATIONS: &[(&str, &str)] = &[
    (
        "write 'do not write anything' to ./a.txt",
        "write 'hello' to ./a.txt",
    ),
    (
        "Write \"never write anything\" to ./a.txt.",
        "Write \"hello\" to ./a.txt.",
    ),
    (
        "Write “don't write anything” to ./a.txt.",
        "Write “hello” to ./a.txt.",
    ),
    (
        "écris « n'écris rien » dans ./a.txt",
        "écris « bonjour » dans ./a.txt",
    ),
    (
        "write 'hello, but do not write anything' to ./a.txt",
        "write 'hello, and goodbye' to ./a.txt",
    ),
    (
        "write 'Stop. Never write anything.' to ./a.txt",
        "write 'Stop. Hello.' to ./a.txt",
    ),
];

#[test]
fn a_negation_inside_quoted_content_is_content_never_a_ban() {
    for (intent, neutral) in QUOTED_NEGATIONS {
        let found = policies(intent);
        assert_eq!(found, policies(neutral), "{intent} reads as {neutral}");
        assert!(
            !found.contains(&EffectPolicy::Forbidden) && !found.contains(&EffectPolicy::Conflict),
            "{intent}: {found:?}"
        );
        assert!(
            found.contains(&EffectPolicy::Automatic),
            "{intent}: {found:?}"
        );
        let out = compile(&CompileRequest::create(*intent)).unwrap();
        assert_ne!(out.status, CompileStatus::Refused, "{intent}: {out:#?}");
        assert!(
            !out.diagnostics.iter().any(|d| {
                d.message.starts_with("Contradictory instructions")
                    || d.message.contains("Prohibited effect")
            }),
            "{intent}: {out:#?}"
        );
    }
    // A quoted object of the positive form is the content, never « write nothing ».
    let quoted_nothing = writes("Write 'nothing'.");
    assert!(
        !quoted_nothing.contains(&EffectPolicy::Forbidden),
        "{quoted_nothing:?}"
    );
}

#[test]
fn a_gate_phrase_inside_quoted_content_is_no_gate() {
    for (intent, neutral) in [
        (
            "write 'ask me before sending anything' to ./a.txt",
            "write 'hello' to ./a.txt",
        ),
        (
            "Write \"only after my approval\" to ./a.txt.",
            "Write \"hello\" to ./a.txt.",
        ),
        (
            "Écris « demande-moi avant d'envoyer quoi que ce soit » dans ./a.txt",
            "Écris « bonjour » dans ./a.txt",
        ),
    ] {
        let found = policies(intent);
        assert_eq!(found, policies(neutral), "{intent} reads as {neutral}");
        assert_eq!(found, [EffectPolicy::Automatic], "{intent}");
    }
}

#[test]
fn a_negation_inside_quoted_source_data_bans_nothing() {
    // What a source says, quoted in the request, is data the workflow reads or filters by;
    // each reads as its neighbour with neutral quoted data.
    for (intent, neutral) in [
        (
            "Read ./rules.txt, which says 'never email anyone', and email a summary to ops@example.test.",
            "Read ./rules.txt, which says 'hello', and email a summary to ops@example.test.",
        ),
        (
            "Read ./in.txt, keep the lines containing 'do not reply', and write them to ./out.txt.",
            "Read ./in.txt, keep the lines containing 'hello', and write them to ./out.txt.",
        ),
        (
            "Lis ./regles.txt (elle dit « n'envoie jamais rien ») et envoie un résumé à ops@example.test.",
            "Lis ./regles.txt (elle dit « bonjour ») et envoie un résumé à ops@example.test.",
        ),
    ] {
        let found = policies(intent);
        assert_eq!(found, policies(neutral), "{intent} reads as {neutral}");
        assert!(
            !found.contains(&EffectPolicy::Forbidden) && !found.contains(&EffectPolicy::Conflict),
            "{intent}: {found:?}"
        );
        assert!(
            found.contains(&EffectPolicy::Automatic),
            "{intent}: {found:?}"
        );
    }
}

#[test]
fn a_governing_negation_beside_quoted_content_still_bans() {
    // The metamorphic neighbours: the same words outside the quotes govern the workflow.
    let both = writes("write 'do not write anything' to ./a.txt but do not write anything");
    assert_eq!(both, [EffectPolicy::Conflict], "{both:?}");
    let out = compile(&CompileRequest::create(
        "write 'do not write anything' to ./a.txt but do not write anything",
    ))
    .unwrap();
    refused_as_contradiction(
        "write 'do not write anything' to ./a.txt but do not write anything",
        &out,
    );
    assert_eq!(
        policies("Read ./rules.txt and never email anyone."),
        [EffectPolicy::Forbidden]
    );
    assert_eq!(
        policies("Lis ./regles.txt et n'envoie jamais rien."),
        [EffectPolicy::Forbidden]
    );
}

fn policy(native: NativeMode) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(native)
        .with_repairs(3)
}

/// A seat that would realize the write if it were ever asked.
fn seat() -> Rotating {
    Rotating::new(vec![
        serde_json::json!({
            "candidate": "nika: hello\npermits:\n  tools: [\"nika:write\"]\n  fs: { write: [\"./a.txt\"] }\ntasks:\n  write:\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./a.txt\", content: \"hello\" }\n",
            "questions": [], "gaps": [], "notes": "write hello"
        })
        .to_string(),
    ])
}

/// A reviewed base that copies one file to another.
const BASE: &str = r#"nika: copy-entree
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./entree.txt"]
    write: ["./a.txt"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: { path: "./entree.txt" }
  write_dest:
    with: { content: "${{ tasks.read_source.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./a.txt", content: "${{ with.content }}", overwrite: true }
"#;

/// A change in words that asks for a write and bans every write is refused before a seat
/// revises the base (the 2026-09-27 audit S04: a contradiction sent to a paid revision).
#[tokio::test]
async fn a_revision_whose_words_contradict_themselves_never_reaches_a_seat() {
    for change in [
        "Also write 'hello' to ./c.txt, but do not write anything.",
        "Écris aussi « bonjour » dans ./c.txt, mais n'écris rien.",
    ] {
        let provider = seat();
        let request = CompileRequest::edit(BASE, change)
            .with_original_intent("Copy ./entree.txt to ./a.txt.")
            .with_authoring_policy(policy(NativeMode::Only));
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert_eq!(
            provider.calls.load(Ordering::SeqCst),
            0,
            "{change}: a paid revision"
        );
        // The change is read beside the original intent: each side is an excerpt of that text.
        refused_as_contradiction(&nika_compile::revise_intent(&request).unwrap(), &out);
    }
}

#[tokio::test]
async fn a_contradiction_is_never_sent_to_a_seat_whatever_the_strategy() {
    for native in [
        NativeMode::Escalate,
        NativeMode::Only,
        NativeMode::Sketch,
        NativeMode::Off,
    ] {
        for intent in CONTRADICTIONS {
            let provider = seat();
            let request = CompileRequest::create(*intent).with_authoring_policy(policy(native));
            let out = compile_with_provider(&request, &provider).await.unwrap();
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                0,
                "{native:?} {intent}: a paid call"
            );
            refused_as_contradiction(intent, &out);
        }
    }
}

/// A clarification replaces the request (the question asks for a complete replacement): a
/// replacement that contradicts itself is refused as read, never answered by the original
/// request's outcome, and a replacement that resolves the contradiction is read afresh.
#[tokio::test]
async fn a_clarification_is_judged_as_the_request_it_replaces() {
    let provider = seat();
    let request = CompileRequest::create("Read ./notes.md and write it to ./a.txt.")
        .answer(
            "intent.clarification",
            serde_json::json!(CONTRADICTIONS[0]).to_string(),
        )
        .with_authoring_policy(policy(NativeMode::Escalate));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0, "a paid call");
    refused_as_contradiction(CONTRADICTIONS[0], &out);
    // The contradiction resolved by its replacement: nothing is refused as contradictory.
    let provider = seat();
    let request = CompileRequest::create(CONTRADICTIONS[0])
        .answer(
            "intent.clarification",
            serde_json::json!("Read ./notes.md and write it to ./a.txt.").to_string(),
        )
        .with_authoring_policy(policy(NativeMode::Escalate));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.starts_with("Contradictory instructions")),
        "{out:#?}"
    );
}

/// The schedule fixture's request with its cadence placed differently: leading (with « every »
/// or « each », with or without a comma), inside its clause, or ending the sentence.
const CADENCE_PLACEMENTS: &[&str] = &[
    "Every weekday at 8, read ./tickets.json, keep only the rows whose status is open and write them to ./open.json",
    "Each weekday at 8, read ./tickets.json, keep only the rows whose status is open and write them to ./open.json",
    "Each weekday at 8 read ./tickets.json, keep only the rows whose status is open and write them to ./open.json",
    "Read ./tickets.json every weekday at 8, keep only the rows whose status is open and write them to ./open.json",
    "Read ./tickets.json each weekday at 8, keep only the rows whose status is open and write them to ./open.json",
    "Read ./tickets.json, keep only the rows whose status is open and write them to ./open.json every weekday at 8",
];

/// The trigger a READY outcome states beside its candidate, as `(kind, cron, status)`.
fn requested(out: &CompileOutcome) -> Option<(String, String, String)> {
    let doc = nika_compile::outcome_document(out)["requested_trigger"].clone();
    doc.is_object().then_some(())?;
    let field = |k: &str| doc[k].as_str().unwrap_or_default().to_owned();
    Some((field("kind"), field("cron"), field("status")))
}

/// A recurrence is never a one-shot READY claim, wherever the request places it: the same
/// candidate bytes, and the same schedule stated beside them as requiring a binding (the
/// program never runs itself; R4 112).
#[test]
fn a_cadence_is_the_same_trigger_wherever_the_request_places_it() {
    let baseline = compile(&observing(CADENCE_PLACEMENTS[0])).unwrap();
    assert_eq!(baseline.status, CompileStatus::Ready, "{baseline:#?}");
    let schedule = Some((
        "schedule".to_owned(),
        "0 8 * * 1-5".to_owned(),
        "requires_binding".to_owned(),
    ));
    assert_eq!(requested(&baseline), schedule);
    for intent in &CADENCE_PLACEMENTS[1..] {
        let out = compile(&observing(intent)).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert_eq!(out.candidate, baseline.candidate, "{intent}");
        assert_eq!(requested(&out), schedule, "{intent}");
    }
}

/// A nonleading cadence in French, and a cadence beside a prohibition: the trigger stays, the
/// ban stays, and neither swallows the other.
#[test]
fn a_nonleading_cadence_and_a_ban_beside_a_trigger_are_both_kept() {
    for (intent, trigger) in [
        (
            "Lis ./tickets.json chaque matin à 8h et écris les lignes ouvertes dans ./open.json.",
            "chaque matin à 8h",
        ),
        (
            "Read ./report.csv each morning at 7 and write a summary to ./summary.md.",
            "each morning at 7",
        ),
    ] {
        let reading = lexicon::read(&lexicon::fold_apostrophes(intent));
        assert_eq!(reading.plan.trigger.as_deref(), Some(trigger), "{intent}");
    }
    let intent = "Each weekday at 8, read ./tickets.json, keep only the rows whose status is open and write them to ./open.json, but never email them.";
    let reading = lexicon::read(intent);
    assert_eq!(reading.plan.trigger.as_deref(), Some("each weekday at 8"));
    assert!(
        reading
            .plan
            .effects
            .iter()
            .any(|e| e.verb == EffectVerb::Send && e.policy == EffectPolicy::Forbidden),
        "{:?}",
        reading.plan.effects
    );
    // The ban of an effect nothing requests adds nothing to the program: the same bytes and the
    // same schedule as the request without it.
    let out = compile(&observing(intent)).unwrap();
    let without = compile(&observing(CADENCE_PLACEMENTS[1])).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate, without.candidate);
    assert_eq!(requested(&out), requested(&without));
}

/// A cadence inside a ban, a grouping or quoted content is no trigger, and « each » over no
/// cadence word opens no head.
#[test]
fn a_cadence_inside_a_ban_a_grouping_or_quotes_is_no_trigger() {
    for intent in [
        "Read ./tickets.json; never email them each weekday at 8.",
        // A clause that opens on a ban keeps its cadence as the ban's scope, as a negated
        // sentence does, wherever the clause stands.
        "Read ./tickets.json, but never email them each weekday at 8, and write them to ./open.json.",
        "Lis ./tickets.json, mais ne les envoie jamais chaque lundi, et écris-les dans ./open.json.",
        "Each row whose status is open, write it to ./open.json.",
        "Read ./sales.csv, sum the sales of each month and write them to ./totals.csv.",
        "Write 'see you each Monday' to ./a.txt and read ./b.txt.",
        "Écris « à chaque lundi » dans ./a.txt puis lis ./b.txt.",
    ] {
        let reading = lexicon::read(&lexicon::fold_apostrophes(intent));
        assert_eq!(reading.plan.trigger, None, "{intent}");
    }
}
