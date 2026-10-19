// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::record::{TaskRecord, TaskStatus, TerminalCause};

fn record(status: TaskStatus, output: Value) -> TaskRecord {
    let cause = match status {
        TaskStatus::Success => TerminalCause::Normal,
        TaskStatus::Failure => TerminalCause::VerbError,
        TaskStatus::Cancelled => TerminalCause::Upstream,
        TaskStatus::Skipped => TerminalCause::Gate,
    };
    let mut rec = TaskRecord::unran(status, cause);
    rec.output = output;
    rec
}

fn fixture() -> (BTreeMap<String, TaskRecord>, BTreeMap<String, Value>) {
    let records = BTreeMap::from([
        (
            "gather".to_owned(),
            record(TaskStatus::Success, serde_json::json!({"a": 1})),
        ),
        (
            "ghosted".to_owned(),
            record(TaskStatus::Skipped, Value::Null),
        ),
    ]);
    let vars = BTreeMap::from([
        ("publish".to_owned(), Value::String("no".to_owned())),
        ("source".to_owned(), Value::String("./news.json".to_owned())),
        ("urls".to_owned(), serde_json::json!(["a", "b"])),
    ]);
    (records, vars)
}

/// A payment release's gate message, its values bound under `with:`.
fn release_scope_values() -> BTreeMap<String, Value> {
    let request = serde_json::json!({"sno": 7, "item": "Laptop batch",
            "description": "Q4 refresh", "requested_by": "ana", "amount": 1250.5});
    BTreeMap::from([
        ("request".to_owned(), request),
        ("to".to_owned(), Value::from("finance@example.test")),
        ("credential".to_owned(), Value::from("[secret:token]")),
    ])
}

const RELEASE: &str = "Release #${{ with.request.sno }} to ${{ with.to }}\n  \
        item: ${{ with.request.item }}\n  description: ${{ with.request.description }}\n  \
        requested_by: ${{ with.request.requested_by }}\n  amount: ${{ with.request.amount }}";

/// Each island's typed value, by the last name its reference reads, equal to what the
/// message renders; a value the secret rule recognizes and an island that does not resolve
/// are withheld and named.
#[test]
fn a_template_shows_each_value_it_renders_and_names_what_it_withholds() {
    let (records, inputs, with_ns) = (BTreeMap::new(), BTreeMap::new(), release_scope_values());
    let scope =
        Scope::workflow(&records, &inputs).with_task_context(Some(&with_ns), None, None, None);
    let message = format!("{RELEASE}\n  via ${{{{ with.credential }}}} ${{{{ with.missing }}}}");
    let secret = |text: &str| text.contains("[secret:");
    let shown = shown(&message, &scope, (1024, 4096), &secret);
    let expected = serde_json::json!({"sno": 7, "to": "finance@example.test",
            "item": "Laptop batch", "description": "Q4 refresh", "requested_by": "ana",
            "amount": 1250.5});
    assert_eq!(Value::Object(shown.values.clone()), expected);
    let rendered = render(RELEASE, &scope).expect("renders");
    for value in shown.values.values() {
        assert!(
            rendered.contains(&render_value(value)),
            "{value} is shown as rendered"
        );
    }
    let withheld = BTreeMap::from([
        ("credential".to_owned(), "secret".to_owned()),
        ("missing".to_owned(), "unresolved".to_owned()),
    ]);
    assert_eq!(shown.withheld, withheld);
}

/// A value past the bound is withheld as `size`; two references ending in one name keep
/// apart, the later one by its own text; a template that does not scan shows nothing.
#[test]
fn a_shown_value_keeps_its_bound_and_its_own_key() {
    let (records, inputs, with_ns) = (BTreeMap::new(), BTreeMap::new(), release_scope_values());
    let scope =
        Scope::workflow(&records, &inputs).with_task_context(Some(&with_ns), None, None, None);
    let bounded = shown(RELEASE, &scope, (8, 4096), &|_| false);
    let mut kept: Vec<_> = bounded.values.keys().cloned().collect();
    kept.sort();
    assert_eq!(kept, ["amount", "requested_by", "sno"]);
    let cut: Vec<_> = (bounded.withheld.iter())
        .map(|(k, why)| format!("{k}:{why}"))
        .collect();
    assert_eq!(cut, ["description:size", "item:size", "to:size"]);
    let all = shown(RELEASE, &scope, (1024, 2), &|_| false);
    assert_eq!(all.values.keys().collect::<Vec<_>>(), ["sno"]);
    let ids = BTreeMap::from([
        ("a".to_owned(), serde_json::json!({"id": 1})),
        ("b".to_owned(), serde_json::json!({"id": 2})),
    ]);
    let both = Scope::workflow(&records, &inputs).with_task_context(Some(&ids), None, None, None);
    let twice = shown(
        "${{ with.a.id }} ${{ with.b.id }}",
        &both,
        (64, 64),
        &|_| false,
    );
    let expected = serde_json::json!({"id": 1, "with.b.id": 2});
    assert_eq!(Value::Object(twice.values), expected);
    assert_eq!(
        shown("${{ with.to", &scope, (64, 64), &|_| false),
        Shown::default()
    );
}

#[test]
fn render_resolves_vars_and_task_outputs() {
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    let out = render(
        "read ${{ inputs.source }} got ${{ tasks.gather.output }}",
        &scope,
    )
    .expect("renders");
    // Containers render as canonical compact JSON (spec 04).
    assert_eq!(out, "read ./news.json got {\"a\":1}");
}

#[test]
fn render_resolves_a_deployment_supplied_input() {
    let (records, mut inputs) = fixture();
    inputs.insert(
        "API_BASE".to_owned(),
        Value::String("https://odin.example".to_owned()),
    );
    let scope =
        Scope::workflow_with_value_authorities(&records, &inputs, &EMPTY_ENV, &EMPTY_SECRETS);
    assert_eq!(
        render("${{ inputs.API_BASE }}", &scope).expect("the input resolves"),
        "https://odin.example"
    );
    // The dead `config` root answers NOTHING — an empty namespace
    // object would have resolved where no authority lives.
    assert!(render("${{ config.API_BASE }}", &scope).is_err());
}

#[test]
fn const_namespace_resolves_from_its_own_authority() {
    let records = BTreeMap::new();
    let inputs = BTreeMap::from([("region".to_owned(), Value::String("input".to_owned()))]);
    let consts = BTreeMap::from([("region".to_owned(), Value::String("eu-west".to_owned()))]);
    let scope = Scope::workflow_with_value_authorities(&records, &inputs, &consts, &EMPTY_SECRETS);
    assert_eq!(
        render("${{ const.region }}", &scope).expect("constant resolves"),
        "eu-west"
    );
}

#[test]
fn render_record_fields_status_and_defined_null() {
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    assert_eq!(
        render("${{ tasks.gather.status }}", &scope).expect("status"),
        "success"
    );
    // Defined-null (spec 04): a skipped task's output reads as null.
    assert_eq!(
        render("v=${{ tasks.ghosted.output }}", &scope).expect("null read"),
        "v=null"
    );
    assert_eq!(
        render("${{ tasks.gather.error }}", &scope).expect("error of non-failure"),
        "null"
    );
}

#[test]
fn render_unknown_reference_is_1702() {
    // CEL milestone: an unknown reference (NIKA-VAR-001) maps to
    // NIKA-1702 · the `reference` carried is the AUTHOR's text.
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    assert!(matches!(
        render("${{ tasks.ghost.output }}", &scope).expect_err("unknown task"),
        DataflowError::UnresolvedTemplate { ref reference } if reference == "tasks.ghost.output"
    ));
    assert!(matches!(
        render("${{ inputs.nope }}", &scope).expect_err("unknown var"),
        DataflowError::UnresolvedTemplate { .. }
    ));
    // A deep path INTO an unknown task is still 1702 (the root id
    // doesn't resolve · CEL's `.field` then raises VAR-001).
    assert!(matches!(
        render("${{ tasks.ghost.output.x }}", &scope).expect_err("unknown deep"),
        DataflowError::UnresolvedTemplate { .. }
    ));
    // `env` is a valid CEL root but UNBOUND → unresolved (1702 · loud).
    // `secrets` with no resolver injected is unbound too (the fixture
    // scope carries an empty secrets map · fail-closed).
    assert!(matches!(
        render("${{ env.HOME }}", &scope).expect_err("env unbound"),
        DataflowError::UnresolvedTemplate { .. }
    ));
    assert!(matches!(
        render("${{ secrets.api_key }}", &scope).expect_err("secrets unbound"),
        DataflowError::UnresolvedTemplate { .. }
    ));
    assert!(matches!(
        render("${{ secrets }}", &scope).expect_err("empty secrets root unbound"),
        DataflowError::UnresolvedTemplate { .. }
    ));
}

/// The facts of the run (spec 04 §`run`): `${{ run.<fact> }}` reads the one
/// object the engine bound, in a value and in a condition; a scope with none
/// bound leaves `run` unresolved (1702 · loud), never a guessed instant.
#[test]
fn the_run_facts_resolve_where_the_engine_bound_them() {
    let (records, inputs) = fixture();
    let facts = serde_json::json!({"scheduled_for": "2026-10-16T09:00:00.000Z", "missed": null});
    let scope = Scope::workflow(&records, &inputs).with_run(Some(&facts));
    assert_eq!(
        render("day ${{ run.scheduled_for }}", &scope).expect("bound"),
        "day 2026-10-16T09:00:00.000Z"
    );
    assert_eq!(
        scope
            .resolve_expr("run.missed == null")
            .expect("a condition"),
        Value::Bool(true)
    );
    let unbound = Scope::workflow(&records, &inputs);
    assert!(matches!(
        render("${{ run.scheduled_for }}", &unbound).expect_err("nothing bound"),
        DataflowError::UnresolvedTemplate { .. }
    ));
}

/// The fan-in fold (spec 03 §group): `${{ group.<name> }}` resolves
/// to ONE array of member records in DECLARATION order — the source
/// order of `tasks:`, which is NOT the records map's key order
/// (`leg_b` is declared before `leg_a` here) and never completion
/// order. Each record is the closed v1 set `id · status · output ·
/// duration_ms · error`, read defined-null.
#[test]
fn group_fold_resolves_member_records_in_declaration_order() {
    let records = BTreeMap::from([
        ("leg_a".to_owned(), record(TaskStatus::Skipped, Value::Null)),
        (
            "leg_b".to_owned(),
            record(TaskStatus::Success, serde_json::json!({"exit_code": 0})),
        ),
    ]);
    let inputs = BTreeMap::new();
    let groups = BTreeMap::from([(
        "probes".to_owned(),
        vec!["leg_b".to_owned(), "leg_a".to_owned()],
    )]);
    let scope = Scope::workflow(&records, &inputs).with_groups(&groups);
    let fold = scope
        .resolve_expr("group.probes")
        .expect("the fold resolves");
    assert_eq!(
        fold,
        serde_json::json!([
            {"id": "leg_b", "status": "success", "output": {"exit_code": 0}, "duration_ms": null, "error": null},
            {"id": "leg_a", "status": "skipped", "output": null, "duration_ms": null, "error": null},
        ])
    );
    // CEL owns the navigation on top of the fold, as over `tasks`.
    assert_eq!(
        render("${{ size(group.probes) }}", &scope).expect("size"),
        "2"
    );
    assert_eq!(
        render("${{ group.probes[1].id }}", &scope).expect("index step"),
        "leg_a"
    );
    // The record set is CLOSED at v1: no `cause`, no stamps, no
    // `output:` named bindings ride along.
    let mut keys: Vec<&str> = fold[0]
        .as_object()
        .expect("a member record")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["duration_ms", "error", "id", "output", "status"]);
}

/// No membership bound (every scope but a `with:` render) → the root
/// is unbound → NIKA-1702, exactly like an unknown root — never an
/// empty array where the checker said a fold lives.
#[test]
fn group_fold_without_bound_membership_is_1702() {
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    assert!(matches!(
        render("${{ group.probes }}", &scope).expect_err("unbound root"),
        DataflowError::UnresolvedTemplate { ref reference } if reference == "group.probes"
    ));
}

/// A fold never harvests a PARTIAL set: a member without a settled
/// record (a schedule breach — the waves order members first) leaves
/// its group out, so the read is loud, not a silently smaller array;
/// an empty membership is the same fact as an absent one (spec 03).
/// A sibling group whose every member settled still folds.
#[test]
fn group_fold_with_an_unsettled_or_empty_membership_is_1702() {
    let (records, vars) = fixture();
    let groups = BTreeMap::from([
        (
            "probes".to_owned(),
            vec!["gather".to_owned(), "ghost".to_owned()],
        ),
        ("nobody".to_owned(), Vec::new()),
        ("whole".to_owned(), vec!["gather".to_owned()]),
    ]);
    let scope = Scope::workflow(&records, &vars).with_groups(&groups);
    assert!(matches!(
        render("${{ group.probes }}", &scope).expect_err("an unsettled member"),
        DataflowError::UnresolvedTemplate { ref reference } if reference == "group.probes"
    ));
    assert!(matches!(
        render("${{ group.nobody }}", &scope).expect_err("an empty group"),
        DataflowError::UnresolvedTemplate { .. }
    ));
    assert_eq!(
        render("${{ group.whole[0].id }}", &scope).expect("the whole group folds"),
        "gather"
    );
}

#[test]
fn secrets_namespace_resolves_when_bound() {
    // MINOR-B: when the composer resolved the `secrets:` namespace, a
    // `${{ secrets.X }}` reference reads its value (DATA · injection-safe
    // · the same binding boundary as every other namespace).
    let records = BTreeMap::new();
    let vars = BTreeMap::new();
    let secrets = BTreeMap::from([(
        "api_key".to_owned(),
        Value::String("sk-resolved".to_owned()),
    )]);
    let scope = Scope::workflow_with_secrets(&records, &vars, &secrets);
    assert_eq!(
        render("Bearer ${{ secrets.api_key }}", &scope).expect("resolves"),
        "Bearer sk-resolved"
    );
    // An UNDECLARED secret is still unresolved (1702 · fail-closed).
    assert!(matches!(
        render("${{ secrets.absent }}", &scope).expect_err("absent secret"),
        DataflowError::UnresolvedTemplate { .. }
    ));
    // A secret value that LOOKS like an expression is DATA, never
    // re-evaluated (injection-safe · the load-bearing invariant).
    let evil = BTreeMap::from([(
        "x".to_owned(),
        Value::String("${{ inputs.admin == true }}".to_owned()),
    )]);
    let evil_scope = Scope::workflow_with_secrets(&records, &vars, &evil);
    assert_eq!(
        render("${{ secrets.x }}", &evil_scope).expect("data"),
        "${{ inputs.admin == true }}",
        "a secret value is bound as data, never re-parsed"
    );
}

#[test]
fn render_deep_paths_and_indexing_resolve() {
    // The CEL milestone supersedes the v0 "deep path = 1703" rule:
    // `tasks.<id>.output.<field>` and `[i]` now RESOLVE.
    let records = BTreeMap::from([(
        "gather".to_owned(),
        record(
            TaskStatus::Success,
            serde_json::json!({"a": 1, "nested": {"b": "deep"}, "list": [10, 20]}),
        ),
    )]);
    let vars = BTreeMap::from([("urls".to_owned(), serde_json::json!(["x", "y"]))]);
    let scope = Scope::workflow(&records, &vars);
    // Deep object path.
    assert_eq!(
        render("${{ tasks.gather.output.a }}", &scope).expect("deep a"),
        "1"
    );
    assert_eq!(
        render("${{ tasks.gather.output.nested.b }}", &scope).expect("deeper"),
        "deep"
    );
    // Index access on a deep array + a flat var array.
    assert_eq!(
        render("${{ tasks.gather.output.list[1] }}", &scope).expect("deep index"),
        "20"
    );
    assert_eq!(
        render("${{ inputs.urls[0] }}", &scope).expect("flat index"),
        "x"
    );
}

#[test]
fn render_unknown_field_and_grammar_errors_are_loud() {
    // A missing FIELD on a resolved object is unresolved (1702 ·
    // VAR-001) · a genuine GRAMMAR violation is 1703 (VAR-005).
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    // `tasks.gather.output` is {"a":1}; `.weird` is absent → 1702.
    assert!(matches!(
        render("${{ tasks.gather.output.weird }}", &scope).expect_err("missing field"),
        DataflowError::UnresolvedTemplate { .. }
    ));
    // A genuinely malformed expression → 1703 (out of grammar).
    assert!(matches!(
        render("${{ inputs.a && }}", &scope).expect_err("malformed"),
        DataflowError::WhenUnsupported { .. }
    ));
    assert!(matches!(
        render("${{ inputs.a @ vars.b }}", &scope).expect_err("bad token"),
        DataflowError::WhenUnsupported { .. }
    ));
}

#[test]
fn render_dangling_island_is_loud() {
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    render("oops ${{ inputs.source", &scope).expect_err("dangling island");
}

#[test]
fn render_never_rescans_injected_values() {
    // A task output carrying a literal `${{` is DATA · single-pass
    // template scan must pass it through untouched.
    let records = BTreeMap::from([(
        "t".to_owned(),
        record(
            TaskStatus::Success,
            Value::String("${{ not.a.ref }}".into()),
        ),
    )]);
    let vars = BTreeMap::new();
    let scope = Scope::workflow(&records, &vars);
    let out = render("v=${{ tasks.t.output }}", &scope).expect("renders");
    assert_eq!(out, "v=${{ not.a.ref }}");
}

#[test]
fn render_honors_backslash_escape() {
    // spec 04 §Escaping ("the engine MUST honor this") + conformance
    // fixture core/variables/010-valid-escaped-literal: `\${{ ref }}`
    // renders a LITERAL `${{ ref }}` (backslash consumed · the inner
    // reference NEVER resolved). Previously the runtime kept the `\`
    // and resolved the ref — a check⇄runtime drift, since the static
    // analyzer already skips escaped openers (nika-schema template scan).
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);

    // The fixture-010 documentation case: `vars.x` is UNKNOWN, but the
    // escape means it is never looked up — so this renders cleanly
    // (pre-fix it raised NIKA-1702 on the undefined reference).
    assert_eq!(
        render("The syntax \\${{ inputs.x }} references variables.", &scope)
            .expect("escaped ref is documentation text, not resolved"),
        "The syntax ${{ inputs.x }} references variables."
    );

    // The escape wins even when the reference WOULD resolve — a literal
    // `${{` is the author's intent, never a substitution.
    assert_eq!(
        render("literal \\${{ inputs.source }}", &scope).expect("literal"),
        "literal ${{ inputs.source }}"
    );

    // Escaped + real island mix: the escape stays literal, the real
    // island resolves (mirrors the static analyzer's mixed-scan test).
    assert_eq!(
        render("esc \\${{ a }} real ${{ inputs.source }}", &scope).expect("mix"),
        "esc ${{ a }} real ./news.json"
    );

    // SECURITY: an injected value that CONTAINS `\${{` is DATA — it is
    // pushed out, never re-scanned, so the escape is NOT applied to it
    // (the single-pass injection-safe boundary is unchanged).
    let injected = BTreeMap::from([(
        "t".to_owned(),
        record(TaskStatus::Success, Value::String("\\${{ x }}".into())),
    )]);
    let iscope = Scope::workflow(&injected, &vars);
    assert_eq!(
        render("${{ tasks.t.output }}", &iscope).expect("injected data"),
        "\\${{ x }}",
        "an injected backslash-opener is verbatim data, never escape-processed"
    );
}

#[test]
fn shared_scanner_is_wired_and_quote_aware() {
    // Smoke that the runtime resolves islands through the ONE shared lexer
    // (exhaustively tested in nika-tmpl); a `}}` inside a literal is body.
    assert_eq!(nika_tmpl::find_island_close(" vars.x }}", 0), Some(8));
    assert_eq!(nika_tmpl::find_island_close(" '}}' }}", 0), Some(6));
    assert_eq!(nika_tmpl::find_island_close(" \"}}\" }}", 0), Some(6));
    assert_eq!(nika_tmpl::find_island_close(" no real close ", 0), None);
}

#[test]
fn structural_error_precedes_semantic_on_doubly_malformed_template() {
    // Gate-11 (rust-pro P1 · ACCEPTED as intentional · pinned): a template
    // with BOTH a resolvable-but-erroring island AND a later UNTERMINATED
    // opener now surfaces the STRUCTURAL fault first — `render` pre-scans via
    // `nika_tmpl::scan_islands`, which errors on the unterminated opener
    // before any island is resolved. Structural-before-semantic is standard
    // parser discipline; both are hard errors either way, and a passed
    // `nika check` never reaches this (the checker rejects unterminated
    // islands). This locks the ordering as intentional, not accidental.
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    // island #1 `vars.nope` is unknown (a semantic fault); island #2 is
    // unterminated (a structural fault). The structural fault wins, and the
    // surfaced reference is the DANGLING body — not `vars.nope`.
    assert!(
        matches!(
            render("${{ inputs.nope }} ${{ dangling", &scope)
                .expect_err("doubly-malformed template must error"),
            DataflowError::UnresolvedTemplate { ref reference } if reference == "dangling"
        ),
        "the structural (unterminated) fault must win over the earlier semantic one"
    );
}

#[test]
fn render_close_is_quote_aware() {
    // check⇄runtime drift (sister to render_honors_backslash_escape): a
    // `}}` INSIDE a CEL string literal must NOT close the island early.
    // The static analyzer scans quote-aware (nika-schema template tests);
    // the runtime's naive `find("}}")` truncated `${{ … "}}" … }}`
    // mid-literal → a parse error on a template that checked clean.
    let (records, vars) = fixture(); // vars.source == "./news.json"
    let scope = Scope::workflow(&records, &vars);

    // RHS string literal carrying `}}` resolves (island not truncated).
    assert_eq!(
        render("${{ inputs.source == '}}' }}", &scope).expect("compare vs }} literal"),
        "false"
    );
    // `.contains('}}')` — the literal `}}` is method-arg data, not a close.
    assert_eq!(
        render("${{ inputs.source.contains('}}') }}", &scope).expect("contains }}"),
        "false"
    );
    // The REAL close (after the literal) is found, so a trailing real
    // island still scans: a literal-bearing island, then a normal one.
    assert_eq!(
        render(
            "${{ inputs.source != '}}' }} :: ${{ inputs.source }}",
            &scope
        )
        .expect("literal-island then real island"),
        "true :: ./news.json"
    );
}

#[test]
fn injected_value_is_cel_data_never_re_evaluated() {
    // SECURITY (the load-bearing invariant): a task output / var that
    // CONTAINS expression-looking text is bound as a typed CEL DATA
    // value · the author's expression is parsed once · the injected
    // string is NEVER re-parsed as an expression. A payload that
    // WOULD resolve to something else (or trip a guard / inject a
    // reference) if re-evaluated must come back as the literal bytes.
    let payload = "${{ inputs.secret_admin == true }}"; // attacker-controlled
    let records = BTreeMap::from([(
        "evil".to_owned(),
        record(TaskStatus::Success, Value::String(payload.into())),
    )]);
    let vars = BTreeMap::from([
        (
            "tainted".to_owned(),
            Value::String("'; DROP ${{ x }}".into()),
        ),
        ("secret_admin".to_owned(), serde_json::json!(false)),
    ]);
    let scope = Scope::workflow(&records, &vars);

    // render(): the island resolves to the OUTPUT VALUE (the payload
    // string verbatim) · the `${{` inside is data, not re-scanned.
    assert_eq!(
        render("${{ tasks.evil.output }}", &scope).expect("data"),
        payload,
        "an injected `${{` payload renders as the literal value"
    );

    // render_json() single-island: same · the value is type-preserved
    // and NOT re-evaluated (it does NOT become a bool from the payload).
    let v = render_json(&serde_json::json!("${{ tasks.evil.output }}"), &scope).expect("data");
    assert_eq!(v, Value::String(payload.to_owned()));

    // eval_when(): a gate that READS the tainted/payload value as DATA
    // (string comparison) evaluates against the literal bytes — the
    // embedded `${{ x }}` is never resolved, so the equality is honest.
    assert!(
        eval_when(
            "${{ tasks.evil.output == \"${{ inputs.secret_admin == true }}\" }}",
            &scope
        )
        .expect("compares the literal payload bytes"),
        "the gate compares the payload as DATA · the inner island is never evaluated"
    );
    // And a tainted var compared as data is a plain (false) string compare,
    // never a re-parse of its `${{ x }}` fragment.
    assert!(!eval_when("${{ inputs.tainted == 'admin' }}", &scope).expect("string compare"),);
}

#[test]
fn render_json_for_each_deep_path_array_resolves() {
    // `for_each: ${{ tasks.files.output }}` over a deep-path array
    // (the bug: a deep-path array resolved to a string → VAR-006).
    // render_json single-island preserves the ARRAY type.
    let records = BTreeMap::from([(
        "files".to_owned(),
        record(
            TaskStatus::Success,
            serde_json::json!({"paths": ["a.md", "b.md", "c.md"]}),
        ),
    )]);
    let vars = BTreeMap::new();
    let scope = Scope::workflow(&records, &vars);
    // A flat task-output array.
    let flat = BTreeMap::from([(
        "list".to_owned(),
        record(TaskStatus::Success, serde_json::json!(["x", "y"])),
    )]);
    let flat_scope = Scope::workflow(&flat, &vars);
    assert_eq!(
        render_json(&serde_json::json!("${{ tasks.list.output }}"), &flat_scope)
            .expect("flat array"),
        serde_json::json!(["x", "y"])
    );
    // A DEEP-path array stays an array (type-preserving · the fan-out
    // collection relies on this).
    assert_eq!(
        render_json(
            &serde_json::json!("${{ tasks.files.output.paths }}"),
            &scope
        )
        .expect("deep array"),
        serde_json::json!(["a.md", "b.md", "c.md"])
    );
}

#[test]
fn item_field_access_resolves_in_for_each_body() {
    // The t3-localization shape: `${{ item.path }}` / `${{ item.text }}`
    // — for_each locals are objects too · field access works.
    let (records, vars) = fixture();
    let item = serde_json::json!({"path": "docs/intro.md", "text": "Hello"});
    let scope = Scope::workflow_with_value_authorities(&records, &vars, &EMPTY_ENV, &EMPTY_SECRETS)
        .with_task_context(None, Some(&item), Some(0), None);
    assert_eq!(
        render("${{ item.path }}", &scope).expect("item.path"),
        "docs/intro.md"
    );
    assert_eq!(
        render("${{ item.text }}", &scope).expect("item.text"),
        "Hello"
    );
}

#[test]
fn ternary_value_substitution_resolves() {
    // A ternary in a VALUE position (model/prompt/path selection).
    let vars = BTreeMap::from([("env".to_owned(), Value::String("prod".into()))]);
    let records = BTreeMap::new();
    let scope = Scope::workflow(&records, &vars);
    assert_eq!(
        render(
            "${{ inputs.env == 'prod' ? 'anthropic/claude' : 'ollama/llama3' }}",
            &scope
        )
        .expect("ternary value"),
        "anthropic/claude"
    );
    // has() guard form (the `prompt:` default-fallback idiom).
    assert_eq!(
        render("${{ has(vars.style) ? vars.style : 'be concise' }}", &scope).expect("has fallback"),
        "be concise"
    );
}

#[test]
fn render_json_single_island_preserves_value_type() {
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    // Exactly-one-island string leaf → the VALUE (array stays array).
    let v = render_json(&serde_json::json!("${{ inputs.urls }}"), &scope).expect("renders");
    assert_eq!(v, serde_json::json!(["a", "b"]));
    // Mixed text → textual render.
    let v = render_json(&serde_json::json!("x ${{ inputs.source }}"), &scope).expect("renders");
    assert_eq!(v, serde_json::json!("x ./news.json"));
    // Whitespace around the island is text too: the newline after a value is kept, and a value
    // between spaces renders as its text between them.
    let v = render_json(&serde_json::json!("${{ inputs.source }}\n"), &scope).expect("renders");
    assert_eq!(v, serde_json::json!("./news.json\n"));
    let v = render_json(&serde_json::json!(" ${{ inputs.urls }}\n\n"), &scope).expect("renders");
    assert_eq!(v, serde_json::json!(" [\"a\",\"b\"]\n\n"));
    // Nested leaves resolve · non-strings untouched.
    let v = render_json(
        &serde_json::json!({"path": "${{ inputs.source }}", "n": [7, true]}),
        &scope,
    )
    .expect("renders");
    assert_eq!(
        v,
        serde_json::json!({"path": "./news.json", "n": [7, true]})
    );
}

#[test]
fn with_and_for_each_locals_resolve() {
    let (records, vars) = fixture();
    let with_ns = BTreeMap::from([("page".to_owned(), Value::String("p1".into()))]);
    let item = Value::String("element".to_owned());
    let scope = Scope::workflow_with_value_authorities(&records, &vars, &EMPTY_ENV, &EMPTY_SECRETS)
        .with_task_context(Some(&with_ns), Some(&item), Some(3), None);
    assert_eq!(
        render("${{ with.page }}/${{ item }}@${{ index }}", &scope).expect("renders"),
        "p1/element@3"
    );
    // Outside an iteration the locals are unknown (1702).
    let bare = Scope::workflow(&records, &vars);
    assert!(matches!(
        render("${{ item }}", &bare).expect_err("no item outside for_each"),
        DataflowError::UnresolvedTemplate { .. }
    ));
}

#[test]
fn scope_accessors_preserve_all_bound_authorities_and_context() {
    let (records, inputs) = fixture();
    let consts = BTreeMap::from([("region".to_owned(), serde_json::json!("eu"))]);
    let secrets = BTreeMap::from([("token".to_owned(), serde_json::json!("resolved"))]);
    let with_ns = BTreeMap::from([("page".to_owned(), serde_json::json!(2))]);
    let item = serde_json::json!({"id": 7});
    let permits = Permits::new();
    let scope = Scope::workflow_with_value_authorities(&records, &inputs, &consts, &secrets)
        .with_task_context(Some(&with_ns), Some(&item), Some(4), Some(&permits));

    assert_eq!(scope.records().len(), records.len());
    assert!(scope.records().contains_key("gather"));
    assert_eq!(scope.inputs(), &inputs);
    assert_eq!(scope.consts(), &consts);
    assert_eq!(scope.secrets(), &secrets);
    assert_eq!(scope.with_namespace(), Some(&with_ns));
    assert_eq!(scope.item(), Some(&item));
    assert_eq!(scope.index(), Some(4));
    assert_eq!(scope.permits(), Some(&permits));
}

#[test]
fn when_equality_open_and_closed() {
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    assert!(!eval_when("${{ inputs.publish == 'yes' }}", &scope).expect("closed"));
    assert!(eval_when("${{ inputs.publish == 'no' }}", &scope).expect("open"));
    assert!(eval_when("${{ inputs.publish != 'yes' }}", &scope).expect("negated open"));
    assert!(eval_when("inputs.publish == \"no\"", &scope).expect("bare body · double quotes"));
    // Status comparisons (spec 04 · the always-pattern's idiom).
    assert!(eval_when("${{ tasks.gather.status == 'success' }}", &scope).expect("status"));
    assert!(eval_when("${{ tasks.ghosted.status != 'success' }}", &scope).expect("skipped"));
}

#[test]
fn when_full_cel_gate_forms_evaluate() {
    // The CEL milestone: the gate is the full cel-subset/0.1 — not a
    // bare-ref truthiness table. Comparisons (`<` `>=` `!=`), deep
    // boolean paths, `size()`, `.contains()`, booleans, membership,
    // and the ternary all evaluate.
    let records = BTreeMap::from([(
        "check".to_owned(),
        record(
            TaskStatus::Success,
            serde_json::json!({"valid": true, "coverage": 92, "tags": ["a", "b"]}),
        ),
    )]);
    let vars = BTreeMap::from([
        ("env".to_owned(), Value::String("production".into())),
        ("threshold".to_owned(), serde_json::json!(80)),
        ("msg".to_owned(), Value::String("error: boom".into())),
    ]);
    let scope = Scope::workflow(&records, &vars);
    // Deep-path boolean compare (the NIKA-1703 bug this fix closes).
    assert!(eval_when("${{ tasks.check.output.valid == true }}", &scope).expect("deep bool"));
    // Numeric comparisons.
    assert!(eval_when("${{ tasks.check.output.coverage > 80 }}", &scope).expect("> "));
    assert!(eval_when("${{ inputs.threshold >= 80 }}", &scope).expect(">="));
    assert!(eval_when("${{ tasks.check.output.coverage != 0 }}", &scope).expect("!="));
    assert!(!eval_when("${{ tasks.check.output.coverage < 80 }}", &scope).expect("<"));
    // size() + .contains() + membership + boolean ops + ternary.
    assert!(eval_when("${{ size(tasks.check.output.tags) > 0 }}", &scope).expect("size"));
    assert!(eval_when("${{ inputs.msg.contains('error') }}", &scope).expect("contains"));
    assert!(
        eval_when(
            "${{ inputs.env == 'production' && tasks.check.output.valid }}",
            &scope
        )
        .expect("&&")
    );
    assert!(eval_when("${{ inputs.env in ['staging', 'production'] }}", &scope).expect("in"));
    assert!(
        eval_when("${{ inputs.env == 'production' ? true : false }}", &scope).expect("ternary")
    );
    // The bare YAML-boolean idiom (a wrapped `true`/`false` literal).
    assert!(eval_when("${{ true }}", &scope).expect("literal true"));
}

#[test]
fn when_non_boolean_value_is_var_006() {
    // Spec 03 §when shape: a `when:` that evaluates non-boolean is
    // NIKA-VAR-006 (carried as CelEval) — NOT a silent skip, NOT
    // the old string-truthiness. (A non-boolean ROOT is the
    // checker's NIKA-VAR-005; the runtime sees the eval-time class.)
    let vars = BTreeMap::from([
        ("count".to_owned(), serde_json::json!(3)),
        ("name".to_owned(), Value::String("nika".into())),
    ]);
    let records = BTreeMap::new();
    let scope = Scope::workflow(&records, &vars);
    for non_bool in [
        "${{ inputs.count }}",
        "${{ inputs.name }}",
        "${{ size(inputs.name) }}",
    ] {
        let err = eval_when(non_bool, &scope).expect_err(non_bool);
        assert!(
            matches!(&err, DataflowError::CelEval { code, .. } if *code == "NIKA-VAR-006"),
            "{non_bool} → {err}"
        );
    }
    // A cross-type compare is also VAR-006 (no silent `false`).
    let err = eval_when("${{ inputs.count == 'three' }}", &scope).expect_err("cross-type");
    assert!(matches!(
        &err,
        DataflowError::CelEval { code, .. } if *code == "NIKA-VAR-006"
    ));
}

#[test]
fn when_grammar_violation_is_nika_1703() {
    // Genuinely-out-of-grammar forms (NIKA-VAR-005) are 1703 — the
    // runtime backstop (the checker is the primary site). Spec-VALID
    // forms must NEVER reach here (that is the bug this fix closes).
    let (records, vars) = fixture();
    let scope = Scope::workflow(&records, &vars);
    for malformed in [
        "${{ inputs.a < vars.b < vars.c }}", // chained relation
        "${{ frobnicate(vars.x) }}",         // unknown function
        "${{ inputs.a && }}",                // dangling operator
        "${{ inputs.a @ vars.b }}",          // bad token
    ] {
        let err = eval_when(malformed, &scope).expect_err(malformed);
        assert!(
            matches!(err, DataflowError::WhenUnsupported { .. }),
            "{malformed} → {err}"
        );
    }
    // An UNQUOTED right-hand `yes` is a valid IDENT (an unbound root)
    // → unresolved (1702), NOT a grammar error · `yes`/`no` are not
    // reserved words in cel-subset/0.1 (only true/false/null/in are).
    assert!(matches!(
        eval_when("${{ inputs.publish == yes }}", &scope).expect_err("unbound rhs ident"),
        DataflowError::UnresolvedTemplate { .. }
    ));
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        cases: 256,
        ..proptest::test_runner::Config::default()
    })]

    /// Render is total over arbitrary templates + scopes: it returns
    /// Ok or a typed error · never panics · and on Ok the output
    /// carries no unresolved island from the TEMPLATE.
    #[test]
    fn prop_render_total_and_clean(
        template in ".{0,80}",
        key in "[a-z]{1,8}",
        value in ".{0,20}",
    ) {
        let records = BTreeMap::new();
        let vars = BTreeMap::from([(key, Value::String(value))]);
        let scope = Scope::workflow(&records, &vars);
        // A `\${{` escape legitimately renders a LITERAL `${{` (spec 04
        // §Escaping), so the no-leak invariant only holds when the
        // template carries no escape — a real island is always resolved
        // away or errs, so a surviving `${{` then means a silent leak.
        if let Ok(out) = render(&template, &scope)
            && template.contains("${{")
            && !template.contains("\\${{")
        {
            proptest::prop_assert!(!out.contains("${{"));
        }
    }

    /// Canonical rendering is deterministic: same value · same string ·
    /// and JSON-reparseable for containers (round-trip law).
    #[test]
    fn prop_render_value_deterministic_and_reparseable(
        keys in proptest::collection::vec("[a-z]{1,6}", 0..6),
    ) {
        let map: serde_json::Map<String, Value> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), serde_json::json!(i)))
            .collect();
        let v = Value::Object(map);
        let a = render_value(&v);
        let b = render_value(&v);
        proptest::prop_assert_eq!(&a, &b);
        let reparsed: Value = serde_json::from_str(&a).expect("canonical JSON reparses");
        proptest::prop_assert_eq!(reparsed, v);
    }
}
