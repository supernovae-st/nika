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
    let text = "nika: w\ntasks:\n  a:\n    } x";
    let cursor = text.rfind('}').expect("brace"); // caret at the indent
    let items = completion(text, cursor);
    assert!(
        items.is_empty(),
        "a `}}`-leading line is not a field position: {:?}",
        labels(&items)
    );
    // CONTRAST: a normal indented ident fragment DOES offer fields — so
    // the suppression above is the `}` guard, not a blanket empty.
    let normal = "nika: w\ntasks:\n  a:\n    de";
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
    let text =
        "nika: w\ntasks:\n  a:\n    infer:\n      prompt: |\n        Summarize the following ";
    assert!(completion(text, text.len()).is_empty());
}

/// `tool: ` offers the full catalog-derived `nika:*` set — and the
/// count IS the catalog's (the born-stale gate: a builtin added to
/// the catalog appears here with zero LSP edits).
#[test]
fn tool_value_offers_exactly_the_builtin_set() {
    let text = "nika: w\ntasks:\n  a:\n    invoke:\n      tool: ";
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
    let text = "nika: v1\ntasks:\n  a:\n    exec:\n      command: [\"x\"]\n      capture: ";
    let labels_ = labels(&completion(text, text.len()));
    assert_eq!(labels_, vec!["text".to_owned(), "structured".to_owned()]);

    let text2 = "nika: v1\ntasks:\n  a:\n    retry:\n      backoff_strategy: ";
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
    let text4 = "nika: v1\ninputs:\n  city:\n    type: ";
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
    let text = "nika: w\nmodel: ";
    let items = completion(text, text.len());
    // DERIVED from the vocabulary, in ITS order — the retyped copy that
    // used to live here carried `google/`, an alias the catalog does not
    // know, so the editor completed a provider the binary refuses
    // (2026-08-02). What this pins is the SHAPE: every provider, each
    // suffixed with `/`, order preserved.
    let want: Vec<String> = crate::analysis::vocab::PROVIDERS
        .iter()
        .map(|e| format!("{}/", e.name))
        .collect();
    assert_eq!(labels(&items), want, "the model lane offers the catalog");
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
fn after_key_offers_exactly_the_legal_producers() {
    // A bare key position under `after:` — the mid-keystroke document
    // does not parse → the scan path yields the ids with detail "task"
    // and VARIABLE kind. EXACT set (self excluded — a self-edge is a
    // cycle the check refuses).
    let text =
        "nika: w\ntasks:\n  extract:\n    exec: { command: [\"x\"] }\n  save:\n    after:\n      ";
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
fn a_parseable_doc_carries_the_verb_detail_in_task_refs() {
    // W2 migration of the depends_on parse-path scenario: a FULLY VALID
    // document with the cursor mid-island in a workflow-level `outputs:`
    // value (outside any task — every id is legal). The parse path wins,
    // so each id carries its verb in the detail (`task (exec)`) — the
    // `!wf.tasks.is_empty()` guard + the label/kind/detail fields of the
    // parse-path CompletionItem.
    let text = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"y\"] }\noutputs:\n  first: \"${{ tasks.a.output }}\"\n";
    let cursor = text.find("${{ tasks.").expect("island") + "${{ tasks.".len();
    let items = completion(text, cursor);
    assert_eq!(
        labels(&items),
        vec!["a", "b"],
        "outside a task, every id — from the parse path"
    );
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
    let text = "nika: w\ntasks:\n  extract:\n    exec: { command: [\"x\"] }\n  save:\n    depends_on: [extract] ";
    let items = completion(text, text.len());
    assert!(
        !labels(&items).iter().any(|l| l == "extract" || l == "save"),
        "a closed depends_on offers no task ids: {:?}",
        labels(&items)
    );
}

#[test]
fn after_value_offers_the_closed_predicate_set() {
    // `<producer>: <cursor>` inside an `after:` block — the VALUE
    // position completes the CLOSED predicate set, in spec order
    // (03 §after · never a producer id here).
    let text = "nika: w\ntasks:\n  extract:\n    exec: { command: [\"x\"] }\n  save:\n    after:\n      extract: ";
    let items = completion(text, text.len());
    assert_eq!(
        labels(&items),
        vec!["success", "failure", "skipped", "terminal", "unwind"],
        "the closed predicate set, spec order"
    );
    assert!(
        items
            .iter()
            .all(|i| i.kind == Some(CompletionItemKind::ENUM_MEMBER)),
        "predicates are ENUM_MEMBER-kinded"
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
fn template_tasks_dot_in_a_verb_body_offers_nothing() {
    // W2 mutation of « template tasks. offers task ids »: a verb body is
    // OUTSIDE the reference boundary (NIKA-VAR-021 — data crosses via a
    // `with:` binding), so the lane offers NOTHING there — silence beats
    // teaching an error (never offer what check refuses).
    let text = "nika: w\ntasks:\n  extract:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n  use:\n    exec: { command: \"echo ${{ tasks.";
    let items = completion(text, text.len());
    assert!(
        items.is_empty(),
        "a verb-body island is outside the boundary: {:?}",
        labels(&items)
    );
}

#[test]
fn template_island_without_tasks_dot_offers_nothing() {
    // An open `${{ ` island that does NOT end with `tasks.` is not a task
    // reference. `is_template_tasks_ref` requires BOTH `!contains("}}")`
    // AND `ends_with("tasks.")` — flipping the `&&` to `||` would fire on
    // any open island.
    let text = "nika: w\ntasks:\n  extract:\n    exec: { command: \"echo ${{ inputs.";
    let items = completion(text, text.len());
    assert!(
        !labels(&items).iter().any(|l| l == "extract"),
        "an `${{{{ inputs.` island is not a tasks ref: {:?}",
        labels(&items)
    );
}

#[test]
fn top_level_offers_exactly_the_envelope_keys() {
    let text = "nika: v1\nwo";
    let items = completion(text, text.len());
    // DERIVED — the retyped copy here offered `description`, which the
    // parser refuses with NIKA-PARSE-021, and omitted four keys it
    // accepts. A third hand-written list cannot referee the other two.
    let want: Vec<&str> = crate::analysis::vocab::TOP_LEVEL_KEYS
        .iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(labels(&items), want, "the envelope lane offers the parser");
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
    let text = "nika: v1\nmodel: o";
    let items = completion(text, text.len());
    assert!(
        !labels(&items).contains(&"workflow".to_owned()),
        "a `key: value` line is not a key position: {:?}",
        labels(&items)
    );
}

#[test]
fn task_field_offers_exactly_the_fields_and_verbs() {
    // indented key position inside a task: the task fields in vocab
    // order, then the 4 verbs. EXACT set + kinds (fields PROPERTY,
    // verbs KEYWORD). The SET itself is pinned to the parser by
    // `vocab::tests::task_field_keys_mirror_the_parser` — this test
    // owns the ORDER and the kinds, which no derivation can carry.
    let text = "nika: w\ntasks:\n  a:\n    af";
    let items = completion(text, text.len());
    assert_eq!(
        labels(&items),
        vec![
            // W2: after/with are the two doors · id and depends_on are
            // dead forms · `max_parallel`/`fail_fast` live INSIDE
            // `for_each:` and `on_finally` died with the E_f rewrite
            "after", "when", "for_each", "retry", "on_error", "timeout", "with", "extract",
            "returns", "group", "lift", // then the 4 verbs
            "infer", "exec", "invoke", "agent",
        ],
        "task fields then the 4 verbs"
    );
    // Fields are PROPERTY-kinded, verbs KEYWORD-kinded — the SPLIT
    // POINT derives from the tables, because a hand-typed index moves
    // silently every time the task shape does (it did, three times in
    // one night: `max_parallel` · `fail_fast` · `on_finally`).
    let fields = crate::analysis::vocab::TASK_FIELD_KEYS.len();
    let ks = kinds(&items);
    assert_eq!(
        ks.len(),
        fields + crate::analysis::vocab::VERBS.len(),
        "every field and every verb, all kinded"
    );
    assert!(
        ks[..fields]
            .iter()
            .all(|k| *k == CompletionItemKind::PROPERTY),
        "fields are PROPERTY-kinded"
    );
    assert!(
        ks[fields..]
            .iter()
            .all(|k| *k == CompletionItemKind::KEYWORD),
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
    let text = "nika: w\ntasks:\n  a:\n    exec: { command";
    let items = completion(text, text.len());
    // `command` has no colon yet, BUT the line started a flow map — it is
    // still a bare ident fragment, so fields ARE offered here. Assert the
    // CONVERSE boundary: once a `:` appears, nothing fires.
    let after_colon = "nika: w\ntasks:\n  a:\n    exec: x";
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
    let text = "nika: w\ntasks:\n  a:\n    for_";
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
/// the bare `tasks.` island stays task-id territory where the lane
/// answers (W2: the recover carve-out), never methods.
#[test]
fn expression_post_dot_completes_cel_methods() {
    // the data path rides a `with:` binding value (the W2 boundary form)
    let text = "nika: w\ntasks:\n  a:\n    infer: { prompt: \"x\" }\n  b:\n    with:\n      data: ${{ tasks.a.output.";
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

    // Bare `tasks.` still resolves to task IDS, never methods — probed
    // on the recover lane (spec 05 · refs there are NOT edges).
    let bare = "nika: w\ntasks:\n  a:\n    infer: { prompt: \"x\" }\n  b:\n    exec:\n      command: [\"false\"]\n    on_error:\n      recover: \"${{ tasks.";
    let ids: Vec<String> = completion(bare, bare.len())
        .into_iter()
        .map(|i| i.label)
        .collect();
    assert!(
        ids.contains(&"a".to_owned()),
        "task ids win on the bare root: {ids:?}"
    );
    assert!(
        !ids.iter().any(|l| l.starts_with("size")),
        "never methods on the bare root: {ids:?}"
    );
}

/// B6 · the island start completes the five locked roots + the two
/// free functions — the loop-scoped pair stays OUT of a non-fan-out
/// task (its own law test below covers the gate both ways).
#[test]
fn expression_start_completes_roots_and_free_functions() {
    let text = "nika: w\ntasks:\n  a:\n    infer: { prompt: \"${{ ";
    let labels: Vec<String> = completion(text, text.len())
        .into_iter()
        .map(|i| i.label)
        .collect();
    for root in ["tasks", "inputs", "const", "secrets", "with"] {
        assert!(
            labels.contains(&root.to_owned()),
            "missing root {root}: {labels:?}"
        );
    }
    assert!(labels.contains(&"has(…)".to_owned()));

    // A closed island offers nothing from the expression vocab.
    let closed = "nika: w\ntasks:\n  a:\n    infer: { prompt: \"${{ inputs.x }} and ";
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
    let text =
        "nika: w\ntasks:\n  my_task:\n    exec: { command: [\"x\"] }\n  b:\n    after:\n      ";
    let items = completion(text, text.len());
    assert_eq!(
        labels(&items),
        vec!["my_task"],
        "the underscored id is read whole, not truncated to `my` \
         (`b` is the task being edited — self excluded)"
    );
}

#[test]
fn list_marker_is_a_task_field_position() {
    // The `- ` list marker is a field position: after stripping `- `,
    // the body is a bare ident fragment (`is_task_field_key` strips the
    // marker — the mechanics serve `on_finally:` mini-task items).
    let text = "nika: w\ntasks:\n  - w";
    let items = completion(text, text.len());
    assert!(
        labels(&items).contains(&"when".to_owned()),
        "the `- ` marker line offers task fields: {:?}",
        labels(&items)
    );
}

#[test]
fn inside_a_value_offers_nothing() {
    // cursor inside a quoted prompt value — no trigger context
    let text = "nika: w\ntasks:\n  a:\n    exec: { command: \"echo ";
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
    let text = "nika: w\ntasks:\n  first:\n    exec: { command: [\"x\"] }\n  second:\n    exec: { command: [\"y\"] }\n  first:\n    exec: { command: [\"z\"] }\n  editor:\n    after:\n      ";
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
    let text =
        "nika: w\ntasks:\n  get:\n    invoke:\n      tool: nika:fetch\n      args:\n        ";
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
    let text =
        "nika: w\ntasks:\n  gh:\n    invoke:\n      tool: github.search\n      args:\n        ";
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
    let text =
        "nika: w\ntasks:\n  get:\n    invoke:\n      tool: nika:fetch\n      args:\n        mode: ";
    let items = completion(text, text.len());
    assert_eq!(items.len(), nika_types::ExtractMode::ALL.len());
    let labels = labels(&items);
    assert_eq!(labels[0], "markdown", "canon order — the default first");
    assert!(labels.contains(&"jq".to_owned()), "{labels:?}");

    let other = "nika: w\ntasks:\n  x:\n    invoke:\n      tool: github.search\n      args:\n        mode: ";
    assert!(
        labels_of(other).iter().all(|l| l != "jq"),
        "another tool's `mode:` is not the extract vocabulary"
    );
}

/// `${{ inputs.` offers the file's OWN declared vars. On a document
/// that parses (cursor mid-island, island closed), typed vars carry
/// type + required + description and untyped ones their default; a
/// mid-keystroke document that no longer parses falls back to the
/// block scan — names still arrive, detail goes generic.
#[test]
fn island_vars_offer_the_declared_names() {
    let text = "nika: w\ninputs:\n  city:\n    type: string\n    required: true\n    description: target city\n  out_dir: { type: string, required: false, default: \"./out\" }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ inputs.city }}\"] }\n";
    let cursor = text.find("${{ inputs.").expect("island") + "${{ inputs.".len();
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
    let typing = "nika: w\ninputs:\n  city:\n    type: string\n  out_dir: { type: string, required: false, default: \"./out\" }\ntasks:\n  a:\n    exec: { command: \"echo ${{ inputs.";
    let fallback = labels(&completion(typing, typing.len()));
    assert_eq!(fallback, vec!["city", "out_dir"], "{fallback:?}");
}

/// `${{ secrets.` / `${{ inputs.` offer the file's own declared names.
#[test]
fn island_secrets_and_env_offer_declared_names() {
    let text = "nika: w\ninputs:\n  REGION: eu-west-1\nsecrets:\n  api_key:\n    source: env\n    key: MY_KEY\ntasks:\n  a:\n    exec: { command: \"echo ${{ secrets.";
    let labels_s = labels(&completion(text, text.len()));
    assert_eq!(labels_s, vec!["api_key"], "{labels_s:?}");

    let text2 = "nika: w\ninputs:\n  REGION: eu-west-1\ntasks:\n  a:\n    exec: { command: \"echo ${{ inputs.";
    let labels_e = labels(&completion(text2, text2.len()));
    assert_eq!(labels_e, vec!["REGION"], "{labels_e:?}");
}

/// A `${{ tasks.` island inside a `with:` VALUE — the boundary lane.
/// INTENDED (refs.rs's own doc + spec 04 §the reference boundary): every
/// task except itself and its downstream closure — in the diamond
/// a → {b, c} → d, editing `b` should offer `a` and `c` (the binding IS
/// the edge; DAG-001 bounds it). TODAY the routing yields NOTHING:
/// `scope::enclosing_block_key` answers only at bare-KEY positions, so
/// at an island VALUE position it returns `None` and the `with` /
/// The boundary lanes ALIVE (the block-walk router fix): a `with:`
/// island offers everything but self + its downstream closure (the
/// binding IS the edge · a downstream ref is a cycle); an `on_finally:`
/// island offers the PARENT alone (the only readable task there).
#[test]
fn with_and_on_finally_islands_offer_the_boundary_sets() {
    let text = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n  b:\n    with:\n      article: \"${{ tasks.a.output }}\"\n    exec: { command: [\"x\"] }\n  c:\n    after: { a: success }\n    exec: { command: [\"x\"] }\n  d:\n    after: { b: success, c: success }\n    exec: { command: [\"x\"] }\n";
    // cursor mid-island inside task b's `with:` value (document parses)
    let cursor = text.find("${{ tasks.").expect("island") + "${{ tasks.".len();
    let got = labels(&completion(text, cursor));
    assert_eq!(
        got,
        vec!["a", "c"],
        "everything but self (`b`) and its downstream closure (`d`)"
    );

    // on_finally — the PARENT is the only readable task.
    let cleanup = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n  b:\n    exec: { command: [\"y\"] }\n  sweep:\n    after: { b: unwind }\n    exec: { command: [\"rm\", \"${{ tasks.";
    let got2 = labels(&completion(cleanup, cleanup.len()));
    assert_eq!(got2, vec!["b"], "the parent alone");
}

/// The recover carve-out (spec 05 · refs there are NOT edges · DAG-004
/// binds): a `recover:` island offers everything except the task itself
/// and its downstream closure — the deadlock set. The document parses
/// whole, so the parse path carries the verb detail.
#[test]
fn recover_island_offers_the_dag004_legal_set() {
    let text = "nika: w\ntasks:\n  cached:\n    exec: { command: [\"echo\", \"fallback\"] }\n  live:\n    exec:\n      command: [\"false\"]\n    on_error:\n      recover: \"${{ tasks.cached.output }}\"\n  downstream:\n    after: { live: success }\n    exec: { command: [\"x\"] }\n";
    let cursor = text.find("${{ tasks.").expect("island") + "${{ tasks.".len();
    let items = completion(text, cursor);
    assert_eq!(
        labels(&items),
        vec!["cached"],
        "not itself · not `downstream` (deadlock): {:?}",
        labels(&items)
    );
    assert_eq!(
        items[0].detail.as_deref(),
        Some("task (exec)"),
        "the parse path carries the verb detail"
    );
}

/// A workflow-level `outputs:` island is not inside any task — every
/// task is legal there (probed green at the binary).
#[test]
fn outputs_island_offers_every_task() {
    let text = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"y\"] }\noutputs:\n  first: \"${{ tasks.";
    let got = labels(&completion(text, text.len()));
    assert_eq!(got, vec!["a", "b"], "outside a task, all ids: {got:?}");
}

fn labels_of(text: &str) -> Vec<String> {
    labels(&completion(text, text.len()))
}

// ─── the 557 lanes: tools list · task members · schema context ─────────

#[test]
fn agent_tools_list_speaks_the_catalog() {
    let text = "nika: v1\ntasks:\n  judge:\n    agent:\n      prompt: \"rule\"\n      tools: [\"";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.iter().any(|l| l == "nika:fetch"),
        "the whitelist position offers the catalog: {got:?}"
    );
    // a CLOSED list is not a completion position
    let closed = "nika: v1\ntasks:\n  judge:\n    agent:\n      tools: [\"nika:fetch\"]\n      ";
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
    let text = "nika: v1\ntasks:\n  judge:\n    agent:\n      tools: [\"\n      schema:\n        ";
    let got = labels(&completion(text, text.len()));
    assert!(
        !got.iter().any(|l| l.starts_with("nika:")),
        "the abandoned bracket stays in its block: {got:?}"
    );
}

#[test]
fn task_member_island_teaches_the_three_facts_and_the_bindings() {
    let text = "nika: w\ntasks:\n  gather:\n    exec:\n      command: [\"ls\"]\n    extract:\n      first_line: \".stdout\"\n  use:\n    with:\n      report: ${{ tasks.gather.";
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
    let text = "nika: v1\ntasks:\n  a:\n    exec: {command: [\"ls\"]}\n  b:\n    with:\n      x: ${{ tasks.ghost.";
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
    let text = "nika: v1\ntasks:\n  b:\n    with:\n      d: ${{ tasks.x.output.";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.iter().any(|l| l.starts_with("size")),
        "past a data path the methods speak: {got:?}"
    );
}

#[test]
fn schema_children_speak_json_schema_not_task_fields() {
    let text = "nika: v1\ntasks:\n  a:\n    infer:\n      prompt: \"p\"\n      schema:\n        ";
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
    let text = "nika: v1\ntasks:\n  a:\n    infer:\n      schema:\n        properties:\n          ";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.is_empty(),
        "property NAMES are the author's — silence beats noise: {got:?}"
    );
}

#[test]
fn items_inside_a_schema_keeps_the_keyset() {
    let text = "nika: v1\ntasks:\n  a:\n    infer:\n      schema:\n        properties:\n          rows:\n            items:\n              ";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.contains(&"type".to_owned()) && got.contains(&"enum".to_owned()),
        "items: nested anywhere inside a schema speaks JSON Schema: {got:?}"
    );
}

#[test]
fn task_fields_survive_outside_schema() {
    let text = "nika: v1\ntasks:\n  a:\n    exec:\n      command: [\"ls\"]\n  b:\n    ";
    let got = labels(&completion(text, text.len()));
    assert!(
        got.contains(&"after".to_owned()),
        "the task-field lane is untouched outside schema: {got:?}"
    );
}

// ─── the wave-2 lanes: for_each/when islands · skills list ──────────────────

/// The W2 flow doc — control via `after:`, a complete `for_each:` value
/// (so the doc PARSES and the island lanes ride the parse path; the
/// cursor sits just after `for_each: ` where the typed prefix is still
/// the empty-value position).
const FLOW_DOC: &str = "nika: w\ninputs:\n  urls:\n    type: { array: string }\n    default: []\n  topic: { type: string, required: false, default: \"rust\" }\ntasks:\n  gather:\n    exec:\n      command: [\"ls\"]\n  fan:\n    after: { gather: success }\n    for_each: { items: \"${{ inputs.urls }}\" }\n    exec:\n      command: [\"echo\"]\n  last:\n    after: { fan: success }\n    exec:\n      command: [\"true\"]\n";

#[test]
fn for_each_offers_typed_arrays_first_then_the_boundary_import() {
    // W2: no `tasks.*` form is EVER offered here (NIKA-VAR-021 — the
    // collection is a pre-fan-out LOCAL surface; an upstream array
    // crosses through `with:` first). With no binding declared and
    // upstream present, the lane TEACHES the import instead.
    let at = FLOW_DOC.find("for_each: ").expect("key") + "for_each: ".len();
    let items = completion(FLOW_DOC, at);
    let got = labels(&items);
    assert_eq!(
        got,
        vec![
            "${{ inputs.urls }}",
            "${{ with.items }}",
            "${{ inputs.topic }}"
        ],
        "typed array first · the teaching import · other vars honestly"
    );
    assert!(
        !got.iter().any(|l| l.contains("tasks.")),
        "no tasks.* form — the boundary (VAR-021): {got:?}"
    );
    let teach = items
        .iter()
        .find(|i| i.label == "${{ with.items }}")
        .and_then(|i| i.detail.clone())
        .unwrap_or_default();
    assert!(
        teach.contains("bind the upstream array first"),
        "the teaching names the with: import: {teach}"
    );

    // Once the task DECLARES the binding, the same label rides as the
    // binding itself (the boundary import), not the teaching.
    let bound = FLOW_DOC.replace(
        "    after: { gather: success }\n",
        "    with:\n      items: \"${{ tasks.gather.output }}\"\n",
    );
    let at2 = bound.find("for_each: ").expect("key") + "for_each: ".len();
    let items2 = completion(&bound, at2);
    let binding = items2
        .iter()
        .find(|i| i.label == "${{ with.items }}")
        .and_then(|i| i.detail.clone())
        .unwrap_or_default();
    assert!(
        binding.contains("boundary import"),
        "a declared binding is the collection: {binding}"
    );
}

#[test]
fn when_composes_the_cel_shapes_from_the_document() {
    // W2: the CEL shapes are LOCAL — var switches + the binding null
    // test (the skip-acknowledgement idiom) + the size() empty-check.
    // No tasks.* form appears (status gating lives in `after:`).
    let doc = FLOW_DOC
        .replace(
            "    after: { gather: success }\n",
            "    with:\n      items: \"${{ tasks.gather.output }}\"\n",
        )
        .replace(
            "for_each: { items: \"${{ inputs.urls }}\" }",
            "when: \"${{ with.items != null }}\"",
        );
    let at = doc.find("when: ").expect("key") + "when: ".len();
    let got = labels(&completion(&doc, at));
    assert_eq!(
        got,
        vec![
            "${{ inputs.urls }}",
            "${{ inputs.topic }}",
            "${{ with.items != null }}",
            "${{ size(with.items) > 0 }}"
        ],
        "var switches · the binding null test · the size() check"
    );
    assert!(
        !got.iter().any(|l| l.contains("tasks.")),
        "no tasks.* form in a when: (the boundary · VAR-021): {got:?}"
    );
}

#[test]
fn a_partial_non_island_value_stays_silent() {
    let doc = FLOW_DOC.replace(
        "for_each: { items: \"${{ inputs.urls }}\" }",
        "for_each: som",
    );
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
    let flow = "nika: v1\ntasks:\n  a:\n    agent:\n      prompt: \"p\"\n      skills: [\"";
    assert!(completion(flow, flow.len()).is_empty());
    let block =
        "nika: v1\ntasks:\n  a:\n    agent:\n      prompt: \"p\"\n      skills:\n        - ";
    assert!(completion(block, block.len()).is_empty());
    // A block item under TOOLS (not skills) keeps its own lane silent-or-
    // catalog — never the skills walk; the ancestor check is exact.
    let other = "nika: v1\ntasks:\n  a:\n    agent:\n      prompt: \"p\"\n      tools:\n        - ";
    let got = labels(&completion(other, other.len()));
    assert!(
        !got.iter().any(|l| l.ends_with("SKILL.md")),
        "tools block items never route to the skills walk: {got:?}"
    );
}

// ─── the block-keyset door: child keys by the parser's own vocabulary ───

/// A key position inside `retry:` offers EXACTLY the parser's
/// `RETRY_KEYS` — the door returns the const by reference, so the count
/// is the parser's own (born-stale, key edition).
#[test]
fn retry_block_offers_exactly_the_parser_keyset() {
    let text = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n    retry:\n      ";
    let items = completion(text, text.len());
    let expected = nika_schema::types::keys::known_child_keys("retry", None).expect("keyset");
    assert_eq!(items.len(), expected.len());
    let got = labels(&items);
    assert!(got.contains(&"max_attempts:".to_owned()), "{got:?}");
    assert!(got.contains(&"backoff_strategy:".to_owned()), "{got:?}");
    assert!(
        items
            .iter()
            .all(|i| i.detail.as_deref() == Some("`retry:` field")),
        "the detail names the block"
    );
}

/// The verb bodies switch registers too — `infer:` children are the
/// spec's field table, not the task keys.
#[test]
fn infer_block_offers_the_verb_fields() {
    let text = "nika: w\ntasks:\n  a:\n    infer:\n      ";
    let got = labels(&completion(text, text.len()));
    assert!(got.contains(&"prompt:".to_owned()), "{got:?}");
    assert!(got.contains(&"thinking:".to_owned()), "{got:?}");
    assert!(
        !got.contains(&"depends_on:".to_owned()),
        "task keys stay out of the verb body: {got:?}"
    );
}

/// `permits:` at the workflow level — and `fs:` switches only UNDER
/// permits (an `fs:` block elsewhere is not the door's business).
#[test]
fn permits_and_fs_route_by_parent() {
    let text = "nika: w\npermits:\n  ";
    let got = labels(&completion(text, text.len()));
    assert_eq!(
        got,
        vec!["fs:", "net:", "exec:", "tools:", "env:"],
        "{got:?}"
    );

    let text2 = "nika: w\npermits:\n  fs:\n    ";
    let got2 = labels(&completion(text2, text2.len()));
    assert_eq!(got2, vec!["read:", "write:"], "{got2:?}");
}

/// Free-form maps (`args:` of an MCP tool · `with:`) miss the door —
/// no invented keys.
#[test]
fn free_form_maps_stay_free() {
    let text =
        "nika: w\ntasks:\n  a:\n    invoke:\n      tool: github.search\n      args:\n        ";
    let got = labels(&completion(text, text.len()));
    assert!(
        !got.iter().any(|l| l == "prompt:" || l == "max_attempts:"),
        "no keyset leak into a free-form map: {got:?}"
    );
}

/// `${{ with.` offers the ENCLOSING task's own aliases only — spec 04:
/// `with` is task-local, another task's aliases are out of scope.
#[test]
fn with_island_offers_the_enclosing_tasks_aliases() {
    let text = "nika: w\ntasks:\n  a:\n    with:\n      other: 1\n    exec: { command: [\"x\"] }\n  b:\n    after: { a: success }\n    with:\n      article: \"${{ tasks.a.output }}\"\n      limit: 5\n    exec: { command: [\"echo\", \"${{ with.article }}\", \"${{ with.\"] }\n";
    let cursor = text.rfind("${{ with.").expect("island") + "${{ with.".len();
    let got = labels(&completion(text, cursor));
    assert_eq!(
        got,
        vec!["article", "limit"],
        "b's aliases, never a's: {got:?}"
    );
}

/// The loop-scoped pair rides ONLY under a `for_each:` task (spec 04
/// §loop-scoped locals · #574): outside one, `item`/`index` would
/// complete references the run cannot resolve.
#[test]
fn loop_roots_are_gated_to_for_each_tasks() {
    let plain = "nika: w\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ ";
    let got = labels(&completion(plain, plain.len()));
    assert!(
        !got.contains(&"item".to_owned()),
        "no item outside fan-out: {got:?}"
    );
    assert!(!got.contains(&"index".to_owned()), "{got:?}");

    let fanned = "nika: w\ninputs:\n  urls: [1, 2]\ntasks:\n  a:\n    for_each: { items: \"${{ inputs.urls }}\" }\n    exec: { command: [\"echo\", \"${{ ";
    let got = labels(&completion(fanned, fanned.len()));
    assert!(got.contains(&"item".to_owned()), "{got:?}");
    assert!(
        got.contains(&"index".to_owned()),
        "the missing twin joins: {got:?}"
    );
}
