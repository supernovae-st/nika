// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE graph projector (spec 03 §graph-projection) — `--json` is the canonical
//! projection; mermaid/dot/ASCII derive from it, never from the workflow
//! directly. Versioned envelope (`graph_format: 3` · typed edges) · nodes topologically
//! sorted (stable order = stable layouts) · `edges.kind` closed enum ·
//! the static graph NEVER carries run state.

use std::fmt::Write as _;

use nika_check::CheckReport;

pub use nika_graph::{GraphDoc, Node, project};

use crate::verbs::{VerbOutput, load_checked};

/// The 4 verb hues — the shared visual vocabulary (spec `design/tokens.yaml`,
/// vendored in the pack as `design-tokens.yaml`). Hand-written consts, NOT a
/// runtime parse (L-grain: zero yaml in the render path) — the parity test
/// below pins them against `nika_pack::design_tokens()`, so a spec-side hue
/// change fails `cargo test` here until this table follows. Terminal output
/// never uses these (nika-display is ANSI-16 semantic by doctrine); they
/// exist for the HEX surfaces: mermaid classDefs, byte-parity with the
/// spec's showcase projector (`fill = color + "22"` alpha).
const VERB_COLORS: [(&str, &str); 4] = [
    ("infer", "#5b8cff"),
    ("exec", "#ff7a3c"),
    ("invoke", "#22d3ee"),
    ("agent", "#b07bff"),
];

/// Mermaid renderer — derives from the projection, labels carry the verb,
/// verb identity paints the node (the same classDef map every projected
/// docs diagram uses — one visual language on every surface).
#[must_use]
pub fn to_mermaid(doc: &GraphDoc) -> String {
    let mut out = String::from("graph TD\n");
    for node in &doc.nodes {
        let detail = node
            .tool
            .as_deref()
            .or(node.model.as_deref())
            .map(|d| format!(" · {}", mermaid_escape(d)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {id}[\"{id} · {verb}{detail}\"]:::{verb}",
            id = node.id,
            verb = node.verb
        );
    }
    for edge in &doc.edges {
        let _ = writeln!(out, "  {} --> {}", edge.from, edge.to);
    }
    // Only the classDefs of verbs actually drawn (the spec projector's rule).
    for (verb, color) in VERB_COLORS {
        if doc.nodes.iter().any(|n| n.verb == verb) {
            let _ = writeln!(
                out,
                "  classDef {verb} fill:{color}22,stroke:{color},color:{color}"
            );
        }
    }
    out
}

/// Graphviz dot renderer — derives from the projection.
#[must_use]
pub fn to_dot(doc: &GraphDoc) -> String {
    let mut out = format!(
        "digraph \"{}\" {{\n  rankdir=TB;\n  node [shape=box];\n",
        dot_escape(&doc.workflow)
    );
    for node in &doc.nodes {
        let detail = node
            .tool
            .as_deref()
            .or(node.model.as_deref())
            .map(|d| format!("\\n{}", dot_escape(d)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  \"{id}\" [label=\"{id}\\n{verb}{detail}\"];",
            id = node.id,
            verb = node.verb
        );
    }
    for edge in &doc.edges {
        let _ = writeln!(out, "  \"{}\" -> \"{}\";", edge.from, edge.to);
    }
    out.push_str("}\n");
    out
}

/// Escape a label fragment for a Graphviz dot quoted string. A `"` or `\`
/// in the tool/model would otherwise terminate the `label="…"` and emit
/// broken dot (e.g. a templated `model: ${{ inputs.m }}` or an MCP tool name).
fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Escape a label fragment for a Mermaid `["…"]` node label. A literal `"`
/// would close the label early (then a stray `]` closes the node); brackets
/// are entity-escaped defensively. Mermaid reads `#NN;`/`#name;` HTML entities.
fn mermaid_escape(s: &str) -> String {
    s.replace('"', "#quot;")
        .replace('[', "#91;")
        .replace(']', "#93;")
}

/// Output format for the `graph` verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    /// The canonical JSON projection (machine surface · never coloured).
    Json,
    /// Mermaid flowchart text.
    Mermaid,
    /// Graphviz dot text.
    Dot,
    /// The terminal drawing (waves as columns · real wires) — falls back
    /// to the wave listing when the one-rail layout cannot be truthful.
    Ascii,
}

/// The clap-facing arm of [`GraphFormat`] (descended from the bin's
/// dispatcher 2026-07-21 · the 1500-line file cap — the bin composes,
/// the verbs own their arg types).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum GraphFormatArg {
    /// Canonical JSON projection (`graph_format: 3`).
    Json,
    /// Mermaid flowchart.
    Mermaid,
    /// Graphviz dot.
    Dot,
    /// Terminal drawing (waves as columns · real wires · honest fallback).
    Ascii,
}

impl From<GraphFormatArg> for GraphFormat {
    fn from(arg: GraphFormatArg) -> Self {
        match arg {
            GraphFormatArg::Json => Self::Json,
            GraphFormatArg::Mermaid => Self::Mermaid,
            GraphFormatArg::Dot => Self::Dot,
            GraphFormatArg::Ascii => Self::Ascii,
        }
    }
}

/// The `nika inspect <file> --format …` projector arm. The theme feeds the `ascii` renderer
/// only — the file formats (json · mermaid · dot) never carry escapes.
#[must_use]
pub fn run(path: &str, format: GraphFormat, theme: crate::display::theme::Theme) -> VerbOutput {
    let (wf, report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    if !report.conformance.is_empty() {
        let mut text = String::from("cannot project: no valid DAG order while conformance fails\n");
        for c in &report.conformance {
            let _ = writeln!(text, "  [{}] {}", c.code, c.message);
        }
        return VerbOutput::file(text);
    }
    let doc = project(&wf, &report);
    let text = match format {
        GraphFormat::Json => match serde_json::to_string_pretty(&doc) {
            Ok(json) => json,
            Err(e) => return VerbOutput::env(format!("cannot serialize graph: {e}")),
        },
        GraphFormat::Mermaid => to_mermaid(&doc),
        GraphFormat::Dot => to_dot(&doc),
        GraphFormat::Ascii => to_ascii(&doc, &report, theme),
    };
    VerbOutput::ok(text)
}

/// The terminal drawing — real wires when the layout can be truthful,
/// the wave listing otherwise (never a wrong picture). The theme rides
/// in from the binary's ONE resolution chain, so a pipe still gets
/// escape-free bytes (colour auto-resolves off) while a TTY finally
/// sees the art it was owed.
/// The drawing over an already-projected doc (the verb's own
/// `--format ascii` arm) — the drawing lives in `nika_display::dag_art`
/// (the 15k descent, 2026-07-29); the sibling surfaces ride it there.
pub(crate) fn to_ascii(
    doc: &GraphDoc,
    report: &CheckReport,
    theme: crate::display::theme::Theme,
) -> String {
    nika_display::dag_art::to_ascii(doc, report, theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renders(yaml: &str) -> (String, String) {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        let doc = project(&wf, &report);
        (to_mermaid(&doc), to_dot(&doc))
    }

    /// The hand-written [`VERB_COLORS`] table must MATCH the vendored
    /// design-tokens (spec `design/tokens.yaml` → pack `design-tokens.yaml`)
    /// — the shared visual vocabulary is spec-first: a hue change lands
    /// there, re-vendors, and THIS test stays red until the table follows.
    #[test]
    fn verb_colors_match_the_pack_design_tokens() {
        let tokens = nika_pack::design_tokens();
        assert!(
            !tokens.is_empty(),
            "pack carries design-tokens.yaml (sync-pack.sh vendored it)"
        );
        for (verb, color) in VERB_COLORS {
            let section = tokens
                .split(&format!("  {verb}:\n"))
                .nth(1)
                .expect("tokens carry every verb section");
            let pinned = section
                .lines()
                .find_map(|l| l.trim().strip_prefix("color: \""))
                .and_then(|rest| rest.split('"').next())
                .expect("verb section carries a color");
            assert_eq!(
                color, pinned,
                "VERB_COLORS.{verb} drifted from the pack design-tokens"
            );
        }
    }

    /// Mermaid nodes carry their verb class and the classDef map is the
    /// SAME derivation the spec's showcase projector emits (fill = color +
    /// `22` alpha) — one visual language on every rendered surface. Only
    /// verbs actually drawn get a classDef.
    #[test]
    fn mermaid_paints_verb_identity() {
        let (mermaid, _) = renders(
            "nika: paint\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"p\", max_tokens: 5 }\n",
        );
        assert!(mermaid.contains(":::infer"), "node classed:\n{mermaid}");
        assert!(
            mermaid.contains("classDef infer fill:#5b8cff22,stroke:#5b8cff,color:#5b8cff"),
            "spec-parity classDef:\n{mermaid}"
        );
        assert!(
            !mermaid.contains("classDef exec"),
            "undrawn verbs stay classless:\n{mermaid}"
        );
    }

    /// A model with chars special to mermaid (`"` `]`) or dot (`"`) must be
    /// ESCAPED — an unescaped quote closed the label early and emitted broken
    /// markup (a templated `model: ${{ inputs.m }}` or an MCP tool name hits this
    /// too). Regression: graph used to interpolate the model raw.
    #[test]
    fn special_chars_in_model_are_escaped_in_both_renders() {
        // model value parses to ·  mock/echo"]x
        let (mermaid, dot) = renders(
            "nika: adv\nmodel: \"mock/echo\\\"]x\"\ntasks:\n  a:\n    infer: { prompt: \"p\", max_tokens: 5 }\n",
        );
        assert!(
            mermaid.contains("#quot;"),
            "mermaid quote unescaped:\n{mermaid}"
        );
        assert!(
            !mermaid.contains("echo\"]"),
            "raw breaker leaked:\n{mermaid}"
        );
        assert!(dot.contains("echo\\\"]"), "dot quote unescaped:\n{dot}");
    }

    /// Declared policy PROJECTS (one voice — clients stop re-parsing the
    /// YAML): retry budget, timeout in ms, the `on_error` action, and the
    /// declared output names. Undeclared policy stays ABSENT on the wire
    /// (`skip_serializing` — no fake defaults).
    #[test]
    fn declared_policy_projects_and_undeclared_stays_absent() {
        let yaml = "nika: policy\nmodel: mock/echo\ntasks:\n  guarded:\n    infer: { prompt: \"p\", max_tokens: 5 }\n    timeout: \"30s\"\n    retry:\n      max_attempts: 3\n    on_error:\n      skip: true\n    extract:\n      summary: \".text\"\n      title: \".title\"\n  bare:\n    after:\n      guarded: success\n    infer: { prompt: \"q\", max_tokens: 5 }\n";
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        let doc = project(&wf, &report);

        let guarded = doc
            .nodes
            .iter()
            .find(|n| n.id == "guarded")
            .expect("guarded projected");
        assert_eq!(guarded.retry_max_attempts, Some(3));
        assert_eq!(guarded.timeout_ms, Some(30_000));
        assert_eq!(guarded.on_error, Some("skip"));
        assert_eq!(guarded.outputs, vec!["summary", "title"]);

        let bare = doc
            .nodes
            .iter()
            .find(|n| n.id == "bare")
            .expect("bare projected");
        assert_eq!(bare.retry_max_attempts, None);
        assert_eq!(bare.timeout_ms, None);
        assert_eq!(bare.on_error, None);
        assert!(bare.outputs.is_empty());

        // The WIRE stays additive: undeclared policy keys are absent.
        let json = serde_json::to_string(&doc).expect("serializes");
        assert!(json.contains("\"retry_max_attempts\":3"));
        assert!(json.contains("\"on_error\":\"skip\""));
        let bare_json = serde_json::to_string(doc.nodes.iter().find(|n| n.id == "bare").unwrap())
            .expect("serializes");
        assert!(!bare_json.contains("retry_max_attempts"));
        assert!(!bare_json.contains("timeout_ms"));
        assert!(!bare_json.contains("on_error"));
        assert!(!bare_json.contains("outputs"));
    }
}
