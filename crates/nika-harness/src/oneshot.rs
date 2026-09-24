// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One-shot seats beyond codex (P4, widened on 2026-09-22 by the operator: « the harness access
//! must work for infer, agent and compile through our Claude Code, Codex, Kimi, Grok Build… »).
//!
//! Claude Code, Grok Build and GitHub Copilot CLI each answer ONE prompt with their tools
//! disabled at the argv and report the responding model — measured on this machine that day,
//! scripted in the tests below. Each adapter proves the four infer-grade conjuncts its own
//! way (`Cli::attestation`); nothing of the request reaches a child before its `--version`
//! identity passes, exactly like `codex exec`. The numbers a CLI prints beside its answer
//! (`total_cost_usd`, credits) are the CLI's own estimate on a subscription: they never leave
//! this module — only the booleans (`usage_observed`) and the responder's name do.
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::infer::{
    HarnessInferOutcome, HarnessInferRequest, InferGradeAttestation, InferGradeError,
    StructuredOutputGrade,
};
use crate::probe::VersionPin;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PROBE_BYTES: usize = 64 * 1024;

/// Which one-shot CLI a seat runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cli {
    /// `claude -p` (2.1.280 measured): `--tools ""` disables every tool, `--json-schema`
    /// returns `structured_output`, `modelUsage` names the responder.
    Claude,
    /// `grok -p` (1.0.40 measured): `--tools ""`, `--json-schema` → `structuredOutput`,
    /// `modelUsage` names the responder, `num_turns` counts.
    Grok,
    /// `copilot -p` (1.0.77 measured): `--available-tools` with no name leaves the model no
    /// tool, `assistant.message.data.model` names the responder; no schema flag (text grade).
    Copilot,
}

impl Cli {
    /// The `--access` seat this CLI serves.
    pub(crate) const fn seat(self) -> &'static str {
        match self {
            Self::Claude => "claude-code",
            Self::Grok => "grok-build",
            Self::Copilot => "copilot",
        }
    }

    const fn command(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Grok => "grok",
            Self::Copilot => "copilot",
        }
    }

    /// The product token the CLI's `--version` line must carry beside its version.
    const fn product(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Grok => "grok",
            Self::Copilot => "Copilot CLI",
        }
    }

    /// The accepted version range, measured 2026-09-22: `2.1.280 (Claude Code)` · `grok 1.0.40
    /// (eb1a2256660d) [stable]` · `GitHub Copilot CLI 1.0.77.`.
    const fn pin(self) -> VersionPin {
        match self {
            Self::Claude => VersionPin::new((2, 1), 2),
            Self::Grok | Self::Copilot => VersionPin::new((1, 0), 1),
        }
    }

    /// The `provider/` prefixes whose model name is forwarded to the CLI (`default` is never
    /// forwarded: the harness's own choice stands).
    const fn model_prefixes(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["anthropic", "claude-code"],
            Self::Grok => &["xai", "grok-build"],
            Self::Copilot => &["copilot", "github"],
        }
    }

    /// The extra parent variables the CLI may read (a relocatable config home; never a
    /// credential-shaped name — `compose_env` refuses those).
    const fn passthrough(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["CLAUDE_CONFIG_DIR"],
            Self::Grok | Self::Copilot => &[],
        }
    }

    /// The four conjuncts, as each CLI proves them.
    pub(crate) const fn attestation(self) -> InferGradeAttestation {
        match self {
            Self::Claude => InferGradeAttestation {
                single_turn: true,
                no_implicit_tools: true,
                structured_output: StructuredOutputGrade::JsonSchema,
                model_identity_observable: true,
                proof: "scripted fake claude · `-p --max-turns 1` · `--tools \"\"` and an empty permission_denials · `--json-schema` structured_output · usage object · modelUsage names the responder · spawn-time --version identity",
            },
            Self::Grok => InferGradeAttestation {
                single_turn: true,
                no_implicit_tools: true,
                structured_output: StructuredOutputGrade::JsonSchema,
                model_identity_observable: true,
                proof: "scripted fake grok · `-p --max-turns 1` and num_turns == 1 · `--tools \"\"` · `--json-schema` structuredOutput · usage object · modelUsage names the responder · spawn-time --version identity",
            },
            Self::Copilot => InferGradeAttestation {
                single_turn: true,
                no_implicit_tools: true,
                structured_output: StructuredOutputGrade::Text,
                model_identity_observable: true,
                proof: "scripted fake copilot · one assistant.message with no toolRequests and no tool event · `--available-tools` with no name · the terminal result's usage · assistant.message.data.model names the responder · spawn-time --version identity",
            },
        }
    }
}

/// One admitted one-shot CLI.
#[derive(Debug, Clone)]
pub(crate) struct OneShotExec {
    cli: Cli,
    command: PathBuf,
}

impl OneShotExec {
    pub(crate) fn new(cli: Cli) -> Self {
        Self {
            cli,
            command: PathBuf::from(cli.command()),
        }
    }

    /// Against an explicit binary path for the hermetic proof.
    #[cfg(test)]
    pub(crate) fn with_command(cli: Cli, command: impl Into<PathBuf>) -> Self {
        Self {
            cli,
            command: command.into(),
        }
    }

    pub(crate) const fn cli(&self) -> Cli {
        self.cli
    }

    fn env(&self) -> BTreeMap<String, String> {
        #[allow(clippy::disallowed_methods)] // process boundary; filtered by compose_env
        let parent: BTreeMap<String, String> = std::env::vars().collect();
        let passthrough: Vec<String> = self
            .cli
            .passthrough()
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        crate::compose_env(&parent, &passthrough)
    }

    /// Re-attest the binary at spawn: `<command> --version` under the composed env, a null
    /// stdin, a bounded answer and a deadline, judged by version pin AND product token.
    async fn probe_identity(&self) -> Result<(u32, u32), InferGradeError> {
        let child = tokio::process::Command::new(&self.command)
            .arg("--version")
            .env_clear()
            .envs(self.env())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                self.execution(format!(
                    "cannot spawn `{} --version`: {e}",
                    self.command.display()
                ))
            })?;
        let output = tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| {
                refused(format!(
                    "spawn-time attestation of `{}`: `--version` did not answer in {}s",
                    self.command.display(),
                    PROBE_TIMEOUT.as_secs()
                ))
            })?
            .map_err(|e| self.execution(format!("version probe wait: {e}")))?;
        if output.stdout.len() > MAX_PROBE_BYTES || output.stderr.len() > MAX_PROBE_BYTES {
            return Err(refused(format!(
                "spawn-time attestation of `{}`: `--version` printed more than {MAX_PROBE_BYTES} bytes",
                self.command.display()
            )));
        }
        let answer = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        judge_identity(self.cli, &self.command, &answer)
    }

    /// The argv of one call, and what rides stdin (Claude reads its prompt from stdin, the
    /// others take it as the `-p` value).
    fn argv(&self, request: &HarnessInferRequest) -> (Vec<OsString>, Option<String>) {
        let mut args: Vec<OsString> = Vec::new();
        let model = model_arg(self.cli, &request.requested_model);
        let schema = request
            .schema
            .as_ref()
            .and_then(|s| serde_json::to_string(s).ok());
        match self.cli {
            Cli::Claude => {
                for a in [
                    "-p",
                    "--output-format",
                    "json",
                    "--tools",
                    "",
                    "--max-turns",
                    "1",
                    "--no-session-persistence",
                ] {
                    args.push(a.into());
                }
                if let Some(system) = &request.system {
                    args.push("--system-prompt".into());
                    args.push(system.into());
                }
                if let Some(model) = model {
                    args.push("--model".into());
                    args.push(model.into());
                }
                if let Some(schema) = schema {
                    args.push("--json-schema".into());
                    args.push(schema.into());
                }
                (args, Some(format!("{}\n", request.prompt)))
            }
            Cli::Grok => {
                for a in [
                    "-p",
                    request.prompt.as_str(),
                    "--output-format",
                    "json",
                    "--max-turns",
                    "1",
                    "--tools",
                    "",
                ] {
                    args.push(a.into());
                }
                if let Some(system) = &request.system {
                    args.push("--system-prompt-override".into());
                    args.push(system.into());
                }
                if let Some(model) = model {
                    args.push("-m".into());
                    args.push(model.into());
                }
                if let Some(schema) = schema {
                    args.push("--json-schema".into());
                    args.push(schema.into());
                }
                (args, None)
            }
            Cli::Copilot => {
                let prompt = match &request.system {
                    Some(system) => format!(
                        "<nika-system>\n{system}\n</nika-system>\n<nika-user>\n{}\n</nika-user>",
                        request.prompt
                    ),
                    None => request.prompt.clone(),
                };
                for a in [
                    "-p",
                    prompt.as_str(),
                    "-s",
                    "--output-format",
                    "json",
                    "--available-tools",
                    "--no-ask-user",
                ] {
                    args.push(a.into());
                }
                if let Some(model) = model {
                    args.push("--model".into());
                    args.push(model.into());
                }
                (args, None)
            }
        }
    }

    /// The one call: identity first, then the child under the composed env, bounded output,
    /// the deadline, the CLI's own answer read by its measured grammar.
    pub(crate) async fn run(
        &self,
        request: HarnessInferRequest,
    ) -> Result<HarnessInferOutcome, InferGradeError> {
        let attested_version = self.probe_identity().await?;
        // No project instructions or relative files are ambient authoring context.
        let scratch =
            tempfile::tempdir().map_err(|e| self.execution(format!("scratch dir: {e}")))?;
        let (args, stdin_text) = self.argv(&request);
        let mut command = tokio::process::Command::new(&self.command);
        command
            .args(&args)
            .current_dir(scratch.path())
            .env_clear()
            .envs(self.env())
            .stdin(if stdin_text.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|e| {
            self.execution(format!("cannot spawn `{}`: {e}", self.command.display()))
        })?;
        if let Some(text) = stdin_text {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| self.execution("spawned without stdin".to_owned()))?;
            stdin
                .write_all(text.as_bytes())
                .await
                .map_err(|e| self.execution(format!("stdin: {e}")))?;
            stdin
                .shutdown()
                .await
                .map_err(|e| self.execution(format!("stdin close: {e}")))?;
            drop(stdin);
        }
        let timeout = request.timeout.unwrap_or(DEFAULT_TIMEOUT);
        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| self.execution(format!("timed out after {} ms", timeout.as_millis())))?
            .map_err(|e| self.execution(format!("wait: {e}")))?;
        if output.stdout.len() > MAX_OUTPUT_BYTES || output.stderr.len() > MAX_OUTPUT_BYTES {
            return Err(self.execution("answer exceeded 8 MiB".to_owned()));
        }
        let with_schema = request.schema.is_some();
        let read = match self.cli {
            Cli::Claude => read_claude(&output.stdout, with_schema),
            Cli::Grok => read_grok(&output.stdout, with_schema),
            Cli::Copilot => read_copilot(&output.stdout),
        };
        if !output.status.success() {
            // The CLI's own words are the witness: stderr when it spoke there, else what its
            // stdout answer said (a `claude -p` error rides `result` on a zero exit too).
            let stderr = String::from_utf8_lossy(&output.stderr);
            let witness = match stderr.trim() {
                "" => read.err().map_or_else(String::new, |e| e.to_string()),
                said => said.to_owned(),
            };
            return Err(self.execution(format!(
                "{} exited {} · {witness}",
                self.cli.command(),
                output.status
            )));
        }
        let read = read?;
        Ok(HarnessInferOutcome {
            output: read.text,
            requested_model: request.requested_model,
            usage_observed: read.usage_observed,
            attested_version,
            observed_model: read.observed_model,
        })
    }

    fn execution(&self, detail: String) -> InferGradeError {
        InferGradeError::Execution {
            seat: self.cli.seat(),
            detail,
        }
    }
}

fn refused(witness: String) -> InferGradeError {
    InferGradeError::Refused { witness }
}

/// The model name forwarded to the CLI, when the request names this seat's provider.
fn model_arg(cli: Cli, requested: &str) -> Option<&str> {
    let (provider, model) = requested.split_once('/')?;
    (cli.model_prefixes().contains(&provider) && !model.is_empty() && model != "default")
        .then_some(model)
}

/// The two-sided spawn-time judgement: a version inside the pin AND the product token.
fn judge_identity(cli: Cli, command: &Path, answer: &str) -> Result<(u32, u32), InferGradeError> {
    let seen = crate::probe::judge_version(cli.seat(), answer, &cli.pin()).map_err(|e| {
        refused(format!(
            "spawn-time attestation of `{}`: {e}",
            command.display()
        ))
    })?;
    if !answer.lines().any(|line| line.contains(cli.product())) {
        return Err(refused(format!(
            "spawn-time attestation of `{}`: `--version` answered {}.{} without naming `{}` \
             ({:?}) — this binary is not the seat it claims to be",
            command.display(),
            seen.0,
            seen.1,
            cli.product(),
            answer.lines().next().unwrap_or("").trim()
        )));
    }
    Ok(seen)
}

/// What a one-shot answer proved: the text, whether a usage object rode beside it, the
/// responder the CLI named.
struct Read {
    text: String,
    usage_observed: bool,
    observed_model: Option<String>,
}

fn one_json_object(bytes: &[u8], seat: &'static str) -> Result<Value, InferGradeError> {
    let text = std::str::from_utf8(bytes).map_err(|e| InferGradeError::Execution {
        seat,
        detail: format!("stdout is not utf-8: {e}"),
    })?;
    serde_json::from_str::<Value>(text.trim()).map_err(|e| InferGradeError::Execution {
        seat,
        detail: format!("stdout is not one JSON object: {e}"),
    })
}

fn tokens_observed(usage: Option<&Value>, input: &str, output: &str) -> bool {
    usage.is_some_and(|u| {
        u.get(input).and_then(Value::as_u64).is_some()
            && u.get(output).and_then(Value::as_u64).is_some()
    })
}

/// The one responder `modelUsage` names, when it names exactly one.
fn single_model(usage: Option<&Value>) -> Option<String> {
    let map = usage?.as_object()?;
    (map.len() == 1)
        .then(|| map.keys().next().cloned())
        .flatten()
}

/// `claude -p --output-format json`: one `result` object (measured 2.1.280).
fn read_claude(bytes: &[u8], with_schema: bool) -> Result<Read, InferGradeError> {
    let seat = Cli::Claude.seat();
    let v = one_json_object(bytes, seat)?;
    if v.get("type").and_then(Value::as_str) != Some("result") {
        return Err(InferGradeError::Execution {
            seat,
            detail: "the answer is not a `result` object".to_owned(),
        });
    }
    let result = v.get("result").and_then(Value::as_str).unwrap_or("");
    if v.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Err(InferGradeError::Execution {
            seat,
            detail: format!("claude answered an error: {result}"),
        });
    }
    if let Some(denials) = v.get("permission_denials").and_then(Value::as_array)
        && !denials.is_empty()
    {
        return Err(refused(format!(
            "claude-code infer-grade run asked for {} tool permission(s) with every tool disabled",
            denials.len()
        )));
    }
    let text = if with_schema {
        let structured = v
            .get("structured_output")
            .ok_or_else(|| InferGradeError::Execution {
                seat,
                detail: "the schema was requested but the answer carries no structured_output"
                    .to_owned(),
            })?;
        serde_json::to_string(structured).map_err(|e| InferGradeError::Execution {
            seat,
            detail: format!("structured_output serialization: {e}"),
        })?
    } else {
        result.to_owned()
    };
    Ok(Read {
        text,
        usage_observed: tokens_observed(v.get("usage"), "input_tokens", "output_tokens"),
        observed_model: single_model(v.get("modelUsage")),
    })
}

/// `grok -p --output-format json`: one object with `text` / `structuredOutput`, `num_turns`,
/// `stopReason`, `usage`, `modelUsage` (measured 1.0.40).
fn read_grok(bytes: &[u8], with_schema: bool) -> Result<Read, InferGradeError> {
    let seat = Cli::Grok.seat();
    let v = one_json_object(bytes, seat)?;
    let turns = v.get("num_turns").and_then(Value::as_u64).unwrap_or(0);
    if turns != 1 {
        return Err(refused(format!(
            "grok-build infer-grade run violated single_turn: num_turns={turns}"
        )));
    }
    match v.get("stopReason").and_then(Value::as_str) {
        Some("end_turn") | None => {}
        Some(other) => {
            return Err(refused(format!(
                "grok-build infer-grade run stopped on `{other}`, not end_turn"
            )));
        }
    }
    let text = if with_schema {
        let structured = v
            .get("structuredOutput")
            .ok_or_else(|| InferGradeError::Execution {
                seat,
                detail: "the schema was requested but the answer carries no structuredOutput"
                    .to_owned(),
            })?;
        serde_json::to_string(structured).map_err(|e| InferGradeError::Execution {
            seat,
            detail: format!("structuredOutput serialization: {e}"),
        })?
    } else {
        v.get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| InferGradeError::Execution {
                seat,
                detail: "the answer carries no text".to_owned(),
            })?
            .to_owned()
    };
    Ok(Read {
        text,
        usage_observed: tokens_observed(v.get("usage"), "input_tokens", "output_tokens"),
        observed_model: single_model(v.get("modelUsage")),
    })
}

/// `copilot -p --output-format json`: JSONL events (measured 1.0.77) — one `assistant.message`
/// with an empty `toolRequests`, no `tool.*` event, a terminal `result` with its usage.
fn read_copilot(bytes: &[u8]) -> Result<Read, InferGradeError> {
    let seat = Cli::Copilot.seat();
    let text = std::str::from_utf8(bytes).map_err(|e| InferGradeError::Execution {
        seat,
        detail: format!("stdout is not utf-8: {e}"),
    })?;
    let mut message: Option<String> = None;
    let mut model = None;
    let mut usage_observed = false;
    let mut exit_code = None;
    for (index, line) in text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let event: Value = serde_json::from_str(line).map_err(|e| InferGradeError::Execution {
            seat,
            detail: format!("JSONL line {}: {e}", index + 1),
        })?;
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        let data = event.get("data");
        if kind.starts_with("tool") || kind.contains("tool_request") {
            return Err(refused(format!(
                "copilot infer-grade run emitted a tool event `{kind}` with no tool available"
            )));
        }
        match kind {
            "assistant.message" => {
                let asks = data
                    .and_then(|d| d.get("toolRequests"))
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                if asks != 0 {
                    return Err(refused(format!(
                        "copilot infer-grade run requested {asks} tool(s) with no tool available"
                    )));
                }
                message = data
                    .and_then(|d| d.get("content"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                model = data
                    .and_then(|d| d.get("model"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            "result" => {
                exit_code = event.get("exitCode").and_then(Value::as_i64);
                usage_observed = event.get("usage").is_some_and(Value::is_object);
            }
            _ => {}
        }
    }
    if exit_code.is_some_and(|c| c != 0) {
        return Err(InferGradeError::Execution {
            seat,
            detail: format!(
                "copilot's result reports exit code {}",
                exit_code.unwrap_or(0)
            ),
        });
    }
    let text = message.ok_or_else(|| InferGradeError::Execution {
        seat,
        detail: "no assistant.message in the answer".to_owned(),
    })?;
    Ok(Read {
        text,
        usage_observed,
        observed_model: model,
    })
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A scripted CLI: the measured `--version` line, then the body for a call.
    fn scripted(name: &str, version_line: &str, body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let bin = dir.path().join(name);
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\nset -eu\nif [ \"${{1:-}}\" = --version ]; then printf '%s\\n' '{version_line}'; exit 0; fi\n{body}\n"
            ),
        )
        .expect("script");
        let mut permissions = std::fs::metadata(&bin).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&bin, permissions).expect("chmod");
        (dir, bin)
    }

    fn schema_request(model: &str) -> HarnessInferRequest {
        let mut request = HarnessInferRequest::new("classify this", model);
        request.schema = Some(serde_json::json!({
            "type": "object",
            "required": ["label"],
            "properties": {"label": {"type": "string"}},
        }));
        request
    }

    // The measured claude answer (2.1.280, 2026-09-22), abridged to the read fields.
    const CLAUDE_OK: &str = r#"cat >/dev/null; printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"{\"label\":\"ok\"}","structured_output":{"label":"ok"},"num_turns":2,"permission_denials":[],"usage":{"input_tokens":2,"output_tokens":53},"modelUsage":{"claude-fable-5-1":{"inputTokens":2,"outputTokens":53}},"total_cost_usd":0.84}'"#;

    #[tokio::test]
    async fn one_shot_inference_uses_a_disposable_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let capture = dir.path().join("cwd");
        let quote = capture.display().to_string().replace('\'', "'\\''");
        let body = format!("pwd > '{quote}'\n{CLAUDE_OK}");
        let (_script, bin) = scripted("claude", "2.1.280 (Claude Code)", &body);
        OneShotExec::with_command(Cli::Claude, &bin)
            .run(schema_request("claude-code/default"))
            .await
            .expect("fixture answer");
        let cwd = std::fs::read_to_string(capture).unwrap();
        let scratch = PathBuf::from(cwd.trim());
        assert_ne!(scratch, std::env::current_dir().unwrap());
        assert!(!scratch.exists(), "scratch removed after the call");
    }

    #[tokio::test]
    async fn claude_answers_the_schema_and_names_the_responder() {
        let (_dir, bin) = scripted("claude", "2.1.280 (Claude Code)", CLAUDE_OK);
        let seat = OneShotExec::with_command(Cli::Claude, &bin);
        let out = seat
            .run(schema_request("claude-code/default"))
            .await
            .expect("the measured answer reads");
        assert_eq!(out.output, r#"{"label":"ok"}"#);
        assert!(out.usage_observed);
        assert_eq!(out.observed_model.as_deref(), Some("claude-fable-5-1"));
        assert_eq!(out.attested_version, (2, 1));
    }

    #[tokio::test]
    async fn claude_refuses_a_permission_ask_and_an_error_answer() {
        let denial = r#"cat >/dev/null; printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"x","permission_denials":[{"tool_name":"Bash"}],"usage":{"input_tokens":1,"output_tokens":1},"modelUsage":{"m":{}}}'"#;
        let (_dir, bin) = scripted("claude", "2.1.280 (Claude Code)", denial);
        let err = OneShotExec::with_command(Cli::Claude, &bin)
            .run(HarnessInferRequest::new("q", "claude-code/default"))
            .await
            .expect_err("a tool ask refuses");
        assert!(err.to_string().contains("permission"), "{err}");
        let error = r#"cat >/dev/null; printf '%s\n' '{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login","usage":{},"modelUsage":{}}'"#;
        let (_dir, bin) = scripted("claude", "2.1.280 (Claude Code)", error);
        let err = OneShotExec::with_command(Cli::Claude, &bin)
            .run(HarnessInferRequest::new("q", "claude-code/default"))
            .await
            .expect_err("the CLI's own error is the witness");
        assert!(err.to_string().contains("Not logged in"), "{err}");
    }

    #[tokio::test]
    async fn a_claude_below_the_pin_or_without_its_product_name_never_runs() {
        let (_dir, bin) = scripted("claude", "1.0.9 (Claude Code)", CLAUDE_OK);
        let err = OneShotExec::with_command(Cli::Claude, &bin)
            .run(HarnessInferRequest::new("q", "claude-code/default"))
            .await
            .expect_err("below the floor");
        assert!(err.to_string().contains("1.0"), "{err}");
        let (_dir, bin) = scripted("claude", "2.1.280", CLAUDE_OK);
        let err = OneShotExec::with_command(Cli::Claude, &bin)
            .run(HarnessInferRequest::new("q", "claude-code/default"))
            .await
            .expect_err("no product name");
        assert!(err.to_string().contains("Claude Code"), "{err}");
    }

    // The measured grok answer (1.0.40, 2026-09-22), abridged.
    const GROK_OK: &str = r#"printf '%s\n' '{"text":"{\"label\":\"ok\"}","structuredOutput":{"label":"ok"},"stopReason":"end_turn","num_turns":1,"usage":{"input_tokens":21789,"output_tokens":117},"modelUsage":{"grok-4.7-build":{"inputTokens":21789,"outputTokens":117}},"total_cost_usd":0.017}'"#;

    #[tokio::test]
    async fn grok_answers_the_schema_and_a_second_turn_refuses() {
        let (_dir, bin) = scripted("grok", "grok 1.0.40 (eb1a2256660d) [stable]", GROK_OK);
        let out = OneShotExec::with_command(Cli::Grok, &bin)
            .run(schema_request("xai/grok-4.7"))
            .await
            .expect("the measured answer reads");
        assert_eq!(out.output, r#"{"label":"ok"}"#);
        assert_eq!(out.observed_model.as_deref(), Some("grok-4.7-build"));
        assert!(out.usage_observed);
        let two = GROK_OK.replace(r#""num_turns":1"#, r#""num_turns":2"#);
        let (_dir, bin) = scripted("grok", "grok 1.0.40 (eb1a2256660d) [stable]", &two);
        let err = OneShotExec::with_command(Cli::Grok, &bin)
            .run(HarnessInferRequest::new("q", "xai/grok-4.7"))
            .await
            .expect_err("two turns refuse");
        assert!(err.to_string().contains("single_turn"), "{err}");
    }

    // The measured copilot JSONL (1.0.77, 2026-09-22), abridged to the read events.
    const COPILOT_OK: &str = r#"printf '%s\n' '{"type":"session.tools_updated","data":{"model":"mai-code-1.1-flash"}}' '{"type":"assistant.message","data":{"content":"OK","model":"mai-code-1.1-flash","toolRequests":[]}}' '{"type":"result","exitCode":0,"usage":{"premiumRequests":1}}'"#;

    #[tokio::test]
    async fn copilot_answers_text_and_a_tool_request_refuses() {
        let (_dir, bin) = scripted("copilot", "GitHub Copilot CLI 1.0.77.", COPILOT_OK);
        let out = OneShotExec::with_command(Cli::Copilot, &bin)
            .run(HarnessInferRequest::new("q", "copilot/default"))
            .await
            .expect("the measured answer reads");
        assert_eq!(out.output, "OK");
        assert_eq!(out.observed_model.as_deref(), Some("mai-code-1.1-flash"));
        assert!(out.usage_observed);
        let asked = COPILOT_OK.replace(
            r#""toolRequests":[]"#,
            r#""toolRequests":[{"name":"bash"}]"#,
        );
        let (_dir, bin) = scripted("copilot", "GitHub Copilot CLI 1.0.77.", &asked);
        let err = OneShotExec::with_command(Cli::Copilot, &bin)
            .run(HarnessInferRequest::new("q", "copilot/default"))
            .await
            .expect_err("a tool request refuses");
        assert!(err.to_string().contains("tool"), "{err}");
    }

    #[test]
    fn the_model_rides_only_for_the_seats_own_provider_and_never_as_default() {
        assert_eq!(
            model_arg(Cli::Claude, "anthropic/claude-sonnet-4-5"),
            Some("claude-sonnet-4-5")
        );
        assert_eq!(model_arg(Cli::Claude, "claude-code/sonnet"), Some("sonnet"));
        assert_eq!(model_arg(Cli::Claude, "claude-code/default"), None);
        assert_eq!(model_arg(Cli::Claude, "openai/gpt-5.5"), None);
        assert_eq!(model_arg(Cli::Grok, "xai/grok-4.7"), Some("grok-4.7"));
        assert_eq!(model_arg(Cli::Copilot, "github/gpt-5.4"), Some("gpt-5.4"));
        assert_eq!(model_arg(Cli::Copilot, "mock/echo"), None);
    }

    #[test]
    fn every_seat_argv_disables_its_tools_and_bounds_its_turns() {
        let request = schema_request("claude-code/sonnet");
        let (args, stdin) = OneShotExec::new(Cli::Claude).argv(&request);
        let joined: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            joined
                .windows(2)
                .any(|w| w[0] == "--tools" && w[1].is_empty()),
            "{joined:?}"
        );
        assert!(
            joined
                .windows(2)
                .any(|w| w[0] == "--max-turns" && w[1] == "1")
        );
        assert!(
            joined
                .windows(2)
                .any(|w| w[0] == "--model" && w[1] == "sonnet")
        );
        assert!(joined.iter().any(|a| a == "--json-schema"));
        assert!(stdin.is_some_and(|s| s.starts_with("classify this")));
        let (args, stdin) = OneShotExec::new(Cli::Grok).argv(&schema_request("xai/grok-4.7"));
        let joined: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            joined
                .windows(2)
                .any(|w| w[0] == "--tools" && w[1].is_empty())
        );
        assert!(
            joined
                .windows(2)
                .any(|w| w[0] == "-p" && w[1] == "classify this")
        );
        assert!(stdin.is_none());
        let (args, _) =
            OneShotExec::new(Cli::Copilot).argv(&HarnessInferRequest::new("q", "copilot/default"));
        let joined: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(joined.iter().any(|a| a == "--available-tools"));
        assert!(joined.iter().any(|a| a == "--no-ask-user"));
    }
}
