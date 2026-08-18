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

/// The task body's dead keys from the same sweep — a task's twenty-key set
/// stays silent by design (no set listing · no near-miss for these), so
/// without an arm each reads as `zzz:` to the author who typed it last week.
pub(super) fn task(key: &str) -> Option<&'static str> {
    match key {
        "on_finally" => Some(
            "cleanup is a TASK now, joined by an unwind edge (2026-08-11) — write \
             it as its own task with `after: { <parent>: unwind }` (a `finally` \
             node in `graph_format: 3`); the second grammar for a task body died",
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

#[cfg(test)]
mod tests {
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

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

    /// The task body had its own retired keys (the same 2026-08-11 sweep)
    /// — a task's twenty-key set stays silent by design, so without an
    /// arm each of these reads as `zzz:` to the author who typed it last
    /// week.
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
