// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Context-aware completion.
//!
//! v0.1 uses robust line-prefix heuristics (no incremental AST) to pick
//! one of five contexts, each backed by the [`vocab`] tables
//! or the workflow's own task ids ·
//!
//! 1. **`model:` value** — after `model:` (or `model: <provider>/`),
//!    suggest the provider names (with the trailing `/`).
//! 2. **task reference** — inside `depends_on:` `[ … ]` or after a
//!    `tasks.` inside `${{ … }}`, suggest the workflow's own task ids.
//! 3. **task field** — an indented key position inside a task, suggest the
//!    task-field keys + the 4 verbs.
//! 4. **top-level key** — a column-0 key position, suggest the envelope
//!    keys.
//! 5. **none** — inside a free-form value with no trigger, suggest nothing
//!    (silence beats noise).
//!
//! Pure: `(text, offset) -> Vec<CompletionItem>`. Task ids are read from a
//! best-effort parse (lenient · a partially-typed document still parses
//! enough of its tasks to suggest their ids).

use std::path::Path;

use lsp_types::{CompletionItem, CompletionItemKind};
use nika_schema::{FileId, ParseMode, parse};

use super::islands;
use super::members;
use super::refs;
use super::scope;
use super::skills;
use super::vocab::{self, Entry};

/// Compute completion items for the cursor at `offset` in `text` —
/// the pure lane set (everything derivable from the text alone).
#[must_use]
pub fn completion(text: &str, offset: usize) -> Vec<CompletionItem> {
    completion_at(text, offset, None)
}

/// The full lane set: the pure lanes plus the one disk-backed lane
/// (agent `skills:` — the workspace's own `SKILL.md` files, rooted at
/// the document's directory). The server passes the document's dir;
/// pure callers (tests · non-file documents) pass `None` and lose only
/// that lane.
#[must_use]
pub fn completion_at(text: &str, offset: usize, doc_dir: Option<&Path>) -> Vec<CompletionItem> {
    let offset = floor_char_boundary(text, offset);
    let line = current_line(text, offset);
    let prefix = line_prefix(text, offset);

    // The agent skills list — checked FIRST among list lanes: `skills:`
    // and `tools:` share the flow-list shape, and the skills walk only
    // fires when a document directory exists to root it.
    if in_open_flow_list(text, offset, "skills:")
        || scope::in_block_list_item(text, offset, "skills:")
    {
        return doc_dir.map(skills::skill_items).unwrap_or_default();
    }

    if let Some(items) = value_lane_items(text, offset, line, prefix) {
        return items;
    }
    if is_template_tasks_ref(prefix) {
        return refs::island_task_refs(text, offset);
    }
    if let Some(items) = member_root_items(text, offset, prefix) {
        return items;
    }
    // `${{ tasks.<id>. }}` — the task's member facts (output · status ·
    // error · its named bindings), BEFORE the CEL post-dot lane: a data
    // path ends in methods, a task ref ends in members.
    if let Some(task_id) = members::template_task_member(prefix) {
        return members::task_member_items(text, &task_id);
    }
    if is_expression_post_dot(prefix) {
        return cel_methods();
    }
    if is_expression_start(prefix) {
        return expression_roots(scope::in_for_each_task(text, offset));
    }
    // A bare key inside `args:` of a KNOWN builtin — that tool's own
    // argument names, required ones floated first (catalog-derived).
    if scope::in_args_key_position(text, offset)
        && let Some(items) = builtin_arg_keys(text, offset)
    {
        return items;
    }
    // A bare `for_each:` / `when:` value — whole islands composed from
    // THIS document (typed arrays · upstream gates · the size() idiom).
    // Once an island opens, the expression lanes above take over.
    if let Some(key) = islands::island_value_key(prefix) {
        return if key == "for_each" {
            islands::for_each_items(text, offset)
        } else {
            islands::when_items(text, offset)
        };
    }
    key_lane_items(text, offset, line, prefix)
}

/// The value lanes: `model:` · `tool:` · extract `mode:` · closed enums ·
/// the two `after:` positions · the agent `tools: [ … ]` whitelist.
fn value_lane_items(
    text: &str,
    offset: usize,
    line: &str,
    prefix: &str,
) -> Option<Vec<CompletionItem>> {
    if is_model_value(prefix) {
        // A provider already typed (`model: ollama/`) narrows to ITS
        // models — the second half of the address, catalog-derived.
        if let Some(models) = provider_models(prefix) {
            return Some(models);
        }
        return Some(providers());
    }
    if is_tool_value(prefix) {
        return Some(builtin_tools());
    }
    // `mode:` inside a `nika:fetch` block — the stdlib extract vocabulary
    // (L0-derived). Contextual: a `mode:` argument of some OTHER tool
    // (an MCP tool with its own vocabulary) stays silent.
    if is_extract_mode_value(text, offset, prefix) {
        return Some(extract_mode_items());
    }
    if let Some(items) = enum_values(prefix) {
        return Some(items);
    }
    // `after:` — the control boundary's two positions: a bare KEY under
    // the block completes the LEGAL producers (everything but self and
    // its downstream closure — never offer what check refuses); the
    // VALUE after `<producer>: ` completes the closed predicate set.
    if let Some(items) = after_lane_items(text, offset, line, prefix) {
        return Some(items);
    }
    // An agent's `tools: [ … ]` whitelist — the same catalog register the
    // singular `tool:` value gets (MCP refs stay the author's to type:
    // the server has no MCP registry to speak from).
    if in_open_tools_list(text, offset) {
        return Some(builtin_tools());
    }
    None
}

/// The key lanes (checked last): top-level keys · the `schema:` JSON
/// Schema vocabulary · task-field keys + verbs.
fn key_lane_items(text: &str, offset: usize, line: &str, prefix: &str) -> Vec<CompletionItem> {
    if is_top_level_key(prefix) {
        return keyword_items(vocab::TOP_LEVEL_KEYS);
    }
    // Inside a `schema:` block the vocabulary is JSON Schema, never the
    // task fields (the 557 probe: `schema:` used to offer `depends_on`).
    // Direct children of `schema:`/`items:` get the keyset; children of
    // `properties:` stay silent — those names belong to the author.
    if scope::in_schema_key_position(text, offset) {
        return keyword_items(vocab::SCHEMA_KEYS);
    }
    if is_task_field_key(line, prefix) && !scope::in_schema_scope(text, offset) {
        if let Some(items) = block_keyset_items(text, offset) {
            return items;
        }
        let mut items = keyword_items(vocab::TASK_FIELD_KEYS);
        items.extend(verb_items());
        return items;
    }
    Vec::new()
}

/// An open island ending in a member root (`vars.` · `secrets.` ·
/// `env.` · `with.`) — `with` routes to the task-local lane, the
/// workflow-level roots to the declaration lane.
fn member_root_items(text: &str, offset: usize, prefix: &str) -> Option<Vec<CompletionItem>> {
    let root = members::template_member_root(prefix)?;
    Some(if root == "with" {
        members::with_items(text, offset)
    } else {
        members::member_items(text, root)
    })
}

/// A key position inside a KNOWN block (`retry:` · `on_error:` · a
/// verb body · `permits:` …) offers that block's OWN keyset — the
/// parser's exact vocabulary via the keysets door (born-stale: a
/// field added to the parser const appears here with zero LSP edits).
/// Free-form maps (`args:` · `with:` · `env:`) miss the door → `None`.
fn block_keyset_items(text: &str, offset: usize) -> Option<Vec<CompletionItem>> {
    let (block, parent) = scope::enclosing_block_key(text, offset)?;
    let keys = nika_schema::types::keys::known_child_keys(&block, parent.as_deref())?;
    Some(
        keys.iter()
            .map(|k| CompletionItem {
                label: format!("{k}:"),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(format!("`{block}:` field")),
                ..CompletionItem::default()
            })
            .collect(),
    )
}

/// Clamp `offset` to a valid UTF-8 char boundary ≤ `text.len()`. A request
/// offset may sit past the end, or — for a non-server caller — mid-char;
/// every slice below indexes `text` on this value, so flooring it here once
/// keeps the whole primitive panic-free (a panic in the sync serve loop
/// takes the language server down). `str::floor_char_boundary` is still
/// unstable, so it is spelled out — the same shape as `LineIndex`'s.
fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut idx = offset.min(text.len());
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// The line text containing `offset` (without the trailing newline).
fn current_line(text: &str, offset: usize) -> &str {
    let start = text[..offset].rfind('\n').map_or(0, |nl| nl + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |nl| offset + nl);
    text.get(start..end).unwrap_or("")
}

/// The text on the current line up to (and excluding) `offset`.
fn line_prefix(text: &str, offset: usize) -> &str {
    let start = text[..offset].rfind('\n').map_or(0, |nl| nl + 1);
    text.get(start..offset).unwrap_or("")
}

/// A `model:` value position — the line declares `model:` and the cursor
/// is in its value (possibly after a `<provider>/` already typed, in which
/// case we still offer providers; the client filters).
fn is_model_value(prefix: &str) -> bool {
    let trimmed = prefix.trim_start();
    let Some(rest) = trimmed.strip_prefix("model:") else {
        return false;
    };
    // Value position only — the colon must be followed by a space, so a
    // completed provider lands as `model: ollama/` (valid YAML), never glued
    // to the key as `model:ollama/`. The caret on the bare `model:` key
    // offers nothing here (key completion handles the key itself).
    let Some(after) = rest.strip_prefix(' ') else {
        return false;
    };
    !after.contains(char::is_whitespace) || after.trim().is_empty()
}

/// The `after:` completion lane — `None` when the cursor is not inside
/// an `after:` block (spec 03 §after · the CLOSED predicate set).
fn after_lane_items(
    text: &str,
    offset: usize,
    line: &str,
    prefix: &str,
) -> Option<Vec<CompletionItem>> {
    if !scope::in_task_block(text, offset, "after") {
        return None;
    }
    let head = line.trim_start();
    if head == "after:" || head.starts_with("after:") {
        return None; // the block head line itself — field lanes own it
    }
    // `<producer>: <cursor>` — the predicate position.
    if let Some((_key, rest)) = prefix.trim_start().split_once(':') {
        let _ = rest;
        return Some(
            nika_schema::types::AfterPredicate::all()
                .iter()
                .map(|p| CompletionItem {
                    label: (*p).to_owned(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some(match *p {
                        "success" => "admits on success".to_owned(),
                        "failure" => "admits on failure".to_owned(),
                        "skipped" => "admits on skipped".to_owned(),
                        _ => "admits on any settled state — incl. cancelled".to_owned(),
                    }),
                    ..CompletionItem::default()
                })
                .collect(),
        );
    }
    // bare key position — the legal producers.
    Some(task_ids(
        text,
        scope::current_task_id(text, offset).as_deref(),
    ))
}

/// Whether `offset` sits inside an unclosed agent `tools:` flow list —
/// the default-deny register position (the singular `tool:` value lane
/// serves invoke; THIS one serves the whitelist).
fn in_open_tools_list(text: &str, offset: usize) -> bool {
    in_open_flow_list(text, offset, "tools:")
}

fn in_open_flow_list(text: &str, offset: usize, key_name: &str) -> bool {
    let upto = text.get(..offset).unwrap_or("");
    let Some(key) = upto.rfind(key_name) else {
        return false;
    };
    let between = upto.get(key..).unwrap_or("");
    if between.matches('[').count() <= between.matches(']').count() {
        return false;
    }
    // The open bracket must still be THIS key's scope: an unclosed `[`
    // abandoned upstream must not capture every later position in the
    // file. Any non-blank line at indent ≤ the key's ends the block —
    // deeper continuation lines (`depends_on: [\n  a,\n  <cursor>`) pass.
    let key_line_start = upto
        .get(..key)
        .map_or(0, |s| s.rfind('\n').map_or(0, |i| i + 1));
    let key_indent = upto
        .get(key_line_start..)
        .unwrap_or("")
        .chars()
        .take_while(|c| *c == ' ')
        .count();
    for line in between.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= key_indent {
            return false;
        }
    }
    true
}

/// Whether the current-line `prefix` ends in an open `${{ … tasks.` island.
fn is_template_tasks_ref(prefix: &str) -> bool {
    let Some(island) = prefix.rfind("${{") else {
        return false;
    };
    let after = prefix.get(island..).unwrap_or("");
    !after.contains("}}") && after.trim_end().ends_with("tasks.")
}

/// The open `${{ … }}` island of `prefix`, when one exists on this line.
fn open_island(prefix: &str) -> Option<&str> {
    let island = prefix.rfind("${{")?;
    let after = prefix.get(island + 3..).unwrap_or("");
    if after.contains("}}") {
        None
    } else {
        Some(after)
    }
}

/// Post-dot INSIDE an expression, past a data path (`tasks.x.output.` ·
/// `vars.name.`) — the CEL method position. The bare `tasks.` island is
/// task-id territory (checked before this) so ids win there.
fn is_expression_post_dot(prefix: &str) -> bool {
    let Some(after) = open_island(prefix) else {
        return false;
    };
    let t = after.trim_end();
    // A dot that FOLLOWS at least one path segment (`root.seg…`) — one
    // trailing dot with no prior dot is the root's own member position.
    t.ends_with('.') && t.trim_end_matches('.').contains('.')
}

/// The very start of an expression island (`${{ ` · or a bare partial
/// identifier with no dot yet) — the roots + free-functions position.
fn is_expression_start(prefix: &str) -> bool {
    let Some(after) = open_island(prefix) else {
        return false;
    };
    let t = after.trim_start();
    t.is_empty() || (!t.contains('.') && t.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// The cel-subset/0.1 METHOD set — mirrors `nika-cel`'s parser arity
/// table (the vocabulary is CLOSED there: an unknown method is a static
/// error, so this list cannot silently drift wider than the engine).
fn cel_methods() -> Vec<CompletionItem> {
    const METHODS: &[(&str, &str, &str)] = &[
        (
            "size()",
            "size",
            "length of a string · list · map (cel-subset/0.1)",
        ),
        (
            "contains(…)",
            "contains",
            "substring test on a string (cel-subset/0.1)",
        ),
        (
            "startsWith(…)",
            "startsWith",
            "prefix test on a string (cel-subset/0.1)",
        ),
        (
            "endsWith(…)",
            "endsWith",
            "suffix test on a string (cel-subset/0.1)",
        ),
    ];
    METHODS
        .iter()
        .map(|(label, insert, doc)| CompletionItem {
            label: (*label).to_owned(),
            insert_text: Some((*insert).to_owned()),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some((*doc).to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

/// Expression ROOTS (the four value authorities + the runtime
/// functions the parser accepts (`size` · `has` — a closed set there too).
/// The loop-scoped pair (`item` · `index` — spec 04 §loop-scoped locals)
/// appears ONLY when the enclosing task declares `for_each:` (#574):
/// offering `item` outside a fan-out completes a reference the run
/// cannot resolve — never offer what check refuses, island edition.
fn expression_roots(in_for_each: bool) -> Vec<CompletionItem> {
    const ROOTS: &[(&str, &str)] = &[
        ("tasks", "an upstream task's output (`tasks.<id>.output`)"),
        ("inputs", "a declared typed workflow input"),
        ("const", "a declared named constant"),
        ("secrets", "a declared secret (never echoed)"),
        ("with", "this task's own `with:` aliases"),
    ];
    const LOOP_ROOTS: &[(&str, &str)] = &[
        ("item", "the current `for_each` element (loop-scoped)"),
        ("index", "the element's 0-based position (loop-scoped)"),
    ];
    let mut items: Vec<CompletionItem> = ROOTS
        .iter()
        .chain(if in_for_each { LOOP_ROOTS } else { &[] })
        .map(|(name, doc)| CompletionItem {
            label: (*name).to_owned(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some((*doc).to_owned()),
            ..CompletionItem::default()
        })
        .collect();
    for (label, insert, doc) in [
        (
            "size(…)",
            "size",
            "length of a string · list · map (free form)",
        ),
        (
            "has(…)",
            "has",
            "presence test — true when the path resolves",
        ),
    ] {
        items.push(CompletionItem {
            label: label.to_owned(),
            insert_text: Some(insert.to_owned()),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(doc.to_owned()),
            ..CompletionItem::default()
        });
    }
    items
}

/// A top-level key position — column 0 (no indentation) and the prefix is
/// a bare key fragment (no `:` yet, or an empty line).
fn is_top_level_key(prefix: &str) -> bool {
    if prefix.starts_with(char::is_whitespace) {
        return false; // indented → not top-level
    }
    // either an empty line or a bare identifier being typed before any `:`
    let typed = prefix.trim_end();
    !typed.contains(':') && typed.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// A task-field key position — indented, sits at a key fragment, and is not
/// already in a value (no `:` typed on this line yet), and not a list-only
/// continuation. The list-marker `- ` introducing `id:` is also a field
/// position.
fn is_task_field_key(line: &str, prefix: &str) -> bool {
    if !prefix.starts_with(char::is_whitespace) {
        return false; // top-level handled elsewhere
    }
    // strip leading whitespace and an optional `- ` list marker
    let body = prefix.trim_start();
    let body = body.strip_prefix("- ").unwrap_or(body);
    // a key fragment: identifier chars only, no `:` yet
    !body.contains(':') && body.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
        // guard against the indented inside-of-a-flow-map value position
        && !line.trim_start().starts_with('}')
}

/// Provider completion items (`<provider>/`), local-first per vocab order.
/// `tool: ` value position — same grammar as the model detector (the
/// colon-space law · one leading token only).
fn is_tool_value(prefix: &str) -> bool {
    let trimmed = prefix.trim_start();
    let Some(rest) = trimmed.strip_prefix("tool:") else {
        return false;
    };
    let Some(after) = rest.strip_prefix(' ') else {
        return false;
    };
    let after = after.trim_start_matches(['"', '\'']);
    !after.contains(char::is_whitespace) || after.trim().is_empty()
}

/// The canonical builtin set (`nika:*`) — DERIVED from the catalog at
/// call time (born-stale law: the 27 never live in a hand table here).
fn builtin_tools() -> Vec<CompletionItem> {
    nika_catalog::all_builtins()
        .iter()
        .map(|b| CompletionItem {
            label: format!("nika:{}", b.name),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some(format!(
                "{:?} · args: {}",
                b.category,
                if b.args.is_empty() {
                    "(none)".to_owned()
                } else {
                    b.args.join(" · ")
                }
            )),
            ..CompletionItem::default()
        })
        .collect()
}

/// `model: <provider>/` typed → that provider's catalog models (id ·
/// wire name · context window as the detail facts).
fn provider_models(prefix: &str) -> Option<Vec<CompletionItem>> {
    let trimmed = prefix.trim_start();
    let after = trimmed.strip_prefix("model:")?.strip_prefix(' ')?;
    let (prov, _partial) = after.split_once('/')?;
    let provider = nika_catalog::all_providers()
        .iter()
        .find(|p| p.id == prov)?;
    let items: Vec<CompletionItem> = provider
        .models
        .iter()
        .map(|m| CompletionItem {
            label: format!("{prov}/{}", m.model),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some(format!(
                "{} · {}k ctx · {}k out",
                m.id,
                m.context_window_tokens / 1000,
                m.max_output_tokens / 1000
            )),
            ..CompletionItem::default()
        })
        .collect();
    (!items.is_empty()).then_some(items)
}

/// Closed-enum field values (`nika:` · `capture:` · `backoff_strategy:` ·
/// `type:`) — the schema's own vocabulary, offered where the spec closes
/// the set. Output-side enums (finding severity · permits source) are
/// engine-stamped, never authored — they do not belong here.
fn enum_values(prefix: &str) -> Option<Vec<CompletionItem>> {
    const ENUMS: &[(&str, &[(&str, &str)])] = &[
        (
            "nika:",
            &[(
                "v1",
                "the envelope — a single version marker, frozen forever",
            )],
        ),
        (
            "capture:",
            &[
                ("text", "stdout as one trimmed string (the default)"),
                ("structured", ".output = { stdout, stderr, exit_code }"),
            ],
        ),
        (
            "backoff_strategy:",
            &[
                ("exponential", "1s · 2s · 4s … (the retry default)"),
                ("linear", "1s · 2s · 3s … steady climb"),
                ("fixed", "the same delay every attempt"),
            ],
        ),
        // `vars:` declarations (spec 01-envelope §vars) — the same six
        // words double as the JSON-Schema `type:` vocabulary inside
        // `schema:` blocks, so one lane serves both authoring sites.
        (
            "type:",
            &[
                ("string", "a UTF-8 string"),
                ("number", "any JSON number"),
                ("integer", "a JSON integer"),
                ("boolean", "true or false"),
                ("array", "a JSON array"),
                ("object", "a JSON object"),
            ],
        ),
    ];
    let trimmed = prefix.trim_start();
    for (key, values) in ENUMS {
        if let Some(rest) = trimmed.strip_prefix(key)
            && let Some(after) = rest.strip_prefix(' ')
            && (!after.contains(char::is_whitespace) || after.trim().is_empty())
        {
            return Some(
                values
                    .iter()
                    .map(|(v, doc)| CompletionItem {
                        label: (*v).to_owned(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some((*doc).to_owned()),
                        ..CompletionItem::default()
                    })
                    .collect(),
            );
        }
    }
    None
}

/// `mode: ` inside a `nika:fetch` invoke block — one token typed at most.
fn is_extract_mode_value(text: &str, offset: usize, prefix: &str) -> bool {
    let trimmed = prefix.trim_start();
    let Some(rest) = trimmed.strip_prefix("mode:") else {
        return false;
    };
    let Some(after) = rest.strip_prefix(' ') else {
        return false;
    };
    if after.contains(char::is_whitespace) && !after.trim().is_empty() {
        return false;
    }
    scope::enclosing_tool(text, offset).as_deref() == Some("nika:fetch")
}

/// The stdlib extract-mode vocabulary — the SET is `ExtractMode::ALL`
/// (nika-types L0 · closed at stdlib v0.1), so a mode added there
/// appears here with zero LSP edits; the one-line prose is local.
fn extract_mode_items() -> Vec<CompletionItem> {
    use nika_types::ExtractMode as M;
    fn doc(m: M) -> &'static str {
        match m {
            M::Markdown => "HTML → cleaned Markdown (the content default)",
            M::Article => "readability article body → Markdown",
            M::Text => "tags stripped · plain text",
            M::Selector => "raw HTML of the `selector:` matches",
            M::Jq => "JSON body · a `jq:` expression applied",
            M::Metadata => "meta tags · OpenGraph · canonical · lang",
            M::Links => "every <a href> as an absolute URL",
            M::Feed => "RSS · Atom · JSON Feed → normalized object",
            M::Sitemap => "sitemap.xml → URL entries",
            _ => "the decoded body verbatim",
        }
    }
    M::ALL
        .iter()
        .map(|m| CompletionItem {
            label: m.as_str().to_owned(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some(doc(*m).to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

/// The argument names of the enclosing block's builtin — catalog-derived
/// (born-stale law), required ones sorted first and marked. `None` when
/// the enclosing `tool:` is absent, unknown, or not a `nika:*` builtin
/// (an MCP tool's args are its own business — silence beats noise).
fn builtin_arg_keys(text: &str, offset: usize) -> Option<Vec<CompletionItem>> {
    let tool = scope::enclosing_tool(text, offset)?;
    let short = tool.strip_prefix("nika:")?;
    let b = nika_catalog::all_builtins()
        .iter()
        .find(|b| b.name == short)?;
    Some(
        b.args
            .iter()
            .map(|arg| {
                let required = b.required.contains(arg);
                CompletionItem {
                    label: format!("{arg}:"),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(if required {
                        format!("required · {tool} argument")
                    } else {
                        format!("{tool} argument")
                    }),
                    sort_text: Some(format!("{}{arg}", if required { '0' } else { '1' })),
                    ..CompletionItem::default()
                }
            })
            .collect(),
    )
}

fn providers() -> Vec<CompletionItem> {
    vocab::PROVIDERS
        .iter()
        .map(|p| CompletionItem {
            label: format!("{}/", p.name),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some(p.doc.to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

/// The 4 verbs as completion items.
fn verb_items() -> Vec<CompletionItem> {
    vocab::VERBS
        .iter()
        .map(|v| CompletionItem {
            label: v.name.to_owned(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(v.doc.to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

/// Keyword (envelope / task-field) completion items.
fn keyword_items(table: &[Entry]) -> Vec<CompletionItem> {
    table
        .iter()
        .map(|e| CompletionItem {
            label: e.name.to_owned(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(e.doc.to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

/// The workflow's own task ids, as completion items (for `depends_on:` /
/// `${{ tasks.X }}`).
///
/// A partially-typed document is the COMMON completion case (`depends_on:
/// [` with nothing after it does not parse as YAML), so ids are read from
/// a robust line scan for `id:` declarations rather than a full parse —
/// the parse path would yield nothing exactly when completion is needed.
pub(super) fn task_ids(text: &str, exclude: Option<&str>) -> Vec<CompletionItem> {
    // Prefer the parser's authoritative ids (with verb detail) when the
    // document parses; fall back to the line scan otherwise. The
    // exclusion is the editing task's whole ILLEGAL set — itself plus
    // its downstream closure: a reference downstream is a cycle (a
    // template ref is an implicit edge) or a recover deadlock (DAG-004),
    // and completion never offers what check refuses. The scan fallback
    // (no graph) honestly degrades to excluding self alone.
    if let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient)
        && !wf.tasks.is_empty()
    {
        let illegal: std::collections::BTreeSet<&str> = exclude
            .map_or_else(std::collections::BTreeSet::new, |id| {
                super::graph::illegal_reference_targets(&wf, id)
            });
        return wf
            .tasks
            .iter()
            .filter(|t| !illegal.contains(&t.value.id.value.as_str()))
            .map(|t| CompletionItem {
                label: t.value.id.value.clone(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some(format!("task ({})", t.value.action.verb())),
                ..CompletionItem::default()
            })
            .collect();
    }
    scan_task_ids(text)
        .into_iter()
        .filter(|id| Some(id.as_str()) != exclude)
        .map(|id| CompletionItem {
            label: id,
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("task".to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

/// Scan `text` for `id:` declarations under `tasks:`, returning each task
/// id in source order (deduplicated). Robust to a partially-typed document
/// that does not yet parse. A line is a task id when, after stripping
/// leading whitespace and an optional `- ` marker, it reads `id: <ident>`.
pub(super) fn scan_task_ids(text: &str) -> Vec<String> {
    // W1 « the map »: a task declares as a bare `name:` key at indent 2
    // inside the top-level `tasks:` section — the same boundary shape
    // scope.rs walks (the `- id:` row died with the list form).
    let mut ids = Vec::new();
    let mut in_tasks = false;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.starts_with('#') && line.contains(':') {
            in_tasks = line.starts_with("tasks:");
            continue;
        }
        if !in_tasks {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent != 2 {
            continue;
        }
        let head = line.trim_start().split('#').next().unwrap_or("").trim_end();
        let Some(name) = head.strip_suffix(':') else {
            continue;
        };
        let ok = !name.is_empty()
            && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if ok && !ids.iter().any(|i| i == name) {
            ids.push(name.to_owned());
        }
    }
    ids
}

#[cfg(test)]
mod tests;
