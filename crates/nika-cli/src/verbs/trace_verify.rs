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

/// The variadic form (`nika trace verify .nika/traces/*.ndjson` — the
/// glob muscle memory types): each file verifies under its own header,
/// no stop-at-first, and the worst exit survives — the `check`
/// multi-file law, one voice across verbs. Store handles resolve
/// exactly as the single form does.
#[must_use]
pub fn verify_many(traces: &[std::path::PathBuf]) -> VerbOutput {
    let mut blocks = Vec::with_capacity(traces.len());
    let mut worst = super::exit::OK;
    for path in traces {
        let resolved = super::trace::manage::resolve_store_handle(path);
        let out = verify(&resolved.to_string_lossy());
        let mut block = format!("{}\n", resolved.display());
        for line in out.text.lines() {
            block.push_str("  ");
            block.push_str(line);
            block.push('\n');
        }
        blocks.push(block);
        worst = worst.max(out.code);
    }
    VerbOutput {
        text: blocks.join("\n"),
        code: worst,
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

#[cfg(test)]
mod tests {
    use super::*;
    use nika_dap::chain::CHAIN_GENESIS;
    use nika_dap::source_id::sha256_hex;

    /// Build a chained journal the way the sink does (the nika-dap
    /// chain-test idiom).
    fn chained(kinds: &[&str]) -> String {
        let mut chain = sha256_hex(CHAIN_GENESIS);
        let mut out = String::new();
        for kind in kinds {
            let mut v = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
                "timestamp": 1000, "kind": kind, "run": null,
                "correlation": null, "fields": []
            });
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = sha256_hex(line.as_bytes());
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    fn stage(name: &str, raw: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("nika-verify-many-{name}"));
        std::fs::write(&path, raw).expect("stage");
        path
    }

    /// The glob form: every file gets its own header + verdict, the
    /// verify never stops at the first failure, and the worst exit
    /// survives (2 FILE from the tampered journal beats the intact 0).
    #[test]
    fn verify_many_reports_per_file_and_exits_worst_of() {
        let intact = stage(
            "intact.ndjson",
            &chained(&["workflow_started", "task_completed"]),
        );
        // Tamper a MIDDLE line — the next line's recorded chain breaks.
        // (An edited FINAL line is exactly what only the externally
        // anchored head can catch — the module doc's whole point.)
        let mut tampered_raw =
            chained(&["workflow_started", "task_completed", "workflow_completed"]);
        tampered_raw = tampered_raw.replacen("task_completed", "task_failedxx", 1);
        let tampered = stage("tampered.ndjson", &tampered_raw);

        let out = verify_many(&[intact.clone(), tampered.clone()]);
        assert_eq!(out.code, super::super::exit::FILE, "worst-of: {}", out.text);
        assert!(
            out.text.contains("nika-verify-many-intact.ndjson"),
            "first header: {}",
            out.text
        );
        assert!(
            out.text.contains("nika-verify-many-tampered.ndjson"),
            "second header (no stop-at-first): {}",
            out.text
        );
        assert!(out.text.contains("OK — 2 events"), "{}", out.text);
        assert!(out.text.contains("BROKEN at line"), "{}", out.text);
        let _ = std::fs::remove_file(intact);
        let _ = std::fs::remove_file(tampered);
    }
}
