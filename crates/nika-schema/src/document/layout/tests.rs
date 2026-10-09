// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::{Layout, Style};
use crate::document::Path;

const SRC: &str = "nika: probe # c\nmodel: \"mock/echo\"\ntasks:\n  a:\n    retry:\n      max_attempts: 3\n      backoff_ms: 100\n    infer:\n      prompt: |\n        hello\n        world\n\n      system: >-\n        folded\n        text\n    with: { x: 1, y: [a, b] }\n  b:\n    after:\n      a: success\n    list:\n    - one\n    - two: 2\n      three: 3\n    -   four\n    plain: multi\n      line\n    q: 'it''s'\n# trailing\noutputs:\n  r: ${{ tasks.b.output }}\n";

fn span(path: &[&str]) -> Option<(Style, String)> {
    let layout = Layout::build(SRC)?;
    let entry = layout.get(&Path::new(path.iter().copied()))?;
    let end = entry.end?;
    Some((entry.style, SRC.get(entry.start..end)?.to_owned()))
}

#[test]
fn every_scalar_style_spans_exactly_its_own_bytes() {
    let cases: &[(&[&str], Style, &str)] = &[
        (&["nika"], Style::Plain, "probe"),
        (&["model"], Style::DoubleQuoted, "\"mock/echo\""),
        (&["tasks", "a", "retry", "max_attempts"], Style::Plain, "3"),
        (
            &["tasks", "a", "infer", "prompt"],
            Style::Literal,
            "|\n        hello\n        world",
        ),
        (
            &["tasks", "a", "infer", "system"],
            Style::Folded,
            ">-\n        folded\n        text",
        ),
        (&["tasks", "a", "with", "x"], Style::Plain, "1"),
        (&["tasks", "a", "with", "y", "1"], Style::Plain, "b"),
        (&["tasks", "b", "list", "0"], Style::Plain, "one"),
        (&["tasks", "b", "list", "2"], Style::Plain, "four"),
        (&["tasks", "b", "plain"], Style::Plain, "multi\n      line"),
        (&["tasks", "b", "q"], Style::SingleQuoted, "'it''s'"),
        (&["outputs", "r"], Style::Plain, "${{ tasks.b.output }}"),
    ];
    for (path, style, text) in cases {
        assert_eq!(span(path), Some((*style, (*text).to_owned())), "{path:?}");
    }
}

/// yaml-rust2 0.10 counts a block scalar line's BYTES into its character
/// index; positions read from that index drift after non-ASCII content.
#[test]
fn positions_hold_after_a_non_ascii_block_scalar() {
    let src = "nika: x\nnote: |\n  ─── · — ▶ ───\nafter: \"kept\"\nlast: { k: v }\n";
    let layout = Layout::build(src).expect("layout");
    let at = |path: &[&str]| {
        let entry = layout.get(&Path::new(path.iter().copied())).expect("entry");
        src.get(entry.start..entry.end.expect("placed"))
            .map(str::to_owned)
    };
    assert_eq!(at(&["note"]).as_deref(), Some("|\n  ─── · — ▶ ───"));
    assert_eq!(at(&["after"]).as_deref(), Some("\"kept\""));
    assert_eq!(at(&["last"]).as_deref(), Some("{ k: v }"));
}

#[test]
fn collections_span_from_their_first_entry_to_their_last_byte() {
    assert_eq!(
        span(&["tasks", "a", "with"]),
        Some((Style::Flow, "{ x: 1, y: [a, b] }".to_owned()))
    );
    assert_eq!(
        span(&["tasks", "a", "retry"]),
        Some((
            Style::Block,
            "max_attempts: 3\n      backoff_ms: 100".to_owned()
        ))
    );
    assert_eq!(
        span(&["tasks", "b", "list"]),
        Some((
            Style::Block,
            "- one\n    - two: 2\n      three: 3\n    -   four".to_owned()
        ))
    );
    let whole = span(&[]).map(|(_, text)| text);
    assert_eq!(whole.as_deref(), SRC.strip_suffix('\n'));
}
