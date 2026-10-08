// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Which routes can run a one-shot `infer:` over ACP, and that one-shot.
//!
//! An ACP session proves an agentic loop; a one-shot inference also needs
//! the subscription harness meet (single turn · no implicit tools ·
//! structured output · model identity) with a PRE-EXECUTION bound on what
//! can run: no tool at all, or a named residue confined to the per-call
//! scratch — and any tool beat refuses the answer. A direct CLI one-shot
//! (`codex exec`, `claude -p` …) never satisfies an explicit `acp`
//! selection, whatever profile it carries.
//!
//! - `claude-code` — the audited claude-agent-acp 0.81.1 completion
//!   profile, the one ACP authoring uses: SDK options sent with
//!   `session/new` before any prompt (built-in `tools: []`, `mcpServers:
//!   {}` under `strictMcpConfig`, `settingSources: []`, no plugins, skills
//!   or agents, `maxTurns: 1`, no persisted session), a fresh scratch
//!   directory as the adapter's cwd, its identity admitted at
//!   `initialize`, the model and native effort applied and read back
//!   before the single prompt, every update judged (a tool, media or
//!   permission event refuses the whole answer) and `end_turn` required.
//!   `maxTurns: 1` bounds this one operation, never a conversation. The
//!   profile enforces no schema, so its structured output is text: a
//!   `schema:` task refuses before inference.
//! - `codex` — the codex-acp 1.13.1 completion profile (codex 0.156.1),
//!   whose contract is NOT « no tools »: every tool surface that can reach
//!   beyond the call is closed before the adapter starts and read back on
//!   the exact binary (shell and exec, code mode, web, apps, plugins, every
//!   configured MCP server, browser and computer use, image tools,
//!   subagents, hooks); the ACP mode `read-only` is applied and read back
//!   (the per-call scratch the only writable root, no network, every
//!   approval denied). One residue stays callable: `apply_patch`, which the
//!   model catalogue registers whenever the turn has an environment; it can
//!   only write inside the scratch, removed after the call, and a turn that
//!   carried any tool beat has its answer refused. Output level text.
//! - any other route — no audited ACP one-shot completion profile.

use std::pin::Pin;
use std::time::Duration;

use futures_core::Stream;
use nika_kernel::ai::harness::{
    DynAgentBackend, HarnessError, HarnessEvent, HarnessEventStream, HarnessOutcome,
    HarnessRequest, PermissionDecision,
};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::authoring::acp::{Completion, Profile};
use crate::{InferGradeAttestation, StructuredOutputGrade};

/// Why a route cannot run an `infer:` one-shot over ACP.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{witness}")]
#[non_exhaustive]
pub struct AcpOneShotRefused {
    /// The teaching witness.
    pub witness: String,
}

/// A route whose ACP one-shot profile is attested — the evidence the
/// subscription harness meet reads, and the door to run it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcpOneShot {
    /// The route id.
    pub seat: &'static str,
    /// What the profile proves.
    pub attestation: InferGradeAttestation,
    /// The adapter profile the one-shot runs under.
    profile: Profile,
}

/// The claude-agent-acp completion profile's evidence. Its output level is
/// text: the profile enforces no schema (the SDK's own structured output
/// would answer through a tool, which the profile forbids).
const CLAUDE_CODE: InferGradeAttestation = InferGradeAttestation {
    single_turn: true,
    no_implicit_tools: true,
    structured_output: StructuredOutputGrade::Text,
    model_identity_observable: true,
    proof: "audited claude-agent-acp 0.81.1 completion profile (tools [], mcpServers {} under strictMcpConfig, settingSources [], no plugins, skills or agents, maxTurns 1, persistSession false) · identity admitted at initialize · fresh scratch cwd · model and native effort applied and read back before the prompt · every update judged, a tool, media or permission event refuses · end_turn required · scripted ACP peers · live 2026-10-08 on claude-agent-acp 0.81.1: opus[1m] and effort max applied and read back before one prompt, the exact answer returned, a tool canary read no file and wrote none",
};

/// The codex-acp completion profile's evidence. NOT a no-tools profile:
/// `apply_patch` stays callable inside the per-call scratch (see the module
/// doc); its output level is text.
const CODEX: InferGradeAttestation = InferGradeAttestation {
    single_turn: true,
    no_implicit_tools: true,
    structured_output: StructuredOutputGrade::Text,
    model_identity_observable: true,
    proof: "codex-acp 1.13.1 completion profile (codex 0.156.1): every tool-bearing feature off, plugins off and every configured MCP server disabled, web search, agents, plan and user-input tools off, all read back with features list and mcp list on the exact bundled binary before the spawn, CODEX_CONFIG and CODEX_PATH pinning it · ACP mode read-only applied and read back (the per-call scratch the only writable root, no network, every approval denied) · identity admitted at initialize · model and native effort applied and read back before the prompt · every update judged, a tool, plan or permission beat refuses the answer · end_turn required · RESIDUE: apply_patch stays callable and can write inside the per-call scratch, removed after the call",
};

/// Meet `seat` for an ACP one-shot `infer:`.
///
/// # Errors
///
/// A route with no audited ACP one-shot completion profile in this build;
/// the witness names the gap.
pub fn meet_acp_one_shot(seat: &str) -> Result<AcpOneShot, AcpOneShotRefused> {
    let witness = match seat {
        "claude-code" => {
            return Ok(AcpOneShot {
                seat: "claude-code",
                attestation: CLAUDE_CODE,
                profile: Profile::ClaudeCode,
            });
        }
        "codex" => {
            return Ok(AcpOneShot {
                seat: "codex",
                attestation: CODEX,
                profile: Profile::Codex,
            });
        }
        other => format!("`{other}` has no audited ACP one-shot completion profile"),
    };
    Err(AcpOneShotRefused { witness })
}

impl AcpOneShot {
    /// The meet for one task's output need: the profile's level must cover
    /// it, or the task refuses before any spawn.
    ///
    /// # Errors
    ///
    /// A failed conjunct, named.
    pub fn grade(self, need: StructuredOutputGrade) -> Result<Self, AcpOneShotRefused> {
        let failed = self.attestation.failed(need);
        if failed.is_empty() {
            return Ok(self);
        }
        Err(AcpOneShotRefused {
            witness: format!(
                "`{}` over ACP is not infer-grade for {}: failed {} (its completion profile \
                 enforces no schema; drop `schema:` or declare an API route)",
                self.seat,
                need.as_str(),
                failed.join(" ∧ ")
            ),
        })
    }

    /// Run ONE `infer:` turn on the route's registry adapter under the
    /// completion profile, in a fresh scratch directory. `timeout` bounds
    /// the whole operation; dropping the future ends the session and its
    /// process group.
    ///
    /// # Errors
    ///
    /// The adapter is absent ([`HarnessError::Unavailable`]), its identity
    /// or a selection is refused before the prompt, the session dies, or
    /// the answer carries anything but text.
    pub async fn run(
        self,
        request: HarnessRequest,
        timeout: Option<Duration>,
    ) -> Result<HarnessOutcome, HarnessError> {
        let harness = crate::seat_from_id(self.seat)
            .map_err(|reason| HarnessError::Unavailable { reason })?
            .ok_or_else(|| HarnessError::Unavailable {
                reason: format!("no registry row builds the `{}` ACP adapter", self.seat),
            })?
            .for_completion(Completion::Infer)?;
        self.run_over(&harness, request, timeout).await
    }

    /// [`Self::run`] over a lent transport, which MUST drive the completion
    /// profile ([`Self::drive`] · a spawned adapter
    /// [`for_completion`](crate::SpawnedHarness)): the scratch directory,
    /// the deadline and the event judgment are this door's.
    ///
    /// # Errors
    ///
    /// As [`Self::run`].
    pub async fn run_over(
        self,
        transport: &(dyn DynAgentBackend + '_),
        mut request: HarnessRequest,
        timeout: Option<Duration>,
    ) -> Result<HarnessOutcome, HarnessError> {
        let scratch = tempfile::tempdir().map_err(|error| HarnessError::Unavailable {
            reason: format!("ACP infer cannot create its scratch directory: {error}"),
        })?;
        request.cwd = scratch.path().to_path_buf();
        let turn = collect(transport, request);
        let outcome = match timeout {
            Some(limit) => tokio::time::timeout(limit, turn)
                .await
                .unwrap_or_else(|_| Err(refused("ACP infer timed out; no answer accepted"))),
            None => turn.await,
        }?;
        exact(outcome)
    }

    /// Drive ONE one-shot `infer:` over `reader`/`writer` under this route's
    /// completion profile — the transport-generic twin of a spawned adapter,
    /// for scripted peers and embedders that own the process (a profile
    /// proven at spawn, like Codex's configuration, is the spawner's to prove).
    pub fn drive<R, W>(self, reader: R, writer: W, request: HarnessRequest) -> HarnessEventStream
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        crate::client::drive_profile(
            reader,
            writer,
            request,
            Duration::from_secs(crate::IDLE_TIMEOUT_SECS),
            Some(crate::authoring::acp::OneShot {
                role: Completion::Infer,
                profile: self.profile,
            }),
        )
    }
}

/// An explicit selection is exact: an answer during which the route moved
/// a model or an effort that was applied was not produced under it.
fn exact(outcome: HarnessOutcome) -> Result<HarnessOutcome, HarnessError> {
    match outcome.selection.moved_refusal() {
        Some(refusal) => Err(refusal),
        None => Ok(outcome),
    }
}

fn refused(reason: &str) -> HarnessError {
    HarnessError::Refused {
        reason: reason.to_owned(),
    }
}

/// The whole answer, or the refusal: text chunks only, then one completed
/// turn without images. The driver already refuses tool and permission
/// beats before they reach here; this is the second check.
async fn collect(
    transport: &(dyn DynAgentBackend + '_),
    request: HarnessRequest,
) -> Result<HarnessOutcome, HarnessError> {
    let mut stream = transport.run_agent_boxed(request).await?;
    while let Some(event) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
        match event? {
            HarnessEvent::MessageChunk { .. } => {}
            HarnessEvent::Completed { outcome } if outcome.images.is_empty() => {
                return Ok(*outcome);
            }
            HarnessEvent::PermissionAsked { reply, .. } => {
                reply.respond(PermissionDecision::Deny);
                return Err(refused("ACP infer requested a tool; no answer accepted"));
            }
            _ => {
                return Err(refused(
                    "ACP infer returned a tool, media or unsupported event; no answer accepted",
                ));
            }
        }
    }
    Err(HarnessError::Session {
        reason: "ACP infer ended without a completed answer".to_owned(),
    })
}

#[cfg(test)]
mod tests;
