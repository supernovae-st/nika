// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F-P4 · the human approval is a BOUNDED CAPABILITY (NEP-0013 · the 6th
//! invariant) — the `nika:prompt` gate, ticketed.
//!
//! The gate today is a bare call (the `Prompter` seam in nika-builtin):
//! nothing binds what was SHOWN to what was SIGNED, nothing bounds the
//! prompt storm, nothing attests the decision in the journal. This
//! module wraps the seam — never edits it, the builtin stays a pure
//! call — with the five laws:
//!
//! 1. **Content-bound (WYSIWYS)** — the ticket hashes the CANONICAL
//!    render of the shown content (mode · message · choices over the
//!    secret-marker scope, the pause payload's own discipline) + the
//!    action's identity + the effect classes the yes unleashes (JCS +
//!    blake3 · never an LLM summary). A resumed `--answer` whose
//!    recomputed hash ≠ the shown hash is refused
//!    (`approval.content_mismatch`).
//! 2. **Scope + TTL** — the ticket lives for THIS run (the nonce is the
//!    `workflow_started` event id) × THIS step × THIS hash · TTL
//!    [`APPROVAL_TTL_SECONDS`] · expired = re-prompt · a cross-run
//!    replay is refused (the nonce names another run).
//! 3. **Anti-fatigue** — at most [`APPROVAL_MAX_TICKETS_PER_RUN`]
//!    distinct tickets mint per run; identical content dedups to ONE
//!    ticket (the second ask is attested `dedup`, never re-questioned);
//!    the N+1ᵗʰ distinct mint is the typed refusal
//!    ([`APPROVAL_CODE`] · `approval.rate_limited`) — never a queue.
//! 4. **Attestation** — every decision lands as a hash-chained
//!    `approval_decided` frame (digest · shown-hash · decision · TTL ·
//!    scope) beside the task's terminal frame; a blocking prompt that
//!    pauses the run serializes its mint on the `workflow_paused`
//!    frame, and the resumed run validates the `--answer` against it
//!    BEFORE binding.
//! 5. **Revocation** — before execution only: the pause IS the
//!    pre-execution window (a ticket is revoked by never answering it);
//!    the journal is append-only, nothing rewrites retroactively.
//!
//! The ticket attests what happened — it never promises.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use serde_json::{Value, json};

use crate::expr::{self, Scope};
use crate::record::TaskRecord;

/// NEP-0013 law 2 — the ticket's time-to-live: the fresh-consent window
/// (the sudo timestamp precedent). The named engine constant · v1 fixes
/// it (a `policy:` knob is the named owe if a deployment ever proves it
/// needs another).
pub const APPROVAL_TTL_SECONDS: u32 = 15 * 60;

/// NEP-0013 law 3 — the per-run prompt-storm bound: at most this many
/// DISTINCT tickets mint per run. The next distinct mint is the typed
/// refusal, never a queue.
pub const APPROVAL_MAX_TICKETS_PER_RUN: u32 = 5;

/// The wire code every approval-capability refusal speaks — the
/// `security_error` family, the NEP-0013 row after `NIKA-SEC-009`
/// (trifecta). Catchable by `on_error.on_codes:` like every spec-plane
/// security stop; never transient.
pub const APPROVAL_CODE: &str = "NIKA-SEC-010";

/// The canonical-content recipe version (the resume `KEY_VERSION`
/// precedent): bump on any shape change — older tickets simply mismatch
/// and re-ask, honest, never a wrong bind.
const CONTENT_RECIPE_VERSION: u32 = 1;

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
    /// An identical earlier ticket's decision replayed (the human was
    /// NOT re-questioned · law 3).
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

/// One bounded-approval capability (NEP-0013 · law 1+2). Minted BEFORE
/// the ask, inside the runtime layer — the prompter seam itself stays a
/// pure call and never sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApprovalTicket {
    /// blake3 hex over the JCS canonical shown content (law 1).
    pub content_hash: String,
    /// The run's identity — its `workflow_started` event id (law 2).
    pub run_nonce: String,
    /// The prompt task this ticket was minted at (law 2 · the journal's
    /// `task` on the frames it rides).
    pub step: String,
    /// Mint time — wall-clock unix ms from the run's injected clock seam
    /// (a determinism demand resolves a deterministic clock · F-P3).
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

    /// The ticket's own digest — JCS + blake3 over the MINT fields (the
    /// capability's identity). The decision is excluded on purpose: the
    /// digest names the capability, the `approval_decided` frame names
    /// its use — so the digest the pause showed equals the digest the
    /// answer signs. `None` only if the payload cannot canonicalize
    /// (string/int fields by construction — the honest-absent door,
    /// never a fabricated digest).
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

/// The folded resume authority (NEP-0013 law 1+2) — what the composer
/// reads back from a paused trace: the journaled ticket plus the run
/// identity of the trace it came from (its `workflow_started` event id).
/// The cross-run check is self-contained in the trace bytes: a ticket
/// whose `run_nonce` names another run is a replay, refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PausedApproval {
    /// The ticket the paused run journaled on its `workflow_paused` frame.
    pub ticket: ApprovalTicket,
    /// The run identity of the trace the ticket was folded FROM.
    pub trace_nonce: String,
}

impl PausedApproval {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(ticket: ApprovalTicket, trace_nonce: String) -> Self {
        Self {
            ticket,
            trace_nonce,
        }
    }
}

/// The `approval_decided` payload, assembled at the pipeline and emitted
/// by the settle spine (the pens stay in ONE site — the declassify
/// precedent). `task` on the wire IS the ticket's `step` (the journal
/// joins per-task frames on it; the dossier's scope word is `step`).
pub(crate) struct ApprovalAttestation {
    pub task: String,
    pub mode: String,
    pub decision: &'static str,
    /// WHO answered, honestly scoped to what this layer can know:
    /// `resumed` (a validated paused-ticket `--answer`) · `answer` (a
    /// fresh-run `--answer`) · `dedup` (an in-run replay) · `builtin`
    /// (the builtin resolved it — a TTY answer or an authored `default:`,
    /// the seam cannot split them and never guesses) · `engine` (an
    /// engine refusal).
    pub source: &'static str,
    pub shown_hash: String,
    pub digest: Option<String>,
    pub run_nonce: String,
    pub ttl_seconds: u32,
    pub ttl_remaining_seconds: i64,
    /// The law applied on an engine refusal (`approval.rate_limited` ·
    /// `approval.content_mismatch` · `approval.scope_mismatch`).
    pub why: Option<&'static str>,
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
        fields
    }
}

/// The gate's verdict (pipeline-facing).
pub(crate) enum Gate {
    /// Not a direct `invoke: nika:prompt` — the pipeline runs unchanged.
    NotPrompt,
    /// Run the (possibly answer/dedup-bound) task. Boxed: the enum
    /// stays slim on every path (the `RawTask` is the heavy arm).
    Run(Box<RawTask>),
    /// The capability was refused (`NIKA-SEC-010`) — the task never
    /// starts; the attestation rides the Finish to the settle spine.
    /// Boxed: the happy path stays slim (the `Run` variant's size law).
    Refused(Box<Refusal>),
}

/// A typed approval refusal — the failure detail plus the attestation.
pub(crate) struct Refusal {
    pub detail: String,
    pub attestation: ApprovalAttestation,
}

/// One gated action in a prompt's unleashed closure (the content's
/// `gated` entries).
#[derive(Debug, Clone)]
struct GatedAction {
    task: String,
    classes: Vec<&'static str>,
}

/// The book's per-content record (the dedup state · law 3).
struct MintedApproval {
    ticket: ApprovalTicket,
    decided: Option<(ApprovalDecision, Value)>,
}

/// The book's per-step line (the pause payload + the attestation read it).
struct StepEntry {
    ticket: ApprovalTicket,
    mode: String,
    source: &'static str,
    dedup: bool,
}

#[derive(Default)]
struct BookInner {
    /// THIS run's identity (the `workflow_started` event id).
    nonce: String,
    /// Prompt step → its unleashed closure (computed once per run).
    gated: BTreeMap<String, Vec<GatedAction>>,
    /// Content hash → the minted ticket + its decision (the dedup map).
    minted: BTreeMap<String, MintedApproval>,
    /// Prompt step → its entry (one prompt task mints at most once a run).
    steps: BTreeMap<String, StepEntry>,
    /// The anti-fatigue counter (law 3) — DISTINCT mints this run.
    mints: u32,
    /// The folded resume authority (set by the composer).
    paused: Option<PausedApproval>,
}

/// The per-run approval state. Shared `&self` across the wave's
/// concurrent pipelines — a Mutex over the tiny maps is the whole story
/// (the `RunLedger` precedent: short synchronous critical sections,
/// never held across an await).
pub(crate) struct ApprovalBook {
    inner: Mutex<BookInner>,
}

/// The book's answer to the gate.
enum Admit {
    /// Run — binding this answer (a `--answer` · a dedup replay) when set.
    Run { bind: Option<Value> },
    /// Refuse — the typed detail + the attestation.
    Refused(Refusal),
}

impl ApprovalBook {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(BookInner::default()),
        }
    }

    /// A poisoned lock = a sibling panicked mid-fold (test-harness
    /// class): the maps are plain accumulators with no invariant a
    /// partial write could break — recover and keep the law (the
    /// `RunLedger` idiom, verbatim).
    fn lock(&self) -> std::sync::MutexGuard<'_, BookInner> {
        match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Open the run: stamp the nonce + precompute every prompt's
    /// unleashed closure (static over the workflow bytes — identical on
    /// the paused and the resumed run, which is what makes the shown
    /// hash recomputable at resume).
    pub(crate) fn begin_run(&self, wf: &RawWorkflow, nonce: String) {
        let mut inner = self.lock();
        inner.nonce = nonce;
        inner.gated = gated_closures(wf);
    }

    /// Inject the composer's folded resume authority (ADR-099 rider ·
    /// `None` on a fresh run).
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

    /// The gate's state machine (laws 1–3). `answer` is the operator's
    /// `--answer` for this step when present. Every path either names
    /// the ticket to run under or refuses — never a queue.
    fn admit(
        &self,
        step: &str,
        mode: &str,
        shown_hash: &str,
        now_ms: i64,
        answer: Option<&Value>,
    ) -> Admit {
        let mut inner = self.lock();
        if let Some(verdict) = admit_resumed(&mut inner, step, mode, shown_hash, now_ms, answer) {
            return verdict;
        }
        admit_live(&mut inner, step, mode, shown_hash, now_ms, answer)
    }

    /// Assemble the `approval_decided` payload for a Ran task — `Some`
    /// only when the ask RESOLVED (a success): a blocked prompt carries
    /// its mint on the `workflow_paused` frame instead, and a failed one
    /// attests nothing beyond its `task_failed`. Records the decision
    /// for the run's later dedup (law 3).
    pub(crate) fn attest_outcome(
        &self,
        task: &str,
        settle: &crate::task::SettleAs,
        now_ms: i64,
    ) -> Option<ApprovalAttestation> {
        let crate::task::SettleAs::Ran(ran) = settle else {
            return None;
        };
        let crate::task::RunResult::Success { value, .. } = &ran.result else {
            return None;
        };
        let mut inner = self.lock();
        let entry = inner.steps.get(task)?;
        let (ticket, mode, source, dedup) = (
            entry.ticket.clone(),
            entry.mode.clone(),
            entry.source,
            entry.dedup,
        );
        let decision: &'static str = if dedup {
            ApprovalDecision::Dedup.as_str()
        } else if mode == "confirm" && matches!(value, Value::Bool(false)) {
            ApprovalDecision::Deny.as_str()
        } else {
            ApprovalDecision::Allow.as_str()
        };
        if !dedup && let Some(minted) = inner.minted.get_mut(&ticket.content_hash) {
            minted.decided = Some((
                if decision == ApprovalDecision::Deny.as_str() {
                    ApprovalDecision::Deny
                } else {
                    ApprovalDecision::Allow
                },
                value.clone(),
            ));
            minted.ticket.decision = if decision == ApprovalDecision::Deny.as_str() {
                ApprovalDecision::Deny
            } else {
                ApprovalDecision::Allow
            };
        }
        Some(ApprovalAttestation {
            task: task.to_owned(),
            mode,
            decision,
            source,
            shown_hash: ticket.content_hash.clone(),
            digest: ticket.digest(),
            run_nonce: ticket.run_nonce.clone(),
            ttl_seconds: ticket.ttl_seconds,
            ttl_remaining_seconds: ticket.ttl_remaining_seconds(now_ms),
            why: None,
        })
    }
}

/// Law 1+2 — the resumed ticket validates BEFORE anything binds.
/// `Some(verdict)` when a paused ticket covers this answered step
/// (refused · re-minted · or bound against the SHOWN ticket); `None`
/// hands the step to the live state machine. The caller holds the lock.
fn admit_resumed(
    inner: &mut BookInner,
    step: &str,
    mode: &str,
    shown_hash: &str,
    now_ms: i64,
    answer: Option<&Value>,
) -> Option<Admit> {
    // (cloned up front — the expiry path consumes the slot, so the
    // borrow cannot live across the state machine.)
    let paused = inner
        .paused
        .as_ref()
        .filter(|p| p.ticket.step == step)
        .map(|p| (p.ticket.clone(), p.trace_nonce.clone()));
    let (ticket, trace_nonce) = paused?;
    let answer = answer?;
    if ticket.run_nonce != trace_nonce {
        return Some(Admit::Refused(refusal(
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
        return Some(Admit::Refused(refusal(
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
        // Law 2 — expired = re-prompt: the stale answer does NOT bind;
        // the authority is consumed and a fresh ticket mints (a
        // non-interactive surface pauses again).
        inner.paused = None;
        return Some(mint(inner, step, mode, shown_hash, now_ms, None));
    }
    // Valid — the answer binds against the SHOWN ticket. No new mint,
    // no count: the capability was issued by the paused run.
    inner.steps.insert(
        step.to_owned(),
        StepEntry {
            ticket,
            mode: mode.to_owned(),
            source: "resumed",
            dedup: false,
        },
    );
    Some(Admit::Run {
        bind: Some(answer.clone()),
    })
}

/// The live state machine (no resumed ticket in play): dedup a DECIDED
/// ticket for the same content (law 3 — the human is never re-questioned
/// inside the TTL), re-mint a stale one, share an in-flight twin's, or
/// mint a new distinct content. The caller holds the lock.
fn admit_live(
    inner: &mut BookInner,
    step: &str,
    mode: &str,
    shown_hash: &str,
    now_ms: i64,
    answer: Option<&Value>,
) -> Admit {
    let prior = inner.minted.get(shown_hash).map(|m| {
        (
            m.ticket.clone(),
            m.decided.clone(),
            m.ticket.is_expired(now_ms),
        )
    });
    if let Some((ticket, decided, expired)) = prior {
        match decided {
            Some((_decision, value)) if !expired => {
                inner.steps.insert(
                    step.to_owned(),
                    StepEntry {
                        ticket,
                        mode: mode.to_owned(),
                        source: "dedup",
                        dedup: true,
                    },
                );
                return Admit::Run { bind: Some(value) };
            }
            // A decided-but-stale ticket re-mints (fresh consent) · an
            // undecided one is an in-flight twin (same-wave fan-out):
            // same ticket, no new mint, no count.
            Some(_) => {
                inner.minted.remove(shown_hash);
                return mint(inner, step, mode, shown_hash, now_ms, answer);
            }
            None => {
                inner.steps.insert(
                    step.to_owned(),
                    StepEntry {
                        ticket,
                        mode: mode.to_owned(),
                        source: if answer.is_some() {
                            "answer"
                        } else {
                            "builtin"
                        },
                        dedup: false,
                    },
                );
                return Admit::Run {
                    bind: answer.cloned(),
                };
            }
        }
    }
    mint(inner, step, mode, shown_hash, now_ms, answer)
}

/// Mint a fresh ticket for a new distinct content — the N+1ᵗʰ distinct
/// mint of the run is the typed refusal (law 3 · never a queue). The
/// caller holds the lock.
fn mint(
    inner: &mut BookInner,
    step: &str,
    mode: &str,
    shown_hash: &str,
    now_ms: i64,
    answer: Option<&Value>,
) -> Admit {
    if inner.mints >= APPROVAL_MAX_TICKETS_PER_RUN {
        return Admit::Refused(refusal(
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
    inner.steps.insert(
        step.to_owned(),
        StepEntry {
            ticket,
            mode: mode.to_owned(),
            source: if answer.is_some() {
                "answer"
            } else {
                "builtin"
            },
            dedup: false,
        },
    );
    Admit::Run {
        bind: answer.cloned(),
    }
}

/// Build a refusal with its attestation (the deny event the settle
/// spine journals before the task's failure frame).
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

/// Bind an answer to a `nika:prompt` task as its `default:` (the
/// answered branch of the stdlib contract — the builtin validates the
/// TYPE per mode, so a bad answer fails with the same honest
/// PROMPT-001/002 diagnostics). The gate owns every CALL (it validates
/// the ticket BEFORE binding · NEP-0013); this is the mechanical
/// binder — dispatch-only, never the resume identity.
pub(crate) fn prompt_task_with_default(task: &RawTask, answer: &Value) -> RawTask {
    let mut bound = task.clone();
    let RawAction::Invoke(invoke) = &mut bound.action else {
        return bound; // unreachable — the gate only calls for a prompt
    };
    if let Some(args) = invoke.args.as_mut() {
        // Non-object args fail the builtin's own validation — never
        // silently rewritten here.
        if let Value::Object(map) = &mut args.value {
            map.insert("default".to_owned(), answer.clone());
        }
    } else {
        // No args at all (message missing → the builtin refuses
        // loudly anyway) — still bind, one behavior.
        let span = task.id.span;
        invoke.args = Some(nika_schema::Spanned::new(
            serde_json::json!({ "default": answer }),
            span,
        ));
    }
    bound
}

/// The task's coarse effect classes, sorted by name (the canonical
/// content's `effects` · nika-cap's policy projection — one vocabulary
/// for the shown content and the check's batch rule).
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

/// Every prompt step's unleashed closure: the descendants its yes
/// unleashes BEFORE any other human question, with their effect classes.
/// The walk never traverses THROUGH another `nika:prompt` — the nearest
/// gate owns what it re-asks for (the check's batch rule reads the same
/// closure law).
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

/// The canonical shown content (NEP-0013 law 1): the rendered mode ·
/// message · choices over the SECRET-MARKER scope (the pause payload's
/// discipline — a resolved secret value never enters the hash, a render
/// miss falls back to the raw authored text), the action's identity,
/// its effect classes, and the unleashed closure. JCS + blake3 — never
/// a summary, never a float (the recipe is strings/arrays/ints only).
fn canonical_content(task: &RawTask, gated: &[GatedAction], scope: &Scope<'_>) -> (String, Value) {
    let RawAction::Invoke(invoke) = &task.action else {
        // The gate guards this (is_prompt_task) — a non-invoke never
        // reaches here; the fallback keeps the function total.
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
        },
    });
    (mode, content)
}

impl<S, T, H, P, D, C> crate::Runtime<S, T, H, P, D, C>
where
    C: nika_kernel::clock::ClockDyn + Sync,
{
    /// The wall-clock unix ms from the run's injected clock seam (never
    /// a direct `SystemTime` read — a determinism demand resolves a
    /// deterministic clock · F-P3).
    pub(crate) fn now_unix_ms(&self) -> i64 {
        self.clock
            .system_now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }

    /// The F-P4 gate, applied to every task BEFORE it runs: a direct
    /// `invoke: nika:prompt` mints its ticket first (rate-limit + dedup
    /// enforced · the resumed `--answer` validated against the shown
    /// hash), everything else passes through untouched.
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
        // The content renders over the SECRET-MARKER scope — the pause
        // payload's own discipline (never a resolved secret value in the
        // hash · markers only — a low-entropy secret inside a hash is an
        // oracle, the resume-identity law). The task's `with:` namespace
        // materializes over the SAME marker scope (upstream data reaches
        // a prompt only through `with:` · NIKA-VAR-021), so the hash
        // binds the RESOLVED question the human actually read.
        let base = Scope {
            records,
            inputs,
            consts,
            secrets: resume_ctx.markers(),
            with_ns: None,
            item: None,
            index: None,
            permits: None,
        };
        let mut with_ns = BTreeMap::new();
        let with_rendered = task.with.iter().all(|(key, value)| {
            match expr::render_json(&value.value, &base) {
                Ok(v) => {
                    with_ns.insert(key.value.clone(), v);
                    true
                }
                // A `with:` miss degrades to the raw authored args (the
                // pause payload's fallback) — the hash still binds.
                Err(_) => false,
            }
        });
        let scope = Scope {
            with_ns: if with_rendered { Some(&with_ns) } else { None },
            ..base
        };
        let gated = self.approvals.gated_for(step);
        let (mode, content) = canonical_content(task, &gated, &scope);
        let Some(shown_hash) = crate::resume::jcs_blake3_hex(&content) else {
            // Strings/arrays/ints by construction — unreachable in
            // practice; the law stays fail-closed anyway: an unhashable
            // ask is never an unbounded one.
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
        match self
            .approvals
            .admit(step, &mode, &shown_hash, self.now_unix_ms(), answer)
        {
            Admit::Run { bind } => match bind {
                Some(value) => Gate::Run(Box::new(prompt_task_with_default(task, &value))),
                None => Gate::Run(Box::new(task.clone())),
            },
            Admit::Refused(r) => Gate::Refused(Box::new(r)),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests;
