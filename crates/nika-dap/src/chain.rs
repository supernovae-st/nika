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
    /// Every line chained, and the last one CLOSES the run's lifecycle
    /// — `head` is the last line's sha256.
    #[non_exhaustive]
    Intact {
        /// Verified event-line count.
        events: usize,
        /// sha256 hex of the last verified line's exact bytes.
        head: String,
    },
    /// Every line chained, but the last complete line never CLOSES the
    /// run's lifecycle (no terminal frame, no seal): the run was killed
    /// or crashed between writes. The chain attests every complete line
    /// exactly as `Intact` does — the LIFECYCLE end is absent, said out
    /// loud (a killed run is a finding, never a silence; the attestation
    /// rides the verifier, never the dying run).
    #[non_exhaustive]
    Incomplete {
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
    /// A line exceeds [`MAX_LINE_BYTES`] — beyond the verifier's bounds
    /// (F-P1 · NEP-0012: a 100-megabyte line is a denial-of-service,
    /// never a journal line; the refusal fires BEFORE the parse).
    #[non_exhaustive]
    LineOverLong {
        /// FILE line number (1-based).
        line: usize,
        /// Observed byte length.
        got: usize,
    },
}

/// The kinds that CLOSE a run's lifecycle: the four terminal frames
/// (spec 13 · `EventKind::is_terminal`) plus the seal that rides after
/// one. A journal whose last complete line carries any of these reached
/// an attested end; anything else means the run died mid-flight (kill ·
/// crash · power) — the walk names that [`Verdict::Incomplete`], never
/// `Intact` (F-P2 · LOT-1).
const LIFECYCLE_TERMINAL: &[&str] = &[
    "workflow_completed",
    "workflow_failed",
    "workflow_cancelled",
    "workflow_paused",
    "run_sealed",
];

/// Maximum bytes per journal LINE the walk parses (F-P1 · NEP-0012) —
/// the same 1 MiB grain as [`crate::bounded::MAX_ARTIFACT_BYTES`]: a
/// real event line is under 10 KB (the seal's covers included); a line
/// beyond this is a denial-of-service vector, refused before
/// `serde_json` sees it.
pub const MAX_LINE_BYTES: usize = crate::bounded::MAX_ARTIFACT_BYTES;

/// Does ANY of these lines carry a `chain` field? The pre-0.96 era is
/// defined by their total absence, so one chained line refutes it — and
/// refuting it is what separates an old journal from a stripped header.
fn any_line_is_chained(lines: &[(usize, &str)]) -> bool {
    lines.iter().any(|&(_, l)| {
        serde_json::from_str::<serde_json::Value>(l)
            .ok()
            .and_then(|v| v.get("chain").and_then(|c| c.as_str()).map(str::to_owned))
            .is_some()
    })
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
    let mut last_closes = false;
    for (pos, &(lineno, line)) in numbered.iter().enumerate() {
        let is_last = pos + 1 == numbered.len();
        // The fortress line bound (F-P1) — refused BEFORE the parse, so
        // the DoS class never reaches serde_json (bounds are code).
        if line.len() > MAX_LINE_BYTES {
            return Verdict::LineOverLong {
                line: lineno + 1,
                got: line.len(),
            };
        }
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
            // The first line proposes the era; the REST of the file
            // decides it. A pre-0.96 journal carries zero chained
            // lines, so a chainless first line above chained ones is a
            // STRIPPED HEADER, not an era — and that question is
            // decidable from the bytes in hand. Answering it from line
            // 0 alone renders a tampered trace as "nothing to verify,
            // nothing to distrust", which is the one thing a proof
            // layer may never say about a file that was edited.
            if pos == 0 && !any_line_is_chained(&numbered[1..]) {
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
        last_closes = value
            .get("kind")
            .and_then(|k| k.as_str())
            .is_some_and(|k| LIFECYCLE_TERMINAL.contains(&k));
    }
    if last_closes {
        Verdict::Intact {
            events: verified,
            head: expected,
        }
    } else {
        Verdict::Incomplete {
            events: verified,
            head: expected,
        }
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

    /// A STRIPPED HEADER is not a pre-chain era, and the difference is
    /// decidable from the file itself: a real pre-0.96 journal carries
    /// ZERO chained lines, while this one carries every line but the
    /// first. Deciding the era from line 0 alone resolves the ambiguity
    /// toward the reassuring reading and renders a TAMPERED trace as
    /// "nothing to verify, nothing to distrust".
    #[test]
    fn a_stripped_header_is_broken_not_a_pre_chain_era() {
        let raw = chained(&[
            ev("workflow_started"),
            ev("task_completed"),
            ev("workflow_completed"),
        ]);
        let mut lines: Vec<String> = raw.lines().map(str::to_owned).collect();
        // Rename the genesis line's `chain` key — the exact shape an
        // attacker reaches for to buy a benign verdict.
        lines[0] = lines[0].replacen("\"chain\"", "\"chbin\"", 1);
        let tampered = format!("{}\n", lines.join("\n"));
        assert!(
            matches!(walk(&tampered), Verdict::Broken { line: 1, .. }),
            "a header stripped from a chained file is a BREAK, not an era"
        );
    }

    /// The other direction, so the fix cannot be bought by calling
    /// everything broken: a genuine pre-chain journal has no chained
    /// line anywhere and must stay `Unchained`.
    #[test]
    fn a_genuine_pre_chain_journal_with_many_lines_stays_unchained() {
        let raw = format!(
            "{}\n{}\n{}\n",
            ev("workflow_started"),
            ev("task_completed"),
            ev("workflow_completed")
        );
        assert!(
            matches!(walk(&raw), Verdict::Unchained),
            "no line carries a chain · this really is the pre-0.96 era"
        );
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
        let raw = chained(&[ev("workflow_started"), ev("workflow_completed")]);
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

    /// F-P2 · a chain-intact journal whose last complete line never
    /// closes the lifecycle is `Incomplete` (the run died mid-flight),
    /// never `Intact`; the four terminal frames and the seal all close
    /// it. The chain facts (events · head) carry identically on both.
    #[test]
    fn a_journal_without_a_terminal_frame_is_incomplete_not_intact() {
        let killed = chained(&[ev("workflow_started"), ev("task_started")]);
        let Verdict::Incomplete { events, head } = walk(&killed) else {
            panic!("a killed run's journal is Incomplete, never Intact");
        };
        assert_eq!(events, 2);
        let last = killed.lines().last().expect("last line");
        assert_eq!(
            head,
            sha256_hex(last.as_bytes()),
            "the head still binds every line"
        );

        for terminal in [
            "workflow_completed",
            "workflow_failed",
            "workflow_cancelled",
            "workflow_paused",
            "run_sealed",
        ] {
            let raw = chained(&[ev("workflow_started"), ev(terminal)]);
            assert!(
                matches!(walk(&raw), Verdict::Intact { events: 2, .. }),
                "{terminal} closes the lifecycle"
            );
        }
        // A mid-flight kind anywhere but last never flips the class.
        let finished = chained(&[ev("task_started"), ev("workflow_completed")]);
        assert!(matches!(walk(&finished), Verdict::Intact { .. }));
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

    /// (F-P1 · the fortress line bound) An oversized line refuses BEFORE
    /// any parse — the walk names the line and the observed length, and
    /// a normal-sized journal is untouched by the bound.
    #[test]
    fn an_oversized_line_refuses_before_the_parse() {
        let mut long = ev("workflow_started");
        long["pad"] = serde_json::Value::String("x".repeat(MAX_LINE_BYTES));
        let raw = chained(&[long]);
        let Verdict::LineOverLong { line, got } = walk(&raw) else {
            panic!("the DoS line is refused, never parsed");
        };
        assert_eq!(line, 1);
        assert!(got > MAX_LINE_BYTES, "the observed length is named: {got}");
        // The bound never bites a real journal (the flagship pair).
        let normal = chained(&[ev("workflow_started"), ev("workflow_completed")]);
        assert!(matches!(walk(&normal), Verdict::Intact { .. }));
    }
}
