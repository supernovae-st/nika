// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! D2 · `check` judges a seat's declared capabilities before the run, through the ONE door:
//! what the run refuses on the seat is not RUN READY here; what it now carries (`schema:`,
//! `vision:` files) is.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;

use super::*;

fn audit(source: &str) -> Audit {
    audit_source(source, "w.nika", None, None, AuditOptions::default()).expect("the fixture parses")
}

/// A Codex seat over ACP, one `infer:` task whose body is `body`.
fn seat_infer(body: &str) -> String {
    format!(
        "nika: seat-probe\nmodel: openai/gpt-6-luna\nrun:\n  access:\n    via: codex\n    \
         protocol: acp\npermits:\n  fs:\n    read: [\"./in/**\"]\ntasks:\n  ask:\n    infer:\n      \
         prompt: \"describe it\"\n{body}"
    )
}

fn seat_blockers(audit: &Audit) -> Vec<&str> {
    (audit.verdict.layers.blockers.iter())
        .filter(|b| b.starts_with("seat · "))
        .map(String::as_str)
        .collect()
}

#[test]
fn schema_and_vision_files_are_carried_on_the_seat() {
    for body in [
        "      schema: { type: object, required: [a], properties: { a: { type: string } } }\n",
        "      vision:\n        - { source: file, path: \"./in/logo.png\" }\n",
        "",
    ] {
        let judged = audit(&seat_infer(body));
        assert!(
            seat_blockers(&judged).is_empty(),
            "{body}: {:?}",
            judged.verdict.layers.blockers
        );
        assert!(judged.verdict.seated.contains("ask"), "{body}");
    }
}

/// The gauntlet's D2 probes, as files: what the seat cannot carry blocks RUN READY with the
/// field named, where `check` used to say `run ready ✔` and the run refused.
#[test]
fn what_the_seat_cannot_carry_is_not_run_ready() {
    for (body, named) in [
        ("      temperature: 0.2\n", "`temperature`"),
        ("      max_tokens: 400\n", "`max_tokens`"),
        (
            "      thinking: { enabled: true, budget_tokens: 2000 }\n",
            "`thinking`",
        ),
        (
            "      vision:\n        - { source: url, url: \"https://example.com/a.png\" }\n",
            "a `vision:` URL",
        ),
    ] {
        let judged = audit(&seat_infer(body));
        let blockers = seat_blockers(&judged);
        assert_eq!(blockers.len(), 1, "{body}: {blockers:?}");
        assert!(
            blockers[0].contains("`ask` is an infer: on `codex` over ACP")
                && blockers[0].contains(named)
                && blockers[0].contains("NIKA-432"),
            "{}",
            blockers[0]
        );
        assert_eq!(
            judged.verdict.layers.run_ready(),
            Some(false),
            "{body}: never run-ready"
        );
    }
}

#[test]
fn an_agent_whitelist_on_a_seat_and_images_without_one_block() {
    let whitelist = audit(
        "nika: seat-agent\nmodel: openai/gpt-6-luna\nrun:\n  access:\n    via: codex\n    \
         protocol: acp\npermits:\n  tools: [\"nika:read\"]\ntasks:\n  draw:\n    agent:\n      \
         prompt: \"go\"\n      tools: [\"nika:read\"]\n",
    );
    let blockers = seat_blockers(&whitelist);
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(blockers[0].contains("`tools:` whitelist") && blockers[0].contains("NIKA-465"));

    let native = audit(
        "nika: native-images\nmodel: mock/echo\npermits:\n  fs:\n    write: [\"./out/**\"]\n\
         tasks:\n  draw:\n    agent:\n      prompt: \"go\"\n      images:\n        save: \
         \"./out/a.png\"\n",
    );
    let blockers = seat_blockers(&native);
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(
        blockers[0].contains("`draw` declares `images:`") && blockers[0].contains("no seat"),
        "{}",
        blockers[0]
    );
}

/// B-D18 · an `images.save` agent on the seat, chained into `nika:image_fx`: run-ready, its
/// write judged inside `permits.fs.write`, the harness hint stated, no `cost` hint pushing
/// `max_tokens_total` on the seat.
#[test]
fn a_saved_seat_image_chained_into_image_fx_checks_clean() {
    let source = "nika: banner\nmodel: openai/gpt-6-luna\nrun:\n  access:\n    via: codex\n    \
         protocol: acp\npermits:\n  fs:\n    read: [\"./out/**\"]\n    write: [\"./out/**\"]\n  \
         tools: [\"nika:image_fx\"]\ntasks:\n  banner:\n    agent:\n      prompt: \"draw\"\n      \
         images:\n        save: \"./out/banner.png\"\n  fx:\n    with:\n      banner: \
         ${{ tasks.banner.output.images[0].path }}\n    invoke:\n      tool: \"nika:image_fx\"\n      \
         args: { input: \"${{ with.banner }}\", out: \"./out/fx.png\", ops: [{ grayscale: {} }] }\n";
    let chained = audit(source);
    assert!(chained.report.is_clean(), "{:?}", chained.report.findings);
    assert!(
        seat_blockers(&chained).is_empty(),
        "{:?}",
        chained.verdict.layers.blockers
    );
    assert!(chained.verdict.harness.iter().any(|h| h.task == "banner"));
    let obj = audit_json(
        &chained.wf,
        &chained.report,
        &chained.skills,
        &chained.verdict,
        Lanes::default(),
    )
    .expect("serializes");
    let cost: Vec<&Value> = (obj["hints"].as_array().expect("hints"))
        .iter()
        .filter(|row| row["kind"] == "cost" && row["task"] == "banner")
        .collect();
    assert!(
        cost.is_empty(),
        "no max_tokens_total pushed on the seat: {cost:?}"
    );

    // The engine's write is judged like any file write: outside fs.write, refused statically.
    let escaped = audit(&source.replace("save: \"./out/banner.png\"", "save: \"./banner.png\""));
    assert!(
        escaped.report.findings.iter().any(|f| f.code.as_deref() == Some("NIKA-SEC-004")
            && f.message.contains("images.save")),
        "{:?}",
        escaped.report.findings
    );
}
