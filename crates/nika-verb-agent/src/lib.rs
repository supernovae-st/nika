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
//! - **intrinsics** (`intrinsic` · private) — `agent:compose` drafts a
//!   Nika workflow and gets the full `nika check` verdict back in-turn
//!   (« generation is not permission »: composition yields an artifact
//!   + its certificate, never an execution);
//! - **telemetry** ([`observe`]) — every loop DECISION is an event;
//!   the L3 runtime maps them onto the `nika-event` agent kinds.
//!
//! v0.1 fences (spec §5): `ReAct` shape · sequential dispatch ·
//! cost/duration stops are engine concerns. (The spec's « no
//! reflection » fence is amended by ADR-093: ONE bounded corrective
//! nudge on detected no-progress, engine-internal, no YAML surface.)

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
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, Role, ToolDef,
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
            observer: Arc::new(NoopObserver),
        }
    }

    /// Override the intelligence-layer tuning (ADR-093).
    #[must_use]
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Wire the telemetry observer (the L3 runtime adapts it onto the
    /// `nika-event` agent kinds; tests capture it directly).
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn AgentObserver>) -> Self {
        self.observer = observer;
        self
    }
}

impl<P, T, D> AgentVerb<P, T, D>
where
    P: ProviderInferDyn,
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
        validate_params(&input)?;
        let whitelist = Whitelist::new(&input.tools);
        let defs = self.whitelisted_defs(&whitelist).await?;

        let model = input
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let max_turns = input.max_turns.unwrap_or(DEFAULT_MAX_TURNS);

        let mut router = ToolRouter::new(&defs, self.config.router.clone());
        let mut guard = Guard::new(self.config.guard.clone());
        #[allow(clippy::cast_possible_truncation)] // universe ≪ u32::MAX
        self.observer.on_event(&AgentEvent::RunStarted {
            model: model.clone(),
            universe: defs.len() as u32,
            routed: router.is_routing(),
        });

        let mut messages = opening_messages(&input);
        let mut turns: u32 = 0;
        let mut total_tokens: u64 = 0;
        let mut last_text = String::new();
        let mut last_observations = String::new();

        while turns < max_turns {
            turns += 1;
            self.observer
                .on_event(&AgentEvent::TurnStarted { turn: turns });

            // Per-turn routing: rank the universe against the LIVE task
            // context (prompt + the model's last words + last observations),
            // each component budgeted so a long prompt can't evict the live
            // tail the routing is supposed to read.
            let query = routing_query(&input.prompt, &last_text, &last_observations);
            let offered = self.route_turn(&router, &defs, &query, turns);

            let request = build_request(&model, messages.clone(), &input, offered);
            let response = self
                .infer_turn(turns, request, &mut total_tokens, &input)
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
            match classify_turn(&response, &text, &ctx)? {
                TurnVerdict::Done(output) => {
                    self.observer.on_event(&AgentEvent::Finished {
                        turns,
                        total_tokens,
                    });
                    return Ok(output);
                }
                TurnVerdict::Dispatch(tool_uses) => {
                    // The loop WILL iterate to feed results back. On the last
                    // allowed turn there is no iteration left to show them —
                    // stop NOW without spending the batch (mirrors the token
                    // gate's "before spending more": no wasted side effects).
                    if turns >= max_turns {
                        return Err(VerbAgentError::MaxTurns {
                            turns,
                            partial_output: last_text,
                        });
                    }
                    // All-whitelisted, non-sentinel tools · feed results back.
                    messages.push(Message::new(Role::Assistant, response.content.clone()));
                    last_observations = self
                        .dispatch_turn(turns, tool_uses, &mut router, &mut guard, &mut messages)
                        .await
                        .map_err(|(period, repeats)| VerbAgentError::Stalled {
                            period,
                            repeats,
                            partial_output: last_text.clone(),
                        })?;
                }
            }
        }

        Err(VerbAgentError::MaxTurns {
            turns,
            partial_output: last_text,
        })
    }

    /// One provider call: infer, fold its usage into the running total,
    /// and report the budget checkpoint. Maps a provider failure to 463.
    async fn infer_turn(
        &self,
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
        self.observer.on_event(&AgentEvent::BudgetCheckpoint {
            turn,
            total_tokens: *total_tokens,
            budget: input.max_tokens_total,
        });
        Ok(response)
    }

    /// One routing decision: select this turn's definitions + report it.
    fn route_turn(
        &self,
        router: &ToolRouter,
        defs: &[ToolDef],
        query: &str,
        turn: u32,
    ) -> Vec<ToolDef> {
        let (offered, selection) = router.select(defs, query, turn);
        self.observer.on_event(&AgentEvent::ToolsSelected {
            turn,
            offered: selection.offered,
            universe: selection.universe,
            by_source: selection.by_source,
        });
        offered
    }

    /// One Dispatch turn: run the batch, feed results into the
    /// transcript, then consult the stall guard (signature = actions +
    /// outcomes). Returns the observations digest for the next routing
    /// query, or the stall evidence `(period, repeats)` when the guard
    /// stops the run.
    async fn dispatch_turn(
        &self,
        turn: u32,
        tool_uses: Vec<(String, String, serde_json::Value)>,
        router: &mut ToolRouter,
        guard: &mut Guard,
        messages: &mut Vec<Message>,
    ) -> Result<String, (u32, u32)> {
        let batch = self.run_batch(turn, tool_uses, router).await;
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
                self.observer.on_event(&AgentEvent::Nudged {
                    turn,
                    reason,
                    period,
                });
                content.push(ContentBlock::Text {
                    text: guard::nudge_text(reason, period),
                });
            }
            GuardVerdict::Stall { period, repeats } => {
                self.observer.on_event(&AgentEvent::Stalled {
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

    /// Dispatch one turn's validated tool batch: intrinsics are served
    /// in-loop (never reach the executor seam), everything else goes
    /// through the invoke verb; telemetry + the guard signature parts
    /// are collected as the batch runs.
    async fn run_batch(
        &self,
        turn: u32,
        tool_uses: Vec<(String, String, serde_json::Value)>,
        router: &mut ToolRouter,
    ) -> BatchOutcome {
        let mut results: Vec<ContentBlock> = Vec::with_capacity(tool_uses.len());
        let mut sig_calls: Vec<(String, serde_json::Value)> = Vec::with_capacity(tool_uses.len());
        let mut sig_results: Vec<(String, bool)> = Vec::with_capacity(tool_uses.len());
        // The error STREAK counts real tool failures only — a compose
        // verdict of `invalid` is the EXPECTED feedback of the draft→repair
        // loop, never a tool fault, so it must not arm the error-streak
        // nudge (which would spend the one reflection budget during normal
        // repair). Tracked separately from the per-block is_error.
        let mut all_dispatch_errors = true;
        let mut had_dispatch = false;
        for (id, name, args) in tool_uses {
            let is_intrinsic = intrinsic::Intrinsic::parse(&name).is_some();
            let block = if let Some(intrinsic) = intrinsic::Intrinsic::parse(&name) {
                let outcome = self.run_intrinsic(turn, intrinsic, args.clone()).await;
                ContentBlock::ToolResult {
                    tool_use_id: id,
                    content: outcome.0,
                    is_error: outcome.1,
                }
            } else {
                self.dispatch(&id, &name, args.clone()).await
            };
            if let ContentBlock::ToolResult {
                content, is_error, ..
            } = &block
            {
                self.observer.on_event(&AgentEvent::ToolCompleted {
                    turn,
                    name: name.clone(),
                    is_error: *is_error,
                });
                if !is_intrinsic {
                    had_dispatch = true;
                    all_dispatch_errors &= *is_error;
                }
                sig_results.push((content.clone(), *is_error));
            }
            router.note_used(&name, turn);
            sig_calls.push((name, args));
            results.push(block);
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

    /// Run one loop intrinsic off the async executor — the static check
    /// (`nika-schema` parse + the full ladder over a ≤256 KiB model draft)
    /// is sync CPU work that must not starve sibling workflows on the
    /// runtime (the `nika-ocr`/`jq` `spawn_blocking` precedent). A join
    /// failure (runtime shutdown) feeds back as an error, never fatal.
    async fn run_intrinsic(
        &self,
        turn: u32,
        intrinsic: intrinsic::Intrinsic,
        args: serde_json::Value,
    ) -> (String, bool) {
        let join = tokio::task::spawn_blocking(move || match intrinsic {
            intrinsic::Intrinsic::Compose => intrinsic::run_compose(&args),
        })
        .await;
        match join {
            Ok((content, is_error, outcome)) => {
                self.observer.on_event(&AgentEvent::ComposeChecked {
                    turn,
                    valid: outcome.valid,
                    violations: outcome.violations,
                });
                (content, is_error)
            }
            Err(join_err) => {
                self.observer.on_event(&AgentEvent::ComposeChecked {
                    turn,
                    valid: false,
                    violations: 1,
                });
                (format!("agent:compose check task failed: {join_err}"), true)
            }
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
            // Exclude any source-supplied `nika:done` AND the whole
            // `agent:` namespace (the loop owns the sentinel + every
            // intrinsic · a poisoned def must never shadow them) +
            // sanitize names the way the invoke seam does (NIKA-450 parity):
            // a control-char/whitespace-padded name from a compromised MCP
            // `tools/list` must not be serialized into the model's tool list.
            .filter(|def| {
                def.name != DONE_TOOL
                    && !def.name.starts_with("agent:")
                    && whitelist.admits(&def.name)
                    && is_clean_tool_name(&def.name)
            })
            .collect();
        if whitelist.admits(DONE_TOOL) {
            // The loop owns the sentinel's definition — sources never do.
            defs.push(ToolDef::new(
                DONE_TOOL,
                "Finish the task. Pass `result` when the answer is a value; \
                 omit it to finish with your final message.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "result": {
                            "description": "The final answer value (any JSON)."
                        }
                    }
                }),
            ));
        }
        if whitelist.admits(intrinsic::COMPOSE_TOOL) {
            // Loop-owned like the sentinel: drafting stays default-deny
            // (absent from `tools:`, the model never sees it).
            defs.push(intrinsic::compose_def());
        }
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

/// Per-component character budgets for the routing query — the live tail
/// (`last_text` · `last_observations`) must always reach the ranker, so a
/// long prompt can't evict it via a single tail-truncating cap.
const QUERY_PROMPT_CHARS: usize = 2048;
const QUERY_TEXT_CHARS: usize = 1024;
const QUERY_OBS_CHARS: usize = 1024;

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

/// What the loop does after one model response.
enum TurnVerdict {
    /// A terminal was reached (natural · sentinel) — return this output.
    Done(AgentOutput),
    /// Continue: dispatch these (validated, non-sentinel) tool calls.
    Dispatch(Vec<(String, String, serde_json::Value)>),
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
    let tool_uses: Vec<(String, String, serde_json::Value)> = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => {
                Some((id.clone(), name.clone(), input.clone()))
            }
            _ => None,
        })
        .collect();

    // Terminal 1 · a concluded answer is a SUCCESS even if it spent the
    // last token (budgets stop CONTINUING, they don't fail a finished run).
    if tool_uses.is_empty() {
        let output = shape_output(AgentValue::Text(text.to_owned()), ctx.input.schema.as_ref())?;
        return Ok(TurnVerdict::Done(AgentOutput::new(
            output,
            AgentStopReason::Completed,
            ctx.turns,
            ctx.total_tokens,
        )));
    }

    // Security FIRST · validate the WHOLE batch before ANY dispatch. The
    // name is MODEL-controlled — an over-long or control-char name is a
    // violation in itself (matches no sane whitelist), its error REDACTED
    // so the security-path log can't be injected (NIKA-450 parity).
    for (_, name, _) in &tool_uses {
        if name.len() > MAX_TOOL_NAME_LEN
            || !is_clean_tool_name(name)
            || !ctx.whitelist.admits(name)
        {
            return Err(VerbAgentError::WhitelistViolation {
                tool: redact_tool_name(name),
            });
        }
    }

    // Terminal 2 · the sentinel is loop-owned (never dispatched). When it
    // shares a turn with other tools, those siblings do NOT run: the model
    // signalled completion · a side effect on a terminating turn would be
    // spent and never fed back. `done` wins, deterministically.
    if let Some((_, _, args)) = tool_uses.iter().find(|(_, name, _)| name == DONE_TOOL) {
        let value = match args.get("result") {
            Some(result) => shape_output(
                AgentValue::Structured(result.clone()),
                ctx.input.schema.as_ref(),
            )?,
            None => shape_output(
                AgentValue::Text(ctx.last_text.to_owned()),
                ctx.input.schema.as_ref(),
            )?,
        };
        return Ok(TurnVerdict::Done(AgentOutput::new(
            value,
            AgentStopReason::ExplicitCompletion,
            ctx.turns,
            ctx.total_tokens,
        )));
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
mod tests {
    use super::*;
    use nika_kernel::ai::provider::{InferResponse, StopReason, TokenUsage};
    use nika_kernel::runtime::tool_executor::ToolResult;
    use nika_kernel_mock::{MockProvider, MockToolDefinitionProvider, MockToolExecutor};

    fn usage(input: u64, output: u64) -> TokenUsage {
        let mut usage = TokenUsage::default();
        usage.input_tokens = input;
        usage.output_tokens = output;
        usage
    }

    fn text_response(text: &str) -> InferResponse {
        InferResponse::new(
            vec![ContentBlock::Text {
                text: text.to_owned(),
            }],
            usage(10, 5),
            StopReason::EndTurn,
        )
    }

    fn tool_use_response(id: &str, name: &str, args: serde_json::Value) -> InferResponse {
        InferResponse::new(
            vec![
                ContentBlock::Text {
                    text: format!("calling {name}"),
                },
                ContentBlock::ToolUse {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    input: args,
                },
            ],
            usage(10, 5),
            StopReason::ToolUse,
        )
    }

    fn def(name: &str) -> ToolDef {
        ToolDef::new(name, format!("{name} description"), serde_json::json!({}))
    }

    struct Rig {
        provider: Arc<MockProvider>,
        tools: Arc<MockToolExecutor>,
        verb: AgentVerb<MockProvider, MockToolExecutor, MockToolDefinitionProvider>,
    }

    fn rig(provider: MockProvider, tools: MockToolExecutor, universe: Vec<ToolDef>) -> Rig {
        let provider = Arc::new(provider);
        let tools = Arc::new(tools);
        let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
        let verb = AgentVerb::new(
            Arc::clone(&provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::with_defs(universe)),
            "mock/agent",
        );
        Rig {
            provider,
            tools,
            verb,
        }
    }

    // ── §6 · single-turn no-tools → Completed ──────────────────────────

    #[tokio::test]
    async fn no_tools_single_turn_completes() {
        let r = rig(
            MockProvider::new("mock").enqueue_response(text_response("the answer is 4")),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let out = r
            .verb
            .run(AgentInput::new("2+2?"))
            .await
            .expect("completes");
        assert_eq!(out.output, AgentValue::Text("the answer is 4".to_owned()));
        assert_eq!(out.stop_reason, AgentStopReason::Completed);
        assert_eq!(out.turns, 1);
        assert_eq!(out.total_tokens, 15);
        // Pure conversation: the model received NO tools.
        let reqs = r.provider.captured_requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].tools.is_empty());
    }

    // ── §6 · tool-use → dispatch → feed-back → final ────────────────────

    #[tokio::test]
    async fn tool_use_dispatches_feeds_back_then_completes() {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(tool_use_response(
                    "call-1",
                    "nika:read",
                    serde_json::json!({"path": "./notes.md"}),
                ))
                .enqueue_response(text_response("summary: notes say hello")),
            MockToolExecutor::new().enqueue_ok(ToolResult::success("call-1", "hello from notes")),
            vec![def("nika:read"), def("nika:write")],
        );
        let mut input = AgentInput::new("summarize my notes");
        input.tools = vec!["nika:read".to_owned()];
        let out = r.verb.run(input).await.expect("completes");
        assert_eq!(
            out.output,
            AgentValue::Text("summary: notes say hello".to_owned())
        );
        assert_eq!(out.turns, 2);
        assert_eq!(out.total_tokens, 30, "both turns counted");

        // The dispatch reached the REAL invoke verb with the model's args.
        let calls = r.tools.captured_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "nika:read");
        assert_eq!(calls[0].id.as_str(), "call-1", "engine-supplied id rides");

        // Turn 2 saw the full transcript: tool defs (filtered to the
        // whitelist) + the assistant turn + the fed-back result.
        let reqs = r.provider.captured_requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].tools.len(), 1, "universe filtered to whitelist");
        assert_eq!(reqs[0].tools[0].name, "nika:read");
        let turn2 = &reqs[1];
        assert_eq!(turn2.messages.len(), 3, "user · assistant · tool-results");
        assert!(matches!(turn2.messages[1].role, Role::Assistant));
        assert!(turn2.messages[2].content.iter().any(|b| matches!(
            b,
            ContentBlock::ToolResult { tool_use_id, content, is_error: false }
                if tool_use_id == "call-1" && content == "hello from notes"
        )));
    }

    // ── §6 · nika:done with / without result ───────────────────────────

    #[tokio::test]
    async fn done_sentinel_with_result_is_explicit_structured() {
        let r = rig(
            MockProvider::new("mock").enqueue_response(tool_use_response(
                "call-done",
                DONE_TOOL,
                serde_json::json!({"result": {"answer": 4}}),
            )),
            MockToolExecutor::new(),
            vec![def("nika:read")],
        );
        let mut input = AgentInput::new("finish with a value");
        input.tools = vec!["nika:*".to_owned()];
        let out = r.verb.run(input).await.expect("explicit completion");
        assert_eq!(
            out.output,
            AgentValue::Structured(serde_json::json!({"answer": 4}))
        );
        assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
        // The sentinel is loop-owned: NOTHING was dispatched.
        assert!(r.tools.captured_calls().is_empty());
        // And its def was synthesized into the model's tool list.
        let reqs = r.provider.captured_requests();
        assert!(reqs[0].tools.iter().any(|d| d.name == DONE_TOOL));
    }

    #[tokio::test]
    async fn done_sentinel_without_result_finishes_with_last_text() {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(tool_use_response("c1", "nika:read", serde_json::json!({})))
                .enqueue_response(InferResponse::new(
                    vec![
                        ContentBlock::Text {
                            text: "done reading — all good".to_owned(),
                        },
                        ContentBlock::ToolUse {
                            id: "c2".to_owned(),
                            name: DONE_TOOL.to_owned(),
                            input: serde_json::json!({}),
                        },
                    ],
                    usage(10, 5),
                    StopReason::ToolUse,
                )),
            MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "file body")),
            vec![def("nika:read")],
        );
        let mut input = AgentInput::new("read then finish");
        input.tools = vec!["nika:read".to_owned(), DONE_TOOL.to_owned()];
        let out = r.verb.run(input).await.expect("explicit completion");
        assert_eq!(
            out.output,
            AgentValue::Text("done reading — all good".to_owned())
        );
        assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
        assert_eq!(out.turns, 2);
    }

    // ── §6 · budgets are failures with partial_output ───────────────────

    #[tokio::test]
    async fn max_turns_fails_with_partial_output() {
        let looping = tool_use_response("c", "nika:read", serde_json::json!({}));
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(looping.clone())
                .enqueue_response(looping.clone())
                .enqueue_response(looping),
            MockToolExecutor::new()
                .enqueue_ok(ToolResult::success("c", "1"))
                .enqueue_ok(ToolResult::success("c", "2"))
                .enqueue_ok(ToolResult::success("c", "3")),
            vec![def("nika:read")],
        );
        let mut input = AgentInput::new("never finishes");
        input.tools = vec!["nika:read".to_owned()];
        input.max_turns = Some(3);
        let err = r.verb.run(input).await.expect_err("budget failure");
        assert!(matches!(
            &err,
            VerbAgentError::MaxTurns { turns: 3, partial_output }
                if partial_output == "calling nika:read"
        ));
    }

    #[tokio::test]
    async fn max_tokens_total_fails_at_the_exact_boundary_before_dispatch() {
        // Turn 1 spends exactly the budget (15) AND wants to continue (tool
        // call) → exhausted (`>=`) → MaxTokens BEFORE dispatching, with the
        // turn's text as partial_output. The `>` mutation survivor dies here.
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(tool_use_response("c", "nika:read", serde_json::json!({})))
                .enqueue_response(text_response("should never run")),
            MockToolExecutor::new().enqueue_ok(ToolResult::success("c", "data")),
            vec![def("nika:read")],
        );
        let mut input = AgentInput::new("expensive");
        input.tools = vec!["nika:read".to_owned()];
        input.max_tokens_total = Some(15); // turn 1 spends exactly 15
        let err = r.verb.run(input).await.expect_err("token budget");
        assert!(
            matches!(
                &err,
                VerbAgentError::MaxTokens { total_tokens: 15, partial_output }
                    if partial_output == "calling nika:read"
            ),
            "{err}"
        );
        // The budget gate fired BEFORE the tool ran (no wasted side effect).
        assert!(r.tools.captured_calls().is_empty());
        // And the second turn was never requested.
        assert_eq!(r.provider.captured_requests().len(), 1);
    }

    #[tokio::test]
    async fn natural_completion_over_budget_is_success_not_failure() {
        // A FINISHED answer is a success even if its turn crossed the budget
        // — budgets stop the loop from CONTINUING, they don't fail a
        // concluded run (spec §2: terminal-1 precedes the budget gate).
        let r = rig(
            MockProvider::new("mock").enqueue_response(text_response("the final answer")),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let mut input = AgentInput::new("one expensive answer");
        input.max_tokens_total = Some(1); // the single turn spends 15 > 1
        let out = r
            .verb
            .run(input)
            .await
            .expect("a concluded answer succeeds");
        assert_eq!(out.output, AgentValue::Text("the final answer".to_owned()));
        assert_eq!(out.stop_reason, AgentStopReason::Completed);
    }

    // ── multiple tool calls in ONE turn ─────────────────────────────────

    #[tokio::test]
    async fn multiple_tools_in_one_turn_all_dispatch_in_order() {
        let two_calls = InferResponse::new(
            vec![
                ContentBlock::ToolUse {
                    id: "a".to_owned(),
                    name: "nika:read".to_owned(),
                    input: serde_json::json!({"path": "a.txt"}),
                },
                ContentBlock::ToolUse {
                    id: "b".to_owned(),
                    name: "nika:glob".to_owned(),
                    input: serde_json::json!({"pattern": "*.md"}),
                },
            ],
            usage(10, 5),
            StopReason::ToolUse,
        );
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(two_calls)
                .enqueue_response(text_response("both done")),
            MockToolExecutor::new()
                .enqueue_ok(ToolResult::success("a", "A body"))
                .enqueue_ok(ToolResult::success("b", "b.md")),
            vec![def("nika:read"), def("nika:glob")],
        );
        let mut input = AgentInput::new("read and glob");
        input.tools = vec!["nika:read".to_owned(), "nika:glob".to_owned()];
        let out = r.verb.run(input).await.expect("completes");
        assert_eq!(out.output, AgentValue::Text("both done".to_owned()));
        let calls = r.tools.captured_calls();
        assert_eq!(calls.len(), 2, "both tools dispatched");
        assert_eq!(calls[0].name, "nika:read");
        assert_eq!(calls[1].name, "nika:glob");
        // Both results fed back in ONE user message.
        let turn2 = &r.provider.captured_requests()[1];
        let fed = &turn2.messages[2];
        assert_eq!(fed.content.len(), 2, "both tool results in one turn");
    }

    #[tokio::test]
    async fn whitelist_violation_on_second_tool_dispatches_neither() {
        // The whole batch is validated BEFORE any dispatch: the allowed
        // first tool must NOT run when a later sibling is denied.
        let two = InferResponse::new(
            vec![
                ContentBlock::ToolUse {
                    id: "a".to_owned(),
                    name: "nika:read".to_owned(),
                    input: serde_json::json!({}),
                },
                ContentBlock::ToolUse {
                    id: "b".to_owned(),
                    name: "nika:write".to_owned(),
                    input: serde_json::json!({"path": "/etc/x"}),
                },
            ],
            usage(10, 5),
            StopReason::ToolUse,
        );
        let r = rig(
            MockProvider::new("mock").enqueue_response(two),
            MockToolExecutor::new()
                .enqueue_ok(ToolResult::success("a", "leaked?"))
                .enqueue_ok(ToolResult::success("b", "never")),
            vec![def("nika:read"), def("nika:write")],
        );
        let mut input = AgentInput::new("partial escape");
        input.tools = vec!["nika:read".to_owned()]; // write NOT whitelisted
        let err = r.verb.run(input).await.expect_err("security stop");
        assert!(matches!(
            &err,
            VerbAgentError::WhitelistViolation { tool } if tool == "nika:write"
        ));
        assert!(
            r.tools.captured_calls().is_empty(),
            "the allowed sibling NEVER ran — batch validated before dispatch"
        );
    }

    #[tokio::test]
    async fn done_among_siblings_completes_without_dispatching_them() {
        // [nika:read, nika:done] in ONE turn: done wins, the sibling does
        // NOT run (no side effect on a terminating turn · no dropped result).
        let mixed = InferResponse::new(
            vec![
                ContentBlock::Text {
                    text: "final words".to_owned(),
                },
                ContentBlock::ToolUse {
                    id: "r".to_owned(),
                    name: "nika:read".to_owned(),
                    input: serde_json::json!({}),
                },
                ContentBlock::ToolUse {
                    id: "d".to_owned(),
                    name: DONE_TOOL.to_owned(),
                    input: serde_json::json!({}),
                },
            ],
            usage(10, 5),
            StopReason::ToolUse,
        );
        let r = rig(
            MockProvider::new("mock").enqueue_response(mixed),
            MockToolExecutor::new().enqueue_ok(ToolResult::success("r", "should not run")),
            vec![def("nika:read")],
        );
        let mut input = AgentInput::new("read then done together");
        input.tools = vec!["nika:read".to_owned(), DONE_TOOL.to_owned()];
        let out = r.verb.run(input).await.expect("explicit completion");
        assert_eq!(out.output, AgentValue::Text("final words".to_owned()));
        assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
        assert!(
            r.tools.captured_calls().is_empty(),
            "the sibling tool never ran — done preempts the batch"
        );
    }

    // ── mid-loop inference failure → NIKA-463 ───────────────────────────

    #[tokio::test]
    async fn inference_error_mid_loop_maps_to_463() {
        use nika_kernel::ai::provider::ProviderError;
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(tool_use_response("c", "nika:read", serde_json::json!({})))
                .enqueue_error(ProviderError::Api {
                    status: 500,
                    message: "upstream 500".to_owned(),
                }),
            MockToolExecutor::new().enqueue_ok(ToolResult::success("c", "data")),
            vec![def("nika:read")],
        );
        let mut input = AgentInput::new("fails on turn 2");
        input.tools = vec!["nika:read".to_owned()];
        let err = r.verb.run(input).await.expect_err("provider error");
        assert!(matches!(err, VerbAgentError::Inference { .. }), "{err}");
        assert_eq!(nika_error::traits::NikaErrorCode::nika_code(&err).num, 463);
    }

    // ── §6 · whitelist violation = immediate, zero dispatch ─────────────

    #[tokio::test]
    async fn whitelist_violation_fails_immediately_with_zero_dispatch() {
        let r = rig(
            MockProvider::new("mock").enqueue_response(tool_use_response(
                "c1",
                "nika:write",
                serde_json::json!({"path": "/etc/passwd"}),
            )),
            MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "never")),
            vec![def("nika:read"), def("nika:write")],
        );
        let mut input = AgentInput::new("try to escape");
        input.tools = vec!["nika:read".to_owned()];
        let err = r.verb.run(input).await.expect_err("security stop");
        assert!(matches!(
            &err,
            VerbAgentError::WhitelistViolation { tool } if tool == "nika:write"
        ));
        assert!(
            r.tools.captured_calls().is_empty(),
            "the denied tool was NEVER dispatched"
        );
    }

    // ── §6 · tool errors are fed back, never fatal ──────────────────────

    #[tokio::test]
    async fn tool_error_feeds_back_and_the_loop_continues() {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(tool_use_response("c1", "nika:read", serde_json::json!({})))
                .enqueue_response(text_response("recovered without the file")),
            MockToolExecutor::new(), // empty queue → dispatch error
            vec![def("nika:read")],
        );
        let mut input = AgentInput::new("resilient");
        input.tools = vec!["nika:read".to_owned()];
        let out = r.verb.run(input).await.expect("loop survives tool failure");
        assert_eq!(
            out.output,
            AgentValue::Text("recovered without the file".to_owned())
        );
        // The fed-back result is an ERROR result carrying the NIKA code.
        let reqs = r.provider.captured_requests();
        let fed_back = &reqs[1].messages[2];
        assert!(fed_back.content.iter().any(|b| matches!(
            b,
            ContentBlock::ToolResult { is_error: true, content, .. }
                if content.starts_with("NIKA-")
        )));
    }

    // ── §6 · final-message schema validation ────────────────────────────

    #[tokio::test]
    async fn schema_validates_the_final_text_into_structured() {
        let r = rig(
            MockProvider::new("mock").enqueue_response(text_response(r#"{"score": 9}"#)),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let mut input = AgentInput::new("rate it");
        input.schema = Some(serde_json::json!({
            "type": "object",
            "properties": {"score": {"type": "integer"}},
            "required": ["score"]
        }));
        let out = r.verb.run(input).await.expect("valid");
        assert_eq!(
            out.output,
            AgentValue::Structured(serde_json::json!({"score": 9}))
        );
    }

    #[tokio::test]
    async fn schema_rejects_a_nonconforming_final_message() {
        let r = rig(
            MockProvider::new("mock").enqueue_response(text_response("not json at all")),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let mut input = AgentInput::new("rate it");
        input.schema = Some(serde_json::json!({"type": "object"}));
        let err = r.verb.run(input).await.expect_err("schema gate");
        assert!(matches!(err, VerbAgentError::SchemaValidation { .. }));
    }

    #[tokio::test]
    async fn schema_extracts_json_from_a_fenced_final_message() {
        // The infer.schema: parity — real models wrap JSON in ``` fences /
        // prose; the agent must extract the balanced span, not reject it.
        let r = rig(
            MockProvider::new("mock").enqueue_response(text_response(
                "Here is the result:\n```json\n{\"score\": 9}\n```\nHope that helps!",
            )),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let mut input = AgentInput::new("rate it");
        input.schema = Some(serde_json::json!({
            "type": "object",
            "properties": {"score": {"type": "integer"}},
            "required": ["score"]
        }));
        let out = r.verb.run(input).await.expect("fenced JSON extracts");
        assert_eq!(
            out.output,
            AgentValue::Structured(serde_json::json!({"score": 9}))
        );
    }

    #[tokio::test]
    async fn schema_validates_the_done_result_value() {
        let r = rig(
            MockProvider::new("mock").enqueue_response(tool_use_response(
                "c",
                DONE_TOOL,
                serde_json::json!({"result": {"score": "nine"}}),
            )),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let mut input = AgentInput::new("rate it");
        input.tools = vec![DONE_TOOL.to_owned()];
        input.schema = Some(serde_json::json!({
            "type": "object",
            "properties": {"score": {"type": "integer"}},
            "required": ["score"]
        }));
        let err = r
            .verb
            .run(input)
            .await
            .expect_err("done result misses schema");
        assert!(matches!(err, VerbAgentError::SchemaValidation { .. }));
    }

    // ── §6 · seam failures + parameter validation ───────────────────────

    #[tokio::test]
    async fn tool_defs_failure_maps_to_466() {
        let provider = Arc::new(MockProvider::new("mock"));
        let tools = Arc::new(MockToolExecutor::new());
        let verb = AgentVerb::new(
            Arc::clone(&provider),
            Arc::new(InvokeVerb::new(Arc::clone(&tools))),
            Arc::new(MockToolDefinitionProvider::unavailable("mcp down")),
            "mock/agent",
        );
        let mut input = AgentInput::new("needs tools");
        input.tools = vec!["nika:*".to_owned()];
        let err = verb.run(input).await.expect_err("seam failure");
        assert!(matches!(err, VerbAgentError::ToolDefs { .. }));
        assert_eq!(nika_error::traits::NikaErrorCode::nika_code(&err).num, 466);
        // Pure conversation never touches the seam:
        let ok = verb.run(AgentInput::new("no tools")).await;
        assert!(
            ok.is_err(),
            "provider queue empty — but NOT a ToolDefs error"
        );
        assert!(matches!(ok.unwrap_err(), VerbAgentError::Inference { .. }));
    }

    #[tokio::test]
    async fn control_char_tool_name_is_a_redacted_violation_zero_dispatch() {
        // A model name with a newline (log-injection class) is a violation
        // by construction · the error carries a REDACTED form, never raw.
        let r = rig(
            MockProvider::new("mock").enqueue_response(tool_use_response(
                "c1",
                "nika:read\n<FAKE LOG LINE> level=error",
                serde_json::json!({}),
            )),
            MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "never")),
            vec![def("nika:read")],
        );
        let mut input = AgentInput::new("inject");
        input.tools = vec!["nika:*".to_owned()];
        let err = r.verb.run(input).await.expect_err("violation");
        let VerbAgentError::WhitelistViolation { tool } = &err else {
            panic!("expected WhitelistViolation, got {err}");
        };
        assert!(
            !tool.contains('\n'),
            "the logged name is sanitized: {tool:?}"
        );
        assert!(r.tools.captured_calls().is_empty(), "nothing dispatched");
    }

    #[tokio::test]
    async fn max_turns_above_the_ceiling_is_a_param_error() {
        let r = rig(
            MockProvider::new("mock"),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let mut input = AgentInput::new("ok");
        input.max_turns = Some(MAX_TURNS_CEILING + 1);
        let err = r.verb.run(input).await.expect_err("over ceiling");
        assert!(
            matches!(&err, VerbAgentError::InvalidParam { param, .. } if *param == "max_turns"),
            "{err}"
        );
    }

    #[test]
    fn is_clean_tool_name_draws_the_control_char_boundary_exactly() {
        // The boundary is 0x20: control chars (< 0x20) are dirty, SPACE
        // (0x20) is clean — an interior space is unusual but not a control
        // char (the `< 0x20` vs `<= 0x20` distinction).
        assert!(is_clean_tool_name("nika:read"));
        assert!(
            is_clean_tool_name("a b"),
            "interior space is not a control char"
        );
        assert!(!is_clean_tool_name("a\tb"), "tab (0x09) is a control char");
        assert!(!is_clean_tool_name("a\nb"), "newline is a control char");
        assert!(
            !is_clean_tool_name(" a"),
            "leading whitespace rejected (trim)"
        );
        assert!(
            !is_clean_tool_name("a "),
            "trailing whitespace rejected (trim)"
        );
        assert!(!is_clean_tool_name(""), "empty rejected");
        assert!(
            !is_clean_tool_name("a\u{7f}b"),
            "DEL (0x7f) is a control char"
        );
    }

    #[test]
    fn agent_output_new_constructs_the_non_exhaustive_struct() {
        let out = AgentOutput::new(
            AgentValue::Text("x".to_owned()),
            AgentStopReason::Completed,
            3,
            42,
        );
        assert_eq!(out.turns, 3);
        assert_eq!(out.total_tokens, 42);
    }

    #[tokio::test]
    async fn invalid_params_are_465() {
        let r = rig(
            MockProvider::new("mock"),
            MockToolExecutor::new(),
            Vec::new(),
        );
        for (input, param) in [
            (AgentInput::new("   "), "prompt"),
            (
                {
                    let mut i = AgentInput::new("ok");
                    i.temperature = Some(3.0);
                    i
                },
                "temperature",
            ),
            (
                {
                    let mut i = AgentInput::new("ok");
                    i.max_turns = Some(0);
                    i
                },
                "max_turns",
            ),
        ] {
            let err = r.verb.run(input).await.expect_err(param);
            assert!(
                matches!(&err, VerbAgentError::InvalidParam { param: p, .. } if *p == param),
                "{err}"
            );
        }
    }

    // ── model resolution + system prompt plumbing ───────────────────────

    #[tokio::test]
    async fn model_override_and_system_prompt_ride_the_request() {
        let r = rig(
            MockProvider::new("mock").enqueue_response(text_response("ok")),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let mut input = AgentInput::new("hello");
        input.model = Some("mock/special".to_owned());
        input.system = Some("be terse".to_owned());
        input.temperature = Some(0.2);
        let _ = r.verb.run(input).await.expect("completes");
        let req = &r.provider.captured_requests()[0];
        assert_eq!(req.model, "mock/special");
        assert_eq!(req.temperature, Some(0.2));
        assert!(matches!(req.messages[0].role, Role::System));
        assert!(matches!(req.messages[1].role, Role::User));
    }
}
