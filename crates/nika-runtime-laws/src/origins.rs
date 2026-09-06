// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F-P13 (NEP-0014 law 2) — every input the run binds carries an
//! enumerated ORIGIN, journaled on the boot manifest so the run's
//! attested record names the channel each value came from. A CI
//! environment can never silently become the operator: the journal says
//! `ci-context` where a pipeline supplied the value, `cli-operator`
//! where a human typed it, `env` where the declared `@env:` channel
//! read it, and `file` where the workflow's own `default:` filled it.
//!
//! The vocabulary itself descended to `nika-types` at the 15k wall
//! (ADR-110 · #889); the derivation stays here, over the file's own
//! declarations.

use std::collections::{BTreeMap, BTreeSet};

use nika_schema::raw::RawWorkflow;
use nika_schema::types::VarDecl;
use serde_json::Value;

pub use nika_types::InputOrigin;

/// The origin map of a run's bound inputs — one entry per input that
/// carries a value into the run (caller overrides + declared defaults).
/// An optional input nobody supplied binds nothing and has NO entry
/// (absent is honest — there is no origin to attest for a value that
/// does not exist).
///
/// - `overrides` — the validated `--var` bindings (keys are declared
///   inputs by the CLI's preflight).
/// - `env_sourced` — the subset of `overrides` bound through the
///   declared `@env:` channel (checked first: an env read is `env`
///   wherever it happened).
/// - `ci` — the run executes under a CI context (the `CI` environment
///   variable · the de-facto standard): an operator-spelled `--var`
///   there speaks `ci-context`, never `cli-operator`.
#[must_use]
pub fn input_origins(
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
    env_sourced: &BTreeSet<String>,
    ci: bool,
) -> BTreeMap<String, InputOrigin> {
    let mut out = BTreeMap::new();
    for (key, decl) in &wf.inputs {
        let name = &key.value;
        let origin = if env_sourced.contains(name) {
            InputOrigin::Env
        } else if overrides.contains_key(name) {
            if ci {
                InputOrigin::CiContext
            } else {
                InputOrigin::CliOperator
            }
        } else {
            match decl {
                // A declared static value (bare const · typed default) is
                // the FILE origin; an undeclared-optional input binds
                // nothing (no entry — absent is honest).
                VarDecl::Untyped(_)
                | VarDecl::Typed {
                    default: Some(_), ..
                } => InputOrigin::File,
                VarDecl::Typed { default: None, .. } => continue,
            }
        };
        out.insert(name.clone(), origin);
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    fn wf(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses")
    }

    const WF: &str = "nika: t\ninputs:\n  count: { type: integer, required: true }\n  region: { type: string, default: \"eu\" }\n  note: { type: string }\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";

    #[test]
    fn every_bound_input_carries_its_origin() {
        let wf = wf(WF);
        let overrides = BTreeMap::from([("count".to_owned(), Value::from(42))]);
        // At a terminal: the override is the operator's, the default the
        // file's; an unsupplied optional input has NO origin row.
        let origins = input_origins(&wf, &overrides, &BTreeSet::new(), false);
        assert_eq!(origins["count"], InputOrigin::CliOperator);
        assert_eq!(origins["region"], InputOrigin::File);
        assert!(!origins.contains_key("note"), "unbound = no origin");
    }

    #[test]
    fn the_ci_context_never_reads_as_the_operator() {
        let wf = wf(WF);
        let overrides = BTreeMap::from([("count".to_owned(), Value::from(42))]);
        let origins = input_origins(&wf, &overrides, &BTreeSet::new(), true);
        assert_eq!(
            origins["count"],
            InputOrigin::CiContext,
            "a pipeline-supplied --var names its channel"
        );
        // …while the declared default stays the file's, CI or not.
        assert_eq!(origins["region"], InputOrigin::File);
    }

    #[test]
    fn the_declared_env_channel_wins_over_the_caller_context() {
        let wf = wf(WF);
        let overrides = BTreeMap::from([("count".to_owned(), Value::from(42))]);
        let env = BTreeSet::from(["count".to_owned()]);
        for ci in [false, true] {
            let origins = input_origins(&wf, &overrides, &env, ci);
            assert_eq!(
                origins["count"],
                InputOrigin::Env,
                "an @env: read is `env` in every context (ci={ci})"
            );
        }
    }
}
