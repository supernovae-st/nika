//! `nika trace export` — project a run journal to OTLP/JSON lines.
//!
//! Every `OTel` tool becomes a nika viewer with zero vendor capture: the
//! artifact is a LOCAL `.otlp.jsonl` file (one `{"resourceSpans":[…]}`
//! per line — the de-facto file-exporter shape). Jaeger UI ≥ 1.60 takes
//! it by drag-and-drop; any OTLP/HTTP endpoint takes it line-by-line at
//! `:4318/v1/traces` (Jaeger all-in-one · Aspire · Grafana · Langfuse).
//!
//! The verb is a SHIM (the `trace_verify` pattern): the projection —
//! the deterministic identity, the hand-writer laws, the content
//! policy — lives in the forensics crate (`nika_dap::otel` · the
//! 2026-07-09 W0 trace descent); this file keeps the file plumbing and
//! the operator's message.

use super::VerbOutput;

#[must_use]
pub fn export(trace: &str, out: Option<&str>, include_content: bool) -> VerbOutput {
    // The plumbing (read · recover · verify-then-trust · project ·
    // write) lives in the forensics crate (nika_dap::otel —
    // the 15k descent); this shim keeps the operator's message.
    let outcome = match nika_dap::otel::export_journal(
        trace,
        out,
        include_content,
        env!("CARGO_PKG_VERSION"),
    ) {
        Ok(outcome) => outcome,
        Err(e) => return VerbOutput::env(e),
    };
    let mut msg = format!(
        "exported → {}\n  view: drag into Jaeger UI (≥1.60) · or POST the line to any OTLP/HTTP endpoint (`:4318/v1/traces`)",
        outcome.target
    );
    if let Some(broken_at) = outcome.broken_at {
        use std::fmt::Write as _;
        let _ = write!(
            msg,
            "\n  WARNING — this journal fails verification (chain broken at line {broken_at}); its claims are unverified"
        );
    }
    VerbOutput::ok(msg)
}

#[cfg(test)]
mod tests {
    /// The default target lands beside the journal, `.ndjson` swapped
    /// for `.otlp.jsonl` (a foreign extension keeps its full name).
    #[test]
    fn default_out_path_lands_beside_the_journal() {
        assert_eq!(
            nika_dap::otel::default_out_path("run.ndjson"),
            "run.otlp.jsonl"
        );
        assert_eq!(
            nika_dap::otel::default_out_path(".nika/traces/2026-a3f2.ndjson"),
            ".nika/traces/2026-a3f2.otlp.jsonl"
        );
        assert_eq!(
            nika_dap::otel::default_out_path("journal.log"),
            "journal.log.otlp.jsonl"
        );
    }
}
