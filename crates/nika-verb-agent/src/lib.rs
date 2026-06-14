// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-verb-agent` — the `agent` verb executor (L2 · the 4th and last
//! verb · D-2026-05-22-N18).
//!
//! The multi-turn `ReAct` loop (spec `docs/crate-specs/nika-verb-agent.md`
//! §2): model response → whitelisted tool dispatch → results fed back →
//! repeat, until the model completes (no tool calls), the `nika:done`
//! sentinel fires, or a budget stops the run (turns · tokens — budgets
//! are FAILURES per spec, with `partial_output` preserved).
//!
//! ## Seams (all injected · INV-027 hermeticity)
//!
//! - inference: [`ProviderInferDyn`] — production passes the resolved
//!   `nika-providers` provider; tests script a mock. (The spec §1 DRAFT
//!   held the registry itself; the loop needs SCRIPTED multi-turn
//!   responses, which the echo-only registry mock can never produce —
//!   the kernel seam is the testable shape, the registry resolution is
//!   the wiring layer's job.)
//! - tool dispatch: [`InvokeVerb`] over `ToolExecuteDyn` (same-layer L2).
//! - tool definitions: [`ToolDefinitionProviderDyn`] — the §8 seam; the
//!   universe is filtered against the whitelist HERE (glob semantics
//!   stay with the whitelist owner).
//!
//! ## Security (spec §3)
//!
//! `tools:` is default-deny. A model tool-use outside the whitelist is
//! an IMMEDIATE failure (NIKA-462, zero dispatch) — security boundaries
//! are not model-negotiable. Failing TOOLS are fed back as error
//! results (the agentic convention); the loop continues on its budgets.
//!
//! ## Structured output (spec §2 `infer.schema:` parity · BUG#11)
//!
//! When the task declares `schema:`, the engine makes the FINAL answer
//! conform — the same guarantee `infer`+schema gives, with no author
//! hand-instruction. The loop is two-phase by necessity:
//!
//! - the **tool-calling turns** run UNCONSTRAINED (tools on, no schema);
//! - the **final answer** is the only thing the schema binds. A
//!   deliberate `nika:done` `result:` is validated directly (the model
//!   committed to that shape); a free-text answer (natural completion ·
//!   result-less `done`) is validated, and if it does not yet conform the
//!   loop RE-ASKS the provider WITH the schema wired (native
//!   `response_format` when supported · an instruction otherwise),
//!   bounded by [`DEFAULT_SCHEMA_RETRY_BUDGET`].
//!
//! The schema NEVER rides a tool-calling turn — tool-calling and
//! structured-output do not reliably coexist in one request across
//! providers (the anthropic wire rejects `response_format` outright;
//! openai/gemini are fragile combining the two), so the re-ask is a
//! tools-OFF turn and the conflict is sidestepped by construction. The
//! post-hoc validation remains the safety net (a malformed answer that
//! exhausts the budget is the NIKA-464 verdict).
//!
//! ## The intelligence layer (ADR-093 · engine-internal, zero YAML)
//!
//! Four orthogonal pieces ride the same loop, all deterministic and
//! all observable through the [`AgentObserver`] seam:
//!
//! - **per-turn tool routing** (`router` · private) — BM25 active
//!   discovery over the whitelisted universe (MCP-Zero-style;
//!   sovereign: the engine's own `nika-bm25` satellite, zero LLM calls);
//! - **stall guard** (`guard` · private) — windowed cycle detection
//!   over action+observation turn signatures, with a bounded Reflexion
//!   nudge before the NIKA-467 stop;
//! - **intrinsics** (`intrinsic` · private) — `nika:compose` drafts a
//!   Nika workflow and gets the full `nika check` verdict back in-turn
//!   (« generation is not permission »: composition yields an artifact
//!   + its certificate, never an execution);
//! - **telemetry** ([`observe`]) — every loop DECISION is an event;
//!   the L3 runtime maps them onto the `nika-event` agent kinds.
//!
//! v0.1 fences (spec §5): `ReAct` shape · cost/duration stops are
//! engine concerns. Two fences carry ADR amendments: « no reflection »
//! → ADR-093 (ONE bounded corrective nudge, engine-internal) ·
//! « sequential dispatch » → ADR-094 (one turn's batch resolves
//! CONCURRENTLY, results in request order — the transcript, signature
//! and event stream are byte-identical to sequential; no YAML surface).

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod config;
pub mod errors;
pub mod observe;
pub mod whitelist;

mod guard;
mod intrinsic;
mod router;
mod shape;

use std::sync::Arc;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, ProviderMeta,
    ResponseFormat, Role, ToolDef,
};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::runtime::agent::AgentStopReason;
use nika_verb_invoke::{InvokeInput, InvokeVerb, VerbInvokeError};

use crate::guard::{Guard, GuardVerdict};
use crate::router::ToolRouter;
use crate::shape::shape_output;

pub use config::{AgentConfig, GuardConfig, RouterConfig};
pub use errors::VerbAgentError;
pub use intrinsic::COMPOSE_TOOL;
pub use observe::{AgentEvent, AgentObserver, NoopObserver, NudgeReason, SourceCounts, ToolSource};
pub use whitelist::Whitelist;

/// Default turn budget (spec §1 · `max_turns` default 10).
pub const DEFAULT_MAX_TURNS: u32 = 10;

/// Default schema-validation retry budget for the FINAL free-text answer
/// (BUG#11 · parity with `nika_verb_infer::DEFAULT_SCHEMA_RETRY_BUDGET`).
///
/// When a `schema:` task's final answer arrives as prose (not yet a
/// conforming object), the loop re-asks it WITH the schema constrained to
/// the provider, up to this many extra provider round-trips before the
/// NIKA-464 verdict. `0` makes the final answer single-shot (validate the
/// text as-is, no re-ask). Spec-sanctioned: « MAY auto-retry validation
/// before emitting the schema-validation error ».
pub const DEFAULT_SCHEMA_RETRY_BUDGET: u8 = 2;

/// The sentinel tool the model calls to finish explicitly (spec §2).
/// Include it in `tools:` to enable explicit completion (default-deny:
/// absent ⇒ the model never sees the sentinel def and can't call it).
pub const DONE_TOOL: &str = "nika:done";

/// Hard ceiling on a turn budget — bounds the O(turns) transcript clones
/// even if an author writes `max_turns: u32::MAX` (validated in §param).
pub const MAX_TURNS_CEILING: u32 = 1000;

/// Longest a model-emitted tool name may be before it is a violation by
/// construction (bounds the whitelist match on untrusted input · a real
/// `nika:x`/`mcp:server/tool` id is well under this).
const MAX_TOOL_NAME_LEN: usize = 256;

/// The `agent` task input — spec fields, already CEL-resolved.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AgentInput {
    /// The initial user message (required · spec §agent).
    pub prompt: String,
    /// Optional system prompt.
    pub system: Option<String>,
    /// Optional model override of the verb's default.
    pub model: Option<String>,
    /// Tool whitelist · gitignore-style globs · DEFAULT-DENY (empty =
    /// pure conversation, no tools).
    pub tools: Vec<String>,
    /// Turn budget (default [`DEFAULT_MAX_TURNS`]).
    pub max_turns: Option<u32>,
    /// Cumulative token budget across all turns.
    pub max_tokens_total: Option<u64>,
    /// Sampling temperature (0-2 · validated here).
    pub temperature: Option<f32>,
    /// JSON Schema validating the FINAL output (spec `schema:`).
    pub schema: Option<serde_json::Value>,
}

impl AgentInput {
    /// A tool-less conversation with every optional field unset.
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            system: None,
            model: None,
            tools: Vec::new(),
            max_turns: None,
            max_tokens_total: None,
            temperature: None,
            schema: None,
        }
    }
}

/// The verb's result value — text, or the validated JSON value when the
/// task carried a `schema:` (or `nika:done` carried a `result:`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AgentValue {
    /// Free-form final assistant text.
    Text(String),
    /// Schema-validated structured output.
    Structured(serde_json::Value),
}

/// The `agent` verb output.
#[derive(Debug)]
#[non_exhaustive]
pub struct AgentOutput {
    /// The shaped output value (`.output` in spec terms).
    pub output: AgentValue,
    /// Why the loop ended (success reasons only — failures are errors).
    pub stop_reason: AgentStopReason,
    /// How many model turns ran.
    pub turns: u32,
    /// Cumulative tokens across all turns (input + output).
    pub total_tokens: u64,
}

impl AgentOutput {
    /// Construct an output (INV-019 · per-crate `new()` on every
    /// `#[non_exhaustive]` struct so L3/test harnesses can build one).
    #[must_use]
    pub fn new(
        output: AgentValue,
        stop_reason: AgentStopReason,
        turns: u32,
        total_tokens: u64,
    ) -> Self {
        Self {
            output,
            stop_reason,
            turns,
            total_tokens,
        }
    }
}

/// The multi-turn agentic loop — the `agent` verb executor.
pub struct AgentVerb<P, T, D> {
    provider: Arc<P>,
    invoke: Arc<InvokeVerb<T>>,
    tool_defs: Arc<D>,
    default_model: String,
    config: AgentConfig,
    // Extra provider round-trips the FINAL free-text answer may spend
    // converging on the task `schema:` (BUG#11 · infer parity). Not part
    // of AgentConfig: it is the schema-enforcement budget, sibling to the
    // verb's other top-level knobs, not intelligence-layer tuning.
    schema_retry_budget: u8,
    // dyn on purpose: a 4th OPTIONAL seam as a generic would infect every
    // embedder signature (Runtime::new already carries the verb generics);
    // the observer is telemetry, never on the data path's hot types.
    observer: Arc<dyn AgentObserver>,
}

impl<P, T, D> std::fmt::Debug for AgentVerb<P, T, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentVerb")
            .field("default_model", &self.default_model)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<P, T, D> AgentVerb<P, T, D> {
    /// Create the verb over its three injected seams (default config ·
    /// no-op observer).
    #[must_use]
    pub fn new(
        provider: Arc<P>,
        invoke: Arc<InvokeVerb<T>>,
        tool_defs: Arc<D>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            invoke,
            tool_defs,
            default_model: default_model.into(),
            config: AgentConfig::new(),
            schema_retry_budget: DEFAULT_SCHEMA_RETRY_BUDGET,
            observer: Arc::new(NoopObserver),
        }
    }

    /// Override the intelligence-layer tuning (ADR-093).
    #[must_use]
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Override the final-answer schema-validation retry budget (BUG#11).
    /// `0` = single-shot (validate the final text as-is, never re-ask).
    #[must_use]
    pub fn with_schema_retry_budget(mut self, budget: u8) -> Self {
        self.schema_retry_budget = budget;
        self
    }

    /// Wire the observer the [`Self::run`] path reports to (tests
    /// capture it directly; embedders driving the verb themselves use
    /// it as their telemetry tap).
    ///
    /// NOTE: callers of [`Self::run_observed`] — the L3 runtime among
    /// them — pass a RUN-SCOPED observer that REPLACES this one for
    /// that call (no tee): through the runtime, this verb-wide observer
    /// is not consulted.
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn AgentObserver>) -> Self {
        self.observer = observer;
        self
    }
}

impl<P, T, D> AgentVerb<P, T, D>
where
    P: ProviderInferDyn + ProviderMeta,
    T: nika_kernel::runtime::tool_executor::ToolExecuteDyn,
    D: ToolDefinitionProviderDyn,
{
    /// Execute the `agent` task.
    ///
    /// CANCEL SAFETY: between awaits the loop holds only owned state;
    /// dropping the future abandons the in-flight provider/tool call per
    /// THAT seam's contract (no partial messages are observable).
    ///
    /// # Errors
    ///
    /// [`VerbAgentError::InvalidParam`] (465) on an empty prompt or an
    /// out-of-range temperature · [`VerbAgentError::ToolDefs`] (466)
    /// when the definitions seam fails · [`VerbAgentError::Inference`]
    /// (463) on a mid-loop provider failure ·
    /// [`VerbAgentError::WhitelistViolation`] (462) the IMMEDIATE
    /// security stop · [`VerbAgentError::MaxTurns`] (460) /
    /// [`VerbAgentError::MaxTokens`] (461) the budget failures ·
    /// [`VerbAgentError::SchemaValidation`] (464) when the final output
    /// misses the task `schema:`.
    pub async fn run(&self, input: AgentInput) -> Result<AgentOutput, VerbAgentError> {
        let observer = Arc::clone(&self.observer);
        self.run_observed(input, &*observer).await
    }

    /// Execute the `agent` task, reporting decisions to a RUN-SCOPED
    /// observer instead of the verb-wide one set by [`Self::with_observer`].
    ///
    /// This is the L3 runtime's seam: the verb is one shared instance
    /// dispatching CONCURRENT tasks in a wave — a verb-wide observer
    /// would interleave their decision streams, while a per-call
    /// observer keeps each run's telemetry attributable (ADR-093).
    /// Same contract as [`Self::run`] otherwise.
    ///
    /// # Errors
    ///
    /// Exactly [`Self::run`]'s errors.
    pub async fn run_observed(
        &self,
        input: AgentInput,
        observer: &dyn AgentObserver,
    ) -> Result<AgentOutput, VerbAgentError> {
        validate_params(&input)?;
        let whitelist = Whitelist::new(&input.tools);
        let defs = self.whitelisted_defs(&whitelist).await?;
        let model = input
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let max_turns = input.max_turns.unwrap_or(DEFAULT_MAX_TURNS);
        let (mut router, mut guard) = self.arm_loop(observer, &model, &defs);

        let mut messages = opening_messages(&input);
        let mut turns: u32 = 0;
        let mut total_tokens: u64 = 0;
        let mut last_text = String::new();
        let mut last_observations = String::new();

        // `loop`, not `while turns < max_turns`: the Dispatch arm below is
        // the SOLE max_turns authority (it fires BEFORE spending the final
        // batch). A trailing `while`-condition exit could never be
        // reached — every turn at the budget returns from inside — so a
        // duplicate trailing `Err(MaxTurns)` would be dead, untestable
        // code (J2 review fold).
        loop {
            turns += 1;
            observer.on_event(&AgentEvent::TurnStarted { turn: turns });

            // Per-turn routing: rank the universe against the LIVE task
            // context (prompt + the model's last words + last observations),
            // each component budgeted so a long prompt can't evict the live
            // tail the routing is supposed to read.
            let query = routing_query(&input.prompt, &last_text, &last_observations);
            let offered = route_turn(observer, &router, &defs, &query, turns);

            let request = build_request(&model, messages.clone(), &input, offered);
            let response = self
                .infer_turn(observer, turns, request, &mut total_tokens, &input)
                .await?;
            let text = joined_text(&response.content);
            if !text.is_empty() {
                last_text.clone_from(&text);
            }

            // Decide this turn (terminals · security batch · budget) — the
            // ONE place the loop's exit conditions live (spec §2 ordering).
            let ctx = TurnCtx {
                input: &input,
                whitelist: &whitelist,
                turns,
                total_tokens,
                last_text: &last_text,
            };
            // Terminal verdicts return one output (a `FinalText` answer is
            // shaped to `schema:` here · BUG#11); Dispatch feeds results
            // back and iterates. One Finished event, one exit point.
            let output = match classify_turn(&response, &text, &ctx)? {
                TurnVerdict::Done(output) => output,
                TurnVerdict::FinalText { text, stop_reason } => {
                    self.finalize_schema(
                        observer,
                        &mut messages,
                        &model,
                        text,
                        &response,
                        stop_reason,
                        &input,
                        &mut total_tokens,
                        turns,
                    )
                    .await?
                }
                TurnVerdict::Dispatch(tool_uses) => {
                    last_observations = self
                        .dispatch_and_feed(
                            observer,
                            turns,
                            max_turns,
                            tool_uses,
                            response,
                            &mut router,
                            &mut guard,
                            &mut messages,
                            &last_text,
                        )
                        .await?;
                    continue;
                }
            };
            observer.on_event(&AgentEvent::Finished {
                turns,
                total_tokens,
            });
            return Ok(output);
        }
    }

    /// One Dispatch turn within the loop: stop at the turn budget (the SOLE
    /// `max_turns` exit · BEFORE spending the batch, mirroring the token
    /// gate's "no wasted side effects"), else append the assistant turn and
    /// feed the tool batch back. Returns the observations digest for the
    /// next routing query; maps a stall to NIKA-467.
    #[allow(clippy::too_many_arguments)] // the loop's owned state threaded
    // once into the dispatch step; splitting only relocates the args.
    async fn dispatch_and_feed(
        &self,
        observer: &dyn AgentObserver,
        turns: u32,
        max_turns: u32,
        tool_uses: Vec<ToolUse>,
        response: InferResponse,
        router: &mut ToolRouter,
        guard: &mut Guard,
        messages: &mut Vec<Message>,
        last_text: &str,
    ) -> Result<String, VerbAgentError> {
        if turns >= max_turns {
            return Err(VerbAgentError::MaxTurns {
                turns,
                partial_output: last_text.to_owned(),
            });
        }
        // All-whitelisted, non-sentinel tools · feed results back.
        messages.push(Message::new(Role::Assistant, response.content));
        self.dispatch_turn(observer, turns, tool_uses, router, guard, messages)
            .await
            .map_err(|(period, repeats)| VerbAgentError::Stalled {
                period,
                repeats,
                partial_output: last_text.to_owned(),
            })
    }

    /// Arm the intelligence layer for one run: build the router + the
    /// guard from config and announce the run (universe + routing mode).
    fn arm_loop(
        &self,
        observer: &dyn AgentObserver,
        model: &str,
        defs: &[ToolDef],
    ) -> (ToolRouter, Guard) {
        let router = ToolRouter::new(defs, self.config.router.clone());
        let guard = Guard::new(self.config.guard.clone());
        #[allow(clippy::cast_possible_truncation)] // universe ≪ u32::MAX
        observer.on_event(&AgentEvent::RunStarted {
            model: model.to_owned(),
            universe: defs.len() as u32,
            routed: router.is_routing(),
        });
        (router, guard)
    }

    /// One provider call: infer, fold its usage into the running total,
    /// and report the budget checkpoint. Maps a provider failure to 463.
    async fn infer_turn(
        &self,
        observer: &dyn AgentObserver,
        turn: u32,
        request: InferRequest,
        total_tokens: &mut u64,
        input: &AgentInput,
    ) -> Result<InferResponse, VerbAgentError> {
        let response = self
            .provider
            .infer(request)
            .await
            .map_err(|source| VerbAgentError::Inference { source })?;
        *total_tokens = total_tokens.saturating_add(
            response
                .usage
                .input_tokens
                .saturating_add(response.usage.output_tokens),
        );
        observer.on_event(&AgentEvent::BudgetCheckpoint {
            turn,
            total_tokens: *total_tokens,
            budget: input.max_tokens_total,
        });
        Ok(response)
    }

    /// Turn a free-text FINAL answer into a `schema:`-conforming object —
    /// the same guarantee `infer`+schema gives, but for the agent's final
    /// turn (BUG#11). The schema rides the PROVIDER (native
    /// `response_format` when supported · an instruction otherwise) so the
    /// model produces the object itself; the existing post-hoc validation
    /// is the safety net that still catches a malformed answer (NIKA-464).
    ///
    /// Zero re-asks when the answer already conforms (a well-behaved model
    /// · the common path). Otherwise the assistant's prose turn is appended
    /// and the provider is re-asked WITH the schema constrained, up to the
    /// retry budget. The tool loop never carries the schema — the re-ask is
    /// a tools-OFF turn, so the tools-vs-structured-output provider conflict
    /// (anthropic rejects `response_format` outright · openai/gemini are
    /// fragile combining the two) is sidestepped by construction.
    #[allow(clippy::too_many_arguments)] // a terminal-path helper threading
    // the loop's owned state once; splitting it would only relocate the args.
    async fn finalize_schema(
        &self,
        observer: &dyn AgentObserver,
        messages: &mut Vec<Message>,
        model: &str,
        answer: String,
        final_response: &InferResponse,
        stop_reason: AgentStopReason,
        input: &AgentInput,
        total_tokens: &mut u64,
        turn: u32,
    ) -> Result<AgentOutput, VerbAgentError> {
        // `FinalText` is only produced under a `schema:` task; if it is
        // somehow absent the answer simply stands as text (no panic, no
        // phantom error · the no-schema completion shape).
        let Some(schema) = input.schema.as_ref() else {
            return Ok(AgentOutput::new(
                AgentValue::Text(answer),
                stop_reason,
                turn,
                *total_tokens,
            ));
        };
        let validator = shape::compile(schema)?;

        // Try the answer as-is first — no wasted round-trip on a model that
        // already replied with a conforming object (the common path · keeps
        // structured agent tasks single-round-trip when the model complies).
        let mut detail = match shape::validate_text(&answer, &validator) {
            Ok(value) => {
                return Ok(shaped(value, stop_reason, turn, *total_tokens));
            }
            Err(detail) => detail,
        };

        // The answer is prose · append it and re-ask WITH the schema wired.
        // The cumulative token budget gates the TOOL loop (classify_turn);
        // the final shaping is bounded by schema_retry_budget instead — a
        // concluded answer is a success even over budget (spec §2 terminal).
        messages.push(Message::new(
            Role::Assistant,
            final_response.content.clone(),
        ));
        let native = self.provider.supports_response_format();
        for _ in 0..self.schema_retry_budget {
            messages.push(Message::text(
                Role::User,
                shape::reask_message(Some(&detail), schema),
            ));
            let request = schema_request(model, messages.clone(), input, schema, native);
            let response = self
                .infer_turn(observer, turn, request, total_tokens, input)
                .await?;
            let text = joined_text(&response.content);
            match shape::validate_text(&text, &validator) {
                Ok(value) => {
                    return Ok(shaped(value, stop_reason, turn, *total_tokens));
                }
                Err(d) => {
                    detail = d;
                    messages.push(Message::new(Role::Assistant, response.content));
                }
            }
        }
        Err(VerbAgentError::SchemaValidation { detail })
    }

    /// One Dispatch turn: run the batch, feed results into the
    /// transcript, then consult the stall guard (signature = actions +
    /// outcomes). Returns the observations digest for the next routing
    /// query, or the stall evidence `(period, repeats)` when the guard
    /// stops the run.
    async fn dispatch_turn(
        &self,
        observer: &dyn AgentObserver,
        turn: u32,
        tool_uses: Vec<ToolUse>,
        router: &mut ToolRouter,
        guard: &mut Guard,
        messages: &mut Vec<Message>,
    ) -> Result<String, (u32, u32)> {
        let batch = self.run_batch(observer, turn, tool_uses, router).await;
        // Consult the guard BEFORE pushing, so a nudge rides INSIDE the
        // same user message as the tool results — never a second adjacent
        // `Role::User` message (which some provider wires reject as
        // non-alternating roles · the anthropic wire serializes messages
        // verbatim · a trailing text block after tool results is the
        // documented-legal shape on every wire).
        let mut content = batch.results;
        match guard.observe_turn(batch.signature, batch.all_errors) {
            GuardVerdict::Proceed => {}
            GuardVerdict::Nudge(reason, period) => {
                observer.on_event(&AgentEvent::Nudged {
                    turn,
                    reason,
                    period,
                });
                content.push(ContentBlock::Text {
                    text: guard::nudge_text(reason, period),
                });
            }
            GuardVerdict::Stall { period, repeats } => {
                observer.on_event(&AgentEvent::Stalled {
                    turn,
                    period,
                    repeats,
                });
                return Err((period, repeats));
            }
        }
        messages.push(Message::new(Role::User, content));
        Ok(batch.observations_digest)
    }

    /// Dispatch one turn's validated tool batch — CONCURRENTLY, results
    /// in REQUEST order (ADR-094 · the `LLMCompiler` direction, Kim et al.
    /// 2023, arxiv.org/abs/2312.04511: calls the model batched into ONE
    /// turn are independent by construction — interleaved sequential
    /// round-trips waste wall-clock the same way interleaved reasoning
    /// does, `ReWOO`, arxiv.org/abs/2305.18323).
    ///
    /// Two phases keep every guarantee intact:
    /// 1. CONCURRENT resolve (`buffered(max_parallel_tools)` — yields in
    ///    INPUT order): intrinsics are served in-loop (never reach the
    ///    executor seam), everything else goes through the invoke verb.
    ///    No observer calls, no router mutation in this phase — two
    ///    concurrent composes must not interleave the event stream.
    /// 2. SEQUENTIAL fold (request order): telemetry, the router's
    ///    recency ledger, the guard signature parts. The transcript,
    ///    the signature, and the event stream are byte-identical to the
    ///    sequential dispatch's — `max_parallel_tools: 1` restores it
    ///    exactly.
    ///
    /// CANCEL SAFETY: dropping this future drops the buffered stream →
    /// in-flight calls cancel per THEIR seam contracts (a compose
    /// `spawn_blocking` runs to completion detached, result discarded —
    /// the documented blocking-pool contract).
    async fn run_batch(
        &self,
        observer: &dyn AgentObserver,
        turn: u32,
        tool_uses: Vec<ToolUse>,
        router: &mut ToolRouter,
    ) -> BatchOutcome {
        use futures_util::StreamExt;
        let cap = self.config.max_parallel_tools.max(1);
        let resolved: Vec<Resolved> =
            futures_util::stream::iter(tool_uses.into_iter().map(|u| self.resolve_tool(u)))
                .buffered(cap)
                .collect()
                .await;

        let mut results: Vec<ContentBlock> = Vec::with_capacity(resolved.len());
        let mut sig_calls: Vec<(String, serde_json::Value)> = Vec::with_capacity(resolved.len());
        let mut sig_results: Vec<(String, bool)> = Vec::with_capacity(resolved.len());
        // The error STREAK counts real tool failures only — a compose
        // verdict of `invalid` is the EXPECTED feedback of the draft→repair
        // loop, never a tool fault, so it must not arm the error-streak
        // nudge (which would spend the one reflection budget during normal
        // repair). Tracked separately from the per-block is_error.
        let mut all_dispatch_errors = true;
        let mut had_dispatch = false;
        for r in resolved {
            // An intrinsic reports ComposeChecked; a real dispatch reports
            // ToolCompleted. They are NOT both — `nika:compose` is
            // loop-served, never a tool invocation, so it must not surface
            // as one on the stream (a `tool_invoked` for a call that never
            // hit the executor would mislead every reader).
            if let Some(outcome) = r.compose {
                observer.on_event(&AgentEvent::ComposeChecked {
                    turn,
                    valid: outcome.valid,
                    violations: outcome.violations,
                });
            } else if let ContentBlock::ToolResult { is_error, .. } = &r.block {
                had_dispatch = true;
                all_dispatch_errors &= *is_error;
                observer.on_event(&AgentEvent::ToolCompleted {
                    turn,
                    name: r.name.clone(),
                    is_error: *is_error,
                });
            }
            // The guard signature reads EVERY observation (compose
            // included — a repeating compose draft is still a no-progress
            // loop) regardless of which event reported it.
            if let ContentBlock::ToolResult {
                content, is_error, ..
            } = &r.block
            {
                sig_results.push((content.clone(), *is_error));
            }
            router.note_used(&r.name, turn);
            sig_calls.push((r.name, r.args));
            results.push(r.block);
        }
        // ' '-joined so adjacent results don't fuse into phantom seam tokens
        // ("…statusfetch…") in the next turn's BM25 query.
        let observations_digest = sig_results
            .iter()
            .flat_map(|(content, _)| content.chars().take(512).chain(std::iter::once(' ')))
            .take(2048)
            .collect();
        BatchOutcome {
            signature: guard::turn_signature(&sig_calls, &sig_results),
            results,
            observations_digest,
            // No real dispatch this turn (compose-only) ⇒ no error streak.
            all_errors: had_dispatch && all_dispatch_errors,
        }
    }

    /// Resolve ONE tool call to its result block (phase-1 unit — pure
    /// with respect to loop state: no observer, no router).
    async fn resolve_tool(&self, u: ToolUse) -> Resolved {
        if let Some(intrinsic) = intrinsic::Intrinsic::parse(&u.name) {
            let (content, is_error, outcome) = self.run_intrinsic(intrinsic, u.args.clone()).await;
            Resolved {
                block: ContentBlock::ToolResult {
                    tool_use_id: u.id,
                    content,
                    is_error,
                },
                name: u.name,
                args: u.args,
                compose: Some(outcome),
            }
        } else {
            Resolved {
                block: self.dispatch(&u.id, &u.name, u.args.clone()).await,
                name: u.name,
                args: u.args,
                compose: None,
            }
        }
    }

    /// Run one loop intrinsic off the async executor — the static check
    /// (`nika-schema` parse + the full ladder over a ≤256 KiB model draft)
    /// is sync CPU work that must not starve sibling workflows on the
    /// runtime (the `nika-ocr`/`jq` `spawn_blocking` precedent). A join
    /// failure (runtime shutdown) feeds back as an error, never fatal.
    /// Telemetry is the FOLD's job (request order · ADR-094 phase 2).
    async fn run_intrinsic(
        &self,
        intrinsic: intrinsic::Intrinsic,
        args: serde_json::Value,
    ) -> (String, bool, intrinsic::ComposeOutcome) {
        let join = tokio::task::spawn_blocking(move || match intrinsic {
            intrinsic::Intrinsic::Compose => intrinsic::run_compose(&args),
        })
        .await;
        match join {
            Ok(done) => done,
            Err(join_err) => (
                format!("nika:compose check task failed: {join_err}"),
                true,
                intrinsic::ComposeOutcome {
                    valid: false,
                    violations: 1,
                },
            ),
        }
    }

    /// The whitelisted tool universe + the synthesized sentinel def.
    async fn whitelisted_defs(
        &self,
        whitelist: &Whitelist,
    ) -> Result<Vec<ToolDef>, VerbAgentError> {
        if whitelist.is_empty() {
            return Ok(Vec::new()); // pure conversation · skip the seam
        }
        let universe = self
            .tool_defs
            .tool_defs()
            .await
            .map_err(|source| VerbAgentError::ToolDefs { source })?;
        let mut defs: Vec<ToolDef> = universe
            .into_iter()
            // Drop every LOOP-OWNED name a source supplied (the loop
            // synthesizes its own · a poisoned def must never shadow them)
            // + sanitize names the way the invoke seam does (NIKA-450
            // parity): a control-char/whitespace-padded name from a
            // compromised MCP `tools/list` must not reach the model's list.
            .filter(|def| {
                !intrinsic::is_loop_owned(&def.name)
                    && whitelist.admits(&def.name)
                    && is_clean_tool_name(&def.name)
            })
            .collect();
        defs.extend(intrinsic::synthesized_defs(whitelist));
        Ok(defs)
    }

    /// Dispatch one whitelisted tool call; a failing tool is FED BACK
    /// (`is_error: true`), never fatal (spec §2 · the ONE exception is
    /// the whitelist, handled before dispatch).
    async fn dispatch(&self, id: &str, name: &str, args: serde_json::Value) -> ContentBlock {
        let mut call = InvokeInput::new(name);
        call.args = args;
        call.call_id = Some(id.to_owned());
        match self.invoke.run(call).await {
            Ok(output) => ContentBlock::ToolResult {
                tool_use_id: id.to_owned(),
                content: output.content,
                is_error: false,
            },
            Err(err) => ContentBlock::ToolResult {
                tool_use_id: id.to_owned(),
                content: feedback_text(&err),
                is_error: true,
            },
        }
    }
}

/// One resolved tool call (phase-1 output · ADR-094): the result block
/// plus what the fold needs (name + args for the signature/router · the
/// compose outcome for its telemetry).
struct Resolved {
    block: ContentBlock,
    name: String,
    args: serde_json::Value,
    compose: Option<intrinsic::ComposeOutcome>,
}

/// What one dispatched batch produced (results + the guard's evidence).
struct BatchOutcome {
    /// The tool-result blocks, in dispatch order.
    results: Vec<ContentBlock>,
    /// Turn signature over actions + observations (see `guard`).
    signature: u64,
    /// A bounded digest of the observations, for the next routing query.
    observations_digest: String,
    /// Whether EVERY call in the batch errored.
    all_errors: bool,
}

/// One turn's request — the live transcript + this turn's routed tools.
/// `defs` is consumed (the router already handed us an owned `Vec`) so a
/// large fail-open universe isn't cloned a second time per turn.
fn build_request(
    model: &str,
    messages: Vec<Message>,
    input: &AgentInput,
    defs: Vec<ToolDef>,
) -> InferRequest {
    let mut request = InferRequest::new(model, messages);
    request.temperature = input.temperature;
    request.tools = defs;
    request
}

/// The FINAL schema-constrained re-ask request (BUG#11): tools OFF (the
/// schema constraint and tool-calling do not reliably coexist in one
/// request across providers — anthropic rejects `response_format`,
/// openai/gemini are fragile), with the schema wired natively when the
/// provider supports it (`infer`+schema parity · `build_request` mirror).
fn schema_request(
    model: &str,
    messages: Vec<Message>,
    input: &AgentInput,
    schema: &serde_json::Value,
    native: bool,
) -> InferRequest {
    let mut request = InferRequest::new(model, messages);
    request.temperature = input.temperature;
    // tools deliberately left empty (default) — see the doc comment.
    if native {
        request.response_format = ResponseFormat::JsonSchema(schema.clone());
    }
    request
}

/// Assemble a finalized structured output (INV-019 constructor reuse).
fn shaped(
    value: serde_json::Value,
    stop_reason: AgentStopReason,
    turn: u32,
    total_tokens: u64,
) -> AgentOutput {
    AgentOutput::new(
        AgentValue::Structured(value),
        stop_reason,
        turn,
        total_tokens,
    )
}

/// Per-component character budgets for the routing query — the live tail
/// (`last_text` · `last_observations`) must always reach the ranker, so a
/// long prompt can't evict it via a single tail-truncating cap.
const QUERY_PROMPT_CHARS: usize = 2048;
const QUERY_TEXT_CHARS: usize = 1024;
const QUERY_OBS_CHARS: usize = 1024;

/// One routing decision: select this turn's definitions + report it.
fn route_turn(
    observer: &dyn AgentObserver,
    router: &ToolRouter,
    defs: &[ToolDef],
    query: &str,
    turn: u32,
) -> Vec<ToolDef> {
    let (offered, selection) = router.select(defs, query, turn);
    observer.on_event(&AgentEvent::ToolsSelected {
        turn,
        offered: selection.offered,
        universe: selection.universe,
        by_source: selection.by_source,
    });
    offered
}

/// Build the BM25 routing query from the live task context, each piece
/// bounded independently (a 100 KB prompt can't crowd out the model's
/// last words or the last observations — the signal routing ranks on).
fn routing_query(prompt: &str, last_text: &str, last_observations: &str) -> String {
    let take = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    format!(
        "{} {} {}",
        take(prompt, QUERY_PROMPT_CHARS),
        take(last_text, QUERY_TEXT_CHARS),
        take(last_observations, QUERY_OBS_CHARS),
    )
}

/// The loop state a single turn's decision reads (keeps `classify_turn`
/// a small pure function instead of a 7-argument signature).
struct TurnCtx<'a> {
    input: &'a AgentInput,
    whitelist: &'a Whitelist,
    turns: u32,
    total_tokens: u64,
    last_text: &'a str,
}

/// One model-emitted tool call, named (two adjacent `String`s in a raw
/// tuple invite a silent id/name swap; the struct makes every read
/// self-documenting · the same move as `TurnCtx`).
struct ToolUse {
    id: String,
    name: String,
    args: serde_json::Value,
}

/// What the loop does after one model response.
enum TurnVerdict {
    /// A terminal was reached and the output is FINALIZED — return it as
    /// is. Covers every no-schema completion and the `nika:done` result
    /// (a deliberate structured value · validated directly · BUG#11).
    Done(AgentOutput),
    /// A free-text final answer under a `schema:` task — the loop must turn
    /// it into a conforming object (validate · re-ask the provider WITH the
    /// schema constrained if it does not yet conform · BUG#11). Carries the
    /// answer text and the terminal reason it would close on.
    FinalText {
        /// The model's free-text final answer (Terminal 1 text, or the
        /// `last_text` a result-less `nika:done` finishes on).
        text: String,
        /// `Completed` (natural) or `ExplicitCompletion` (`nika:done`).
        stop_reason: AgentStopReason,
    },
    /// Continue: dispatch these (validated, non-sentinel) tool calls.
    Dispatch(Vec<ToolUse>),
}

/// Decide one turn — the ONE place the loop's exit conditions live, in
/// spec §2 order: terminal-1 (no tools → Completed, success even over
/// budget) → security batch-validate (before any dispatch) → terminal-2
/// (`nika:done` → `ExplicitCompletion`, wins over batch-mates) → budget
/// gate (`>=` exhausted, before spending more). Falls through to
/// `Dispatch` when the loop should iterate.
fn classify_turn(
    response: &InferResponse,
    text: &str,
    ctx: &TurnCtx<'_>,
) -> Result<TurnVerdict, VerbAgentError> {
    let tool_uses: Vec<ToolUse> = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => Some(ToolUse {
                id: id.clone(),
                name: name.clone(),
                args: input.clone(),
            }),
            _ => None,
        })
        .collect();

    // Terminal 1 · a concluded answer is a SUCCESS even if it spent the
    // last token (budgets stop CONTINUING, they don't fail a finished run).
    if tool_uses.is_empty() {
        return Ok(final_text_verdict(
            text.to_owned(),
            AgentStopReason::Completed,
            ctx,
        ));
    }

    // Security FIRST · validate the WHOLE batch before ANY dispatch. The
    // name is MODEL-controlled — an over-long or control-char name is a
    // violation in itself (matches no sane whitelist), its error REDACTED
    // so the security-path log can't be injected (NIKA-450 parity).
    for u in &tool_uses {
        if u.name.len() > MAX_TOOL_NAME_LEN
            || !is_clean_tool_name(&u.name)
            || !ctx.whitelist.admits(&u.name)
        {
            return Err(VerbAgentError::WhitelistViolation {
                tool: redact_tool_name(&u.name),
            });
        }
    }

    // Terminal 2 · the sentinel is loop-owned (never dispatched). When it
    // shares a turn with other tools, those siblings do NOT run: the model
    // signalled completion · a side effect on a terminating turn would be
    // spent and never fed back. `done` wins, deterministically.
    if let Some(done) = tool_uses.iter().find(|u| u.name == DONE_TOOL) {
        // A `result:` is a DELIBERATE structured value — validate it
        // directly (the model committed to that exact shape · a miss is a
        // verdict, never a re-ask · BUG#11). A result-LESS done finishes on
        // the last free text, which DOES get the schema re-ask.
        return match done.args.get("result") {
            Some(result) => {
                let value = shape_output(
                    AgentValue::Structured(result.clone()),
                    ctx.input.schema.as_ref(),
                )?;
                Ok(TurnVerdict::Done(AgentOutput::new(
                    value,
                    AgentStopReason::ExplicitCompletion,
                    ctx.turns,
                    ctx.total_tokens,
                )))
            }
            None => Ok(final_text_verdict(
                ctx.last_text.to_owned(),
                AgentStopReason::ExplicitCompletion,
                ctx,
            )),
        };
    }

    // The loop WILL iterate to feed tool results back · enforce the token
    // budget NOW (spec §2 case 3 · `>=` exhausted · before spending more).
    if let Some(budget) = ctx.input.max_tokens_total
        && ctx.total_tokens >= budget
    {
        return Err(VerbAgentError::MaxTokens {
            total_tokens: ctx.total_tokens,
            partial_output: ctx.last_text.to_owned(),
        });
    }

    Ok(TurnVerdict::Dispatch(tool_uses))
}

/// Route a free-text final answer: a no-schema task closes immediately
/// with the text; a `schema:` task defers to the loop's schema-finalize
/// (validate · re-ask · BUG#11) — the provider call can't run from this
/// sync classifier, so it returns [`TurnVerdict::FinalText`].
fn final_text_verdict(
    text: String,
    stop_reason: AgentStopReason,
    ctx: &TurnCtx<'_>,
) -> TurnVerdict {
    if ctx.input.schema.is_some() {
        TurnVerdict::FinalText { text, stop_reason }
    } else {
        TurnVerdict::Done(AgentOutput::new(
            AgentValue::Text(text),
            stop_reason,
            ctx.turns,
            ctx.total_tokens,
        ))
    }
}

/// Verb-boundary validation BEFORE any seam call (NIKA-465).
fn validate_params(input: &AgentInput) -> Result<(), VerbAgentError> {
    if input.prompt.trim().is_empty() {
        return Err(VerbAgentError::InvalidParam {
            param: "prompt",
            detail: "prompt must be a non-empty string".to_owned(),
        });
    }
    if let Some(temperature) = input.temperature
        && !(0.0..=2.0).contains(&temperature)
    {
        return Err(VerbAgentError::InvalidParam {
            param: "temperature",
            detail: format!("temperature must be within 0-2, got {temperature}"),
        });
    }
    if let Some(max_turns) = input.max_turns
        && !(1..=MAX_TURNS_CEILING).contains(&max_turns)
    {
        return Err(VerbAgentError::InvalidParam {
            param: "max_turns",
            detail: format!("max_turns must be 1-{MAX_TURNS_CEILING}, got {max_turns}"),
        });
    }
    Ok(())
}

/// Make a model-supplied tool name safe to log on the security path:
/// control chars stripped, length capped (NIKA-450 parity · the error
/// Display must never carry raw model bytes — log-injection class).
fn redact_tool_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(64).collect()
}

/// The opening transcript: optional system, then the user prompt.
fn opening_messages(input: &AgentInput) -> Vec<Message> {
    let mut messages = Vec::with_capacity(2);
    if let Some(system) = &input.system {
        messages.push(Message::text(Role::System, system.clone()));
    }
    messages.push(Message::text(Role::User, input.prompt.clone()));
    messages
}

/// Concatenate a response's text blocks (the assistant's words).
fn joined_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The typed feedback a failing tool sends the model — the NIKA code
/// rides along so the model can reason about (and the human can grep)
/// the failure class.
fn feedback_text(err: &VerbInvokeError) -> String {
    use nika_error::traits::NikaErrorCode;
    format!("{} · {err}", err.nika_code())
}

/// Is this a tool name the model may safely SEE (NIKA-450 parity)?
///
/// A def name from a compromised `tools/list` could carry control chars
/// or whitespace padding — it must not be serialized into the model's
/// tool list (log/wire injection class · the invoke seam rejects the
/// same shapes at dispatch · this is the GO-TO-model mirror of that).
fn is_clean_tool_name(name: &str) -> bool {
    !name.is_empty() && !name.bytes().any(|b| b < 0x20 || b == 0x7f) && name.trim() == name
}

#[cfg(test)]
mod tests;
