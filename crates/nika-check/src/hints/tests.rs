use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

fn hints_of(yaml: &str) -> Vec<Hint> {
    scan_hints(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
}

#[test]
fn a_structured_capture_nobody_branches_on_names_what_it_swallows() {
    // Reported 2026-07-28: a run showed 23/23 green with four tasks
    // failed. Under `structured` a non-zero exit is DATA, so the task
    // succeeds whatever the command returns; the error surfaced three
    // waves later on an unrelated jq and pointed at the wrong task.
    let blind = hints_of(
        "nika: w\npermits:\n  exec: true\ntasks:\n  s:\n    exec: { command: [\"false\"], capture: structured }\n",
    );
    let hit = blind
        .iter()
        .find(|h| h.kind == "swallowed-exit")
        .expect("a structured capture nobody branches on names what it swallows");
    assert_eq!(hit.task, "s");
    assert!(
        hit.advice.contains("NIKA-EXEC-001"),
        "the hint names the failure the author gave up: {}",
        hit.advice
    );

    // Reading the branch IS the legitimate use — nothing to say.
    let branched = hints_of(
        "nika: w\npermits:\n  exec: true\ntasks:\n  s:\n    exec: { command: [\"false\"], capture: structured }\n  guard:\n    with:\n      code: ${{ tasks.s.output.exit_code }}\n    exec: { command: [\"true\"] }\n",
    );
    assert!(
        !branched.iter().any(|h| h.kind == "swallowed-exit"),
        "a workflow that reads exit_code is using structured on purpose"
    );
}

#[test]
fn a_reasoning_model_without_a_thinking_budget_gets_the_teaching() {
    // #651 leg 3: the reasoning share lives INSIDE `max_tokens` — a
    // heavy think concludes with a blank visible answer (NIKA-INFER-004
    // at run). The check names the missing declaration before a token
    // is spent. o3 is reasoning-capable in the vendored catalog.
    let hits = hints_of(
        "nika: w\ntasks:\n  t:\n    infer:\n      model: \"openai/o3\"\n      max_tokens: 500\n      prompt: hi\n",
    );
    let hit = hits
        .iter()
        .find(|h| h.kind == "thinking-budget")
        .expect("a reasoning seat with a bare cap gets the teaching");
    assert_eq!(hit.task, "t");
    assert!(hit.advice.contains("thinking:"), "{}", hit.advice);

    // Declared thinking · a no-think model · a templated seat · no cap:
    // nothing to teach in any of the four.
    for yaml in [
        "nika: w\ntasks:\n  t:\n    infer:\n      model: \"openai/o3\"\n      max_tokens: 500\n      thinking: { enabled: true }\n      prompt: hi\n",
        "nika: w\ntasks:\n  t:\n    infer:\n      model: \"openai/gpt-4o-mini\"\n      max_tokens: 500\n      prompt: hi\n",
        "nika: w\ninputs:\n  seat:\n    type: string\n    default: \"openai/o3\"\ntasks:\n  t:\n    infer:\n      model: \"${{ inputs.seat }}\"\n      max_tokens: 500\n      prompt: hi\n",
        "nika: w\ntasks:\n  t:\n    infer:\n      model: \"openai/o3\"\n      prompt: hi\n",
    ] {
        assert!(
            !hints_of(yaml).iter().any(|h| h.kind == "thinking-budget"),
            "no hint here: {yaml}"
        );
    }
}

#[test]
fn a_prompt_without_a_default_names_its_headless_cost() {
    // Reported from Cursor 2026-07-28 (a green audit, then a dead
    // first run) and again live 2026-07-31 (seo-live-review): the
    // oracle was silent about a fact sitting in the file. The hint
    // now teaches the BEHAVIOR (unattended = durable pause) and the
    // two one-command answers, not a code the CLI no longer dies on.
    let bare = hints_of(
        "nika: w\npermits:\n  tools: [\"nika:prompt\"]\ntasks:\n  confirm:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"ship?\" }\n",
    );
    let hit = bare
        .iter()
        .find(|h| h.kind == "headless-prompt")
        .expect("a prompt with no default names its headless cost");
    assert_eq!(hit.task, "confirm");
    for lesson in [
        "pauses at this gate",
        "--answer confirm=<value>",
        "`default:`",
    ] {
        assert!(
            hit.advice.contains(lesson),
            "the hint teaches `{lesson}`: {}",
            hit.advice
        );
    }

    // A declared default IS the unattended path — nothing to say.
    let defaulted = hints_of(
        "nika: w\npermits:\n  tools: [\"nika:prompt\"]\ntasks:\n  confirm:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"ship?\", default: false }\n",
    );
    assert!(
        !defaulted.iter().any(|h| h.kind == "headless-prompt"),
        "a declared default silences the hint"
    );
}

#[test]
fn non_tightening_after_terminal_beside_value_edge_is_redundant() {
    // one-obvious-way/008 — `after: {a: terminal}` beside a value
    // edge to `a` composes to the value edge's own pass-set:
    // {success, skipped} ∩ terminal changes nothing.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { shell: \"true\" }\n  b:\n    after: { a: terminal }\n    with: { data: \"${{ tasks.a.output }}\" }\n    exec: { shell: \"true\" }\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "redundant-gate" && x.task == "b")
        .expect("the /008 hint fires");
    assert!(hit.advice.contains("tighten to `success`"), "{hit:?}");
}

#[test]
fn tightening_after_success_beside_value_edge_is_meaningful() {
    // `success` NARROWS the composed gate ({success, skipped} ∩
    // {success} = {success} — the skipped-null case is excluded), so
    // the restatement is meaningful; the spec's own tightened form
    // (conformance dag-topology/009) must never be flagged.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { shell: \"true\" }\n  b:\n    after: { a: success }\n    with: { data: \"${{ tasks.a.output }}\" }\n    exec: { shell: \"true\" }\n",
    );
    assert!(!h.iter().any(|x| x.kind == "redundant-gate"), "{h:?}");
}

#[test]
fn after_terminal_without_a_value_edge_is_not_flagged() {
    // The two legitimate terminal shapes stay silent · the pure
    // always-pattern (no binding at all) and the report pattern
    // (terminal + a `.status` OBSERVATION — not a value edge, the
    // pairing the spec itself teaches in 03 §after).
    let always = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { shell: \"true\" }\n  b:\n    after: { a: terminal }\n    exec: { shell: \"true\" }\n",
    );
    assert!(
        !always.iter().any(|x| x.kind == "redundant-gate"),
        "{always:?}"
    );
    let report = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { shell: \"true\" }\n  b:\n    after: { a: terminal }\n    with: { outcome: \"${{ tasks.a.status }}\" }\n    exec: { shell: \"true\" }\n",
    );
    assert!(
        !report.iter().any(|x| x.kind == "redundant-gate"),
        "{report:?}"
    );
}

#[test]
fn unbounded_infer_gets_a_cost_hint() {
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\" }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(h.iter().any(|x| x.kind == "cost" && x.task == "a"), "{h:?}");
}

#[test]
fn unconsumed_infer_is_dead_spend() {
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    exec: { shell: \"echo done\" }\n",
    );
    assert!(h.iter().any(|x| x.kind == "dead-spend" && x.task == "a"));
    // consumed via outputs: → no dead-spend hint
    let h2 = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(!h2.iter().any(|x| x.kind == "dead-spend"), "{h2:?}");
}

/// The first-day trap, taught where it is born: `outputs.r: ${{
/// tasks.a }}` binds the ENVELOPE — the hint names the output, the
/// task, the drift, and the fix; the contradictory dead-spend voice
/// (« nothing consumes it ») is suppressed for that task.
#[test]
fn envelope_bound_output_teaches_and_silences_dead_spend() {
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a }}\n",
    );
    let env: Vec<_> = h.iter().filter(|x| x.kind == "envelope-output").collect();
    assert_eq!(env.len(), 1, "{h:?}");
    assert_eq!(env[0].task, "a");
    assert!(env[0].advice.contains("outputs.r"), "{}", env[0].advice);
    assert!(
        env[0].advice.contains("${{ tasks.a.output }}"),
        "the fix is spelled: {}",
        env[0].advice
    );
    assert!(
        !h.iter().any(|x| x.kind == "dead-spend"),
        "one voice — the envelope binding IS consumption: {h:?}"
    );
    // A bare envelope in a GATE is plumbing, not a trap — silent.
    let gate = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    with: { a_out: \"${{ tasks.a.output }}\" }\n    when: ${{ size(with.a_out) > 0 }}\n    exec: { shell: \"echo go\" }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(
        !gate.iter().any(|x| x.kind == "envelope-output"),
        "{gate:?}"
    );
}

#[test]
fn deeply_referenced_unschema_d_output_gets_a_typing_hint() {
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    with: { f: \"${{ tasks.a.output.field }}\" }\n    exec: { shell: \"echo ${{ with.f }}\" }\n",
    );
    assert!(
        h.iter().any(|x| x.kind == "typing" && x.task == "a"),
        "{h:?}"
    );
    // shallow consumption only → no typing hint
    let h2 = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    with: { whole: \"${{ tasks.a.output }}\" }\n    exec: { shell: \"echo ${{ with.whole }}\" }\n",
    );
    assert!(!h2.iter().any(|x| x.kind == "typing"), "{h2:?}");
}

#[test]
fn effectful_without_permits_gets_no_hint_the_error_owns_it() {
    // F-O8: absent + effects is the NIKA-AUTH-006 ERROR (the
    // capability_escapes lane), so THIS lane stays silent — an
    // advisory hint next to the hard finding would double-teach.
    let h = hints_of("nika: w\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n");
    assert!(
        !h.iter().any(|x| x.kind == "permits"),
        "the error owns the absent+effects case: {h:?}"
    );
    // boundary declared → no hint either (unchanged).
    let h2 = hints_of(
        "nika: w\npermits: { exec: true }\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n",
    );
    assert!(!h2.iter().any(|x| x.kind == "permits"), "{h2:?}");
}

#[test]
fn structured_exec_parsing_stdout_json_gets_capture_hint() {
    let h = hints_of(
        "nika: w\npermits: { exec: true }\ntasks:\n  crawl:\n    exec:\n      command: [\"node\", \"helper.mjs\"]\n      capture: structured\n    extract:\n      crawl: \".stdout | fromjson\"\n      url: \".stdout | fromjson | .url\"\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "exec-json-capture" && x.task == "crawl")
        .expect("capture hint");
    assert!(hit.advice.contains("capture: stdout"), "{hit:?}");
    assert!(hit.advice.contains("exit_code"), "{hit:?}");

    let intentional = hints_of(
        "nika: w\npermits: { exec: true }\ntasks:\n  probe:\n    exec:\n      command: [\"false\"]\n      capture: structured\n    extract:\n      exit_code: \".exit_code\"\n",
    );
    assert!(
        !intentional.iter().any(|x| x.kind == "exec-json-capture"),
        "{intentional:?}"
    );

    // The MIXED task — one binding parses stdout JSON, ANOTHER branches on
    // exit_code. `structured` is the point (switching would break `ok`);
    // the hint must stay silent (Gate-11 review: the any-vs-all misfire).
    let mixed = hints_of(
        "nika: w\npermits: { exec: true }\ntasks:\n  health:\n    exec:\n      command: [\"curl\", \"-s\", \"https://api.example/health\"]\n      capture: structured\n    extract:\n      body: \".stdout | fromjson\"\n      ok: \".exit_code == 0\"\n",
    );
    assert!(
        !mixed.iter().any(|x| x.kind == "exec-json-capture"),
        "{mixed:?}"
    );

    // Substring lookalike — the binding CONTAINS both `.stdout` and
    // `fromjson` (the old independent-substring predicate fired) but they
    // never form the `.stdout | fromjson` chain; no hint.
    let lookalike = hints_of(
        "nika: w\npermits: { exec: true }\ntasks:\n  diag:\n    exec:\n      command: [\"node\", \"diag.mjs\"]\n      capture: structured\n    extract:\n      log: \".raw | fromjson | .stdout_field\"\n",
    );
    assert!(
        !lookalike.iter().any(|x| x.kind == "exec-json-capture"),
        "{lookalike:?}"
    );
}

#[test]
fn consumption_inside_nested_with_json_counts() {
    // the output is consumed inside a NESTED `with:` JSON value —
    // the visit_json walker path; with it blinded, a phantom
    // dead-spend hint would fire here.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    with: { payload: { content: \"${{ tasks.a.output }}\" } }\n    invoke: { tool: \"nika:write\", args: { path: \"./o\", content: \"${{ with.payload }}\" } }\n",
    );
    assert!(
        !h.iter().any(|x| x.kind == "dead-spend"),
        "consumed via nested with JSON: {h:?}"
    );
}

#[test]
fn pure_compute_hints_stay_silent_in_this_lane() {
    // infer-only → no permits hint HERE (the legal-zero hint for
    // absent + pure compute is stamped by `check()` itself, which
    // sees the escape scan — this lane's half of the F-O8 split).
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(!h.iter().any(|x| x.kind == "permits"), "{h:?}");
}

#[test]
fn open_object_schema_gets_the_strictness_hint() {
    // properties declared but additionalProperties unclosed → the
    // model can emit undeclared keys → shape varies across providers.
    let open = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          s: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(
        open.iter().any(|h| h.kind == "strictness" && h.task == "a"),
        "{open:?}"
    );
    // closed at every object node → no hint
    let closed = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          s: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(!closed.iter().any(|h| h.kind == "strictness"), "{closed:?}");
}

#[test]
fn nested_open_object_is_found_one_hint_per_task() {
    // the root is closed but a nested items-object is open — still
    // hinted, and only ONCE for the task.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          tags:\n            type: array\n            items:\n              type: object\n              properties:\n                name: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert_eq!(
        h.iter().filter(|x| x.kind == "strictness").count(),
        1,
        "{h:?}"
    );
}

// ─── schema-portability hint · grammar-blind keywords ─────────────

#[test]
fn grammar_blind_keywords_get_the_portability_hint() {
    // uniqueItems:true + not — every provider wire ACCEPTS this
    // schema and no grammar enforces either keyword (llama.cpp +
    // ollama proven live 2026-07-07); the hint names the local-
    // validation-only reality, once per task, listing both.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          tags:\n            type: array\n            uniqueItems: true\n            items:\n              type: string\n              not: { enum: [forbidden] }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "schema-portability")
        .expect("hint");
    assert_eq!(hit.task, "a");
    assert!(
        hit.advice.contains("`uniqueItems`") && hit.advice.contains("`not`"),
        "{hit:?}"
    );
    assert_eq!(
        h.iter().filter(|x| x.kind == "schema-portability").count(),
        1,
        "one hint per task: {h:?}"
    );
}

#[test]
fn conditional_family_flags_only_when_it_binds() {
    // `if` + `then` binds → hinted; a bare `if` without then/else
    // constrains nothing anywhere — not even locally — so no claim.
    let bound = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          x: { type: string }\n        if:\n          properties:\n            x: { const: a }\n        then:\n          required: [x]\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(
        bound
            .iter()
            .any(|x| x.kind == "schema-portability" && x.advice.contains("`if/then/else`")),
        "{bound:?}"
    );
    let bare = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          x: { type: string }\n        if:\n          required: [x]\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(
        !bare.iter().any(|x| x.kind == "schema-portability"),
        "{bare:?}"
    );
}

#[test]
fn portability_hint_reads_keywords_not_property_names() {
    // a property NAMED `not` + `uniqueItems: false` (the default,
    // binds nothing) → silence; the walker reads keys only at
    // schema-node positions.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          not: { type: string }\n          tags:\n            type: array\n            uniqueItems: false\n            items: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
    );
    assert!(!h.iter().any(|x| x.kind == "schema-portability"), "{h:?}");
}

#[test]
fn schema_d_task_gets_no_typing_hint() {
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          field: { type: string }\n  b:\n    with: { f: \"${{ tasks.a.output.field }}\" }\n    exec: { shell: \"echo ${{ with.f }}\" }\n",
    );
    assert!(!h.iter().any(|x| x.kind == "typing"), "{h:?}");
}

#[test]
fn retried_exec_warns_at_least_once_semantics() {
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  deploy:\n    retry: { max_attempts: 3 }\n    exec: { shell: \"./deploy.sh\" }\n",
    );
    let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
    assert_eq!(hit.task, "deploy");
    assert!(hit.advice.contains("at-least-once"), "{hit:?}");
}

#[test]
fn retried_mcp_tool_warns_no_idempotency_contract() {
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  post:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: mcp:slack/send\n      args: { text: \"hi\" }\n",
    );
    let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
    assert!(hit.advice.contains("mcp:slack/send"), "{hit:?}");
}

#[test]
fn retried_notify_webhook_warns_duplicate_side_effect() {
    // P0-17 — `nika:notify` is a webhook send (nika-builtin defs.rs):
    // NOT idempotent, so a retry can deliver the notification twice.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  alert:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:notify\n      args: { target: \"https://hooks.example.com/x\", message: \"boom\" }\n",
    );
    let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
    assert_eq!(hit.task, "alert");
    assert!(hit.advice.contains("nika:notify"), "{hit:?}");
}

#[test]
fn retried_fetch_post_warns_without_idempotency_key() {
    // P0-17 — `nika:fetch` accepts POST (defs.rs · http.rs: « POST must
    // pair an idempotency key »): a bare retried POST replays the
    // request's side effects.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  post:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/items\", method: POST, body: { a: 1 } }\n",
    );
    let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
    assert_eq!(hit.task, "post");
    assert!(hit.advice.contains("POST"), "{hit:?}");
}

#[test]
fn retried_fetch_post_with_idempotency_key_makes_no_claim() {
    // The declared `idempotency-key` header discharges the hazard —
    // the receiver can dedup the replay.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  post:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:fetch\n      args:\n        url: \"https://api.example.com/items\"\n        method: POST\n        headers: { idempotency-key: \"${{ inputs.order_id }}\" }\n        body: { a: 1 }\n",
    );
    assert!(!h.iter().any(|x| x.kind == "retry-effects"), "{h:?}");
}

#[test]
fn retried_fetch_get_makes_no_claim() {
    // GET (explicit OR the default) is idempotent — retrying it
    // replays nothing (http.rs: « GET idempotent »).
    let explicit = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  probe:\n    retry: { max_attempts: 3 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/health\", method: GET }\n",
    );
    assert!(
        !explicit.iter().any(|x| x.kind == "retry-effects"),
        "{explicit:?}"
    );
    let defaulted = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  probe:\n    retry: { max_attempts: 3 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/health\" }\n",
    );
    assert!(
        !defaulted.iter().any(|x| x.kind == "retry-effects"),
        "{defaulted:?}"
    );
}

#[test]
fn retried_write_makes_no_claim() {
    // `nika:write` is an atomic temp+rename overwrite — the replay
    // lands the SAME final bytes: idempotent, no hint.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  save:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:write\n      args: { path: out.md, content: \"x\" }\n",
    );
    assert!(!h.iter().any(|x| x.kind == "retry-effects"), "{h:?}");
}

#[test]
fn retry_on_contracted_effects_makes_no_claim() {
    // infer retries re-spend tokens (covered by cost) · `nika:write`
    // is a documented atomic overwrite (idempotent — P0-17 narrowed
    // this test's blanket « nika: builtins are idempotent » claim:
    // `nika:notify` and non-GET `nika:fetch` are NOT, the two tests
    // above pin their hint) · max_attempts 1 is no retry at all —
    // none of these hint.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  ask:\n    retry: { max_attempts: 3 }\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n  save:\n    retry: { max_attempts: 3 }\n    with: { content: \"${{ tasks.ask.output }}\" }\n    invoke:\n      tool: nika:write\n      args: { path: out.md, content: \"${{ with.content }}\" }\n  once:\n    retry: { max_attempts: 1 }\n    after: { save: success }\n    exec: { shell: \"true\" }\n",
    );
    assert!(!h.iter().any(|x| x.kind == "retry-effects"), "{h:?}");
}

// ─── secrets-store hint pipeline ───────────────────────────────────
// push_unresolvable_secret_hints → referenced_secrets → task_text_fields
//   → collect_json_strings_into. These functions are exercised both
//   behaviorally (through scan_hints) and as units below.

fn wf_of(yaml: &str) -> RawWorkflow {
    parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")
}

#[test]
fn referenced_vault_secret_gets_the_secrets_store_hint() {
    // a vault-source secret that IS referenced via `${{ secrets.FOO }}`
    // in a task field → push_unresolvable_secret_hints must emit a
    // `secrets-store` hint naming FOO. Kills:
    //   - push_unresolvable_secret_hints → () (no hint at all)
    //   - referenced_secrets → {} / {""} / {"xyzzy"} (FOO not in set →
    //     the `referenced.contains(name)` guard fails → no hint)
    let h = hints_of(
        "nika: w\nsecrets:\n  FOO:\n    source: vault\n    key: prod/foo\ntasks:\n  t:\n    exec: { shell: \"echo ${{ secrets.FOO }}\" }\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "secrets-store")
        .expect("secrets-store hint");
    assert_eq!(hit.task, "-");
    assert!(hit.advice.contains("secrets.FOO"), "{hit:?}");
}

#[test]
fn unreferenced_vault_secret_gets_no_hint() {
    // declared-but-unused vault secret is harmless — the hint fires
    // ONLY for a referenced secret. If referenced_secrets returned a
    // spurious {"FOO"} (the from_iter(["xyzzy"]) family with a
    // matching name would not, but a hardcoded set could) this would
    // also catch over-collection.
    let h = hints_of(
        "nika: w\nsecrets:\n  FOO:\n    source: vault\n    key: prod/foo\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n",
    );
    assert!(!h.iter().any(|x| x.kind == "secrets-store"), "{h:?}");
}

#[test]
fn referenced_secrets_collects_exactly_the_referenced_names() {
    // Direct unit on referenced_secrets — FOO referenced in a prompt,
    // BAR referenced in an output, BAZ declared but never referenced.
    // The returned set must be exactly {FOO, BAR}. Kills the
    // referenced_secrets → BTreeSet::new() / from_iter([""]) /
    // from_iter(["xyzzy"]) mutations precisely (wrong cardinality OR
    // wrong contents).
    let wf = wf_of(
        "nika: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\n  BAR:\n    source: vault\n    key: b\n  BAZ:\n    source: vault\n    key: c\ntasks:\n  t:\n    infer: { prompt: \"use ${{ secrets.FOO }}\", max_tokens: 10 }\noutputs:\n  r: ${{ secrets.BAR }}\n",
    );
    let refs = referenced_secrets(&wf);
    let got: Vec<&str> = refs.iter().map(String::as_str).collect();
    assert_eq!(got, vec!["BAR", "FOO"], "BTreeSet is sorted");
}

#[test]
fn referenced_secrets_empty_when_none_referenced() {
    // No `${{ secrets.X }}` island anywhere → empty set. This is the
    // baseline the from_iter([""]) / from_iter(["xyzzy"]) mutations
    // violate (they would return a non-empty set here).
    let wf = wf_of(
        "nika: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  t:\n    exec: { shell: \"echo plain\" }\n",
    );
    assert!(referenced_secrets(&wf).is_empty());
}

#[test]
fn secret_referenced_only_in_task_field_is_found() {
    // Isolates task_text_fields: the secret appears ONLY inside a task
    // field (the infer prompt), NEVER in outputs. If task_text_fields
    // returns vec![] / vec![""] / vec!["xyzzy"], the prompt island is
    // never scanned → FOO is absent → the secrets-store hint vanishes.
    let h = hints_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  t:\n    infer: { prompt: \"call with ${{ secrets.FOO }}\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.t.output }}\n",
    );
    assert!(
        h.iter()
            .any(|x| x.kind == "secrets-store" && x.advice.contains("secrets.FOO")),
        "{h:?}"
    );
}

#[test]
fn task_text_fields_collects_every_action_text_surface() {
    // Direct unit on task_text_fields across the action variants +
    // `with:`. Kills task_text_fields → vec![] / vec![""] /
    // vec!["xyzzy"] (the real surfaces are none of those) and confirms
    // the exec/invoke/infer/agent + with arms each contribute.

    // exec: command + stdin + env values
    let exec = wf_of(
        "nika: w\ntasks:\n  t:\n    exec:\n      shell: \"run CMD\"\n      stdin: \"STDIN\"\n      env: { K: \"ENVVAL\" }\n",
    );
    let f = task_text_fields(&exec.tasks[0].value);
    assert!(f.contains(&"run CMD"), "{f:?}");
    assert!(f.contains(&"STDIN"), "{f:?}");
    assert!(f.contains(&"ENVVAL"), "{f:?}");

    // infer: prompt + system
    let infer = wf_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: \"PROMPT\", system: \"SYSTEM\", max_tokens: 10 }\n",
    );
    let f = task_text_fields(&infer.tasks[0].value);
    assert!(f.contains(&"PROMPT") && f.contains(&"SYSTEM"), "{f:?}");

    // agent: prompt + system
    let agent = wf_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    agent: { prompt: \"APROMPT\", system: \"ASYSTEM\", max_tokens_total: 10 }\n",
    );
    let f = task_text_fields(&agent.tasks[0].value);
    assert!(f.contains(&"APROMPT") && f.contains(&"ASYSTEM"), "{f:?}");

    // invoke args JSON strings + with JSON strings
    let invoke = wf_of(
        "nika: w\ntasks:\n  t:\n    with: { wkey: \"WITHVAL\" }\n    invoke: { tool: \"nika:write\", args: { path: \"ARGVAL\" } }\n",
    );
    let f = task_text_fields(&invoke.tasks[0].value);
    assert!(f.contains(&"ARGVAL"), "invoke args string: {f:?}");
    assert!(f.contains(&"WITHVAL"), "with value string: {f:?}");
}

#[test]
fn collect_json_strings_into_gathers_all_nested_string_leaves() {
    // Direct unit on collect_json_strings_into. Feed
    // {"a":"x","b":["y",{"c":"z"}]} → ALL of x,y,z must be collected.
    //   - String arm deleted → top-level "x" (and nested) dropped
    //   - Array arm deleted → "y" and the object under it dropped
    //   - Object arm deleted → "z" (object inside array) + "x"/"y"
    //     (top object) dropped
    //   - whole fn → () → nothing collected
    let value = serde_json::json!({ "a": "x", "b": ["y", { "c": "z" }] });
    let mut out = Vec::new();
    collect_json_strings_into(&value, &mut out);
    out.sort_unstable();
    assert_eq!(out, vec!["x", "y", "z"], "all nested leaves: {out:?}");
}

#[test]
fn collect_json_strings_into_array_arm_descends() {
    // Targeted at the Array match arm: a top-level array of strings.
    // Deleting the Array arm drops both leaves; the String/Object arms
    // alone cannot reach them.
    let value = serde_json::json!(["one", "two"]);
    let mut out = Vec::new();
    collect_json_strings_into(&value, &mut out);
    out.sort_unstable();
    assert_eq!(out, vec!["one", "two"], "{out:?}");
}

#[test]
fn collect_json_strings_into_object_arm_descends() {
    // Targeted at the Object match arm: a flat object. Deleting the
    // Object arm drops the leaf entirely.
    let value = serde_json::json!({ "k": "deep" });
    let mut out = Vec::new();
    collect_json_strings_into(&value, &mut out);
    assert_eq!(out, vec!["deep"], "{out:?}");
}

#[test]
fn collect_json_strings_into_string_arm_pushes_the_leaf() {
    // Targeted at the String match arm: a bare string value. Deleting
    // the String arm drops it; the `_ => {}` catch-all would swallow it.
    let value = serde_json::json!("bare");
    let mut out = Vec::new();
    collect_json_strings_into(&value, &mut out);
    assert_eq!(out, vec!["bare"], "{out:?}");
}

#[test]
fn collect_json_strings_into_ignores_non_string_scalars() {
    // numbers/bools/null contribute nothing (the `_ => {}` arm). This
    // pins the boundary the deleted-arm mutants must not cross.
    let value = serde_json::json!({ "n": 1, "b": true, "z": null, "s": "keep" });
    let mut out = Vec::new();
    collect_json_strings_into(&value, &mut out);
    assert_eq!(out, vec!["keep"], "{out:?}");
}

#[test]
fn secret_referenced_inside_invoke_args_json_is_found() {
    // End-to-end: a secret reachable ONLY through the invoke-args JSON
    // walk (collect_json_strings_into via task_text_fields). With any
    // of the collect arms blinded, FOO is never seen → no hint.
    let h = hints_of(
        "nika: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  t:\n    invoke: { tool: \"nika:write\", args: { path: \"./o\", content: \"${{ secrets.FOO }}\" } }\n",
    );
    assert!(
        h.iter()
            .any(|x| x.kind == "secrets-store" && x.advice.contains("secrets.FOO")),
        "{h:?}"
    );
}

#[test]
fn unwrapped_output_ref_is_hinted_wrapped_is_silent() {
    // Output gauntlet (2026-07-11): a bare `tasks.X.output…` output
    // value is the LITERAL STRING (the run returns the path text, not
    // the value) — hint the wrap. The pattern is distinctive across
    // the five reference namespaces.
    let h = hints_of(
        "nika: w\nmodel: mock/echo\ntasks:\n  data:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: { count: 42 } } }\noutputs:\n  just_count: tasks.data.output.count\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "unwrapped-ref")
        .unwrap_or_else(|| panic!("expected unwrapped-ref: {h:?}"));
    assert_eq!(hit.task, "just_count");
    assert!(
        hit.advice.contains("literal string")
            && hit.advice.contains("${{ tasks.data.output.count }}"),
        "{}",
        hit.advice
    );

    // A properly wrapped output is SILENT (the common correct case).
    let wrapped = hints_of(
        "nika: w\nmodel: mock/echo\ntasks:\n  data:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: { count: 42 } } }\noutputs:\n  just_count: ${{ tasks.data.output.count }}\n",
    );
    assert!(
        !wrapped.iter().any(|x| x.kind == "unwrapped-ref"),
        "{wrapped:?}"
    );

    // A genuine string constant that is NOT a namespace path is silent.
    let plain = hints_of(
        "nika: w\nmodel: mock/echo\ntasks:\n  data:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: {} } }\noutputs:\n  label: production\n",
    );
    assert!(
        !plain.iter().any(|x| x.kind == "unwrapped-ref"),
        "{plain:?}"
    );
}

// ── F-P3 · the run-clock hint (finding (b) · WARN-dur, never a refusal) ──

#[test]
fn a_deadline_against_an_undeclared_clock_is_hinted() {
    let h = hints_of(
        "nika: w\ntasks:\n  slow:\n    exec: { command: [\"sleep\", \"1\"] }\n    timeout: \"5m\"\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "run-clock")
        .expect("the undeclared-clock hint fires");
    assert_eq!(hit.task, "-", "one workflow-level row");
    assert!(hit.advice.contains("1 task(s)"), "{}", hit.advice);
    assert!(
        hit.advice.contains("run: { clock: system }"),
        "the fix is spelled: {}",
        hit.advice
    );
}

#[test]
fn the_run_clock_hint_stays_silent_when_the_clock_is_named() {
    // Explicit clock: — named.
    let explicit = hints_of(
        "nika: w\nrun: { clock: system }\ntasks:\n  slow:\n    exec: { command: [\"sleep\", \"1\"] }\n    timeout: \"5m\"\n",
    );
    assert!(
        !explicit.iter().any(|x| x.kind == "run-clock"),
        "{explicit:?}"
    );
    // Deterministic entropy binds the virtual clock by law — named too.
    let seeded = hints_of(
        "nika: w\nrun: { entropy: { seeded: 42 } }\ntasks:\n  slow:\n    exec: { command: [\"sleep\", \"1\"] }\n    timeout: \"5m\"\n",
    );
    assert!(!seeded.iter().any(|x| x.kind == "run-clock"), "{seeded:?}");
    // No deadline at all — nothing to teach.
    let no_timeout = hints_of("nika: w\ntasks:\n  fast:\n    exec: { command: [\"true\"] }\n");
    assert!(
        !no_timeout.iter().any(|x| x.kind == "run-clock"),
        "{no_timeout:?}"
    );
}

#[test]
fn the_run_clock_hint_counts_every_deadline_once() {
    let h = hints_of(
        "nika: w\nrun: { entropy: ambient }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    timeout: \"30s\"\n  b:\n    exec: { command: [\"true\"] }\n    timeout: \"30s\"\n",
    );
    let hits: Vec<_> = h.iter().filter(|x| x.kind == "run-clock").collect();
    assert_eq!(hits.len(), 1, "one deduped row: {hits:?}");
    assert!(hits[0].advice.contains("2 task(s)"), "{}", hits[0].advice);
}

#[test]
fn digit_string_enum_is_hinted_words_are_silent() {
    let h = hints_of(
        "nika: w\nmodel: mock/echo\ntasks:\n  t:\n    infer:\n      prompt: x\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          n: { type: string, enum: [\"0\", \"1\", \"3\"] }\noutputs:\n  r: ${{ tasks.t.output }}\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "digit-string-enum")
        .expect("digit-string-enum");
    assert_eq!(hit.task, "t");
    assert!(hit.advice.contains("integer"), "{}", hit.advice);

    let words = hints_of(
        "nika: w\nmodel: mock/echo\ntasks:\n  t:\n    infer:\n      prompt: x\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          n: { type: string, enum: [none, S, M] }\noutputs:\n  r: ${{ tasks.t.output }}\n",
    );
    assert!(
        !words.iter().any(|x| x.kind == "digit-string-enum"),
        "{words:?}"
    );
}

#[test]
fn hash_of_an_object_task_output_does_not_hint_tojson() {
    // Runtime hashes a non-string `content:` as compact JSON. Check must
    // not push authors toward `| tojson` on an object-shaped binding.
    let h = hints_of(
        "nika: w\npermits: { tools: [\"nika:jq\", \"nika:hash\"] }\ntasks:\n  roster:\n    invoke: { tool: nika:jq, args: { input: [{stem: ada}], expression: \".\" } }\n  fp:\n    with: { roster: \"${{ tasks.roster.output }}\" }\n    invoke: { tool: nika:hash, args: { content: \"${{ with.roster }}\" } }\n",
    );
    assert!(
        !h.iter()
            .any(|x| x.advice.contains("tojson") || x.advice.contains("to_json")),
        "object-shaped hash content must not be hinted to | tojson: {h:?}"
    );
}

#[test]
fn inspect_invoke_is_hinted_as_unwired() {
    let h = hints_of(
        "nika: w\npermits: { tools: [\"nika:inspect\"] }\ntasks:\n  look:\n    invoke: { tool: \"nika:inspect\", args: { view: cost } }\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "inspect-unwired")
        .expect("inspect-unwired");
    assert_eq!(hit.task, "look");
    assert!(hit.advice.contains("available: false"), "{}", hit.advice);
}

#[test]
fn markdown_glob_without_readme_exclude_is_hinted() {
    let h = hints_of(
        "nika: w\npermits: { tools: [\"nika:glob\"], fs: { read: [\"held\"] } }\ntasks:\n  find:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"held/*.md\" } }\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "glob-readme")
        .expect("glob-readme");
    assert_eq!(hit.task, "find");
    assert!(hit.advice.contains("README"), "{}", hit.advice);

    let excluded = hints_of(
        "nika: w\npermits: { tools: [\"nika:glob\"], fs: { read: [\"held\"] } }\ntasks:\n  find:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"held/*.md\", exclude: \"**/README.md\" } }\n",
    );
    assert!(
        !excluded.iter().any(|x| x.kind == "glob-readme"),
        "{excluded:?}"
    );
}

#[test]
fn jq_as_then_bare_map_is_hinted() {
    let h = hints_of(
        "nika: w\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  score:\n    invoke:\n      tool: nika:jq\n      args:\n        input: []\n        expression: |\n          . as $c\n          | map(.n)\n",
    );
    let hit = h.iter().find(|x| x.kind == "jq-as-map").expect("jq-as-map");
    assert_eq!(hit.task, "score");
    assert!(hit.advice.contains("$name | map"), "{}", hit.advice);

    let ok = hints_of(
        "nika: w\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  score:\n    invoke:\n      tool: nika:jq\n      args:\n        input: []\n        expression: |\n          . as $c\n          | ($c | map(.n))\n",
    );
    assert!(!ok.iter().any(|x| x.kind == "jq-as-map"), "{ok:?}");

    let oneline = hints_of(
        "nika: w\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  score:\n    invoke: { tool: nika:jq, args: { input: [], expression: \". as $c | map(.n)\" } }\n",
    );
    assert!(
        oneline.iter().any(|x| x.kind == "jq-as-map"),
        "one-liner . as $c | map( must hint: {oneline:?}"
    );
}

#[test]
fn assert_after_a_write_names_the_quarantine() {
    let h = hints_of(
        "nika: w\npermits: { tools: [\"nika:write\", \"nika:assert\"], fs: { write: [\"out\"] } }\ntasks:\n  save:\n    invoke: { tool: \"nika:write\", args: { path: out/a.md, content: \"x\" } }\n  gate:\n    invoke: { tool: \"nika:assert\", args: { that: true } }\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "assert-quarantine")
        .expect("assert-quarantine");
    assert_eq!(hit.task, "gate");
    assert!(hit.advice.contains("quarantine"), "{}", hit.advice);
}

#[test]
fn infer_that_assigns_a_belt_is_the_law_hint() {
    let h = hints_of(
        "nika: w\nmodel: mock/echo\ntasks:\n  judge:\n    infer:\n      prompt: |\n        Read the note and assign a belt.\n      max_tokens: 64\noutputs:\n  r: ${{ tasks.judge.output }}\n",
    );
    let hit = h
        .iter()
        .find(|x| x.kind == "infer-as-law")
        .expect("infer-as-law");
    assert_eq!(hit.task, "judge");
    assert!(hit.advice.contains("13-extract-then-law"), "{}", hit.advice);

    let extract = hints_of(
        "nika: w\nmodel: mock/echo\ntasks:\n  facts:\n    infer:\n      prompt: |\n        Extract facts only. Never assign a belt.\n      max_tokens: 64\noutputs:\n  r: ${{ tasks.facts.output }}\n",
    );
    assert!(
        !extract.iter().any(|x| x.kind == "infer-as-law"),
        "a never-assign extract stays silent: {extract:?}"
    );
}

#[test]
fn a_locale_infer_after_extract_is_not_the_law() {
    // A second infer + string enum is language-id / sentiment — the
    // one-way. Phrase list stays the detector (a structural arm
    // false-reds BCP-47).
    let h = hints_of(
        "nika: w\nmodel: mock/echo\ntasks:\n  facts:\n    infer: { prompt: extract, max_tokens: 32 }\n  lang:\n    with: { facts: \"${{ tasks.facts.output }}\" }\n    infer:\n      prompt: Name the BCP-47 language.\n      max_tokens: 16\n      schema: { type: string, enum: [en, fr, de, es] }\noutputs:\n  r: ${{ tasks.lang.output }}\n",
    );
    assert!(
        !h.iter().any(|x| x.kind == "infer-as-law"),
        "locale id is language, not the law: {h:?}"
    );
}
