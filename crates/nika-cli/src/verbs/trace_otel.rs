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

pub fn export(trace: &str, out: Option<&str>, include_content: bool) -> VerbOutput {
    let raw = match std::fs::read_to_string(trace) {
        Ok(raw) => raw,
        Err(e) => return VerbOutput::env(format!("cannot read {trace}: {e}")),
    };
    let recovered = match super::run::recover_events(&raw, trace) {
        Ok(recovered) => recovered,
        Err(e) => return VerbOutput::env(e.to_string()),
    };
    // Verify before trusting (the git index-pack model — affordable at
    // 0.4µs/line): a BROKEN chain still exports, but says so; the head
    // rides the export so the anchor travels with the trace.
    let verdict = super::trace_verify::walk(&raw);
    let line = match nika_dap::otel::project(
        &recovered.events,
        include_content,
        Some(&verdict),
        env!("CARGO_PKG_VERSION"),
    ) {
        Ok(line) => line,
        Err(e) => return VerbOutput::env(e),
    };
    let target = out.map_or_else(|| default_out_path(trace), ToOwned::to_owned);
    if let Err(e) = std::fs::write(&target, format!("{line}\n")) {
        return VerbOutput::env(format!("cannot write {target}: {e}"));
    }
    let mut msg = format!(
        "exported → {target}\n  view: drag into Jaeger UI (≥1.60) · or POST the line to any OTLP/HTTP endpoint (`:4318/v1/traces`)"
    );
    if let super::trace_verify::Verdict::Broken {
        line: broken_at, ..
    } = &verdict
    {
        use std::fmt::Write as _;
        let _ = write!(
            msg,
            "\n  WARNING — this journal fails verification (chain broken at line {broken_at}); its claims are unverified"
        );
    }
    VerbOutput::ok(msg)
}

/// `run.ndjson` → `run.otlp.jsonl`, beside the journal.
fn default_out_path(trace: &str) -> String {
    trace.strip_suffix(".ndjson").map_or_else(
        || format!("{trace}.otlp.jsonl"),
        |stem| format!("{stem}.otlp.jsonl"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default target lands beside the journal, `.ndjson` swapped
    /// for `.otlp.jsonl` (a foreign extension keeps its full name).
    #[test]
    fn default_out_path_lands_beside_the_journal() {
        assert_eq!(default_out_path("run.ndjson"), "run.otlp.jsonl");
        assert_eq!(
            default_out_path(".nika/traces/2026-a3f2.ndjson"),
            ".nika/traces/2026-a3f2.otlp.jsonl"
        );
        assert_eq!(default_out_path("journal.log"), "journal.log.otlp.jsonl");
    }
}
