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
