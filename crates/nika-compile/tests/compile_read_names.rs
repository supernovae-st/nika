// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A read keeps the file name the request states. A cold plan whose read detail was exactly
//! `Notes équipe.txt` bound `équipe.txt` (and, before writes kept whole names, wrote it too:
//! `équipe.txt -> équipe.txt`). A name the request states whole (quoted, rooted, or a
//! capitalized run its reader glues to the file) is bound whole; a longer name it leaves
//! open is asked; the last word of a name is never the file read. Every plan below is a
//! recorded COLD plan replayed through the answer-round door (zero provider calls), or the
//! proposal of the hermetic provider double of `common`.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    CompileOutcome, CompileRequest, CompileStatus, Strategy, compile, intent_sha256,
    outcome_document,
};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

mod common;
use common::{Provider, keys, policy};

const COPY: &str = "Lis Notes équipe.txt et écris son contenu à l'identique dans Copie équipe.txt.";

/// A recorded COLD plan: one read of `detail` and one automatic write to `target`, each
/// with its verbatim excerpt of the request, as an earlier round records them.
fn recorded(detail: &str, read: &str, target: &str, write: &str) -> Value {
    json!({"strategy":"cold",
        "operations":[{"op":"read","detail":detail,"evidence":read,"categories":[]}],
        "effects":[{"verb":"write","target":target,"policy":"automatic","evidence":write,"policy_literal":null}],
        "obligations":[],"bindings":[],"constraints":[],"unknowns":[]})
}

/// The answer-round door: the recorded plan replayed for the same intent, no provider.
fn replay(intent: &str, record: &Value, answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(intent).with_plan(record.clone());
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    compile(&request).unwrap()
}

/// The Ready candidate, checked clean, as a document.
fn document(out: &CompileOutcome) -> Value {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.check_preview.as_ref().unwrap().report.is_clean(),
        "{out:#?}"
    );
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

/// The residual itself: the detail is the whole name, the request glues the same capitalized
/// run to its file (`Lis Notes équipe.txt`), and the candidate reads that file under a read
/// permit for it alone, never `équipe.txt`.
#[test]
fn a_cold_read_detail_that_is_one_name_binds_the_whole_name() {
    let record = recorded(
        "Notes équipe.txt",
        "Lis Notes équipe.txt",
        "Copie équipe.txt",
        "écris son contenu à l'identique dans Copie équipe.txt",
    );
    let out = replay(COPY, &record, &[]);
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    assert_eq!(
        outcome_document(&out)["provenance"]["decision"]["intent_sha256"],
        intent_sha256(COPY)
    );
    let doc = document(&out);
    assert_eq!(doc["const"]["source_path"], "Notes équipe.txt", "{doc:#}");
    assert_eq!(doc["const"]["output_path"], "Copie équipe.txt", "{doc:#}");
    assert_eq!(doc["permits"]["fs"]["read"], json!(["Notes équipe.txt"]));
    assert_eq!(doc["permits"]["fs"]["write"], json!(["Copie équipe.txt"]));
}

/// Prose around the name (a file noun before it, a gloss after it): the request's reader
/// settles the capitalized run it glues to the file; the plan's words around it bind nothing.
#[test]
fn prose_around_a_stated_name_keeps_the_name_whole() {
    let intent = "Lis le fichier Notes équipe.txt et écris son contenu dans Copie équipe.txt.";
    for detail in [
        "le fichier Notes équipe.txt",
        "Notes équipe.txt (la version de lundi)",
    ] {
        let record = recorded(
            detail,
            "Lis le fichier Notes équipe.txt",
            "Copie équipe.txt",
            "écris son contenu dans Copie équipe.txt",
        );
        let doc = document(&replay(intent, &record, &[]));
        assert_eq!(
            doc["const"]["source_path"], "Notes équipe.txt",
            "{detail}: {doc:#}"
        );
        assert_eq!(
            doc["permits"]["fs"]["read"],
            json!(["Notes équipe.txt"]),
            "{detail}"
        );
    }
}

/// A name holding a function word (`de`) is one name where the request says so: quoted, it
/// is read whole; unquoted, its reader cannot tell it from prose, so the file is asked,
/// never cut to `réunion.txt`, and the answer names it.
#[test]
fn a_name_with_a_function_word_is_read_whole_when_quoted_and_asked_otherwise() {
    let quoted = r#"Lis "Notes de réunion.txt" et écris son contenu dans "Compte rendu.docx"."#;
    let record = recorded(
        "Notes de réunion.txt",
        r#"Lis "Notes de réunion.txt""#,
        "Compte rendu.docx",
        r#"écris son contenu dans "Compte rendu.docx""#,
    );
    let doc = document(&replay(quoted, &record, &[]));
    assert_eq!(
        doc["const"]["source_path"], "Notes de réunion.txt",
        "{doc:#}"
    );
    assert_eq!(doc["const"]["output_path"], "Compte rendu.docx", "{doc:#}");
    assert_eq!(
        doc["permits"]["fs"]["read"],
        json!(["Notes de réunion.txt"])
    );

    let bare = "Lis le fichier Notes de réunion.txt et écris son contenu dans sortie.txt.";
    let record = recorded(
        "Notes de réunion.txt",
        "Lis le fichier Notes de réunion.txt",
        "sortie.txt",
        "écris son contenu dans sortie.txt",
    );
    let asked = replay(bare, &record, &[]);
    assert_ne!(asked.status, CompileStatus::Ready, "{asked:#?}");
    assert_eq!(keys(&asked), ["const.source_paths"], "{asked:#?}");
    let answer = [("const.source_paths", r#"["«Notes de réunion.txt»"]"#)];
    let doc = document(&replay(bare, &record, &answer));
    assert_eq!(
        doc["const"]["source_path"], "Notes de réunion.txt",
        "{doc:#}"
    );
    assert_eq!(
        doc["permits"]["fs"]["read"],
        json!(["Notes de réunion.txt"])
    );
}

/// The request spells the plan's longer name but its reader cannot settle it: the name opens
/// the sentence, or the request writes it in lowercase with no file noun. The longer name
/// the plan reads is asked, never cut, and the answer binds the file it names.
#[test]
fn a_longer_name_the_request_leaves_open_is_asked_then_answered_whole() {
    for (intent, read, write, name) in [
        (
            "Notes équipe.txt doit être copié tel quel dans sortie.txt.",
            "Notes équipe.txt doit être copié",
            "copié tel quel dans sortie.txt",
            "Notes équipe.txt",
        ),
        (
            "Copie notes équipe.txt tel quel dans sortie.txt.",
            "Copie notes équipe.txt",
            "tel quel dans sortie.txt",
            "notes équipe.txt",
        ),
    ] {
        let record = recorded("Notes équipe.txt", read, "sortie.txt", write);
        let asked = replay(intent, &record, &[]);
        assert_ne!(asked.status, CompileStatus::Ready, "{intent}: {asked:#?}");
        assert_eq!(keys(&asked), ["const.source_paths"], "{intent}: {asked:#?}");
        let answer = format!(r#"["{name}"]"#);
        let doc = document(&replay(
            intent,
            &record,
            &[("const.source_paths", answer.as_str())],
        ));
        assert_eq!(doc["const"]["source_path"], name, "{intent}: {doc:#}");
        assert_eq!(doc["permits"]["fs"]["read"], json!([name]), "{intent}");
    }
}

/// The fidelity refusal of an answered round, as the message names its path.
fn refused(out: &CompileOutcome) -> Vec<&str> {
    out.diagnostics
        .iter()
        .filter(|d| d.target == "fidelity")
        .filter_map(|d| d.message.split('`').nth(1))
        .collect()
}

/// The whole name the human types holds only the occurrences it spans: a request that also
/// names `équipe.txt` on its own still owes that file, until the answer reads it too.
#[test]
fn a_typed_whole_name_realizes_its_own_occurrence_and_no_separate_file() {
    let intent = "Notes équipe.txt doit être fusionné avec équipe.txt dans sortie.txt.";
    let record = recorded(
        "Notes équipe.txt",
        "Notes équipe.txt doit être fusionné",
        "sortie.txt",
        "fusionné avec équipe.txt dans sortie.txt",
    );
    assert_eq!(keys(&replay(intent, &record, &[])), ["const.source_paths"]);
    let one = replay(
        intent,
        &record,
        &[("const.source_paths", r#"["Notes équipe.txt"]"#)],
    );
    assert_ne!(one.status, CompileStatus::Ready, "{one:#?}");
    assert_eq!(refused(&one), ["équipe.txt"], "{one:#?}");
    let both = [(
        "const.source_paths",
        r#"["Notes équipe.txt", "équipe.txt"]"#,
    )];
    let out = replay(intent, &record, &both);
    assert_eq!(
        outcome_document(&out)["provenance"]["decision"]["intent_sha256"],
        intent_sha256(intent)
    );
    let doc = document(&out);
    let read = json!(["Notes équipe.txt", "équipe.txt"]);
    assert_eq!(doc["const"]["source_paths"], read, "{doc:#}");
    assert_eq!(doc["permits"]["fs"]["read"], read, "{doc:#}");
}

/// An answer the request never writes realizes none of its files: the file it names is still
/// owed, whatever the candidate reads instead.
#[test]
fn an_answered_file_the_request_never_names_realizes_nothing() {
    let intent = "Notes équipe.txt doit être copié tel quel dans sortie.txt.";
    let record = recorded(
        "Notes équipe.txt",
        "Notes équipe.txt doit être copié",
        "sortie.txt",
        "copié tel quel dans sortie.txt",
    );
    let out = replay(
        intent,
        &record,
        &[("const.source_paths", r#"["autre.txt"]"#)],
    );
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(refused(&out), ["équipe.txt"], "{out:#?}");
}

/// An unquoted traversal the reader splits into a folder and a file: the answered whole name,
/// verbatim, is read under its own permit, and the folder is not granted in its place.
#[test]
fn an_unquoted_traversal_answered_whole_keeps_its_exact_spelling() {
    let intent = "Lis ../Partage/Notes équipe.txt et écris son contenu dans ./out/copie.txt.";
    let record = recorded(
        "../Partage/Notes équipe.txt",
        "Lis ../Partage/Notes équipe.txt",
        "./out/copie.txt",
        "écris son contenu dans ./out/copie.txt",
    );
    let answer = [("const.source_paths", r#"["«../Partage/Notes équipe.txt»"]"#)];
    let doc = document(&replay(intent, &record, &answer));
    assert_eq!(
        doc["const"]["source_path"], "../Partage/Notes équipe.txt",
        "{doc:#}"
    );
    assert_eq!(
        doc["permits"]["fs"]["read"],
        json!(["../Partage/Notes équipe.txt"]),
        "{doc:#}"
    );
}

/// The reader glues `Copie équipe.txt` after `dans` into one destination, whose `équipe.txt`
/// is a destination occurrence of the last word the opening name was cut to: the assembler's
/// write of that destination realizes it, beside the typed source.
#[test]
fn a_glued_destination_the_candidate_writes_realizes_its_own_occurrence() {
    let intent = "Notes équipe.txt doit aller dans Copie équipe.txt.";
    let record = recorded(
        "Notes équipe.txt",
        "Notes équipe.txt doit aller",
        "Copie équipe.txt",
        "aller dans Copie équipe.txt",
    );
    assert_eq!(keys(&replay(intent, &record, &[])), ["const.source_paths"]);
    let answer = [("const.source_paths", r#"["Notes équipe.txt"]"#)];
    let out = replay(intent, &record, &answer);
    assert_eq!(
        outcome_document(&out)["provenance"]["decision"]["intent_sha256"],
        intent_sha256(intent)
    );
    let doc = document(&out);
    assert_eq!(doc["const"]["source_path"], "Notes équipe.txt", "{doc:#}");
    assert_eq!(doc["const"]["output_path"], "Copie équipe.txt", "{doc:#}");
    assert_eq!(doc["permits"]["fs"]["read"], json!(["Notes équipe.txt"]));
    assert_eq!(doc["permits"]["fs"]["write"], json!(["Copie équipe.txt"]));
}

/// A plan naming only the last word of a name the request states (`équipe.txt` for
/// `Notes équipe.txt`) reads nothing on that word: the file is asked.
#[test]
fn the_last_word_of_a_stated_name_is_never_the_file_read() {
    let record = recorded(
        "équipe.txt",
        "Lis Notes équipe.txt",
        "Copie équipe.txt",
        "écris son contenu à l'identique dans Copie équipe.txt",
    );
    let asked = replay(COPY, &record, &[]);
    assert_ne!(asked.status, CompileStatus::Ready, "{asked:#?}");
    assert_eq!(keys(&asked), ["const.source_paths"], "{asked:#?}");
}

/// Several files in one detail: each keeps its own whole name, one read permit per file.
#[test]
fn several_read_files_keep_their_whole_names_one_permit_each() {
    let intent =
        "Lis Notes équipe.txt et Planning.md puis écris leur contenu dans Dossier complet.txt.";
    let record = recorded(
        "Notes équipe.txt et Planning.md",
        "Lis Notes équipe.txt et Planning.md",
        "Dossier complet.txt",
        "écris leur contenu dans Dossier complet.txt",
    );
    let doc = document(&replay(intent, &record, &[]));
    let read = json!(["Notes équipe.txt", "Planning.md"]);
    assert_eq!(doc["const"]["source_paths"], read, "{doc:#}");
    assert_eq!(doc["permits"]["fs"]["read"], read, "{doc:#}");
    assert_eq!(
        doc["const"]["output_path"], "Dossier complet.txt",
        "{doc:#}"
    );
}

/// A traversal literal keeps its exact spelling: quoted in the request, the plan's detail
/// binds it verbatim (no normalization, no folder read in its place); unquoted, the request
/// states a folder and a file, and the read is asked rather than widened.
#[test]
fn a_traversal_literal_is_bound_verbatim_or_asked_never_widened() {
    let quoted = r#"Lis "../Partage/Notes équipe.txt" et écris son contenu dans ./out/copie.txt."#;
    let record = recorded(
        "../Partage/Notes équipe.txt",
        r#"Lis "../Partage/Notes équipe.txt""#,
        "./out/copie.txt",
        "écris son contenu dans ./out/copie.txt",
    );
    let doc = document(&replay(quoted, &record, &[]));
    assert_eq!(
        doc["const"]["source_path"], "../Partage/Notes équipe.txt",
        "{doc:#}"
    );
    assert_eq!(
        doc["permits"]["fs"]["read"],
        json!(["../Partage/Notes équipe.txt"]),
        "{doc:#}"
    );
    let bare = "Lis ../Partage/Notes équipe.txt et écris son contenu dans ./out/copie.txt.";
    let record = recorded(
        "../Partage/Notes équipe.txt",
        "Lis ../Partage/Notes équipe.txt",
        "./out/copie.txt",
        "écris son contenu dans ./out/copie.txt",
    );
    let asked = replay(bare, &record, &[]);
    assert_ne!(asked.status, CompileStatus::Ready, "{asked:#?}");
    assert_eq!(keys(&asked), ["const.source_paths"], "{asked:#?}");
}

/// What a read detail bound before still binds the same: a bare file after a file noun or a
/// function word, a rooted path, a verb the request does not spell before its file.
#[test]
fn bare_files_and_rooted_paths_bind_as_before() {
    for (intent, detail, read, target, write, file) in [
        (
            "Lis le fichier entree.txt et écris son contenu dans sortie.txt.",
            "Lis le fichier entree.txt",
            "Lis le fichier entree.txt",
            "sortie.txt",
            "écris son contenu dans sortie.txt",
            "entree.txt",
        ),
        (
            "Read report.md and write it as is to ./out/copy.md.",
            "Read report.md",
            "Read report.md",
            "./out/copy.md",
            "write it as is to ./out/copy.md",
            "report.md",
        ),
        (
            "Read ./notes/brief.md and write it as is to ./out/copy.md.",
            "./notes/brief.md",
            "Read ./notes/brief.md",
            "./out/copy.md",
            "write it as is to ./out/copy.md",
            "./notes/brief.md",
        ),
        (
            "Prends entree.txt et écris son contenu dans sortie.txt.",
            "Lire entree.txt",
            "Prends entree.txt",
            "sortie.txt",
            "écris son contenu dans sortie.txt",
            "entree.txt",
        ),
    ] {
        let record = recorded(detail, read, target, write);
        let doc = document(&replay(intent, &record, &[]));
        assert_eq!(doc["const"]["source_path"], file, "{intent}: {doc:#}");
        assert_eq!(doc["permits"]["fs"]["read"], json!([file]), "{intent}");
    }
}

const STOCK: &str = "Lee el archivo Stock tienda.json, encuentra los productos con menos de 10 unidades y escribe una lista de reposición en ./salida/reposicion.md agrupada por proveedor. Si algún producto tiene 0 unidades, márcalo como URGENTE al principio de su línea.";

/// The plan an authoring seat proposed for this request shape (the stock case of the
/// assembler suite), its source named by a spaced name the request's file noun opens.
fn stock_plan() -> Value {
    json!({"steps":[
        {"op":"read","detail":"Stock tienda.json","evidence":"Lee el archivo Stock tienda.json"},
        {"op":"extract","detail":"productos con menos de 10 unidades","evidence":"encuentra los productos con menos de 10 unidades"},
        {"op":"draft","detail":"lista de reposición agrupada por proveedor","evidence":"escribe una lista de reposición en ./salida/reposicion.md agrupada por proveedor"}],
      "effects":[{"verb":"write","target":"./salida/reposicion.md","policy":"automatic","evidence":"escribe una lista de reposición en ./salida/reposicion.md agrupada por proveedor"}],
      "obligations":[],"constraints":["Si algún producto tiene 0 unidades, márcalo como URGENTE al principio de su línea"],"unknowns":[]})
}

/// Through the provider door: the COLD proposal's read detail is the whole name, the
/// candidate reads it under its own permit, and the recorded plan replays the same
/// candidate with no second call.
#[tokio::test]
async fn a_proposed_cold_plan_reads_the_whole_name_and_its_record_replays_it() {
    let provider = Provider::new(stock_plan());
    let request = CompileRequest::create(STOCK)
        .with_authoring_policy(policy())
        .answer("model", r#""mock/echo""#);
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    let doc = document(&out);
    assert_eq!(doc["const"]["source_path"], "Stock tienda.json", "{doc:#}");
    assert_eq!(doc["permits"]["fs"]["read"], json!(["Stock tienda.json"]));
    let record = out.provenance.plan.clone().expect("a recorded plan");
    let replayed = replay(STOCK, &record, &[("model", r#""mock/echo""#)]);
    assert_eq!(replayed.candidate, out.candidate, "{replayed:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}
