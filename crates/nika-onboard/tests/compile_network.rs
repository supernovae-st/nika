// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Network sentences the deterministic door must read as a human does: a facet of a
//! fetched page (its title, its article) is the fetch's own extract mode written as it is,
//! never a draft; a stated webhook is the endpoint an effect posts to, and a gate that names
//! the action by a verb word gates that effect; a stated cadence is a trigger requirement
//! beside the candidate, never inside its bytes. Every sentence rides with its near-misses.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_onboard::compile::{CompileOutcome, CompileRequest, CompileStatus, Strategy, compile};
use serde_json::{Value, json};

fn keys(out: &CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

fn document(out: &CompileOutcome) -> Value {
    serde_yaml_bw::from_str(out.candidate.as_deref().expect("candidate")).expect("yaml")
}

/// A sentence the deterministic door settles with zero questions: HOT and Ready at once,
/// its preview clean.
fn ready(intent: &str) -> Value {
    let out = compile(&CompileRequest::create(intent)).expect("compile");
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.check_preview
            .as_ref()
            .expect("preview")
            .report
            .is_clean(),
        "{out:#?}"
    );
    document(&out)
}

/// A sentence the deterministic door admits with the one `model` question a language
/// step needs, then Ready.
fn hot_with_model(intent: &str) -> Value {
    let out = compile(&CompileRequest::create(intent)).expect("compile");
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let out = compile(&CompileRequest::create(intent).answer("model", r#""mock/echo""#))
        .expect("compile");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    document(&out)
}

#[test]
fn a_page_title_is_the_fetch_in_metadata_mode_never_a_draft() {
    for intent in [
        "Fetch https://example.com/ and write the page title to ./title.txt",
        "Récupère https://example.com/ et écris le titre de la page dans ./title.txt",
        "Fetch https://example.com/ and save the title of the page to ./title.txt",
    ] {
        let doc = ready(intent);
        assert!(doc.get("model").is_none(), "{intent}: no seat: {doc:#}");
        assert!(doc["tasks"].get("draft").is_none(), "{intent}: {doc:#}");
        assert_eq!(
            doc["const"]["source_url"], "https://example.com/",
            "{intent}"
        );
        assert_eq!(
            doc["tasks"]["fetch_source"]["invoke"]["args"]["mode"], "metadata",
            "{intent}: {doc:#}"
        );
        assert_eq!(
            doc["tasks"]["page_title"]["invoke"]["args"]["expression"], ".title",
            "{intent}: {doc:#}"
        );
        assert_eq!(
            doc["tasks"]["page_title"]["with"]["page"],
            "${{ tasks.fetch_source.output }}"
        );
        assert_eq!(
            doc["tasks"]["write_output"]["with"]["content"],
            "${{ tasks.page_title.output }}"
        );
        assert_eq!(doc["const"]["output_path"], "./title.txt");
        assert_eq!(doc["permits"]["net"]["http"], json!(["example.com"]));
        assert_eq!(
            doc["permits"]["tools"],
            json!(["nika:fetch", "nika:jq", "nika:write"])
        );
        assert!(doc.get("inputs").is_none(), "phantom item: {doc:#}");
    }
}

#[test]
fn the_article_text_is_the_fetch_in_article_mode_written_as_it_is() {
    let doc = ready("Fetch https://example.com/post and save the article text to ./page.md");
    assert!(doc["tasks"].get("draft").is_none(), "{doc:#}");
    assert_eq!(
        doc["tasks"]["fetch_source"]["invoke"]["args"]["mode"],
        "article"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.fetch_source.output }}"
    );
    // Two facets in two modes: the page as text and its title, one fetch per mode.
    let doc = ready(
        "Fetch https://example.com/ and write the page text to ./page.txt and the page title to ./title.txt",
    );
    assert_eq!(
        doc["tasks"]["fetch_source"]["invoke"]["args"]["mode"],
        "text"
    );
    assert_eq!(
        doc["tasks"]["fetch_metadata"]["invoke"]["args"]["mode"],
        "metadata"
    );
    assert_eq!(
        doc["tasks"]["page_title"]["with"]["page"],
        "${{ tasks.fetch_metadata.output }}"
    );
    assert_eq!(
        doc["tasks"]["write_title"]["with"]["content"],
        "${{ tasks.page_title.output }}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.fetch_source.output }}"
    );
}

#[test]
fn a_fetched_page_a_model_reads_stays_a_draft_with_its_seat() {
    // Content the fetch does not yield as it is: a summary is a draft, with its seat.
    let doc = hot_with_model(
        "Fetch https://example.com/ and write a summary of the page to ./summary.md",
    );
    assert!(doc["tasks"]["draft"].is_object(), "{doc:#}");
    assert_eq!(
        doc["tasks"]["fetch_source"]["invoke"]["args"]["mode"],
        "article"
    );
    // A draft beside a facet: the article for the seat, the title as it is.
    let doc = hot_with_model(
        "Fetch https://example.com/, write a summary of the page to ./summary.md and write the page title to ./title.txt",
    );
    assert_eq!(
        doc["tasks"]["fetch_source"]["invoke"]["args"]["mode"],
        "article"
    );
    assert_eq!(
        doc["tasks"]["fetch_metadata"]["invoke"]["args"]["mode"],
        "metadata"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.draft.output.body }}"
    );
    assert_eq!(
        doc["tasks"]["write_title"]["with"]["content"],
        "${{ tasks.page_title.output }}"
    );
    // Without a fetch, "the page title" is content nobody fetched: a draft of it, asked.
    let out = compile(&CompileRequest::create(
        "Read ./doc.md and write the page title to ./title.txt",
    ))
    .unwrap();
    assert_eq!(keys(&out), ["model"], "{out:#?}");
}

#[test]
fn a_fetch_with_no_url_asks_for_it_and_never_invents_one() {
    let out = compile(&CompileRequest::create(
        "Fetch the page and write the page title to ./title.txt",
    ))
    .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(keys(&out), ["const.source_url"], "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let out = compile(
        &CompileRequest::create("Fetch the page and write the page title to ./title.txt")
            .answer("const.source_url", r#""https://example.com/""#),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert_eq!(
        doc["tasks"]["page_title"]["invoke"]["args"]["expression"],
        ".title"
    );
}

#[test]
fn a_loopback_literal_is_declared_and_declassified_and_a_private_range_stays_refused() {
    // The exact loopback literal the request names is the permit; the check states the
    // declassification of the SSRF floor for that host only (#395).
    let out = compile(&CompileRequest::create(
        "Fetch http://127.0.0.1:8765/ and write the page title to ./title.txt",
    ))
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert_eq!(doc["permits"]["net"]["http"], json!(["127.0.0.1"]));
    assert_eq!(doc["const"]["source_url"], "http://127.0.0.1:8765/");
    let boundary = out.requested_boundary.as_ref().unwrap();
    assert!(
        boundary
            .notes
            .iter()
            .any(|n| n.contains("127.0.0.1") && n.contains("loopback")),
        "{:?}",
        boundary.notes
    );
    // A private range that is not loopback is never declassified: the check refuses the
    // candidate and the compile carries the refusal instead of a Ready.
    let out = compile(&CompileRequest::create(
        "Fetch http://10.0.0.5/ and write the page title to ./title.txt",
    ))
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("10.0.0.5") || d.message.contains("SEC")),
        "{out:#?}"
    );
}
