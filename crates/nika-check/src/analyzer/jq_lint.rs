// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static jq COMPILE-check (the deep-static tier · spec `07-conformance.md`
//! §Levels · `NIKA-VAR-005` « static expression violation — … jq compile
//! error »).
//!
//! A `nika:jq` `expression:` (and a `nika:fetch` `mode: jq` `jq:`) is compiled
//! with the SAME jaq stack the runtime `nika:jq` builtin uses (`nika-builtin`
//! `data::jq`): `jaq_core` + `jaq_std` + `jaq_json` at one workspace-pinned
//! version, the SAME `defs`/`funs` chain, so a program the checker accepts the
//! runtime also accepts — the parity is structural (one jaq, one config), not
//! a re-implementation. The layering forbids sharing the engine itself
//! (`nika-schema` is L0 · the runtime jq is L2 `nika-builtin`), so the compile
//! call + its clean-error renderer are mirrored in both — exactly like the net
//! `url_host` split (the layer cannot host a shared std jaq below both).
//!
//! Only a LITERAL program is checked; a `${{ }}`-templated one is built at
//! run time and stays the runtime `NIKA-BUILTIN-JQ-001` check.
//!
//! The diagnostic is a CLEAN one-line reason (NOT the raw jaq `Debug` repr the
//! runtime used to leak · the jq-3 finding) — `Expect::as_str` + the offending
//! source slice.

use jaq_core::Compiler;
use jaq_core::data::JustLut;
use jaq_core::load::{Arena, Error, File, Loader};
use jaq_json::Val;

use nika_schema::error::SchemaError;
use nika_schema::raw::{RawAction, RawWorkflow};

/// Compile-check a jq program with the runtime's exact jaq stack. `Ok(())`
/// when it compiles; `Err(reason)` with a clean one-line message otherwise.
pub(super) fn jq_compiles(program: &str) -> Result<(), String> {
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    // D-2026-08-11-N26 · the withheld natives never enter the function set —
    // the SAME subtraction the two runtime seams apply (`nika-runtime::jq` for
    // `extract:` bindings · `nika-builtin::data` for `nika:jq`), from the SAME
    // list in `nika_cap`. This is what keeps the parity above STRUCTURAL: a
    // program the runtime refuses is refused here, at the same names, because
    // both read one list rather than two copies of a judgment.
    let funs = jaq_core::funs::<JustLut<Val>>()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| !nika_cap::is_withheld_jq_native(f.0));
    let arena = Arena::default();
    let modules = Loader::new(defs)
        .load(
            &arena,
            File {
                code: program,
                path: (),
            },
        )
        .map_err(|errs| render_load(&errs))?;
    Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map(|_| ())
        .map_err(|errs| render_compile(&errs))
}

/// Render a jaq LOAD error set (lex/parse/io) as one clean line.
fn render_load(errs: &[(File<&str, ()>, Error<&str>)]) -> String {
    let Some((_, first)) = errs.first() else {
        return "does not parse".to_owned();
    };
    match first {
        Error::Io(v) => v
            .first()
            .map_or_else(|| "io error".to_owned(), |(_, m)| format!("io: {m}")),
        Error::Lex(v) => v.first().map_or_else(
            || "lexing error".to_owned(),
            |(exp, at)| syntax_msg(exp.as_str(), at),
        ),
        Error::Parse(v) => v.first().map_or_else(
            || "parse error".to_owned(),
            |(exp, at)| syntax_msg(exp.as_str(), at),
        ),
    }
}

/// Render a jaq COMPILE error set (undefined filters/variables) as one line —
/// and, when the undefined name is one this engine WITHHELDS, say so and name
/// the class it would have read (D-2026-08-11-N26) rather than telling the
/// author that a filter jq really does define is « undefined ».
#[allow(clippy::type_complexity)] // the shape is jaq's `compile::Errors`, not ours
fn render_compile<U>(errs: &[(File<&str, ()>, Vec<(&str, U)>)]) -> String {
    errs.first().and_then(|(_, v)| v.first()).map_or_else(
        || "compile error".to_owned(),
        |(name, _)| {
            nika_cap::withheld_jq_reason(name)
                .unwrap_or_else(|| format!("undefined filter or variable `{name}`"))
        },
    )
}

/// One clean « expected X near Y » line from an `Expect::as_str` + slice.
fn syntax_msg(expected: &str, at: &str) -> String {
    let at = at.trim();
    if at.is_empty() {
        format!("expected {expected} (unexpected end of input)")
    } else {
        let snippet: String = at.chars().take(24).collect();
        format!("expected {expected} near `{snippet}`")
    }
}

/// Scan every LITERAL jq program in the workflow (`nika:jq` `expression:` +
/// `nika:fetch` `mode: jq` `jq:`) and compile-check it.
pub(super) fn scan_jq(wf: &RawWorkflow, errors: &mut Vec<SchemaError>) {
    for task in &wf.tasks {
        check_action(&task.value.action, errors);
    }
}

fn check_action(action: &RawAction, errors: &mut Vec<SchemaError>) {
    let RawAction::Invoke(invoke) = action else {
        return;
    };
    let Some(tool_ref) = invoke.tool() else {
        return; // a workflow: call carries no jq program
    };
    let tool = tool_ref.value.as_str();
    let span = tool_ref.span;
    let Some(args) = invoke.args.as_ref().map(|a| &a.value) else {
        return;
    };
    // The jq program lives under `expression` for nika:jq, `jq` for a
    // nika:fetch `mode: jq`.
    let key = match tool {
        "nika:jq" => "expression",
        "nika:fetch" => "jq",
        _ => return,
    };
    let Some(program) = args.get(key).and_then(serde_json::Value::as_str) else {
        return;
    };
    // A templated program is built at run time — the runtime check owns it.
    if program.contains("${{") {
        return;
    }
    // THE PRE-FLIGHT DEPTH GUARD (Gate-11 security finding F1). jaq's parser
    // recurses once per nesting level with no limit of its own: 470 nested
    // brackets — a 1,047-byte paste — overflow a 1 MiB stack (the wasm32
    // budget), and a trapped wasm instance never recovers. The house already
    // caps recursion three times over (MAX_VALUE_DEPTH · nika-tmpl MAX_DEPTH ·
    // the compact-dash cap), all at 128; the jq door gets the same ceiling,
    // paid with one O(n) byte scan BEFORE the recursive compile.
    if let Some(depth) = jq_nesting_over(program, MAX_JQ_NESTING) {
        errors.push(SchemaError::ExpressionViolation {
            reason: format!(
                "jq compile error — nesting depth exceeds {MAX_JQ_NESTING} (measured ≥{depth}); \
                 the runtime enforces the same ceiling"
            ),
            span: Some(span),
        });
        return;
    }
    if let Err(reason) = jq_compiles(program) {
        errors.push(SchemaError::ExpressionViolation {
            reason: format!("jq compile error — {reason}"),
            span: Some(span),
        });
    }
}

/// The nesting ceiling for a jq program at CHECK time — aligned with the
/// house's other recursion caps (`MAX_VALUE_DEPTH` 128 · `nika-tmpl`
/// `MAX_DEPTH` 128): deep enough for any real program, an order of magnitude
/// under the ~470 levels that overflow a 1 MiB stack.
const MAX_JQ_NESTING: usize = 128;

/// One O(n) scan: the maximum bracket/paren nesting depth, string-aware
/// (brackets inside jq string literals do not nest). Returns `Some(depth)`
/// the moment the cap is crossed — the caller refuses without ever handing
/// the program to a recursive parser.
fn jq_nesting_over(program: &str, cap: usize) -> Option<usize> {
    let (mut depth, mut in_str, mut escaped) = (0usize, false, false);
    for c in program.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_str => escaped = true,
            '"' => in_str = !in_str,
            '[' | '(' | '{' if !in_str => {
                depth += 1;
                if depth > cap {
                    return Some(depth);
                }
            }
            ']' | ')' | '}' if !in_str => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn wf_of(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")
    }

    #[test]
    fn valid_jq_compiles() {
        for ok in [
            ".",
            ".a.b",
            "[.items[].name]",
            ".x | length",
            "map(.n)",
            "to_entries",
        ] {
            assert!(jq_compiles(ok).is_ok(), "`{ok}` should compile");
        }
    }

    #[test]
    fn a_withheld_native_is_refused_statically_and_names_its_class() {
        // D-2026-08-11-N26. Before 2026-08-15 every one of these COMPILED
        // here, so `nika check` handed the run a program that read the
        // operator's environment and then printed « pure compute ».
        for (program, class) in [
            ("env.PATH", "ambient process environment"),
            ("env", "ambient process environment"),
            ("now", "host clock"),
            ("0 | localtime", "local timezone"),
            ("0 | strflocaltime(\"%Y\")", "local timezone"),
        ] {
            let reason = jq_compiles(program).expect_err(program);
            assert!(
                reason.contains(class),
                "{program} · must name `{class}` · got: {reason}"
            );
            assert!(
                reason.contains("sees only its input"),
                "{program} · must state the law · got: {reason}"
            );
        }
    }

    #[test]
    fn the_pure_half_of_the_date_family_still_compiles() {
        // The subtraction is SCOPED — a function of its own argument stays.
        for ok in [
            "gmtime",
            "strftime(\"%Y\")",
            "strptime(\"%Y\")",
            "mktime",
            "todateiso8601",
            "fromdateiso8601",
        ] {
            assert!(
                jq_compiles(ok).is_ok(),
                "`{ok}` must survive · {:?}",
                jq_compiles(ok)
            );
        }
    }

    #[test]
    fn a_typo_keeps_jaqs_own_wording() {
        // We never dress an undefined name up as a boundary refusal.
        let reason = jq_compiles("envv").expect_err("undefined");
        assert!(
            reason.contains("undefined filter or variable `envv`"),
            "{reason}"
        );
        assert!(!reason.contains("withheld"), "{reason}");
    }

    #[test]
    fn the_static_refusal_reaches_the_report_as_an_expression_violation() {
        // End of the lane: the withheld native must surface as a FINDING
        // (NIKA-VAR-005 · static expression violation), not merely as an
        // `Err` some caller might drop.
        let wf = wf_of(
            "nika: withheld-env\n\
             tasks:\n  \
               probe:\n    \
                 invoke:\n      \
                   tool: \"nika:jq\"\n      \
                   args:\n        \
                     input: {}\n        \
                     expression: 'env.PATH'\n",
        );
        let mut errors = Vec::new();
        scan_jq(&wf, &mut errors);
        assert_eq!(errors.len(), 1, "{errors:?}");
        let rendered = format!("{:?}", errors[0]);
        assert!(
            rendered.contains("ambient process environment"),
            "{rendered}"
        );
    }

    #[test]
    fn malformed_jq_is_a_clean_error_not_a_debug_repr() {
        // The 006-jq-compile-error fixture shape + the jq-3 leak class.
        let err = jq_compiles(".foo | | bad").expect_err("trailing pipe");
        assert!(
            !err.contains("File {"),
            "no raw jaq Debug repr leaks: {err}"
        );
        assert!(
            !err.contains("Parse(["),
            "no raw jaq Debug repr leaks: {err}"
        );
        assert!(
            jq_compiles(".a |").is_err(),
            "incomplete pipe is a compile error"
        );
    }

    #[test]
    fn unknown_filter_is_an_undefined_error() {
        // `matches` is not in the v0.1 jaq surface — a compile (undefined) error.
        let err = jq_compiles(".s | nonexistent_filter_xyz").expect_err("undefined");
        assert!(err.contains("undefined"), "{err}");
    }

    #[test]
    fn parse_error_renders_a_positive_expected_line() {
        // A trailing pipe is a LOAD/parse error → `render_load` → `syntax_msg`.
        // Pin the CONTENT (not just « no Debug repr ») so a stubbed
        // `render_load`/`syntax_msg` (body → "" / "xyzzy") is caught: the
        // real renderer always emits an « expected … » line.
        let err = jq_compiles(".a |").expect_err("trailing pipe is a syntax error");
        assert!(
            err.contains("expected"),
            "syntax_msg/render_load must name what was expected: {err}"
        );
        assert!(!err.is_empty(), "the rendered reason is never empty");
        assert_ne!(err, "xyzzy", "the reason is the real message, not a stub");
    }

    #[test]
    fn syntax_msg_reports_unexpected_end_of_input() {
        // `${{`-free incomplete program → the empty-`at` branch of `syntax_msg`
        // (« unexpected end of input ») — content pinned so the "" / "xyzzy"
        // body mutants on `syntax_msg` (line 90) cannot survive.
        let msg = syntax_msg("a value", "");
        assert_eq!(msg, "expected a value (unexpected end of input)");
    }

    #[test]
    fn syntax_msg_reports_the_offending_slice() {
        // The non-empty-`at` branch — pins the « expected … near `…` » shape.
        let msg = syntax_msg("a closing bracket", "] rest");
        assert_eq!(msg, "expected a closing bracket near `] rest`");
    }

    #[test]
    fn render_load_on_empty_set_is_the_does_not_parse_fallback() {
        // The `errs.first()` is-empty arm (line 61) — a "" / "xyzzy" body mutant
        // on `render_load` cannot reproduce this exact fallback string.
        assert_eq!(render_load(&[]), "does not parse");
    }

    #[test]
    fn scan_jq_flags_a_nika_jq_task_with_a_broken_expression() {
        // A literal `nika:jq` `expression:` that does not compile must produce
        // a finding. Kills: `scan_jq`→(), `check_action`→(), and the deletion
        // of the `"nika:jq"` match arm (each → no finding).
        let wf = wf_of(
            "\
nika: jq-bad
tasks:
  transform:
    invoke: { tool: \"nika:jq\", args: { expression: \".a |\" } }
",
        );
        let mut errors = Vec::new();
        scan_jq(&wf, &mut errors);
        assert_eq!(errors.len(), 1, "the broken jq must raise one finding");
        let SchemaError::ExpressionViolation { reason, .. } = &errors[0] else {
            panic!("expected an ExpressionViolation, got {:?}", errors[0]);
        };
        assert!(
            reason.contains("jq compile error"),
            "the finding names the jq compile failure: {reason}"
        );
    }

    #[test]
    fn scan_jq_flags_a_nika_fetch_jq_mode_with_a_broken_expression() {
        // The `nika:fetch` `mode: jq` `jq:` arm (line 123) — same broken
        // program under the OTHER key. Deleting the `"nika:fetch"` arm → no
        // finding → caught here (the `nika:jq` test alone would not).
        let wf = wf_of(
            "\
nika: fetch-bad-jq
tasks:
  pull:
    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com\", mode: jq, jq: \".a |\" } }
",
        );
        let mut errors = Vec::new();
        scan_jq(&wf, &mut errors);
        assert_eq!(
            errors.len(),
            1,
            "the broken fetch-jq must raise one finding"
        );
        assert!(
            matches!(errors[0], SchemaError::ExpressionViolation { .. }),
            "got {:?}",
            errors[0]
        );
    }

    #[test]
    fn scan_jq_is_silent_on_valid_programs() {
        // A well-formed `nika:jq` expression raises nothing — guards against a
        // mutant that flags unconditionally (and pins that `scan_jq` runs the
        // real compile, not a constant).
        let wf = wf_of(
            "\
nika: jq-ok
tasks:
  transform:
    invoke: { tool: \"nika:jq\", args: { expression: \"[.items[].name]\" } }
",
        );
        let mut errors = Vec::new();
        scan_jq(&wf, &mut errors);
        assert!(errors.is_empty(), "a valid jq program is clean: {errors:?}");
    }
}
