// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The exec verb's INPUT surface — authored command form → [`ExecInput`]
//! (argv rendered per element · shell kept whole), the subprocess I/O
//! rendering (`cwd` · `env` · `stdin` · spec 02 §exec), the shell
//! leading-program identity, and the `capture:` enum bridge. Split from
//! `dispatch.rs` at the 1500-LOC wall (the `dispatch/{permits,regate,
//! sandbox}` precedent) — the exec DISPATCH (gates · re-gate · spawn)
//! stays in the parent; this module owns the pure input shaping.

use nika_schema::raw::RawCommand;
use nika_schema::types::CaptureMode as SpecCaptureMode;
use nika_verb_exec::{CaptureMode, ExecInput};

use super::{Dispatched, render_opt};
use crate::errors::RuntimeError;
use crate::expr::{self, Scope};

/// Build the [`ExecInput`] from the authored command form — each form
/// maps to its OWN variant: the argv form is rendered PER ELEMENT and
/// passed as a vector (NO join, NO shell), so an interpolated value can
/// never break out of its argv token; the shell form keeps `/bin/sh -c`
/// for genuine pipelines. Returns `(input, program, is_argv)` — a
/// refusal comes back boxed (clippy: the error path stays thin).
pub(super) fn build_exec_input(
    action: &nika_schema::raw::RawExecAction,
    scope: &Scope<'_>,
) -> Result<(ExecInput, String, bool), Box<Dispatched>> {
    match &action.command {
        RawCommand::Shell(text) => match expr::render(&text.value, scope) {
            Ok(line) => {
                let program = shell_leading_program(&line);
                Ok((ExecInput::shell(line), program, false))
            }
            Err(err) => Err(Box::new(Dispatched::template_err("exec · ?", &err))),
        },
        RawCommand::Argv(parts) => {
            let rendered: Result<Vec<_>, _> = parts
                .iter()
                .map(|p| expr::render(&p.value, scope))
                .collect();
            match rendered {
                Ok(argv) => {
                    let program = argv.first().cloned().unwrap_or_else(|| "?".to_owned());
                    Ok((ExecInput::argv(argv), program, true))
                }
                Err(err) => Err(Box::new(Dispatched::template_err("exec · ?", &err))),
            }
        }
        // #[non_exhaustive] · refuse loudly · never guess a shape.
        other => Err(Box::new(Dispatched::unwired(
            "exec · ?",
            format!("command form not wired in the runtime yet: {other:?}"),
        ))),
    }
}

/// Render the exec subprocess I/O (`cwd` · `env` · `stdin`) onto `input`.
///
/// Each field may carry `${{ }}` and is resolved against the scope. `env`
/// keys AND values are both rendered (a value commonly forwards an envelope
/// input · `env: { API_BASE: "${{ inputs.API_BASE }}" }` per spec 02 §exec).
pub(super) fn render_exec_io(
    input: &mut ExecInput,
    action: &nika_schema::raw::RawExecAction,
    scope: &Scope<'_>,
) -> Result<(), RuntimeError> {
    input.cwd = render_opt(action.cwd.as_ref(), scope)?.map(std::path::PathBuf::from);
    input.stdin = render_opt(action.stdin.as_ref(), scope)?;
    for (key, value) in &action.env {
        let k = expr::render(&key.value, scope)?;
        let v = expr::render(&value.value, scope)?;
        input.env.insert(k, v);
    }
    Ok(())
}

/// The leading program token of a shell command line (for the display note +
/// identity), skipping leading `NAME=value` env assignments — `FOO=bar git`
/// → `git`, matching the static `permits_fit::leading_program` so the note
/// names the real program (not the assignment).
fn shell_leading_program(line: &str) -> String {
    line.split_whitespace()
        .find(|token| !is_env_assignment(token))
        .unwrap_or("?")
        .to_owned()
}

/// Whether a shell token is a `NAME=value` environment assignment (a valid
/// env name before the `=`), not the program — `FOO=bar` runs `git`, not `FOO`.
fn is_env_assignment(token: &str) -> bool {
    match token.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        None => false,
    }
}

/// Map the authored `capture:` (spec 02 §exec · schema enum) to the verb's
/// own `CaptureMode`. Two parallel closed enums (the schema layer vs the
/// verb layer) bridged here. `None` (omitted) is the spec default
/// (`stdout`); a future `#[non_exhaustive]` schema variant also falls to
/// `stdout` (the safe spec default) — the named arms below are the wired
/// set, so when a new mode lands in BOTH enums it must be added here to
/// stop riding the default.
pub(super) fn capture_mode(spec: Option<SpecCaptureMode>) -> CaptureMode {
    match spec {
        Some(SpecCaptureMode::Stderr) => CaptureMode::Stderr,
        Some(SpecCaptureMode::Combined) => CaptureMode::Combined,
        Some(SpecCaptureMode::Structured) => CaptureMode::Structured,
        // `stdout`, omitted, OR an unknown future variant → the default.
        None | Some(SpecCaptureMode::Stdout | _) => CaptureMode::Stdout,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::BTreeMap;

    use nika_schema::raw::{RawCommand, RawExecAction};
    use nika_schema::{Span, Spanned};
    use nika_verb_exec::ExecInput;
    use serde_json::Value;

    use super::render_exec_io;
    use crate::expr::Scope;

    fn spanned(s: &str) -> Spanned<String> {
        Spanned::new(s.to_owned(), Span::default())
    }

    #[test]
    fn exec_cwd_env_stdin_render_onto_the_input() {
        // The Findings #3/#4 regression: the parser captured cwd/env/stdin
        // but dispatch dropped them. All three resolve `${{ }}` and land on
        // the ExecInput the verb spawns from.
        let records = BTreeMap::new();
        let vars = BTreeMap::from([
            ("dir".to_owned(), Value::String("./engine".to_owned())),
            ("base".to_owned(), Value::String("https://x".to_owned())),
            ("payload".to_owned(), Value::String("hello".to_owned())),
        ]);
        let scope = Scope::workflow(&records, &vars);

        let mut action = RawExecAction::with_command(RawCommand::Shell(spanned("printenv")));
        action.cwd = Some(spanned("${{ inputs.dir }}"));
        action.env = vec![
            (spanned("API_BASE"), spanned("${{ inputs.base }}")),
            (spanned("STATIC"), spanned("lit")),
        ];
        action.stdin = Some(spanned("${{ inputs.payload }}"));

        let mut input = ExecInput::shell("printenv");
        render_exec_io(&mut input, &action, &scope).expect("renders");

        assert_eq!(input.cwd.as_deref(), Some(std::path::Path::new("./engine")));
        assert_eq!(input.stdin.as_deref(), Some("hello"));
        assert_eq!(
            input.env.get("API_BASE").map(String::as_str),
            Some("https://x")
        );
        assert_eq!(input.env.get("STATIC").map(String::as_str), Some("lit"));
    }

    #[test]
    fn exec_env_forwards_a_deployment_supplied_input() {
        // `exec.env` subprocess variables commonly forward a
        // deployment-supplied declaration
        // (`QRCODE_AI_API_BASE: ${{ inputs.QRCODE_AI_API_BASE }}`); this must
        // render after a green `nika check`. The envelope `env:` layer this
        // test once named died at C2, and `config:` after it — an `inputs:`
        // entry with `required: false` and a `default:` is the supply now.
        let records = BTreeMap::new();
        let inputs = BTreeMap::from([(
            "QRCODE_AI_API_BASE".to_owned(),
            Value::String("https://odin.qrcode-ai.com".to_owned()),
        )]);
        let consts = BTreeMap::new();
        let secrets = BTreeMap::new();
        let scope = Scope::workflow_with_value_authorities(&records, &inputs, &consts, &secrets);

        let mut action = RawExecAction::with_command(RawCommand::Shell(spanned("printenv")));
        action.env = vec![(
            spanned("QRCODE_AI_API_BASE"),
            spanned("${{ inputs.QRCODE_AI_API_BASE }}"),
        )];

        let mut input = ExecInput::shell("printenv");
        render_exec_io(&mut input, &action, &scope).expect("renders");
        assert_eq!(
            input.env.get("QRCODE_AI_API_BASE").map(String::as_str),
            Some("https://odin.qrcode-ai.com")
        );
    }

    #[test]
    fn absent_exec_io_leaves_the_input_at_its_defaults() {
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        let action = RawExecAction::with_command(RawCommand::Shell(spanned("true")));
        let mut input = ExecInput::shell("true");
        render_exec_io(&mut input, &action, &scope).expect("renders");
        assert!(input.cwd.is_none(), "no cwd → inherited");
        assert!(input.stdin.is_none(), "no stdin");
        assert!(input.env.is_empty(), "no env → the composed floor only");
    }

    #[test]
    fn a_bad_template_in_exec_io_is_a_loud_error() {
        // An unresolvable reference in cwd/env/stdin must surface, not be
        // swallowed (the loud doctrine · same class as a bad command).
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        let mut action = RawExecAction::with_command(RawCommand::Shell(spanned("true")));
        action.cwd = Some(spanned("${{ inputs.nope }}"));
        let mut input = ExecInput::shell("true");
        assert!(render_exec_io(&mut input, &action, &scope).is_err());
    }
}
