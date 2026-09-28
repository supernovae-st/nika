// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! B9 · E17-g02: a `model:` the invocation decides (`--var`, `--inputs-json`, a declared
//! default, a const, the envelope) meets the budget floor and the MODELS rung its literal twin
//! meets, before the first task. Every twin writes `./note.txt` first and only then asks, so an
//! absent note proves the ask never dispatched. The runs are keyless: neither a refusal nor a
//! regression can reach a provider. Mock controls run to their exact output under a zero cap.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const PAID: &str = "deepseek/deepseek-v4-pro";
const RENDERED: &str = "${{ inputs.m }}";
const DEFAULT_MOCK: &str =
    "inputs:\n  m: { type: string, required: false, default: \"mock/echo\" }\n";

/// The twin: a note first, then the ask on `model` (empty = the task names none).
fn twin(model: &str, max_tokens: u32, head: &str) -> String {
    let seat = if model.is_empty() {
        String::new()
    } else {
        format!("model: '{model}', ")
    };
    format!(
        "nika: twin\n{head}permits:\n  fs: {{ write: [\"./note.txt\"] }}\n  tools: [\"nika:write\"]\ntasks:\n  note:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"./note.txt\", content: before }} }}\n  ask:\n    after: {{ note: success }}\n    infer: {{ {seat}prompt: 'say hi', max_tokens: {max_tokens} }}\noutputs:\n  answer: ${{{{ tasks.ask.output }}}}\n"
    )
}

/// One isolated, keyless `nika run`: its output and whether the note was written.
fn run(source: &str, args: &[&str], stdin: Option<&str>) -> (Output, bool) {
    let room = tempfile::tempdir().expect("room");
    std::fs::create_dir(room.path().join("home")).expect("home");
    std::fs::write(room.path().join("twin.nika"), source).expect("workflow");
    let mut child = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["run", "twin.nika"])
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room.path().join("home"))
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .current_dir(room.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("isolated binary");
    let mut pipe = child.stdin.take().expect("stdin");
    pipe.write_all(stdin.unwrap_or_default().as_bytes())
        .expect("stdin");
    drop(pipe);
    let out = child.wait_with_output().expect("the run settles");
    (out, noted(room.path()))
}

/// The note in place, or moved aside by a failed run's quarantine.
fn noted(room: &Path) -> bool {
    room.join("note.txt").exists()
        || std::fs::read_dir(room.join(".nika/quarantine"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| entry.path().join("note.txt").exists())
}

fn text(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Every spelling of the paid seat refuses under a cap below its floor, with no note.
#[test]
fn every_spelling_of_a_paid_seat_meets_the_floor_before_the_first_task() {
    let default =
        format!("inputs:\n  m: {{ type: string, required: false, default: \"{PAID}\" }}\n");
    let constant = format!("{DEFAULT_MOCK}const:\n  m: \"{PAID}\"\n");
    let envelope = format!("model: '{RENDERED}'\n{DEFAULT_MOCK}");
    let bound = format!("m={PAID}");
    let json = format!("{{\"m\": \"{PAID}\"}}");
    let cases: [(&str, String, Vec<&str>, Option<&str>); 7] = [
        ("literal", twin(PAID, 512, DEFAULT_MOCK), vec![], None),
        (
            "--var",
            twin(RENDERED, 512, DEFAULT_MOCK),
            vec!["--var", bound.as_str()],
            None,
        ),
        (
            "--inputs-json",
            twin(RENDERED, 512, DEFAULT_MOCK),
            vec!["--inputs-json", "-"],
            Some(json.as_str()),
        ),
        ("default", twin(RENDERED, 512, &default), vec![], None),
        (
            "const",
            twin("${{ const.m }}", 512, &constant),
            vec![],
            None,
        ),
        (
            "envelope",
            twin("", 512, &envelope),
            vec!["--var", bound.as_str()],
            None,
        ),
        (
            "task over --model",
            twin(RENDERED, 512, DEFAULT_MOCK),
            vec!["--model", "mock/echo", "--var", bound.as_str()],
            None,
        ),
    ];
    for cap in ["0", "0.001"] {
        for (name, source, args, stdin) in &cases {
            let mut argv = args.clone();
            argv.extend(["--max-cost-usd", cap]);
            let (out, noted) = run(source, &argv, *stdin);
            assert_eq!(out.status.code(), Some(2), "{name} {cap}: {}", text(&out));
            assert!(
                text(&out).contains("NIKA-1709"),
                "{name} {cap}: {}",
                text(&out)
            );
            assert!(!noted, "{name} {cap}: no task ran");
        }
    }
}

/// A reasoning seat under its cap floor is the MODELS rung's refusal, however it is spelled.
#[test]
fn a_rendered_reasoning_seat_meets_the_models_rung_before_the_first_task() {
    let bound = format!("m={PAID}");
    let json = format!("{{\"m\": \"{PAID}\"}}");
    for (name, source, args, stdin) in [
        ("literal", twin(PAID, 64, DEFAULT_MOCK), vec![], None),
        (
            "--var",
            twin(RENDERED, 64, DEFAULT_MOCK),
            vec!["--var", bound.as_str()],
            None,
        ),
        (
            "--inputs-json",
            twin(RENDERED, 64, DEFAULT_MOCK),
            vec!["--inputs-json", "-"],
            Some(json.as_str()),
        ),
    ] {
        let (out, noted) = run(&source, &args, stdin);
        assert_eq!(out.status.code(), Some(2), "{name}: {}", text(&out));
        assert!(
            text(&out).contains("too small for reasoning seat `deepseek/deepseek-v4-pro`"),
            "{name}: {}",
            text(&out)
        );
        assert!(!noted, "{name}: no task ran");
    }
}

/// The controls: mock however it is seated, and `--model` replacing a rendered envelope, run
/// under a zero cap to the exact answer.
#[test]
fn mock_controls_run_to_their_exact_answer_under_a_zero_cap() {
    let envelope = format!("model: '{RENDERED}'\n{DEFAULT_MOCK}");
    let bound = format!("m={PAID}");
    for (name, source, args) in [
        ("literal", twin("mock/echo", 64, DEFAULT_MOCK), vec![]),
        (
            "--var",
            twin(RENDERED, 64, DEFAULT_MOCK),
            vec!["--var", "m=mock/echo"],
        ),
        ("default", twin(RENDERED, 64, DEFAULT_MOCK), vec![]),
        (
            "--model over the envelope",
            twin("", 64, &envelope),
            vec!["--model", "mock/echo", "--var", bound.as_str()],
        ),
    ] {
        let mut argv = args.clone();
        argv.extend(["--max-cost-usd", "0"]);
        let (out, noted) = run(&source, &argv, None);
        assert_eq!(out.status.code(), Some(0), "{name}: {}", text(&out));
        assert!(
            text(&out).contains("mock(echo) · say hi"),
            "{name}: {}",
            text(&out)
        );
        assert!(noted, "{name}: the note precedes the ask");
    }
}

/// A fan over `inputs.xs` declaring `default`, whose items each ask the paid seat once; a note
/// is written first, so an absent note proves no task ran.
fn input_fan(default: &str) -> String {
    format!(
        "nika: fan\ninputs:\n  xs: {{ type: {{ array: string }}, required: false, default: {default} }}\npermits:\n  fs: {{ write: [\"./note.txt\"] }}\n  tools: [\"nika:write\"]\ntasks:\n  note:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"./note.txt\", content: before }} }}\n  ask:\n    after: {{ note: success }}\n    for_each: {{ items: \"${{{{ inputs.xs }}}}\", max_parallel: 1 }}\n    infer: {{ model: '{PAID}', prompt: 'say ${{{{ item }}}}', max_tokens: 512 }}\n"
    )
}

/// One floor case: its name, workflow, arguments, stdin and whether the floor refuses it.
type FloorCase<'a> = (&'a str, &'a String, Vec<&'a str>, Option<&'a str>, bool);

/// Runs each case under `cap` (0.003: one call fits, five do not). A refused case exits 2 on
/// NIKA-1709. An admitted one passes the floor and meets the next launch gate: keyless, the
/// access gate refuses it (NIKA-1800). Either way no task runs, so no note is written.
fn floor_cases(cap: &str, cases: &[FloorCase<'_>]) {
    for (name, source, args, stdin, refused) in cases {
        let mut argv = args.clone();
        argv.extend(["--max-cost-usd", cap]);
        let (out, noted) = run(source, &argv, *stdin);
        if *refused {
            assert_eq!(out.status.code(), Some(2), "{name}: {}", text(&out));
            assert!(text(&out).contains("NIKA-1709"), "{name}: {}", text(&out));
        } else {
            assert!(!text(&out).contains("NIKA-1709"), "{name}: {}", text(&out));
            assert!(text(&out).contains("NIKA-1800"), "{name}: {}", text(&out));
        }
        assert!(!noted, "{name}: no task ran");
    }
}

/// B11 · B1: a `for_each` over an input meets the floor of the items the invocation gives it, not
/// of its declared default. Five items given over a one-item default refuse before the first task
/// (NIKA-1709, no note), by `--inputs-json` and by `--var` alike. Each default alone keeps its own
/// floor.
#[test]
fn a_fan_over_an_input_meets_the_floor_of_the_items_it_is_given() {
    let one = input_fan("[\"a\"]");
    let five = input_fan("[\"a\", \"b\", \"c\", \"d\", \"e\"]");
    floor_cases(
        "0.003",
        &[
            (
                "five by --inputs-json over one",
                &one,
                vec!["--inputs-json", "-"],
                Some("{\"xs\": [\"a\", \"b\", \"c\", \"d\", \"e\"]}"),
                true,
            ),
            (
                "five by --var over one",
                &one,
                vec!["--var", "xs=[\"a\",\"b\",\"c\",\"d\",\"e\"]"],
                None,
                true,
            ),
            ("the one-item default", &one, vec![], None, false),
            ("the five-item default", &five, vec![], None, true),
        ],
    );
}

/// B11 · B1: the CLI's own preflight prices the items the invocation gives, as the runtime's gate
/// does. One item given over a five-item default is admitted (the floor passes; keyless, the
/// access gate refuses next), by `--inputs-json` and by `--var` alike. An explicit empty list is
/// zero calls, even under an explicit zero cap, while five given under that cap refuse. `--model`
/// composes with the given items on a model-less fan. A value that is not a list never falls back
/// to the default: the invocation is refused as mistyped before any gate prices it.
#[test]
fn a_smaller_bound_fan_is_admitted_where_its_default_is_refused() {
    let one = input_fan("[\"a\"]");
    let five = input_fan("[\"a\", \"b\", \"c\", \"d\", \"e\"]");
    let seatless = |default: &str| {
        input_fan(default)
            .replace("tasks:\n", "model: mock/echo\ntasks:\n")
            .replace(&format!("model: '{PAID}', "), "")
    };
    let (seatless_one, seatless_five) = (
        seatless("[\"a\"]"),
        seatless("[\"a\", \"b\", \"c\", \"d\", \"e\"]"),
    );
    let five_json = Some("{\"xs\": [\"a\", \"b\", \"c\", \"d\", \"e\"]}");
    floor_cases(
        "0.003",
        &[
            (
                "one by --inputs-json over five",
                &five,
                vec!["--inputs-json", "-"],
                Some("{\"xs\": [\"a\"]}"),
                false,
            ),
            (
                "one by --var over five",
                &five,
                vec!["--var", "xs=[\"a\"]"],
                None,
                false,
            ),
            (
                "an empty list over five",
                &five,
                vec!["--inputs-json", "-"],
                Some("{\"xs\": []}"),
                false,
            ),
            (
                "--model with five over one",
                &seatless_one,
                vec!["--model", PAID, "--inputs-json", "-"],
                five_json,
                true,
            ),
            (
                "--model with one over five",
                &seatless_five,
                vec!["--model", PAID, "--var", "xs=[\"a\"]"],
                None,
                false,
            ),
        ],
    );
    floor_cases(
        "0",
        &[
            (
                "an empty list under a zero cap",
                &five,
                vec!["--var", "xs=[]"],
                None,
                false,
            ),
            (
                "five under a zero cap",
                &one,
                vec!["--inputs-json", "-"],
                five_json,
                true,
            ),
        ],
    );
    let (out, noted) = run(
        &one,
        &["--inputs-json", "-", "--max-cost-usd", "0.003"],
        Some("{\"xs\": \"a\"}"),
    );
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    assert!(
        text(&out).contains("does not conform to its declared type"),
        "{}",
        text(&out)
    );
    assert!(
        !text(&out).contains("NIKA-1709") && !noted,
        "{}",
        text(&out)
    );
}

/// B11 · the cleanup lane meets the main lane's guard: an `unwind` cleanup whose seat only the
/// run decides (its producer's output names it) is refused before its request under a zero cap.
/// The run still succeeds (a cleanup is best-effort) and the refusal is journaled on the cleanup
/// lane. Keyless: before the fix this cleanup reached the provider call and failed on the key.
#[test]
fn a_cleanup_seat_only_the_run_decides_meets_the_guard() {
    let source = format!(
        "nika: twin\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  pick:\n    invoke: {{ tool: \"nika:jq\", args: {{ input: {{ m: \"{PAID}\" }}, expression: \".m\" }} }}\n  tidy:\n    after: {{ pick: unwind }}\n    with: {{ m: \"${{{{ tasks.pick.output }}}}\" }}\n    infer: {{ model: '${{{{ with.m }}}}', prompt: 'say hi', max_tokens: 512 }}\n"
    );
    let (out, _) = run(&source, &["--json", "--max-cost-usd", "0"], None);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        text(&out).contains("NIKA-1704")
            && text(&out).contains("refused before the provider request"),
        "{}",
        text(&out)
    );
}
