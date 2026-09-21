// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The multilingual readings of the deterministic door, sentence by sentence: what the
//! reader produces, what the strict admission says, what the compiler asks. Every exact
//! sentence rides with its near-misses, so a widened table is proven not to mislead a
//! sentence that merely resembles the one it was widened for.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::plan::{EffectPolicy, EffectVerb};
use super::super::{CompileOutcome, CompileRequest, CompileStatus, Strategy, compile, hot};
use super::{Reading, read};
use serde_json::Value;

const RESUME_FR: &str =
    "Lis ./notes/brief.md, rédige un résumé en 3 puces et écris ce résumé dans ./out/resume.md";
const RIASSUNTO_IT: &str =
    "Leggi ./notes/brief.md, scrivi un riassunto in 3 punti e salvalo in ./out/riassunto.md";
const RESUMEN_ES: &str =
    "Lee ./notes/brief.md, redacta un resumen en 3 puntos y guárdalo en ./out/resumen.md";

fn steps(reading: &Reading) -> Vec<(&'static str, String)> {
    reading
        .plan
        .steps
        .iter()
        .map(|s| (s.op.word(), s.detail.clone()))
        .collect()
}

fn writes(reading: &Reading) -> Vec<(String, EffectPolicy)> {
    reading
        .plan
        .effects
        .iter()
        .filter(|e| e.verb == EffectVerb::Write)
        .map(|e| (e.target.clone(), e.policy))
        .collect()
}

/// The strict admission verdict on a sentence: the reader's own rejections and the
/// positive laws of the HOT door, together.
fn admission(intent: &str) -> Vec<String> {
    let folded = super::fold_apostrophes(intent);
    let reading = read(&folded);
    let mut why = reading.hot_rejections();
    why.extend(hot::rejections(&folded, &reading));
    why
}

fn keys(out: &CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

fn document(out: &CompileOutcome) -> Value {
    serde_yaml_bw::from_str(out.candidate.as_deref().expect("candidate")).expect("yaml")
}

/// A sentence the deterministic door admits: HOT, one `model` question, then Ready.
fn hot_then_ready(intent: &str) -> Value {
    let out = compile(&CompileRequest::create(intent)).expect("compile");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let out = compile(&CompileRequest::create(intent).answer("model", r#""mock/echo""#))
        .expect("compile");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.check_preview
            .as_ref()
            .expect("preview")
            .report
            .is_clean()
    );
    document(&out)
}

#[test]
fn the_french_summary_keeps_its_reading_beside_the_widened_tables() {
    let reading = read(RESUME_FR);
    assert_eq!(
        steps(&reading),
        [
            ("read", "./notes/brief.md".to_owned()),
            ("draft", "un résumé en 3 puces".to_owned()),
        ]
    );
    assert_eq!(
        writes(&reading),
        [("./out/resume.md".to_owned(), EffectPolicy::Automatic)]
    );
    let doc = hot_then_ready(RESUME_FR);
    assert_eq!(doc["const"]["source_path"], "./notes/brief.md");
    assert!(doc["inputs"].get("item").is_none(), "{doc:#}");
    // `in 3 bullets to ./x.md` keeps its `to`: the locative never steals the destination.
    let reading =
        read("Read ./notes/brief.md and write the summary in 3 bullets to ./out/summary.md");
    assert_eq!(
        writes(&reading),
        [("./out/summary.md".to_owned(), EffectPolicy::Automatic)]
    );
    assert!(
        reading.plan.has(super::super::plan::Op::Draft),
        "{:?}",
        steps(&reading)
    );
}

#[test]
fn an_italian_read_draft_and_save_is_read_without_a_seat() {
    let reading = read(RIASSUNTO_IT);
    assert_eq!(
        steps(&reading),
        [
            ("read", "./notes/brief.md".to_owned()),
            ("draft", "un riassunto in 3 punti".to_owned()),
        ]
    );
    assert_eq!(
        writes(&reading),
        [("./out/riassunto.md".to_owned(), EffectPolicy::Automatic)]
    );
    assert!(reading.unresolved.is_empty(), "{:?}", reading.unresolved);
    assert_eq!(admission(RIASSUNTO_IT), Vec::<String>::new());
    let doc = hot_then_ready(RIASSUNTO_IT);
    assert_eq!(doc["const"]["source_path"], "./notes/brief.md", "{doc:#}");
    assert_eq!(
        doc["permits"]["fs"]["write"],
        serde_json::json!(["./out/riassunto.md"])
    );
    assert!(doc["inputs"].get("item").is_none(), "{doc:#}");
    // Near-misses. A send to an address is an effect the compiler must wire, never a write.
    let reading =
        read("Leggi ./notes/brief.md, scrivi un riassunto e invialo a ops@example.invalid");
    assert!(
        reading
            .plan
            .effects
            .iter()
            .any(|e| e.verb == EffectVerb::Send)
    );
    assert!(writes(&reading).is_empty());
    // A draft with no destination is the workflow's output, nothing written.
    let reading = read("Leggi ./notes/brief.md e scrivi un riassunto in 3 punti");
    assert_eq!(steps(&reading).len(), 2, "{:?}", steps(&reading));
    assert!(writes(&reading).is_empty());
    // A gate in Italian holds the write for a human.
    let reading = read(
        "Leggi ./notes/brief.md, scrivi un riassunto e salvalo in ./out/riassunto.md solo dopo la mia approvazione",
    );
    assert_eq!(
        writes(&reading),
        [("./out/riassunto.md".to_owned(), EffectPolicy::HumanFirst)]
    );
}

#[test]
fn a_spanish_read_draft_and_save_is_read_without_a_seat() {
    let reading = read(RESUMEN_ES);
    assert_eq!(
        steps(&reading),
        [
            ("read", "./notes/brief.md".to_owned()),
            ("draft", "un resumen en 3 puntos".to_owned()),
        ]
    );
    assert_eq!(
        writes(&reading),
        [("./out/resumen.md".to_owned(), EffectPolicy::Automatic)]
    );
    assert_eq!(admission(RESUMEN_ES), Vec::<String>::new());
    let doc = hot_then_ready(RESUMEN_ES);
    assert_eq!(doc["const"]["source_path"], "./notes/brief.md", "{doc:#}");
    assert!(doc["inputs"].get("item").is_none(), "{doc:#}");
    // Near-misses. `resume` is left to English: the clause stays unresolved, asked.
    let reading = read("resume ./notes/brief.md y guárdalo en ./out/resumen.md");
    assert_eq!(reading.unresolved, ["resume ./notes/brief.md"]);
    // A write whose object is new content is the draft the write demands.
    let reading = read("Lee ./notes/brief.md y escribe un resumen en ./out/resumen.md");
    assert_eq!(
        steps(&reading),
        [
            ("read", "./notes/brief.md".to_owned()),
            ("draft", "un resumen".to_owned()),
        ]
    );
    assert_eq!(writes(&reading).len(), 1);
    // A prohibition in Spanish forbids the effect it names.
    let reading = read("Lee ./notes/brief.md y redacta un resumen. Nunca envíes un correo.");
    let send = reading
        .plan
        .effects
        .iter()
        .find(|e| e.verb == EffectVerb::Send)
        .expect("the forbidden send");
    assert_eq!(send.policy, EffectPolicy::Forbidden);
}
