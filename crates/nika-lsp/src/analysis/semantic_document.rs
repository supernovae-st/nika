// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika/semanticDocument` — the analyzed workflow as ONE JSON payload.
//!
//! The graph half is `nika_graph::project` VERBATIM — the same
//! `graph_format: 3` document `nika inspect --format json` prints (spec
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
//! - the surface names its OWN version via `semantic_document_format`
//!   (spec 16 · distinct from the nested `graph.graph_format`, which
//!   versions the graph; a surface that grows additive fields needs a
//!   version of its own — in-payload, additive, spec-first evolution).
//!
//! Security: structure only. No env values, no secret material — the
//! projector never carried them.

use std::collections::BTreeMap;

use lsp_types::Range;
use nika_graph::GraphDoc;
use nika_schema::{FileId, ParseMode, parse};
use serde::Serialize;

use super::position::LineIndex;

/// The oracle surface's own version (spec 16 · `semantic_document_format`).
/// Distinct from the nested `graph.graph_format` (which versions the
/// graph): a surface that grows additive fields — holes · actions ·
/// capabilities — is versioned here, so a consumer reasons about the
/// surface without unwrapping the graph.
pub const SEMANTIC_DOCUMENT_FORMAT: u32 = 1;

/// The `nika/semanticDocument` result — a TYPED contract, not a JSON
/// bag: renaming a field breaks compilation (and the law tests), never
/// the wire silently.
#[derive(Debug, Serialize)]
pub struct SemanticDocument {
    /// The surface's own version (spec 16 · [`SEMANTIC_DOCUMENT_FORMAT`]) —
    /// serialized first, distinct from the nested `graph.graph_format`.
    pub semantic_document_format: u32,
    /// The canonical projection, verbatim — absent while the document
    /// cannot be projected (see [`Self::reason`]).
    pub graph: Option<GraphDoc>,
    /// Why `graph` is null — `None` when it is present. Two words, no
    /// finding duplication: the diagnostics lane carries the details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    /// Task id → the declaring `id` token's range (presentation: a
    /// canvas links nodes back to source without re-scanning YAML).
    pub spans: BTreeMap<String, Range>,
}

/// Compute the semantic document for a source text.
///
/// Always answers — the request is a READ, never a judgment; the
/// judgment lives in diagnostics. `reason` says only WHY the graph is
/// absent: `"parse"` (the document does not parse) or `"findings"`
/// (it parses, the check ladder refuses — no valid DAG order).
#[must_use]
pub fn semantic_document(text: &str) -> SemanticDocument {
    let index = LineIndex::new(text);
    let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient) else {
        return SemanticDocument {
            semantic_document_format: SEMANTIC_DOCUMENT_FORMAT,
            graph: None,
            reason: Some("parse"),
            spans: BTreeMap::new(),
        };
    };
    let report = nika_check::check(&wf);
    let spans: BTreeMap<String, Range> = wf
        .tasks
        .iter()
        .map(|t| {
            let start = t.value.id.span.start.0 as usize;
            let end = (t.value.id.span.end.0 as usize).max(start + t.value.id.value.len());
            (
                t.value.id.value.clone(),
                Range::new(index.position(start), index.position(end)),
            )
        })
        .collect();
    if report.is_clean() {
        SemanticDocument {
            semantic_document_format: SEMANTIC_DOCUMENT_FORMAT,
            graph: Some(nika_graph::project(&wf, &report)),
            reason: None,
            spans,
        }
    } else {
        SemanticDocument {
            semantic_document_format: SEMANTIC_DOCUMENT_FORMAT,
            graph: None,
            reason: Some("findings"),
            spans,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIAMOND: &str = "nika: w\npermits: { exec: [\"true\"] }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n  c:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n  d:\n    after: { b: success, c: success }\n    exec: { command: [\"true\"] }\n";

    fn as_value(doc: &SemanticDocument) -> serde_json::Value {
        serde_json::to_value(doc).expect("payload serializes")
    }

    /// The graph half is the CLI projection VERBATIM — byte-equal JSON
    /// to `nika_graph::project` (the parity that makes this a
    /// semantic ORACLE, not a second projector).
    #[test]
    fn graph_half_is_the_canonical_projection_verbatim() {
        let doc = as_value(&semantic_document(DIAMOND));
        let wf = parse(DIAMOND, FileId::new(0), ParseMode::Lenient).expect("parses");
        let report = nika_check::check(&wf);
        let expected = serde_json::to_value(nika_graph::project(&wf, &report)).expect("serializes");
        assert_eq!(doc["graph"], expected);
        assert_eq!(doc["graph"]["graph_format"], 3, "in-payload version (W2)");
        assert_eq!(
            doc["graph"]["nodes"].as_array().map(Vec::len),
            Some(4),
            "wave-ordered nodes"
        );
    }

    /// The surface names its OWN version (spec 16 · `semantic_document_format`),
    /// present in every case and DISTINCT from the nested `graph.graph_format`.
    #[test]
    fn the_surface_names_its_own_version() {
        // healthy: both versions present, and they are not the same key
        let ok = as_value(&semantic_document(DIAMOND));
        assert_eq!(
            ok["semantic_document_format"], 1,
            "the surface's own version"
        );
        assert_eq!(ok["graph"]["graph_format"], 3, "the nested graph's version");
        // absent-graph cases still name the surface version (findings · parse)
        let findings = "nika: w\ntasks:\n  a:\n    after: { b: success }\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n";
        assert_eq!(
            as_value(&semantic_document(findings))["semantic_document_format"],
            1
        );
        assert_eq!(
            as_value(&semantic_document("nika: v1\ntasks: ["))["semantic_document_format"],
            1
        );
    }

    /// Spans map every task id to its declaring token's range.
    #[test]
    fn spans_land_on_the_declaring_id_tokens() {
        let doc = as_value(&semantic_document(DIAMOND));
        let spans = doc["spans"].as_object().expect("spans object");
        assert_eq!(spans.len(), 4);
        let a = &spans["a"];
        // task `a`'s declaring KEY sits on line 3 (0-based · `nika:`
        // + the permits line + `tasks:` precede it) — the span points
        // there. It was line 5 until the envelope nuke removed the
        // two-line `workflow:` object.
        assert_eq!(a["start"]["line"], 3, "{a}");
    }

    /// A document with findings projects NO graph (the CLI skips PLAN
    /// for the same reason) — but the request still answers, with the
    /// spans it could read.
    #[test]
    fn findings_yield_a_null_graph_not_an_error() {
        let cyclic = "nika: w\ntasks:\n  a:\n    after: { b: success }\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n";
        let doc = as_value(&semantic_document(cyclic));
        assert_eq!(doc["graph"], serde_json::Value::Null);
        assert_eq!(doc["reason"], "findings", "why, in one word");
        assert_eq!(doc["spans"].as_object().map(serde_json::Map::len), Some(2));

        let unparseable = "nika: v1\ntasks: [";
        let doc2 = as_value(&semantic_document(unparseable));
        assert_eq!(doc2["graph"], serde_json::Value::Null);
        assert_eq!(doc2["reason"], "parse");

        // a healthy document carries NO reason key at all
        let ok = as_value(&semantic_document(DIAMOND));
        assert!(ok.get("reason").is_none(), "{ok}");
    }
}
