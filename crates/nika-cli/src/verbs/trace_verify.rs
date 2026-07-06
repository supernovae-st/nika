//! `nika trace verify` — is this journal internally consistent?
//!
//! Every journal line (0.96+) carries a `chain` field: the sha256 of
//! the PREVIOUS line's exact bytes (genesis: a constant tag). Verify
//! walks the file and recomputes — any edited, inserted, dropped or
//! reordered line breaks every hash after it.
//!
//! The claim is TAMPER-EVIDENT, never "tamper-proof": with no external
//! trust root, an attacker can rewrite the whole chain. The parade is
//! the anchor the run printed (`trace: … · chain <head>`) — CI logs and
//! scrollback hold a head a whole-file rewrite cannot reproduce.
//!
//! Exit codes (the house taxonomy): 0 = intact · 2 (FILE) = broken ·
//! 3 (ENV) = unchained (a pre-chain journal — stated, never guessed).

use super::VerbOutput;

/// The chain's genesis tag — must match the sink's.
const CHAIN_GENESIS: &[u8] = b"nika-trace-v1";

#[must_use]
pub fn verify(trace: &str) -> VerbOutput {
    let raw = match std::fs::read_to_string(trace) {
        Ok(raw) => raw,
        Err(e) => return VerbOutput::env(format!("cannot read {trace}: {e}")),
    };
    match walk(&raw) {
        Verdict::Intact { events, head } => VerbOutput::ok(format!(
            "OK — {events} events · chain intact · head {head}\n  internally consistent (tamper-evident, not tamper-proof) — compare the head\n  against the one the run printed to close the loop"
        )),
        Verdict::Broken {
            line,
            recorded,
            computed,
        } => VerbOutput::file(format!(
            "BROKEN at line {line} — recorded chain {} · computed {}\n  every line from here on is unverified (edited, inserted, dropped or reordered)",
            short(&recorded),
            short(&computed),
        )),
        Verdict::Unchained => VerbOutput::env(format!(
            "unchained — {trace} predates the chain (pre-0.96 journal): nothing to verify, nothing to distrust"
        )),
        Verdict::Empty => VerbOutput::env(format!("{trace}: no events")),
    }
}

enum Verdict {
    Intact {
        events: usize,
        head: String,
    },
    Broken {
        line: usize,
        recorded: String,
        computed: String,
    },
    Unchained,
    Empty,
}

/// The pure walk — recompute the chain over exact line bytes.
fn walk(raw: &str) -> Verdict {
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Verdict::Empty;
    }
    let mut expected = sha256_hex(CHAIN_GENESIS);
    for (i, line) in lines.iter().enumerate() {
        let Some(recorded) = chain_of(line) else {
            // The FIRST line decides the era: no chain there = a
            // pre-chain journal. A chain that starts and then STOPS is
            // a break, not an era.
            if i == 0 {
                return Verdict::Unchained;
            }
            return Verdict::Broken {
                line: i + 1,
                recorded: "(absent)".to_owned(),
                computed: expected,
            };
        };
        if recorded != expected {
            return Verdict::Broken {
                line: i + 1,
                recorded,
                computed: expected,
            };
        }
        expected = sha256_hex(line.as_bytes());
    }
    Verdict::Intact {
        events: lines.len(),
        head: expected,
    }
}

/// Extract the `chain` field without deserializing the full event —
/// verify must work on journals whose event shapes it predates.
fn chain_of(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.get("chain")?.as_str().map(ToOwned::to_owned)
}

fn short(hex: &str) -> &str {
    hex.get(..16).unwrap_or(hex)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
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
    fn a_chain_that_stops_is_broken_not_unchained() {
        let mut raw = chained(&[ev("workflow_started")]);
        raw.push_str(&ev("workflow_completed").to_string());
        raw.push('\n');
        assert!(matches!(walk(&raw), Verdict::Broken { line: 2, .. }));
    }
}
