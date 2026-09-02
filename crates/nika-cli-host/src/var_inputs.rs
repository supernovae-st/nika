// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--var KEY=VALUE` input seam — parse, key-validate, type-honor
//! (extracted from `run/mod.rs` 2026-07-11 at the 1500-LOC ratchet, and
//! from `nika-cli` 2026-09-02 at the 15k crate wall: the golden test
//! binds a case through the same door, so the door lives in the host): one
//! unit — the raw pairs in, the validated `BTreeMap<String, Value>` out,
//! the declared `inputs:` the sole authority (keys · types · #603 required).
//!
//! F-P13 (NEP-0014 law 2): the binding is a LAW, not a convenience —
//! every bound input carries an enumerated ORIGIN (cli-operator ·
//! ci-context · env · file · [`nika_runtime::InputOrigin`]) that rises
//! to the boot manifest, and the environment can never silently become
//! the operator: the declared env channel is the EXPLICIT `@env:VAR`
//! spelling, and under CI an env read the workflow never declared
//! (outside `permits.env:`) is a refusal, naming the declaration that
//! would admit it. The untyped JSON-or-string guess stays the graved
//! fallback (spec 01 §inputs — documented, never silent).

use std::collections::{BTreeMap, BTreeSet};

use nika_schema::raw::RawWorkflow;
use nika_schema::types::VarDecl;
use serde_json::Value;

/// The validated `--var` surface (F-P13): the bound values AND the
/// origin map every one of them speaks — computed at the binding seam,
/// the one place the caller context (CI · the declared `@env:` reads)
/// is known.
#[derive(Debug)]
pub struct ValidatedInputs {
    /// The bound values (composer's `with_var_overrides`).
    pub values: BTreeMap<String, Value>,
    /// The origin of every input the run binds (overrides + declared
    /// defaults — the boot manifest's `inputs` field).
    pub origins: BTreeMap<String, nika_runtime::InputOrigin>,
}

/// Whether the run executes under a CI context — the `CI` environment
/// variable (the de-facto standard: GitHub Actions · GitLab CI ·
/// Buildkite all set it). Non-empty and not a false spelling.
fn in_ci() -> bool {
    #[allow(clippy::disallowed_methods)]
    // the sanctioned env edge (the caller-context read IS this module's job)
    let ci = std::env::var("CI").unwrap_or_default();
    !matches!(ci.as_str(), "" | "false" | "0")
}

/// The `@env:` prefix — the DECLARED environment channel (NEP-0014 law
/// 2): `--var key=@env:VAR` reads the value from the OS environment
/// through an explicit spelling, never an ambient guess.
const ENV_PREFIX: &str = "@env:";

/// Parse the repeatable `--var KEY=VALUE` overrides and validate every
/// key against the workflow's declared `inputs:` — an unknown key is
/// refused with the declared set (a typo'd override silently doing
/// nothing would be the worst outcome). A TYPED input's declared `type:`
/// DRIVES the value parse (spec 01 §inputs · R3b: the full `TypeExpr`,
/// the one fit): `--var count=notanumber` on an `integer` refuses up
/// front · a `string` takes the raw text (`--var name=5` is `"5"`).
/// An UNTYPED constant keeps the JSON-or-string guess (`limit=5` → `5`).
///
/// F-P13 (NEP-0014 law 2): a value spelled `@env:VAR` reads the named
/// OS environment variable (the declared env channel) and parses it by
/// the same declared-type law. Under CI, an env read the workflow never
/// declared — the name is absent from `permits.env:` — is a REFUSAL
/// naming the declaration that would admit it: the file names every env
/// the run consumes, so a pipeline variable can never silently become
/// the operator.
///
/// # Errors
/// The first refusal as one sentence: an unknown key, a value the
/// declared type refuses, an undeclared `@env:` read under CI.
pub fn parse_var_overrides(pairs: &[String], wf: &RawWorkflow) -> Result<ValidatedInputs, String> {
    parse_var_overrides_with(
        pairs,
        wf,
        &|name| {
            #[allow(clippy::disallowed_methods)]
            // the sanctioned env edge — the declared @env: channel reads here
            std::env::var(name).ok().filter(|v| !v.is_empty())
        },
        in_ci(),
    )
}

/// The injectable core of [`parse_var_overrides`] — the environment
/// lookup and the CI verdict arrive as parameters so the law's fixtures
/// are hermetic (no `set_var` races).
fn parse_var_overrides_with(
    pairs: &[String],
    wf: &RawWorkflow,
    env: &dyn Fn(&str) -> Option<String>,
    ci: bool,
) -> Result<ValidatedInputs, String> {
    let named = BTreeMap::new();
    let type_names = BTreeSet::new();
    let mut overrides = BTreeMap::new();
    let mut env_sourced = BTreeSet::new();
    for pair in pairs {
        let (key, raw) = match pair.split_once('=') {
            Some((k, v)) if !k.trim().is_empty() => (k.trim(), v),
            _ => return Err(format!("--var expects KEY=VALUE, got `{pair}`")),
        };
        let Some((_, decl)) = wf.inputs.iter().find(|(k, _)| k.value == key) else {
            let declared: Vec<&str> = wf.inputs.iter().map(|(k, _)| k.value.as_str()).collect();
            return Err(if declared.is_empty() {
                format!("--var {key}: this workflow declares no `inputs:`")
            } else {
                format!(
                    "--var {key}: unknown input — the workflow declares: {}",
                    declared.join(" · ")
                )
            });
        };
        // F-P13 · the declared env channel: `@env:VAR` reads the named
        // variable, CI judges the declaration BEFORE the read.
        let (raw, from_env) = match raw.strip_prefix(ENV_PREFIX) {
            Some(var) => (resolve_env_channel(key, var, wf, env, ci)?, true),
            None => (std::borrow::Cow::Borrowed(raw), false),
        };
        let value = match decl {
            // The declared TypeExpr drives the parse (the one type core,
            // never a second fit) — a mismatch names the form + the value.
            VarDecl::Typed { r#type, .. } => {
                nika_schema::types::coerce_declared(&r#type.value, &type_names, &named, &raw)
                    .map_err(|why| format!("--var {key}: {why}"))?
            }
            // Untyped constant: the JSON-or-string guess (no declared type
            // — the graved fallback of spec 01 §inputs).
            VarDecl::Untyped(_) => serde_json::from_str::<Value>(&raw)
                .unwrap_or_else(|_| Value::String(raw.as_ref().to_owned())),
        };
        if from_env {
            env_sourced.insert(key.to_owned());
        }
        overrides.insert(key.to_owned(), value);
    }
    let origins = nika_runtime::input_origins(wf, &overrides, &env_sourced, ci);
    Ok(ValidatedInputs {
        values: overrides,
        origins,
    })
}

/// The F-P13 env channel: resolve one `@env:VAR` read. The CI law first
/// (NEP-0014 law 2 — an env read the workflow never declared, absent
/// from `permits.env:`, is a refusal naming the declaration that would
/// admit it), then the read itself (unset/empty refuses — an empty
/// value is a fact the file must spell, never an ambient guess).
fn resolve_env_channel<'e>(
    key: &str,
    var: &str,
    wf: &RawWorkflow,
    env: &'e dyn Fn(&str) -> Option<String>,
    ci: bool,
) -> Result<std::borrow::Cow<'e, str>, String> {
    if var.is_empty() {
        return Err(format!(
            "--var {key}: `{ENV_PREFIX}` expects the variable name — `{ENV_PREFIX}MY_VAR`"
        ));
    }
    if ci {
        let declared: Vec<&str> = wf
            .permits
            .as_ref()
            .and_then(|p| p.value.env.as_ref())
            .map_or(Vec::new(), |names| {
                names.iter().map(String::as_str).collect()
            });
        if !declared.contains(&var) {
            return Err(format!(
                "--var {key}={ENV_PREFIX}{var}: CI context — the environment variable \
                 {var} feeds input `{key}` without being declared (F-P13 · NEP-0014 \
                 law 2) · fix: add `{var}` to the workflow's `permits.env:` so the \
                 file names every env the run consumes"
            ));
        }
    }
    env(var).map(std::borrow::Cow::Owned).ok_or_else(|| {
        format!("--var {key}={ENV_PREFIX}{var}: environment variable {var} is not set")
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    use super::*;

    /// The hermetic environment: a fixed map + an explicit CI verdict.
    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn parse_var_overrides_types_json_else_string() {
        let wf = parse(
            "nika: t\ninputs:\n  topic: { type: string, required: true }\n  limit: { type: integer, default: 3 }\n  flags: { type: { array: string }, required: true }\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n",
        );

        // string verbatim · integer typed · array typed (the JSON coercion).
        let overrides = parse_var_overrides(
            &[
                "topic=quantum news".to_owned(),
                "limit=5".to_owned(),
                "flags=[\"x\",\"y\"]".to_owned(),
            ],
            &wf,
        )
        .expect("valid overrides")
        .values;
        assert_eq!(overrides["topic"], json!("quantum news"));
        assert_eq!(overrides["limit"], json!(5));
        assert_eq!(overrides["flags"], json!(["x", "y"]));

        // The unknown-key refusal NAMES the declared set (actionable).
        let err = parse_var_overrides(&["ghost=1".to_owned()], &wf).expect_err("unknown key");
        assert!(err.contains("ghost"), "{err}");
        assert!(err.contains("topic"), "lists the declared inputs: {err}");

        // `=` in the VALUE is preserved (split_once · key=v=w).
        let eq = parse_var_overrides(&["topic=a=b".to_owned()], &wf)
            .expect("value may carry '='")
            .values;
        assert_eq!(eq["topic"], json!("a=b"));
    }

    #[test]
    fn typed_var_overrides_honor_the_declared_type() {
        // Input gauntlet (2026-07-11): a declared `type:` is the input
        // CONTRACT — the CLI value must honor it, not be embedded
        // type-blind (`count=notanumber` used to ride through as a string).
        // Post-R3b the type speaks the full TypeExpr (`bool` is the one
        // boolean spelling).
        let wf = parse(
            "nika: t\ninputs:\n  count: { type: integer, required: true }\n  ratio: { type: number, default: 1.0 }\n  on: { type: bool, default: false }\n  name: { type: string, required: true }\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n",
        );

        // The type DRIVES the parse — well-typed values land as their type.
        let ok = parse_var_overrides(
            &[
                "count=42".to_owned(),
                "ratio=2.5".to_owned(),
                "on=true".to_owned(),
                "name=5".to_owned(), // a STRING var takes the raw text verbatim
            ],
            &wf,
        )
        .expect("well-typed overrides")
        .values;
        assert_eq!(ok["count"], json!(42));
        assert_eq!(ok["ratio"], json!(2.5));
        assert_eq!(ok["on"], json!(true));
        assert_eq!(ok["name"], json!("5"), "string var never JSON-coerces");

        // A mismatch is refused UP FRONT, naming the type + the value.
        for (bad, want) in [
            ("count=notanumber", "integer"),
            ("ratio=lots", "number"),
            ("on=maybe", "bool"),
        ] {
            let err = parse_var_overrides(&[bad.to_owned()], &wf).expect_err("type mismatch");
            assert!(
                err.contains(want) && err.contains(bad.split('=').next_back().unwrap()),
                "{err}"
            );
        }
    }

    // ── F-P13 · the origin law (NEP-0014 law 2) ─────────────────────

    const ORIGIN_WF: &str = "nika: t\ninputs:\n  count: { type: integer, required: true }\n  region: { type: string, default: \"eu\" }\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";

    #[test]
    fn every_origin_is_enumerated_at_the_binding() {
        let wf = parse(ORIGIN_WF);
        // The operator at a terminal: --var → cli-operator · default → file.
        let v = parse_var_overrides_with(&["count=42".to_owned()], &wf, &env_of(&[]), false)
            .expect("binds");
        assert_eq!(v.origins["count"], nika_runtime::InputOrigin::CliOperator);
        assert_eq!(v.origins["region"], nika_runtime::InputOrigin::File);
        // Under CI the SAME --var speaks ci-context — a pipeline can
        // never silently read as the operator.
        let v = parse_var_overrides_with(&["count=42".to_owned()], &wf, &env_of(&[]), true)
            .expect("binds");
        assert_eq!(v.origins["count"], nika_runtime::InputOrigin::CiContext);
        assert_eq!(v.origins["region"], nika_runtime::InputOrigin::File);
    }

    #[test]
    fn the_env_channel_binds_through_the_declared_spelling() {
        // POSITIVE — `@env:VAR` reads the named variable and the declared
        // type STILL governs the parse (42 rides as an integer, never a
        // string); the origin is `env`, CI or not, once the name is
        // declared in `permits.env:`.
        let wf = parse(
            "nika: t\npermits:\n  env: [\"BUILD_COUNT\"]\ninputs:\n  count: { type: integer, required: true }\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n",
        );
        for ci in [false, true] {
            let v = parse_var_overrides_with(
                &["count=@env:BUILD_COUNT".to_owned()],
                &wf,
                &env_of(&[("BUILD_COUNT", "42")]),
                ci,
            )
            .expect("the declared env channel binds");
            assert_eq!(v.values["count"], json!(42), "the type governs (ci={ci})");
            assert_eq!(v.origins["count"], nika_runtime::InputOrigin::Env);
        }
    }

    #[test]
    fn an_undeclared_env_read_in_ci_is_a_finding() {
        // NEGATIVE — the law's teeth: CI context, the variable feeds an
        // input, the file never declared it (no `permits.env:`) →
        // refusal naming the declaration that would admit it. Outside CI
        // the explicit spelling binds (the operator IS the authority at
        // a terminal) — the journal still names the channel `env`.
        let wf = parse(ORIGIN_WF);
        let err = parse_var_overrides_with(
            &["count=@env:BUILD_COUNT".to_owned()],
            &wf,
            &env_of(&[("BUILD_COUNT", "42")]),
            true,
        )
        .expect_err("undeclared env read in CI refuses");
        assert!(err.contains("BUILD_COUNT"), "{err}");
        assert!(err.contains("permits.env:"), "the fix is named: {err}");

        let v = parse_var_overrides_with(
            &["count=@env:BUILD_COUNT".to_owned()],
            &wf,
            &env_of(&[("BUILD_COUNT", "42")]),
            false,
        )
        .expect("a terminal's explicit spelling binds");
        assert_eq!(v.origins["count"], nika_runtime::InputOrigin::Env);
    }

    #[test]
    fn the_env_channel_refuses_unset_and_empty_spellings() {
        let wf = parse(ORIGIN_WF);
        // An unset variable is a loud refusal, never an empty string.
        let err =
            parse_var_overrides_with(&["count=@env:MISSING".to_owned()], &wf, &env_of(&[]), false)
                .expect_err("unset refuses");
        assert!(err.contains("MISSING") && err.contains("not set"), "{err}");
        // The bare prefix names its own grammar.
        let err = parse_var_overrides_with(&["count=@env:".to_owned()], &wf, &env_of(&[]), false)
            .expect_err("a name is required");
        assert!(err.contains("expects the variable name"), "{err}");
    }
}
