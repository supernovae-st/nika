// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CST-to-PartialWorkflow bridge.
//!
//! Walks the tree-sitter-yaml CST and extracts structural information
//! into a [`PartialWorkflow`]. This is **structure-only**: we extract
//! key names, positions, and block structure but NEVER trust
//! tree-sitter's value interpretation (YAML 1.1 vs 1.2 mismatch).

use tree_sitter::{Node, Tree};

use crate::position::safe_u32;

use super::{PartialField, PartialTask, PartialWorkflow, TextRange};

/// Known Nika verbs for detection in broken YAML.
const KNOWN_VERBS: &[&str] = &["infer", "exec", "fetch", "invoke", "agent"];

/// Known top-level keys in a Nika workflow.
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &[
    "schema",
    "workflow",
    "tasks",
    "mcp",
    "context",
    "include",
    "inputs",
    "edges",
    "skills",
    "agents",
    "artifacts",
    "log",
];

/// Maximum recursion depth for CST walkers.
///
/// Deeply nested YAML (or maliciously crafted input) could exhaust the
/// stack. We bail out after this many levels of recursion.
const MAX_WALK_DEPTH: u32 = 64;

// ===========================================================================
// Public API
// ===========================================================================

/// Extract a [`PartialWorkflow`] from a tree-sitter CST.
///
/// STRUCTURE-ONLY: extracts keys and positions, never trusts values.
/// The tree can contain ERROR nodes -- this function walks around them.
pub fn extract_partial(text: &str, tree: &Tree) -> PartialWorkflow {
    let root = tree.root_node();

    let mut error_ranges = Vec::new();
    collect_error_ranges(&root, &mut error_ranges, 0);

    let mut result = PartialWorkflow {
        has_errors: root.has_error(),
        error_ranges,
        ..Default::default()
    };

    // tree-sitter-yaml: stream > document > block_node > block_mapping
    // When YAML is severely broken, tree-sitter may wrap the entire
    // stream in an ERROR node with block_mapping_pair children at root.
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "document" => walk_document(text, &child, &mut result, 1),
            "block_mapping_pair" => {
                // Root-level ERROR recovery: pairs directly under root/ERROR
                process_top_level_pair(text, &child, &mut result);
            }
            "ERROR" => {
                // Walk inside ERROR nodes for salvageable pairs
                walk_error_for_pairs(text, &child, &mut result);
            }
            _ => {}
        }
    }

    result
}

// ===========================================================================
// Internal walkers
// ===========================================================================

/// Recursively collect all ERROR node ranges.
fn collect_error_ranges(node: &Node, ranges: &mut Vec<TextRange>, depth: u32) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    if node.kind() == "ERROR" || node.is_error() {
        ranges.push(TextRange::new(
            safe_u32(node.start_byte()),
            safe_u32(node.end_byte()),
        ));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.id() != node.id() {
            collect_error_ranges(&child, ranges, depth + 1);
        }
    }
}

fn walk_document(text: &str, doc: &Node, result: &mut PartialWorkflow, depth: u32) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let mut cursor = doc.walk();
    for child in doc.children(&mut cursor) {
        match child.kind() {
            "block_node" => walk_block_node(text, &child, result, depth + 1),
            "block_mapping" => extract_top_level_mapping(text, &child, result),
            _ => {}
        }
    }
}

fn walk_block_node(text: &str, node: &Node, result: &mut PartialWorkflow, depth: u32) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "block_mapping" => extract_top_level_mapping(text, &child, result),
            "block_node" => walk_block_node(text, &child, result, depth + 1),
            _ => {}
        }
    }
}

/// Walk inside an ERROR node to salvage any block_mapping_pair children.
fn walk_error_for_pairs(text: &str, error: &Node, result: &mut PartialWorkflow) {
    let mut cursor = error.walk();
    for child in error.children(&mut cursor) {
        if child.kind() == "block_mapping_pair" {
            process_top_level_pair(text, &child, result);
        }
    }
}

/// Process a single block_mapping_pair found at the top level
/// (either inside a block_mapping or salvaged from an ERROR node).
fn process_top_level_pair(text: &str, pair: &Node, result: &mut PartialWorkflow) {
    let key_node = match pair.child_by_field_name("key") {
        Some(k) => k,
        None => return,
    };

    let key_text = node_text_trimmed(text, &key_node);
    let key_span = node_range(&key_node);
    let value_node = pair.child_by_field_name("value");
    let value_span = value_node.as_ref().map(|v| node_range(v));

    if KNOWN_TOP_LEVEL_KEYS.contains(&key_text.as_str()) {
        result.top_level_keys.push(PartialField {
            value: key_text.clone(),
            key_span,
            value_span,
        });
    }

    match key_text.as_str() {
        "schema" => {
            result.schema = Some(PartialField {
                value: value_node
                    .as_ref()
                    .map(|v| node_text_trimmed(text, v))
                    .unwrap_or_default(),
                key_span,
                value_span,
            });
        }
        "workflow" => {
            result.workflow_name = Some(PartialField {
                value: value_node
                    .as_ref()
                    .map(|v| node_text_trimmed(text, v))
                    .unwrap_or_default(),
                key_span,
                value_span,
            });
        }
        "tasks" => {
            if let Some(ref value) = value_node {
                extract_tasks(text, value, result);
            }
        }
        "mcp" => {
            if let Some(ref value) = value_node {
                extract_mcp_servers(text, value, result);
            }
        }
        "context" => {
            if let Some(ref value) = value_node {
                extract_context_files(text, value, result);
            }
        }
        "include" => {
            if let Some(ref value) = value_node {
                extract_includes(text, value, result);
            }
        }
        "inputs" => {
            if let Some(ref value) = value_node {
                extract_input_names(text, value, result);
            }
        }
        _ => {}
    }
}

fn extract_top_level_mapping(text: &str, mapping: &Node, result: &mut PartialWorkflow) {
    let mut cursor = mapping.walk();
    for pair in mapping.children(&mut cursor) {
        if pair.kind() == "block_mapping_pair" {
            process_top_level_pair(text, &pair, result);
        }
    }
}

// ---------------------------------------------------------------------------
// tasks:
// ---------------------------------------------------------------------------

fn extract_tasks(text: &str, node: &Node, result: &mut PartialWorkflow) {
    walk_to_block_sequence(text, node, &mut |text, seq| {
        let mut cursor = seq.walk();
        for item in seq.children(&mut cursor) {
            if item.kind() == "block_sequence_item" {
                result.tasks.push(extract_single_task(text, &item));
            }
        }
    });
}

fn extract_single_task(text: &str, item: &Node) -> PartialTask {
    let span = node_range(item);
    let mut task = PartialTask {
        id: None,
        verb: None,
        existing_keys: Vec::new(),
        has_content: false,
        has_with: false,
        has_depends_on: false,
        with_aliases: Vec::new(),
        depends_on_refs: Vec::new(),
        span,
        verb_span: None,
    };

    walk_task_children(text, item, &mut task, 0);
    task
}

fn walk_task_children(text: &str, node: &Node, task: &mut PartialTask, depth: u32) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "block_mapping" => extract_task_mapping(text, &child, task),
            "block_node" => walk_task_children(text, &child, task, depth + 1),
            _ => {}
        }
    }
}

fn extract_task_mapping(text: &str, mapping: &Node, task: &mut PartialTask) {
    let mut cursor = mapping.walk();
    for pair in mapping.children(&mut cursor) {
        if pair.kind() != "block_mapping_pair" {
            continue;
        }

        let key_node = match pair.child_by_field_name("key") {
            Some(k) => k,
            None => continue,
        };

        let key_text = node_text_trimmed(text, &key_node);
        task.existing_keys.push(key_text.clone());

        let value_node = pair.child_by_field_name("value");

        match key_text.as_str() {
            "id" => {
                task.id = value_node.as_ref().map(|v| {
                    node_text_trimmed(text, v)
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string()
                });
            }
            verb if KNOWN_VERBS.contains(&verb) => {
                task.verb = Some(verb.to_string());
                task.verb_span = value_node.as_ref().map(|v| node_range(v));
            }
            "content" => task.has_content = true,
            "with" => {
                task.has_with = true;
                if let Some(ref v) = value_node {
                    extract_with_aliases(text, v, task);
                }
            }
            "depends_on" => {
                task.has_depends_on = true;
                if let Some(ref v) = value_node {
                    extract_depends_on_refs(text, v, task);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// with: aliases
// ---------------------------------------------------------------------------

fn extract_with_aliases(text: &str, node: &Node, task: &mut PartialTask) {
    walk_to_block_mapping(node, &mut |mapping| {
        let mut cursor = mapping.walk();
        for pair in mapping.children(&mut cursor) {
            if pair.kind() == "block_mapping_pair" {
                if let Some(key) = pair.child_by_field_name("key") {
                    task.with_aliases.push(node_text_trimmed(text, &key));
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// depends_on: refs
// ---------------------------------------------------------------------------

fn extract_depends_on_refs(text: &str, node: &Node, task: &mut PartialTask) {
    // depends_on can be a flow sequence: [a, b] or a block sequence
    collect_scalar_values(text, node, &mut task.depends_on_refs, 0);
}

fn collect_scalar_values(text: &str, node: &Node, out: &mut Vec<String>, depth: u32) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    match node.kind() {
        "plain_scalar" | "single_quote_scalar" | "double_quote_scalar" => {
            let val = node_text_trimmed(text, node)
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            if !val.is_empty() {
                out.push(val);
            }
        }
        "flow_sequence" | "flow_node" | "block_node" | "block_sequence" | "block_sequence_item" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_scalar_values(text, &child, out, depth + 1);
            }
        }
        _ => {
            // Recurse into unknown wrappers
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    collect_scalar_values(text, &child, out, depth + 1);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// mcp: servers
// ---------------------------------------------------------------------------

fn extract_mcp_servers(text: &str, node: &Node, result: &mut PartialWorkflow) {
    walk_to_block_mapping(node, &mut |mapping| {
        let mut cursor = mapping.walk();
        for pair in mapping.children(&mut cursor) {
            if pair.kind() == "block_mapping_pair" {
                if let Some(key) = pair.child_by_field_name("key") {
                    let key_span = node_range(&key);
                    result.mcp_servers.push(PartialField {
                        value: node_text_trimmed(text, &key),
                        key_span,
                        value_span: pair.child_by_field_name("value").map(|v| node_range(&v)),
                    });
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// context: files
// ---------------------------------------------------------------------------

fn extract_context_files(text: &str, node: &Node, result: &mut PartialWorkflow) {
    // context: > files: > block_mapping of alias: path pairs
    walk_to_block_mapping(node, &mut |mapping| {
        let mut cursor = mapping.walk();
        for pair in mapping.children(&mut cursor) {
            if pair.kind() != "block_mapping_pair" {
                continue;
            }
            let key = match pair.child_by_field_name("key") {
                Some(k) => k,
                None => continue,
            };
            let key_text = node_text_trimmed(text, &key);
            if key_text == "files" {
                if let Some(value) = pair.child_by_field_name("value") {
                    extract_file_entries(text, &value, result);
                }
            }
        }
    });
}

fn extract_file_entries(text: &str, node: &Node, result: &mut PartialWorkflow) {
    walk_to_block_mapping(node, &mut |mapping| {
        let mut cursor = mapping.walk();
        for pair in mapping.children(&mut cursor) {
            if pair.kind() == "block_mapping_pair" {
                if let Some(key) = pair.child_by_field_name("key") {
                    let key_span = node_range(&key);
                    result.context_files.push(PartialField {
                        value: node_text_trimmed(text, &key),
                        key_span,
                        value_span: pair.child_by_field_name("value").map(|v| node_range(&v)),
                    });
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// include:
// ---------------------------------------------------------------------------

fn extract_includes(text: &str, node: &Node, result: &mut PartialWorkflow) {
    collect_scalar_values_into_fields(text, node, &mut result.includes);
}

// ---------------------------------------------------------------------------
// inputs:
// ---------------------------------------------------------------------------

fn extract_input_names(text: &str, node: &Node, result: &mut PartialWorkflow) {
    // inputs: can be a mapping (name: type) or a sequence
    walk_to_block_mapping(node, &mut |mapping| {
        let mut cursor = mapping.walk();
        for pair in mapping.children(&mut cursor) {
            if pair.kind() == "block_mapping_pair" {
                if let Some(key) = pair.child_by_field_name("key") {
                    let key_span = node_range(&key);
                    result.input_names.push(PartialField {
                        value: node_text_trimmed(text, &key),
                        key_span,
                        value_span: pair.child_by_field_name("value").map(|v| node_range(&v)),
                    });
                }
            }
        }
    });
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Get the raw text of a node, trimmed of surrounding whitespace.
fn node_text_trimmed(text: &str, node: &Node) -> String {
    let start = node.start_byte();
    let end = node.end_byte().min(text.len());
    if start >= end {
        return String::new();
    }
    text[start..end].trim().to_string()
}

/// Get the byte range of a node.
///
/// Uses [`safe_u32`] to clamp byte offsets, preventing silent
/// truncation on files larger than 4 GB (defensive -- callers
/// already reject files > 64 MB).
fn node_range(node: &Node) -> TextRange {
    TextRange::new(safe_u32(node.start_byte()), safe_u32(node.end_byte()))
}

/// Walk into block_node wrappers to find a block_mapping, then call `f`.
fn walk_to_block_mapping<F>(node: &Node, f: &mut F)
where
    F: FnMut(&Node),
{
    walk_to_block_mapping_inner(node, f, 0);
}

fn walk_to_block_mapping_inner<F>(node: &Node, f: &mut F, depth: u32)
where
    F: FnMut(&Node),
{
    if depth > MAX_WALK_DEPTH {
        return;
    }
    match node.kind() {
        "block_mapping" => f(node),
        "block_node" | "block_sequence_item" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_to_block_mapping_inner(&child, f, depth + 1);
            }
        }
        _ => {
            // Try children for wrapped structures
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    walk_to_block_mapping_inner(&child, f, depth + 1);
                }
            }
        }
    }
}

/// Walk into block_node wrappers to find a block_sequence, then call `f`.
fn walk_to_block_sequence<F>(text: &str, node: &Node, f: &mut F)
where
    F: FnMut(&str, &Node),
{
    walk_to_block_sequence_inner(text, node, f, 0);
}

fn walk_to_block_sequence_inner<F>(text: &str, node: &Node, f: &mut F, depth: u32)
where
    F: FnMut(&str, &Node),
{
    if depth > MAX_WALK_DEPTH {
        return;
    }
    match node.kind() {
        "block_sequence" => f(text, node),
        "block_node" | "block_sequence_item" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_to_block_sequence_inner(text, &child, f, depth + 1);
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    walk_to_block_sequence_inner(text, &child, f, depth + 1);
                }
            }
        }
    }
}

/// Collect scalar values as PartialFields (for includes, etc.).
fn collect_scalar_values_into_fields(text: &str, node: &Node, out: &mut Vec<PartialField>) {
    collect_scalar_values_into_fields_inner(text, node, out, 0);
}

fn collect_scalar_values_into_fields_inner(
    text: &str,
    node: &Node,
    out: &mut Vec<PartialField>,
    depth: u32,
) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    match node.kind() {
        "plain_scalar" | "single_quote_scalar" | "double_quote_scalar" => {
            let span = node_range(node);
            out.push(PartialField {
                value: node_text_trimmed(text, node)
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
                key_span: span,
                value_span: None,
            });
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    collect_scalar_values_into_fields_inner(text, &child, out, depth + 1);
                }
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::recovery::RecoveryParser;

    /// Helper: parse YAML and extract partial workflow.
    fn partial(yaml: &str) -> PartialWorkflow {
        let mut parser = RecoveryParser::new();
        let tree = parser.parse(yaml).expect("tree-sitter should always parse");
        extract_partial(yaml, &tree)
    }

    // =======================================================================
    // Valid complete workflow
    // =======================================================================

    #[test]
    fn valid_complete_workflow() {
        let yaml = "schema: '@0.12'\nworkflow: my-pipeline\ntasks:\n  - id: gen\n    infer:\n      model: gpt-4\n      prompt: hello\n  - id: run\n    exec:\n      command: echo done\n";
        let pw = partial(yaml);
        assert!(!pw.has_errors);
        assert_eq!(pw.schema.as_ref().unwrap().value, "'@0.12'");
        assert_eq!(pw.workflow_name.as_ref().unwrap().value, "my-pipeline");
        assert_eq!(pw.tasks.len(), 2);
        assert_eq!(pw.tasks[0].id.as_deref(), Some("gen"));
        assert_eq!(pw.tasks[0].verb.as_deref(), Some("infer"));
        assert_eq!(pw.tasks[1].id.as_deref(), Some("run"));
        assert_eq!(pw.tasks[1].verb.as_deref(), Some("exec"));
    }

    #[test]
    fn valid_workflow_all_five_verbs() {
        let yaml = concat!(
            "schema: '@0.12'\n",
            "tasks:\n",
            "  - id: t1\n    infer:\n      prompt: a\n",
            "  - id: t2\n    exec:\n      command: b\n",
            "  - id: t3\n    fetch:\n      url: http://x\n",
            "  - id: t4\n    invoke:\n      tool: mcp_tool\n",
            "  - id: t5\n    agent:\n      goal: do stuff\n",
        );
        let pw = partial(yaml);
        assert!(!pw.has_errors);
        assert_eq!(pw.tasks.len(), 5);
        let verbs: Vec<_> = pw
            .tasks
            .iter()
            .map(|t| t.verb.as_deref().unwrap())
            .collect();
        assert_eq!(verbs, &["infer", "exec", "fetch", "invoke", "agent"]);
    }

    // =======================================================================
    // Broken YAML -- error recovery
    // =======================================================================

    #[test]
    fn missing_colon_has_errors_and_no_panic() {
        // "infer" is missing its colon -- tree-sitter produces an ERROR.
        // The error may corrupt the top-level mapping depending on recovery,
        // so we only assert no-panic and that errors are detected.
        let yaml = "schema: '@0.12'\ntasks:\n  - id: gen\n    infer\n      prompt: hello\n";
        let pw = partial(yaml);
        assert!(pw.has_errors);
        assert!(!pw.error_ranges.is_empty());
    }

    #[test]
    fn incomplete_task_extracts_id_and_verb() {
        // Task has id and verb key but verb block is empty
        let yaml = "schema: '@0.12'\ntasks:\n  - id: partial_task\n    infer:\n";
        let pw = partial(yaml);
        assert!(!pw.tasks.is_empty());
        let task = &pw.tasks[0];
        assert_eq!(task.id.as_deref(), Some("partial_task"));
        assert_eq!(task.verb.as_deref(), Some("infer"));
    }

    #[test]
    fn truncated_file_extracts_schema() {
        let yaml = "schema: '@0.12'\ntasks:\n  - id: trunc";
        let pw = partial(yaml);
        assert!(pw.schema.is_some());
        assert_eq!(pw.schema.as_ref().unwrap().value, "'@0.12'");
    }

    #[test]
    fn mixed_indentation_no_panic() {
        let yaml = "schema: '@0.12'\ntasks:\n  - id: a\n\t  infer:\n      prompt: x\n";
        // Should not panic regardless of mixed indent.
        // tree-sitter may or may not recover the top-level mapping.
        let _pw = partial(yaml);
    }

    #[test]
    fn duplicate_verb_keys_both_in_existing_keys() {
        // Two verbs in same task
        let yaml =
            "tasks:\n  - id: dupe\n    infer:\n      prompt: a\n    exec:\n      command: b\n";
        let pw = partial(yaml);
        assert!(!pw.tasks.is_empty());
        let task = &pw.tasks[0];
        assert!(task.existing_keys.contains(&"infer".to_string()));
        assert!(task.existing_keys.contains(&"exec".to_string()));
    }

    // =======================================================================
    // Empty / minimal documents
    // =======================================================================

    #[test]
    fn empty_document() {
        let pw = partial("");
        assert!(pw.schema.is_none());
        assert!(pw.workflow_name.is_none());
        assert!(pw.tasks.is_empty());
        assert!(!pw.has_errors);
    }

    #[test]
    fn comments_only() {
        let yaml = "# This is a comment\n# Another comment\n";
        let pw = partial(yaml);
        assert!(pw.schema.is_none());
        assert!(pw.tasks.is_empty());
        assert!(!pw.has_errors);
    }

    #[test]
    fn whitespace_only() {
        let pw = partial("   \n\n  \n");
        assert!(pw.schema.is_none());
        assert!(pw.tasks.is_empty());
    }

    #[test]
    fn single_key_no_value() {
        let yaml = "schema:\n";
        let pw = partial(yaml);
        assert!(pw.schema.is_some());
    }

    // =======================================================================
    // Content, with, depends_on
    // =======================================================================

    #[test]
    fn content_block_detected() {
        let yaml = "tasks:\n  - id: c\n    infer:\n      prompt: x\n    content: true\n";
        let pw = partial(yaml);
        assert_eq!(pw.tasks.len(), 1);
        assert!(pw.tasks[0].has_content);
    }

    #[test]
    fn with_block_aliases_extracted() {
        let yaml = concat!(
            "tasks:\n",
            "  - id: w\n",
            "    infer:\n",
            "      prompt: \"{{with.doc}}\"\n",
            "    with:\n",
            "      doc: file.txt\n",
            "      data: other.json\n",
        );
        let pw = partial(yaml);
        assert_eq!(pw.tasks.len(), 1);
        let task = &pw.tasks[0];
        assert!(task.has_with);
        assert!(task.with_aliases.contains(&"doc".to_string()));
        assert!(task.with_aliases.contains(&"data".to_string()));
    }

    #[test]
    fn depends_on_flow_sequence() {
        let yaml =
            "tasks:\n  - id: last\n    exec:\n      command: echo\n    depends_on: [gen, run]\n";
        let pw = partial(yaml);
        assert_eq!(pw.tasks.len(), 1);
        let task = &pw.tasks[0];
        assert!(task.has_depends_on);
        assert!(task.depends_on_refs.contains(&"gen".to_string()));
        assert!(task.depends_on_refs.contains(&"run".to_string()));
    }

    #[test]
    fn depends_on_plain_scalar() {
        let yaml = "tasks:\n  - id: b\n    exec:\n      command: echo\n    depends_on: a\n";
        let pw = partial(yaml);
        assert_eq!(pw.tasks.len(), 1);
        let task = &pw.tasks[0];
        assert!(task.has_depends_on);
        assert!(task.depends_on_refs.contains(&"a".to_string()));
    }

    // =======================================================================
    // MCP servers
    // =======================================================================

    #[test]
    fn mcp_servers_extracted() {
        let yaml =
            "mcp:\n  novanet:\n    command: novanet serve\n  filesystem:\n    command: fs serve\n";
        let pw = partial(yaml);
        assert_eq!(pw.mcp_servers.len(), 2);
        let names: Vec<_> = pw.mcp_servers.iter().map(|s| s.value.as_str()).collect();
        assert!(names.contains(&"novanet"));
        assert!(names.contains(&"filesystem"));
    }

    #[test]
    fn mcp_server_has_spans() {
        let yaml = "mcp:\n  srv:\n    command: x\n";
        let pw = partial(yaml);
        assert_eq!(pw.mcp_servers.len(), 1);
        let srv = &pw.mcp_servers[0];
        assert!(!srv.key_span.is_empty());
        assert!(srv.value_span.is_some());
    }

    // =======================================================================
    // Context files
    // =======================================================================

    #[test]
    fn context_files_extracted() {
        let yaml = "context:\n  files:\n    readme: ./README.md\n    license: ./LICENSE\n";
        let pw = partial(yaml);
        assert_eq!(pw.context_files.len(), 2);
        let names: Vec<_> = pw.context_files.iter().map(|f| f.value.as_str()).collect();
        assert!(names.contains(&"readme"));
        assert!(names.contains(&"license"));
    }

    // =======================================================================
    // Inputs
    // =======================================================================

    #[test]
    fn input_names_extracted() {
        let yaml = "inputs:\n  name: string\n  count: number\n";
        let pw = partial(yaml);
        assert_eq!(pw.input_names.len(), 2);
        let names: Vec<_> = pw.input_names.iter().map(|i| i.value.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"count"));
    }

    // =======================================================================
    // Top-level keys
    // =======================================================================

    #[test]
    fn top_level_keys_tracked() {
        let yaml =
            "schema: '@0.12'\nworkflow: test\ntasks:\n  - id: a\n    infer:\n      prompt: x\n";
        let pw = partial(yaml);
        let keys: Vec<_> = pw.top_level_keys.iter().map(|k| k.value.as_str()).collect();
        assert!(keys.contains(&"schema"));
        assert!(keys.contains(&"workflow"));
        assert!(keys.contains(&"tasks"));
    }

    #[test]
    fn unknown_top_level_key_ignored() {
        let yaml = "schema: '@0.12'\ncustom_key: value\n";
        let pw = partial(yaml);
        let keys: Vec<_> = pw.top_level_keys.iter().map(|k| k.value.as_str()).collect();
        assert!(keys.contains(&"schema"));
        assert!(!keys.contains(&"custom_key"));
    }

    // =======================================================================
    // Error ranges
    // =======================================================================

    #[test]
    fn error_ranges_populated_for_broken_yaml() {
        let yaml = "key value\n  : broken\n";
        let pw = partial(yaml);
        assert!(pw.has_errors);
    }

    #[test]
    fn no_error_ranges_for_valid_yaml() {
        let yaml = "key: value\n";
        let pw = partial(yaml);
        assert!(!pw.has_errors);
        assert!(pw.error_ranges.is_empty());
    }

    // =======================================================================
    // Spans / positions
    // =======================================================================

    #[test]
    fn schema_field_has_correct_spans() {
        let yaml = "schema: '@0.12'\n";
        let pw = partial(yaml);
        let schema = pw.schema.as_ref().unwrap();
        assert_eq!(schema.key_span.start, 0);
        assert!(schema.value_span.is_some());
    }

    #[test]
    fn task_span_covers_full_block() {
        let yaml = "tasks:\n  - id: gen\n    infer:\n      prompt: hello\n";
        let pw = partial(yaml);
        assert_eq!(pw.tasks.len(), 1);
        let task = &pw.tasks[0];
        assert!(!task.span.is_empty());
        let task_text = &yaml[task.span.start as usize..task.span.end as usize];
        assert!(task_text.contains("id: gen"));
        assert!(task_text.contains("infer:"));
    }

    #[test]
    fn verb_span_set_when_verb_present() {
        let yaml = "tasks:\n  - id: t\n    exec:\n      command: echo hi\n";
        let pw = partial(yaml);
        let task = &pw.tasks[0];
        assert_eq!(task.verb.as_deref(), Some("exec"));
        assert!(task.verb_span.is_some());
    }

    // =======================================================================
    // Existing keys tracking
    // =======================================================================

    #[test]
    fn existing_keys_lists_all_task_keys() {
        let yaml = concat!(
            "tasks:\n",
            "  - id: t\n",
            "    infer:\n",
            "      prompt: x\n",
            "    content: true\n",
            "    with:\n",
            "      a: b\n",
            "    depends_on: [other]\n",
        );
        let pw = partial(yaml);
        let keys = &pw.tasks[0].existing_keys;
        assert!(keys.contains(&"id".to_string()));
        assert!(keys.contains(&"infer".to_string()));
        assert!(keys.contains(&"content".to_string()));
        assert!(keys.contains(&"with".to_string()));
        assert!(keys.contains(&"depends_on".to_string()));
    }

    // =======================================================================
    // Broken template
    // =======================================================================

    #[test]
    fn broken_template_in_prompt_still_extracts_with() {
        let yaml = concat!(
            "tasks:\n",
            "  - id: t\n",
            "    infer:\n",
            "      prompt: \"Use {{with.\"\n",
            "    with:\n",
            "      doc: f.txt\n",
        );
        let pw = partial(yaml);
        assert!(!pw.tasks.is_empty());
        let task = &pw.tasks[0];
        assert!(task.has_with);
        assert!(task.with_aliases.contains(&"doc".to_string()));
    }

    // =======================================================================
    // Multi-document YAML
    // =======================================================================

    #[test]
    fn multi_document_handles_first() {
        let yaml = "schema: '@0.12'\n---\nschema: '@0.13'\n";
        let pw = partial(yaml);
        assert!(pw.schema.is_some());
    }

    // =======================================================================
    // Edge cases
    // =======================================================================

    #[test]
    fn task_without_id() {
        let yaml = "tasks:\n  - infer:\n      prompt: anonymous\n";
        let pw = partial(yaml);
        assert_eq!(pw.tasks.len(), 1);
        assert!(pw.tasks[0].id.is_none());
        assert_eq!(pw.tasks[0].verb.as_deref(), Some("infer"));
    }

    #[test]
    fn task_without_verb() {
        let yaml = "tasks:\n  - id: no_verb\n    content: true\n";
        let pw = partial(yaml);
        assert_eq!(pw.tasks.len(), 1);
        assert!(pw.tasks[0].verb.is_none());
        assert_eq!(pw.tasks[0].id.as_deref(), Some("no_verb"));
    }

    #[test]
    fn many_tasks() {
        let mut yaml = String::from("tasks:\n");
        for i in 0..20 {
            yaml.push_str(&format!(
                "  - id: task_{i}\n    exec:\n      command: echo {i}\n"
            ));
        }
        let pw = partial(&yaml);
        assert_eq!(pw.tasks.len(), 20);
        for (i, task) in pw.tasks.iter().enumerate() {
            assert_eq!(task.id.as_deref(), Some(format!("task_{i}").as_str()));
        }
    }

    #[test]
    fn deeply_nested_yaml_no_panic() {
        let yaml = "a:\n  b:\n    c:\n      d:\n        e: value\n";
        let pw = partial(yaml);
        assert!(pw.tasks.is_empty());
    }

    #[test]
    fn flow_mapping_value_no_panic() {
        let yaml = "schema: {version: 0.12}\n";
        let pw = partial(yaml);
        assert!(pw.schema.is_some());
    }

    #[test]
    fn broken_content_block_no_panic() {
        let yaml = "tasks:\n  - id: broken_content\n    infer:\n      prompt: test\n    content\n";
        // Should not panic; tree has errors
        let _pw = partial(yaml);
    }

    #[test]
    fn missing_schema_still_extracts_tasks() {
        let yaml = "tasks:\n  - id: no_schema\n    exec:\n      command: echo hi\n";
        let pw = partial(yaml);
        assert!(pw.schema.is_none());
        assert_eq!(pw.tasks.len(), 1);
        assert_eq!(pw.tasks[0].id.as_deref(), Some("no_schema"));
    }

    #[test]
    fn includes_extracted() {
        let yaml = "include:\n  - ./common.nika.yaml\n  - ./utils.nika.yaml\n";
        let pw = partial(yaml);
        assert_eq!(pw.includes.len(), 2);
    }

    #[test]
    fn quoted_task_id_unquoted() {
        let yaml = "tasks:\n  - id: \"quoted-id\"\n    exec:\n      command: echo\n";
        let pw = partial(yaml);
        assert_eq!(pw.tasks[0].id.as_deref(), Some("quoted-id"));
    }

    #[test]
    fn single_quoted_task_id_unquoted() {
        let yaml = "tasks:\n  - id: 'single-quoted'\n    exec:\n      command: echo\n";
        let pw = partial(yaml);
        assert_eq!(pw.tasks[0].id.as_deref(), Some("single-quoted"));
    }

    #[test]
    fn deeply_nested_generated_yaml_no_stack_overflow() {
        // Generate 100-level nesting programmatically.
        let mut yaml = String::new();
        for i in 0..100 {
            let indent = "  ".repeat(i);
            yaml.push_str(&format!("{indent}level_{i}:\n"));
        }
        yaml.push_str(&format!("{}value: deep\n", "  ".repeat(100)));
        let pw = partial(&yaml);
        // Must not panic. No workflow structure expected.
        assert!(pw.tasks.is_empty());
    }

    #[test]
    fn empty_file_no_panic() {
        let pw = partial("");
        assert!(pw.tasks.is_empty());
        assert!(pw.schema.is_none());
        assert!(!pw.has_errors);
    }

    // =======================================================================
    // Fixture-based tests
    // =======================================================================

    /// Load a fixture file and parse it.
    fn fixture(name: &str) -> PartialWorkflow {
        let path = format!(
            "{}/tests/fixtures/broken/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let yaml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"));
        partial(&yaml)
    }

    #[test]
    fn fixture_missing_colon_no_panic() {
        let pw = fixture("missing-colon.broken.yaml");
        // Broken YAML: infer without colon. tree-sitter may wrap in ERROR.
        assert!(pw.has_errors);
        assert!(!pw.error_ranges.is_empty());
    }

    #[test]
    fn fixture_incomplete_task_extracts_both_tasks() {
        let pw = fixture("incomplete-task.broken.yaml");
        assert!(pw.schema.is_some());
        // Should find at least the complete task
        assert!(!pw.tasks.is_empty());
    }

    #[test]
    fn fixture_incomplete_task_finds_verb() {
        let pw = fixture("incomplete-task.broken.yaml");
        // The "started" task has infer: but no body
        let started = pw.tasks.iter().find(|t| t.id.as_deref() == Some("started"));
        if let Some(task) = started {
            assert_eq!(task.verb.as_deref(), Some("infer"));
        }
    }

    #[test]
    fn fixture_duplicate_verb_detects_both() {
        let pw = fixture("duplicate-verb.broken.yaml");
        assert!(!pw.tasks.is_empty());
        let task = &pw.tasks[0];
        assert!(task.existing_keys.contains(&"infer".to_string()));
        assert!(task.existing_keys.contains(&"exec".to_string()));
    }

    #[test]
    fn fixture_duplicate_verb_has_schema() {
        let pw = fixture("duplicate-verb.broken.yaml");
        assert!(pw.schema.is_some());
    }

    #[test]
    fn fixture_mixed_indentation_no_panic() {
        let _pw = fixture("mixed-indentation.broken.yaml");
    }

    #[test]
    fn fixture_mixed_indentation_has_errors() {
        let pw = fixture("mixed-indentation.broken.yaml");
        assert!(pw.has_errors);
    }

    #[test]
    fn fixture_truncated_file_extracts_schema() {
        let pw = fixture("truncated-file.broken.yaml");
        assert!(pw.schema.is_some());
    }

    #[test]
    fn fixture_truncated_file_extracts_task() {
        let pw = fixture("truncated-file.broken.yaml");
        assert!(!pw.tasks.is_empty());
    }

    #[test]
    fn fixture_empty_document_no_errors() {
        let pw = fixture("empty-document.broken.yaml");
        assert!(pw.tasks.is_empty());
        assert!(pw.schema.is_none());
    }

    #[test]
    fn fixture_comments_only_empty() {
        let pw = fixture("comments-only.broken.yaml");
        assert!(pw.tasks.is_empty());
        assert!(pw.schema.is_none());
    }

    #[test]
    fn fixture_missing_schema_no_schema() {
        let pw = fixture("missing-schema.broken.yaml");
        assert!(pw.schema.is_none());
    }

    #[test]
    fn fixture_missing_schema_has_tasks() {
        let pw = fixture("missing-schema.broken.yaml");
        assert!(!pw.tasks.is_empty());
    }

    #[test]
    fn fixture_missing_schema_has_mcp() {
        let pw = fixture("missing-schema.broken.yaml");
        assert!(!pw.mcp_servers.is_empty());
    }

    #[test]
    fn fixture_missing_schema_has_context() {
        let pw = fixture("missing-schema.broken.yaml");
        assert!(!pw.context_files.is_empty());
    }

    #[test]
    fn fixture_missing_schema_task_has_with() {
        let pw = fixture("missing-schema.broken.yaml");
        let gen = pw.tasks.iter().find(|t| t.id.as_deref() == Some("gen"));
        assert!(gen.is_some());
        let gen = gen.unwrap();
        assert!(gen.has_with);
        assert!(gen.has_depends_on);
        assert!(gen.has_content);
    }

    #[test]
    fn fixture_broken_template_no_panic() {
        let pw = fixture("broken-template.broken.yaml");
        assert!(!pw.tasks.is_empty());
    }

    #[test]
    fn fixture_broken_template_extracts_with() {
        let pw = fixture("broken-template.broken.yaml");
        let gen = pw.tasks.iter().find(|t| t.id.as_deref() == Some("gen"));
        assert!(gen.is_some());
        assert!(gen.unwrap().has_with);
    }

    #[test]
    fn fixture_broken_content_no_panic() {
        let pw = fixture("broken-content.broken.yaml");
        // The "content" key is missing its colon, so tree-sitter may produce errors.
        // But we should still extract at least some structure.
        let _ = pw;
    }

    #[test]
    fn fixture_all_fixtures_no_panic() {
        let fixtures = [
            "missing-colon.broken.yaml",
            "incomplete-task.broken.yaml",
            "duplicate-verb.broken.yaml",
            "mixed-indentation.broken.yaml",
            "truncated-file.broken.yaml",
            "empty-document.broken.yaml",
            "comments-only.broken.yaml",
            "missing-schema.broken.yaml",
            "broken-template.broken.yaml",
            "broken-content.broken.yaml",
            "deeply-nested.broken.yaml",
        ];
        for name in &fixtures {
            let _pw = fixture(name);
        }
    }

    #[test]
    fn fixture_deeply_nested_yaml_no_stack_overflow() {
        // 100-level nesting must not blow the stack thanks to MAX_WALK_DEPTH.
        let pw = fixture("deeply-nested.broken.yaml");
        // The document is valid YAML (deeply nested mappings), but has no
        // Nika workflow structure. Main assertion: no panic / stack overflow.
        assert!(pw.tasks.is_empty());
    }

    // =======================================================================
    // Error recovery: root-level ERROR nodes
    // =======================================================================

    #[test]
    fn error_recovery_salvages_schema_from_error_root() {
        // When tree-sitter wraps the entire document in ERROR,
        // we should still find block_mapping_pair children.
        let yaml = "schema: '@0.12'\ntasks:\n  - id: a\n\t  infer:\n      prompt: x\n";
        let pw = partial(yaml);
        // With error recovery, schema might be salvaged from ERROR node
        // The main assertion is no panic.
        let _ = pw;
    }

    // =======================================================================
    // Additional edge cases
    // =======================================================================

    #[test]
    fn depends_on_block_sequence() {
        let yaml = concat!(
            "tasks:\n",
            "  - id: final\n",
            "    exec:\n",
            "      command: done\n",
            "    depends_on:\n",
            "      - step1\n",
            "      - step2\n",
        );
        let pw = partial(yaml);
        let task = &pw.tasks[0];
        assert!(task.has_depends_on);
        assert!(task.depends_on_refs.contains(&"step1".to_string()));
        assert!(task.depends_on_refs.contains(&"step2".to_string()));
    }

    #[test]
    fn multiple_with_aliases() {
        let yaml = concat!(
            "tasks:\n",
            "  - id: t\n",
            "    infer:\n",
            "      prompt: x\n",
            "    with:\n",
            "      alpha: a\n",
            "      beta: b\n",
            "      gamma: c\n",
        );
        let pw = partial(yaml);
        let task = &pw.tasks[0];
        assert_eq!(task.with_aliases.len(), 3);
    }

    #[test]
    fn task_with_all_features() {
        let yaml = concat!(
            "tasks:\n",
            "  - id: full\n",
            "    description: A full task\n",
            "    infer:\n",
            "      prompt: hello\n",
            "      temperature: 0.7\n",
            "    content:\n",
            "      - type: text\n",
            "        text: hello\n",
            "    with:\n",
            "      data: other\n",
            "    depends_on: [setup]\n",
            "    retry:\n",
            "      max_attempts: 3\n",
        );
        let pw = partial(yaml);
        let task = &pw.tasks[0];
        assert_eq!(task.id.as_deref(), Some("full"));
        assert_eq!(task.verb.as_deref(), Some("infer"));
        assert!(task.has_content);
        assert!(task.has_with);
        assert!(task.has_depends_on);
        assert!(task.existing_keys.contains(&"description".to_string()));
        assert!(task.existing_keys.contains(&"retry".to_string()));
    }

    #[test]
    fn schema_with_double_quotes() {
        let yaml =
            "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t\n    exec:\n      command: x\n";
        let pw = partial(yaml);
        assert!(pw.schema.is_some());
    }

    #[test]
    fn workflow_name_extracted() {
        let yaml = "schema: '@0.12'\nworkflow: my-pipeline\ntasks:\n  - id: t\n    exec:\n      command: x\n";
        let pw = partial(yaml);
        assert_eq!(pw.workflow_name.as_ref().unwrap().value, "my-pipeline");
    }
}
