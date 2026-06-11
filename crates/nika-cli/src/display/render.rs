// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Frame rendering (spec §3.3) — a pure function `(RunView, Theme, tick) →
//! lines`. No I/O here: the replay loop owns the terminal, this module owns
//! the truth-to-text mapping. Snapshot tests pin BOTH glyph themes.

use crate::display::state::{RunView, TaskState};
use crate::display::theme::{Role, Theme};

/// Render one frame of the run card.
#[must_use]
pub fn frame(view: &RunView, theme: &Theme, tick: usize) -> Vec<String> {
    let mut lines = Vec::with_capacity(view.rows().len() + 6);

    // Header: identity + the statically-proven ceiling.
    let ceiling = view
        .ceiling_usd
        .map(|c| format!(" · ceiling ≤ ${c:.2}"))
        .unwrap_or_default();
    lines.push(format!(
        "  {} nika · {} · {} tasks{ceiling}",
        theme.logo(),
        theme.paint(Role::Strong, &view.workflow),
        view.rows().len(),
    ));

    // The audit-as-greeting line (the trust moment, every run).
    if let Some(permits) = &view.permits {
        let mark = if theme.ascii { "OK" } else { "✓" };
        lines.push(format!(
            "     permits {} {}",
            theme.paint(Role::Good, mark),
            theme.paint(Role::Dim, permits),
        ));
    }
    lines.push(String::new());

    // Task rows — stable order, aligned ids, notes dimmed.
    let width = view.rows().iter().map(|r| r.id.len()).max().unwrap_or(8);
    for row in view.rows() {
        let mut note = row.note.clone();
        if row.state == TaskState::Running && !view.token_samples.is_empty() {
            let spark = theme.sparkline(&view.token_samples);
            if !spark.is_empty() {
                note = format!("{note} {spark}");
            }
        }
        lines.push(format!(
            "  {} {:<width$}  {}",
            theme.glyph(row.state, tick),
            row.id,
            theme.paint(Role::Dim, &note),
        ));
    }

    // Footer meter: progress · live cost vs ceiling · wall clock.
    let cost = match view.ceiling_usd {
        Some(c) => format!("${:.3} of ≤${c:.2}", view.cost_usd),
        None => format!("${:.3}", view.cost_usd),
    };
    #[allow(clippy::cast_precision_loss)] // display-only seconds
    let secs = view.elapsed_ms as f64 / 1000.0;
    let meter = format!(
        "── {}/{} done · {cost} · elapsed {secs:.1}s ",
        view.done_count(),
        view.rows().len(),
    );
    lines.push(format!(
        "  {}",
        theme.paint(Role::Dim, &pad_rule(&meter, 64))
    ));

    // Failure card (only on a failed verdict · derives the explain hint).
    if view.verdict == Some(false) {
        for row in view.rows() {
            if row.state == TaskState::Failed && !row.detail.is_empty() {
                lines.push(String::new());
                lines.push(format!(
                    "  {}{}",
                    theme.glyph(TaskState::Failed, 0),
                    theme.paint(Role::Strong, &row.detail),
                ));
                if let Some(code) = row
                    .detail
                    .split_whitespace()
                    .find(|w| w.starts_with("NIKA-"))
                {
                    lines.push(theme.paint(Role::Dim, &format!("    fix: nika explain {code}")));
                }
            }
        }
    }
    lines
}

/// Extend a meter line with rule dashes to a stable width.
fn pad_rule(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_owned();
    }
    let mut out = String::with_capacity(width * 3);
    out.push_str(text);
    out.extend(std::iter::repeat_n('─', width - len));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    fn fold(events: &[nika_event::Event]) -> RunView {
        let mut view = RunView::new();
        for ev in events {
            view.apply(ev);
        }
        view
    }

    const UNICODE: Theme = Theme {
        color: false,
        ascii: false,
        animate: false,
    };
    const ASCII: Theme = Theme {
        color: false,
        ascii: true,
        animate: false,
    };

    /// Golden frame — the unicode theme, colour off (the exact spec story).
    #[test]
    fn golden_success_frame_unicode() {
        let lines = frame(&fold(&demo::success()), &UNICODE, 0);
        let expected = [
            "  🦋 nika · veille-news · 5 tasks · ceiling ≤ $0.04",
            "     permits ✓ network:read(hn.algolia.com) · fs:write(./out)",
            "",
            "  ✔  fetch_top     http 200 · 1.2s · 34 KB",
            "  ✔  extract_ai    jq · 0.1s · 12 items",
            "  ✔  summarize     claude-sonnet · 3.1s · $0.011",
            "  ✔  write_md      2.1 KB written",
            "  ⊘  notify_slack  when: env.CI != 'true'",
        ];
        assert_eq!(&lines[..8], &expected[..]);
        // The meter line: pinned prefix + rule-padded to a stable width.
        assert!(
            lines[8].starts_with("  ── 5/5 done · $0.011 of ≤$0.04 · elapsed 4.7s "),
            "meter: {}",
            lines[8]
        );
        assert_eq!(lines[8].chars().count(), 66, "2-indent + 64-rule");
    }

    /// Golden frame — the ASCII theme is first-class, not best-effort.
    #[test]
    fn golden_success_frame_ascii() {
        let lines = frame(&fold(&demo::success()), &ASCII, 0);
        assert_eq!(
            lines[0],
            "  [nika] nika · veille-news · 5 tasks · ceiling ≤ $0.04"
        );
        assert_eq!(lines[3], "  ok fetch_top     http 200 · 1.2s · 34 KB");
    }

    #[test]
    fn golden_failure_card_carries_the_explain_hint() {
        let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
        let tail = &lines[lines.len() - 2..];
        assert!(tail[0].contains("NIKA-431"), "headline: {tail:?}");
        assert_eq!(tail[1], "    fix: nika explain NIKA-431");
    }

    #[test]
    fn frame_is_stable_under_ticks_when_nothing_runs() {
        let view = fold(&demo::success());
        assert_eq!(frame(&view, &UNICODE, 0), frame(&view, &UNICODE, 9));
    }
}
