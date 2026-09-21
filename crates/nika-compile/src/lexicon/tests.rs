// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The multilingual readings of the deterministic door, sentence by sentence: what the
//! reader produces, what the strict admission says, what the compiler asks. Every exact
//! sentence rides with its near-misses, so a widened table is proven not to mislead a
//! sentence that merely resembles the one it was widened for.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::plan::{EffectPolicy, EffectVerb, Op};
use super::super::{CompileOutcome, CompileRequest, CompileStatus, Strategy, compile, hot};
use super::{Reading, read};
use serde_json::Value;

const DIGEST_FR: &str = "fais-moi un digest des notes dans ./notes et écris-le dans ./digest.md";
const RESUME_FR: &str =
    "Lis ./notes/brief.md, rédige un résumé en 3 puces et écris ce résumé dans ./out/resume.md";
const TRANSLATE_FR: &str =
    "Traduis ./notes/brief.md en anglais et écris la traduction dans ./out/brief-en.md";
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
fn a_french_digest_of_a_folder_reads_every_file_drafts_one_digest_and_writes_it() {
    let reading = read(DIGEST_FR);
    assert_eq!(
        steps(&reading),
        [
            ("read", "./notes/*".to_owned()),
            ("draft", "un digest des notes dans ./notes".to_owned()),
        ]
    );
    assert_eq!(
        writes(&reading),
        [("./digest.md".to_owned(), EffectPolicy::Automatic)]
    );
    assert!(reading.unresolved.is_empty(), "{:?}", reading.unresolved);
    assert_eq!(admission(DIGEST_FR), Vec::<String>::new());
    let doc = hot_then_ready(DIGEST_FR);
    // The folder is read as every file directly under it: a derivation of the stated
    // literal, never a guessed extension; the human is not asked for a glob.
    assert_eq!(doc["const"]["source_glob"], "./notes/*", "{doc:#}");
    assert_eq!(
        doc["permits"]["fs"]["read"],
        serde_json::json!(["./notes/**"])
    );
    assert_eq!(
        doc["permits"]["fs"]["write"],
        serde_json::json!(["./digest.md"])
    );
    assert_eq!(doc["tasks"]["glob_source"]["invoke"]["tool"], "nika:glob");
    assert!(
        doc["tasks"]["documents"].is_object(),
        "one digest over the fan-in"
    );
    assert!(doc["tasks"]["draft"].is_object());
    assert!(
        doc["tasks"]["draft_items"].is_null(),
        "never one draft per file"
    );
    assert!(doc["inputs"].get("item").is_none(), "{doc:#}");
}

#[test]
fn a_make_head_drafts_only_produced_content_and_a_bare_folder_read_is_every_file() {
    // `fais-moi un café` is not a draft of a coffee: the clause stays unresolved.
    let reading = read("fais-moi un café et écris-le dans ./digest.md");
    assert_eq!(reading.unresolved, ["fais-moi un café"]);
    assert!(!reading.plan.has(Op::Draft));
    let out = compile(&CompileRequest::create(
        "fais-moi un café et écris-le dans ./digest.md",
    ))
    .expect("compile");
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert_eq!(keys(&out), ["intent.clarification"], "{out:#?}");
    // `fais le ménage dans ./notes` names a folder but no produced content.
    let reading = read("fais le ménage dans ./notes");
    assert_eq!(reading.unresolved, ["fais le ménage dans ./notes"]);
    assert!(!reading.plan.has(Op::Read));
    // Without a folder, the digest is a draft over the material supplied at invocation.
    let reading = read("fais-moi un digest des notes");
    assert_eq!(
        steps(&reading),
        [("draft", "un digest des notes".to_owned())]
    );
    // A bare folder read is every file directly under the folder.
    let reading = read("Lis ./notes, rédige un résumé et écris-le dans ./out/resume.md");
    assert_eq!(steps(&reading)[0], ("read", "./notes/*".to_owned()));
    assert_eq!(
        admission("Lis ./notes, rédige un résumé et écris-le dans ./out/resume.md"),
        Vec::<String>::new()
    );
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
}

#[test]
fn a_french_translation_of_a_file_reads_the_file_and_never_asks_for_an_item() {
    let reading = read(TRANSLATE_FR);
    assert_eq!(
        steps(&reading),
        [
            ("read", "./notes/brief.md".to_owned()),
            ("draft", "./notes/brief.md en anglais".to_owned()),
        ]
    );
    assert_eq!(
        writes(&reading),
        [("./out/brief-en.md".to_owned(), EffectPolicy::Automatic)]
    );
    assert_eq!(admission(TRANSLATE_FR), Vec::<String>::new());
    let doc = hot_then_ready(TRANSLATE_FR);
    assert_eq!(doc["const"]["source_path"], "./notes/brief.md", "{doc:#}");
    assert!(doc["inputs"].get("item").is_none(), "{doc:#}");
    assert_eq!(doc["tasks"]["read_source"]["invoke"]["tool"], "nika:read");
    // Near-misses. A translation with no source file is a draft of the supplied item.
    let reading = read("Traduis le texte en anglais et écris la traduction dans ./out/en.md");
    assert!(!reading.plan.has(Op::Read), "{:?}", steps(&reading));
    assert!(reading.plan.has(Op::Draft));
    // A bare path as the whole object is a write with no content, still refused.
    assert!(
        admission("Traduis ./notes/brief.md")
            .iter()
            .any(|w| w.contains("a write with no content")),
        "{:?}",
        admission("Traduis ./notes/brief.md")
    );
    // The destination of a write is never read as its material.
    let reading = read("Lis ./notes/brief.md et écris un résumé dans ./out/resume.md");
    assert_eq!(steps(&reading)[0], ("read", "./notes/brief.md".to_owned()));
    assert_eq!(writes(&reading).len(), 1);
    assert_eq!(reading.plan.steps.len(), 2, "{:?}", steps(&reading));
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
    // Near-misses. A folder is every file under it.
    let reading = read("Leggi ./notes e scrivi un riassunto in ./out/riassunto.md");
    assert_eq!(steps(&reading)[0], ("read", "./notes/*".to_owned()));
    assert_eq!(steps(&reading)[1], ("draft", "un riassunto".to_owned()));
    assert_eq!(writes(&reading).len(), 1);
    // A send to an address is an effect the compiler must wire, never a write.
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
    // `fammi un caffè` is not a draft.
    let reading = read("Leggi ./notes/brief.md e fammi un caffè");
    assert_eq!(reading.unresolved, ["fammi un caffè"]);
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
    // `hazme un resumen de ./notes/brief.md` reads the file and drafts the summary.
    let reading = read("hazme un resumen de ./notes/brief.md y guárdalo en ./out/resumen.md");
    assert_eq!(steps(&reading)[0], ("read", "./notes/brief.md".to_owned()));
    assert!(reading.plan.has(Op::Draft));
    assert_eq!(writes(&reading).len(), 1);
}

/// A question is a conversation: its relative-looking opener and its copula ("Which ending
/// is gentler?") never make it a rule the grammar reads whole, in any of the reader's
/// languages, with or without the mark. The compiler leaves it to the conversation (an
/// honest incomplete with the clarification alone), so the session's reasoner answers it.
/// The same words behind a head stay the request they are.
#[test]
fn a_question_is_a_conversation_never_a_rule_read_whole() {
    for question in [
        "Which ending is gentler?",
        "Which ending is gentler",
        "Is the total above 100?",
        "is the total above 100",
        "How many rows are open?",
        "Quelle ligne est la plus chère ?",
        "Quale riga è la più cara?",
        "Cuál es la fila más cara?",
    ] {
        let reading = read(&super::fold_apostrophes(question));
        assert!(
            reading.plan.rules.is_empty(),
            "{question}: {:?}",
            reading.plan.rules
        );
        assert!(
            steps(&reading).is_empty(),
            "{question}: {:?}",
            steps(&reading)
        );
        let out = compile(&CompileRequest::create(question)).expect("compile");
        assert_eq!(
            out.status,
            CompileStatus::Incomplete,
            "{question}: {out:#?}"
        );
        assert!(
            out.questions
                .iter()
                .all(|q| q.key == "intent.clarification"),
            "{question}: {:?}",
            out.questions
        );
    }
    for headed in [
        "Read ./tickets.json, keep only the tickets whose status is open and write them to ./open.json?",
        "Read ./sales.csv, count the rows per client and write the counts to ./per-client.json",
        "Read ./notes/brief.md and write it to ./out/copy.md?",
    ] {
        let out = compile(&CompileRequest::create(headed)).expect("compile");
        assert_eq!(out.status, CompileStatus::Ready, "{headed}: {out:#?}");
    }
}

/// A French sort head (« trie », « triez », « trier ») that the reader knows as a classify
/// reads as the sort the closed grammar sees when the clause names its column: a
/// computation, never a classification asking for categories. A classification with
/// stated categories keeps its reading; a sort without a key stays unresolved.
#[test]
fn a_french_sort_head_whose_clause_names_its_column_is_the_sort() {
    let reading =
        read("Lis ./sales.csv, trie les lignes par montant décroissant et écris ./sorted.csv");
    assert_eq!(
        steps(&reading),
        [
            ("read", "./sales.csv".to_owned()),
            (
                "compute",
                "trie les lignes par montant décroissant".to_owned()
            ),
        ]
    );
    assert_eq!(reading.plan.rules.len(), 1);
    assert!(reading.unresolved.is_empty(), "{:?}", reading.unresolved);
    let reading = read("Lis ./tickets.json, classe les tickets en bugs et features");
    assert_eq!(steps(&reading).len(), 2, "{:?}", steps(&reading));
    assert_eq!(steps(&reading)[1].0, "classify");
    assert!(reading.plan.rules.is_empty());
    let reading = read("Lis ./sales.csv, trie les lignes et écris ./sorted.csv");
    assert!(reading.plan.rules.is_empty(), "{:?}", reading.plan.rules);
}

#[test]
fn a_headless_clause_the_rule_grammar_reads_whole_is_a_computation() {
    let intent =
        "Read ./sales.csv, count the rows per client and write the counts to ./per-client.json";
    let reading = read(intent);
    assert_eq!(
        steps(&reading),
        [
            ("read", "./sales.csv".to_owned()),
            ("compute", "count the rows per client".to_owned()),
        ]
    );
    assert_eq!(reading.plan.rules.len(), 1);
    assert!(reading.unresolved.is_empty(), "{:?}", reading.unresolved);
    assert_eq!(writes(&reading).len(), 1);
    assert_eq!(admission(intent), Vec::<String>::new());
    // A clause the grammar cannot read whole stays unresolved; a write whose whole object
    // is a path is the write of the latest result, never a read of the destination.
    let reading = read("Read ./sales.csv, sort the rows and write ./sorted.csv");
    assert_eq!(reading.unresolved, ["sort the rows"]);
    assert_eq!(steps(&reading), [("read", "./sales.csv".to_owned())]);
    assert_eq!(
        writes(&reading),
        [("./sorted.csv".to_owned(), EffectPolicy::Automatic)]
    );
    assert!(!reading.plan.has(Op::Draft));
    // With nothing produced, the same bare path is a write with no content.
    assert!(
        admission("Read ./a.txt and write ./b.txt")
            .iter()
            .any(|w| w.contains("a write with no content")),
        "{:?}",
        admission("Read ./a.txt and write ./b.txt")
    );
    // An exclusion names what leaves: never a keep of those rows. The clause stays the
    // prose the reader cannot parse, and the deterministic door refuses it.
    let intent =
        "Read ./sales.csv, exclude the rows whose amount is below 100 and write them to ./big.csv";
    let reading = read(intent);
    assert!(!reading.plan.has(Op::Compute), "{:?}", steps(&reading));
    assert!(reading.plan.rules.is_empty());
    assert_eq!(
        reading.soft_constraints,
        ["exclude the rows whose amount is below 100"]
    );
    assert!(!admission(intent).is_empty());
    // A list of files is read as stated; a join on a column is a computation; a join with
    // no key is unresolved; a merge of prose into a file stays the effect it is.
    let reading = read(
        "Read ./a.csv and ./b.csv, merge them on the id column and write the result to ./merged.csv",
    );
    assert_eq!(
        steps(&reading),
        [
            ("read", "./a.csv ; ./b.csv".to_owned()),
            ("compute", "merge them on the id column".to_owned()),
        ]
    );
    assert!(
        reading
            .plan
            .effects
            .iter()
            .all(|e| e.verb == EffectVerb::Write),
        "{:?}",
        reading.plan.effects
    );
    let reading = read("Read ./a.csv and ./b.csv, merge them and write the result to ./merged.csv");
    assert_eq!(reading.unresolved, ["merge them"]);
    let reading = read(
        "Read ./notes/a.md and ./notes/b.md, draft a summary and merge the summaries into ./out/all.md",
    );
    assert!(
        reading
            .plan
            .effects
            .iter()
            .any(|e| e.verb == EffectVerb::Merge && e.target == "./out/all.md"),
        "{:?}",
        reading.plan.effects
    );
}

#[test]
fn an_english_sentence_keeps_its_reading_and_the_vague_request_still_asks() {
    // The vague English request of the matrix: no source, no head the reader lists.
    let out = compile(&CompileRequest::create("build me a digest of the docs")).expect("compile");
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(out.candidate.is_none(), "{out:#?}");
    // A source path inside an English draft object is read, like its French twin.
    let reading = read(
        "Summarize ./notes/brief.md in three bullets and write the summary to ./out/summary.md",
    );
    assert_eq!(steps(&reading)[0], ("read", "./notes/brief.md".to_owned()));
    assert_eq!(writes(&reading).len(), 1);
    // `in 3 bullets to ./x.md` keeps its `to`: the locative never steals the destination.
    let reading =
        read("Read ./notes/brief.md and write the summary in 3 bullets to ./out/summary.md");
    assert_eq!(
        writes(&reading),
        [("./out/summary.md".to_owned(), EffectPolicy::Automatic)]
    );
    assert!(reading.plan.has(Op::Draft), "{:?}", steps(&reading));
}
