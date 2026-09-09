// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Human approval as a bounded capability (NEP-0013): shown content is
//! hash-bound to one run and step, tickets expire and are single-use, each
//! run has a mint limit, and every decision is journaled. A resumed answer is
//! validated before binding; refusing or abandoning it grants no authority.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use serde_json::{Value, json};

use crate::expr::{self, Scope};
use crate::record::TaskRecord;

/// Fresh-consent window (NEP-0013 law 2).
pub const APPROVAL_TTL_SECONDS: u32 = 15 * 60;

/// Per-run prompt-storm bound (NEP-0013 law 3).
pub const APPROVAL_MAX_TICKETS_PER_RUN: u32 = 5;

/// Wire code for non-transient approval-capability refusals.
pub const APPROVAL_CODE: &str = "NIKA-SEC-010";

/// Bump when the canonical shown-content shape changes.
const CONTENT_RECIPE_VERSION: u32 = 2;

/// What a ticket resolved to (NEP-0013 law 4 · the wire vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApprovalDecision {
    /// Minted, not yet answered (the `workflow_paused` frame carries
    /// this state).
    Pending,
    /// The gate resolved to proceed.
    Allow,
    /// The gate resolved to refuse (a confirm answered false · an
    /// engine refusal — the event's `why` names the law applied).
    Deny,
    /// Legacy wire value retained for historical trace readers.
    Dedup,
}

impl ApprovalDecision {
    /// The wire word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Dedup => "dedup",
        }
    }
}

/// One bounded-approval capability, minted before the prompt runs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApprovalTicket {
    /// blake3 hex over the JCS canonical shown content (law 1).
    pub content_hash: String,
    /// The run's identity — its `workflow_started` event id (law 2).
    pub run_nonce: String,
    /// The prompt task this ticket was minted at.
    pub step: String,
    /// Mint time from the run's injected clock, in Unix milliseconds.
    pub minted_at_ms: i64,
    /// The TTL the mint carries ([`APPROVAL_TTL_SECONDS`] today).
    pub ttl_seconds: u32,
    /// The decision state — a fresh mint is [`ApprovalDecision::Pending`].
    pub decision: ApprovalDecision,
}

impl ApprovalTicket {
    /// Mint (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(
        content_hash: String,
        run_nonce: String,
        step: String,
        minted_at_ms: i64,
        ttl_seconds: u32,
    ) -> Self {
        Self {
            content_hash,
            run_nonce,
            step,
            minted_at_ms,
            ttl_seconds,
            decision: ApprovalDecision::Pending,
        }
    }

    /// JCS + blake3 over the mint fields; the later decision is excluded.
    #[must_use]
    pub fn digest(&self) -> Option<String> {
        crate::resume::jcs_blake3_hex(&json!({
            "v": CONTENT_RECIPE_VERSION,
            "ticket": {
                "content_hash": self.content_hash,
                "run_nonce": self.run_nonce,
                "step": self.step,
                "minted_at_ms": self.minted_at_ms,
                "ttl_seconds": self.ttl_seconds,
            },
        }))
    }

    /// Law 2 — the consent is stale at the boundary (`sudo timestamp`
    /// semantics: valid strictly inside the window).
    #[must_use]
    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.ttl_remaining_seconds(now_ms) == 0
    }

    /// Seconds of freshness left at `now_ms` (saturating at 0).
    #[must_use]
    pub fn ttl_remaining_seconds(&self, now_ms: i64) -> i64 {
        let ttl_ms = i64::from(self.ttl_seconds).saturating_mul(1000);
        let left_ms = self.minted_at_ms.saturating_add(ttl_ms) - now_ms;
        (left_ms.max(0)) / 1000
    }
}

/// A journaled ticket folded from a paused trace with that trace's run nonce.
#[derive(Clone)]
#[non_exhaustive]
pub struct PausedApproval {
    /// The ticket the paused run journaled on its `workflow_paused` frame.
    pub ticket: ApprovalTicket,
    /// The run identity of the trace the ticket was folded FROM.
    pub trace_nonce: String,
    claim: Arc<ApprovalClaim>,
}

impl PausedApproval {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(ticket: ApprovalTicket, trace_nonce: String) -> Self {
        Self {
            ticket,
            trace_nonce,
            claim: Arc::new(ApprovalClaim::ephemeral()),
        }
    }

    /// Bind the ticket to a descriptor-held, create-once claim store.
    ///
    /// # Errors
    /// Returns an error when the owned claim directory cannot be opened.
    pub fn with_durable_claim_root(mut self, root: &Path) -> io::Result<Self> {
        let store = nika_fs::OwnedDir::create(root, &[".nika", "approval-claims"])?;
        self.claim = Arc::new(ApprovalClaim::durable(store));
        Ok(self)
    }

    fn consume(&self) -> Result<(), ClaimError> {
        if self.claim.consumed.swap(true, Ordering::AcqRel) {
            return Err(ClaimError::Consumed);
        }
        let Some(dir) = &self.claim.dir else {
            return Ok(());
        };
        let digest = self.ticket.digest().ok_or(ClaimError::Unavailable)?;
        let name = format!(".nika-approval-{digest}.claimed");
        match dir.write_once(&name, &format!("{digest}\n")) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(ClaimError::Consumed),
            Err(_) => Err(ClaimError::Unavailable),
        }
    }
}

impl std::fmt::Debug for PausedApproval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PausedApproval")
            .field("ticket", &self.ticket)
            .field("trace_nonce", &self.trace_nonce)
            .finish_non_exhaustive()
    }
}

impl PartialEq for PausedApproval {
    fn eq(&self, other: &Self) -> bool {
        self.ticket == other.ticket && self.trace_nonce == other.trace_nonce
    }
}

impl Eq for PausedApproval {}

struct ApprovalClaim {
    consumed: AtomicBool,
    dir: Option<nika_fs::OwnedDir>,
}

impl ApprovalClaim {
    fn ephemeral() -> Self {
        Self {
            consumed: AtomicBool::new(false),
            dir: None,
        }
    }

    fn durable(dir: nika_fs::OwnedDir) -> Self {
        Self {
            consumed: AtomicBool::new(false),
            dir: Some(dir),
        }
    }
}

enum ClaimError {
    Consumed,
    Unavailable,
}

/// Payload for the settle spine's `approval_decided` frame.
pub(crate) struct ApprovalAttestation {
    pub task: String,
    pub mode: String,
    pub decision: &'static str,
    /// Provenance: `resume`, `cli`, `policy`, `builtin`, or `engine`.
    pub source: &'static str,
    pub shown_hash: String,
    pub digest: Option<String>,
    pub run_nonce: String,
    pub ttl_seconds: u32,
    pub ttl_remaining_seconds: i64,
    /// The law applied on an engine refusal.
    pub why: Option<&'static str>,
    /// WHO answered (#1284 · additive): `NIKA_OPERATOR` when declared,
    /// else declared `user@host` — informational, never authentication.
    pub operator: String,
    /// WHAT was asked — the gate's resolved `message` (secret-marker
    /// scope · the same text the shown-hash binds). `None` when the
    /// prompt declared none.
    pub question: Option<String>,
    /// WHAT was answered — the decided value, rendered (spec 04 value
    /// rendering). `None` on a refusal (nothing was decided).
    pub answer: Option<String>,
}

impl ApprovalAttestation {
    /// The frame fields (additive · older readers ignore them).
    pub(crate) fn fields(&self) -> Vec<(&'static str, nika_types::resource::Value)> {
        let mut fields = vec![
            ("task", crate::s(&self.task)),
            ("mode", crate::s(&self.mode)),
            ("decision", crate::s(self.decision)),
            ("source", crate::s(self.source)),
            ("shown_hash", crate::s(&self.shown_hash)),
            ("run_nonce", crate::s(&self.run_nonce)),
            ("ttl_seconds", crate::i(i64::from(self.ttl_seconds))),
            (
                "ttl_remaining_seconds",
                crate::i(self.ttl_remaining_seconds),
            ),
        ];
        if let Some(digest) = &self.digest {
            fields.push(("digest", crate::s(digest)));
        }
        if let Some(why) = self.why {
            fields.push(("why", crate::s(why)));
        }
        fields.push(("operator", crate::s(&self.operator)));
        if let Some(question) = &self.question {
            fields.push(("question", crate::s(question)));
        }
        if let Some(answer) = &self.answer {
            fields.push(("answer", crate::s(answer)));
        }
        fields
    }
}

/// The operator attribution (#1284) — pure and total: a non-empty
/// `NIKA_OPERATOR` declaration wins whole; else `user@host`; an absent
/// piece reads `unknown`, never a guess.
#[must_use]
pub(crate) fn compose_operator(
    declared: Option<String>,
    user: Option<String>,
    host: Option<String>,
) -> String {
    if let Some(operator) = declared.filter(|d| !d.is_empty()) {
        return operator;
    }
    let piece = |v: Option<String>| {
        v.filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into())
    };
    format!("{}@{}", piece(user), piece(host))
}

/// The gate's verdict (pipeline-facing).
pub(crate) enum Gate {
    /// Not a direct `invoke: nika:prompt` — the pipeline runs unchanged.
    NotPrompt,
    /// Run the possibly answer-bound task.
    Run(Box<RawTask>),
    /// Refuse before the task starts and retain the attestation.
    Refused(Box<Refusal>),
}

/// A typed approval refusal — the failure detail plus the attestation.
pub(crate) struct Refusal {
    pub detail: String,
    pub attestation: ApprovalAttestation,
}

/// One action in a prompt's unleashed closure.
#[derive(Debug, Clone)]
struct GatedAction {
    task: String,
    classes: Vec<&'static str>,
}

/// Per-content mint state.
struct MintedApproval {
    ticket: ApprovalTicket,
    decided: Option<(ApprovalDecision, Value)>,
}

/// Per-step state read by pause and attestation paths.
struct StepEntry {
    ticket: ApprovalTicket,
    mode: String,
    source: &'static str,
    /// The gate's resolved `message` (#1284 · rides the attestation).
    question: Option<String>,
}

#[derive(Default)]
struct BookInner {
    operator: String,
    /// This run's `workflow_started` event id.
    nonce: String,
    /// Prompt step → its unleashed closure (computed once per run).
    gated: BTreeMap<String, Vec<GatedAction>>,
    /// Content hash → mint state.
    minted: BTreeMap<String, MintedApproval>,
    /// Prompt step → its entry.
    steps: BTreeMap<String, StepEntry>,
    /// The anti-fatigue counter (law 3) — DISTINCT mints this run.
    mints: u32,
    /// The folded resume authority (set by the composer).
    paused: Option<PausedApproval>,
}

/// Per-run approval state shared across concurrent task pipelines.
pub(crate) struct ApprovalBook {
    inner: Mutex<BookInner>,
}

/// The book's answer to the gate.
enum Admit {
    /// Run — binding this answer (a validated CLI/resume answer) when set.
    Run { bind: Option<Value> },
    /// Refuse — the typed detail + the attestation.
    Refused(Box<Refusal>),
}

impl Admit {
    fn refused(refusal: Refusal) -> Self {
        Self::Refused(Box::new(refusal))
    }
}

impl ApprovalBook {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(BookInner {
                operator: "unknown@unknown".to_owned(),
                ..BookInner::default()
            }),
        }
    }

    pub(crate) fn with_operator(self, operator: Option<String>) -> Self {
        if let Some(operator) = operator.filter(|s| !s.is_empty()) {
            self.lock().operator = operator;
        }
        self
    }

    /// Judge bound answers after ticket admission; expired answers stay discarded.
    fn bind_answer(
        &self,
        task: &RawTask,
        mode: &str,
        content: &Value,
        hash: &str,
        value: &Value,
    ) -> Gate {
        if let Some(teach) = answer_shape_error(mode, &content["approval"]["choices"], value) {
            let inner = self.lock();
            let step = task.id.value.as_str();
            let mut refused = refusal(
                &inner,
                step,
                mode,
                hash,
                None,
                0,
                "approval.answer_malformed",
                format!(
                    "task '{step}' · approval.answer_malformed — the answer does not fit this gate: {teach} ({APPROVAL_CODE})"
                ),
            );
            refused.attestation.question =
                content["approval"]["message"].as_str().map(str::to_owned);
            return Gate::Refused(Box::new(refused));
        }
        Gate::Run(Box::new(prompt_task_with_default(task, value)))
    }

    /// Recover the plain accumulator maps after a test-harness panic.
    fn lock(&self) -> std::sync::MutexGuard<'_, BookInner> {
        match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Stamp the nonce and precompute each prompt's static closure.
    pub(crate) fn begin_run(&self, wf: &RawWorkflow, nonce: String) {
        let mut inner = self.lock();
        inner.nonce = nonce;
        inner.gated = gated_closures(wf);
    }

    /// Inject folded resume authority (`None` for a fresh run).
    pub(crate) fn set_paused(&self, paused: Option<PausedApproval>) {
        self.lock().paused = paused;
    }

    /// The ticket a step minted this run (the pause payload attaches it).
    pub(crate) fn ticket_for(&self, step: &str) -> Option<ApprovalTicket> {
        self.lock().steps.get(step).map(|e| e.ticket.clone())
    }

    /// One prompt's unleashed closure (empty for a non-prompt step).
    fn gated_for(&self, step: &str) -> Vec<GatedAction> {
        self.lock().gated.get(step).cloned().unwrap_or_default()
    }

    /// Admit or refuse this step under the ticket state machine.
    fn admit(
        &self,
        step: &str,
        mode: &str,
        shown_hash: &str,
        now_ms: i64,
        answer: Option<&Value>,
        source: &'static str,
        question: Option<&str>,
    ) -> Admit {
        let mut inner = self.lock();
        if let Some(verdict) = admit_resumed(
            &mut inner, step, mode, shown_hash, now_ms, answer, source, question,
        ) {
            return verdict;
        }
        admit_live(
            &mut inner, step, mode, shown_hash, now_ms, answer, source, question,
        )
    }

    /// Attest a resolved prompt; blocked and failed prompts attest elsewhere.
    pub(crate) fn attest_outcome(
        &self,
        task: &str,
        settle: &mut crate::task::SettleAs,
        now_ms: i64,
    ) -> Option<ApprovalAttestation> {
        let value = match settle {
            crate::task::SettleAs::Ran(ran) => match &ran.result {
                crate::task::RunResult::Success { value, .. } => value.clone(),
                _ => return None,
            },
            _ => return None,
        };
        let mut inner = self.lock();
        let entry = inner.steps.get(task)?;
        let (ticket, mode, source) = (entry.ticket.clone(), entry.mode.clone(), entry.source);
        let question = entry.question.clone();
        let answer_text = crate::record::render_value(&value);
        let proposed = if mode == "confirm" && matches!(value, Value::Bool(false)) {
            ApprovalDecision::Deny
        } else {
            ApprovalDecision::Allow
        };
        // First terminal wins; a racing terminal cannot rewrite it.
        let decision = if let Some(minted) = inner.minted.get_mut(&ticket.content_hash) {
            if let Some((settled, _)) = &minted.decided {
                *settled
            } else {
                minted.decided = Some((proposed, value));
                minted.ticket.decision = proposed;
                proposed
            }
        } else {
            proposed
        };
        Some(ApprovalAttestation {
            task: task.to_owned(),
            mode,
            decision: decision.as_str(),
            source,
            shown_hash: ticket.content_hash.clone(),
            digest: ticket.digest(),
            run_nonce: ticket.run_nonce.clone(),
            ttl_seconds: ticket.ttl_seconds,
            ttl_remaining_seconds: ticket.ttl_remaining_seconds(now_ms),
            why: None,
            operator: inner.operator.clone(),
            question,
            answer: Some(answer_text),
        })
    }
}

/// Validate a matching paused ticket before binding its answer.
// REASON: the ticket laws + the attribution facts — each param is one
// distinct axis of the same admission (the admit() shape).
#[allow(clippy::too_many_arguments)]
fn admit_resumed(
    inner: &mut BookInner,
    step: &str,
    mode: &str,
    shown_hash: &str,
    now_ms: i64,
    answer: Option<&Value>,
    source: &'static str,
    question: Option<&str>,
) -> Option<Admit> {
    // Clone because the expiry path consumes the paused slot.
    let paused = inner
        .paused
        .as_ref()
        .filter(|p| p.ticket.step == step)
        .cloned();
    let paused = paused?;
    let ticket = paused.ticket.clone();
    let trace_nonce = paused.trace_nonce.clone();
    let answer = answer?;
    if ticket.run_nonce != trace_nonce {
        return Some(Admit::refused(refusal(
            inner,
            step,
            mode,
            shown_hash,
            ticket.digest(),
            0,
            "approval.scope_mismatch",
            format!(
                "task '{step}' · approval.scope_mismatch — the paused ticket names run \
                 `{}` but the resumed trace is `{trace_nonce}`: a cross-run replay is never an \
                 approval ({APPROVAL_CODE})",
                ticket.run_nonce
            ),
        )));
    }
    if ticket.content_hash != shown_hash {
        return Some(Admit::refused(refusal(
            inner,
            step,
            mode,
            shown_hash,
            ticket.digest(),
            ticket.ttl_remaining_seconds(now_ms),
            "approval.content_mismatch",
            format!(
                "task '{step}' · approval.content_mismatch — the resolved content hash \
                 `{shown_hash}` ≠ the shown hash `{}`: the answer signs content the \
                 human was never shown ({APPROVAL_CODE})",
                ticket.content_hash
            ),
        )));
    }
    if ticket.is_expired(now_ms) {
        // Expired authority re-mints without binding the stale answer.
        inner.paused = None;
        return Some(mint(
            inner, step, mode, shown_hash, now_ms, None, source, question,
        ));
    }
    if let Err(error) = paused.consume() {
        let (why, detail) = match error {
            ClaimError::Consumed => (
                "approval.replayed",
                format!(
                    "task '{step}' · approval.replayed — this paused approval was already consumed; a resume answer is single-use ({APPROVAL_CODE})"
                ),
            ),
            ClaimError::Unavailable => (
                "approval.claim_unavailable",
                format!(
                    "task '{step}' · approval.claim_unavailable — the engine could not durably consume this paused approval and refused to run ({APPROVAL_CODE})"
                ),
            ),
        };
        return Some(Admit::refused(refusal(
            inner,
            step,
            mode,
            shown_hash,
            ticket.digest(),
            ticket.ttl_remaining_seconds(now_ms),
            why,
            detail,
        )));
    }
    // This capability was issued by the paused run, so no new mint counts.
    inner.paused = None;
    remember_step(inner, step, ticket, mode, "resume", question);
    Some(Admit::Run {
        bind: Some(answer.clone()),
    })
}

/// Admit a live ticket; decided tickets re-mint, in-flight twins may share.
// REASON: the admit() shape — see admit_resumed.
#[allow(clippy::too_many_arguments)]
fn admit_live(
    inner: &mut BookInner,
    step: &str,
    mode: &str,
    shown_hash: &str,
    now_ms: i64,
    answer: Option<&Value>,
    source: &'static str,
    question: Option<&str>,
) -> Admit {
    let prior = inner
        .minted
        .get(shown_hash)
        .map(|m| (m.ticket.clone(), m.decided.clone()));
    if let Some((ticket, decided)) = prior {
        if decided.is_some() {
            inner.minted.remove(shown_hash);
            return mint(
                inner, step, mode, shown_hash, now_ms, answer, source, question,
            );
        }
        remember_step(inner, step, ticket, mode, source, question);
        return Admit::Run {
            bind: answer.cloned(),
        };
    }
    mint(
        inner, step, mode, shown_hash, now_ms, answer, source, question,
    )
}

/// Mint a ticket or refuse the first mint above the per-run bound.
// REASON: the admit() shape — see admit_resumed.
#[allow(clippy::too_many_arguments)]
fn mint(
    inner: &mut BookInner,
    step: &str,
    mode: &str,
    shown_hash: &str,
    now_ms: i64,
    answer: Option<&Value>,
    source: &'static str,
    question: Option<&str>,
) -> Admit {
    if inner.mints >= APPROVAL_MAX_TICKETS_PER_RUN {
        return Admit::refused(refusal(
            inner,
            step,
            mode,
            shown_hash,
            None,
            0,
            "approval.rate_limited",
            format!(
                "task '{step}' · approval.rate_limited — the run already minted \
                 {APPROVAL_MAX_TICKETS_PER_RUN} distinct approval tickets: the storm dies \
                 here, typed, never queued ({APPROVAL_CODE} · NEP-0013 law 3 · a workflow \
                 that legitimately asks more declares batched gates at the contract)"
            ),
        ));
    }
    inner.mints += 1;
    let ticket = ApprovalTicket::new(
        shown_hash.to_owned(),
        inner.nonce.clone(),
        step.to_owned(),
        now_ms,
        APPROVAL_TTL_SECONDS,
    );
    inner.minted.insert(
        shown_hash.to_owned(),
        MintedApproval {
            ticket: ticket.clone(),
            decided: None,
        },
    );
    remember_step(inner, step, ticket, mode, source, question);
    Admit::Run {
        bind: answer.cloned(),
    }
}

fn remember_step(
    inner: &mut BookInner,
    step: &str,
    ticket: ApprovalTicket,
    mode: &str,
    source: &'static str,
    question: Option<&str>,
) {
    inner.steps.insert(
        step.to_owned(),
        StepEntry {
            ticket,
            mode: mode.to_owned(),
            source,
            question: question.map(str::to_owned),
        },
    );
}

/// Build the deny attestation journaled before task failure.
// REASON: the deny's whole fact surface — the admit() shape.
#[allow(clippy::too_many_arguments)]
fn refusal(
    inner: &BookInner,
    step: &str,
    mode: &str,
    shown_hash: &str,
    digest: Option<String>,
    ttl_remaining_seconds: i64,
    why: &'static str,
    detail: String,
) -> Refusal {
    Refusal {
        detail,
        attestation: ApprovalAttestation {
            task: step.to_owned(),
            mode: mode.to_owned(),
            decision: ApprovalDecision::Deny.as_str(),
            source: "engine",
            shown_hash: shown_hash.to_owned(),
            digest,
            run_nonce: inner.nonce.clone(),
            ttl_seconds: APPROVAL_TTL_SECONDS,
            ttl_remaining_seconds,
            why: Some(why),
            operator: inner.operator.clone(),
            question: None,
            answer: None,
        },
    }
}

/// Is this task a direct `invoke: nika:prompt` (the gate's only subject)?
pub(crate) fn is_prompt_task(task: &RawTask) -> bool {
    matches!(
        &task.action,
        RawAction::Invoke(invoke)
            if invoke.tool().map(|t| t.value.as_str()) == Some("nika:prompt")
    )
}

/// Bind an answer as `default:` after the gate has validated its ticket.
pub(crate) fn prompt_task_with_default(task: &RawTask, answer: &Value) -> RawTask {
    let mut bound = task.clone();
    let RawAction::Invoke(invoke) = &mut bound.action else {
        return bound; // unreachable — the gate only calls for a prompt
    };
    if let Some(args) = invoke.args.as_mut() {
        // Preserve non-object args for the builtin's own validation.
        if let Value::Object(map) = &mut args.value {
            map.insert("default".to_owned(), answer.clone());
        }
    } else {
        // Bind even without args; the builtin still validates required fields.
        let span = task.id.span;
        invoke.args = Some(nika_schema::Spanned::new(
            serde_json::json!({ "default": answer }),
            span,
        ));
    }
    bound
}

/// The task's coarse effect classes, sorted for canonical content.
fn effect_classes_of(task: &RawTask) -> Vec<&'static str> {
    let tool = match &task.action {
        RawAction::Invoke(invoke) => invoke.tool().map(|t| t.value.as_str()),
        _ => None,
    };
    let mut names: Vec<&'static str> = nika_cap::EffectClass::classify(task.action.verb(), tool)
        .iter()
        .map(|c| c.as_str())
        .collect();
    names.sort_unstable();
    names
}

/// Effectful descendants unleashed before another prompt boundary.
fn gated_closures(wf: &RawWorkflow) -> BTreeMap<String, Vec<GatedAction>> {
    let mut downstream: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for task in &wf.tasks {
        for up in crate::resume::referenced_upstreams(&task.value) {
            downstream
                .entry(up)
                .or_default()
                .push(task.value.id.value.clone());
        }
    }
    let mut out = BTreeMap::new();
    for task in &wf.tasks {
        let prompt = &task.value;
        if !is_prompt_task(prompt) {
            continue;
        }
        let mut seen = BTreeSet::new();
        let mut queue = vec![prompt.id.value.clone()];
        let mut gated = Vec::new();
        while let Some(id) = queue.pop() {
            let Some(children) = downstream.get(&id) else {
                continue;
            };
            for next in children.clone() {
                if !seen.insert(next.clone()) {
                    continue;
                }
                let Some(next_task) = wf.tasks.iter().find(|t| t.value.id.value == next) else {
                    continue;
                };
                if is_prompt_task(&next_task.value) {
                    continue; // another gate — it owns its own closure
                }
                let classes = effect_classes_of(&next_task.value);
                if !classes.is_empty() {
                    gated.push(GatedAction {
                        task: next.clone(),
                        classes,
                    });
                }
                queue.push(next);
            }
        }
        gated.sort_by(|a, b| a.task.cmp(&b.task));
        out.insert(prompt.id.value.clone(), gated);
    }
    out
}

/// Canonical shown content over the secret-marker scope (NEP-0013 law 1).
fn canonical_content(task: &RawTask, gated: &[GatedAction], scope: &Scope<'_>) -> (String, Value) {
    let RawAction::Invoke(invoke) = &task.action else {
        // The gate guarantees invoke; keep this helper total regardless.
        return ("confirm".to_owned(), Value::Null);
    };
    let raw = invoke
        .args
        .as_ref()
        .map_or(Value::Null, |a| a.value.clone());
    let rendered = expr::render_json(&raw, scope).unwrap_or(raw);
    let mode = rendered
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("confirm")
        .to_owned();
    let message = rendered
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let choices = rendered
        .get("choices")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let gated_json: Vec<Value> = gated
        .iter()
        .map(|g| json!({ "classes": g.classes, "task": g.task }))
        .collect();
    let content = json!({
        "v": CONTENT_RECIPE_VERSION,
        "approval": {
            "action": { "tool": "nika:prompt", "verb": task.action.verb() },
            "choices": choices,
            "effects": effect_classes_of(task),
            "gated": gated_json,
            "message": message,
            "mode": mode,
            "step": task.id.value,
        },
    });
    (mode, content)
}

impl<S, T, H, P, D, C> crate::Runtime<S, T, H, P, D, C>
where
    C: nika_kernel::clock::ClockDyn + Sync,
{
    /// Unix milliseconds from the injected clock seam.
    pub(crate) fn now_unix_ms(&self) -> i64 {
        self.clock
            .system_now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }

    /// Apply the bounded approval gate before a prompt runs.
    pub(crate) fn approval_gate(
        &self,
        task: &RawTask,
        records: &BTreeMap<String, TaskRecord>,
        inputs: &BTreeMap<String, Value>,
        consts: &BTreeMap<String, Value>,
        resume_ctx: &crate::resume::ResumeContext,
    ) -> Gate {
        if !is_prompt_task(task) {
            return Gate::NotPrompt;
        }
        let step = task.id.value.as_str();
        // Hash the resolved question, but keep secrets as markers.
        let base =
            Scope::workflow_with_value_authorities(records, inputs, consts, resume_ctx.markers());
        let mut with_ns = BTreeMap::new();
        let with_rendered = task.with.iter().all(|(key, value)| {
            match expr::render_json(&value.value, &base) {
                Ok(v) => {
                    with_ns.insert(key.value.clone(), v);
                    true
                }
                // A miss falls back to authored args; the hash still binds.
                Err(_) => false,
            }
        });
        let scope = base.with_task_context(
            if with_rendered { Some(&with_ns) } else { None },
            None,
            None,
            None,
        );
        let gated = self.approvals.gated_for(step);
        let (mode, content) = canonical_content(task, &gated, &scope);
        let question = content["approval"]["message"].as_str().map(str::to_owned);
        let Some(shown_hash) = crate::resume::jcs_blake3_hex(&content) else {
            // The content shape is canonicalizable; still fail closed.
            let inner = self.approvals.lock();
            return Gate::Refused(Box::new(refusal(
                &inner,
                step,
                &mode,
                "",
                None,
                0,
                "approval.content_unhashable",
                format!(
                    "task '{step}' · approval.content_unhashable — the shown content cannot \
                     canonicalize, so it cannot be signed ({APPROVAL_CODE})"
                ),
            )));
        };
        let answer = self.prompt_answers.get(step);
        let source = if answer.is_some() {
            "cli"
        } else if matches!(
            &task.action,
            RawAction::Invoke(invoke)
                if invoke
                    .args
                    .as_ref()
                    .and_then(|args| args.value.get("default"))
                    .is_some()
        ) {
            "policy"
        } else {
            // The generic tool seam cannot prove a human answered.
            "builtin"
        };
        match self.approvals.admit(
            step,
            &mode,
            &shown_hash,
            self.now_unix_ms(),
            answer,
            source,
            question.as_deref(),
        ) {
            Admit::Run { bind } => match bind {
                Some(value) => {
                    self.approvals
                        .bind_answer(task, &mode, &content, &shown_hash, &value)
                }
                None => Gate::Run(Box::new(task.clone())),
            },
            Admit::Refused(r) => Gate::Refused(r),
        }
    }
}

/// The answer-shape law (#1284): what a gate MODE accepts — `None` = it
/// fits (unknown modes stay un-judged; the builtin owns them).
fn answer_shape_error(mode: &str, choices: &Value, answer: &Value) -> Option<&'static str> {
    match mode {
        "confirm" if !answer.is_boolean() => Some("a confirm gate takes true or false"),
        "input" if !answer.is_string() => Some("an input gate takes a string (quote it)"),
        "choice"
            if !answer.is_string()
                || choices
                    .as_array()
                    .is_some_and(|items| !items.contains(answer)) =>
        {
            Some("a choice gate takes one of its declared choices as a string")
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests;
