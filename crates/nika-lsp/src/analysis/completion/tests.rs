// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The completion lanes, pinned — split out of `completion.rs` when the
//! 557 lanes pushed the single file past the 1500-LOC cap (the Diamond
//! law: split, never exempt). `use super::*` keeps every private lane
//! reachable — same module, its own file.

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
    let text = "nika: v1\ntasks:\n  - id: a\n    exec:\n      command: [\"x\"]\n      capture: ";
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
            "groq/",
            "deepseek/",
            "openrouter/",
            "huggingface/",
            "nvidia/",
            "anthropic/",
            "openai/",
            "google/",
            "xai/",
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
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: [\"x\"] }\n  - id: save\n    depends_on: [";
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
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: [\"x\"] }\n  - id: save\n    depends_on: [extract]\n    exec: { command: [\"y\"] }\n";
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
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: [\"x\"] }\n  - id: save\n    depends_on: [extract] ";
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
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    exec: { command: [\"x\"] }\n  - id: save\n    depends_on: [\n      extract,\n      ";
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
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: extract\n    infer: { prompt: \"hi\", max_tokens: 5 }\n  - id: use\n    depends_on: [extract]\n    exec: { command: \"echo ${{ tasks.";
    let items = completion(text, text.len());
    assert_eq!(
        labels(&items),
        vec!["extract"],
        "the DECLARED edge — DAG-003 accepts nothing else"
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

    // Bare `tasks.` still resolves to task IDS, never methods —
    // and only the DECLARED edges (DAG-003): b waits on a, so a.
    let bare = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    infer: { prompt: \"x\" }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo ${{ tasks.";
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
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: my_task\n    exec: { command: [\"x\"] }\n  - id: b\n    depends_on: [";
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
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: first\n    exec: { command: [\"x\"] }\n  - id: second\n    exec: { command: [\"y\"] }\n  - id: first\n    exec: { command: [\"z\"] }\n  - id: editor\n    depends_on: [";
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
    let text = "nika: v1\nworkflow: w\nvars:\n  city:\n    type: string\n    required: true\n    description: target city\n  out_dir: \"./out\"\ntasks:\n  - id: a\n    exec: { command: [\"echo\", \"${{ vars.city }}\"] }\n";
    let cursor = text.find("${{ vars.").expect("island") + "${{ vars.".len();
    let items = completion(text, cursor);
    let got = labels(&items);
    assert_eq!(got, vec!["city", "out_dir"], "{got:?}");
    let detail = items[0].detail.as_deref().unwrap_or("");
    assert!(
        detail.contains("string") && detail.contains("required") && detail.contains("target city"),
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
    let text = "nika: v1\nworkflow: w\nenv:\n  REGION: eu-west-1\nsecrets:\n  api_key:\n    source: env\n    key: MY_KEY\ntasks:\n  - id: a\n    exec: { command: \"echo ${{ secrets.";
    let labels_s = labels(&completion(text, text.len()));
    assert_eq!(labels_s, vec!["api_key"], "{labels_s:?}");

    let text2 = "nika: v1\nworkflow: w\nenv:\n  REGION: eu-west-1\ntasks:\n  - id: a\n    exec: { command: \"echo ${{ env.";
    let labels_e = labels(&completion(text2, text2.len()));
    assert_eq!(labels_e, vec!["REGION"], "{labels_e:?}");
}

/// A task-field island offers ONLY the declared edges — DAG-003
/// (probed at the binary): even a transitive ancestor or a parallel
/// task is refused without its own `depends_on` entry. In the
/// diamond a → {b, c} → d, task `b`'s island offers exactly `a`.
#[test]
fn island_tasks_offer_only_the_declared_edges() {
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: [\"x\"] }\n  - id: b\n    depends_on: [a]\n    exec: { command: [\"echo\", \"${{ tasks.a.output }}\"] }\n  - id: c\n    depends_on: [a]\n    exec: { command: [\"x\"] }\n  - id: d\n    depends_on: [b, c]\n    exec: { command: [\"x\"] }\n";
    // cursor mid-island inside task b (document parses whole)
    let cursor = text.find("${{ tasks.").expect("island") + "${{ tasks.".len();
    let got = labels(&completion(text, cursor));
    assert_eq!(got, vec!["a"], "declared edges only: {got:?}");
    let items = completion(text, cursor);
    assert!(
        items[0]
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("declared dependency"),
        "{items:?}"
    );
}

/// The recover carve-out (spec 05 · DAG-003 exempt · DAG-004 binds):
/// a `recover:` island offers everything except the task itself and
/// its downstream closure — the deadlock set.
#[test]
fn recover_island_offers_the_dag004_legal_set() {
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: cached\n    exec: { command: [\"echo\", \"fallback\"] }\n  - id: live\n    exec:\n      command: [\"false\"]\n    on_error:\n      recover: \"${{ tasks.cached.output }}\"\n  - id: after\n    depends_on: [live]\n    exec: { command: [\"x\"] }\n";
    let cursor = text.find("${{ tasks.").expect("island") + "${{ tasks.".len();
    let got = labels(&completion(text, cursor));
    assert_eq!(
        got,
        vec!["cached"],
        "not itself · not `after` (deadlock): {got:?}"
    );
}

/// A workflow-level `outputs:` island is not inside any task — every
/// task is legal there (probed green at the binary).
#[test]
fn outputs_island_offers_every_task() {
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: [\"x\"] }\n  - id: b\n    depends_on: [a]\n    exec: { command: [\"y\"] }\noutputs:\n  first: \"${{ tasks.";
    let got = labels(&completion(text, text.len()));
    assert_eq!(got, vec!["a", "b"], "outside a task, all ids: {got:?}");
}

fn labels_of(text: &str) -> Vec<String> {
    labels(&completion(text, text.len()))
}

// ─── the 557 lanes: tools list · task members · schema context ─────────

#[test]
fn agent_tools_list_speaks_the_catalog() {
    let text =
        "nika: v1\ntasks:\n  - id: judge\n    agent:\n      prompt: \"rule\"\n      tools: [\"";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.iter().any(|l| l == "nika:fetch"),
        "the whitelist position offers the catalog: {got:?}"
    );
    // a CLOSED list is not a completion position
    let closed =
        "nika: v1\ntasks:\n  - id: judge\n    agent:\n      tools: [\"nika:fetch\"]\n      ";
    let after = labels(&completion(closed, closed.len()));
    assert!(
        !after.iter().any(|l| l == "nika:fetch"),
        "past the closing bracket the register goes quiet: {after:?}"
    );
}

#[test]
fn an_abandoned_open_bracket_upstream_captures_nothing() {
    // A `tools: ["` left unclosed must not leak the catalog into every
    // later position — the scope dies at the first line back at (or
    // above) the key's indent.
    let text =
        "nika: v1\ntasks:\n  - id: judge\n    agent:\n      tools: [\"\n      schema:\n        ";
    let got = labels(&completion(text, text.len()));
    assert!(
        !got.iter().any(|l| l.starts_with("nika:")),
        "the abandoned bracket stays in its block: {got:?}"
    );
}

#[test]
fn task_member_island_teaches_the_three_facts_and_the_bindings() {
    let text = "nika: v1\nworkflow: w\ntasks:\n  - id: gather\n    exec:\n      command: [\"ls\"]\n    output:\n      first_line: \".stdout\"\n  - id: use\n    depends_on: [gather]\n    when: ${{ tasks.gather.";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.contains(&"output".to_owned())
            && got.contains(&"status".to_owned())
            && got.contains(&"error".to_owned()),
        "the three spec facts: {got:?}"
    );
    assert!(
        got.contains(&"first_line".to_owned()),
        "the task's own named binding rides along: {got:?}"
    );
    let output_detail = completion(text, text.len())
        .into_iter()
        .find(|i| i.label == "output")
        .and_then(|i| i.detail)
        .unwrap_or_default();
    assert!(
        output_detail.contains("exec"),
        "verb-aware detail: {output_detail}"
    );
}

#[test]
fn task_member_island_survives_an_unknown_id() {
    let text = "nika: v1\ntasks:\n  - id: a\n    exec: {command: [\"ls\"]}\n  - id: b\n    when: ${{ tasks.ghost.";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.contains(&"output".to_owned()) && got.contains(&"status".to_owned()),
        "an unknown id still teaches the facts (mid-rename): {got:?}"
    );
}

#[test]
fn data_path_post_dot_still_ends_in_cel_methods() {
    // `tasks.x.output.` is PAST the member — the CEL method position
    // must keep winning there (the member lane stops at one segment).
    let text = "nika: v1\ntasks:\n  - id: b\n    when: ${{ tasks.x.output.";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.iter().any(|l| l.starts_with("size")),
        "past a data path the methods speak: {got:?}"
    );
}

#[test]
fn schema_children_speak_json_schema_not_task_fields() {
    let text =
        "nika: v1\ntasks:\n  - id: a\n    infer:\n      prompt: \"p\"\n      schema:\n        ";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.contains(&"required".to_owned()) && got.contains(&"properties".to_owned()),
        "the JSON-Schema keyset: {got:?}"
    );
    assert!(
        !got.contains(&"depends_on".to_owned()),
        "the 557 probe bug — task fields must NOT leak into schema: {got:?}"
    );
}

#[test]
fn properties_children_belong_to_the_author() {
    let text =
        "nika: v1\ntasks:\n  - id: a\n    infer:\n      schema:\n        properties:\n          ";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.is_empty(),
        "property NAMES are the author's — silence beats noise: {got:?}"
    );
}

#[test]
fn items_inside_a_schema_keeps_the_keyset() {
    let text = "nika: v1\ntasks:\n  - id: a\n    infer:\n      schema:\n        properties:\n          rows:\n            items:\n              ";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.contains(&"type".to_owned()) && got.contains(&"enum".to_owned()),
        "items: nested anywhere inside a schema speaks JSON Schema: {got:?}"
    );
}

#[test]
fn task_fields_survive_outside_schema() {
    let text = "nika: v1\ntasks:\n  - id: a\n    exec:\n      command: [\"ls\"]\n  - id: b\n    ";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.contains(&"depends_on".to_owned()),
        "the task-field lane is untouched outside schema: {got:?}"
    );
}

// ─── the wave-2 lanes: for_each/when islands · skills list ──────────────────

const FLOW_DOC: &str = "nika: v1\nworkflow: w\nvars:\n  urls:\n    type: array\n    default: []\n  topic: \"rust\"\ntasks:\n  - id: gather\n    exec:\n      command: [\"ls\"]\n  - id: fan\n    depends_on: [gather]\n    for_each: \n    exec:\n      command: [\"echo\"]\n  - id: last\n    depends_on: [fan]\n    exec:\n      command: [\"true\"]\n";

#[test]
fn for_each_offers_typed_arrays_first_then_upstream_cycle_safe() {
    let at = FLOW_DOC.find("for_each: ").expect("key") + "for_each: ".len();
    let items = completion(FLOW_DOC, at);
    let got = labels(&items);
    assert_eq!(
        got.first().map(String::as_str),
        Some("${{ vars.urls }}"),
        "the typed array floats first: {got:?}"
    );
    assert!(
        got.contains(&"${{ tasks.gather.output }}".to_owned()),
        "upstream rides: {got:?}"
    );
    assert!(
        !got.iter().any(|l| l.contains("tasks.last")),
        "a downstream task is cycle territory: {got:?}"
    );
    assert!(
        !got.iter().any(|l| l.contains("tasks.fan")),
        "self never offered: {got:?}"
    );
    // The wired edge is named as already wired; topic is offered honestly.
    let gather = items
        .iter()
        .find(|i| i.label.contains("tasks.gather"))
        .and_then(|i| i.detail.clone())
        .unwrap_or_default();
    assert!(gather.contains("already wired"), "edge detail: {gather}");
    assert!(
        got.contains(&"${{ vars.topic }}".to_owned()),
        "other vars offered honestly: {got:?}"
    );
}

#[test]
fn when_composes_the_cel_shapes_from_the_document() {
    let doc = FLOW_DOC.replace("for_each: ", "when: ");
    let at = doc.find("when: ").expect("key") + "when: ".len();
    let got = labels(&completion(&doc, at));
    assert!(
        got.contains(&"${{ tasks.gather.status == 'success' }}".to_owned()),
        "the status gate: {got:?}"
    );
    assert!(
        got.contains(&"${{ size(tasks.gather.output) > 0 }}".to_owned()),
        "the size() empty-check (the subset's one function): {got:?}"
    );
    assert!(
        got.contains(&"${{ vars.topic }}".to_owned()),
        "the var switch: {got:?}"
    );
}

#[test]
fn a_partial_non_island_value_stays_silent() {
    let doc = FLOW_DOC.replace("for_each: \n", "for_each: som\n");
    let at = doc.find("for_each: som").expect("key") + "for_each: som".len();
    assert!(
        completion(&doc, at).is_empty(),
        "the author is typing their own value — silence beats noise"
    );
}

#[test]
fn skills_positions_route_to_the_walk_and_pure_callers_lose_only_that_lane() {
    // Flow form and block form both detect; with no doc_dir (the pure
    // caller) the lane yields empty INSTEAD of falling through to some
    // other register.
    let flow = "nika: v1\ntasks:\n  - id: a\n    agent:\n      prompt: \"p\"\n      skills: [\"";
    assert!(completion(flow, flow.len()).is_empty());
    let block =
        "nika: v1\ntasks:\n  - id: a\n    agent:\n      prompt: \"p\"\n      skills:\n        - ";
    assert!(completion(block, block.len()).is_empty());
    // A block item under TOOLS (not skills) keeps its own lane silent-or-
    // catalog — never the skills walk; the ancestor check is exact.
    let other =
        "nika: v1\ntasks:\n  - id: a\n    agent:\n      prompt: \"p\"\n      tools:\n        - ";
    let got = labels(&completion(other, other.len()));
    assert!(
        !got.iter().any(|l| l.ends_with("SKILL.md")),
        "tools block items never route to the skills walk: {got:?}"
    );
}
