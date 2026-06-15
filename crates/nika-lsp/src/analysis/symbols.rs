// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Document symbols — the task outline.
//!
//! A parsed [`RawWorkflow`] becomes a one-root tree: the workflow node
//! (named by its `workflow:` id) with one child per task. Each task child
//! is named by its `id`, kinded as a method, and detailed with its verb
//! (`infer · exec · invoke · agent`). The task child's selection range is
//! the `id`'s own span (so « reveal symbol » jumps to the id token).
//!
//! Single-file, pure: `(text) -> Vec<DocumentSymbol>`. A document that does
//! not parse yields an empty outline (the diagnostics surface owns the
//! error; the outline simply has nothing to show).

use lsp_types::{DocumentSymbol, Range, SymbolKind};
use nika_schema::raw::RawWorkflow;
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
    let children: Vec<DocumentSymbol> = wf
        .tasks
        .iter()
        .map(|task| task_symbol(index, &task.value, task.span))
        .collect();
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
        let yaml = "nika: v1\nworkflow: my-flow\ntasks:\n  - id: a\n    exec: { command: \"x\" }\n";
        let syms = document_symbols(yaml);
        assert_eq!(syms.len(), 1, "one root");
        assert_eq!(syms[0].name, "my-flow");
        assert_eq!(syms[0].kind, SymbolKind::MODULE);
        assert_eq!(syms[0].detail.as_deref(), Some("workflow"));
    }

    #[test]
    fn each_task_is_a_child_detailed_with_its_verb() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: fetch_it\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n  - id: think\n    depends_on: [fetch_it]\n    infer: { prompt: \"hi\", max_tokens: 10 }\n";
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
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: hello\n    exec: { command: \"x\" }\n";
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
        // both task enclosing ranges. child[0] encloses (3,6)..(5,2),
        // child[1] encloses (5,6)..(7,0) · the union is (3,6)..(7,0).
        // This pins union_range + min_pos (start) + max_pos (end) to exact
        // positions — Default::default() (0,0)..(0,0), a flipped min_pos
        // (returns (5,6)) or a flipped max_pos (returns (5,2)) all diverge.
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: aa\n    exec: { command: \"x\" }\n  - id: bbb\n    exec: { command: \"y\" }\n";
        let syms = document_symbols(yaml);
        let children = syms[0].children.as_ref().expect("children");
        assert_eq!(children[0].range, rng(3, 6, 5, 2), "child 0 enclosing");
        assert_eq!(children[1].range, rng(5, 6, 7, 0), "child 1 enclosing");
        assert_eq!(
            syms[0].range,
            rng(3, 6, 7, 0),
            "root unions both children: min start (3,6), max end (7,0)"
        );
    }

    #[test]
    fn task_enclosing_range_unions_id_into_the_task_span() {
        // A single task · its enclosing range unions the task span with the
        // id span. The task span is (3,6)..(5,0), the id span is the point
        // (3,8) · the union keeps the wider span. min_pos((3,6),(3,8))=(3,6),
        // max_pos((5,0),(3,8))=(5,0) → (3,6)..(5,0).
        let yaml = "nika: v1\nworkflow: my-flow\ntasks:\n  - id: a\n    exec: { command: \"x\" }\n";
        let syms = document_symbols(yaml);
        let child = &syms[0].children.as_ref().expect("children")[0];
        assert_eq!(
            child.range,
            rng(3, 6, 5, 0),
            "id span folded into task span"
        );
        // the selection range is the id point (3,8)..(3,8)
        assert_eq!(child.selection_range, rng(3, 8, 3, 8), "id selection");
    }

    #[test]
    fn taskless_workflow_root_spans_the_whole_document() {
        // No tasks · the root range falls back to `whole_document`, which is
        // (0,0)..(end-of-doc). The doc is 4 lines (trailing \n → empty line
        // 3) so the end is (3,0). Default::default() would wrongly give
        // (0,0)..(0,0).
        let yaml = "nika: v1\nworkflow: empty\ntasks: []\n";
        let syms = document_symbols(yaml);
        assert_eq!(syms.len(), 1, "one root even with no tasks");
        let root = &syms[0];
        assert!(
            root.children.as_ref().is_none_or(Vec::is_empty),
            "no task children"
        );
        assert_eq!(
            root.range,
            rng(0, 0, 3, 0),
            "whole-document fallback span, not the (0,0) default"
        );
    }

    #[test]
    fn all_four_verbs_render() {
        let yaml = "nika: v1\nworkflow: w\nmodel: ollama/m\ntasks:\n  - id: i\n    infer: { prompt: \"p\", max_tokens: 5 }\n  - id: e\n    exec: { command: \"x\" }\n  - id: v\n    invoke: { tool: \"nika:read\", args: { path: \"./p\" } }\n  - id: g\n    agent: { prompt: \"p\", tools: [\"nika:read\"], max_turns: 2 }\n";
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
}
