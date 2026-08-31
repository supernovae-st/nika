// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika test <file>` — the golden test (F7 · workflows you can CI).
//!
//! check the workflow (the ADR-092 ladder · audit BEFORE run) · run it
//! under the SIMULATED plane — the MODEL is `mock/echo` (offline · zero
//! key · deterministic + schema-conformant since F3) AND real effects
//! are refused (P0-16 · no network, no subprocess, no writes: a task
//! that needs one fails with « effects disabled under `nika test` » —
//! it never silently performs it) · capture the typed `outputs:` as ONE
//! canonical JSON document · compare against the sibling golden
//! `<file>.golden.json`. `--update` (re)writes the golden instead.
//!
//! ```text
//! check dirty  → the findings render · exit 2
//! mock run ✗   → the failure card · exit 1
//! no golden    → how to create one · exit 3
//! mismatch     → a readable per-path diff · exit 1
//! match        → exit 0
//! ```

// Like `run`, the verb owns its streams (verdict/diff on stdout ·
// diagnostics on stderr) — the same sanctioned exemption.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde_json::Value;

use crate::Theme;
use crate::display::theme::Role;
use crate::verbs::exit;

/// Readable-diff budget — enough to see the shape of a drift without
/// flooding a CI log when a whole subtree changed.
const DIFF_LINE_CAP: usize = 20;

/// stdin (`-`) is `load_checked`'s seam, but this verb needs the FILE
/// twice over: the golden lives beside it (`<file>.golden.json`) and a
/// dirty doc re-renders through `check::run` — which would re-read an
/// already-consumed stdin and report a lying parse error. Refused with
/// guidance (spec §4: environment · exit 3).
fn refuse_stdin() -> u8 {
    eprintln!(
        "nika test: stdin (`-`) is not supported — the golden lives beside the file (<file>.golden.json)"
    );
    3
}

/// The `nika test <file> [--update]` verb (spec §4 exit contract).
#[must_use]
pub fn run(file: &str, update: bool, theme: Theme) -> u8 {
    run_with_answers(file, update, &[], theme)
}

fn capture_answered_outputs(
    wf: &nika_schema::raw::RawWorkflow,
    report: &nika_check::CheckReport,
    skills: std::collections::BTreeMap<String, String>,
    answer: &[String],
    theme: Theme,
) -> Result<(u8, std::collections::BTreeMap<String, Value>), u8> {
    let coerced = crate::verbs::run::coerce_answer_pairs(answer);
    let answers = nika_dap::resume::parse_answers(&coerced, wf).map_err(|message| {
        eprintln!("nika test: environment: {message}");
        exit::ENV
    })?;
    for task_id in answers.keys() {
        let is_prompt = wf.tasks.iter().any(|task| {
            task.value.id.value == *task_id
                && matches!(
                    &task.value.action,
                    nika_schema::raw::RawAction::Invoke(invoke)
                        if invoke.tool().map(|tool| tool.value.as_str()) == Some("nika:prompt")
                )
        });
        if !is_prompt {
            eprintln!(
                "nika test: environment: --answer {task_id}: only a direct `nika:prompt` task is answerable under the mock plane"
            );
            return Err(exit::ENV);
        }
    }
    super::run::capture_mock_outputs_with_answers(wf, report, skills, answers, theme).map_err(
        |message| {
            eprintln!("nika test: environment: {message}");
            exit::ENV
        },
    )
}

/// The golden test with explicit answers for blocking `nika:prompt` tasks.
///
/// An answer is an operator decision for this invocation. It never edits the
/// workflow and therefore never turns a security gate into a headless default.
#[must_use]
pub fn run_with_answers(file: &str, update: bool, answer: &[String], theme: Theme) -> u8 {
    if file == "-" {
        return refuse_stdin();
    }
    // ── Audit BEFORE run — the same gate `nika run` holds ───────────
    let (wf, report) = match crate::verbs::load_checked(file) {
        Ok(pair) => pair,
        Err(out) => {
            eprintln!("nika test: {}", out.text.trim_end());
            return out.code;
        }
    };
    // #473 · a golden pins the run WITH its skills composed, never without.
    let resolved = crate::verbs::resolve_workflow_skills(&wf, crate::verbs::workflow_base(file));
    if !report.is_clean() || !resolved.findings.is_empty() {
        // The SAME findings `nika check` renders — a dirty file never
        // pins (or judges) a golden.
        let out = crate::verbs::check::run(file, false, false, None, theme);
        print!("{}", out.text);
        return out.code;
    }

    // ── The mock run (simulated plane · offline · deterministic) ──────
    let (code, outputs) =
        match capture_answered_outputs(&wf, &report, resolved.texts, answer, theme) {
            Ok(pair) => pair,
            Err(code) => return code,
        };
    if code != exit::OK {
        eprintln!(
            "nika test: the mock run failed (exit {code}) — a golden pins a \
             GREEN run · fix the workflow first. If a blocking `nika:prompt` \
             needs a decision, rerun with `--answer TASK=VALUE`; never add a \
             headless default to a security gate"
        );
        return code;
    }

    let actual = match serde_json::to_value(&outputs) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("nika test: environment: cannot serialize outputs: {e}");
            return exit::ENV;
        }
    };
    let golden_path = golden_path_of(file);

    // ── `--update` — (re)write the golden and stop ───────────────────
    if update {
        return write_golden(&golden_path, &actual, theme);
    }

    // ── Compare against the golden ───────────────────────────────────
    let raw = match std::fs::read_to_string(&golden_path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            print!("{}", missing_golden_hint(file, &golden_path));
            return exit::ENV;
        }
        Err(e) => {
            eprintln!("nika test: environment: cannot read {golden_path}: {e}");
            return exit::ENV;
        }
    };
    let expected: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(e) => {
            eprintln!(
                "nika test: environment: {golden_path} is not valid JSON ({e}) \
                 — re-pin it with `nika test {file} --update`"
            );
            return exit::ENV;
        }
    };

    if expected == actual {
        // The match verdict says WHAT was pinned, not just that it holds:
        // top-level output keys + the canonical document's byte size.
        let keys = actual.as_object().map_or(0, serde_json::Map::len);
        let bytes = crate::display::shape::fmt_bytes(canonical_json(&actual).len());
        let key_word = if keys == 1 { "key" } else { "keys" };
        println!(
            "{} golden match · {keys} {key_word} · {bytes} {}",
            theme.paint(Role::Good, if theme.ascii { "ok" } else { "✔" }),
            theme.paint(
                Role::Dim,
                &format!("· {}", crate::verbs::linked_path(theme, &golden_path)),
            )
        );
        return exit::OK;
    }
    print!(
        "{}",
        render_mismatch(file, &golden_path, &expected, &actual, theme)
    );
    exit::WORKFLOW
}

/// The sibling golden path: the workflow file + `.golden.json` (no
/// extension surgery — the mapping stays unambiguous for any input name).
fn golden_path_of(file: &str) -> String {
    format!("{file}.golden.json")
}

/// Serialize the canonical golden document: pretty 2-space JSON, key-sorted
/// (`serde_json` maps are `BTreeMap`-backed · deterministic bytes), trailing
/// newline (a POSIX text file · clean diffs in the user's own VCS).
fn canonical_json(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_owned());
    text.push('\n');
    text
}

/// Write the golden (the `--update` arm) and report where it landed.
fn write_golden(golden_path: &str, actual: &Value, theme: Theme) -> u8 {
    if let Err(e) = std::fs::write(golden_path, canonical_json(actual)) {
        eprintln!("nika test: environment: cannot write {golden_path}: {e}");
        return exit::ENV;
    }
    println!(
        "{} golden written {}",
        theme.paint(Role::Good, if theme.ascii { "ok" } else { "✔" }),
        theme.paint(
            Role::Dim,
            &format!("· {}", crate::verbs::linked_path(theme, golden_path)),
        )
    );
    println!("  review it once, commit it — `nika test` now guards this workflow");
    exit::OK
}

/// The no-golden-yet teaching text (exit 3 · the operator asked to compare
/// against a pin that does not exist — say exactly how to create it).
fn missing_golden_hint(file: &str, golden_path: &str) -> String {
    format!(
        "nika test · no golden yet for {file}\n\
         \n  the golden pins this workflow's typed `outputs:` under the simulated\n\
         \x20 plane (mock model · effects refused · offline · deterministic) —\n\
         \x20 future runs must match it.\n\
         \n  create it:   nika test {file} --update\n\
         \x20 then commit: {golden_path}\n"
    )
}

/// Render the mismatch verdict + the readable per-path diff.
fn render_mismatch(
    file: &str,
    golden_path: &str,
    expected: &Value,
    actual: &Value,
    theme: Theme,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} outputs drifted from the golden {}",
        theme.paint(Role::Bad, if theme.ascii { "X" } else { "✖" }),
        theme.paint(
            Role::Dim,
            &format!("· {}", crate::verbs::linked_path(theme, golden_path)),
        )
    );
    for line in diff_lines(expected, actual) {
        let _ = writeln!(out, "  {line}");
    }
    // The ONE hint vocabulary (`label: command`) — same form as the
    // failure card's fix hint and the run epilogue's explore hint.
    let _ = writeln!(
        out,
        "\n  intended? {}",
        crate::display::vocab::hint(theme, "re-baseline", &format!("nika test {file} --update"))
    );
    out
}

/// The readable diff: one line per divergent JSON path, capped.
fn diff_lines(expected: &Value, actual: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    diff_at("outputs", expected, actual, &mut lines);
    if lines.len() > DIFF_LINE_CAP {
        let more = lines.len() - DIFF_LINE_CAP;
        lines.truncate(DIFF_LINE_CAP);
        lines.push(format!(
            "… and {}",
            crate::text::count(more, "more difference")
        ));
    }
    lines
}

/// Walk both trees, reporting the FIRST divergence per path (`golden →
/// run` orientation: the golden is the contract, the run is the accused).
fn diff_at(path: &str, expected: &Value, actual: &Value, out: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            let keys: BTreeSet<&String> = e.keys().chain(a.keys()).collect();
            for key in keys {
                let child = format!("{path}.{key}");
                match (e.get(key.as_str()), a.get(key.as_str())) {
                    (Some(ev), Some(av)) => diff_at(&child, ev, av, out),
                    (Some(ev), None) => {
                        out.push(format!("- {child} · missing (golden has {})", brief(ev)));
                    }
                    (None, Some(av)) => {
                        out.push(format!("+ {child} · not in the golden ({})", brief(av)));
                    }
                    (None, None) => {}
                }
            }
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                out.push(format!("~ {path} · length {} → {}", e.len(), a.len()));
            }
            for (i, (ev, av)) in e.iter().zip(a.iter()).enumerate() {
                diff_at(&format!("{path}[{i}]"), ev, av, out);
            }
        }
        (e, a) if e != a => {
            out.push(format!("~ {path} · golden {} → run {}", brief(e), brief(a)));
        }
        _ => {}
    }
}

/// One-line value preview for diff rows (long strings elided mid-value —
/// the diff names WHERE, the golden file holds the full WHAT).
fn brief(value: &Value) -> String {
    const CAP: usize = 80;
    let mut text = value.to_string();
    if text.chars().count() > CAP {
        text = format!("{}…", text.chars().take(CAP).collect::<String>());
    }
    text
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A noiseless theme — the tests exercise verdicts, not paint.
    fn plain_theme() -> Theme {
        Theme::new(false, true, false)
    }

    /// A deterministic mock workflow with a typed `outputs:` block.
    const GOLDEN_WF: &str = "nika: golden-fixture\nmodel: mock/echo\ntasks:\n  gen:\n    infer: { prompt: \"say hi\" }\noutputs:\n  reply: ${{ tasks.gen.output }}\n";

    fn stage(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("nika-cli-test-verb");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, GOLDEN_WF).expect("fixture written");
        // A stale golden from a previous run must not leak across tests.
        std::fs::remove_file(golden_path_of(&path.to_string_lossy())).ok();
        path
    }

    /// `--update` writes the golden · the immediate re-run matches (the
    /// pin → guard loop that makes a workflow CI-able).
    #[test]
    fn update_writes_the_golden_and_the_rerun_matches() {
        let wf = stage("update-then-match.nika.yaml");
        let file = wf.to_string_lossy();

        assert_eq!(run(&file, true, plain_theme()), exit::OK, "--update pins");
        let golden = golden_path_of(&file);
        let raw = std::fs::read_to_string(&golden).expect("golden written");
        let value: Value = serde_json::from_str(&raw).expect("golden is JSON");
        assert!(
            value.get("reply").is_some(),
            "the golden pins the typed outputs: {raw}"
        );
        assert!(raw.ends_with('\n'), "a POSIX text file");

        assert_eq!(
            run(&file, false, plain_theme()),
            exit::OK,
            "the mock run is deterministic — the fresh pin matches"
        );
    }

    /// A drifted golden fails the test (exit 1 · the CI gate).
    #[test]
    fn drifted_golden_is_a_mismatch() {
        let wf = stage("drift.nika.yaml");
        let file = wf.to_string_lossy();
        std::fs::write(
            golden_path_of(&file),
            "{\n  \"reply\": \"something else entirely\"\n}\n",
        )
        .expect("drifted golden staged");
        assert_eq!(run(&file, false, plain_theme()), exit::WORKFLOW);
    }

    /// No golden + no `--update` = teach, don't guess (exit 3).
    #[test]
    fn missing_golden_exits_env_with_the_hint() {
        let wf = stage("no-golden.nika.yaml");
        let file = wf.to_string_lossy();
        assert_eq!(run(&file, false, plain_theme()), exit::ENV);
        // The hint itself is pure — assert the teaching content directly.
        let hint = missing_golden_hint(&file, &golden_path_of(&file));
        assert!(hint.contains("--update"), "says HOW: {hint}");
        assert!(hint.contains(".golden.json"), "names the pin: {hint}");
    }

    #[test]
    fn explicit_answer_runs_a_blocking_prompt_without_a_default() {
        let dir = std::env::temp_dir().join("nika-cli-test-verb");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("answered-gate.nika.yaml");
        std::fs::write(
            &path,
            "nika: answered-gate\nmodel: mock/echo\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  approve:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"continue?\" }\noutputs:\n  answer: ${{ tasks.approve.output }}\n",
        )
        .expect("fixture written");
        let file = path.to_string_lossy();
        std::fs::remove_file(golden_path_of(&file)).ok();

        assert_ne!(
            run(&file, true, plain_theme()),
            exit::OK,
            "the default-free gate stays blocking without an operator answer"
        );
        assert_eq!(
            run_with_answers(&file, true, &["approve=false".to_owned()], plain_theme(),),
            exit::OK,
            "the invocation-bound decline executes safely"
        );
        let golden = std::fs::read_to_string(golden_path_of(&file)).expect("golden written");
        assert!(
            golden.contains("false"),
            "the explicit decision is pinned: {golden}"
        );
        assert!(
            !std::fs::read_to_string(&path)
                .expect("workflow kept")
                .contains("default:"),
            "nika test never edits the security gate"
        );
    }

    #[test]
    fn answer_for_a_non_prompt_task_refuses_instead_of_disappearing() {
        let wf = stage("non-prompt-answer.nika.yaml");
        let file = wf.to_string_lossy();
        assert_eq!(
            run_with_answers(&file, true, &["gen=yes".to_owned()], plain_theme()),
            exit::ENV
        );
    }

    #[test]
    fn answer_for_an_agent_task_refuses_under_the_mock_plane() {
        let dir = std::env::temp_dir().join("nika-cli-test-verb");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("agent-answer.nika.yaml");
        std::fs::write(
            &path,
            "nika: agent-answer\nmodel: mock/echo\ntasks:\n  decide:\n    agent: { prompt: \"continue?\", tools: [] }\n",
        )
        .expect("fixture written");
        assert_eq!(
            run_with_answers(
                &path.to_string_lossy(),
                true,
                &["decide=false".to_owned()],
                plain_theme(),
            ),
            exit::ENV,
            "the harness answer namespace is not imported into nika test"
        );
    }

    #[test]
    fn answered_branch_with_a_cloud_pin_still_uses_mock_echo() {
        let dir = std::env::temp_dir().join("nika-cli-test-verb");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("answered-cloud-pin.nika.yaml");
        std::fs::write(
            &path,
            "nika: answered-cloud-pin\nmodel: mock/echo\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  approve:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"continue?\" }\n  paid:\n    with: { go: \"${{ tasks.approve.output }}\" }\n    when: \"${{ with.go == true }}\"\n    infer:\n      model: openai/gpt-4o\n      prompt: hello\noutputs: { reply: \"${{ tasks.paid.output }}\" }\n",
        )
        .expect("fixture written");
        let file = path.to_string_lossy();
        std::fs::remove_file(golden_path_of(&file)).ok();

        assert_eq!(
            run_with_answers(&file, true, &["approve=true".to_owned()], plain_theme(),),
            exit::OK,
            "the answered branch stays offline even with a task-local cloud pin"
        );
        let golden = std::fs::read_to_string(golden_path_of(&file)).expect("golden written");
        assert!(
            golden.contains("mock(echo) · hello"),
            "the task-local provider was replaced by the mock plane: {golden}"
        );
    }

    /// A dirty file never judges a golden — the check findings render
    /// (exit 2), exactly like `nika run` refuses to execute.
    #[test]
    fn dirty_workflow_renders_findings_not_a_verdict() {
        let dir = std::env::temp_dir().join("nika-cli-test-verb");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("dirty.nika.yaml");
        // An unknown builtin tool is a CHECK finding (parses clean).
        std::fs::write(
            &path,
            "nika: dirty\ntasks:\n  t:\n    invoke: { tool: \"nika:no_such_tool\" }\n",
        )
        .expect("fixture written");
        assert_eq!(
            run(&path.to_string_lossy(), false, plain_theme()),
            exit::FILE
        );
    }

    /// The diff walks paths, reports drift/missing/extra rows, and caps.
    #[test]
    fn diff_lines_name_paths_missing_extra_and_cap() {
        let expected = json!({
            "verdict": "P0",
            "gone": true,
            "items": [1, 2, 3],
            "nested": { "score": 4 }
        });
        let actual = json!({
            "verdict": "P1",
            "extra": "new",
            "items": [1, 9],
            "nested": { "score": 5 }
        });
        let lines = diff_lines(&expected, &actual);
        let joined = lines.join("\n");
        assert!(
            joined.contains("~ outputs.verdict · golden \"P0\" → run \"P1\""),
            "{joined}"
        );
        assert!(joined.contains("- outputs.gone"), "{joined}");
        assert!(joined.contains("+ outputs.extra"), "{joined}");
        assert!(
            joined.contains("~ outputs.items · length 3 → 2"),
            "{joined}"
        );
        assert!(
            joined.contains("~ outputs.items[1] · golden 2 → run 9"),
            "{joined}"
        );
        assert!(joined.contains("~ outputs.nested.score"), "{joined}");

        // The cap: a 100-key drift renders 20 rows + one "and N more".
        let big_e: Value = (0..100)
            .map(|i| (format!("k{i:03}"), json!(1)))
            .collect::<serde_json::Map<_, _>>()
            .into();
        let big_a: Value = (0..100)
            .map(|i| (format!("k{i:03}"), json!(2)))
            .collect::<serde_json::Map<_, _>>()
            .into();
        let capped = diff_lines(&big_e, &big_a);
        assert_eq!(capped.len(), DIFF_LINE_CAP + 1);
        assert!(
            capped.last().expect("cap row").contains("80 more"),
            "{capped:?}"
        );
    }

    /// Long values are elided in diff rows — the row names WHERE.
    #[test]
    fn diff_previews_elide_long_values() {
        let long = "x".repeat(500);
        let mut out = Vec::new();
        diff_at("outputs.blob", &json!(long), &json!("short"), &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].len() < 200, "elided: {}", out[0]);
        assert!(out[0].contains('…'));
    }

    /// The golden path mapping is append-only (no extension surgery).
    #[test]
    fn golden_path_appends_the_suffix() {
        assert_eq!(
            golden_path_of("workflows/brief.nika.yaml"),
            "workflows/brief.nika.yaml.golden.json"
        );
    }

    /// Canonical bytes: pretty · key-sorted · newline-terminated.
    #[test]
    fn canonical_json_is_sorted_pretty_and_newline_terminated() {
        let value = json!({ "zeta": 1, "alpha": 2 });
        let text = canonical_json(&value);
        let a = text.find("alpha").expect("alpha present");
        let z = text.find("zeta").expect("zeta present");
        assert!(a < z, "key-sorted: {text}");
        assert!(text.ends_with("}\n"), "newline-terminated: {text:?}");
    }
}
