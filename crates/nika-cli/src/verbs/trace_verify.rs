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

#[must_use]
pub fn verify(trace: &str) -> VerbOutput {
    let raw = match std::fs::read_to_string(trace) {
        Ok(raw) => raw,
        Err(e) => return VerbOutput::env(format!("cannot read {trace}: {e}")),
    };
    match walk(&raw) {
        Verdict::Intact { events, head, .. } => VerbOutput::ok(format!(
            "OK — {events} events · chain intact · head {head}\n  internally consistent (tamper-evident, not tamper-proof) — compare the head\n  against the one the run printed to close the loop"
        )),
        Verdict::Broken {
            line,
            recorded,
            computed,
            ..
        } => VerbOutput::file(format!(
            "BROKEN at line {line} — recorded chain {} · computed {}\n  every line from here on is unverified (edited, inserted, dropped or reordered)",
            short(&recorded),
            short(&computed),
        )),
        Verdict::TornTail { events, head, .. } => VerbOutput::ok(format!(
            "OK — {events} events · chain intact · head {head}\n  the final line is TORN (a crash mid-write, not tampering) — the chain\n  covers every complete line"
        )),
        Verdict::Unchained => VerbOutput::env(format!(
            "unchained — {trace} predates the chain (pre-0.96 journal): nothing to verify, nothing to distrust"
        )),
        Verdict::Empty => VerbOutput::env(format!("{trace}: no events")),
        Verdict::Unreadable { line, .. } => VerbOutput::env(format!(
            "{trace}:{line}: not a journal — the line is not valid JSON"
        )),
        // The verdict is #[non_exhaustive]: a NEWER forensics crate may
        // learn classes this CLI cannot render — refuse honestly,
        // never mis-render one.
        _ => VerbOutput::env(format!(
            "{trace}: unknown verdict class — the forensics library is newer than this CLI"
        )),
    }
}

// The walk + its verdict live in the forensics crate (one genesis tag ·
// one hash primitive · the sink imports the SAME constant) — re-exported
// here so every `super::trace_verify::walk` consumer reads unchanged.
pub(crate) use nika_dap::chain::{Verdict, walk};

/// First 16 chars when they LOOK like hex — an adversarial `chain`
/// string renders as its sanitized head, never verbatim.
fn short(hex: &str) -> String {
    sanitize(hex).chars().take(16).collect()
}

/// Strip control characters (terminal-escape injection: a journal
/// field must never drive the operator's terminal).
pub(crate) fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}
