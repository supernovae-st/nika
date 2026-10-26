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
//! Only a program known now is checked: a literal one, or one whose templates are all
//! `${{ const.NAME }}`, compiled as the run builds it. Any other `${{ }}`-templated one is
//! built at run time and stays the runtime `NIKA-BUILTIN-JQ-001` check.
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

use crate::static_ref::{bare_static_ref, static_literal_of};

/// Compile-check a jq program with the runtime's exact jaq stack, with `vars` bound as global
/// variables beside the run-start one (`$name` spellings, as the run binds them). `Ok(())` when
/// it compiles; `Err(reason)` with a clean one-line message otherwise.
pub(super) fn jq_compiles(program: &str, vars: &[String]) -> Result<(), String> {
    compile(program, vars).map_err(Refusal::reason)
}

/// Why a jq program does not compile: it does not load, or it reads names nothing defines.
enum Refusal {
    /// A lex, parse or io failure, already rendered as one line.
    Load(String),
    /// The undefined filters and variables, in the compiler's order (a variable keeps its `$`).
    Undefined(Vec<String>),
}

impl Refusal {
    fn reason(self) -> String {
        match self {
            Self::Load(reason) => reason,
            Self::Undefined(names) => render_undefined(&names),
        }
    }
}

/// The variables the run binds for a jq program, as far as the check can know them.
enum Bindings {
    /// The exact set (empty when the run binds none).
    Known(Vec<String>),
    /// An `input:` only the run builds: an object there binds keys the check cannot see.
    AtRun,
}

/// The variables `nika:jq` binds at run from its `input:` (`nika-builtin` `data::jq`
/// `input_variables`): each key of an object input that is a jq identifier, as `$<key>`, never
/// the run-start name. A templated input is an object only the run builds. `nika:fetch`
/// `mode: jq` binds none.
fn bindings(tool: &str, args: &serde_json::Value) -> Bindings {
    if tool != "nika:jq" {
        return Bindings::Known(Vec::new());
    }
    match args.get("input") {
        Some(serde_json::Value::Object(fields)) => Bindings::Known(
            fields
                .keys()
                .filter(|key| is_jq_identifier(key))
                .map(|key| format!("${key}"))
                .filter(|name| name != nika_cap::JQ_RUN_START_VAR)
                .collect(),
        ),
        Some(serde_json::Value::String(text)) if text.contains("${{") => Bindings::AtRun,
        _ => Bindings::Known(Vec::new()),
    }
}

/// The runtime's rule for a key that binds (`nika-builtin` `data::is_jq_identifier`, mirrored
/// across the layer boundary like the compile call itself).
fn is_jq_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Compile-check a program whose `input:` only the run builds: every name it reads must be
/// defined, except a `$name` the run's object input may bind.
fn compiles_but_for_run_variables(program: &str) -> Result<(), String> {
    match compile(program, &[]) {
        Err(Refusal::Undefined(names)) => {
            let names: Vec<String> = names
                .into_iter()
                .filter(|name| !name.starts_with('$'))
                .collect();
            if names.is_empty() {
                Ok(())
            } else {
                Err(render_undefined(&names))
            }
        }
        other => other.map_err(Refusal::reason),
    }
}

fn compile(program: &str, vars: &[String]) -> Result<(), Refusal> {
    let clock_defs = jaq_core::load::parse(nika_cap::JQ_CLOCK_DEFS, |parser| parser.defs())
        .ok_or_else(|| {
            Refusal::Load("internal: canonical jq clock definitions do not parse".to_owned())
        })?;
    let defs = jaq_core::defs()
        .chain(
            jaq_std::defs().filter(|definition| nika_cap::install_jq_definition(definition.name)),
        )
        .chain(jaq_json::defs())
        .chain(std_shadows().map_err(Refusal::Load)?)
        .chain(clock_defs);
    // One typed policy decides every effect-bearing native/definition. Clock
    // forms compile against the run-start variable; execution supplies its
    // value, while the checker needs only its declared name.
    let funs = jaq_core::funs::<JustLut<Val>>()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| nika_cap::install_jq_native(f.0));
    let arena = Arena::default();
    let modules = Loader::new(defs)
        .load(
            &arena,
            File {
                code: program,
                path: (),
            },
        )
        .map_err(|errs| Refusal::Load(render_load(&errs)))?;
    Compiler::default()
        .with_funs(funs)
        .with_global_vars(
            std::iter::once(nika_cap::JQ_RUN_START_VAR).chain(vars.iter().map(String::as_str)),
        )
        .compile(modules)
        .map(|_| ())
        .map_err(|errs| Refusal::Undefined(undefined_names(&errs)))
}

/// The std shadows the runtime chains after jaq-json ([`nika_cap::JQ_STD_SHADOWS`]), parsed as
/// every jq consumer parses them.
fn std_shadows() -> Result<Vec<jaq_core::load::parse::Def<&'static str>>, String> {
    jaq_core::load::parse(nika_cap::JQ_STD_SHADOWS, |parser| parser.defs())
        .ok_or_else(|| "internal: the jq std shadows do not parse".to_owned())
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

/// The undefined names of a jaq COMPILE error set, in the compiler's order.
#[allow(clippy::type_complexity)] // the shape is jaq's `compile::Errors`, not ours
fn undefined_names<U>(errs: &[(File<&str, ()>, Vec<(&str, U)>)]) -> Vec<String> {
    errs.iter()
        .flat_map(|(_, names)| names.iter().map(|(name, _)| (*name).to_owned()))
        .collect()
}

/// Render undefined filters/variables as one line — and, when the first name
/// is one this engine WITHHELDS, say so and name the class it would have read
/// (D-2026-08-11-N26) rather than telling the author that a filter jq really
/// does define is « undefined ».
fn render_undefined(names: &[String]) -> String {
    names.first().map_or_else(
        || "compile error".to_owned(),
        |name| {
            nika_cap::withheld_jq_policy_reason(name)
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
        check_action(wf, &task.value.action, errors);
    }
}

fn check_action(wf: &RawWorkflow, action: &RawAction, errors: &mut Vec<SchemaError>) {
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
    // A templated program is built at run time from what only the run knows, and the runtime
    // check owns it. One whose templates are all `const.*` is known now: it is compiled as the
    // run builds it (live gauntlet wave 11 r03, 2026-10-20: `${{ const.target_dir }}` set
    // unquoted in the expression failed the first paid run, « undefined filter or variable
    // `notes` », after « propre ✔ »).
    let built = program.contains("${{").then(|| const_built(wf, program));
    let program = match &built {
        Some(Some(built)) => built.as_str(),
        Some(None) => return,
        None => program,
    };
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
    // The variables are bound as the run binds them (#1578): an object `input:` binds its
    // keys, so `$rows` over `input: { rows: … }` compiles. A templated input is an object
    // only the run builds, so an undefined `$name` is left to the run, as a templated
    // program is; every other undefined name still refuses here.
    let refusal = match bindings(tool, args) {
        Bindings::Known(vars) => jq_compiles(program, &vars).err(),
        Bindings::AtRun => compiles_but_for_run_variables(program).err(),
    };
    if let Some(reason) = refusal {
        errors.push(SchemaError::ExpressionViolation {
            reason: format!("jq compile error — {reason}"),
            span: Some(span),
        });
        return;
    }
    // The runtime's regex engine (jaq's) has no Unicode classes: `\p{L}` compiles as jq, then
    // fails on the first item it meets (live gauntlet wave 8, 2026-10-19, r12: « invalid regex:
    // Unicode character classes are not supported » on the first run after a clean check).
    if let Some(class) = unicode_class(program) {
        errors.push(SchemaError::ExpressionViolation {
            reason: format!(
                "jq regex error — the regex engine has no Unicode classes (`{class}`): name the \
                 characters instead, as `[A-Za-zÀ-ÖØ-öø-ÿ]`"
            ),
            span: Some(span),
        });
    }
}

/// `program` with each `${{ const.NAME }}` set as a run sets it in a string: a string constant
/// as itself, any other value as its JSON. `None` when a template names anything else, or a
/// constant with no literal ([`static_literal_of`]).
fn const_built(wf: &RawWorkflow, program: &str) -> Option<String> {
    let (mut built, mut rest) = (String::with_capacity(program.len()), program);
    while let Some(open) = rest.find("${{") {
        built.push_str(&rest[..open]);
        let close = open + rest[open..].find("}}")? + 2;
        let template = &rest[open..close];
        if !matches!(bare_static_ref(template), Some(("const.", _))) {
            return None;
        }
        match static_literal_of(wf, template)? {
            serde_json::Value::String(text) => built.push_str(text),
            value => built.push_str(&value.to_string()),
        }
        rest = &rest[close..];
    }
    built.push_str(rest);
    Some(built)
}

/// The first Unicode class a string of `program` names for a regex (`\p{L}`, `\pL`, `\P{N}`),
/// as a regex reads it; `None` when it names none. In jq source a regex's backslash is doubled
/// (`test("\\p{L}")`), and a doubled backslash before a lowercase word (`"C:\\Program"` reads
/// `\P` then `r`) names no class.
fn unicode_class(program: &str) -> Option<String> {
    let mut rest = program;
    while let Some(at) = rest.find("\\\\") {
        let after = &rest[at + 2..];
        let mut chars = after.chars();
        if let Some(sign @ ('p' | 'P')) = chars.next() {
            match chars.next() {
                Some('{') => {
                    let name: String = after[2..].chars().take_while(|c| *c != '}').collect();
                    return Some(format!("\\{sign}{{{name}}}"));
                }
                Some(letter) if letter.is_ascii_uppercase() => {
                    return Some(format!("\\{sign}{letter}"));
                }
                _ => {}
            }
        }
        rest = after;
    }
    None
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
            assert!(jq_compiles(ok, &[]).is_ok(), "`{ok}` should compile");
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
        ] {
            let reason = jq_compiles(program, &[]).expect_err(program);
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

    /// The accepted clock spellings compile against the canonical rebinding.
    #[test]
    fn the_clock_forms_compile_against_the_n27_rebinding() {
        for rebound in ["now", "0 | localtime", "0 | strflocaltime(\"%Y\")"] {
            assert!(
                jq_compiles(rebound, &[]).is_ok(),
                "{rebound} · check and run must accept the same N27 forms"
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
                jq_compiles(ok, &[]).is_ok(),
                "`{ok}` must survive · {:?}",
                jq_compiles(ok, &[])
            );
        }
    }

    #[test]
    fn a_typo_keeps_jaqs_own_wording() {
        // We never dress an undefined name up as a boundary refusal.
        let reason = jq_compiles("envv", &[]).expect_err("undefined");
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
        let err = jq_compiles(".foo | | bad", &[]).expect_err("trailing pipe");
        assert!(
            !err.contains("File {"),
            "no raw jaq Debug repr leaks: {err}"
        );
        assert!(
            !err.contains("Parse(["),
            "no raw jaq Debug repr leaks: {err}"
        );
        assert!(
            jq_compiles(".a |", &[]).is_err(),
            "incomplete pipe is a compile error"
        );
    }

    #[test]
    fn unknown_filter_is_an_undefined_error() {
        // `matches` is not in the v0.1 jaq surface — a compile (undefined) error.
        let err = jq_compiles(".s | nonexistent_filter_xyz", &[]).expect_err("undefined");
        assert!(err.contains("undefined"), "{err}");
    }

    #[test]
    fn parse_error_renders_a_positive_expected_line() {
        // A trailing pipe is a LOAD/parse error → `render_load` → `syntax_msg`.
        // Pin the CONTENT (not just « no Debug repr ») so a stubbed
        // `render_load`/`syntax_msg` (body → "" / "xyzzy") is caught: the
        // real renderer always emits an « expected … » line.
        let err = jq_compiles(".a |", &[]).expect_err("trailing pipe is a syntax error");
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

    /// A program whose templates are all `const.*` is known now: it is compiled as the run builds
    /// it. An unquoted constant that reads as jq names (live gauntlet wave 11 r03) is refused; the
    /// same constant quoted compiles; a template only the run knows is left to the run.
    #[test]
    fn a_program_templated_by_constants_is_compiled_as_the_run_builds_it() {
        let flagged = |expression: &str| {
            let wf = wf_of(&format!(
                "nika: notes-out\nconst:\n  target_dir: \"notes-en\"\ntasks:\n  assemble:\n    invoke: {{ tool: \"nika:jq\", args: {{ expression: '{expression}' }} }}\n"
            ));
            let mut errors = Vec::new();
            scan_jq(&wf, &mut errors);
            errors
        };
        let unquoted = flagged("[.[] | {path: (${{ const.target_dir }} + \"/\" + .name)}]");
        assert_eq!(unquoted.len(), 1, "{unquoted:?}");
        let SchemaError::ExpressionViolation { reason, .. } = &unquoted[0] else {
            panic!("expected an ExpressionViolation, got {:?}", unquoted[0]);
        };
        assert!(reason.contains("jq compile error"), "{reason}");
        let quoted = flagged("[.[] | {path: (\"${{ const.target_dir }}/\" + .name)}]");
        assert!(quoted.is_empty(), "{quoted:?}");
        let run_only = flagged("[.[] | {path: (${{ with.dir }} + .name)}]");
        assert!(run_only.is_empty(), "{run_only:?}");
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

    fn jq_findings(input: &str, expression: &str) -> Vec<SchemaError> {
        let wf = wf_of(&format!(
            "nika: jq-vars\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  x:\n    invoke: {{ tool: \"nika:jq\", args: {{ input: [1], expression: \".\" }} }}\n  rows_var:\n    invoke:\n      tool: nika:jq\n      args:\n        input: {input}\n        expression: '{expression}'\n"
        ));
        let mut errors = Vec::new();
        scan_jq(&wf, &mut errors);
        errors
    }

    /// #1578 · `nika:jq` binds each key of an object `input:` as `$<key>` at run, so the
    /// check compiles the program with the same names bound. Measured on 0.123.2: the
    /// documented `$rows` form failed check with « undefined … `$rows` » and never ran.
    #[test]
    fn input_object_keys_compile_as_variables() {
        let findings = jq_findings(
            "{ rows: [{a: 1}, {a: 2}], batch: \"B1\" }",
            "$rows | map(. + {batch: $batch})",
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// A name the input does not carry is still undefined, and the finding names it.
    #[test]
    fn an_undeclared_variable_still_refuses() {
        let findings = jq_findings(
            "{ rows: [{a: 1}, {a: 2}], batch: \"B1\" }",
            "$nope | map(. + {batch: $batch})",
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        let SchemaError::ExpressionViolation { reason, .. } = &findings[0] else {
            panic!("expected an ExpressionViolation, got {:?}", findings[0]);
        };
        assert!(reason.contains("`$nope`"), "{reason}");
    }

    /// A templated `input:` is an object only the run builds, so its keys are only known
    /// there: an undefined `$name` is left to the run, every other undefined name refuses.
    #[test]
    fn a_templated_input_defers_its_variables_to_the_run() {
        let templated = "\"${{ tasks.x.output }}\"";
        assert!(jq_findings(templated, "$rows").is_empty());
        for (expression, named) in [
            ("undefined_fn(1)", "`undefined_fn`"),
            ("$rows | undefined_fn(1)", "`undefined_fn`"),
        ] {
            let findings = jq_findings(templated, expression);
            assert_eq!(findings.len(), 1, "{expression} · {findings:?}");
            let SchemaError::ExpressionViolation { reason, .. } = &findings[0] else {
                panic!("expected an ExpressionViolation, got {:?}", findings[0]);
            };
            assert!(reason.contains(named), "{expression} · {reason}");
        }
    }

    /// A Unicode class compiles as jq, then the runtime's regex engine refuses it on the first
    /// item it meets (live gauntlet wave 8, r12): the check names it before any run.
    #[test]
    fn scan_jq_flags_a_unicode_regex_class_the_runtime_refuses() {
        let wf = wf_of(
            "\
nika: jq-unicode
tasks:
  names:
    invoke: { tool: \"nika:jq\", args: { expression: '[.[] | select(.name | test(\"^\\\\p{L}+$\"))]' } }
",
        );
        let mut errors = Vec::new();
        scan_jq(&wf, &mut errors);
        assert_eq!(errors.len(), 1, "{errors:?}");
        let SchemaError::ExpressionViolation { reason, .. } = &errors[0] else {
            panic!("expected an ExpressionViolation, got {:?}", errors[0]);
        };
        assert!(
            reason.contains("jq regex error") && reason.contains("(`\\p{L}`)"),
            "{reason}"
        );
    }

    /// The class a regex string names, as the regex reads it; a doubled backslash before a
    /// lowercase word, or any other escape, names none.
    #[test]
    fn a_unicode_class_is_read_from_a_jq_string() {
        assert_eq!(
            unicode_class(r#"test("^\\p{L}+$")"#).as_deref(),
            Some(r"\p{L}")
        );
        assert_eq!(unicode_class(r#"test("\\pN")"#).as_deref(), Some(r"\pN"));
        assert_eq!(
            unicode_class(r#"sub("\\P{Lu}"; "")"#).as_deref(),
            Some(r"\P{Lu}")
        );
        assert_eq!(unicode_class(r#""C:\\Program Files""#), None);
        assert_eq!(unicode_class(r#"test("^[a-z]+$")"#), None);
        assert_eq!(unicode_class(r#"test("\\d+")"#), None);
    }
}

#[cfg(test)]
mod tests_number {
    #[test]
    fn tonumber_and_its_explicit_error_policy_compile() {
        for program in [
            "tonumber",
            "[.[] | tonumber] | add",
            "[.[] | try tonumber] | add",
            "[fromjson]",
        ] {
            assert!(super::jq_compiles(program, &[]).is_ok(), "{program}");
        }
    }

    fn probe_set() -> serde_json::Value {
        serde_json::from_str(nika_cap::JQ_STD_SHADOW_PROBES).expect("the probe set is JSON")
    }

    /// Every program of the shared conformance probes compiles in the checker, as it runs in
    /// the runtime's evaluators.
    #[test]
    fn every_shared_shadow_probe_compiles() {
        for probe in probe_set()["probes"].as_array().expect("probes") {
            let program = probe["program"].as_str().expect("a program");
            assert!(
                super::jq_compiles(program, &[]).is_ok(),
                "{}",
                probe["name"]
            );
        }
    }

    /// The checker loads every shadow the runtime defines: a program is compiled against the
    /// runtime's own `scan` and `tonumber`, never jaq's.
    #[test]
    fn the_checker_loads_every_shared_shadow() {
        let set = probe_set();
        let expected: std::collections::BTreeSet<&str> = set["shadows"]
            .as_array()
            .expect("shadows")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let loaded: Vec<String> = super::std_shadows()
            .expect("the shadows parse")
            .iter()
            .map(|definition| format!("{}/{}", definition.name, definition.args.len()))
            .collect();
        let loaded: std::collections::BTreeSet<&str> = loaded.iter().map(String::as_str).collect();
        assert_eq!(loaded, expected);
    }
}
