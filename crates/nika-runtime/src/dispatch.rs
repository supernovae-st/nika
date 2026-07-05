// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-verb dispatch — render the action's fields over the scope, run
//! the verb, normalize the outcome to the value model (spec 04).
//!
//! Pen-free (INV-024 stays in the settle pass): a dispatch returns
//! [`Dispatched`] · the note (`invoke · <tool>` · `exec · <argv0>` ·
//! `infer · <model>` · `agent · N turns`) + a value-or-error. Verb
//! failures carry the verb's own `nika_code()` wire form · template
//! failures inside a body carry NIKA-1702/1703 — both fail the TASK
//! (cascade) · never the run.

use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::raw::{RawAction, RawCommand};
use nika_schema::types::CaptureMode as SpecCaptureMode;
use nika_verb_agent::{AgentInput, AgentValue};
use nika_verb_exec::{CaptureMode, ExecInput, ExecValue};
use nika_verb_infer::{InferInput, InferValue};
use nika_verb_invoke::InvokeInput;
use serde_json::Value;

use crate::Runtime;
use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::TaskErrorRecord;

/// One dispatch's outcome — the display note + value-or-error.
/// (Agent decisions do NOT ride here: the buffer lives in the CALLER,
/// outside the timeout-cancellable region, so a timed-out attempt's
/// telemetry survives — the wiring review's F1.)
pub(crate) struct Dispatched {
    /// `<verb> · <subject>` (the display contract's `TaskStarted` note).
    pub note: String,
    /// The verb's value (spec-04 typed) or the task error.
    pub result: Result<DispatchOk, TaskErrorRecord>,
}

/// A successful dispatch — the output value + token spend when the
/// verb reports it (infer · agent) + an optional non-fatal WARNING
/// (OBS-E: a thinking model that ate its budget on reasoning and
/// returned a blank answer · the task still succeeded).
pub(crate) struct DispatchOk {
    pub value: Value,
    pub tokens: Option<i64>,
    pub warning: Option<String>,
}

impl Dispatched {
    fn ok(note: String, value: Value, tokens: Option<i64>) -> Self {
        Self {
            note,
            result: Ok(DispatchOk {
                value,
                tokens,
                warning: None,
            }),
        }
    }

    /// `ok` with an OBS-E diagnostic attached (infer/agent only · the
    /// effect verbs never reason).
    fn ok_warned(note: String, value: Value, tokens: Option<i64>, warning: Option<String>) -> Self {
        Self {
            note,
            result: Ok(DispatchOk {
                value,
                tokens,
                warning,
            }),
        }
    }

    fn verb_err(note: String, err: &dyn NikaErrorCode) -> Self {
        Self {
            note,
            result: Err(TaskErrorRecord {
                // The USER-FACING spec code (`NIKA-EXEC-001` · not the engine
                // `NIKA-440`) — the identifier the author is forced (by `nika
                // check`) to write in `on_codes:`, and the one `tasks.X.error
                // .code` exposes (spec 05 §error structure). Selective
                // recovery/retry compares against THIS (BUG-C).
                code: err.spec_code(),
                message: err.to_string(),
                transient: err.is_transient(),
            }),
        }
    }

    fn template_err(note: &str, err: &RuntimeError) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(TaskErrorRecord {
                // The SPEC-PLANE wire code (`NIKA-VAR-001` unresolved ·
                // `NIKA-VAR-005` out-of-subset) the author filters on — never
                // the engine-internal `nika_code()` (spec 05 §142 · the
                // `tasks.X.error.code` leak this closed). The message is
                // code-less (`wire_message`) · the code rides its own field.
                code: err.spec_code(),
                message: err.wire_message(),
                transient: false, // static expression class · retry never helps
            }),
        }
    }

    fn unwired(note: &str, detail: String) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(TaskErrorRecord {
                code: nika_error::codes::NIKA_1703.to_string(),
                message: detail,
                transient: false,
            }),
        }
    }

    /// An effect refused by the declared `permits:` capability boundary
    /// (spec 01 §permits · `NIKA-SEC-004`). A security boundary is never
    /// retryable, and (for `agent:` loops) never fed back to the model.
    fn security_err(note: &str, reason: impl Into<String>) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(TaskErrorRecord {
                code: "NIKA-SEC-004".to_owned(),
                message: reason.into(),
                transient: false,
            }),
        }
    }
}

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C>
where
    S: ShellRunDyn + Sync,
    T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn + ProviderMeta,
    D: ToolDefinitionProviderDyn,
{
    /// Dispatch one action through its verb (see module docs).
    ///
    /// `deadline` — the task-level `timeout:` budget (spec 03). The
    /// attempt loop enforces it as the TOTAL budget; it is ALSO handed
    /// to the infer path so the provider transport deadline cannot
    /// undercut it (F1 · a 30s HTTP default killed every `timeout: 7m`
    /// local-model task at 30s).
    pub(crate) async fn dispatch(
        &self,
        action: &RawAction,
        scope: &Scope<'_>,
        agent_buffer: &crate::agent_events::BufferingObserver,
        deadline: Option<std::time::Duration>,
    ) -> Dispatched {
        match action {
            RawAction::Invoke(inner) => self.dispatch_invoke(inner, scope).await,
            RawAction::Exec(inner) => self.dispatch_shell(inner, scope).await,
            RawAction::Infer(inner) => self.dispatch_infer(inner, scope, deadline).await,
            RawAction::Agent(inner) => self.dispatch_agent(inner, scope, agent_buffer).await,
            // #[non_exhaustive] · a future verb must land HERE loudly ·
            // the runtime refuses rather than silently no-ops.
            other => Dispatched::unwired(
                "unknown verb",
                format!("verb not wired in the runtime yet: {other:?}"),
            ),
        }
    }

    async fn dispatch_invoke(
        &self,
        action: &nika_schema::raw::RawInvokeAction,
        scope: &Scope<'_>,
    ) -> Dispatched {
        let tool = action.tool.value.clone();
        let note = format!("invoke · {tool}");
        let args = match &action.args {
            None => Value::Object(serde_json::Map::new()),
            Some(a) => match expr::render_json(&a.value, scope) {
                Ok(v) => v,
                Err(err) => return Dispatched::template_err(&note, &err),
            },
        };
        let mut input = InvokeInput::new(tool);
        input.args = args;
        match self.invoke.run(input).await {
            // A tool's typed value (builtins · MCP `structuredContent`) flows
            // to tasks.X.output AS ITSELF — an array stays an array so
            // `for_each` / CEL navigation works (spec 04 §tasks.X.output ·
            // "string · object · or bytes · per verb"). A text-only tool
            // (no structured value) stays a String — never silently
            // JSON-coerced from text.
            Ok(out) => {
                // Move the typed value out when present; otherwise wrap the
                // text view — no clone, and `out.content` is only consumed on
                // the None arm (clippy: not a lazy-eval candidate).
                let value = match out.structured {
                    Some(value) => value,
                    None => Value::String(out.content),
                };
                Dispatched::ok(note, value, None)
            }
            Err(err) => Dispatched::verb_err(note, &err),
        }
    }

    async fn dispatch_shell(
        &self,
        action: &nika_schema::raw::RawExecAction,
        scope: &Scope<'_>,
    ) -> Dispatched {
        // Each command form maps to its OWN ExecInput variant — the argv
        // form is rendered PER ELEMENT and passed as a vector (NO join, NO
        // shell), so an interpolated value can never break out of its argv
        // token. The shell form keeps `/bin/sh -c` for genuine pipelines.
        let (mut input, program, is_argv) = match &action.command {
            RawCommand::Shell(text) => match expr::render(&text.value, scope) {
                Ok(line) => {
                    let program = shell_leading_program(&line);
                    (ExecInput::shell(line), program, false)
                }
                Err(err) => return Dispatched::template_err("exec · ?", &err),
            },
            RawCommand::Argv(parts) => {
                let rendered: Result<Vec<_>, _> = parts
                    .iter()
                    .map(|p| expr::render(&p.value, scope))
                    .collect();
                match rendered {
                    Ok(argv) => {
                        let program = argv.first().cloned().unwrap_or_else(|| "?".to_owned());
                        (ExecInput::argv(argv), program, true)
                    }
                    Err(err) => return Dispatched::template_err("exec · ?", &err),
                }
            }
            // #[non_exhaustive] · refuse loudly · never guess a shape.
            other => {
                return Dispatched::unwired(
                    "exec · ?",
                    format!("command form not wired in the runtime yet: {other:?}"),
                );
            }
        };
        // The authored `capture:` mode flows to the verb (spec 02 §exec ·
        // default `stdout`). It selects which streams come back AND the
        // one-obvious-way split: under `structured` a non-zero exit is
        // DATA (the task succeeds · `exit_code` is the branch), under the
        // text modes it fails the task — the verb owns that decision, so
        // it MUST see the mode (omitting this ran every exec in stdout
        // mode · `tasks.X.output.exit_code` was unresolvable).
        input.capture = capture_mode(action.capture.as_ref().map(|c| c.value));
        let note = format!("exec · {program}");

        // cwd · env · stdin flow to the subprocess (spec 02 §exec). All
        // three may carry `${{ }}` and are rendered against the scope; the
        // parser captured them but the dispatch dropped them before this
        // (the subprocess ran in the engine cwd with the inherited env).
        if let Err(err) = render_exec_io(&mut input, action, scope) {
            return Dispatched::template_err(&note, &err);
        }

        if let Some(denial) = check_exec_permits(scope.permits, &note, &program, is_argv) {
            return denial;
        }

        match self.shell.run(input).await {
            Ok(out) => {
                // A text mode (`stdout`/`stderr`/`combined`) yields a
                // trailing-newline-trimmed STRING (the `tasks.X.output ==
                // '42'` ergonomic). `capture: structured` yields the
                // `{ stdout, stderr, exit_code }` OBJECT verbatim — so
                // `tasks.X.output.exit_code` resolves via CEL (spec 02
                // §exec · same class as BUG#3's invoke value). The
                // structured streams are NOT trimmed (fidelity is the
                // whole point of the mode · the verb keeps them raw).
                let value = match out.output {
                    ExecValue::Text(text) => Value::String(text.trim_end().to_owned()),
                    ExecValue::Structured {
                        stdout,
                        stderr,
                        exit_code,
                    } => serde_json::json!({
                        "stdout": stdout,
                        "stderr": stderr,
                        "exit_code": exit_code,
                    }),
                    // #[non_exhaustive] · a future value form fails loudly
                    // rather than dropping fields (the loud doctrine).
                    other => {
                        return Dispatched::unwired(
                            &note,
                            format!("exec value form not wired yet: {other:?}"),
                        );
                    }
                };
                Dispatched::ok(note, value, None)
            }
            Err(err) => Dispatched::verb_err(note, &err),
        }
    }

    async fn dispatch_infer(
        &self,
        action: &nika_schema::raw::RawInferAction,
        scope: &Scope<'_>,
        deadline: Option<std::time::Duration>,
    ) -> Dispatched {
        let prompt = match expr::render(&action.prompt.value, scope) {
            Ok(p) => p,
            Err(err) => return Dispatched::template_err("infer · ?", &err),
        };
        let mut input = InferInput::new(prompt);
        // The task `timeout:` flows to the provider transport deadline —
        // the outer attempt-loop select still enforces the total budget.
        input.timeout = deadline;
        input.system = match render_opt(action.system.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return Dispatched::template_err("infer · ?", &err),
        };
        input.model = action.model.as_ref().map(|m| m.value.clone());
        #[allow(clippy::cast_possible_truncation)] // 0-2 range · checker-validated
        {
            input.temperature = action.temperature.as_ref().map(|t| t.value as f32);
        }
        input.max_tokens = action.max_tokens.as_ref().map(|t| t.value);
        input.schema = action.schema.as_ref().map(|v| v.value.clone());
        match self.infer.run(input).await {
            Ok(out) => {
                let note = format!("infer · {}", out.model_resolved);
                let value = match out.output {
                    InferValue::Text(text) => Value::String(text),
                    // Structured output IS a JSON value (spec 04 typed
                    // dataflow — downstream templates render it
                    // canonically · for_each can fan over arrays).
                    InferValue::Structured(value) => value,
                    // #[non_exhaustive] · a future value form fails loudly.
                    other => {
                        return Dispatched::unwired(
                            &note,
                            format!("infer value form not wired yet: {other:?}"),
                        );
                    }
                };
                let tokens = Some(i64::try_from(out.usage.output_tokens).unwrap_or(i64::MAX));
                // OBS-E: surface the silent empty-answer footgun (a
                // reasoning model that ate its budget on thinking) as a
                // non-fatal warning · the task still succeeds.
                let warning = empty_thinking_warning(&value, &out.usage);
                Dispatched::ok_warned(note, value, tokens, warning)
            }
            Err(err) => Dispatched::verb_err("infer · ?".to_owned(), &err),
        }
    }

    async fn dispatch_agent(
        &self,
        action: &nika_schema::raw::RawAgentAction,
        scope: &Scope<'_>,
        agent_buffer: &crate::agent_events::BufferingObserver,
    ) -> Dispatched {
        let prompt = match expr::render(&action.prompt.value, scope) {
            Ok(p) => p,
            Err(err) => return Dispatched::template_err("agent · ?", &err),
        };
        let mut input = AgentInput::new(prompt);
        input.system = match render_opt(action.system.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return Dispatched::template_err("agent · ?", &err),
        };
        input.model = action.model.as_ref().map(|m| m.value.clone());
        input.tools = action.tools.iter().map(|t| t.value.clone()).collect();
        input.max_turns = action.max_turns.as_ref().map(|t| t.value);
        input.max_tokens_total = action.max_tokens_total.as_ref().map(|t| t.value);
        #[allow(clippy::cast_possible_truncation)] // 0-2 range · checker-validated
        {
            input.temperature = action.temperature.as_ref().map(|t| t.value as f32);
        }
        input.schema = action.schema.as_ref().map(|v| v.value.clone());
        // The buffer is the CALLER's (per task-attempt-loop · still
        // per-dispatch-isolated since a wave's tasks each own one):
        // owning it here would put it inside the timeout-cancellable
        // region and lose a timed-out attempt's telemetry (review F1).
        let ran = self.agent.run_observed(input, agent_buffer).await;
        match ran {
            Ok(out) => {
                let note = format!("agent · {} turns", out.turns);
                let value = match out.output {
                    AgentValue::Text(text) => Value::String(text),
                    AgentValue::Structured(value) => value,
                    // #[non_exhaustive] · a future value form fails loudly.
                    other => {
                        return Dispatched::unwired(
                            &note,
                            format!("agent value form not wired yet: {other:?}"),
                        );
                    }
                };
                let tokens = Some(i64::try_from(out.total_tokens).unwrap_or(i64::MAX));
                Dispatched::ok(note, value, tokens)
            }
            Err(err) => Dispatched::verb_err("agent · ?".to_owned(), &err),
        }
    }
}

/// Render an optional spanned string field.
fn render_opt(
    field: Option<&nika_schema::Spanned<String>>,
    scope: &Scope<'_>,
) -> Result<Option<String>, RuntimeError> {
    field.map(|f| expr::render(&f.value, scope)).transpose()
}

/// Render the exec subprocess I/O (`cwd` · `env` · `stdin`) onto `input`.
///
/// Each field may carry `${{ }}` and is resolved against the scope. `env`
/// keys AND values are both rendered (a value commonly forwards an envelope
/// var · `env: { API_BASE: "${{ env.API_BASE }}" }` per spec 02 §exec).
fn render_exec_io(
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
fn capture_mode(spec: Option<SpecCaptureMode>) -> CaptureMode {
    match spec {
        Some(SpecCaptureMode::Stderr) => CaptureMode::Stderr,
        Some(SpecCaptureMode::Combined) => CaptureMode::Combined,
        Some(SpecCaptureMode::Structured) => CaptureMode::Structured,
        // `stdout`, omitted, OR an unknown future variant → the default.
        None | Some(SpecCaptureMode::Stdout | _) => CaptureMode::Stdout,
    }
}

/// The exec capability boundary (spec 01 §permits · NIKA-SEC-004): once a
/// workflow declares `permits`, the exec sink enforces it. Returns `Some(error)`
/// when the command is refused, `None` when permitted (or no `permits` declared
/// — today's behavior, where the runner floor is the only gate; operator policy
/// is nika-policy's job, s8).
///
/// A program allowlist (`Programs`) governs `argv[0]` of the ARRAY form (the
/// unambiguous program); the SHELL form is REFUSED under an allowlist because a
/// pipeline can launch any program, so a single leading token cannot verify it
/// (use the array form). This is STRICTER than the static `nika check` for
/// shell-under-allowlist — the safe direction.
fn check_exec_permits(
    permits: Option<&nika_schema::types::Permits>,
    note: &str,
    program: &str,
    is_argv: bool,
) -> Option<Dispatched> {
    use nika_schema::types::ExecPermit;
    let permits = permits?;
    match &permits.exec {
        // Omitted or `false` → this workflow runs zero processes.
        None | Some(ExecPermit::No) => Some(Dispatched::security_err(
            note,
            "exec is not permitted by the workflow `permits` boundary",
        )),
        // `true` → any process (still blocklist-gated at the floor).
        Some(ExecPermit::Any) => None,
        // A program allowlist → ARRAY form only (argv[0] must be listed); the
        // SHELL form cannot be verified (a pipeline can launch any program), so
        // it is refused — use the array form.
        Some(ExecPermit::Programs(allowed)) => {
            if !is_argv {
                return Some(Dispatched::security_err(
                    note,
                    "a shell-string command cannot be verified against a \
                     `permits.exec` program allowlist (a pipeline can launch \
                     any program) — use the array form",
                ));
            }
            if !allowed.iter().any(|p| p == program) {
                return Some(Dispatched::security_err(
                    note,
                    format!("program {program:?} is not in the `permits.exec` allowlist"),
                ));
            }
            None
        }
        // #[non_exhaustive] · a future permit form fails CLOSED.
        Some(_) => Some(Dispatched::security_err(
            note,
            "exec permit form not understood by this engine version",
        )),
    }
}

/// OBS-E · the silent-empty-answer footgun for reasoning models.
///
/// A thinking model (gemini-2.5-flash · openai o-series · anthropic
/// extended-thinking) under a tight `max_tokens` can spend the whole
/// budget on its reasoning trace and conclude with a BLANK visible
/// answer — the task completes rc=0 with `output == ""` and every
/// downstream `${{ tasks.X.output }}` silently resolves to nothing.
///
/// The signal is the **empty visible answer paired with reasoning
/// spend**, NOT `output_tokens == 0`: the gemini wire deliberately folds
/// `thoughtsTokenCount` INTO `output_tokens` (so a heavy-think empty
/// answer reports `output_tokens == thoughts > 0`, see
/// `wire::gemini::usage_from`) — keying off `output_tokens` would never
/// fire on the very provider this guards. `reasoning_tokens` (`OpenAI`
/// o-series) is treated the same as `thinking_tokens` (provider-agnostic
/// · the reasoning budget is one concept across wires).
///
/// Non-fatal: returns the diagnostic STRING when the footgun is detected
/// (the caller rides it on `TaskCompleted` as a `warning` field) · the
/// task still succeeds. Pure (no I/O) so it is `--lib`-testable.
fn empty_thinking_warning(
    value: &Value,
    usage: &nika_kernel::provider::TokenUsage,
) -> Option<String> {
    let reasoning = usage
        .thinking_tokens
        .unwrap_or(0)
        .saturating_add(usage.reasoning_tokens.unwrap_or(0));
    if reasoning > 0 && value_is_blank(value) {
        return Some(format!(
            "infer produced an empty answer · max_tokens likely too low for a thinking model \
             (reasoning consumed {reasoning} tokens)"
        ));
    }
    None
}

/// A "blank" visible answer · an empty/whitespace string, JSON null, or
/// an empty container (`""`, `null`, `[]`, `{}`). Anything with content
/// (a non-empty string · a populated object/array · a number/bool) is a
/// real answer, never the footgun.
fn value_is_blank(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
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

    use super::{Scope, render_exec_io};

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
        action.cwd = Some(spanned("${{ vars.dir }}"));
        action.env = vec![
            (spanned("API_BASE"), spanned("${{ vars.base }}")),
            (spanned("STATIC"), spanned("lit")),
        ];
        action.stdin = Some(spanned("${{ vars.payload }}"));

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
    fn absent_exec_io_leaves_the_input_at_its_defaults() {
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        let action = RawExecAction::with_command(RawCommand::Shell(spanned("true")));
        let mut input = ExecInput::shell("true");
        render_exec_io(&mut input, &action, &scope).expect("renders");
        assert!(input.cwd.is_none(), "no cwd → inherited");
        assert!(input.stdin.is_none(), "no stdin");
        assert!(input.env.is_empty(), "no env → inherited only");
    }

    #[test]
    fn a_bad_template_in_exec_io_is_a_loud_error() {
        // An unresolvable reference in cwd/env/stdin must surface, not be
        // swallowed (the loud doctrine · same class as a bad command).
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        let mut action = RawExecAction::with_command(RawCommand::Shell(spanned("true")));
        action.cwd = Some(spanned("${{ vars.nope }}"));
        let mut input = ExecInput::shell("true");
        assert!(render_exec_io(&mut input, &action, &scope).is_err());
    }

    // ── OBS-E · the reasoning-model blank-answer footgun ────────────────

    use nika_kernel::provider::TokenUsage;

    use super::{empty_thinking_warning, value_is_blank};

    fn usage_thinking(output: u64, thinking: u64) -> TokenUsage {
        let mut u = TokenUsage::new(7, output);
        u.thinking_tokens = Some(thinking);
        u
    }

    #[test]
    fn obs_e_fires_on_empty_answer_with_thinking_spend() {
        // The task's footgun: a thinking model that ate its budget on
        // reasoning and concluded with a blank visible answer.
        let usage = usage_thinking(0, 84);
        let warn = empty_thinking_warning(&Value::String(String::new()), &usage)
            .expect("empty answer + thinking → warning");
        assert!(warn.contains("empty answer"), "names the footgun: {warn}");
        assert!(warn.contains("84"), "reports the reasoning spend: {warn}");
        assert!(warn.contains("max_tokens"), "names the likely fix: {warn}");
    }

    #[test]
    fn obs_e_silent_on_a_real_answer_without_thinking() {
        // The normal path · {output:50, thinking:0} with content → no warn.
        let usage = usage_thinking(50, 0);
        assert!(
            empty_thinking_warning(&Value::String("Paris".to_owned()), &usage).is_none(),
            "a real answer is never the footgun"
        );
    }

    #[test]
    fn obs_e_keys_off_the_empty_visible_answer_not_output_tokens() {
        // The gemini wire folds `thoughtsTokenCount` INTO `output_tokens`
        // (usage_from), so a heavy-think empty answer reports
        // output_tokens == thoughts > 0 · keying off output_tokens would
        // MISS it. The blank visible answer is the real signal.
        let usage = usage_thinking(84, 84); // gemini-shaped: candidates(0)+thoughts(84)
        assert!(
            empty_thinking_warning(&Value::String("   ".to_owned()), &usage).is_some(),
            "whitespace-only answer + thinking fires (the gemini reality)"
        );
    }

    #[test]
    fn obs_e_reasoning_tokens_count_like_thinking_tokens() {
        // OpenAI o-series reports the reasoning budget under
        // `reasoning_tokens` · the footgun is provider-agnostic.
        let mut usage = TokenUsage::new(7, 0);
        usage.reasoning_tokens = Some(40);
        assert!(
            empty_thinking_warning(&Value::Null, &usage).is_some(),
            "reasoning_tokens triggers the same diagnostic"
        );
    }

    #[test]
    fn obs_e_silent_when_no_reasoning_even_if_answer_blank() {
        // A blank answer WITHOUT any reasoning spend is a different
        // problem (an empty completion) · OBS-E only owns the thinking
        // footgun, so it stays silent (no false alarm).
        let usage = TokenUsage::new(7, 0); // no thinking, no reasoning
        assert!(empty_thinking_warning(&Value::String(String::new()), &usage).is_none());
        assert!(empty_thinking_warning(&Value::Null, &usage).is_none());
    }

    #[test]
    fn value_is_blank_classifies_empty_forms() {
        assert!(value_is_blank(&Value::Null));
        assert!(value_is_blank(&Value::String(String::new())));
        assert!(value_is_blank(&Value::String("  \n ".to_owned())));
        assert!(value_is_blank(&serde_json::json!([])));
        assert!(value_is_blank(&serde_json::json!({})));
        // Content of any kind is NOT blank.
        assert!(!value_is_blank(&Value::String("x".to_owned())));
        assert!(!value_is_blank(&serde_json::json!({ "k": 1 })));
        assert!(!value_is_blank(&serde_json::json!([1])));
        assert!(!value_is_blank(&Value::Bool(false)));
        assert!(!value_is_blank(&serde_json::json!(0)));
    }
}

/// F1 (field report 2026-07-04) — the task `timeout:` must arrive on the
/// provider HTTP request through the REAL parse → check → run chain: the
/// 30s transport default killed every `timeout: "7m"` local-model task
/// with a 408 at 30s. Asserted at the http seam (a capturing effect
/// under a real `ProviderRegistry` · `ollama` profile · zero network).
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod infer_deadline_tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use bytes::Bytes;
    use nika_kernel::http::{
        HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse,
    };
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_invoke::InvokeVerb;

    use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

    /// Captures every provider request · answers a minimal
    /// openai-compat success so the run settles green.
    #[derive(Default)]
    struct CapturingHttp {
        captured: Mutex<Vec<HttpRequest>>,
    }

    impl CapturingHttp {
        fn captured(&self) -> Vec<HttpRequest> {
            self.captured.lock().expect("test mutex").clone()
        }
    }

    const OPENAI_OK: &str = r#"{"id":"cc","model":"m",
        "choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":1,"completion_tokens":1}}"#;

    impl HttpPostDyn for CapturingHttp {
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.captured
                .lock()
                .expect("test mutex")
                .push(request.clone());
            Ok(HttpResponse::new(
                200,
                BTreeMap::new(),
                Bytes::from_static(OPENAI_OK.as_bytes()),
                request.url,
            ))
        }

        async fn send_streaming(
            &self,
            _request: HttpRequest,
        ) -> Result<HttpStreamResponse, HttpError> {
            Err(HttpError::Unsupported {
                reason: "streaming not exercised here".to_owned(),
            })
        }
    }

    async fn run_and_capture(yaml: &str) -> Vec<HttpRequest> {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
        assert!(report.is_clean(), "fixture passes the ladder");

        let http = Arc::new(CapturingHttp::default());
        let registry = Arc::new(ProviderRegistry::new(
            Arc::clone(&http),
            ProvidersConfig::new(),
        ));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            nika_verb_infer::InferVerb::new(registry, "ollama/llama3.2"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        assert!(outcome.ok, "the canned success settles green");
        http.captured()
    }

    #[tokio::test]
    async fn task_timeout_governs_the_provider_http_deadline() {
        // The exact field repro: `timeout: "7m"` on a local-model infer.
        let captured = run_and_capture(
            "nika: v1\nworkflow: w\nmodel: ollama/llama3.2\ntasks:\n  - id: ask\n    timeout: \"7m\"\n    infer: { prompt: \"hello\" }\n",
        )
        .await;
        assert_eq!(captured.len(), 1, "one provider round-trip");
        assert_eq!(
            captured[0].timeout,
            Some(Duration::from_secs(420)),
            "the task budget rides the provider HTTP request"
        );
    }

    #[tokio::test]
    async fn local_provider_without_task_timeout_gets_the_generous_default() {
        let captured = run_and_capture(
            "nika: v1\nworkflow: w\nmodel: ollama/llama3.2\ntasks:\n  - id: ask\n    infer: { prompt: \"hello\" }\n",
        )
        .await;
        assert_eq!(captured.len(), 1, "one provider round-trip");
        // 300s — nika-providers' LOCAL_DEFAULT_TIMEOUT (pub(crate) there ·
        // the ≥300s F1 acceptance floor pinned at the consumer seam).
        assert_eq!(
            captured[0].timeout,
            Some(Duration::from_secs(300)),
            "a local provider defaults to minutes, never the 30s cloud default"
        );
    }
}
