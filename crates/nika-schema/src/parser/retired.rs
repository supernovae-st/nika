// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The retired keys teach where their role went — the arms behind
//! `parser::retired_key_teaching` (the dispatcher stays in `mod.rs` · the
//! vocabulary lives here). Split out under the ADR-023 1,500-LOC ceiling,
//! which `mod.rs` sat fifteen lines under once the arms landed — the
//! `declassify.rs` / `lift.rs` / `for_each.rs` precedent.
//!
//! Two scopes retired keys in the 2026-08-11/13 sweep — the fourteen-key
//! ENVELOPE (five keys) and the TASK body (six) — and each arm fires in
//! its scope only (`output` is a plausible name in both; one of them
//! retired it). Every teaching is the engine's own words (the pack ·
//! `nika-migrate` · the LOT 2 diff), never guessed.

/// The fourteen-key envelope's five dead keys — where each role went, in
/// the engine's own words (the pack, `nika-migrate`, the LOT 2 diff), never
/// guessed. Measured 2026-08-18: the shipped 0.108.0 passes 0/40 of main's
/// pack examples and main refuses 40/40 of 0.108.0's, so this refusal is
/// the FIRST line every existing file meets on the nine-key engine.
pub(super) fn envelope(key: &str) -> Option<&'static str> {
    match key {
        "config" => Some(
            "this key shipped in the fourteen-key envelope and died with the \
             nine-key one (2026-08-13) — a deployment-supplied value is now an \
             `inputs:` entry with `required: false` and a `default:`; a value \
             baked into the file is a `const:` entry",
        ),
        "workflow" => Some(
            "this block shipped in the fourteen-key envelope and died with the \
             nine-key one (2026-08-12) — the identity moved onto `nika:` itself \
             (`nika: <id>` · a kebab-case name, no longer `v1`); its `description:` \
             prose belongs in a `#` comment above `nika:`, never dropped",
        ),
        "description" => Some(
            "a top-level `description:` shipped twice (bare, then inside \
             `workflow:`) and both died with the nine-key envelope — write the \
             prose as a `#` comment above `nika:` (the identity is `nika: <id>`)",
        ),
        "types" => Some(
            "the named-type block died with the nine-key envelope (2026-08-11 · \
             NIKA-TYPE-002 retired) — a shape rides the verb's own `schema:` \
             (structured output) or a task's `returns:`; the ten primitives stay \
             lowercase (spec 09)",
        ),
        "policy" => Some(
            "this block died with the nine-key envelope (2026-08-11) — a \
             vocabulary is not a policy: the effect boundary is `permits:` \
             (`fs` · `net` · `exec` · `tools`), a governed value is a `secrets:` \
             entry, and the ordering laws are unconditional (spec 10)",
        ),
        "assert" => Some(
            "this block died with the nine-key envelope — a run's obligations \
             are proven on the TRACE, not declared in the file: `nika trace \
             verify` (spec 15) reads them from the sealed run",
        ),
        _ => None,
    }
}

/// The task body's dead keys from the same sweep. The shared unknown-field
/// tail enumerates the task set; these arms preserve migration-specific
/// teaching ahead of it.
pub(super) fn task(key: &str) -> Option<&'static str> {
    match key {
        "on_finally" => Some(
            "cleanup is a TASK now, joined by an unwind edge (2026-08-11) — write \
             it as its own task with `after: { <parent>: unwind }` (a `finally` \
             node in `graph_format: 3`); the second grammar for a task body died. \
             A cleanup runs under the SAME `permits:` boundary as any task — an \
             undeclared effect refuses, and the refusal is journaled on the trace \
             (`permit_checked` · plane `on_finally`), never propagated",
        ),
        "output" => Some(
            "renamed `extract:` (2026-08-11) — same shape, the truthful word for \
             what it does",
        ),
        "declassify" | "inert" => Some(
            "merged into `lift:` (2026-08-11) — the law is a PARAMETER of one \
             door, not two spellings; `lift:` names which law the task opens and \
             why (spec 10 §the authored doors)",
        ),
        "max_parallel" | "fail_fast" => Some(
            "the two fan-out knobs live INSIDE the `for_each:` block now \
             (`for_each: { items: …, max_parallel: N, fail_fast: false }`) — they \
             have no meaning without it (spec 03 §for_each)",
        ),
        _ => None,
    }
}

/// Keys an author BRINGS from another tool's dialect. Nothing here was
/// ever ours, so none of it is retired — but the author is not confused
/// about spelling, they are confused about which language they are in,
/// and `unknown field` answers a question they did not ask.
///
/// Measured 2026-08-20 · a persona wave put fifteen outside authors
/// against 0.111.0 with only the binary, the oracle and the public docs.
/// Two of them, independently, stopped here. One wrote `run:` inside a
/// task and quit at that line, verbatim: « the error message does not
/// teach you what the field should be ». The other wrote `uses:` and
/// `needs:`, reached green only by reading a shipped example, and named
/// the missing sentence exactly: « dependencies are inferred from data
/// bindings, there is no `needs:` field ».
///
/// That sentence already EXISTS. `depends_on:` gets it one layer up as
/// its own code (`W2DependsOnField`). `needs:` is the same concept in
/// another spelling and reached the generic path with nothing. These
/// arms route to the teaching that is already written; they invent no
/// new prose.
pub(super) fn foreign(key: &str) -> Option<&'static str> {
    match key {
        "needs" => Some(
            "a dependency is not declared on the task here — DATA rides a `with:` \
             binding and the binding IS the edge (`with: { x: \"${{ tasks.a.output }}\" }`), \
             and pure ORDERING rides `after: { a: success }` (spec 03 §the flow · \
             `depends_on:` is the same key under its other name)",
        ),
        "uses" | "steps" | "jobs" | "script" => Some(
            "a task body is one of the four verbs — `infer:` · `exec:` · `invoke:` · \
             `agent:` — and there is no action-reference field; a reusable unit is \
             another workflow, called with `invoke: { workflow: ./child.nika.yaml }` \
             (spec 14 §composition)",
        ),
        "run" => Some(
            "GitHub Actions `run:` is a shell step — here the verb is `exec:` \
             (`exec: { command: \"echo hi\" }` plus `permits.exec`). Envelope \
             `run:` (`entropy` · `clock`) belongs at column 0 beside `tasks:`, \
             never inside a task",
        ),
        "params" => Some(
            "`invoke:` takes `args:` (a map), not `params:` — the live dialect \
             is `invoke: { tool: nika:fetch, args: { url: \"https://example.com\" } }` \
             (spec 03 §invoke)",
        ),
        // Airflow's task vocabulary (#1402 · the rival-tool persona renamed
        // its way through every refusal that named a replacement and
        // stopped at the five that did not).
        "on_failure_callback" | "on_success_callback" | "on_retry_callback" => Some(
            "a callback is a TASK that runs on the outcome — declare it with \
             `after: { <task>: failure }` (or `success` · `terminal` · `unwind`) \
             on the task that should react, never as a field of the task it watches",
        ),
        "trigger_rule" => Some(
            "the outcome a task waits for rides `after:` — `after: { x: terminal }` \
             runs whatever `x` did (Airflow `all_done`) · `failure` only after a \
             failed `x` · `unwind` on the failure path · `success` (the default) \
             only after a green `x`",
        ),
        "retries" | "retry_delay" | "retry_exponential_backoff" => Some(
            "retries ride one `retry:` mapping — `retry: { max_attempts: 3, \
             backoff_ms: 1000, backoff_strategy: exponential, jitter: true }` \
             (`on_codes:` narrows it to named refusals)",
        ),
        "execution_timeout" | "timeout_minutes" | "timeout-minutes" => Some(
            "a hard kill rides `timeout:` as a Go duration — `timeout: \"30s\"` \
             (`\"5m\"` · `\"2h\"` · at most `24h`)",
        ),
        "if" => Some(
            "a condition rides `when:` as a CEL boolean — `when: \"${{ inputs.ship }}\"` \
             (a `tasks.*` read is hoisted into `with:` first)",
        ),
        "continue-on-error" | "continue_on_error" => Some(
            "a failure the run may survive rides `on_error:` — `on_error: { skip: true }` \
             skips the task, `on_error: { recover: <value> }` substitutes a value",
        ),
        _ => None,
    }
}

/// An ENVELOPE key from another dialect (#1402 · Airflow · GitHub
/// Actions). The concept exists; it lives in another file or under
/// another word, and the refusal says which.
pub(super) fn foreign_envelope(key: &str) -> Option<&'static str> {
    match key {
        "schedule_interval" | "schedule" | "cron" | "on" | "catchup" => Some(
            "a cadence is not a workflow field — the file proposes, the machine \
             disposes: the beat is declared in the project file (`nika.yaml` → `arm:`) \
             and `nika arm` reads what is armed and when each beat next fires",
        ),
        "default_args" => Some(
            "there are no task defaults — each task declares its own `retry:` · \
             `timeout:` · `model:`; a deployment knob is an `inputs:` entry with \
             `required: false` and a `default:`, read as `${{ inputs.<name> }}`",
        ),
        "dag_id" | "workflow_id" => Some(
            "the file's name is `nika: <kebab-id>` — the mark AND the name, never a \
             version",
        ),
        "jobs" | "steps" => Some(
            "the work lives under `tasks:` — a map keyed by task id (snake_case), \
             each task exactly one verb (`infer:` · `exec:` · `invoke:` · `agent:`)",
        ),
        _ => None,
    }
}

/// An ENVELOPE key typed one level too deep. Every one of these is valid
/// at column 0, so the author holds the right word in the wrong place —
/// and `unknown field` says the word is wrong, which is the one thing it
/// is not. `run:` is the case that cost an outside author the session:
/// it is a real envelope key, and inside a task it read as nonsense.
pub(super) fn envelope_key_at_task_level(key: &str) -> Option<&'static str> {
    crate::parser::TOP_LEVEL_KEYS.contains(&key).then_some(
        "this is a workflow ENVELOPE field · it belongs at column 0 beside \
         `tasks:`, never inside a task. A task body is one of the four verbs \
         (`infer:` · `exec:` · `invoke:` · `agent:`) plus the task modifiers \
         (`after` · `when` · `for_each` · `retry` · `on_error` · `timeout` · \
         `with` · `extract` · `returns` · `lift` · `group`)",
    )
}

#[cfg(test)]
mod tests {
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    /// #1402 — the five Airflow / GitHub Actions terms that reached the
    /// generic field list with no replacement anywhere in the CLI. Each
    /// now names the mechanism it maps to; the assertions name
    /// MECHANISMS, never whole sentences.
    #[test]
    fn foreign_envelope_and_task_terms_name_their_mechanism() {
        let envelope = |line: &str| {
            let yaml =
                format!("nika: w\n{line}\ntasks:\n  t:\n    exec: {{ command: [\"true\"] }}\n");
            parse(&yaml, FileId::new(0), ParseMode::Strict)
                .expect_err("an unknown envelope key refuses")
                .to_string()
        };
        let cadence = envelope("schedule_interval: \"@daily\"");
        assert!(
            cadence.contains("`arm:`")
                && cadence.contains("nika.yaml")
                && cadence.contains("nika arm"),
            "a cadence routes to the project file and the arm verb · {cadence}"
        );
        let defaults = envelope("default_args: { retries: 2 }");
        assert!(
            defaults.contains("`inputs:`") && defaults.contains("`retry:`"),
            "task defaults route to per-task fields and inputs knobs · {defaults}"
        );
        let dag = envelope("dag_id: etl");
        assert!(
            dag.contains("`nika: <kebab-id>`"),
            "dag_id routes to the mark · {dag}"
        );
        let jobs = envelope("jobs: {}");
        assert!(jobs.contains("`tasks:`") && jobs.contains("verb"), "{jobs}");

        let task = |body: &str| {
            let yaml =
                format!("nika: w\ntasks:\n  t:\n{body}    exec: {{ command: [\"true\"] }}\n");
            parse(&yaml, FileId::new(0), ParseMode::Strict)
                .expect_err("an unknown task key refuses")
                .to_string()
        };
        let callback = task("    on_failure_callback: notify\n");
        assert!(
            callback.contains("after:") && callback.contains("failure"),
            "a callback routes to `after: {{ x: failure }}` · {callback}"
        );
        let rule = task("    trigger_rule: all_done\n");
        assert!(
            rule.contains("terminal") && rule.contains("all_done"),
            "`trigger_rule` routes to the `after:` outcomes · {rule}"
        );
        let retries = task("    retries: 3\n");
        assert!(
            retries.contains("max_attempts") && retries.contains("backoff_ms"),
            "`retries` names the retry mapping's fields · {retries}"
        );
        let cond = task("    if: true\n");
        assert!(cond.contains("`when:`"), "`if` routes to `when:` · {cond}");
    }

    /// A key from another dialect, and our own key at the wrong depth,
    /// both TEACH — because in neither case is the author confused about
    /// spelling. Measured 2026-08-20: two outside authors in one persona
    /// wave stopped at exactly these two shapes, one of them for good.
    ///
    /// The assertions name MECHANISMS, never whole sentences: a pin that
    /// quotes its own prose passes on a message that has been reworded
    /// into nonsense, which is a mirror, not a test.
    #[test]
    fn a_task_key_from_another_dialect_or_the_wrong_depth_teaches() {
        let sees = |body: &str| {
            let yaml = format!("nika: w\ntasks:\n  t:\n{body}");
            parse(&yaml, FileId::new(0), ParseMode::Strict)
                .expect_err("an unknown task key refuses")
                .to_string()
        };

        // GitHub Actions `run:` is the false friend (P02/P08 2026-08-31).
        // Envelope `run:` {entropy, clock} is real at column 0; inside a
        // task the author meant a shell step. Name `exec:`, not a hoist.
        let run = sees("    run: echo hi\n");
        assert!(
            run.contains("exec:") && run.contains("GitHub Actions"),
            "task-level `run:` names the GHA false friend and `exec:` · {run}"
        );
        assert!(
            run.contains("column 0") && run.contains("entropy"),
            "and still points envelope `run:` at column 0 · {run}"
        );

        let params = sees("    params: { url: u }\n    exec: { command: [\"true\"] }\n");
        assert!(
            params.contains("`args:`") && params.contains("params:"),
            "task-level `params:` routes to live `args:` · {params}"
        );

        // The GitHub Actions spelling of a concept that already has a
        // teaching one layer up (`depends_on:` · W2DependsOnField).
        let needs = sees("    needs: [other]\n    exec: { command: [\"true\"] }\n");
        assert!(
            needs.contains("with:") && needs.contains("after:"),
            "`needs:` routes to the mechanism, both halves · {needs}"
        );
        assert!(
            needs.contains("depends_on"),
            "and names the same key under its other spelling · {needs}"
        );

        // An action reference: the shape has no counterpart, so the
        // teaching has to name the four verbs AND where reuse lives.
        let uses = sees("    uses: some/action@v4\n");
        assert!(
            uses.contains("four verbs") && uses.contains("invoke:"),
            "`uses:` names the verb set · {uses}"
        );
        assert!(
            uses.contains("workflow:"),
            "and sends reuse to composition · {uses}"
        );

        // The silence the threshold exists to keep: a key that is nobody's
        // envelope word and nobody's import stays a bare unknown field.
        let noise = sees("    zzqx: 1\n    exec: { command: [\"true\"] }\n");
        assert!(
            !noise.contains("ENVELOPE") && !noise.contains("four verbs"),
            "an ordinary unknown key gains no teaching · {noise}"
        );
    }

    /// EVERY key the fourteen-key era shipped teaches where its role went
    /// — not a spelling guess, not the bare set listing (that one says
    /// what is valid NOW and nothing about the past). Measured 2026-08-18:
    /// the shipped 0.108.0 passes 0/40 of main's pack examples and main
    /// refuses 40/40 of 0.108.0's, so this refusal is the FIRST line every
    /// existing file meets on the nine-key engine · it has to teach.
    #[test]
    fn every_retired_envelope_key_teaches_where_its_role_went() {
        let sees = |key: &str, value: &str| {
            let yaml = format!(
                "nika: w\n{key}: {value}\ntasks:\n  t:\n    exec: {{ command: [\"true\"] }}\n"
            );
            parse(&yaml, FileId::new(0), ParseMode::Strict)
                .expect_err("an unknown envelope key refuses")
                .to_string()
        };
        let workflow = sees("workflow", "{ id: old, description: d }");
        assert!(
            workflow.contains("`nika: <id>`") && workflow.contains("comment"),
            "`workflow:` names the identity move AND the prose demotion · {workflow}"
        );
        assert!(
            !workflow.contains("did you mean"),
            "not a misspelling · {workflow}"
        );
        let description = sees("description", "\"prose\"");
        assert!(
            description.contains("comment") && description.contains("`nika:`"),
            "a top-level `description:` becomes a comment above `nika:` · {description}"
        );
        let types = sees("types", "{}");
        assert!(
            types.contains("`schema:`") && types.contains("`returns:`"),
            "`types:` names the two shape carriers that survive · {types}"
        );
        let policy = sees("policy", "{}");
        assert!(
            policy.contains("`permits:`") && policy.contains("vocabulary"),
            "`policy:` names the boundary that survives it · {policy}"
        );
        let obligations = sees("assert", "[]");
        assert!(
            obligations.contains("nika trace verify"),
            "`assert:` sends the obligation to the trace · {obligations}"
        );
    }

    /// The task body had its own retired keys (the same 2026-08-11 sweep).
    /// The shared tail enumerates the task set; these arms must preserve
    /// migration-specific teaching ahead of it.
    #[test]
    fn every_retired_task_key_teaches_where_its_role_went() {
        let sees = |key: &str, value: &str| {
            let yaml = format!(
                "nika: w\ntasks:\n  t:\n    exec: {{ command: [\"true\"] }}\n    {key}: {value}\n"
            );
            parse(&yaml, FileId::new(0), ParseMode::Strict)
                .expect_err("an unknown task key refuses")
                .to_string()
        };
        let on_finally = sees(
            "on_finally",
            "{ cleanup: { exec: { command: [\"true\"] } } }",
        );
        assert!(
            on_finally.contains("unwind") && on_finally.contains("after: {"),
            "`on_finally:` teaches the unwind edge · {on_finally}"
        );
        assert!(
            !on_finally.contains("did you mean"),
            "not a misspelling · {on_finally}"
        );
        assert!(
            sees("output", "{ x: \".x\" }").contains("`extract:`"),
            "`output:` → `extract:`"
        );
        assert!(
            sees("declassify", "[]").contains("`lift:`"),
            "`declassify:` → `lift:`"
        );
        assert!(
            sees("inert", "\"x\"").contains("`lift:`"),
            "`inert:` → `lift:`"
        );
        assert!(
            sees("max_parallel", "4").contains("`for_each:`"),
            "`max_parallel:` lives in the block"
        );
        assert!(
            sees("fail_fast", "false").contains("`for_each:`"),
            "`fail_fast:` lives in the block"
        );
    }

    /// Scope is symmetric · an envelope arm never fires in a task body and
    /// a task arm never fires in the envelope (`output` is a plausible name
    /// in both places; only ONE of them retired it).
    #[test]
    fn the_retired_teachings_stay_in_their_own_scope() {
        assert!(super::super::retired_key_teaching("workflow", "task `t`").is_none());
        assert!(super::super::retired_key_teaching("types", "`exec:`").is_none());
        assert!(
            super::super::retired_key_teaching("on_finally", "the workflow envelope").is_none()
        );
        assert!(super::super::retired_key_teaching("output", "the workflow envelope").is_none());
        assert!(super::super::retired_key_teaching("output", "`exec:`").is_none());
        assert!(super::super::retired_key_teaching("output", "task `t`").is_some());
    }
}
