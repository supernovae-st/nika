// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! E2E tests for inlay_hints handler.
use nika_lsp_core::handlers::inlay_hints::inlay_hints;

#[test]
fn timeout_seconds() {
    let yaml = "    timeout: 30\n";
    let hints = inlay_hints(yaml, 0, yaml.len() as u32);
    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("seconds"));
}

#[test]
fn timeout_minutes() {
    let hints = inlay_hints("    timeout: 120\n", 0, 100);
    assert!(hints[0].label.contains("2min"));
}

#[test]
fn binding_source() {
    let hints = inlay_hints("      data: $step1\n", 0, 100);
    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("step1"));
}

#[test]
fn depends_on_count() {
    let hints = inlay_hints("    depends_on: [a, b, c]\n", 0, 100);
    assert!(hints[0].label.contains("3 dep"));
}

#[test]
fn max_turns_hint() {
    let hints = inlay_hints("      max_turns: 10\n", 0, 100);
    assert!(hints[0].label.contains("iterations"));
}

#[test]
fn concurrency_hint() {
    let hints = inlay_hints("    concurrency: 5\n", 0, 100);
    assert!(hints[0].label.contains("parallel"));
}

#[test]
fn model_cost_hint() {
    let yaml = "    model: claude-sonnet-4-20250514\n";
    let hints = inlay_hints(yaml, 0, yaml.len() as u32);
    assert_eq!(hints.len(), 1);
    assert!(
        hints[0].label.contains("$"),
        "Should show cost: {:?}",
        hints[0].label
    );
}

#[test]
fn no_hints_for_regular_line() {
    let hints = inlay_hints("    infer: \"hello\"\n", 0, 100);
    assert!(hints.is_empty());
}

#[test]
fn multiple_hints() {
    let text = "\
tasks:
  - id: step1
    infer: \"Generate\"
    timeout: 30
    model: gpt-4o

  - id: step2
    with:
      data: $step1
    exec: \"echo\"
    depends_on: [step1]
";
    let hints = inlay_hints(text, 0, text.len() as u32);
    assert!(
        hints.len() >= 4,
        "Expected at least 4 hints, got {}",
        hints.len()
    );
}
