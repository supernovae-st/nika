// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Infer-grade harness access (P4) — deliberately separate from ACP.
//!
//! An ACP speaker proves an agentic loop, not a one-shot inference
//! contract. This module has one admitted adapter: `codex exec --json`,
//! with `--output-schema` when the task needs JSON Schema. The accepted
//! path is one turn, rejects every implicit tool item, and records only
//! the requested model identity. Numeric usage is parsed from the terminal
//! `turn.completed` event as protocol evidence and never leaves this
//! module.
//!
//! Admission ([`meet_infer_grade`]) reads the static row; execution
//! re-attests the binary at spawn (#1253): `codex --version` must name the
//! product and sit inside `CODEX_VERSION_PIN` before any prompt is written
//! to its stdin. PATH presence admits a seat, it never proves one — a shim
//! that only speaks the event shape is refused here, not believed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use std::io::Write as _;

use serde_json::Value;
use tokio::io::AsyncWriteExt;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;

/// The accepted `codex --version` range. 0.149 is the release the
/// `exec --json` flag shape was proven against (2026-08-25 · the registry's
/// current release that day); an older CLI lacks `--ephemeral`,
/// `--ignore-user-config` and `--ignore-rules`. A new major is a new dialect.
const CODEX_VERSION_PIN: crate::probe::VersionPin = crate::probe::VersionPin::new((0, 149), 0);
/// The product token the real CLI prints (`codex-cli 0.153.4`). A shim
/// answering a bare number is not the seat it claims to be.
const CODEX_PRODUCT: &str = "codex-cli";
/// A `--version` is a millisecond operation; a probe that needs longer is a
/// wrapper resolving something, not the seat answering.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Combined stdout + stderr a version probe may print.
const MAX_PROBE_BYTES: usize = 64 * 1024;

/// Structured-output strength an `infer:` task needs or a seat proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum StructuredOutputGrade {
    /// Free-form text.
    Text,
    /// A JSON value without a supplied schema.
    Json,
    /// The supplied JSON Schema is enforced by the harness.
    JsonSchema,
}

impl StructuredOutputGrade {
    /// Stable teaching string used in refusal witnesses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::JsonSchema => "json_schema",
        }
    }
}

/// Evidence recorded for an infer-grade adapter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct InferGradeAttestation {
    /// Exactly one turn is accepted.
    pub single_turn: bool,
    /// Tool-bearing event items fail the call.
    pub no_implicit_tools: bool,
    /// Highest structured-output level proved by the adapter.
    pub structured_output: StructuredOutputGrade,
    /// The requested model identity is observable at command construction.
    pub model_identity_observable: bool,
    /// The hermetic proof that backs this row.
    pub proof: &'static str,
}

impl InferGradeAttestation {
    const fn codex_exec() -> Self {
        Self {
            single_turn: true,
            no_implicit_tools: true,
            structured_output: StructuredOutputGrade::JsonSchema,
            model_identity_observable: true,
            proof: "scripted fake codex · one turn · tool-event refusal · schema argv · terminal usage · spawn-time --version identity",
        }
    }

    fn failed(self, need: StructuredOutputGrade) -> Vec<&'static str> {
        let mut failed = Vec::new();
        if !self.single_turn {
            failed.push("single_turn");
        }
        if !self.no_implicit_tools {
            failed.push("no_implicit_tools");
        }
        if self.structured_output < need {
            failed.push("structured_output");
        }
        if !self.model_identity_observable {
            failed.push("model_identity");
        }
        failed
    }
}

/// An infer-grade meet refusal.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum InferGradeError {
    /// The row has no proof or misses a required conjunct.
    #[error("{witness}")]
    Refused {
        /// Complete teaching witness.
        witness: String,
    },
    /// The one-shot child could not be executed.
    #[error("infer-grade seat `{seat}` failed: {detail}")]
    Execution {
        /// Seat id.
        seat: &'static str,
        /// Spawn, timeout, exit, or JSONL detail.
        detail: String,
    },
}

impl InferGradeError {
    /// The refusal/execution witness.
    #[must_use]
    pub fn witness(&self) -> &str {
        match self {
            Self::Refused { witness } => witness,
            Self::Execution { detail, .. } => detail,
        }
    }
}

impl nika_error::traits::NikaErrorCode for InferGradeError {
    fn nika_code(&self) -> nika_error::codes::NikaCode {
        match self {
            Self::Refused { .. } => nika_error::codes::NIKA_1800,
            Self::Execution { .. } => nika_error::codes::NIKA_430,
        }
    }
}

/// Run input for a seated one-shot.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HarnessInferRequest {
    /// User prompt.
    pub prompt: String,
    /// Optional system instruction, wrapped into the single prompt.
    pub system: Option<String>,
    /// Author-requested `provider/model`, recorded but never presented as
    /// the responding model.
    pub requested_model: String,
    /// Optional JSON Schema.
    pub schema: Option<Value>,
    /// Task deadline.
    pub timeout: Option<Duration>,
}

impl HarnessInferRequest {
    /// Construct a plain-text request.
    #[must_use]
    pub fn new(prompt: impl Into<String>, requested_model: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            system: None,
            requested_model: requested_model.into(),
            schema: None,
            timeout: None,
        }
    }

    /// Add the optional system instruction.
    #[must_use]
    pub fn with_system(mut self, system: Option<String>) -> Self {
        self.system = system;
        self
    }

    /// Add the structured-output contract.
    #[must_use]
    pub fn with_schema(mut self, schema: Option<Value>) -> Self {
        self.schema = schema;
        self
    }

    /// Add the task deadline.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Successful one-shot result. There are intentionally no token or price
/// fields: subscription quota is not a fabricated numeric meter.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HarnessInferOutcome {
    /// Final assistant text (JSON text when a schema was supplied).
    pub output: String,
    /// Requested identity copied from the request.
    pub requested_model: String,
    /// A terminal usage object was parsed. Its numbers stay private.
    pub usage_observed: bool,
    /// The `MAJOR.MINOR` the seat's own `--version` answered at spawn — the
    /// runtime attestation the receipt can carry, never the static row.
    pub attested_version: (u32, u32),
}

/// The only admitted P4 adapter.
#[derive(Debug, Clone)]
struct CodexExec {
    command: PathBuf,
}

impl Default for CodexExec {
    fn default() -> Self {
        Self {
            command: PathBuf::from("codex"),
        }
    }
}

impl CodexExec {
    /// Construct the production adapter.
    #[must_use]
    fn new() -> Self {
        Self::default()
    }

    /// Construct against an explicit binary path for the hermetic proof; the
    /// script itself is still named `codex` in that fixture.
    #[cfg(test)]
    #[must_use]
    fn with_command(command: impl Into<PathBuf>) -> Self {
        Self {
            command: command.into(),
        }
    }

    fn attestation(&self) -> InferGradeAttestation {
        let _ = self;
        InferGradeAttestation::codex_exec()
    }

    /// Re-attest the binary at spawn (#1253). `<command> --version` runs with
    /// the same composed env as the one-shot, a null stdin, a bounded answer
    /// and a deadline; the answer is judged by [`judge_identity`]. Nothing of
    /// the request reaches the child before this passes.
    async fn probe_identity(&self) -> Result<(u32, u32), InferGradeError> {
        let child = tokio::process::Command::new(&self.command)
            .arg("--version")
            .env_clear()
            .envs(codex_env())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                execution(format!(
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
            .map_err(|e| execution(format!("version probe wait: {e}")))?;
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
        judge_identity(&self.command, &answer)
    }

    async fn run(
        &self,
        request: HarnessInferRequest,
    ) -> Result<HarnessInferOutcome, InferGradeError> {
        let attested_version = self.probe_identity().await?;
        let scratch = tempfile::tempdir().map_err(|e| execution(format!("scratch dir: {e}")))?;
        let schema_path = write_schema(scratch.path(), request.schema.as_ref())?;
        let mut command = tokio::process::Command::new(&self.command);
        command
            .arg("exec")
            .arg("--json")
            .arg("--ephemeral")
            .arg("--ignore-user-config")
            .arg("--ignore-rules")
            .arg("--sandbox")
            .arg("read-only")
            .arg("--skip-git-repo-check")
            .arg("--color")
            .arg("never")
            .arg("-C")
            .arg(scratch.path());
        if let Some(model) = codex_model_arg(&request.requested_model) {
            command.arg("--model").arg(model);
        }
        if let Some(path) = schema_path.as_deref() {
            command.arg("--output-schema").arg(path);
        }
        command
            .arg("-")
            .env_clear()
            .envs(codex_env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = command
            .spawn()
            .map_err(|e| execution(format!("cannot spawn `{}`: {e}", self.command.display())))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| execution("spawned without stdin".to_owned()))?;
        stdin
            .write_all(render_prompt(&request).as_bytes())
            .await
            .map_err(|e| execution(format!("stdin: {e}")))?;
        stdin
            .shutdown()
            .await
            .map_err(|e| execution(format!("stdin close: {e}")))?;
        drop(stdin);

        let timeout = request.timeout.unwrap_or(DEFAULT_TIMEOUT);
        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| execution(format!("timed out after {} ms", timeout.as_millis())))?
            .map_err(|e| execution(format!("wait: {e}")))?;
        if output.stdout.len() > MAX_EVENT_BYTES || output.stderr.len() > MAX_EVENT_BYTES {
            return Err(execution("event stream exceeded 8 MiB".to_owned()));
        }
        if !output.status.success() {
            // The seat's OWN refusal is the witness: in `--json` mode codex
            // writes it as an `error` event on stdout and leaves stderr
            // empty (measured on 0.118.7: « The 'gpt-4o-mini' model is not
            // supported when using Codex with a ChatGPT account » rode a
            // 400 the run rendered as `exited 1 · `).
            let stderr = String::from_utf8_lossy(&output.stderr);
            let witness = match stderr.trim() {
                "" => refusal_from_events(&output.stdout).unwrap_or_default(),
                said => said.to_owned(),
            };
            return Err(execution(format!(
                "codex exec exited {} · {witness}",
                output.status
            )));
        }
        let observed = parse_events(&output.stdout)?;
        Ok(HarnessInferOutcome {
            output: observed.final_text,
            requested_model: request.requested_model,
            usage_observed: true,
            attested_version,
        })
    }
}

/// A seat returned only after all four infer-grade conjuncts pass.
#[derive(Debug, Clone)]
pub struct InferGradeSeat {
    adapter: CodexExec,
    attestation: InferGradeAttestation,
}

impl InferGradeSeat {
    /// Evidence that admitted the seat.
    #[must_use]
    pub const fn attestation(&self) -> InferGradeAttestation {
        self.attestation
    }

    /// Execute the admitted one-shot.
    ///
    /// # Errors
    ///
    /// The child cannot execute, violates the one-turn/tool-free contract,
    /// or omits its terminal answer or usage evidence.
    pub async fn run(
        &self,
        request: HarnessInferRequest,
    ) -> Result<HarnessInferOutcome, InferGradeError> {
        self.adapter.run(request).await
    }
}

/// Meet one named seat against a task need.
///
/// # Errors
///
/// The seat has no infer-grade attestation or a conjunct is insufficient.
pub fn meet_infer_grade(
    seat: &str,
    need: StructuredOutputGrade,
) -> Result<InferGradeSeat, InferGradeError> {
    meet_with_adapter(seat, need, CodexExec::new())
}

fn meet_with_adapter(
    seat: &str,
    need: StructuredOutputGrade,
    adapter: CodexExec,
) -> Result<InferGradeSeat, InferGradeError> {
    let Some(attestation) = (seat == "codex").then(|| adapter.attestation()) else {
        return Err(unattested(seat, need));
    };
    let failed = attestation.failed(need);
    if !failed.is_empty() {
        return Err(InferGradeError::Refused {
            witness: format!(
                "seat `{seat}` is not infer-grade for {}: failed {}",
                need.as_str(),
                failed.join(" ∧ ")
            ),
        });
    }
    Ok(InferGradeSeat {
        adapter,
        attestation,
    })
}

fn unattested(seat: &str, need: StructuredOutputGrade) -> InferGradeError {
    InferGradeError::Refused {
        witness: format!(
            "seat `{seat}` is not infer-grade for {}: unproven single_turn ∧ no_implicit_tools ∧ structured_output ≥ {} ∧ model_identity observable",
            need.as_str(),
            need.as_str()
        ),
    }
}

fn execution(detail: String) -> InferGradeError {
    InferGradeError::Execution {
        seat: "codex",
        detail,
    }
}

fn refused(witness: String) -> InferGradeError {
    InferGradeError::Refused { witness }
}

/// The pure half of the spawn-time attestation: the answer must carry a
/// version inside `CODEX_VERSION_PIN` AND name `CODEX_PRODUCT` — the
/// same two-sided judgement the ACP handshake applies to `agentInfo.name`
/// and `agentInfo.version`, read off `--version` because `codex exec` has
/// no initialize.
fn judge_identity(command: &Path, answer: &str) -> Result<(u32, u32), InferGradeError> {
    let seen = crate::probe::judge_version("codex", answer, &CODEX_VERSION_PIN).map_err(|e| {
        refused(format!(
            "spawn-time attestation of `{}`: {e}",
            command.display()
        ))
    })?;
    if !answer.lines().any(|line| line.contains(CODEX_PRODUCT)) {
        return Err(refused(format!(
            "spawn-time attestation of `{}`: `--version` answered {}.{} without naming \
             `{CODEX_PRODUCT}` ({:?}) — this binary is not the seat it claims to be",
            command.display(),
            seen.0,
            seen.1,
            answer.lines().next().unwrap_or("").trim()
        )));
    }
    Ok(seen)
}

/// The schema lands private to the seat (0600 · #1253): the scratch dir is
/// `$TMPDIR`, which a shared Linux `/tmp` makes world-readable.
fn write_schema(dir: &Path, schema: Option<&Value>) -> Result<Option<PathBuf>, InferGradeError> {
    let Some(schema) = schema else {
        return Ok(None);
    };
    let path = dir.join("output-schema.json");
    let bytes = serde_json::to_vec(schema)
        .map_err(|e| execution(format!("output schema serialization: {e}")))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|e| execution(format!("output schema write: {e}")))?;
    file.write_all(&bytes)
        .map_err(|e| execution(format!("output schema write: {e}")))?;
    Ok(Some(path))
}

/// The seat's refusal text from its JSONL stdout — the LAST `error`
/// event's message (or `turn.failed`'s), with the backend's nested JSON
/// unwrapped to its `error.message` when it parses; `None` when no
/// event carried one.
fn refusal_from_events(stdout: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(stdout).ok()?;
    let mut last = None;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let message = match event.get("type").and_then(Value::as_str) {
            Some("error") => event.get("message").and_then(Value::as_str),
            Some("turn.failed") => event
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str),
            _ => None,
        };
        if let Some(message) = message {
            last = Some(unwrap_nested_message(message));
        }
    }
    last
}

fn unwrap_nested_message(message: &str) -> String {
    serde_json::from_str::<Value>(message)
        .ok()
        .and_then(|nested| {
            nested
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| message.to_owned())
}

fn codex_env() -> BTreeMap<String, String> {
    #[allow(clippy::disallowed_methods)] // process boundary; filtered below
    let parent: BTreeMap<String, String> = std::env::vars().collect();
    crate::compose_env(&parent, &["CODEX_HOME".to_owned()])
}

fn codex_model_arg(requested: &str) -> Option<&str> {
    requested
        .split_once('/')
        .and_then(|(provider, model)| (provider == "openai").then_some(model))
}

fn render_prompt(request: &HarnessInferRequest) -> String {
    match request.system.as_deref() {
        Some(system) => format!(
            "<nika-system>\n{system}\n</nika-system>\n<nika-user>\n{}\n</nika-user>\n",
            request.prompt
        ),
        None => format!("{}\n", request.prompt),
    }
}

struct Observed {
    final_text: String,
}

fn parse_events(bytes: &[u8]) -> Result<Observed, InferGradeError> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| execution(format!("stdout is not utf-8: {e}")))?;
    let mut turns_started = 0_u32;
    let mut turns_completed = 0_u32;
    let mut usage_seen = false;
    let mut final_text = None;
    let mut terminal_seen = false;
    for (index, line) in text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let event: Value = serde_json::from_str(line)
            .map_err(|e| execution(format!("JSONL line {}: {e}", index + 1)))?;
        if terminal_seen {
            return Err(InferGradeError::Refused {
                witness: format!(
                    "codex infer-grade run emitted JSONL after terminal turn.completed at line {}",
                    index + 1
                ),
            });
        }
        match event.get("type").and_then(Value::as_str) {
            Some("thread.started") => {}
            Some("turn.started") => turns_started = turns_started.saturating_add(1),
            Some("turn.completed") => {
                turns_completed = turns_completed.saturating_add(1);
                usage_seen |= read_terminal_usage(&event)?;
                terminal_seen = true;
            }
            Some("item.started" | "item.completed") => {
                let item = event
                    .get("item")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        execution(format!("JSONL line {}: item object absent", index + 1))
                    })?;
                let kind = item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                match kind {
                    "agent_message" => {
                        if event.get("type").and_then(Value::as_str) == Some("item.completed") {
                            final_text =
                                item.get("text").and_then(Value::as_str).map(str::to_owned);
                        }
                    }
                    "reasoning" => {}
                    other => {
                        return Err(InferGradeError::Refused {
                            witness: format!(
                                "codex infer-grade run emitted implicit tool item `{other}`"
                            ),
                        });
                    }
                }
            }
            Some(other) => {
                return Err(InferGradeError::Refused {
                    witness: format!("codex infer-grade run emitted unrecognized event `{other}`"),
                });
            }
            None => return Err(execution(format!("JSONL line {}: type absent", index + 1))),
        }
    }
    if turns_started != 1 || turns_completed != 1 {
        return Err(InferGradeError::Refused {
            witness: format!(
                "codex infer-grade run violated single_turn: started={turns_started} completed={turns_completed}"
            ),
        });
    }
    if !usage_seen {
        return Err(execution(
            "turn.completed omitted its usage object".to_owned(),
        ));
    }
    let final_text =
        final_text.ok_or_else(|| execution("terminal agent_message absent".to_owned()))?;
    Ok(Observed { final_text })
}

fn read_terminal_usage(event: &Value) -> Result<bool, InferGradeError> {
    let usage = event
        .get("usage")
        .and_then(Value::as_object)
        .ok_or_else(|| execution("turn.completed usage is not an object".to_owned()))?;
    for key in ["input_tokens", "output_tokens"] {
        let value = usage
            .get(key)
            .ok_or_else(|| execution(format!("turn.completed usage.{key} is absent")))?;
        if value.as_u64().is_none() {
            return Err(execution(format!("turn.completed usage.{key} is not u64")));
        }
    }
    if let Some(value) = usage.get("cached_input_tokens")
        && value.as_u64().is_none()
    {
        return Err(execution(
            "turn.completed usage.cached_input_tokens is not u64".to_owned(),
        ));
    }
    Ok(true)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// The real CLI's `--version` answer, scripted: every fixture below
    /// passes the spawn-time attestation with it and exercises the exec
    /// contract past it. [`scripted_codex_with`] swaps it out.
    const VERSION_PRELUDE: &str =
        r#"if [ "${1:-}" = --version ]; then printf '%s\n' 'codex-cli 0.153.4'; exit 0; fi"#;

    fn scripted_codex(body: &str) -> (tempfile::TempDir, PathBuf) {
        scripted_codex_with(VERSION_PRELUDE, body)
    }

    fn scripted_codex_with(prelude: &str, body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let bin = dir.path().join("codex");
        std::fs::write(&bin, format!("#!/bin/sh\nset -eu\n{prelude}\n{body}\n")).expect("script");
        let mut permissions = std::fs::metadata(&bin).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&bin, permissions).expect("chmod");
        (dir, bin)
    }

    fn request() -> HarnessInferRequest {
        let mut request = HarnessInferRequest::new("classify this", "openai/gpt-5.5");
        request.schema = Some(serde_json::json!({
            "type": "object",
            "required": ["label"],
            "properties": {"label": {"type": "string"}},
            "additionalProperties": false
        }));
        request
    }

    #[test]
    fn claude_agent_class_refuses_infer_with_every_unproven_conjunct() {
        let err = meet_infer_grade("claude-code", StructuredOutputGrade::JsonSchema)
            .expect_err("ACP is not infer-grade");
        let witness = err.to_string();
        for term in [
            "claude-code",
            "single_turn",
            "no_implicit_tools",
            "structured_output",
            "model_identity",
        ] {
            assert!(witness.contains(term), "missing {term}: {witness}");
        }
    }

    #[test]
    fn the_codex_attestation_proves_all_four_conjuncts() {
        let seat =
            meet_infer_grade("codex", StructuredOutputGrade::JsonSchema).expect("codex is proved");
        let proof = seat.attestation();
        assert!(proof.single_turn);
        assert!(proof.no_implicit_tools);
        assert_eq!(proof.structured_output, StructuredOutputGrade::JsonSchema);
        assert!(proof.model_identity_observable);
        assert!(proof.proof.contains("scripted fake codex"));
    }

    #[tokio::test]
    async fn scripted_fake_codex_proves_json_schema_and_terminal_usage() {
        let body = r#"
json=false
schema=false
model=false
previous=
for arg in "$@"; do
  if [ "$previous" = schema ]; then [ -f "$arg" ]; schema=true; previous=; continue; fi
  if [ "$previous" = model ]; then [ "$arg" = gpt-5.5 ]; model=true; previous=; continue; fi
  case "$arg" in
    --json) json=true ;;
    --output-schema) previous=schema ;;
    --model) previous=model ;;
  esac
done
$json && $schema && $model
IFS= read -r prompt
[ "$prompt" = "classify this" ]
printf '%s\n' '{"type":"thread.started","thread_id":"t"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"{\"label\":\"safe\"}"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":91,"cached_input_tokens":7,"output_tokens":12}}'
"#;
        let (_dir, bin) = scripted_codex(body);
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::JsonSchema,
            CodexExec::with_command(bin),
        )
        .expect("meet");
        let out = seat.run(request()).await.expect("scripted run");
        assert_eq!(out.output, r#"{"label":"safe"}"#);
        assert_eq!(out.requested_model, "openai/gpt-5.5");
        assert!(out.usage_observed, "turn.completed usage was read");
        assert_eq!(
            out.attested_version,
            (0, 153),
            "the outcome carries what the seat answered at spawn, not the row"
        );
    }

    /// #1253 · the probe's own repro: an 8-line shell script ahead of PATH
    /// named `codex` that captures the prompt and answers a crafted event.
    /// It is refused at the spawn-time attestation — and the capture stays
    /// EMPTY, because nothing of the request is written before the probe.
    #[tokio::test]
    async fn the_eight_line_fake_from_the_issue_is_refused_before_the_prompt_reaches_it() {
        let body = r#"
capture="$(dirname "$0")/prompt-capture.txt"
cat > "$capture"
printf '%s' '{"type":"done","result":"arbitrary"}'
"#;
        let (dir, bin) = scripted_codex_with("", body);
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::JsonSchema,
            CodexExec::with_command(bin),
        )
        .expect("the static row admits the name");
        let err = seat.run(request()).await.expect_err("the shim is refused");
        assert!(matches!(err, InferGradeError::Refused { .. }), "{err}");
        let witness = err.to_string();
        assert!(witness.contains("spawn-time attestation"), "{witness}");
        assert!(witness.contains("printed no version"), "{witness}");
        assert!(
            witness.contains("not the adapter it claims to be"),
            "{witness}"
        );
        let captured = std::fs::read(dir.path().join("prompt-capture.txt")).unwrap_or_default();
        assert!(
            captured.is_empty(),
            "the prompt reached the shim: {captured:?}"
        );
    }

    /// A shim that learned to print a version but not the product name is
    /// still not the seat: identity is two-sided, like the ACP handshake.
    #[tokio::test]
    async fn a_bare_version_number_without_the_product_name_is_refused() {
        let (_dir, bin) = scripted_codex_with(
            r#"if [ "${1:-}" = --version ]; then echo '0.153.4'; exit 0; fi"#,
            "IFS= read -r _p\nexit 3",
        );
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::Text,
            CodexExec::with_command(bin),
        )
        .expect("meet");
        let err = seat
            .run(HarnessInferRequest::new("hi", "openai/gpt-5.5"))
            .await
            .expect_err("no product name refuses");
        let witness = err.to_string();
        assert!(witness.contains("without naming `codex-cli`"), "{witness}");
        assert!(witness.contains("0.153"), "{witness}");
    }

    #[tokio::test]
    async fn a_version_below_the_floor_is_refused_with_both_sides_named() {
        let (_dir, bin) = scripted_codex_with(
            r#"if [ "${1:-}" = --version ]; then echo 'codex-cli 0.148.0'; exit 0; fi"#,
            "IFS= read -r _p\nexit 3",
        );
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::Text,
            CodexExec::with_command(bin),
        )
        .expect("meet");
        let err = seat
            .run(HarnessInferRequest::new("hi", "openai/gpt-5.5"))
            .await
            .expect_err("below the floor refuses");
        let witness = err.to_string();
        assert!(witness.contains("0.148"), "{witness}");
        assert!(witness.contains("0.149"), "{witness}");
    }

    /// The pure judge, off the real CLI's prose and the shim shapes.
    #[test]
    fn judge_identity_reads_the_product_line_and_refuses_the_shims() {
        let cmd = Path::new("codex");
        assert_eq!(
            judge_identity(cmd, "codex-cli 0.153.4\n").expect("the real line"),
            (0, 153)
        );
        assert_eq!(
            judge_identity(cmd, "\ncodex-cli 0.149.1 (build abc)\n").expect("stderr side"),
            (0, 149)
        );
        assert!(judge_identity(cmd, "").is_err(), "silence is not a seat");
        assert!(
            judge_identity(cmd, "0.153.4").is_err(),
            "a bare number is not a seat"
        );
        assert!(
            judge_identity(cmd, "codex-cli 1.0.0").is_err(),
            "a new major is a new dialect"
        );
        assert!(
            judge_identity(cmd, r#"{"type":"done","result":"arbitrary"}"#).is_err(),
            "the crafted event is not a version"
        );
    }

    /// #1253 adjacent · the schema the seat reads is private to it (0600),
    /// witnessed by the scripted seat itself: it answers with the mode bits
    /// of the `--output-schema` file it was handed.
    #[tokio::test]
    async fn the_output_schema_lands_private_to_the_seat() {
        let body = r#"
previous=
mode=none
for arg in "$@"; do
  if [ "$previous" = schema ]; then mode="$(ls -l "$arg" | cut -c1-10)"; previous=; continue; fi
  case "$arg" in --output-schema) previous=schema ;; esac
done
IFS= read -r _prompt
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' "{\"type\":\"item.completed\",\"item\":{\"id\":\"m\",\"type\":\"agent_message\",\"text\":\"$mode\"}}"
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}'
"#;
        let (_dir, bin) = scripted_codex(body);
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::JsonSchema,
            CodexExec::with_command(bin),
        )
        .expect("meet");
        let out = seat.run(request()).await.expect("scripted run");
        assert_eq!(out.output, "-rw-------", "0600 · owner-only");
    }

    #[tokio::test]
    async fn an_implicit_tool_event_refuses_the_answer() {
        let body = r#"
IFS= read -r _prompt
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"i","type":"command_execution","command":"pwd"}}'
printf '%s\n' '{"type":"item.completed","item":{"id":"m","type":"agent_message","text":"no"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}'
"#;
        let (_dir, bin) = scripted_codex(body);
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::Text,
            CodexExec::with_command(bin),
        )
        .expect("meet");
        let err = seat
            .run(HarnessInferRequest::new("hi", "openai/gpt-5.5"))
            .await
            .expect_err("tool use refuses");
        assert!(err.to_string().contains("implicit tool"), "{err}");
    }

    /// The seat's own refusal is the witness (measured on 0.118.7): codex
    /// `--json` exits 1 with an EMPTY stderr and an `error` event on
    /// stdout whose message wraps the backend's 400 — the run used to
    /// read `codex exec exited exit status: 1 · ` and taught nothing.
    /// Revert the stdout parse (`unwrap_or_default()` → `String::new()`)
    /// and the witness goes blank again.
    #[tokio::test]
    async fn a_seat_refusal_on_stdout_is_the_witness_when_stderr_is_empty() {
        let body = r#"
IFS= read -r _prompt
printf '%s\n' '{"type":"thread.started","thread_id":"t"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Model metadata for `gpt-4o-mini` not found. Defaulting to fallback metadata; this can degrade performance and cause issues."}}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"error","message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The '"'"'gpt-4o-mini'"'"' model is not supported when using Codex with a ChatGPT account.\"}}"}'
printf '%s\n' '{"type":"turn.failed","error":{"message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The '"'"'gpt-4o-mini'"'"' model is not supported when using Codex with a ChatGPT account.\"}}"}}'
exit 1
"#;
        let (_dir, bin) = scripted_codex(body);
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::Text,
            CodexExec::with_command(bin),
        )
        .expect("meet");
        let err = seat
            .run(HarnessInferRequest::new("hi", "openai/gpt-4o-mini"))
            .await
            .expect_err("exit 1 refuses");
        let witness = err.to_string();
        assert!(
            witness.contains(
                "codex exec exited exit status: 1 · The 'gpt-4o-mini' model is not \
                              supported when using Codex with a ChatGPT account."
            ),
            "{witness}"
        );
        assert!(!witness.ends_with("· "), "never a blank witness: {witness}");
        // stderr still wins when the seat speaks there.
        let (_dir2, loud) = scripted_codex("IFS= read -r _p\necho 'unauthorized' >&2\nexit 2");
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::Text,
            CodexExec::with_command(loud),
        )
        .expect("meet");
        let err = seat
            .run(HarnessInferRequest::new("hi", "openai/gpt-4o-mini"))
            .await
            .expect_err("exit 2 refuses");
        assert!(
            err.to_string()
                .contains("exited exit status: 2 · unauthorized"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn two_turns_refuse() {
        let body = r#"
IFS= read -r _prompt
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"m","type":"agent_message","text":"no"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":999,"output_tokens":888}}'
"#;
        let (_dir, bin) = scripted_codex(body);
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::Text,
            CodexExec::with_command(bin),
        )
        .expect("meet");
        let err = seat
            .run(HarnessInferRequest::new("hi", "anthropic/claude"))
            .await
            .expect_err("two turns refuse");
        assert!(err.to_string().contains("single_turn"), "{err}");
    }

    #[tokio::test]
    async fn terminal_usage_numbers_never_enter_the_outcome() {
        let body = r#"
IFS= read -r _prompt
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"m","type":"agent_message","text":"yes"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":999,"output_tokens":888}}'
"#;
        let (_dir, bin) = scripted_codex(body);
        let seat = meet_with_adapter(
            "codex",
            StructuredOutputGrade::Text,
            CodexExec::with_command(bin),
        )
        .expect("meet");
        let out = seat
            .run(HarnessInferRequest::new("hi", "openai/gpt-5.5"))
            .await
            .expect("one shot");
        let receipt_input = format!("{out:?}");
        assert!(!receipt_input.contains("999"));
        assert!(!receipt_input.contains("888"));
        assert!(out.usage_observed);
    }
}
