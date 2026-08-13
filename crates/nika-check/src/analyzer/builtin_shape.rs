// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin arg-shape rules — the thin analyzer adapter.
//!
//! The RULES live in `nika-cap::builtin_shape_findings` (pure data ·
//! extracted 2026-07-07 when this crate hit its 15k prod budget — the
//! permits-half precedent of 2026-07-03): this side only walks the
//! parsed tasks (main action + `on_finally:` cleanups), hands each
//! `invoke:`'s (tool · args) to the shared rule set, and wraps every
//! finding message in [`SchemaError::BadBuiltinArgs`] with the task's
//! span. The truth-table tests below exercise the WHOLE pipe
//! (yaml → analyze → cap rules → diagnostics), so the two crates
//! cannot drift.

use nika_schema::error::SchemaError;
use nika_schema::raw::{RawAction, RawTask};
use nika_schema::source::Spanned;

/// Run every builtin arg-shape rule over a task's action (and its
/// `on_finally:` cleanup actions — same `invoke:` surface).
pub(super) fn check_builtin_shapes(tasks: &[Spanned<RawTask>], errors: &mut Vec<SchemaError>) {
    for task in tasks {
        let id = task.value.id.value.as_str();
        check_action(&task.value.action, id, errors);
    }
}

fn check_action(action: &RawAction, task: &str, errors: &mut Vec<SchemaError>) {
    let RawAction::Invoke(invoke) = action else {
        return;
    };
    // A `workflow:` call is not a builtin — its contract is the
    // composition lane's (spec 14), not the arg-shape table's.
    let Some(tool_ref) = invoke.tool() else {
        return;
    };
    let tool = tool_ref.value.as_str();
    let span = tool_ref.span;
    let args = invoke.args.as_ref().map(|a| &a.value);
    for reason in nika_cap::builtin_shape_findings(tool, args) {
        errors.push(SchemaError::BadBuiltinArgs {
            task: task.to_owned(),
            tool: tool.to_owned(),
            reason,
            span: Some(span),
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::analyzer::analyze;
    use nika_schema::error::SchemaError;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn has_shape_error(yaml: &str, tool: &str) -> bool {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        analyze(&wf)
            .err()
            .unwrap_or_default()
            .iter()
            .any(|e| matches!(e, SchemaError::BadBuiltinArgs { tool: t, .. } if t == tool))
    }

    /// Run one `(args · tool · violates?)` truth table — each row is one
    /// contract direction.
    fn assert_shape_cases(cases: &[(&str, &str, bool)]) {
        for (args, tool, violates) in cases {
            let yaml = format!(
                "nika: t\ntasks:\n  a:\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                *violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn shape_rules_table() {
        // (args yaml · tool · violates?) — one row per contract direction.
        // NOTE: flat required-arg contracts (`nika:write` content · `nika:jq`
        // expression) moved to the catalog `Builtin::required` set + the
        // `check::tools::scan_missing_args` check — they are NOT shape-rule
        // findings anymore (tested there). This table keeps the non-flat
        // contracts: the `done` sentinel · the `wait` XOR · `fetch` pairings.
        let cases = [
            ("{}", "nika:done", true), // standalone · always the sentinel error
            // A3 (agent battery 2026-07-11): compose is done's sibling —
            // the runtime refused it (COMPOSE-001), the check blessed it.
            ("{}", "nika:compose", true),
            (r#"{ workflow_yaml: "nika: v1" }"#, "nika:compose", true),
            (
                r#"{ duration: "5s", until: "2026-12-01T00:00:00Z" }"#,
                "nika:wait",
                true, // both modes
            ),
            ("{}", "nika:wait", true),                     // neither mode
            (r#"{ duration: "5s" }"#, "nika:wait", false), // exactly one
            // nika:fetch — the closed mode set + arg pairings
            // (conformance stdlib/extract-modes/001..004).
            ("{}", "nika:fetch", true),                 // url: is REQUIRED
            ("{ mode: markdown }", "nika:fetch", true), // still no url
            (r#"{ url: "https://x.test" }"#, "nika:fetch", false), // default markdown
            (
                r#"{ url: "https://x.test", mode: article }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: raw }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: html }"#,
                "nika:fetch",
                true,
            ), // 001: not a mode
            (
                r#"{ url: "https://x.test", mode: llm-txt }"#,
                "nika:fetch",
                true,
            ), // RESERVED
            (
                r#"{ url: "https://x.test", mode: markdown, jq: ".x" }"#,
                "nika:fetch",
                true, // 003: jq only with mode: jq
            ),
            (
                r#"{ url: "https://x.test", jq: ".x" }"#,
                "nika:fetch",
                true, // jq with the DEFAULT mode (markdown) — same violation
            ),
            (
                r#"{ url: "https://x.test", mode: jq, jq: ".items | map(.name)" }"#,
                "nika:fetch",
                false, // 004: the valid pairing
            ),
            (r#"{ url: "https://x.test", mode: jq }"#, "nika:fetch", true), // jq needs jq:
            (
                r#"{ url: "https://x.test", mode: selector }"#,
                "nika:fetch",
                true,
            ), // needs selector:
            (
                r#"{ url: "https://x.test", mode: selector, selector: "div.c" }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: text, selector: "div.c" }"#,
                "nika:fetch",
                true, // selector: only with mode: selector
            ),
            (
                r#"{ url: "https://x.test", mode: "${{ inputs.m }}" }"#,
                "nika:fetch",
                false, // templated mode — runtime business, statically silent
            ),
            (r#"{ url: "https://x.test", mode: 5 }"#, "nika:fetch", true), // not a string
        ];
        for (args, tool, violates) in &cases {
            let yaml = format!(
                "nika: w\ntasks:\n  t:\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                *violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn decide_shape_rules_table() {
        // nika:decide — bundle: path string OR inline object;
        // evidence: the EvidenceSnapshot object (spec 11 §nika:decide).
        assert_shape_cases(&[
            (
                r#"{ bundle: "./triage.bundle.json", evidence: { t: "2026-01-01T00:00:00Z" } }"#,
                "nika:decide",
                false, // the path form
            ),
            (
                r"{ bundle: { manifest: {} }, evidence: {} }",
                "nika:decide",
                false, // the inline form
            ),
            (r"{ bundle: 42, evidence: {} }", "nika:decide", true), // neither form
            (
                r#"{ bundle: "./b.json", evidence: "raw text" }"#,
                "nika:decide",
                true, // a literal non-object snapshot can never satisfy it
            ),
            (
                r#"{ bundle: "./b.json", evidence: "${{ tasks.collect.output }}" }"#,
                "nika:decide",
                false, // templated snapshot — runtime business
            ),
        ]);
    }

    #[test]
    fn fetch_payload_shape_rules_table() {
        // nika:fetch vNext — payload families (stdlib §fetch):
        // body ⊥ form ⊥ multipart · body-bearing method · closed part shape.
        let cases = [
            (
                r#"{ url: "https://x.test", form: { a: "b" } }"#,
                "nika:fetch",
                true, // form on the default GET — no body to carry
            ),
            (
                r#"{ url: "https://x.test", method: POST, form: { a: "b" } }"#,
                "nika:fetch",
                false, // the valid form pairing
            ),
            (
                r#"{ url: "https://x.test", method: post, form: { a: "b" } }"#,
                "nika:fetch",
                false, // method case-folds at runtime — static agrees
            ),
            (
                r#"{ url: "https://x.test", method: POST, form: { a: "b" }, body: "x" }"#,
                "nika:fetch",
                true, // body ⊥ form
            ),
            (
                r#"{ url: "https://x.test", method: "${{ vars.m }}", form: { a: "b" } }"#,
                "nika:fetch",
                false, // templated method — runtime business
            ),
            (
                r#"{ url: "https://x.test", method: POST, form: "nope" }"#,
                "nika:fetch",
                true, // form must be an object
            ),
            (
                r#"{ url: "https://x.test", method: PATCH, multipart: [{ name: f, value: v }] }"#,
                "nika:fetch",
                false, // valid text part on a body-bearing method
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [] }"#,
                "nika:fetch",
                true, // needs at least one part
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [{ name: f, value: v, path: p }] }"#,
                "nika:fetch",
                true, // exactly one of value | path
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [{ name: f, value: v, surprise: 1 }] }"#,
                "nika:fetch",
                true, // unknown part key — the shape is closed
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [{ name: f, value: v, filename: x }] }"#,
                "nika:fetch",
                true, // filename belongs to file parts
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [{ name: f, path: "out/a.png" }] }"#,
                "nika:fetch",
                false, // valid file part
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: "${{ tasks.prep.output }}" }"#,
                "nika:fetch",
                false, // fully-templated parts — runtime business
            ),
            (
                r#"{ url: "https://x.test", method: DELETE, multipart: [{ name: f, value: v }] }"#,
                "nika:fetch",
                true, // DELETE carries no body
            ),
            (
                r#"{ url: "https://x.test", method: POST, form: { a: "b" }, headers: { Content-Type: "application/json" } }"#,
                "nika:fetch",
                true, // the payload family owns its content-type (runtime parity)
            ),
        ];
        for (args, tool, violates) in &cases {
            let yaml = format!(
                "nika: w\ntasks:\n  t:\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                *violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn fetch_traverse_shape_rules_table() {
        // nika:fetch traverse — the bounded crawl (stdlib §fetch · traverse).
        let cases = [
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5 } }"#,
                "nika:fetch",
                false, // the valid crawl
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5, respect_robots: false } }"#,
                "nika:fetch",
                false, // robots opt-out is a bool field
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 0 } }"#,
                "nika:fetch",
                true, // below the range
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 26 } }"#,
                "nika:fetch",
                true, // above the cap
            ),
            (
                r#"{ url: "https://x.test", traverse: {} }"#,
                "nika:fetch",
                true, // max_pages is required
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5, depth: 2 } }"#,
                "nika:fetch",
                true, // the shape is closed
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5 }, mode: raw }"#,
                "nika:fetch",
                true, // traverse excludes the extraction args
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5 }, method: POST }"#,
                "nika:fetch",
                true, // GET only
            ),
            (
                r#"{ url: "https://x.test", traverse: "${{ vars.crawl }}" }"#,
                "nika:fetch",
                false, // fully-templated spec — runtime business
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: "${{ vars.n }}" } }"#,
                "nika:fetch",
                false, // templated field — runtime business
            ),
        ];
        for (args, tool, violates) in cases {
            let yaml = format!(
                "nika: t\ntasks:\n  a:\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn image_generate_v1_reservations_and_enum_rules() {
        assert_shape_cases(&[
            // nika:image_generate — V1 reservations · closed enums ·
            // ranges · size grammar · the transparent×jpeg conflict
            // (stdlib §Media). Flat required args (prompt/output_dir) are
            // the catalog missing-args check's concern — silent here.
            ("{}", "nika:image_generate", false),
            (
                r#"{ prompt: "x", output_dir: "./o", mode: edit }"#,
                "nika:image_generate",
                true, // edit without a source image → requires image:
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", mode: remix }"#,
                "nika:image_generate",
                true, // not a mode at all
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", mode: "${{ inputs.m }}" }"#,
                "nika:image_generate",
                false, // templated — runtime business
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", save: false }"#,
                "nika:image_generate",
                true, // V1: assets always land on disk
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", save: true }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", reference_images: ["a.png"] }"#,
                "nika:image_generate",
                true, // V1: text-to-image only
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", provider: midjourney }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", provider: mock }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", provider: local }"#,
                "nika:image_generate",
                false, // the sovereign path (v1.1)
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", provider: xai }"#,
                "nika:image_generate",
                false, // v1.1
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", format: gif }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", quality: hd }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", background: clear }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", aspect_ratio: "5:4" }"#,
                "nika:image_generate",
                true, // not in the closed common set
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", aspect_ratio: "16:9" }"#,
                "nika:image_generate",
                false,
            ),
        ]);
    }

    #[test]
    fn image_generate_range_size_and_conflict_rules() {
        assert_shape_cases(&[
            (
                r#"{ prompt: "x", output_dir: "./o", n: 0 }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", n: 11 }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", n: 3 }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", compression: 101 }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", timeout_ms: 999 }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", size: "1024" }"#,
                "nika:image_generate",
                true, // not WxH
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", size: auto }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", size: "1536x864" }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", background: transparent, format: jpeg }"#,
                "nika:image_generate",
                true, // jpeg carries no alpha
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", background: transparent, format: webp }"#,
                "nika:image_generate",
                false, // provider/model support is runtime business
            ),
        ]);
    }

    #[test]
    fn compose_in_agent_whitelist_is_legal() {
        // The positive direction of the A3 row: granting nika:compose to
        // an agent loop is exactly what ADR-096 blesses.
        let agent = "nika: t\ntasks:\n  a:\n    agent:\n      \
                     prompt: \"go\"\n      tools: [\"nika:compose\", \"nika:done\"]\n";
        assert!(!has_shape_error(agent, "nika:compose"));
    }
}
