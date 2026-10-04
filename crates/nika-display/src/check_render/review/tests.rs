// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

use super::*;

fn report(yaml: &str) -> nika_check::CheckReport {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
    nika_check::check(&wf)
}

/// A copy that reads one file and writes another, with no model call.
const COPY: &str = "nika: copy\npermits: { fs: { read: [\"./a.md\"], write: [\"./b.md\"] }, tools: [\"nika:read\", \"nika:write\"] }\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./a.md\" } }\n  write:\n    with: { text: \"${{ tasks.read.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./b.md\", content: \"${{ with.text }}\" } }\n";

/// C10 · a review names each effect class the report's own permits and requirements need, the
/// spend a run can reach last: no model call spends nothing on inference, an inference with no
/// token bound stays unbounded, never a zero.
#[test]
fn a_review_names_each_effect_class_then_the_spend() {
    let rows = effect_rows(&report(COPY));
    assert!(
        rows.iter()
            .any(|r| r.starts_with("reads ") && r.contains("a.md")),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|r| r.starts_with("writes ") && r.contains("b.md")),
        "{rows:?}"
    );
    assert!(
        rows.iter().any(|r| r == "tools nika:read · nika:write"),
        "{rows:?}"
    );
    assert_eq!(
        rows.last().map(String::as_str),
        Some("model output estimate · $0 · no direct model task in these checked bytes")
    );
    let open = "nika: open\nmodel: mock/echo\npermits: {}\ntasks:\n  draft:\n    infer: { prompt: \"Say hello\" }\n";
    let rows = effect_rows(&report(open));
    assert!(
        rows.iter()
            .any(|r| r.starts_with("model output estimate · unbounded")),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|r| r.contains("$0")),
        "no zero claimed: {rows:?}"
    );
}

/// C10 · a review lists the report's first findings (`code · message`, at most eight) and hints
/// (`kind · advice`, at most four), in the report's order.
#[test]
fn a_review_lists_the_first_findings_and_hints_in_the_reports_order() {
    let bare = "nika: bare\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./a.md\" } }\n";
    let report = report(bare);
    let (findings, hints) = finding_rows(&report);
    assert!(!findings.is_empty(), "an absent permits block is found");
    assert!(findings.len() <= 8 && hints.len() <= 4);
    let first = &report.findings[0];
    assert_eq!(
        findings[0],
        format!(
            "{} · {}",
            first.code.as_deref().unwrap_or("-"),
            first.message
        )
    );
    let (clean, _) = finding_rows(&self::report(COPY));
    assert!(clean.is_empty(), "{clean:?}");
}

/// A builtin's face is what it does; an unknown tool keeps its id.
#[test]
fn a_builtin_face_is_what_it_does_never_its_id() {
    assert_eq!(builtin_face("nika:jq"), "shapes the data");
    assert_eq!(builtin_face("nika:read"), "reads a file");
    assert_eq!(builtin_face("mcp:slack/post"), "mcp:slack/post");
    for tool in [
        "nika:jq",
        "nika:glob",
        "nika:grep",
        "nika:assert",
        "nika:prompt",
    ] {
        assert!(!builtin_face(tool).contains(':'), "{tool}");
    }
}

/// The review-text byte oracle: every expected row below was captured by
/// running the immutable pre-move Session code
/// (`nika_session::review::{plan_lines, plan_lines_in_order}`) on these same
/// candidates and waves, never by the code under test.
mod plan_oracle {
    use std::fmt::Write as _;

    use super::*;

    type Plan = dyn Fn(&str) -> Vec<String>;
    type Ordered = dyn Fn(&str, &[Vec<usize>]) -> Vec<String>;

    const BUILTINS: [&str; 29] = [
        "read",
        "write",
        "edit",
        "glob",
        "grep",
        "jq",
        "json_diff",
        "json_merge_patch",
        "validate",
        "assert",
        "decide",
        "done",
        "prompt",
        "fetch",
        "notify",
        "emit",
        "log",
        "wait",
        "date",
        "uuid",
        "hash",
        "convert",
        "compose",
        "inspect",
        "chart",
        "image_generate",
        "image_fx",
        "tts_generate",
        "zzz_unknown",
    ];

    fn candidates() -> Vec<(&'static str, String)> {
        let mut builtins = "nika: b\ntasks:\n".to_owned();
        for (i, tool) in BUILTINS.iter().enumerate() {
            let _ = write!(
                builtins,
                "  t{i:02}:\n    invoke: {{ tool: \"nika:{tool}\", args: {{}} }}\n"
            );
        }
        vec![
            (
                "verbs",
                "nika: v\nmodel: mistral/mistral-small-latest\ntasks:\n  a_infer:\n    infer: { prompt: hi }\n  b_own:\n    infer: { prompt: hi, model: ollama/llama3 }\n  c_exec:\n    exec: { command: [\"true\"] }\n  d_agent:\n    agent: { prompt: go }\n  e_mcp:\n    invoke: { tool: \"mcp:slack/post\", args: {} }\n  f_flow:\n    invoke: { workflow: ./sub.nika }\n".to_owned(),
            ),
            (
                "no_model",
                "nika: n\ntasks:\n  only:\n    infer: { prompt: x }\n".to_owned(),
            ),
            (
                "each",
                "nika: e\ninputs:\n  urls: { type: array, default: [] }\ntasks:\n  fetch_each:\n    for_each: { items: \"${{ inputs.urls }}\" }\n    invoke: { tool: \"nika:fetch\", args: { url: \"${{ item }}\" } }\n  sum:\n    for_each: { items: \"${{ inputs.urls }}\" }\n    infer: { prompt: \"${{ item }}\" }\n".to_owned(),
            ),
            (
                "unicode",
                "nika: u\nmodel: \"mock/é ✨\"\ntasks:\n  tâche_é:\n    infer: { prompt: \"« x »\" }\n".to_owned(),
            ),
            ("builtins", builtins),
            ("broken", "nika: [\n".to_owned()),
            ("empty", String::new()),
        ]
    }

    pub(super) fn cases(plan: &Plan, ordered: &Ordered) -> Vec<(String, String)> {
        let waves = [
            ("reversed", vec![vec![2, 0], vec![1]]),
            ("partial_out_of_range", vec![vec![1, 99]]),
            ("duplicate", vec![vec![0, 0], vec![0]]),
        ];
        let mut out = Vec::new();
        for (name, candidate) in candidates() {
            out.push((format!("{name}/file"), format!("{:?}", plan(&candidate))));
            for (w, wave) in &waves {
                out.push((
                    format!("{name}/{w}"),
                    format!("{:?}", ordered(&candidate, wave)),
                ));
            }
        }
        out
    }

    /// `(case, Debug of the rows)` captured from the pre-move Session code; never recomputed here.
    const EXPECTED: &[(&str, &str)] = &[
        (
            "verbs/file",
            "[\"  1. a_infer · infer · mistral/mistral-small-latest\", \"  2. b_own · infer · ollama/llama3\", \"  3. c_exec · exec · runs a program\", \"  4. d_agent · agent · a bounded multi-turn loop\", \"  5. e_mcp · mcp:slack/post\", \"  6. f_flow · invoke · another workflow\"]",
        ),
        (
            "verbs/reversed",
            "[\"  1. c_exec · exec · runs a program\", \"  2. a_infer · infer · mistral/mistral-small-latest\", \"  3. b_own · infer · ollama/llama3\", \"  4. d_agent · agent · a bounded multi-turn loop\", \"  5. e_mcp · mcp:slack/post\", \"  6. f_flow · invoke · another workflow\"]",
        ),
        (
            "verbs/partial_out_of_range",
            "[\"  1. b_own · infer · ollama/llama3\", \"  2. a_infer · infer · mistral/mistral-small-latest\", \"  3. c_exec · exec · runs a program\", \"  4. d_agent · agent · a bounded multi-turn loop\", \"  5. e_mcp · mcp:slack/post\", \"  6. f_flow · invoke · another workflow\"]",
        ),
        (
            "verbs/duplicate",
            "[\"  1. a_infer · infer · mistral/mistral-small-latest\", \"  2. a_infer · infer · mistral/mistral-small-latest\", \"  3. a_infer · infer · mistral/mistral-small-latest\", \"  4. b_own · infer · ollama/llama3\", \"  5. c_exec · exec · runs a program\", \"  6. d_agent · agent · a bounded multi-turn loop\", \"  7. e_mcp · mcp:slack/post\", \"  8. f_flow · invoke · another workflow\"]",
        ),
        (
            "no_model/file",
            "[\"  1. only · infer · (no model named)\"]",
        ),
        (
            "no_model/reversed",
            "[\"  1. only · infer · (no model named)\"]",
        ),
        (
            "no_model/partial_out_of_range",
            "[\"  1. only · infer · (no model named)\"]",
        ),
        (
            "no_model/duplicate",
            "[\"  1. only · infer · (no model named)\", \"  2. only · infer · (no model named)\", \"  3. only · infer · (no model named)\"]",
        ),
        (
            "each/file",
            "[\"  1. fetch_each · fetches from the web · for each item\", \"  2. sum · infer · (no model named) · for each item\"]",
        ),
        (
            "each/reversed",
            "[\"  1. fetch_each · fetches from the web · for each item\", \"  2. sum · infer · (no model named) · for each item\"]",
        ),
        (
            "each/partial_out_of_range",
            "[\"  1. sum · infer · (no model named) · for each item\", \"  2. fetch_each · fetches from the web · for each item\"]",
        ),
        (
            "each/duplicate",
            "[\"  1. fetch_each · fetches from the web · for each item\", \"  2. fetch_each · fetches from the web · for each item\", \"  3. fetch_each · fetches from the web · for each item\", \"  4. sum · infer · (no model named) · for each item\"]",
        ),
        (
            "unicode/file",
            "[\"(the candidate does not parse; the check below says why)\"]",
        ),
        (
            "unicode/reversed",
            "[\"(the candidate does not parse; the check below says why)\"]",
        ),
        (
            "unicode/partial_out_of_range",
            "[\"(the candidate does not parse; the check below says why)\"]",
        ),
        (
            "unicode/duplicate",
            "[\"(the candidate does not parse; the check below says why)\"]",
        ),
        (
            "builtins/file",
            "[\"  1. t00 · reads a file\", \"  2. t01 · writes a file\", \"  3. t02 · edits a file\", \"  4. t03 · lists files\", \"  5. t04 · searches text\", \"  6. t05 · shapes the data\", \"  7. t06 · compares data\", \"  8. t07 · merges data\", \"  9. t08 · validates data\", \"  10. t09 · checks a condition\", \"  11. t10 · decides a branch\", \"  12. t11 · marks the work done\", \"  13. t12 · asks a human\", \"  14. t13 · fetches from the web\", \"  15. t14 · sends a notification\", \"  16. t15 · emits an event\", \"  17. t16 · logs a line\", \"  18. t17 · waits\", \"  19. t18 · reads the clock\", \"  20. t19 · makes an id\", \"  21. t20 · hashes data\", \"  22. t21 · converts a document\", \"  23. t22 · composes a document\", \"  24. t23 · inspects a workflow\", \"  25. t24 · draws a chart\", \"  26. t25 · generates an image\", \"  27. t26 · transforms an image\", \"  28. t27 · speaks text aloud\", \"  29. t28 · nika:zzz_unknown\"]",
        ),
        (
            "builtins/reversed",
            "[\"  1. t02 · edits a file\", \"  2. t00 · reads a file\", \"  3. t01 · writes a file\", \"  4. t03 · lists files\", \"  5. t04 · searches text\", \"  6. t05 · shapes the data\", \"  7. t06 · compares data\", \"  8. t07 · merges data\", \"  9. t08 · validates data\", \"  10. t09 · checks a condition\", \"  11. t10 · decides a branch\", \"  12. t11 · marks the work done\", \"  13. t12 · asks a human\", \"  14. t13 · fetches from the web\", \"  15. t14 · sends a notification\", \"  16. t15 · emits an event\", \"  17. t16 · logs a line\", \"  18. t17 · waits\", \"  19. t18 · reads the clock\", \"  20. t19 · makes an id\", \"  21. t20 · hashes data\", \"  22. t21 · converts a document\", \"  23. t22 · composes a document\", \"  24. t23 · inspects a workflow\", \"  25. t24 · draws a chart\", \"  26. t25 · generates an image\", \"  27. t26 · transforms an image\", \"  28. t27 · speaks text aloud\", \"  29. t28 · nika:zzz_unknown\"]",
        ),
        (
            "builtins/partial_out_of_range",
            "[\"  1. t01 · writes a file\", \"  2. t00 · reads a file\", \"  3. t02 · edits a file\", \"  4. t03 · lists files\", \"  5. t04 · searches text\", \"  6. t05 · shapes the data\", \"  7. t06 · compares data\", \"  8. t07 · merges data\", \"  9. t08 · validates data\", \"  10. t09 · checks a condition\", \"  11. t10 · decides a branch\", \"  12. t11 · marks the work done\", \"  13. t12 · asks a human\", \"  14. t13 · fetches from the web\", \"  15. t14 · sends a notification\", \"  16. t15 · emits an event\", \"  17. t16 · logs a line\", \"  18. t17 · waits\", \"  19. t18 · reads the clock\", \"  20. t19 · makes an id\", \"  21. t20 · hashes data\", \"  22. t21 · converts a document\", \"  23. t22 · composes a document\", \"  24. t23 · inspects a workflow\", \"  25. t24 · draws a chart\", \"  26. t25 · generates an image\", \"  27. t26 · transforms an image\", \"  28. t27 · speaks text aloud\", \"  29. t28 · nika:zzz_unknown\"]",
        ),
        (
            "builtins/duplicate",
            "[\"  1. t00 · reads a file\", \"  2. t00 · reads a file\", \"  3. t00 · reads a file\", \"  4. t01 · writes a file\", \"  5. t02 · edits a file\", \"  6. t03 · lists files\", \"  7. t04 · searches text\", \"  8. t05 · shapes the data\", \"  9. t06 · compares data\", \"  10. t07 · merges data\", \"  11. t08 · validates data\", \"  12. t09 · checks a condition\", \"  13. t10 · decides a branch\", \"  14. t11 · marks the work done\", \"  15. t12 · asks a human\", \"  16. t13 · fetches from the web\", \"  17. t14 · sends a notification\", \"  18. t15 · emits an event\", \"  19. t16 · logs a line\", \"  20. t17 · waits\", \"  21. t18 · reads the clock\", \"  22. t19 · makes an id\", \"  23. t20 · hashes data\", \"  24. t21 · converts a document\", \"  25. t22 · composes a document\", \"  26. t23 · inspects a workflow\", \"  27. t24 · draws a chart\", \"  28. t25 · generates an image\", \"  29. t26 · transforms an image\", \"  30. t27 · speaks text aloud\", \"  31. t28 · nika:zzz_unknown\"]",
        ),
        (
            "broken/file",
            "[\"(the candidate does not parse; the check below says why)\"]",
        ),
        (
            "broken/reversed",
            "[\"(the candidate does not parse; the check below says why)\"]",
        ),
        (
            "broken/partial_out_of_range",
            "[\"(the candidate does not parse; the check below says why)\"]",
        ),
        (
            "broken/duplicate",
            "[\"(the candidate does not parse; the check below says why)\"]",
        ),
        ("empty/file", "[]"),
        ("empty/reversed", "[]"),
        ("empty/partial_out_of_range", "[]"),
        ("empty/duplicate", "[]"),
    ];

    #[test]
    fn plan_rows_keep_the_pre_move_bytes() {
        let cases = cases(&|c| plan_lines(c), &|c, w| plan_lines_in_order(c, w));
        let want: Vec<(String, String)> = EXPECTED
            .iter()
            .map(|(n, t)| ((*n).to_owned(), (*t).to_owned()))
            .collect();
        assert_eq!(cases, want);
    }
}
