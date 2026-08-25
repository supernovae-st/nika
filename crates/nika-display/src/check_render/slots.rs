// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The SLOTS rung — a scaffold saying which values are still its own.
//!
//! This rung blocks the run, and it must not read like a fault. The
//! person typed `nika new` thirty seconds ago and did nothing wrong;
//! the file is simply not finished. So it wears the `Warn` face rather
//! than the `Bad` one, opens on « ready to be filled », names every slot
//! with its LINE (« some slots are empty » would send someone hunting),
//! and closes on one pasteable command.
//!
//! Silent when there is nothing to fill — a rung with no information is
//! a lecture (the `SKILLS` precedent).

use std::fmt::Write as _;

use nika_check::CheckReport;

use crate::theme::{Role, Theme};

/// The 1-based line a byte offset falls on.
fn line_of(source: &str, offset: u32) -> usize {
    let cut = source.len().min(offset as usize);
    source
        .get(..cut)
        .map_or(1, |head| head.matches('\n').count() + 1)
}

/// Render the rung. `path` is played back verbatim in the closing
/// command so the line is a paste, not a template to fill in by hand.
pub(crate) fn slots_rung(
    out: &mut String,
    report: &CheckReport,
    source: &str,
    path: &str,
    t: Theme,
) {
    let slots = &report.slot_findings;
    if slots.is_empty() {
        return;
    }
    let glyph = t.paint(Role::Warn, if t.ascii { ".." } else { "…" });
    let n = slots.len();
    let plural = if n == 1 { "value is" } else { "values are" };
    let _ = writeln!(
        out,
        " {} {} {}",
        glyph,
        t.paint(Role::Strong, &format!("{:<8}", "SLOTS")),
        format_args!("your file is ready to be filled — {n} {plural} still the scaffold's")
    );
    for s in slots {
        let _ = writeln!(
            out,
            "   {}  {} {}",
            t.paint(
                Role::Dim,
                &format!("line {:>4}", line_of(source, s.span.start))
            ),
            s.path,
            t.paint(Role::Dim, &format!("· {}", s.hint))
        );
    }
    let _ = writeln!(
        out,
        "   {}",
        t.paint(Role::Dim, &format!("fill them, then: nika check {path}"))
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);

    fn report(yaml: &str) -> CheckReport {
        nika_check::check(
            &nika_schema::parse(
                yaml,
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict,
            )
            .expect("fixture parses"),
        )
    }

    const SCAFFOLD: &str = concat!(
        "nika: draft\n",
        "model: mock/echo\n",
        "permits: {}\n",
        "tasks:\n",
        "  think:\n",
        "    infer:\n",
        "      prompt: |\n",
        "        <SLOT: the one model job>\n",
        "      max_tokens: 10\n",
    );

    /// Constraint 3 of the ruling: the message names WHICH slot, on
    /// WHICH line, and ends on one command. « some slots are empty »
    /// would make a person hunt through their own file.
    #[test]
    fn the_rung_names_each_slot_its_line_and_one_command() {
        let mut out = String::new();
        slots_rung(
            &mut out,
            &report(SCAFFOLD),
            SCAFFOLD,
            "first.nika.yaml",
            PLAIN,
        );
        assert!(out.contains("SLOTS"), "{out}");
        assert!(
            out.contains("line    8"),
            "the marker sits on line 8:\n{out}"
        );
        assert!(out.contains("tasks.think.infer.prompt"), "{out}");
        assert!(
            out.contains("the one model job"),
            "the marker teaches:\n{out}"
        );
        assert!(
            out.contains("fill them, then: nika check first.nika.yaml"),
            "one pasteable command, naming their file:\n{out}"
        );
    }

    /// Constraint 4: it must not read as a fault. The person just typed
    /// `nika new`. The rung blocks the run — it does not scold.
    #[test]
    fn the_rung_reads_as_a_step_not_a_failure() {
        let mut out = String::new();
        slots_rung(
            &mut out,
            &report(SCAFFOLD),
            SCAFFOLD,
            "first.nika.yaml",
            PLAIN,
        );
        assert!(out.contains("ready to be filled"), "{out}");
        for scold in ["✖", "error", "invalid", "broken", "failed"] {
            assert!(
                !out.to_lowercase().contains(scold),
                "`{scold}` turns a step into a fault:\n{out}"
            );
        }
    }

    /// A rung with nothing to say says nothing — the SKILLS precedent.
    #[test]
    fn a_filled_file_gets_no_rung_at_all() {
        let filled = SCAFFOLD.replace("<SLOT: the one model job>", "Summarise the release notes.");
        let mut out = String::new();
        slots_rung(
            &mut out,
            &report(&filled),
            &filled,
            "first.nika.yaml",
            PLAIN,
        );
        assert!(out.is_empty(), "{out}");
    }

    /// The line arithmetic, at both ends of the file.
    #[test]
    fn line_of_counts_from_one() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\nb\nc", 4), 3);
        assert_eq!(line_of("a\nb\nc", 9_999), 3, "past the end clamps");
    }
}
