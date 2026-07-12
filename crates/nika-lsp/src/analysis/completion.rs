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

use lsp_types::{CompletionItem, CompletionItemKind};
use nika_schema::{FileId, ParseMode, parse};

use super::members;
use super::scope;
use super::vocab::{self, Entry};

/// Compute completion items for the cursor at `offset` in `text`.
#[must_use]
pub fn completion(text: &str, offset: usize) -> Vec<CompletionItem> {
    let offset = floor_char_boundary(text, offset);
    let line = current_line(text, offset);
    let prefix = line_prefix(text, offset);

    if is_model_value(prefix) {
        // A provider already typed (`model: ollama/`) narrows to ITS
        // models — the second half of the address, catalog-derived.
        if let Some(models) = provider_models(prefix) {
            return models;
        }
        return providers();
    }
    if is_tool_value(prefix) {
        return builtin_tools();
    }
    // `mode:` inside a `nika:fetch` block — the stdlib extract vocabulary
    // (L0-derived). Contextual: a `mode:` argument of some OTHER tool
    // (an MCP tool with its own vocabulary) stays silent.
    if is_extract_mode_value(text, offset, prefix) {
        return extract_mode_items();
    }
    if let Some(items) = enum_values(prefix) {
        return items;
    }
    if in_open_depends_on(text, offset) || is_template_tasks_ref(prefix) {
        return task_ids(text, scope::current_task_id(text, offset).as_deref());
    }
    if let Some(root) = members::template_member_root(prefix) {
        return members::member_items(text, root);
    }
    if is_expression_post_dot(prefix) {
        return cel_methods();
    }
    if is_expression_start(prefix) {
        return expression_roots();
    }
    // A bare key inside `args:` of a KNOWN builtin — that tool's own
    // argument names, required ones floated first (catalog-derived).
    if scope::in_args_key_position(text, offset)
        && let Some(items) = builtin_arg_keys(text, offset)
    {
        return items;
    }
    if is_top_level_key(prefix) {
        return keyword_items(vocab::TOP_LEVEL_KEYS);
    }
    if is_task_field_key(line, prefix) {
        let mut items = keyword_items(vocab::TASK_FIELD_KEYS);
        items.extend(verb_items());
        return items;
    }
    Vec::new()
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

/// Whether `offset` sits inside an unclosed `depends_on:` flow list — works
/// ACROSS line breaks (`depends_on: [\n  a,\n  <cursor>`), not only the
/// opening line. The nearest `depends_on:` before the cursor must have an
/// unbalanced `[` up to the cursor.
fn in_open_depends_on(text: &str, offset: usize) -> bool {
    let upto = text.get(..offset).unwrap_or("");
    let Some(key) = upto.rfind("depends_on:") else {
        return false;
    };
    let between = upto.get(key..).unwrap_or("");
    between.matches('[').count() > between.matches(']').count()
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

/// Expression ROOTS (the five locked namespaces · D-N11) + the two free
/// functions the parser accepts (`size` · `has` — a closed set there too).
fn expression_roots() -> Vec<CompletionItem> {
    const ROOTS: &[(&str, &str)] = &[
        ("tasks", "an upstream task's output (`tasks.<id>.output`)"),
        ("vars", "a declared workflow var"),
        ("env", "an allowed environment value"),
        ("secrets", "a declared secret (never echoed)"),
        ("with", "this task's own `with:` aliases"),
        ("item", "the current `for_each` element"),
    ];
    let mut items: Vec<CompletionItem> = ROOTS
        .iter()
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
fn task_ids(text: &str, exclude: Option<&str>) -> Vec<CompletionItem> {
    // Prefer the parser's authoritative ids (with verb detail) when the
    // document parses; fall back to the line scan otherwise. The task
    // being edited is EXCLUDED — a self-dependency is a cycle the check
    // would refuse, so offering it teaches an error.
    if let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient)
        && !wf.tasks.is_empty()
    {
        return wf
            .tasks
            .iter()
            .filter(|t| Some(t.value.id.value.as_str()) != exclude)
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
fn scan_task_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        let body = line.trim_start();
        let body = body.strip_prefix("- ").unwrap_or(body);
        let Some(rest) = body.strip_prefix("id:") else {
            continue;
        };
        let id: String = rest
            .trim()
            .chars()
            .take_while(|c| *c == '_' || c.is_ascii_alphanumeric())
            .collect();
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[CompletionItem]) -> Vec<String> {
        items.iter().map(|i| i.label.clone()).collect()
    }

    fn kinds(items: &[CompletionItem]) -> Vec<CompletionItemKind> {
        items.iter().filter_map(|i| i.kind).collect()
    }

    #[test]
    fn current_line_returns_the_exact_line_around_the_offset() {
        // The line slice must be EXACTLY the current line (no shift): for an
        // offset on the third line, `current_line` is that whole line with no
        // newline. A replaced/empty/shifted body (`""`, `"xyzzy"`, `nl-1`,
        // `offset-nl`) would return a different slice.
        let text = "aa\nbbbb\ncccccc";
        // offset 5 is inside "bbbb" (line 1, starts at byte 3, ends at 7).
        assert_eq!(current_line(text, 5), "bbbb", "the whole middle line");
        // offset 0 → first line, offset at end → last line.
        assert_eq!(current_line(text, 0), "aa", "first line");
        assert_eq!(current_line(text, text.len()), "cccccc", "last line");
        // a one-char shift in either boundary would drop a char or include a
        // newline — assert the precise length too.
        assert_eq!(current_line(text, 5).len(), 4, "no off-by-one in bounds");
    }

    #[test]
    fn flow_map_close_line_suppresses_task_fields() {
        // The cursor sits at the indent of a line whose content begins with
        // `}` (a flow-map close). `is_task_field_key`'s guard reads the WHOLE
        // current line via `current_line`; if that returns "" / "xyzzy" /
        // a shifted slice, the `}` is missed and fields are wrongly offered.
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    } x";
        let cursor = text.rfind('}').expect("brace"); // caret at the indent
        let items = completion(text, cursor);
        assert!(
            items.is_empty(),
            "a `}}`-leading line is not a field position: {:?}",
            labels(&items)
        );
        // CONTRAST: a normal indented ident fragment DOES offer fields — so
        // the suppression above is the `}` guard, not a blanket empty.
        let normal = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    de";
        assert!(
            !completion(normal, normal.len()).is_empty(),
            "a normal field fragment still completes"
        );
    }

    /// The space trigger's cost contract — ` ` is a trigger character
    /// (capabilities.rs), so a space anywhere in running prose fires a
    /// request; this pins that a non-value space answers EMPTY, keeping
    /// the trigger free inside prompt blocks and plain text.
    #[test]
    fn a_space_in_running_prose_offers_nothing() {
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    infer:\n      prompt: |\n        Summarize the following ";
        assert!(completion(text, text.len()).is_empty());
    }

    /// `tool: ` offers the full catalog-derived `nika:*` set — and the
    /// count IS the catalog's (the born-stale gate: a builtin added to
    /// the catalog appears here with zero LSP edits).
    #[test]
    fn tool_value_offers_exactly_the_builtin_set() {
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    invoke:\n      tool: ";
        let items = completion(text, text.len());
        assert_eq!(items.len(), nika_catalog::all_builtins().len());
        let got = labels(&items);
        assert!(got.iter().all(|l| l.starts_with("nika:")), "{got:?}");
        assert!(got.contains(&"nika:read".to_owned()));
        assert!(got.contains(&"nika:jq".to_owned()));
        // a partial token keeps offering (client-side filtering)
        let text2 = format!("{text}nika:re");
        assert!(!completion(&text2, text2.len()).is_empty());
        // quoted form too — `tool: "nika:` is the corpus's own style
        let text3 = format!("{text}\"nika:");
        assert!(!completion(&text3, text3.len()).is_empty());
    }

    /// `model: ollama/` narrows to ollama's OWN catalog models; an
    /// unknown provider falls back to the provider list (never empty).
    #[test]
    fn provider_slash_offers_that_providers_models() {
        let text = "nika: v1\nmodel: ollama/";
        let items = completion(text, text.len());
        assert!(!items.is_empty(), "ollama has catalog models");
        let got = labels(&items);
        assert!(got.iter().all(|l| l.starts_with("ollama/")), "{got:?}");

        let text2 = "nika: v1\nmodel: nosuch/";
        let fallback = completion(text2, text2.len());
        let fb = labels(&fallback);
        assert!(
            fb.iter().any(|l| l == "ollama/"),
            "unknown provider → provider list: {fb:?}"
        );
    }

    /// Closed-enum fields offer exactly the spec's vocabulary.
    #[test]
    fn enum_fields_offer_exactly_the_closed_sets() {
        let text = "nika: v1\ntasks:\n  - id: a\n    exec:\n      command: x\n      capture: ";
        let labels_ = labels(&completion(text, text.len()));
        assert_eq!(labels_, vec!["text".to_owned(), "structured".to_owned()]);

        let text2 = "nika: v1\ntasks:\n  - id: a\n    retry:\n      backoff_strategy: ";
        let l2 = labels(&completion(text2, text2.len()));
        assert_eq!(
            l2,
            vec![
                "exponential".to_owned(),
                "linear".to_owned(),
                "fixed".to_owned()
            ]
        );

        // the envelope value — the FIRST thing every author types
        let text3 = "nika: ";
        let l3 = labels(&completion(text3, text3.len()));
        assert_eq!(l3, vec!["v1".to_owned()]);

        // vars declaration types — and the same lane serves JSON-Schema
        // `type:` lines inside `schema:` blocks
        let text4 = "nika: v1\nvars:\n  city:\n    type: ";
        let l4 = labels(&completion(text4, text4.len()));
        assert_eq!(
            l4,
            vec![
                "string".to_owned(),
                "number".to_owned(),
                "integer".to_owned(),
                "boolean".to_owned(),
                "array".to_owned(),
                "object".to_owned()
            ]
        );
    }

    #[test]
    fn model_value_offers_exactly_the_provider_set() {
        let text = "nika: v1\nworkflow: w\nmodel: ";
        let items = completion(text, text.len());
        // EXACT label set: every provider with a trailing `/`, in canon
        // (local-first) order — not the keyword/field/task sets.
        assert_eq!(
            labels(&items),
            vec![
                "ollama/",
                "lmstudio/",
                "llamacpp/",
                "localai/",
                "vllm/",
                "mistral/",
                "anthropic/",
                "openai/",
                "google/",
                "deepseek/",
                "groq/",
                "xai/",
                "openrouter/",
                "huggingface/",
                "nvidia/",
                "mock/",
            ],
            "the 16-provider catalog, local-first, each suffixed with `/`"
        );
        // every provider item is a VALUE with a non-empty detail (the kind +
        // detail fields, not just the label).
        assert!(
            items
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "providers are VALUE-kinded"
        );
        assert!(
            items
                .iter()
                .all(|i| i.detail.as_deref().is_some_and(|d| !d.is_empty())),
            "every provider carries its doc as detail"
        );
        // the ollama detail is the exact vocab doc (pins the detail field).
        assert_eq!(
            items[0].detail.as_deref(),
            Some("Local models via Ollama (sovereign, open-weight)."),
            "ollama detail is the vocab doc"
        );
    }

    #[test]
    fn model_value_with_a_partial_provider_still_offers_providers() {
        // `model: ollama` (a bare provider, no whitespace after) is still a
        // value position — providers are offered (the client filters). The
        // `||` in is_model_value is load-bearing: `!contains(ws)` is true
        // here so the OR short-circuits to true.
        let text = "nika: v1\nmodel: ollama";
        let items = completion(text, text.len());
        assert_eq!(
            labels(&items).first().map(String::as_str),
            Some("ollama/"),
            "a typed-but-incomplete provider value still offers the catalog"
        );
    }

    #[test]
    fn model_value_after_a_space_separated_token_offers_nothing() {
        // `model: ollama foo` has whitespace WITHIN the value — no longer a
        // single value position. `!after.contains(ws)` is false and
        // `after.trim().is_empty()` is false, so `is_model_value` is false.
        // (Deleting the `!` or flipping `||`→`&&` would mis-classify this.)
        let text = "nika: v1\nmodel: ollama foo";
        let items = completion(text, text.len());
        assert!(
            !labels(&items).iter().any(|l| l.ends_with('/')),
            "a space-separated trailing token is not a provider value: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn depends_on_offers_exactly_the_task_ids() {
        // The partially-typed `depends_on: [` does not parse → the scan path
        // yields the ids with detail "task" and VARIABLE kind. EXACT set.
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: \"x\" }\n  - id: save\n    depends_on: [";
        let items = completion(text, text.len());
        assert_eq!(
            labels(&items),
            vec!["extract"],
            "exactly the OTHER task ids — `save` is the task being edited, \
             and a self-dependency is a cycle the check refuses"
        );
        assert!(
            items
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VARIABLE)),
            "task refs are VARIABLE-kinded"
        );
        assert!(
            items.iter().all(|i| i.detail.as_deref() == Some("task")),
            "scan-path detail is the bare `task` label"
        );
    }

    #[test]
    fn depends_on_in_a_parseable_doc_uses_the_verb_detail() {
        // A FULLY VALID document with the cursor inside an open `[`: the
        // parse path wins, so each id carries its verb in the detail
        // (`task (exec)`) — the `!wf.tasks.is_empty()` guard + the label/
        // kind/detail fields of the parse-path CompletionItem.
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: \"x\" }\n  - id: save\n    depends_on: [extract]\n    exec: { command: \"y\" }\n";
        let cursor = text
            .find("depends_on: [")
            .map(|p| p + "depends_on: [".len())
            .expect("open bracket");
        let items = completion(text, cursor);
        assert_eq!(
            labels(&items),
            vec!["extract"],
            "the other task ids (self excluded)"
        );
        assert!(
            items
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VARIABLE)),
            "VARIABLE-kinded"
        );
        assert_eq!(
            items.iter().map(|i| i.detail.clone()).collect::<Vec<_>>(),
            vec![Some("task (exec)".to_owned())],
            "parse path carries the verb in the detail, NOT the bare `task`"
        );
    }

    #[test]
    fn closed_depends_on_after_the_bracket_does_not_offer_task_ids() {
        // The `[` is BALANCED by `]` — the cursor sits after a fully closed
        // depends_on. `in_open_depends_on` must be FALSE (counts equal, not
        // `>=`), so no task-id completion fires here.
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: \"x\" }\n  - id: save\n    depends_on: [extract] ";
        let items = completion(text, text.len());
        assert!(
            !labels(&items).iter().any(|l| l == "extract" || l == "save"),
            "a closed depends_on offers no task ids: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn depends_on_offers_task_ids_across_lines() {
        // a multi-line flow list: the cursor is on a continuation line, the
        // `depends_on:` key + unclosed `[` are on PREVIOUS lines.
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: \"x\" }\n  - id: save\n    depends_on: [\n      extract,\n      ";
        let items = completion(text, text.len());
        let labels = labels(&items);
        assert!(labels.contains(&"extract".to_owned()), "{labels:?}");
        assert!(
            !labels.contains(&"save".to_owned()),
            "the edited task never offers itself: {labels:?}"
        );
    }

    #[test]
    fn model_key_without_space_offers_envelope_keys_not_providers() {
        // caret right after `model:` (no space) — this is the KEY, not the
        // value. Offering `ollama/` here would glue to `model:ollama/`.
        let text = "nika: v1\nmodel:";
        let items = completion(text, text.len());
        let labels = labels(&items);
        assert!(
            !labels.iter().any(|l| l.ends_with('/')),
            "no provider values at the bare key: {labels:?}"
        );
    }

    #[test]
    fn template_tasks_dot_offers_task_ids() {
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    infer: { prompt: \"hi\", max_tokens: 5 }\n  - id: use\n    exec: { command: \"echo ${{ tasks.";
        let items = completion(text, text.len());
        assert_eq!(
            labels(&items),
            vec!["extract"],
            "the OTHER task ids — `use` is the task being edited"
        );
        assert!(
            items
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VARIABLE)),
            "task refs are VARIABLE-kinded"
        );
    }

    #[test]
    fn template_island_without_tasks_dot_offers_nothing() {
        // An open `${{ ` island that does NOT end with `tasks.` is not a task
        // reference. `is_template_tasks_ref` requires BOTH `!contains("}}")`
        // AND `ends_with("tasks.")` — flipping the `&&` to `||` would fire on
        // any open island.
        let text =
            "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: \"echo ${{ vars.";
        let items = completion(text, text.len());
        assert!(
            !labels(&items).iter().any(|l| l == "extract"),
            "an `${{{{ vars.` island is not a tasks ref: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn top_level_offers_exactly_the_envelope_keys() {
        let text = "nika: v1\nwo";
        let items = completion(text, text.len());
        // EXACT envelope-key set (the vocab order), PROPERTY-kinded, no verbs.
        assert_eq!(
            labels(&items),
            vec![
                "nika",
                "workflow",
                "description",
                "model",
                "vars",
                "env",
                "secrets",
                "permits",
                "tasks",
                "outputs",
            ],
            "the 10 top-level envelope keys, in spec order"
        );
        assert!(
            items
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::PROPERTY)),
            "envelope keys are PROPERTY-kinded"
        );
        // verbs are NOT top-level keys
        assert!(!labels(&items).contains(&"infer".to_owned()));
    }

    #[test]
    fn top_level_non_identifier_prefix_offers_nothing() {
        // A column-0 line with a space in the typed fragment (`wo rk`) is NOT
        // a bare key fragment — `is_top_level_key`'s `all(ident)` is false,
        // so no envelope keys fire. (Flipping `&&`→`||` would still offer.)
        let text = "nika: v1\nwo rk";
        let items = completion(text, text.len());
        assert!(
            items.is_empty(),
            "a non-identifier top-level prefix offers nothing: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn top_level_after_a_colon_offers_no_keys() {
        // Once the line has a `:` it is a VALUE position, not a key position.
        // `is_top_level_key`'s `!typed.contains(':')` must hold. The `==` in
        // the char predicate (`c == '_'`) and the `&&` both matter here.
        let text = "nika: v1\nworkflow: w";
        let items = completion(text, text.len());
        assert!(
            !labels(&items).contains(&"workflow".to_owned()),
            "a `key: value` line is not a key position: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn task_field_offers_exactly_the_fields_and_verbs() {
        // indented key position inside a task: the 12 task fields followed by
        // the 4 verbs, in that order. EXACT set + kinds (fields PROPERTY,
        // verbs KEYWORD).
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    de";
        let items = completion(text, text.len());
        assert_eq!(
            labels(&items),
            vec![
                // the 12 task fields (vocab order)
                "id",
                "depends_on",
                "when",
                "for_each",
                "max_parallel",
                "fail_fast",
                "retry",
                "on_error",
                "timeout",
                "with",
                "output",
                "on_finally",
                // then the 4 verbs
                "infer",
                "exec",
                "invoke",
                "agent",
            ],
            "task fields then the 4 verbs"
        );
        // the first 12 are PROPERTY (fields), the last 4 are KEYWORD (verbs).
        let ks = kinds(&items);
        assert_eq!(ks.len(), 16, "16 items, all kinded");
        assert!(
            ks[..12].iter().all(|k| *k == CompletionItemKind::PROPERTY),
            "fields are PROPERTY-kinded"
        );
        assert!(
            ks[12..].iter().all(|k| *k == CompletionItemKind::KEYWORD),
            "verbs are KEYWORD-kinded"
        );
        // the verb items carry their doc as detail (the detail field).
        let infer = items
            .iter()
            .find(|i| i.label == "infer")
            .expect("infer item");
        assert_eq!(
            infer.detail.as_deref(),
            Some(
                "A single LLM call. Sends one prompt, returns one response \
                 (optionally structured against a `schema:`)."
            ),
            "the infer verb's detail is its vocab doc"
        );
    }

    #[test]
    fn task_field_inside_a_flow_map_value_offers_nothing() {
        // An indented line that has already opened a flow-map value (`{`)
        // and contains a `:` is NOT a field position. `is_task_field_key`
        // requires `!body.contains(':')`; the closing-`}` guard and the
        // ident predicate also gate it.
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command";
        let items = completion(text, text.len());
        // `command` has no colon yet, BUT the line started a flow map — it is
        // still a bare ident fragment, so fields ARE offered here. Assert the
        // CONVERSE boundary: once a `:` appears, nothing fires.
        let after_colon = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: x";
        let none = completion(after_colon, after_colon.len());
        assert!(
            none.is_empty(),
            "an indented `key: value` line offers nothing: {:?}",
            labels(&none)
        );
        // sanity: the flow-map-open case still parses as a field fragment.
        let _ = items;
    }

    #[test]
    fn task_field_fragment_with_underscore_still_completes() {
        // The field fragment `for_` contains an underscore. `is_task_field_
        // key`'s char predicate `c == '_' || alnum` must accept it (flipping
        // `==` to `!=` would reject the `_` and suppress field completion).
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    for_";
        let items = completion(text, text.len());
        assert!(
            labels(&items).contains(&"for_each".to_owned()),
            "an underscore-bearing field fragment still completes: {:?}",
            labels(&items)
        );
        assert!(
            labels(&items).contains(&"on_error".to_owned()),
            "and the rest"
        );
    }

    /// B6 · the CEL method position: a dot PAST a data path inside an
    /// open island completes the closed cel-subset/0.1 method set — and
    /// the bare `tasks.` island stays task-id territory (checked first).
    #[test]
    fn expression_post_dot_completes_cel_methods() {
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    infer: { prompt: \"x\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.output.";
        let items = completion(text, text.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"size()"),
            "cel methods at post-path dot: {labels:?}"
        );
        assert!(labels.contains(&"startsWith(…)"));
        assert_eq!(
            items.len(),
            4,
            "the method set is CLOSED (parser arity table)"
        );

        // Bare `tasks.` still resolves to task IDS, never methods.
        let bare = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    infer: { prompt: \"x\" }\n  - id: b\n    exec: { command: \"echo ${{ tasks.";
        let ids: Vec<String> = completion(bare, bare.len())
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(
            ids.contains(&"a".to_owned()),
            "task ids win on the bare root: {ids:?}"
        );
    }

    /// B6 · the island start completes the five locked roots + item +
    /// the two free functions — and outside any island, nothing changes.
    #[test]
    fn expression_start_completes_roots_and_free_functions() {
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    infer: { prompt: \"${{ ";
        let labels: Vec<String> = completion(text, text.len())
            .into_iter()
            .map(|i| i.label)
            .collect();
        for root in ["tasks", "vars", "env", "secrets", "with", "item"] {
            assert!(
                labels.contains(&root.to_owned()),
                "missing root {root}: {labels:?}"
            );
        }
        assert!(labels.contains(&"has(…)".to_owned()));

        // A closed island offers nothing from the expression vocab.
        let closed =
            "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    infer: { prompt: \"${{ vars.x }} and ";
        let after: Vec<String> = completion(closed, closed.len())
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(
            !after.contains(&"has(…)".to_owned()),
            "closed island leaks: {after:?}"
        );
    }

    #[test]
    fn keyword_items_carry_their_doc_as_detail() {
        // The envelope/task-field items built by `keyword_items` carry the
        // vocab doc in their `detail` field (deleting it would default to
        // None). Assert the exact detail on a known envelope key.
        let text = "nika: v1\nwo";
        let items = completion(text, text.len());
        let tasks = items
            .iter()
            .find(|i| i.label == "tasks")
            .expect("tasks key item");
        assert_eq!(
            tasks.detail.as_deref(),
            Some(
                "The task DAG. Required, non-empty. Each task runs exactly \
                 one verb."
            ),
            "the envelope key carries its vocab doc as detail"
        );
        assert!(
            items.iter().all(|i| i.detail.is_some()),
            "every keyword item has a detail"
        );
    }

    #[test]
    fn scan_task_ids_reads_underscored_ids_in_full() {
        // The scan-path take_while keeps `_` and alnum. An id with an
        // underscore (`my_task`) must be read in FULL (flipping the `==` in
        // the take_while predicate would stop at the `_`, truncating to `my`).
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: my_task\n    exec: { command: \"x\" }\n  - id: b\n    depends_on: [";
        let items = completion(text, text.len());
        assert_eq!(
            labels(&items),
            vec!["my_task"],
            "the underscored id is read whole, not truncated to `my` \
             (`b` is the task being edited — self excluded)"
        );
    }

    #[test]
    fn list_marker_id_is_a_task_field_position() {
        // The `- ` list marker introducing a task is a field position: after
        // stripping `- `, the body is a bare ident fragment. (`is_task_field_
        // key` strips the `- ` marker.)
        let text = "nika: v1\nworkflow: w\ntasks:\n  - i";
        let items = completion(text, text.len());
        assert!(
            labels(&items).contains(&"id".to_owned()),
            "the `- ` marker line offers task fields: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn inside_a_value_offers_nothing() {
        // cursor inside a quoted prompt value — no trigger context
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"echo ";
        let items = completion(text, text.len());
        // not a model/ref/key position → empty (the `command` line has a `:`
        // and an open value)
        assert!(
            items.is_empty(),
            "no spurious completions: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn scan_task_ids_dedups_and_reads_in_order() {
        // The scan path reads `id:` lines in source order and dedups repeats.
        // (`!id.is_empty() && !ids.contains(&id)` — the `&&` and the `==` in
        // the take_while predicate both matter.) A duplicate id must appear
        // ONCE; the order is source order.
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: first\n    exec: { command: \"x\" }\n  - id: second\n    exec: { command: \"y\" }\n  - id: first\n    exec: { command: \"z\" }\n  - id: editor\n    depends_on: [";
        let items = completion(text, text.len());
        assert_eq!(
            labels(&items),
            vec!["first", "second"],
            "source order, deduplicated (`first` once) — and `editor`, \
             the task being edited, excludes itself"
        );
    }

    #[test]
    fn completion_is_total_over_a_mid_char_offset() {
        // The entry clamps a past-the-end offset (`offset.min(len)`) but the
        // clamp does not land on a char BOUNDARY — a mid-multibyte offset
        // slices `text[..offset]` mid-char and panics, which in the sync
        // serve loop takes the WHOLE language server down. `completion` must
        // be total over `(text, offset)`: any offset returns, never panics.
        // (The server floors offsets via `LineIndex::offset`, so this is
        // defence-in-depth for a public analysis primitive, not a reachable
        // crash today — the bar for an LSP is « no input panics ».)
        let _ = completion("é", 1); // byte 1 is inside the 2-byte 'é'
        let _ = completion("nika: v1\n", 99_999); // far past the end
        let doc = "nika: v1\nmodel: 🦋";
        let _ = completion(doc, doc.len() - 1); // inside the trailing 4-byte 🦋
    }

    /// Inside `args:` of a KNOWN builtin, key position → that tool's own
    /// argument names, catalog-derived (the born-stale law again) —
    /// required ones sorted first and marked.
    #[test]
    fn fetch_args_offer_the_catalog_arg_keys() {
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: get\n    invoke:\n      tool: nika:fetch\n      args:\n        ";
        let items = completion(text, text.len());
        let fetch = nika_catalog::all_builtins()
            .iter()
            .find(|b| b.name == "fetch")
            .expect("fetch in catalog");
        assert_eq!(items.len(), fetch.args.len(), "exactly the catalog args");
        let labels = labels(&items);
        assert!(labels.contains(&"url:".to_owned()), "{labels:?}");
        assert!(labels.contains(&"mode:".to_owned()), "{labels:?}");
        // required args float first (sort_text partition: '0…' < '1…')
        for item in &items {
            let required = item
                .detail
                .as_deref()
                .is_some_and(|d| d.starts_with("required"));
            let sort = item.sort_text.as_deref().unwrap_or("");
            assert_eq!(
                sort.starts_with('0'),
                required,
                "sort partition mirrors required: {item:?}"
            );
        }
    }

    /// An UNKNOWN tool's args stay its own business — no `nika:fetch`
    /// keys leak under an MCP tool.
    #[test]
    fn mcp_tool_args_do_not_leak_fetch_keys() {
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: gh\n    invoke:\n      tool: github.search\n      args:\n        ";
        let labels = labels(&completion(text, text.len()));
        assert!(
            !labels.contains(&"url:".to_owned()),
            "no cross-tool leak: {labels:?}"
        );
    }

    /// `mode:` under `nika:fetch` offers the stdlib extract vocabulary —
    /// the SET is `ExtractMode::ALL` (a mode added at L0 appears here
    /// with zero LSP edits). Under any other tool the lane stays silent.
    #[test]
    fn fetch_mode_offers_the_stdlib_extract_vocabulary() {
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: get\n    invoke:\n      tool: nika:fetch\n      args:\n        mode: ";
        let items = completion(text, text.len());
        assert_eq!(items.len(), nika_types::ExtractMode::ALL.len());
        let labels = labels(&items);
        assert_eq!(labels[0], "markdown", "canon order — the default first");
        assert!(labels.contains(&"jq".to_owned()), "{labels:?}");

        let other = "nika: v1\nworkflow: w\ntasks:\n  - id: x\n    invoke:\n      tool: github.search\n      args:\n        mode: ";
        assert!(
            labels_of(other).iter().all(|l| l != "jq"),
            "another tool's `mode:` is not the extract vocabulary"
        );
    }

    /// `${{ vars.` offers the file's OWN declared vars. On a document
    /// that parses (cursor mid-island, island closed), typed vars carry
    /// type + required + description and untyped ones their default; a
    /// mid-keystroke document that no longer parses falls back to the
    /// block scan — names still arrive, detail goes generic.
    #[test]
    fn island_vars_offer_the_declared_names() {
        let text = "nika: v1\nworkflow: w\nvars:\n  city:\n    type: string\n    required: true\n    description: target city\n  out_dir: \"./out\"\ntasks:\n  - id: a\n    exec: { command: \"echo ${{ vars.city }}\" }\n";
        let cursor = text.find("${{ vars.").expect("island") + "${{ vars.".len();
        let items = completion(text, cursor);
        let got = labels(&items);
        assert_eq!(got, vec!["city", "out_dir"], "{got:?}");
        let detail = items[0].detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("string")
                && detail.contains("required")
                && detail.contains("target city"),
            "typed var detail teaches type · required · description: {detail}"
        );
        assert!(
            items[1].detail.as_deref().unwrap_or("").contains("./out"),
            "untyped var detail carries the default"
        );

        // mid-keystroke fallback: unterminated island · parse fails · the
        // block scan still teaches the NAMES
        let typing = "nika: v1\nworkflow: w\nvars:\n  city:\n    type: string\n  out_dir: \"./out\"\ntasks:\n  - id: a\n    exec: { command: \"echo ${{ vars.";
        let fallback = labels(&completion(typing, typing.len()));
        assert_eq!(fallback, vec!["city", "out_dir"], "{fallback:?}");
    }

    /// `${{ secrets.` / `${{ env.` offer the file's own declared names.
    #[test]
    fn island_secrets_and_env_offer_declared_names() {
        let text = "nika: v1\nworkflow: w\nenv:\n  REGION: eu-west-1\nsecrets:\n  api_key:\n    source: env\n    env: MY_KEY\ntasks:\n  - id: a\n    exec: { command: \"echo ${{ secrets.";
        let labels_s = labels(&completion(text, text.len()));
        assert_eq!(labels_s, vec!["api_key"], "{labels_s:?}");

        let text2 = "nika: v1\nworkflow: w\nenv:\n  REGION: eu-west-1\ntasks:\n  - id: a\n    exec: { command: \"echo ${{ env.";
        let labels_e = labels(&completion(text2, text2.len()));
        assert_eq!(labels_e, vec!["REGION"], "{labels_e:?}");
    }

    fn labels_of(text: &str) -> Vec<String> {
        labels(&completion(text, text.len()))
    }
}
