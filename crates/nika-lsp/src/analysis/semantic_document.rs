// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika/semanticDocument` — the analyzed workflow as ONE JSON payload.
//!
//! The graph half is `nika_graph::project` VERBATIM — the same
//! `graph_format: 1` document `nika inspect --format json` prints (spec
//! 03 §graph-projection), so a canvas webview, an agent over MCP and an
//! editor extension all read one truth. The LSP adds a PRESENTATION
//! wrapper only: per-task declaration ranges, so a client can link
//! graph nodes back to source without re-scanning the YAML.
//!
//! Contract ·
//! - `graph` — the canonical projection, or `null` when the document
//!   has findings (no valid DAG order → nothing to project; the
//!   diagnostics lane already tells that story).
//! - `spans` — task id → LSP `Range` of the declaring `id` token.
//! - the payload names its OWN version via `graph.graph_format`
//!   (in-payload versioning — additive, spec-first evolution).
//!
//! Security: structure only. No env values, no secret material — the
//! projector never carried them.

use lsp_types::Range;
use nika_schema::{FileId, ParseMode, parse};
use serde_json::{Value, json};

use super::position::LineIndex;

/// Compute the semantic document for a source text.
///
/// Always answers (an unparseable document yields `{"graph": null,
/// "spans": {}}`) — the request is a READ, never a judgment; the
/// judgment lives in diagnostics.
#[must_use]
pub fn semantic_document(text: &str) -> Value {
    let index = LineIndex::new(text);
    let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient) else {
        return json!({ "graph": null, "spans": {} });
    };
    let report = nika_schema::check(&wf);
    let spans: serde_json::Map<String, Value> = wf
        .tasks
        .iter()
        .map(|t| {
            let start = t.value.id.span.start.0 as usize;
            let end = (t.value.id.span.end.0 as usize).max(start + t.value.id.value.len());
            let range = Range::new(index.position(start), index.position(end));
            (
                t.value.id.value.clone(),
                serde_json::to_value(range).unwrap_or(Value::Null),
            )
        })
        .collect();
    let graph = if report.is_clean() {
        serde_json::to_value(nika_graph::project(&wf, &report)).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    json!({ "graph": graph, "spans": Value::Object(spans) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIAMOND: &str = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"x\" }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"x\" }\n  - id: c\n    depends_on: [a]\n    exec: { command: \"x\" }\n  - id: d\n    depends_on: [b, c]\n    exec: { command: \"x\" }\n";

    /// The graph half is the CLI projection VERBATIM — byte-equal JSON
    /// to `nika_graph::project` (the parity that makes this a
    /// semantic ORACLE, not a second projector).
    #[test]
    fn graph_half_is_the_canonical_projection_verbatim() {
        let doc = semantic_document(DIAMOND);
        let wf = parse(DIAMOND, FileId::new(0), ParseMode::Lenient).expect("parses");
        let report = nika_schema::check(&wf);
        let expected = serde_json::to_value(nika_graph::project(&wf, &report)).expect("serializes");
        assert_eq!(doc["graph"], expected);
        assert_eq!(doc["graph"]["graph_format"], 1, "in-payload version");
        assert_eq!(
            doc["graph"]["nodes"].as_array().map(Vec::len),
            Some(4),
            "wave-ordered nodes"
        );
    }

    /// Spans map every task id to its declaring token's range.
    #[test]
    fn spans_land_on_the_declaring_id_tokens() {
        let doc = semantic_document(DIAMOND);
        let spans = doc["spans"].as_object().expect("spans object");
        assert_eq!(spans.len(), 4);
        let a = &spans["a"];
        // `- id: a` sits on line 3 (0-based) — the span points there.
        assert_eq!(a["start"]["line"], 3, "{a}");
    }

    /// A document with findings projects NO graph (the CLI skips PLAN
    /// for the same reason) — but the request still answers, with the
    /// spans it could read.
    #[test]
    fn findings_yield_a_null_graph_not_an_error() {
        let cyclic = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    depends_on: [b]\n    exec: { command: \"x\" }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"x\" }\n";
        let doc = semantic_document(cyclic);
        assert_eq!(doc["graph"], serde_json::Value::Null);
        assert_eq!(doc["spans"].as_object().map(serde_json::Map::len), Some(2));

        let unparseable = "nika: v1\ntasks: [";
        let doc2 = semantic_document(unparseable);
        assert_eq!(doc2["graph"], serde_json::Value::Null);
    }
}
