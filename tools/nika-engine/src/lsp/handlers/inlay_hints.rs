// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Inlay Hints Handler
//!
//! Shows inline annotations in the editor:
//! - Timeout clarification: `timeout: 30` → `30 seconds`
//! - Binding source: `data: $step1` → `← step1 output`
//! - Dependency count: `depends_on: [a, b]` → `2 deps`
//! - Verb badge: task line → verb icon

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

/// Count UTF-16 code units in a string (LSP spec requires UTF-16 offsets).
#[cfg(feature = "lsp")]
fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16()).sum::<usize>() as u32
}

/// Compute inlay hints for the given document range.
#[cfg(feature = "lsp")]
pub fn compute_inlay_hints(text: &str, range: Range) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    let start_line = range.start.line as usize;
    let end_line = (range.end.line as usize).min(lines.len());

    for (i, line) in lines.iter().enumerate().skip(start_line) {
        if i >= end_line {
            break;
        }
        let trimmed = line.trim();

        // Skip comment lines
        if trimmed.starts_with('#') {
            continue;
        }

        // 1. Timeout clarification: `timeout: 30` → shows ` seconds`
        if let Some(rest) = trimmed.strip_prefix("timeout:") {
            let val = rest.trim();
            if let Ok(secs) = val.parse::<u64>() {
                let label = if secs == 1 {
                    " second".to_string()
                } else if secs < 60 {
                    " seconds".to_string()
                } else {
                    let mins = secs / 60;
                    let rem = secs % 60;
                    if rem == 0 {
                        format!(" ({}min)", mins)
                    } else {
                        format!(" ({}m{}s)", mins, rem)
                    }
                };
                hints.push(InlayHint {
                    position: Position {
                        line: i as u32,
                        character: utf16_len(line),
                    },
                    label: InlayHintLabel::String(label),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: Some(InlayHintTooltip::String(
                        "Nika timeout is always in seconds".to_string(),
                    )),
                    padding_left: Some(false),
                    padding_right: Some(false),
                    data: None,
                });
            }
        }

        // 2. Binding source: `alias: $task_ref` → shows ` ← task_ref output`
        if trimmed.contains(": $") && !trimmed.starts_with('#') && !trimmed.starts_with('-') {
            if let Some(dollar_pos) = trimmed.find(": $") {
                let ref_start = dollar_pos + 3;
                let task_ref: String = trimmed[ref_start..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                if !task_ref.is_empty() {
                    hints.push(InlayHint {
                        position: Position {
                            line: i as u32,
                            character: utf16_len(line),
                        },
                        label: InlayHintLabel::String(format!(" <- {} output", task_ref)),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: Some(InlayHintTooltip::String(format!(
                            "Binds the output of task '{}' to this alias",
                            task_ref
                        ))),
                        padding_left: Some(true),
                        padding_right: Some(false),
                        data: None,
                    });
                }
            }
        }

        // 3. Dependency count: `depends_on: [a, b, c]` → shows ` (3 deps)`
        if let Some(rest) = trimmed.strip_prefix("depends_on:") {
            let deps_str = rest.trim();
            if deps_str.starts_with('[') {
                let count = deps_str
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .count();
                if count > 0 {
                    hints.push(InlayHint {
                        position: Position {
                            line: i as u32,
                            character: utf16_len(line),
                        },
                        label: InlayHintLabel::String(format!(
                            " ({} dep{})",
                            count,
                            if count == 1 { "" } else { "s" }
                        )),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: Some(InlayHintTooltip::String(
                            "Number of upstream dependencies this task waits for".to_string(),
                        )),
                        padding_left: Some(true),
                        padding_right: Some(false),
                        data: None,
                    });
                }
            }
        }

        // 4. Max turns hint: `max_turns: 10` → shows ` iterations`
        if let Some(rest) = trimmed.strip_prefix("max_turns:") {
            let val = rest.trim();
            if val.parse::<u64>().is_ok() {
                hints.push(InlayHint {
                    position: Position {
                        line: i as u32,
                        character: utf16_len(line),
                    },
                    label: InlayHintLabel::String(" iterations".to_string()),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: Some(InlayHintTooltip::String(
                        "Maximum agent loop iterations before stopping".to_string(),
                    )),
                    padding_left: Some(false),
                    padding_right: Some(false),
                    data: None,
                });
            }
        }

        // 5. Concurrency hint: `concurrency: 5` → shows ` parallel`
        if let Some(rest) = trimmed.strip_prefix("concurrency:") {
            let val = rest.trim();
            if val.parse::<u64>().is_ok() {
                hints.push(InlayHint {
                    position: Position {
                        line: i as u32,
                        character: utf16_len(line),
                    },
                    label: InlayHintLabel::String(" parallel".to_string()),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: Some(InlayHintTooltip::String(
                        "Maximum number of parallel for_each iterations".to_string(),
                    )),
                    padding_left: Some(false),
                    padding_right: Some(false),
                    data: None,
                });
            }
        }
    }

    hints
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "lsp")]
    use super::*;

    #[cfg(feature = "lsp")]
    fn full_range() -> Range {
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 999,
                character: 0,
            },
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn timeout_hint() {
        let hints = compute_inlay_hints("    timeout: 30\n", full_range());
        assert_eq!(hints.len(), 1);
        assert!(matches!(&hints[0].label, InlayHintLabel::String(s) if s.contains("seconds")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn timeout_minutes() {
        let hints = compute_inlay_hints("    timeout: 120\n", full_range());
        assert_eq!(hints.len(), 1);
        assert!(matches!(&hints[0].label, InlayHintLabel::String(s) if s.contains("2min")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn binding_source() {
        let hints = compute_inlay_hints("      data: $step1\n", full_range());
        assert_eq!(hints.len(), 1);
        assert!(matches!(&hints[0].label, InlayHintLabel::String(s) if s.contains("step1")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn depends_on_count() {
        let hints = compute_inlay_hints("    depends_on: [a, b, c]\n", full_range());
        assert_eq!(hints.len(), 1);
        assert!(matches!(&hints[0].label, InlayHintLabel::String(s) if s.contains("3 deps")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn max_turns_hint() {
        let hints = compute_inlay_hints("      max_turns: 10\n", full_range());
        assert_eq!(hints.len(), 1);
        assert!(matches!(&hints[0].label, InlayHintLabel::String(s) if s.contains("iterations")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn concurrency_hint() {
        let hints = compute_inlay_hints("    concurrency: 5\n", full_range());
        assert_eq!(hints.len(), 1);
        assert!(matches!(&hints[0].label, InlayHintLabel::String(s) if s.contains("parallel")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn no_hints_for_regular_lines() {
        let hints = compute_inlay_hints("    infer: \"hello\"\n", full_range());
        assert!(hints.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn multiple_hints_in_document() {
        let text = "\
tasks:
  - id: step1
    infer: \"Generate\"
    timeout: 30

  - id: step2
    with:
      data: $step1
    exec: \"echo\"
    depends_on: [step1]
";
        let hints = compute_inlay_hints(text, full_range());
        assert_eq!(hints.len(), 3); // timeout + binding + depends_on
    }
}
