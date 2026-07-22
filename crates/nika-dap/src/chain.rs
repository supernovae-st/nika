// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tamper-evidence chain walk — is a journal internally consistent?
//!
//! Every journal line (0.96+) carries a `chain` field: the sha256 of
//! the PREVIOUS line's exact bytes (genesis: a constant tag). The walk
//! recomputes — any edited, inserted, dropped or reordered line breaks
//! every hash after it. Descended from the `trace verify` verb
//! (2026-07-09 · the nika-dap split): the SINK that writes the chain
//! and every walker that checks it now share ONE genesis tag and ONE
//! hash primitive (three private copies unified).

use nika_event::source_id::sha256_hex;

/// The chain's genesis tag — the first line's `chain` field is the
/// sha256 of exactly these bytes (the sink writes it · the walk checks
/// it · one constant, two duties).
pub const CHAIN_GENESIS: &[u8] = b"nika-trace-v1";

/// The walk's verdict over one raw journal text.
#[non_exhaustive]
pub enum Verdict {
    /// Every line chained — `head` is the last line's sha256.
    #[non_exhaustive]
    Intact {
        /// Verified event-line count.
        events: usize,
        /// sha256 hex of the last verified line's exact bytes.
        head: String,
    },
    /// The chain holds through the last COMPLETE line, but the final
    /// line is not valid JSON — a crash mid-write (torn tail), NOT
    /// tampering. The research-locked distinction: conflating the two
    /// would make every crashed run look forged.
    #[non_exhaustive]
    TornTail {
        /// Verified event-line count (the torn line excluded).
        events: usize,
        /// sha256 hex of the last verified line's exact bytes.
        head: String,
    },
    /// A line's recorded `chain` does not match the recomputation.
    #[non_exhaustive]
    Broken {
        /// FILE line number (1-based · blanks counted, never renumbered).
        line: usize,
        /// The `chain` value the line carries.
        recorded: String,
        /// The sha256 the walk computed from the previous line.
        computed: String,
    },
    /// A pre-chain journal (pre-0.96) — nothing to verify, nothing to
    /// distrust.
    Unchained,
    /// No events at all.
    Empty,
    /// The first non-blank line is not even JSON — not a journal at
    /// all (H1: garbage must never verify OK).
    #[non_exhaustive]
    Unreadable {
        /// FILE line number (1-based).
        line: usize,
    },
}

/// The pure walk — recompute the chain over exact line bytes. Line
/// numbers are FILE lines (blanks skipped, never renumbered — the
/// recover path counts the same way), each line parses exactly once,
/// and a torn tail requires a VERIFIED prefix: a one-line garbage file
/// is `Unreadable`, never OK (the false-green class).
#[must_use]
pub fn walk(raw: &str) -> Verdict {
    let numbered: Vec<(usize, &str)> = raw
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .collect();
    if numbered.is_empty() {
        return Verdict::Empty;
    }
    let mut expected = sha256_hex(CHAIN_GENESIS);
    let mut verified = 0usize;
    for (pos, &(lineno, line)) in numbered.iter().enumerate() {
        let is_last = pos + 1 == numbered.len();
        let parsed: Option<serde_json::Value> = serde_json::from_str(line).ok();
        let Some(value) = parsed else {
            // Invalid JSON: a torn tail (crash mid-write) ONLY when a
            // verified chain precedes it — a first-line failure means
            // this is not a journal at all.
            if is_last && verified > 0 {
                return Verdict::TornTail {
                    events: verified,
                    head: expected,
                };
            }
            return Verdict::Unreadable { line: lineno + 1 };
        };
        let Some(recorded) = value.get("chain").and_then(|c| c.as_str()) else {
            // The FIRST line decides the era: no chain there = a
            // pre-chain journal. A chain that starts and then STOPS is
            // a break, not an era.
            if pos == 0 {
                return Verdict::Unchained;
            }
            return Verdict::Broken {
                line: lineno + 1,
                recorded: "(absent)".to_owned(),
                computed: expected,
            };
        };
        if recorded != expected {
            return Verdict::Broken {
                line: lineno + 1,
                recorded: recorded.to_owned(),
                computed: expected,
            };
        }
        expected = sha256_hex(line.as_bytes());
        verified += 1;
    }
    Verdict::Intact {
        events: verified,
        head: expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a chained journal the way the sink does.
    fn chained(events: &[serde_json::Value]) -> String {
        let mut chain = sha256_hex(CHAIN_GENESIS);
        let mut out = String::new();
        for e in events {
            let mut v = e.clone();
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = sha256_hex(line.as_bytes());
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    fn ev(kind: &str) -> serde_json::Value {
        serde_json::json!({"id": {"uuid": "01912345-0000-7000-8000-000000000001"},
            "timestamp": 1000, "kind": kind, "run": null, "correlation": null, "fields": []})
    }

    #[test]
    fn an_intact_chain_verifies_with_its_head() {
        let raw = chained(&[
            ev("workflow_started"),
            ev("task_completed"),
            ev("workflow_completed"),
        ]);
        match walk(&raw) {
            Verdict::Intact { events, head } => {
                assert_eq!(events, 3);
                let last = raw.lines().last().expect("last line");
                assert_eq!(
                    head,
                    sha256_hex(last.as_bytes()),
                    "head = hash of the last line"
                );
            }
            other => {
                assert!(matches!(other, Verdict::Intact { .. }), "expected intact");
            }
        }
    }

    #[test]
    fn one_edited_byte_breaks_at_that_line() {
        let raw = chained(&[
            ev("workflow_started"),
            ev("task_completed"),
            ev("workflow_completed"),
        ]);
        // Tamper with line 2's content (flip the kind) — line 3's chain
        // no longer matches the edited bytes.
        let tampered = raw.replace("task_completed", "task_complexed");
        match walk(&tampered) {
            Verdict::Broken { line, .. } => assert_eq!(line, 3, "the line AFTER the edit breaks"),
            other => {
                assert!(matches!(other, Verdict::Broken { .. }), "expected broken");
            }
        }
    }

    #[test]
    fn a_dropped_line_breaks_the_chain() {
        let raw = chained(&[
            ev("workflow_started"),
            ev("task_completed"),
            ev("workflow_completed"),
        ]);
        let mut dropped = String::new();
        for (i, l) in raw.lines().enumerate() {
            if i != 1 {
                dropped.push_str(l);
                dropped.push('\n');
            }
        }
        assert!(matches!(walk(&dropped), Verdict::Broken { line: 2, .. }));
    }

    #[test]
    fn a_pre_chain_journal_is_unchained_not_broken() {
        let raw = format!("{}\n", ev("workflow_started"));
        assert!(matches!(walk(&raw), Verdict::Unchained));
    }

    #[test]
    fn single_line_garbage_never_verifies_ok() {
        // The false-green class: `nika trace verify /etc/motd` must not
        // exit 0 — a torn tail requires a VERIFIED prefix.
        assert!(matches!(
            walk("this is not json\n"),
            Verdict::Unreadable { line: 1 }
        ));
    }

    #[test]
    fn broken_line_numbers_are_file_lines_even_with_blanks() {
        let raw = chained(&[ev("workflow_started"), ev("task_completed")]);
        // Insert a blank line between the two — the second event now
        // sits on FILE line 3 and its chain still verifies; tamper it
        // and the report must say line 3, not post-filter line 2.
        let mut lines: Vec<&str> = raw.lines().collect();
        lines.insert(1, "");
        let spaced: String = lines.join("\n") + "\n";
        assert!(matches!(walk(&spaced), Verdict::Intact { events: 2, .. }));
        // Tamper the FIRST event: the NEXT non-blank line detects it —
        // and must name FILE line 3 (the blank counts), not
        // post-filter line 2. (Tampering the LAST line is the printed
        // head anchor's job, by design — intra-file chaining cannot
        // see it.)
        let tampered = spaced.replace("workflow_started", "workflow_startex");
        assert!(matches!(walk(&tampered), Verdict::Broken { line: 3, .. }));
    }

    #[test]
    fn a_torn_final_line_is_a_crash_not_a_tamper() {
        let mut raw = chained(&[ev("workflow_started"), ev("task_completed")]);
        raw.push_str("{\"id\":{\"uuid\":\"01912345-0000-7000-8000-0000000");
        let verdict = walk(&raw);
        assert!(
            matches!(verdict, Verdict::TornTail { events: 2, .. }),
            "a torn final line is a crash, not a tamper"
        );
    }

    #[test]
    fn a_chain_that_stops_is_broken_not_unchained() {
        let mut raw = chained(&[ev("workflow_started")]);
        raw.push_str(&ev("workflow_completed").to_string());
        raw.push('\n');
        assert!(matches!(walk(&raw), Verdict::Broken { line: 2, .. }));
    }
}
