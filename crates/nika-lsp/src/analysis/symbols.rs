// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Document symbols — the file outline.
//!
//! A parsed [`RawWorkflow`] becomes a one-root tree: the workflow node
//! (named by its `workflow:` id) with one child per DECLARATION — every
//! `vars:` entry (variable), `env:` entry (constant), `secrets:` entry
//! (key), then one per task (method, detailed with its verb `infer ·
//! exec · invoke · agent`). Each child's selection range is the name's
//! own span (so « reveal symbol » jumps to the declaring token) — the
//! outline is the navigation twin of the member go-to-definition lane.
//!
//! Single-file, pure: `(text) -> Vec<DocumentSymbol>`. A document that does
//! not parse yields an empty outline (the diagnostics surface owns the
//! error; the outline simply has nothing to show).

use lsp_types::{DocumentSymbol, Range, SymbolKind};
use nika_schema::raw::RawWorkflow;
use nika_schema::types::type_expr_display;
use nika_schema::{FileId, ParseMode, Span, parse};

use super::position::LineIndex;

/// Build the document-symbol outline for a workflow source.
///
/// Returns a single-element vector (the workflow root) when the source
/// parses, or an empty vector when it does not.
#[must_use]
pub fn document_symbols(text: &str) -> Vec<DocumentSymbol> {
    let index = LineIndex::new(text);
    let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient) else {
        return Vec::new();
    };
    vec![workflow_symbol(&index, &wf)]
}

/// The workflow root symbol with one child per task.
fn workflow_symbol(index: &LineIndex, wf: &RawWorkflow) -> DocumentSymbol {
    let name = wf
        .workflow
        .as_ref()
        .map_or_else(|| "workflow".to_owned(), |w| w.value.clone());
    let mut children: Vec<DocumentSymbol> = Vec::new();
    for (name, decl) in &wf.inputs {
        let detail = match decl {
            nika_schema::VarDecl::Typed { r#type, .. } => {
                format!("input · {}", type_expr_display(&r#type.value))
            }
            nika_schema::VarDecl::Untyped(_) => "input".to_owned(),
        };
        children.push(decl_symbol(
            index,
            &name.value,
            name.span,
            detail,
            SymbolKind::VARIABLE,
        ));
    }
    for (name, decl) in &wf.consts {
        let detail = match decl {
            nika_schema::VarDecl::Typed { r#type, .. } => {
                format!("const · {}", type_expr_display(&r#type.value))
            }
            nika_schema::VarDecl::Untyped(_) => "const".to_owned(),
        };
        children.push(decl_symbol(
            index,
            &name.value,
            name.span,
            detail,
            SymbolKind::CONSTANT,
        ));
    }
    for (name, _) in &wf.secrets {
        children.push(decl_symbol(
            index,
            &name.value,
            name.span,
            "secret".to_owned(),
            SymbolKind::KEY,
        ));
    }
    children.extend(
        wf.tasks
            .iter()
            .map(|task| task_symbol(index, &task.value, task.span)),
    );
    // The workflow's range spans every task's id span, or the whole
    // document when there are no tasks.
    let range = children
        .iter()
        .map(|c| c.range)
        .reduce(union_range)
        .unwrap_or_else(|| whole_document(index));
    symbol(
        name,
        Some("workflow".to_owned()),
        SymbolKind::MODULE,
        range,
        range,
        Some(children),
    )
}

/// One declaration symbol (a `vars:` / `env:` / `secrets:` entry) —
/// selection range = the declaring name's own span, so the outline
/// jumps exactly where member go-to-definition lands.
fn decl_symbol(
    index: &LineIndex,
    name: &str,
    span: Span,
    detail: String,
    kind: SymbolKind,
) -> DocumentSymbol {
    let range = token_range_at(index, span, name.len());
    symbol(name.to_owned(), Some(detail), kind, range, range, None)
}

/// A span widened to the token's byte length (the parser emits point
/// spans for mapping keys — same discipline as definition.rs).
fn token_range_at(index: &LineIndex, span: Span, len: usize) -> Range {
    let start = span.start.0 as usize;
    let end = (span.end.0 as usize).max(start + len);
    Range::new(index.position(start), index.position(end))
}

/// One task symbol — named by id, detailed with its verb.
fn task_symbol(
    index: &LineIndex,
    task: &nika_schema::raw::RawTask,
    task_span: Span,
) -> DocumentSymbol {
    let id_range = span_range(index, task.id.span);
    let full_range = span_range(index, task_span);
    // The enclosing range must contain the selection range; the task span
    // always contains the id span, but guard the analyzer/parser invariant.
    let enclosing = union_range(full_range, id_range);
    symbol(
        task.id.value.clone(),
        Some(task.action.verb().to_owned()),
        SymbolKind::METHOD,
        enclosing,
        id_range,
        None,
    )
}

/// Construct a [`DocumentSymbol`] (the `deprecated` field is itself
/// deprecated upstream — we set it to `None` and silence the lint).
#[allow(deprecated)]
fn symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    range: Range,
    selection_range: Range,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children,
    }
}

/// Convert a [`Span`] to an LSP range.
fn span_range(index: &LineIndex, span: Span) -> Range {
    Range::new(
        index.position(span.start.0 as usize),
        index.position(span.end.0 as usize),
    )
}

/// The range covering the whole document.
fn whole_document(index: &LineIndex) -> Range {
    Range::new(index.position(0), index.position(index.text().len()))
}

/// The smallest range covering both inputs.
fn union_range(a: Range, b: Range) -> Range {
    Range::new(min_pos(a.start, b.start), max_pos(a.end, b.end))
}

fn min_pos(a: lsp_types::Position, b: lsp_types::Position) -> lsp_types::Position {
    if (a.line, a.character) <= (b.line, b.character) {
        a
    } else {
        b
    }
}

fn max_pos(a: lsp_types::Position, b: lsp_types::Position) -> lsp_types::Position {
    if (a.line, a.character) >= (b.line, b.character) {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_on_unparseable() {
        let syms = document_symbols("nika: v1\nworkflow: [bad\n");
        assert!(syms.is_empty(), "unparseable → no outline");
    }

    #[test]
    fn workflow_root_named_by_id() {
        let yaml = "nika: my-flow\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n";
        let syms = document_symbols(yaml);
        assert_eq!(syms.len(), 1, "one root");
        assert_eq!(syms[0].name, "my-flow");
        assert_eq!(syms[0].kind, SymbolKind::MODULE);
        assert_eq!(syms[0].detail.as_deref(), Some("workflow"));
    }

    #[test]
    fn each_task_is_a_child_detailed_with_its_verb() {
        let yaml = "nika: w\ntasks:\n  fetch_it:\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n  think:\n    after: { fetch_it: success }\n    infer: { prompt: \"hi\", max_tokens: 10 }\n";
        let syms = document_symbols(yaml);
        let children = syms[0].children.as_ref().expect("children present");
        assert_eq!(children.len(), 2, "two tasks");
        assert_eq!(children[0].name, "fetch_it");
        assert_eq!(children[0].detail.as_deref(), Some("invoke"));
        assert_eq!(children[0].kind, SymbolKind::METHOD);
        assert_eq!(children[1].name, "think");
        assert_eq!(children[1].detail.as_deref(), Some("infer"));
    }

    #[test]
    fn task_selection_range_is_the_id_span() {
        let yaml = "nika: w\ntasks:\n  hello:\n    exec: { command: [\"x\"] }\n";
        let index = LineIndex::new(yaml);
        let syms = document_symbols(yaml);
        let task = &syms[0].children.as_ref().expect("children")[0];
        // selection range must resolve to the `hello` token
        let id_byte = yaml.find("hello").expect("token");
        assert_eq!(task.selection_range.start, index.position(id_byte));
    }

    fn rng(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range::new(
            lsp_types::Position::new(sl, sc),
            lsp_types::Position::new(el, ec),
        )
    }

    #[test]
    fn root_range_unions_every_task_id_span() {
        // Two tasks · the workflow root range is the SMALLEST range covering
        // both task enclosing ranges (W1: spans anchor on the map KEYS).
        // child[0] encloses (2,2)..(4,2), child[1] encloses (4,2)..(6,0) ·
        // the union is (2,2)..(6,0). Every line here moved up by 2 when
        // the envelope nuke collapsed `workflow:`/`id:` onto `nika:`.
        // This pins union_range + min_pos (start) + max_pos (end) to exact
        // positions — Default::default() (0,0)..(0,0), a flipped min_pos
        // (returns (5,6)) or a flipped max_pos (returns (5,2)) all diverge.
        let yaml = "nika: w\ntasks:\n  aa:\n    exec: { command: [\"x\"] }\n  bbb:\n    exec: { command: [\"y\"] }\n";
        let syms = document_symbols(yaml);
        let children = syms[0].children.as_ref().expect("children");
        assert_eq!(children[0].range, rng(2, 2, 4, 2), "child 0 enclosing");
        assert_eq!(children[1].range, rng(4, 2, 6, 0), "child 1 enclosing");
        assert_eq!(
            syms[0].range,
            rng(2, 2, 6, 0),
            "root unions both children: min start (2,2), max end (6,0)"
        );
    }

    #[test]
    fn task_enclosing_range_unions_id_into_the_task_span() {
        // A single task · its enclosing range unions the task span with the
        // id span (W1: both anchor on the map KEY). The task span is
        // (2,2)..(4,0) — key through end of body — and the id span is the
        // key token (2,2)..(2,3) · the union keeps the wider span.
        let yaml = "nika: my-flow\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n";
        let syms = document_symbols(yaml);
        let child = &syms[0].children.as_ref().expect("children")[0];
        assert_eq!(
            child.range,
            rng(2, 2, 4, 0),
            "id span folded into task span"
        );
        // the selection range is the key token (2,2)..(2,3)
        assert_eq!(child.selection_range, rng(2, 2, 2, 3), "id selection");
    }

    #[test]
    fn taskless_workflow_root_spans_the_whole_document() {
        // No tasks · the root range falls back to `whole_document`, which is
        // (0,0)..(end-of-doc). The doc is 2 lines (trailing \n → empty line
        // 2) so the end is (2,0). Default::default() would wrongly give
        // (0,0)..(0,0).
        let yaml = "nika: empty\ntasks: {}\n";
        let syms = document_symbols(yaml);
        assert_eq!(syms.len(), 1, "one root even with no tasks");
        let root = &syms[0];
        assert!(
            root.children.as_ref().is_none_or(Vec::is_empty),
            "no task children"
        );
        assert_eq!(
            root.range,
            rng(0, 0, 2, 0),
            "whole-document fallback span, not the (0,0) default"
        );
    }

    #[test]
    fn all_four_verbs_render() {
        let yaml = "nika: w\nmodel: ollama/m\ntasks:\n  i:\n    infer: { prompt: \"p\", max_tokens: 5 }\n  e:\n    exec: { command: [\"x\"] }\n  v:\n    invoke: { tool: \"nika:read\", args: { path: \"./p\" } }\n  g:\n    agent: { prompt: \"p\", tools: [\"nika:read\"], max_turns: 2 }\n";
        let syms = document_symbols(yaml);
        let verbs: Vec<&str> = syms[0]
            .children
            .as_ref()
            .expect("children")
            .iter()
            .filter_map(|c| c.detail.as_deref())
            .collect();
        assert_eq!(verbs, ["infer", "exec", "invoke", "agent"]);
    }

    /// The outline carries every DECLARATION — vars (typed detail) ·
    /// inputs · const · secrets · tasks, in declaration order, each selection
    /// range on the declaring name (the go-to-definition twin).
    #[test]
    fn outline_carries_vars_env_secrets_and_tasks() {
        let text = "nika: w\ninputs:\n  city:\n    type: string\n  REGION: { type: string, required: false, default: \"eu\" }\nconst:\n  out: \"./o\"\nsecrets:\n  api_key: { source: vault, key: k }\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n";
        let syms = document_symbols(text);
        assert_eq!(syms.len(), 1, "one workflow root");
        let children = syms[0].children.as_ref().expect("children");
        let names: Vec<(&str, SymbolKind)> =
            children.iter().map(|c| (c.name.as_str(), c.kind)).collect();
        assert_eq!(
            names,
            vec![
                ("city", SymbolKind::VARIABLE),
                ("REGION", SymbolKind::VARIABLE),
                ("out", SymbolKind::CONSTANT),
                ("api_key", SymbolKind::KEY),
                ("a", SymbolKind::METHOD),
            ],
            "{names:?}"
        );
        let city = &children[0];
        assert_eq!(city.detail.as_deref(), Some("input · string"));
        // the selection range sits on the declaring token, not 0-width
        assert!(
            city.selection_range.end.character > city.selection_range.start.character,
            "widened to the token: {:?}",
            city.selection_range
        );
    }
}
