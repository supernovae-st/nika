// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Network sentences the deterministic door must read as a human does: a facet of a
//! fetched page (its title, its article) is the fetch's own extract mode written as it is,
//! never a draft; a stated webhook is the endpoint an effect posts to, and a gate that names
//! the action by a verb word gates that effect; a stated cadence is a trigger requirement
//! beside the candidate, never inside its bytes. Every sentence rides with its near-misses.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, Strategy, TriggerKind,
    TriggerStatus, compile, outcome_document,
};
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

fn plan_effects(out: &CompileOutcome) -> Vec<(String, String)> {
    out.provenance.plan.as_ref().expect("plan")["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .map(|e| {
            (
                e["verb"].as_str().unwrap_or_default().to_owned(),
                e["policy"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_stated_webhook_is_the_endpoint_and_a_gate_naming_the_action_gates_it() {
    for (intent, slug) in [
        (
            "Read ./report.md and post it to https://hooks.example.com/notify, but ask me before sending",
            "publish",
        ),
        (
            "Ask me before sending. Read ./report.md and post it to https://hooks.example.com/notify",
            "publish",
        ),
        (
            "Read ./report.md and send it to https://hooks.example.com/notify, but ask me before publishing",
            "send",
        ),
        (
            "Read ./report.md and post it to https://hooks.example.com/notify. Never send anything without my approval.",
            "publish",
        ),
    ] {
        let out = compile(&CompileRequest::create(intent)).expect("compile");
        // One effect, the stated one, gated: no phantom `send` asking for an endpoint.
        assert_eq!(
            plan_effects(&out),
            [(slug.to_owned(), "human_first".to_owned())],
            "{intent}: {out:#?}"
        );
        let doc = ready(intent);
        assert_eq!(
            doc["const"][format!("{slug}_endpoint")],
            "https://hooks.example.com/notify",
            "{intent}: {doc:#}"
        );
        assert!(doc["const"].get("send_endpoint").is_none() || slug == "send");
        let post = &doc["tasks"][slug];
        assert_eq!(post["invoke"]["tool"], "nika:notify", "{intent}: {doc:#}");
        assert_eq!(post["invoke"]["args"]["channel"], "webhook");
        assert_eq!(
            post["invoke"]["args"]["target"],
            format!("${{{{ const.{slug}_endpoint }}}}")
        );
        assert_eq!(post["invoke"]["args"]["message"], "${{ with.content }}");
        assert_eq!(post["with"]["content"], "${{ tasks.read_source.output }}");
        assert_eq!(post["when"], "${{ with.approved == true }}");
        let review = &doc["tasks"][format!("{slug}_review")];
        assert_eq!(review["invoke"]["tool"], "nika:prompt", "{intent}: {doc:#}");
        assert_eq!(
            post["with"]["approved"],
            format!("${{{{ tasks.{slug}_review.output }}}}")
        );
        assert_eq!(
            doc["permits"]["net"]["http"],
            json!(["hooks.example.com"]),
            "{intent}"
        );
        assert_eq!(
            doc["permits"]["tools"],
            json!(["nika:notify", "nika:prompt", "nika:read"])
        );
        assert!(doc.get("model").is_none(), "no seat: {doc:#}");
    }
}

#[test]
fn an_ungated_post_is_a_real_post_with_no_review_and_a_recurring_head_is_carried() {
    let doc = ready("Read ./report.md and post it to https://hooks.example.com/notify");
    assert_eq!(doc["tasks"]["publish"]["invoke"]["tool"], "nika:notify");
    assert!(doc["tasks"].get("publish_review").is_none(), "{doc:#}");
    assert!(doc["tasks"]["publish"].get("when").is_none(), "{doc:#}");
    assert_eq!(
        doc["tasks"]["publish"]["with"]["content"],
        "${{ tasks.read_source.output }}"
    );
    // "the report" after "./report.md" is that file, carried as it is.
    let doc = ready("Read ./report.md and post the report to https://hooks.example.com/notify");
    assert_eq!(
        doc["tasks"]["publish"]["with"]["content"],
        "${{ tasks.read_source.output }}"
    );
    // A drafted reply is carried too, as the draft's body.
    let doc = hot_with_model(
        "Read ./inbox/a.md, draft a reply, and send the reply to https://hooks.example.com/notify",
    );
    assert_eq!(doc["tasks"]["send"]["invoke"]["tool"], "nika:notify");
    assert_eq!(
        doc["tasks"]["send"]["with"]["content"],
        "${{ tasks.draft.output.body }}"
    );
}

#[test]
fn a_post_with_no_destination_asks_and_unproduced_content_is_never_posted() {
    // No URL: the endpoint is asked, once, for the stated effect; the gate gates it.
    let out = compile(&CompileRequest::create(
        "Read ./report.md and post it, but ask me before sending",
    ))
    .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(keys(&out), ["const.publish_endpoint"], "{out:#?}");
    assert_eq!(
        plan_effects(&out),
        [("publish".to_owned(), "human_first".to_owned())]
    );
    // A gate naming a send no clause states: the destination is asked, never invented.
    let out = compile(&CompileRequest::create(
        "Read ./report.md, write a summary to ./s.md, but ask me before sending",
    ))
    .unwrap();
    assert!(keys(&out).contains(&"const.send_endpoint"), "{out:#?}");
    // "a summary" is content no step produces: not admitted, asked.
    let out = compile(&CompileRequest::create(
        "Read ./notes.md and post a summary to https://hooks.example.com/notify",
    ))
    .unwrap();
    assert_ne!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(keys(&out), ["intent.clarification"], "{out:#?}");
    // A cleartext endpoint that is not loopback is refused; the question stays open.
    let out = compile(&CompileRequest::create(
        "Read ./report.md and post it to http://10.0.0.5/notify",
    ))
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(keys(&out), ["const.publish_endpoint"], "{out:#?}");
}

#[test]
fn a_stated_cadence_is_a_trigger_requirement_beside_the_candidate_never_in_its_bytes() {
    let intent = "Every morning at 9, read ./inbox/*.md and write a digest to ./digest.md";
    // The first round (the seat is still asked) already states the requirement.
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let trigger = out.requested_trigger.as_ref().expect("requested_trigger");
    assert_eq!(trigger.kind, TriggerKind::Schedule);
    assert_eq!(trigger.source_hint.as_deref(), Some("every morning at 9"));
    assert_eq!(trigger.cadence.as_deref(), Some("daily"));
    assert_eq!(trigger.at.as_deref(), Some("09:00"));
    assert_eq!(trigger.status, TriggerStatus::RequiresBinding);
    assert_eq!(
        trigger.payload_input, None,
        "a fan-out reads its folder, not an item"
    );
    let note = out
        .diagnostics
        .iter()
        .find(|d| d.target == "trigger")
        .expect("the trigger note");
    assert_eq!(note.kind, DiagnosticKind::Applied);
    assert!(
        note.message.contains("`every morning at 9`") && note.message.contains("daily · 09:00"),
        "{}",
        note.message
    );
    // The wire carries it as one nullable field; the bytes carry no cadence.
    let document = outcome_document(&out);
    assert_eq!(document["requested_trigger"]["kind"], "schedule");
    assert_eq!(document["requested_trigger"]["at"], "09:00");
    assert_eq!(document["requested_trigger"]["status"], "requires_binding");
    let doc = hot_with_model(intent);
    let bytes = serde_yaml_bw::to_string(&doc).unwrap().to_lowercase();
    for word in ["cron", "schedule", "every morning", "morning at 9", "09:00"] {
        assert!(
            !bytes.contains(word),
            "{word} leaked into the bytes: {doc:#}"
        );
    }
    assert_eq!(doc["const"]["source_glob"], "./inbox/*.md");
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    // French, with a named day and a time: weekly at 08:30.
    let out = compile(&CompileRequest::create(
        "Chaque lundi à 8h30, lis ./notes/*.md et écris un digest dans ./digest.md",
    ))
    .unwrap();
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let trigger = out.requested_trigger.as_ref().expect("requested_trigger");
    assert_eq!(trigger.cadence.as_deref(), Some("weekly"));
    assert_eq!(trigger.at.as_deref(), Some("08:30"));
    // A per-item trigger is an event whose firing supplies the item.
    let out = compile(&CompileRequest::create(
        "For each incoming ticket, classify it as bug or feature",
    ))
    .unwrap();
    let trigger = out.requested_trigger.as_ref().expect("requested_trigger");
    assert_eq!(trigger.kind, TriggerKind::Event);
    assert_eq!(trigger.payload_input.as_deref(), Some("item"));
    assert_eq!(trigger.cadence, None);
    // No trigger clause: nothing stated, no note.
    let out = compile(&CompileRequest::create(
        "Read ./report.md and post it to https://hooks.example.com/notify",
    ))
    .unwrap();
    assert!(out.requested_trigger.is_none(), "{out:#?}");
    assert!(!out.diagnostics.iter().any(|d| d.target == "trigger"));
    assert!(outcome_document(&out)["requested_trigger"].is_null());
}

#[test]
fn a_loopback_webhook_is_declared_and_declassified() {
    let out = compile(&CompileRequest::create(
        "Read ./report.md and post it to http://127.0.0.1:8766/notify, but ask me before sending",
    ))
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert_eq!(doc["permits"]["net"]["http"], json!(["127.0.0.1"]));
    assert_eq!(
        doc["const"]["publish_endpoint"],
        "http://127.0.0.1:8766/notify"
    );
    let boundary = out.requested_boundary.as_ref().unwrap();
    assert!(
        boundary
            .notes
            .iter()
            .any(|n| n.contains("127.0.0.1") && n.contains("loopback")),
        "{:?}",
        boundary.notes
    );
}
