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

/// `thinking:` blocks exactly where the run refuses it: an enabled budget reaches the
/// session's refusal (NIKA-432); a disabled block, or one with no budget, is never sent.
#[test]
fn thinking_blocks_only_where_the_run_refuses_it() {
    for body in [
        "      thinking: { enabled: false }\n",
        "      thinking: { enabled: false, budget_tokens: 2000 }\n",
        "      thinking: { enabled: true }\n",
    ] {
        let judged = audit(&seat_infer(body));
        assert!(
            seat_blockers(&judged).is_empty(),
            "{body}: {:?}",
            judged.verdict.layers.blockers
        );
    }
}

/// A seat with no audited ACP one-shot (gemini-cli) is the access line's refusal: the seat
/// rung adds no field blocker claiming what that seat would carry.
#[test]
fn a_seat_with_no_one_shot_profile_is_left_to_the_access_line() {
    let source = seat_infer("      temperature: 0.2\n").replace("via: codex", "via: gemini-cli");
    let judged = audit(&source);
    assert!(
        seat_blockers(&judged).is_empty(),
        "{:?}",
        judged.verdict.layers.blockers
    );
    assert_eq!(
        judged.verdict.layers.run_ready(),
        Some(false),
        "the access line refuses"
    );
}

/// A seat route with no `protocol: acp` (its direct one-shot), one `infer:` whose body is `body`.
fn direct_infer(via: &str, model: &str, body: &str) -> String {
    format!(
        "nika: seat-direct\nmodel: {model}\nrun:\n  access:\n    via: {via}\npermits:\n  fs:\n    \
         read: [\"./in/**\"]\ntasks:\n  ask:\n    infer:\n      prompt: \"describe it\"\n{body}"
    )
}

const VISION_FILE: &str = "      vision:\n        - { source: file, path: \"./in/logo.png\" }\n";
const VISION_URL: &str =
    "      vision:\n        - { source: url, url: \"https://example.com/a.png\" }\n";

/// `codex exec` and `claude -p` put no image on their command: a `vision:` task there is not
/// RUN READY, with the run's code and the route that carries the image, where `check` said
/// `run ready ✔` and the run answered without it. The ACP one-shot carries files, not a URL:
/// a URL's remedy leads with the API route and names what the ACP route would refuse.
#[test]
fn a_vision_task_on_a_direct_seat_is_not_run_ready() {
    let api_first = |via: &str| {
        format!(
            "— declare an API route or drop `vision:`; `run.access: {{ via: {via}, protocol: acp \
             }}` carries `vision:` files, not a `vision:` URL (Nika does not fetch it: use a local \
             file)"
        )
    };
    for (via, model, file_remedy, url_remedy) in [
        (
            "codex",
            "openai/gpt-6-luna",
            "— declare `run.access: { via: codex, protocol: acp }` (its ACP one-shot carries \
             `vision:` files), drop `vision:` or declare an API route"
                .to_owned(),
            api_first("codex"),
        ),
        (
            "claude-code",
            "anthropic/claude-opus-4-6",
            "— declare `run.access: { via: claude-code, protocol: acp }` (its ACP one-shot \
             carries `vision:` files), drop `vision:` or declare an API route"
                .to_owned(),
            api_first("claude-code"),
        ),
        // No ACP one-shot that reads images: the remedy never names one.
        (
            "grok-build",
            "xai/grok-4",
            "— drop `vision:` or declare an API route".to_owned(),
            "— drop `vision:` or declare an API route".to_owned(),
        ),
    ] {
        for (vision, remedy) in [(VISION_FILE, &file_remedy), (VISION_URL, &url_remedy)] {
            let judged = audit(&direct_infer(via, model, vision));
            let blockers = seat_blockers(&judged);
            assert_eq!(blockers.len(), 1, "{via} {vision}: {blockers:?}");
            assert!(
                blockers[0].contains(&format!("`ask` is an infer: on `{via}`'s direct one-shot"))
                    && blockers[0].contains(
                        "whose command carries the prompt, system, model and a `schema:`, never \
                         an image"
                    )
                    && blockers[0].contains("NIKA-432")
                    && blockers[0].ends_with(remedy.as_str()),
                "{}",
                blockers[0]
            );
            assert_eq!(
                judged.verdict.layers.run_ready(),
                Some(false),
                "{via}: never run-ready"
            );
            assert!(judged.verdict.seated.contains("ask"), "{via}");
        }
    }
}

/// The remedy never leads to a second refusal: a task that also sets what the ACP one-shot
/// refuses is sent to an API route first, with those fields named, and the clean remedy is
/// one the ACP judge then admits.
#[test]
fn the_direct_seat_remedy_names_what_the_acp_route_would_refuse() {
    let body = format!("      max_tokens: 400\n      temperature: 0.2\n{VISION_FILE}");
    let judged = audit(&direct_infer("codex", "openai/gpt-6-luna", &body));
    let blockers = seat_blockers(&judged);
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(
        blockers[0].ends_with(
            "— declare an API route or drop `vision:`; `run.access: { via: codex, protocol: acp \
             }` carries `vision:` files, not `temperature` · `max_tokens`"
        ),
        "{}",
        blockers[0]
    );
    // Following the clean remedy as written: the ACP judge has nothing left to refuse.
    let followed = audit(&seat_infer(VISION_FILE));
    assert!(
        seat_blockers(&followed).is_empty(),
        "{:?}",
        followed.verdict.layers.blockers
    );
}

/// `copilot`'s direct command is text-grade: its blocker never says it carries a `schema:`,
/// and a task with both `schema:` and `vision:` reads the same NIKA-432 the run now gives.
#[test]
fn a_text_grade_seat_blocker_says_what_its_command_carries() {
    let body = format!(
        "      schema: {{ type: object, required: [a], properties: {{ a: {{ type: string }} }} \
         }}\n{VISION_FILE}"
    );
    let judged = audit(&direct_infer("copilot", "openai/gpt-6-luna", &body));
    let blockers = seat_blockers(&judged);
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(
        blockers[0].contains(
            "`ask` is an infer: on `copilot`'s direct one-shot, whose command carries the prompt, \
             system and model, never an image"
        ) && !blockers[0].contains("schema:")
            && blockers[0].contains("(NIKA-432) — drop `vision:` or declare an API route"),
        "{}",
        blockers[0]
    );
}

/// The direct one-shot carries a `schema:`: only `vision:` blocks it here.
#[test]
fn a_direct_seat_task_without_vision_has_no_seat_blocker() {
    for body in [
        "",
        "      schema: { type: object, required: [a], properties: { a: { type: string } } }\n",
    ] {
        let judged = audit(&direct_infer("codex", "openai/gpt-6-luna", body));
        assert!(
            seat_blockers(&judged).is_empty(),
            "{body}: {:?}",
            judged.verdict.layers.blockers
        );
    }
}

/// A seat with no direct one-shot (gemini-cli) is the access line's refusal: the seat rung
/// claims nothing about what that seat's command would carry.
#[test]
fn a_seat_with_no_direct_one_shot_is_left_to_the_access_line() {
    let judged = audit(&direct_infer(
        "gemini-cli",
        "openai/gpt-6-luna",
        "      vision:\n        - { source: file, path: \"./in/logo.png\" }\n",
    ));
    assert!(
        seat_blockers(&judged).is_empty(),
        "{:?}",
        judged.verdict.layers.blockers
    );
    assert_eq!(
        judged.verdict.layers.run_ready(),
        Some(false),
        "the access line refuses"
    );
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
