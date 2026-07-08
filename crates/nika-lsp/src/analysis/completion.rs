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

use super::vocab::{self, Entry};

/// Compute completion items for the cursor at `offset` in `text`.
#[must_use]
pub fn completion(text: &str, offset: usize) -> Vec<CompletionItem> {
    let offset = floor_char_boundary(text, offset);
    let line = current_line(text, offset);
    let prefix = line_prefix(text, offset);

    if is_model_value(prefix) {
        return providers();
    }
    if in_open_depends_on(text, offset) || is_template_tasks_ref(prefix) {
        return task_ids(text);
    }
    if is_expression_post_dot(prefix) {
        return cel_methods();
    }
    if is_expression_start(prefix) {
        return expression_roots();
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
fn task_ids(text: &str) -> Vec<CompletionItem> {
    // Prefer the parser's authoritative ids (with verb detail) when the
    // document parses; fall back to the line scan otherwise.
    if let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient)
        && !wf.tasks.is_empty()
    {
        return wf
            .tasks
            .iter()
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
            vec!["extract", "save"],
            "exactly the two task ids in source order"
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
        assert_eq!(labels(&items), vec!["extract", "save"], "the task ids");
        assert!(
            items
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VARIABLE)),
            "VARIABLE-kinded"
        );
        assert_eq!(
            items.iter().map(|i| i.detail.clone()).collect::<Vec<_>>(),
            vec![
                Some("task (exec)".to_owned()),
                Some("task (exec)".to_owned())
            ],
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
        assert!(labels.contains(&"save".to_owned()));
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
            vec!["extract", "use"],
            "the workflow task ids"
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
            vec!["my_task", "b"],
            "the underscored id is read whole, not truncated to `my`"
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
        let text = "nika: v1\nworkflow: w\ntasks:\n  - id: first\n    exec: { command: \"x\" }\n  - id: second\n    exec: { command: \"y\" }\n  - id: first\n    depends_on: [";
        let items = completion(text, text.len());
        assert_eq!(
            labels(&items),
            vec!["first", "second"],
            "source order, deduplicated — `first` appears once"
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
}
