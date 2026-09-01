// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The firing machine (W7 · D4) — ONE slot's lifecycle as a pure state
//! machine: zero IO, no clock read, and no bare `String`/`u64` at the
//! boundary (the newtypes carry them). The firer (L4) classifies
//! through it, the report folds with it, the sweep (W8) will drive it.
//!
//! ## The identities
//!
//! - [`SlotId`] — the slot's canonical identity: sha256 over the
//!   domain-separated `nika/arm-slot@1` string (workflow path VERBATIM ·
//!   cadence VERBATIM · the slot's instant as UTC RFC 3339). Relocated
//!   from the CLI's ledger in W7 — a relocation, never a change: the
//!   known-vector test hashes the bytes itself.
//! - [`FencingToken`] — the claim's seq (Kleppmann): a receipt settles
//!   the claim by naming it.
//! - [`ArmGeneration`] — `sha256("nika/arm-gen@2\n" + declaration +
//!   "\0" + snapshot_digest)` (F17): every firing PINS its generation —
//!   an update never changes a run in progress. ONE law, ONE domain,
//!   both firing edges. The declaration is the edge's own identity: the
//!   CLI arm-fire hashes the beat's DECLARED fields in the struct's
//!   fixed order (workflow · cadence · où · plafond · manqué ·
//!   chevauchement · `après_saut` · actif · raison · `jusqu_au` ·
//!   tolérance · décalage · par), one `key=value` line each — strings
//!   quoted, the absent `null`, floats in shortest roundtrip (`{f64:?}`
//!   — deterministic for a given toolchain; an engine upgrade that
//!   moved the formatting would read as a new generation, the cautious
//!   direction) — the resident serve hashes its `ScheduleRevision`. The
//!   positional label NEVER enters the hash; the two refused keys
//!   (`signature:` · `budget:`) carry no content. The source half is
//!   the admitted world's FULL snapshot digest (root + children +
//!   skills + imports): editing ANY world byte between two fires mints
//!   a new generation. `@1` hashed the root workflow's bytes alone;
//!   those values remain interpretable as historical ledger evidence.
//!
//! ## The transition table (this module's law)
//!
//! `FROM --EVENT--> TO · trigger · precondition · durable effect ·
//! crash behavior`. Every pair not listed is OFF THE TABLE: identity,
//! and [`decide`] answers `[Ignore]`.
//!
//! - `Planned --Due--> Due` · the planner judged the slot inside its
//!   window · none · no durable effect (the verdict is in-memory) · a
//!   crash loses the tick, the next one re-judges (N2 invents no
//!   backlog).
//! - `Planned | Due | Claimed | FailedRetryable | Ambiguous
//!   --Claimed--> Claimed` · the firer appended the durable claim · the
//!   beat lock held (the one-firer law) · the `claimed` line + fsync
//!   BEFORE any run · none possible: the event IS the durable fact, a
//!   crash before it leaves no claim and no run. A re-claim REBINDS the
//!   token (the newer attempt owns the lifecycle).
//! - `Claimed --Started--> Running` · the run actually started · a
//!   claim is outstanding · nothing journaled in v0 · the claim
//!   outlives the crash — the deadline detects it.
//! - `Planned | Due | Claimed | Running | Ambiguous --Finished(code)-->`
//!   · `Succeeded` (0) · `Cancelled` (4 — the human gate's park,
//!   never resumed) · `FailedRetryable` (any other) · the run exited ·
//!   the fencing matches the outstanding claim, OR no claim is
//!   outstanding (a bare receipt: a W2-era line · a direct record · a
//!   claim lost to a chain cut) · the receipt line + fsync, settling
//!   the claim by fencing · claim without receipt = the orphan the
//!   deadline surfaces.
//! - `FailedRetryable --AttemptsExhausted--> FailedPermanent` · the
//!   policy counted the last attempt (v0: always — one attempt) · a
//!   failure is settled · none (derivable from receipt + policy) · the
//!   receipt stands, the exhaustion is re-derivable.
//! - `Planned | Due --Skipped--> Skipped` · a policy said no · none ·
//!   the `skipped` line (a skip consumes the slot it bears) · the skip
//!   is re-decided at the next tick.
//! - `Due --Deferred--> Deferred` · the cost ceiling reached (W8 —
//!   never emitted in v0) · a due slot · journaled WITHOUT consuming an
//!   attempt (the reject/nack law) · nothing was consumed.
//! - `Planned | Due | Claimed | Running --Cancelled--> Cancelled` ·
//!   the operator's cancellation (v0: the gate's park arrives as
//!   `Finished(4)`) · the fencing pairing as `Finished` · the receipt
//!   line (kind `paused`) · a parked run is NEVER resumed (N2).
//! - `Claimed | Running --DeadlinePassed--> Ambiguous` · the injected
//!   now passed the claim's deadline, no receipt · a claim outstanding
//!   · none — the orphan is VISIBLE · n/a (the detection is a read).
//! - `Ambiguous --Rescued--> Due` · the sweep re-arms the orphan (W8) ·
//!   the rescue budget stands · the rescue journaled by identity · the
//!   deadline re-detects.
//! - `Ambiguous --Poisoned--> DeadLettered` · the poison threshold
//!   (W8) · `Ambiguous` · the dead letter is durable · it stands.
//! - `Ambiguous --Finished--> the terminal` · the late receipt resolves
//!   the ambiguity — the run HAD happened.
//! - The terminals (`Succeeded · FailedPermanent · Deferred · Skipped
//!   · Cancelled · DeadLettered`) absorb every event: identity. A
//!   settled lifecycle never moves again.
//!
//! [`fold`] reduces an event stream through the table with the fencing
//! pairing applied (a receipt naming ANOTHER token settles nothing);
//! [`transition`] is the pure `(state, event)` half (context-free — the
//! pairing is the caller's); [`decide`] pairs each transition with its
//! durable effects, `now` riding the journal effects (the ledger's
//! `ts`) and the policy gating the `Fire`.

use std::borrow::Cow;

use jiff::{Timestamp, Zoned};

use crate::registry::{AfterSkip, Beat, Locus, MissPolicy, Overlap};
use crate::schedule::ScheduleRevision;

/// The slot's canonical identity (64 lowercase hex) — the dedup unit.
/// NEVER the positional label: a `-2`/`-3` permutes at insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotId(String);

impl SlotId {
    /// Derive the identity: the domain-separated canonical string,
    /// hashed — derivable before any write, stable across restarts.
    #[must_use]
    pub fn derive(workflow: &str, cadence: &str, slot: &Zoned) -> Self {
        Self(sha256_hex(
            format!(
                "nika/arm-slot@1\n{workflow}\n{cadence}\n{}",
                slot.timestamp()
            )
            .as_bytes(),
        ))
    }

    /// Read one off the wire — 64 lowercase hex, nothing else.
    #[must_use]
    pub fn from_wire(raw: &str) -> Option<Self> {
        (raw.len() == 64
            && raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then(|| Self(raw.to_owned()))
    }

    /// The full hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The report's 8-char form (both constructors gate the length).
    #[must_use]
    pub fn short(&self) -> &str {
        &self.0[..8]
    }
}

/// The fencing token — the claim's own seq (Kleppmann): the receipt
/// settles the claim by naming it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FencingToken(u64);

impl FencingToken {
    /// Wrap the claim's seq.
    #[must_use]
    pub const fn new(seq: u64) -> Self {
        Self(seq)
    }

    /// The wrapped seq.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The generation a firing PINS (F17 — 64 lowercase hex): the edge's
/// declaration identity + the admitted world's full snapshot digest.
/// An update to either mints a new generation; an in-flight run keeps
/// its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmGeneration(String);

/// The ONE generation preimage domain, both firing edges. Bumped `@1` →
/// `@2` when the source half stopped being the root workflow's bytes
/// alone and became the admitted world's full snapshot digest — `@1`
/// values stay interpretable as historical ledger evidence.
const GENERATION_DOMAIN: &[u8] = b"nika/arm-gen@2\n";

impl ArmGeneration {
    /// The ONE law (see the module doc): the domain, the edge's
    /// declaration identity, a NUL, the full snapshot digest. Exact
    /// cross-edge equality is impossible BY DESIGN — the CLI declares a
    /// beat, the resident serve declares a schedule revision — so both
    /// named constructors judge through this one helper.
    #[must_use]
    fn pin(declaration: &str, snapshot_digest: &str) -> Self {
        let mut preimage = Vec::with_capacity(
            GENERATION_DOMAIN.len() + declaration.len() + 1 + snapshot_digest.len(),
        );
        preimage.extend_from_slice(GENERATION_DOMAIN);
        preimage.extend_from_slice(declaration.as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(snapshot_digest.as_bytes());
        Self(sha256_hex(&preimage))
    }

    /// The CLI arm-fire edge: the declaration is the beat's canonical
    /// form — the DECLARED fields in the struct's fixed order (see the
    /// module doc). The source half is the admitted world's digest.
    #[must_use]
    pub fn compute(beat: &Beat, snapshot_digest: &str) -> Self {
        Self::pin(&canonical_beat(beat), snapshot_digest)
    }

    /// The resident-serve edge: the declaration is the schedule's
    /// revision. Same domain, same preimage — the one law.
    #[must_use]
    pub fn compute_resident(revision: &ScheduleRevision, snapshot_digest: &str) -> Self {
        Self::pin(revision.as_str(), snapshot_digest)
    }

    /// Read one off the wire — 64 lowercase hex, nothing else.
    #[must_use]
    pub fn from_wire(raw: &str) -> Option<Self> {
        (raw.len() == 64
            && raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then(|| Self(raw.to_owned()))
    }

    /// The full hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The report's 8-char form.
    #[must_use]
    pub fn short(&self) -> &str {
        &self.0[..8]
    }
}

/// The twelve firing states (the lifecycle's whole vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FiringState {
    /// On the calendar, never judged due.
    Planned,
    /// Inside the on-time window (or owed by the miss policy).
    Due,
    /// The durable claim landed (+ fsync) — no run yet.
    Claimed,
    /// The run started.
    Running,
    /// The run exited 0.
    Succeeded,
    /// The run failed — attempts remain (the exhaustion verdict is a
    /// separate event the policy sends).
    FailedRetryable,
    /// The run failed and no attempt remains.
    FailedPermanent,
    /// The cost ceiling deferred the slot — NOT a consumed attempt.
    Deferred,
    /// A policy said no (the slot is consumed when the skip bears one).
    Skipped,
    /// Cancelled — v0: the human gate's park (never resumed, N2).
    Cancelled,
    /// The poison threshold parked the beat (W8).
    DeadLettered,
    /// Claimed, the deadline passed, no receipt: the run MAY have
    /// happened — the at-least-once honesty.
    Ambiguous,
}

impl FiringState {
    /// Every state, in the vocabulary's frozen order (the tests walk it).
    pub const ALL: [Self; 12] = [
        Self::Planned,
        Self::Due,
        Self::Claimed,
        Self::Running,
        Self::Succeeded,
        Self::FailedRetryable,
        Self::FailedPermanent,
        Self::Deferred,
        Self::Skipped,
        Self::Cancelled,
        Self::DeadLettered,
        Self::Ambiguous,
    ];

    /// The wire word (the report prints it).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Due => "due",
            Self::Claimed => "claimed",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::FailedRetryable => "failed-retryable",
            Self::FailedPermanent => "failed-permanent",
            Self::Deferred => "deferred",
            Self::Skipped => "skipped",
            Self::Cancelled => "cancelled",
            Self::DeadLettered => "dead-lettered",
            Self::Ambiguous => "ambiguous",
        }
    }
}

/// The skip's machine reason — the firer's tokens, typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    /// `actif: false` — the declared suspension.
    Inactive,
    /// `où: cloud` — the cloud executes, the calendar stays.
    Cloud,
    /// `jusqu_au` passed.
    Expired,
    /// `on-webhook` — fires on its event, never on the clock.
    Webhook,
    /// `missed:n` — the silence's count.
    Missed(u32),
    /// `chevauchement: sauter` — a live tick holds the slot.
    Overlap,
    /// `chevauchement: file` — the queue's budget ran out.
    OverlapTimeout,
    /// `serve` stopped mid-wait.
    ServeStop,
    /// The slot was already DECIDED — a duplicate tick.
    Already,
    /// Outside the window, no state — N2 invents no backlog.
    NotDue,
}

impl SkipReason {
    /// The wire token.
    #[must_use]
    pub fn as_str(&self) -> Cow<'static, str> {
        match self {
            Self::Inactive => Cow::Borrowed("inactive"),
            Self::Cloud => Cow::Borrowed("cloud"),
            Self::Expired => Cow::Borrowed("expired"),
            Self::Webhook => Cow::Borrowed("webhook"),
            Self::Missed(n) => Cow::Owned(format!("missed:{n}")),
            Self::Overlap => Cow::Borrowed("overlap"),
            Self::OverlapTimeout => Cow::Borrowed("overlap-timeout"),
            Self::ServeStop => Cow::Borrowed("serve-stop"),
            Self::Already => Cow::Borrowed("already"),
            Self::NotDue => Cow::Borrowed("not-due"),
        }
    }

    /// Read one back — an unknown token is `None` (a reason this
    /// machine predates), never a guess.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "inactive" => Some(Self::Inactive),
            "cloud" => Some(Self::Cloud),
            "expired" => Some(Self::Expired),
            "webhook" => Some(Self::Webhook),
            "overlap" => Some(Self::Overlap),
            "overlap-timeout" => Some(Self::OverlapTimeout),
            "serve-stop" => Some(Self::ServeStop),
            "already" => Some(Self::Already),
            "not-due" => Some(Self::NotDue),
            _ => raw
                .strip_prefix("missed:")
                .and_then(|n| n.parse::<u32>().ok())
                .map(Self::Missed),
        }
    }
}

/// One lifecycle event — typed, never a `String` at the boundary. The
/// slot's identity is the CALLER's context (the ledger walk groups one
/// lifecycle at a time); the events carry what the transition needs.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FiringEvent {
    /// The planner judged the slot due (in-memory — never journaled).
    Due,
    /// The durable claim landed (+ fsync) BEFORE the run.
    Claimed {
        /// The claim's seq — its own fencing token.
        fencing: FencingToken,
        /// The pinned generation — `None` when the workflow bytes were
        /// unreadable at claim time (the run then fails its receipt).
        generation: Option<ArmGeneration>,
        /// The crash-detector deadline (the beat's next slot).
        deadline: Timestamp,
    },
    /// The run started (in-memory in v0 — never journaled).
    Started {
        /// The claim this start belongs to.
        fencing: FencingToken,
    },
    /// The run exited with its code (0 · 1|2|3 · 4). `None` fencing =
    /// a bare receipt (W2-era · direct record · a claim lost to a cut).
    Finished {
        /// The claim this receipt settles, when it names one.
        fencing: Option<FencingToken>,
        /// The run's exit code.
        code: u8,
    },
    /// The policy counted the last attempt (v0: always — one attempt).
    AttemptsExhausted {
        /// The settled receipt's token.
        fencing: FencingToken,
    },
    /// A policy said no. `reason` is `None` for a token this machine
    /// predates (a W2 line carries the word, not always a known one).
    Skipped {
        /// The machine token, when known.
        reason: Option<SkipReason>,
    },
    /// The cost ceiling defers the slot (W8 — never emitted in v0).
    Deferred,
    /// The operator's cancellation.
    Cancelled {
        /// The claim this cancels, when it names one.
        fencing: Option<FencingToken>,
    },
    /// The claim's deadline passed with no receipt — the crash detector.
    DeadlinePassed {
        /// The expired claim's token.
        fencing: FencingToken,
    },
    /// The sweep re-armed the orphan (W8).
    Rescued {
        /// The orphaned claim's token.
        fencing: FencingToken,
    },
    /// The poison threshold hit (W8) — the beat dead-letters.
    Poisoned {
        /// The orphaned claim's token.
        fencing: FencingToken,
    },
}

/// The firing policy the decisions consult. v0 carries the ONE knob
/// (`max_attempts: 1` — no retry); W8 turns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiringPolicy {
    /// How many attempts a slot gets (1 in v0 — the retry is W8's).
    pub max_attempts: u32,
}

impl FiringPolicy {
    /// The v0 policy: one attempt.
    #[must_use]
    pub const fn single() -> Self {
        Self { max_attempts: 1 }
    }
}

/// What [`decide`] hands back: the transition plus the durable effects
/// the runtime owes, in order. `now` rides the journal effects — the
/// ledger's `ts` is the decision instant.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Decision {
    /// Transition to this state.
    Become(FiringState),
    /// Append the claim BEFORE anything runs (the order law).
    JournalClaim {
        /// The decision instant.
        at: Timestamp,
    },
    /// Append the receipt settling the claim.
    JournalReceipt {
        /// The terminal state the receipt records.
        state: FiringState,
        /// The decision instant.
        at: Timestamp,
    },
    /// Append the skip (a skip consumes the slot it bears).
    JournalSkip {
        /// The decision instant.
        at: Timestamp,
    },
    /// Append the defer — NOT a consumed attempt.
    JournalDefer {
        /// The decision instant.
        at: Timestamp,
    },
    /// The slot is cleared to fire.
    Fire,
    /// A claim outlived its deadline — the orphan is VISIBLE.
    SurfaceOrphan,
    /// Re-arm the orphaned slot (the sweep's act — W8).
    Rearm,
    /// The poison verdict — the beat parks in the dead letters (W8).
    DeadLetter,
    /// Off the table for this state — a lawful no-op.
    Ignore,
}

/// The pure `(state, event)` half — the transition table made code.
/// Context-free: the fencing pairing is the caller's ([`fold`] applies
/// it). Off the table ⇒ the state itself.
#[must_use]
pub fn transition(from: FiringState, event: &FiringEvent) -> FiringState {
    use FiringState as S;
    match (from, event) {
        (S::Planned | S::Due, FiringEvent::Due) | (S::Ambiguous, FiringEvent::Rescued { .. }) => {
            S::Due
        }
        (
            S::Planned | S::Due | S::Claimed | S::FailedRetryable | S::Ambiguous,
            FiringEvent::Claimed { .. },
        ) => S::Claimed,
        (S::Claimed, FiringEvent::Started { .. }) => S::Running,
        (
            S::Planned | S::Due | S::Claimed | S::Running | S::Ambiguous,
            FiringEvent::Finished { code, .. },
        ) => terminal_of(*code),
        (S::FailedRetryable, FiringEvent::AttemptsExhausted { .. }) => S::FailedPermanent,
        (S::Planned | S::Due, FiringEvent::Skipped { .. }) => S::Skipped,
        (S::Due, FiringEvent::Deferred) => S::Deferred,
        (S::Planned | S::Due | S::Claimed | S::Running, FiringEvent::Cancelled { .. }) => {
            S::Cancelled
        }
        (S::Claimed | S::Running, FiringEvent::DeadlinePassed { .. }) => S::Ambiguous,
        (S::Ambiguous, FiringEvent::Poisoned { .. }) => S::DeadLettered,
        _ => from,
    }
}

/// The receipt's code classifies the terminal (see the table): 0
/// succeeds, 4 parks (never resumed), anything else fails retryable —
/// the exhaustion verdict is the policy's own event.
fn terminal_of(code: u8) -> FiringState {
    match code {
        0 => FiringState::Succeeded,
        4 => FiringState::Cancelled,
        _ => FiringState::FailedRetryable,
    }
}

/// Fold one slot's event stream to its current state — the pairing
/// applied: an event naming a token this lifecycle does not hold
/// settles nothing (a receipt for ANOTHER claim, a deadline for a
/// claim we never saw). Never panics; the proptest holds every step
/// to the table's independent reading.
#[must_use]
pub fn fold(events: &[FiringEvent]) -> FiringState {
    let mut machine = FiringFold {
        state: FiringState::Planned,
        outstanding: None,
        settled: None,
    };
    for event in events {
        machine.apply(event);
    }
    machine.state
}

/// The fold's context: the live claim's token (`outstanding`) and the
/// last settled receipt's (`settled`).
struct FiringFold {
    state: FiringState,
    outstanding: Option<FencingToken>,
    /// A receipt has settled (`Some`), carrying its fence when the
    /// wire had one. `None` means no receipt at all; `Some(None)` is a
    /// W2/bare receipt and may receive the v0 exhaustion verdict.
    settled: Option<Settled>,
}

#[derive(Clone, Copy)]
enum Settled {
    Bare,
    Fenced(FencingToken),
}

/// Is the pair on the table? The two self-loops ARE rows of the table
/// (a due re-judged · a re-claim that rebinds the token), never
/// identities — this predicate, not `next != from`, is the check.
fn on_table(from: FiringState, event: &FiringEvent) -> bool {
    use FiringState as S;
    transition(from, event) != from
        || matches!(
            (from, event),
            (
                S::Planned | S::Due | S::Claimed | S::FailedRetryable | S::Ambiguous,
                FiringEvent::Claimed { .. }
            ) | (S::Planned | S::Due, FiringEvent::Due)
        )
}

impl FiringFold {
    fn apply(&mut self, event: &FiringEvent) {
        if !self.pairs(event) {
            return; // a foreign token — never this lifecycle's
        }
        if !on_table(self.state, event) {
            return; // off the table — nothing rebinds
        }
        let next = transition(self.state, event);
        match event {
            FiringEvent::Claimed { fencing, .. } => {
                self.outstanding = Some(*fencing);
                self.settled = None;
            }
            FiringEvent::Finished { fencing, .. } | FiringEvent::Cancelled { fencing } => {
                self.settled = Some(fencing.map_or(Settled::Bare, Settled::Fenced));
            }
            _ => {}
        }
        self.state = next;
    }

    /// The fencing pairing: does this event name a token this
    /// lifecycle holds? A `None` token (a bare receipt) always pairs;
    /// a named token pairs with no outstanding claim (its claim was
    /// lost to a chain cut) but never with a DIFFERENT one.
    fn pairs(&self, event: &FiringEvent) -> bool {
        match event {
            FiringEvent::Started { fencing }
            | FiringEvent::DeadlinePassed { fencing }
            | FiringEvent::Rescued { fencing }
            | FiringEvent::Poisoned { fencing } => self.outstanding == Some(*fencing),
            FiringEvent::Finished { fencing, .. } | FiringEvent::Cancelled { fencing } => {
                fencing.is_none_or(|f| self.outstanding.is_none_or(|o| o == f))
            }
            FiringEvent::AttemptsExhausted { fencing } => match self.settled {
                Some(Settled::Bare) => true,
                Some(Settled::Fenced(token)) => token == *fencing,
                None => false,
            },
            _ => true,
        }
    }
}

/// The policy door: the transition plus the durable effects the
/// runtime owes (W8's sweep drives this; v0 classifies through
/// [`fold`]). Fencing-blind by design — the caller pairs the event to
/// its lifecycle before asking. Off the table ⇒ `[Ignore]`.
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub fn decide(
    state: FiringState,
    event: &FiringEvent,
    now: &Timestamp,
    policy: &FiringPolicy,
) -> impl Iterator<Item = Decision> {
    let next = transition(state, event);
    if !on_table(state, event) {
        return [Some(Decision::Ignore), None, None].into_iter().flatten();
    }
    let (second, third) = match event {
        FiringEvent::Due => ((policy.max_attempts > 0).then_some(Decision::Fire), None),
        FiringEvent::Claimed { .. } => (
            Some(Decision::JournalClaim { at: *now }),
            (policy.max_attempts > 0).then_some(Decision::Fire),
        ),
        FiringEvent::Finished { .. } | FiringEvent::Cancelled { .. } => (
            Some(Decision::JournalReceipt {
                state: next,
                at: *now,
            }),
            None,
        ),
        FiringEvent::Skipped { .. } => (Some(Decision::JournalSkip { at: *now }), None),
        FiringEvent::Deferred => (Some(Decision::JournalDefer { at: *now }), None),
        FiringEvent::DeadlinePassed { .. } => (Some(Decision::SurfaceOrphan), None),
        FiringEvent::Rescued { .. } => (Some(Decision::Rearm), None),
        FiringEvent::Poisoned { .. } => (Some(Decision::DeadLetter), None),
        FiringEvent::Started { .. } | FiringEvent::AttemptsExhausted { .. } => (None, None),
    };
    [Some(Decision::Become(next)), second, third]
        .into_iter()
        .flatten()
}

/// The beat's canonical form: the DECLARED fields, the struct's fixed
/// order, one `key=value` line each (see the module doc). The two
/// refused keys carry no content; the label never enters.
fn canonical_beat(beat: &Beat) -> String {
    let null = |v: Option<String>| v.unwrap_or_else(|| "null".to_owned());
    let fields = [
        ("workflow", quoted(&beat.workflow)),
        ("cadence", quoted(&beat.cadence)),
        ("où", null(beat.ou.map(|l| quoted(locus_word(l))))),
        ("plafond", null(beat.plafond.map(|p| format!("{p:?}")))),
        ("manqué", null(beat.manque.map(|m| quoted(miss_word(m))))),
        (
            "chevauchement",
            null(beat.chevauchement.map(|o| quoted(overlap_word(o)))),
        ),
        (
            "après_saut",
            null(beat.apres_saut.map(|a| quoted(after_skip_word(a)))),
        ),
        ("actif", null(beat.actif.map(|a| a.to_string()))),
        ("raison", null(beat.raison.as_deref().map(quoted))),
        ("jusqu_au", null(beat.jusqu_au.as_deref().map(quoted))),
        ("tolérance", null(beat.tolerance.as_deref().map(quoted))),
        ("décalage", null(beat.decalage.as_deref().map(quoted))),
        ("par", null(beat.par.as_deref().map(quoted))),
    ];
    fields
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The wire words of the beat's enums (the serde vocabulary, pinned).
fn locus_word(locus: Locus) -> &'static str {
    match locus {
        Locus::Local => "local",
        Locus::Cloud => "cloud",
    }
}

/// The wire words of `manqué:`.
fn miss_word(policy: MissPolicy) -> &'static str {
    match policy {
        MissPolicy::Rattraper => "rattraper",
        MissPolicy::RattraperUneFois => "rattraper-une-fois",
        MissPolicy::Sauter => "sauter",
    }
}

/// The wire words of `chevauchement:`.
fn overlap_word(overlap: Overlap) -> &'static str {
    match overlap {
        Overlap::Sauter => "sauter",
        Overlap::File => "file",
        Overlap::Remplacer => "remplacer",
    }
}

/// The wire words of `après_saut:`.
fn after_skip_word(after: AfterSkip) -> &'static str {
    match after {
        AfterSkip::ProchainCreneau => "prochain-créneau",
        AfterSkip::ACompletion => "à-complétion",
    }
}

/// A quoted canonical string — the JSON escapes (a free-text field
/// must never redraw the field boundaries). The same shape the CLI's
/// ledger speaks; the planes stay separate, the shape is the law.
pub(crate) fn quoted(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// sha256 hex over the exact bytes — the source-identity convention
/// (`nika-event`'s `workflow_sha256` speaks it; the primitive is pure,
/// so it lives here directly rather than as a dependency).
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const WORKFLOW: &str = "workflows/doctor.nika.yaml";
    const CADENCE: &str = "TZ=UTC 0 3 * * *";

    fn at(text: &str) -> jiff::Zoned {
        text.parse::<jiff::Timestamp>()
            .expect("ts")
            .to_zoned(jiff::tz::TimeZone::UTC)
    }

    fn ts(text: &str) -> jiff::Timestamp {
        text.parse::<jiff::Timestamp>().expect("ts")
    }

    /// A one-beat registry (parse only — the canonical reads the FIELDS,
    /// validated or not).
    fn registry_with(body: &str) -> crate::registry::ArmRegistry {
        let text = format!(
            "nika: v1\narm:\n  - workflow: {WORKFLOW}\n    cadence: \"{CADENCE}\"\n    plafond: 0.25\n{body}"
        );
        crate::parse::parse_registry(&text).expect("parse")
    }

    fn token() -> FencingToken {
        FencingToken::new(7)
    }

    fn claimed() -> FiringEvent {
        FiringEvent::Claimed {
            fencing: token(),
            generation: None,
            deadline: ts("2026-08-20T03:00:00Z"),
        }
    }

    // ── (d) the SlotId known vector, relocated BYTE-IDENTICAL ────────

    /// The derivation moved here from the CLI's ledger (W7) — a
    /// relocation, never a change: the test hashes the domain-separated
    /// canonical string ITSELF, never through the implementation's own
    /// helper.
    #[test]
    fn the_slot_id_is_the_canonical_hash() {
        use sha2::Digest as _;
        let expected = format!(
            "{:x}",
            sha2::Sha256::digest(
                b"nika/arm-slot@1\nworkflows/doctor.nika.yaml\nTZ=UTC 0 3 * * *\n2026-08-19T03:00:00Z"
            )
        );
        assert_eq!(
            SlotId::derive(WORKFLOW, CADENCE, &at("2026-08-19T03:00:00Z")).as_str(),
            expected
        );
        // The INSTANT is the identity — a zoned view of the same
        // instant hashes identically (UTC on the wire).
        let paris: jiff::Zoned = "2026-08-19T05:00:00+02:00[Europe/Paris]"
            .parse()
            .expect("zoned");
        assert_eq!(SlotId::derive(WORKFLOW, CADENCE, &paris).as_str(), expected);
    }

    /// The wire door: 64 lowercase hex, nothing else. `short()` is the
    /// report's 8-char form.
    #[test]
    fn the_slot_id_from_wire_accepts_only_the_hash_shape() {
        let id = SlotId::derive(WORKFLOW, CADENCE, &at("2026-08-19T03:00:00Z"));
        let round = SlotId::from_wire(id.as_str()).expect("the real id reads back");
        assert_eq!(round, id);
        assert_eq!(id.short().len(), 8);
        assert!(SlotId::from_wire("nope").is_none());
        assert!(SlotId::from_wire(&"ab".repeat(31)).is_none(), "63 ≠ 64");
        assert!(
            SlotId::from_wire(&"AB".repeat(32)).is_none(),
            "lowercase hex only"
        );
    }

    #[test]
    fn the_fencing_token_round_trips_its_sequence() {
        assert_eq!(FencingToken::new(7).get(), 7);
        assert_eq!(FencingToken::new(u64::MAX).get(), u64::MAX);
    }

    // ── (f) the ArmGeneration (F17 — a firing pins its generation) ───

    /// Identical digests hash identically across two computations; one
    /// changed digest mints a NEW generation; a beat renamed by
    /// position (the label never enters) keeps its gen.
    #[test]
    fn the_generation_is_stable_on_the_digest_and_deaf_to_the_label() {
        let registry = registry_with("    manqué: sauter\n");
        let beat = registry.beats().next().expect("one beat");
        let world_a = "a".repeat(64);
        let world_b = "b".repeat(64);
        let one = ArmGeneration::compute(beat, &world_a);
        let two = ArmGeneration::compute(beat, &world_a);
        assert_eq!(one, two, "identical digest, identical gen");
        let changed = ArmGeneration::compute(beat, &world_b);
        assert_ne!(one, changed, "one world byte changed → a new gen");
        // The renamed-but-identical beat: two registry entries with the
        // SAME fields take the labels `doctor` and `doctor-2` — the gen
        // never sees them.
        let pair = crate::parse::parse_registry(&format!(
            "nika: v1\narm:\n  - workflow: {WORKFLOW}\n    cadence: \"{CADENCE}\"\n    plafond: 0.25\n    manqué: sauter\n  - workflow: {WORKFLOW}\n    cadence: \"{CADENCE}\"\n    plafond: 0.25\n    manqué: sauter\n"
        ))
        .expect("parse");
        let gens: Vec<ArmGeneration> = pair
            .beats()
            .map(|b| ArmGeneration::compute(b, &world_a))
            .collect();
        assert_eq!(gens[0], gens[1], "the label never enters the hash");
        // … but any FIELD of the beat does.
        let other = registry_with("    manqué: rattraper-une-fois\n");
        let other = other.beats().next().expect("one beat");
        assert_ne!(
            ArmGeneration::compute(beat, &world_a),
            ArmGeneration::compute(other, &world_a),
            "a changed beat field mints a new gen"
        );
    }

    /// The canonical byte shape, pinned from WITHOUT: the test builds
    /// the hash input by hand (the `@2` domain · declared field order ·
    /// quoted strings · `null` for the absent · a NUL · the admitted
    /// world's snapshot digest last).
    #[test]
    fn the_generation_hash_covers_the_declared_canonical_form() {
        use sha2::Digest as _;
        let registry = registry_with("    manqué: sauter\n");
        let beat = registry.beats().next().expect("one beat");
        let digest = "f".repeat(64);
        let mut preimage = format!(
            "nika/arm-gen@2\nworkflow=\"{WORKFLOW}\"\ncadence=\"{CADENCE}\"\noù=null\nplafond=0.25\nmanqué=\"sauter\"\nchevauchement=null\naprès_saut=null\nactif=null\nraison=null\njusqu_au=null\ntolérance=null\ndécalage=null\npar=null"
        )
        .into_bytes();
        preimage.push(0);
        preimage.extend_from_slice(digest.as_bytes());
        let expected = format!("{:x}", sha2::Sha256::digest(&preimage));
        assert_eq!(ArmGeneration::compute(beat, &digest).as_str(), expected);
        // The short form the report prints.
        assert_eq!(ArmGeneration::compute(beat, &digest).short().len(), 8);
        // The wire door.
        let generation = ArmGeneration::compute(beat, &digest);
        assert_eq!(
            ArmGeneration::from_wire(generation.as_str()).expect("round-trip"),
            generation
        );
        assert!(ArmGeneration::from_wire("not-a-gen").is_none());
        assert!(ArmGeneration::from_wire(&"g0".repeat(32)).is_none());
        assert!(ArmGeneration::from_wire(&"AB".repeat(32)).is_none());
    }

    /// Every declared-field renderer participates in the generation,
    /// including escaping: this vector executes the enum words and a
    /// control character instead of covering only the all-null shape.
    #[test]
    fn the_generation_canonical_covers_every_declared_field_renderer() {
        use sha2::Digest as _;
        let registry = registry_with(concat!(
            "    où: cloud\n",
            "    manqué: rattraper-une-fois\n",
            "    chevauchement: remplacer\n",
            "    après_saut: à-complétion\n",
            "    actif: false\n",
            "    raison: \"pause\\tcontrôlée\"\n",
            "    jusqu_au: \"2099-12-31\"\n",
            "    tolérance: \"3/4\"\n",
            "    décalage: hash\n",
            "    par: \"nika\"\n",
        ));
        let beat = registry.beats().next().expect("one beat");
        let digest = "f".repeat(64);
        let mut preimage = format!(
            "nika/arm-gen@2\nworkflow=\"{WORKFLOW}\"\ncadence=\"{CADENCE}\"\noù=\"cloud\"\nplafond=0.25\nmanqué=\"rattraper-une-fois\"\nchevauchement=\"remplacer\"\naprès_saut=\"à-complétion\"\nactif=false\nraison=\"pause\\u0009contrôlée\"\njusqu_au=\"2099-12-31\"\ntolérance=\"3/4\"\ndécalage=\"hash\"\npar=\"nika\""
        )
        .into_bytes();
        preimage.push(0);
        preimage.extend_from_slice(digest.as_bytes());
        let expected = format!("{:x}", sha2::Sha256::digest(&preimage));
        assert_eq!(ArmGeneration::compute(beat, &digest).as_str(), expected);
    }

    /// The resident serve hashes the SAME preimage under the SAME `@2`
    /// domain with its own declaration identity — the `ScheduleRevision`
    /// string. Exact cross-edge equality is impossible by design (a beat
    /// is not a revision); the shared `pin` is the one judge both edges
    /// answer to.
    #[test]
    fn the_resident_edge_hashes_the_one_law_with_its_revision() {
        use sha2::Digest as _;
        let revision =
            crate::schedule::ScheduleRevision::from_wire(&format!("sha256:{}", "e".repeat(64)))
                .expect("revision");
        let digest = "f".repeat(64);
        let mut preimage = GENERATION_DOMAIN.to_vec();
        preimage.extend_from_slice(revision.as_str().as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(digest.as_bytes());
        let expected = format!("{:x}", sha2::Sha256::digest(&preimage));
        assert_eq!(
            ArmGeneration::compute_resident(&revision, &digest).as_str(),
            expected
        );
    }

    // ── the machine · scripted walks ─────────────────────────────────

    /// The happy path end to end: due → claimed → running → succeeded.
    #[test]
    fn the_happy_path_folds_to_succeeded() {
        let events = vec![
            FiringEvent::Due,
            claimed(),
            FiringEvent::Started { fencing: token() },
            FiringEvent::Finished {
                fencing: Some(token()),
                code: 0,
            },
        ];
        assert_eq!(fold(&events), FiringState::Succeeded);
    }

    /// The receipt's code classifies: 4 parks (Cancelled — never
    /// resumed), any other non-zero fails (`FailedRetryable` — the
    /// EXHAUSTION verdict is a separate event the policy sends).
    #[test]
    fn the_exit_code_classifies_the_terminal() {
        for (code, expected) in [
            (0u8, FiringState::Succeeded),
            (4, FiringState::Cancelled),
            (1, FiringState::FailedRetryable),
            (2, FiringState::FailedRetryable),
            (3, FiringState::FailedRetryable),
        ] {
            let events = vec![
                FiringEvent::Due,
                claimed(),
                FiringEvent::Finished {
                    fencing: Some(token()),
                    code,
                },
            ];
            assert_eq!(fold(&events), expected, "code {code}");
        }
        // … and the exhaustion event completes the v0 story.
        let events = vec![
            FiringEvent::Due,
            claimed(),
            FiringEvent::Finished {
                fencing: Some(token()),
                code: 1,
            },
            FiringEvent::AttemptsExhausted { fencing: token() },
        ];
        assert_eq!(fold(&events), FiringState::FailedPermanent);
    }

    /// A bare receipt (a W2-era line · a direct record) settles the
    /// slot in one step — the ledger is the truth, claim or no claim.
    #[test]
    fn a_bare_receipt_settles_the_slot() {
        let events = vec![FiringEvent::Finished {
            fencing: None,
            code: 0,
        }];
        assert_eq!(fold(&events), FiringState::Succeeded);
        let events = vec![FiringEvent::Skipped {
            reason: Some(SkipReason::Missed(2)),
        }];
        assert_eq!(fold(&events), FiringState::Skipped);
    }

    /// The crash detector: a claim whose deadline passed is AMBIGUOUS —
    /// the run may have happened. A late receipt resolves it; the
    /// sweep's rescue re-arms it; the poison verdict dead-letters it.
    #[test]
    fn the_orphan_is_ambiguous_until_resolved() {
        let events = vec![claimed(), FiringEvent::DeadlinePassed { fencing: token() }];
        assert_eq!(fold(&events), FiringState::Ambiguous);
        // The late receipt resolves the ambiguity.
        let events = vec![
            claimed(),
            FiringEvent::DeadlinePassed { fencing: token() },
            FiringEvent::Finished {
                fencing: Some(token()),
                code: 0,
            },
        ];
        assert_eq!(fold(&events), FiringState::Succeeded);
        // The rescue re-arms (W8's sweep).
        let events = vec![
            claimed(),
            FiringEvent::DeadlinePassed { fencing: token() },
            FiringEvent::Rescued { fencing: token() },
        ];
        assert_eq!(fold(&events), FiringState::Due);
        // The poison verdict parks the beat in the dead letters.
        let events = vec![
            claimed(),
            FiringEvent::DeadlinePassed { fencing: token() },
            FiringEvent::Poisoned { fencing: token() },
        ];
        assert_eq!(fold(&events), FiringState::DeadLettered);
    }

    /// The fencing pairing: a receipt naming ANOTHER token settles
    /// nothing; a deadline for another token expires nothing.
    #[test]
    fn a_foreign_token_never_settles_the_claim() {
        let events = vec![
            claimed(),
            FiringEvent::Finished {
                fencing: Some(FencingToken::new(9)),
                code: 0,
            },
        ];
        assert_eq!(fold(&events), FiringState::Claimed, "a foreign receipt");
        let events = vec![
            claimed(),
            FiringEvent::DeadlinePassed {
                fencing: FencingToken::new(9),
            },
        ];
        assert_eq!(fold(&events), FiringState::Claimed, "a foreign deadline");
        // A re-claim REBINDS: the newer token owns the lifecycle.
        let events = vec![
            claimed(),
            FiringEvent::Claimed {
                fencing: FencingToken::new(9),
                generation: None,
                deadline: ts("2026-08-21T03:00:00Z"),
            },
            FiringEvent::Finished {
                fencing: Some(token()),
                code: 0,
            },
        ];
        assert_eq!(
            fold(&events),
            FiringState::Claimed,
            "the re-claimed token owns — the old receipt settles nothing"
        );
        // Exhaustion belongs to the receipt it names, never another
        // claim's failure.
        let events = vec![
            claimed(),
            FiringEvent::Finished {
                fencing: Some(token()),
                code: 1,
            },
            FiringEvent::AttemptsExhausted {
                fencing: FencingToken::new(9),
            },
        ];
        assert_eq!(
            fold(&events),
            FiringState::FailedRetryable,
            "foreign exhaustion cannot make this claim permanent"
        );
    }

    /// The terminal states absorb everything: a settled lifecycle never
    /// moves again (a duplicate receipt, a late skip — identity).
    #[test]
    fn the_terminal_states_are_sticky() {
        for terminal in [
            FiringState::Succeeded,
            FiringState::FailedPermanent,
            FiringState::Skipped,
            FiringState::Cancelled,
            FiringState::DeadLettered,
            FiringState::Deferred,
        ] {
            for event in [
                FiringEvent::Due,
                claimed(),
                FiringEvent::Skipped { reason: None },
                FiringEvent::Finished {
                    fencing: None,
                    code: 0,
                },
            ] {
                assert_eq!(
                    transition(terminal, &event),
                    terminal,
                    "{terminal:?} absorbs {event:?}"
                );
            }
        }
    }

    /// The skip from the planner's own states (pre-slot: inactive ·
    /// cloud · expired · webhook) — never claimed, never run.
    #[test]
    fn a_pre_slot_skip_folds_from_planned() {
        for reason in [
            SkipReason::Inactive,
            SkipReason::Cloud,
            SkipReason::Expired,
            SkipReason::Webhook,
        ] {
            let events = vec![FiringEvent::Skipped {
                reason: Some(reason),
            }];
            assert_eq!(fold(&events), FiringState::Skipped);
        }
    }

    // ── decide() — the policy door (W8 consumes it) ──────────────────

    /// decide is transition + the durable effects, `now` riding the
    /// journal effects (the ledger's ts). The policy gates the Fire.
    #[test]
    fn decide_pairs_each_transition_with_its_durable_effect() {
        let now = ts("2026-08-19T03:02:00Z");
        let policy = FiringPolicy::single();
        let decisions: Vec<_> =
            decide(FiringState::Planned, &FiringEvent::Due, &now, &policy).collect();
        assert_eq!(
            decisions,
            vec![Decision::Become(FiringState::Due), Decision::Fire]
        );
        // A zero-attempt policy clears nothing to fire.
        let none = FiringPolicy { max_attempts: 0 };
        let decisions: Vec<_> =
            decide(FiringState::Planned, &FiringEvent::Due, &now, &none).collect();
        assert_eq!(decisions, vec![Decision::Become(FiringState::Due)]);
        // The claim: journal BEFORE anything runs (the order law).
        let decisions: Vec<_> = decide(FiringState::Due, &claimed(), &now, &policy).collect();
        assert_eq!(
            decisions,
            vec![
                Decision::Become(FiringState::Claimed),
                Decision::JournalClaim { at: now },
                Decision::Fire,
            ]
        );
        let decisions: Vec<_> = decide(FiringState::Due, &claimed(), &now, &none).collect();
        assert_eq!(
            decisions,
            vec![
                Decision::Become(FiringState::Claimed),
                Decision::JournalClaim { at: now },
            ]
        );
        // The receipt: the terminal state + its journal.
        let finished = FiringEvent::Finished {
            fencing: Some(token()),
            code: 0,
        };
        let decisions: Vec<_> = decide(FiringState::Claimed, &finished, &now, &policy).collect();
        assert_eq!(
            decisions,
            vec![
                Decision::Become(FiringState::Succeeded),
                Decision::JournalReceipt {
                    state: FiringState::Succeeded,
                    at: now
                },
            ]
        );
        // The deadline: the orphan SURFACES.
        let passed = FiringEvent::DeadlinePassed { fencing: token() };
        let decisions: Vec<_> = decide(FiringState::Claimed, &passed, &now, &policy).collect();
        assert_eq!(
            decisions,
            vec![
                Decision::Become(FiringState::Ambiguous),
                Decision::SurfaceOrphan,
            ]
        );
        // Off-table: a lawful no-op, said as such.
        let decisions: Vec<_> = decide(FiringState::Succeeded, &claimed(), &now, &policy).collect();
        assert_eq!(decisions, vec![Decision::Ignore]);
    }

    /// The skip and the defer journal their own way (a defer is NOT a
    /// consumed attempt — the reject/nack law).
    #[test]
    fn decide_journals_the_skip_and_the_defer() {
        let now = ts("2026-08-19T03:02:00Z");
        let policy = FiringPolicy::single();
        let skip = FiringEvent::Skipped {
            reason: Some(SkipReason::Overlap),
        };
        let decisions: Vec<_> = decide(FiringState::Due, &skip, &now, &policy).collect();
        assert_eq!(
            decisions,
            vec![
                Decision::Become(FiringState::Skipped),
                Decision::JournalSkip { at: now },
            ]
        );
        let decisions: Vec<_> =
            decide(FiringState::Due, &FiringEvent::Deferred, &now, &policy).collect();
        assert_eq!(
            decisions,
            vec![
                Decision::Become(FiringState::Deferred),
                Decision::JournalDefer { at: now },
            ]
        );
        // The W8 doors: rescue re-arms, poison dead-letters.
        let rescued = FiringEvent::Rescued { fencing: token() };
        let decisions: Vec<_> = decide(FiringState::Ambiguous, &rescued, &now, &policy).collect();
        assert_eq!(
            decisions,
            vec![Decision::Become(FiringState::Due), Decision::Rearm]
        );
        let poisoned = FiringEvent::Poisoned { fencing: token() };
        let decisions: Vec<_> = decide(FiringState::Ambiguous, &poisoned, &now, &policy).collect();
        assert_eq!(
            decisions,
            vec![
                Decision::Become(FiringState::DeadLettered),
                Decision::DeadLetter,
            ]
        );
    }

    /// The state words — the report prints them, the vocabulary is
    /// frozen.
    #[test]
    fn the_state_words_are_the_wire_vocabulary() {
        let words: Vec<&'static str> = FiringState::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            words,
            vec![
                "planned",
                "due",
                "claimed",
                "running",
                "succeeded",
                "failed-retryable",
                "failed-permanent",
                "deferred",
                "skipped",
                "cancelled",
                "dead-lettered",
                "ambiguous",
            ]
        );
    }

    /// The skip reasons: the firer's machine tokens, round-tripped.
    #[test]
    fn the_skip_reasons_round_trip() {
        for (reason, word) in [
            (SkipReason::Inactive, "inactive"),
            (SkipReason::Cloud, "cloud"),
            (SkipReason::Expired, "expired"),
            (SkipReason::Webhook, "webhook"),
            (SkipReason::Missed(3), "missed:3"),
            (SkipReason::Overlap, "overlap"),
            (SkipReason::OverlapTimeout, "overlap-timeout"),
            (SkipReason::ServeStop, "serve-stop"),
            (SkipReason::Already, "already"),
            (SkipReason::NotDue, "not-due"),
        ] {
            assert_eq!(reason.as_str(), word);
            assert_eq!(SkipReason::parse(word), Some(reason));
        }
        assert_eq!(SkipReason::parse("nimporte"), None, "an unknown reason");
    }

    // ── (b) the proptest: every sequence folds lawfully ──────────────

    fn arb_event() -> impl Strategy<Value = FiringEvent> {
        let fencing = FencingToken::new(7);
        let deadline = (1_700_000_000i64..2_000_000_000).prop_map(move |s| FiringEvent::Claimed {
            fencing,
            generation: None,
            deadline: jiff::Timestamp::from_second(s).expect("in range"),
        });
        let finished =
            (0u8..=5, prop::bool::ANY).prop_map(move |(code, named)| FiringEvent::Finished {
                fencing: named.then_some(fencing),
                code,
            });
        let skipped = (0u32..50).prop_map(|n| FiringEvent::Skipped {
            reason: Some(SkipReason::Missed(n)),
        });
        prop_oneof![
            Just(FiringEvent::Due),
            deadline,
            Just(FiringEvent::Started { fencing }),
            finished,
            Just(FiringEvent::AttemptsExhausted { fencing }),
            skipped,
            Just(FiringEvent::Deferred),
            Just(FiringEvent::Cancelled {
                fencing: Some(fencing)
            }),
            Just(FiringEvent::DeadlinePassed { fencing }),
            Just(FiringEvent::Rescued { fencing }),
            Just(FiringEvent::Poisoned { fencing }),
        ]
    }

    /// The transition table, written a SECOND time from the module doc
    /// — the property holds the implementation to this independent
    /// reading: every step is identity or one of these rows.
    fn lawful(from: FiringState, event: &FiringEvent, to: FiringState) -> bool {
        use FiringState as S;
        let terminal_of = |code: u8| match code {
            0 => S::Succeeded,
            4 => S::Cancelled,
            _ => S::FailedRetryable,
        };
        let on_table = match (from, event) {
            (S::Planned | S::Due, FiringEvent::Due)
            | (S::Ambiguous, FiringEvent::Rescued { .. }) => S::Due,
            (
                S::Planned | S::Due | S::Claimed | S::FailedRetryable | S::Ambiguous,
                FiringEvent::Claimed { .. },
            ) => S::Claimed,
            (S::Claimed, FiringEvent::Started { .. }) => S::Running,
            (
                S::Planned | S::Due | S::Claimed | S::Running | S::Ambiguous,
                FiringEvent::Finished { code, .. },
            ) => terminal_of(*code),
            (S::FailedRetryable, FiringEvent::AttemptsExhausted { .. }) => S::FailedPermanent,
            (S::Planned | S::Due, FiringEvent::Skipped { .. }) => S::Skipped,
            (S::Due, FiringEvent::Deferred) => S::Deferred,
            (S::Planned | S::Due | S::Claimed | S::Running, FiringEvent::Cancelled { .. }) => {
                S::Cancelled
            }
            (S::Claimed | S::Running, FiringEvent::DeadlinePassed { .. }) => S::Ambiguous,
            (S::Ambiguous, FiringEvent::Poisoned { .. }) => S::DeadLettered,
            _ => from,
        };
        to == on_table
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        /// (b) · arbitrary event sequences: the fold never panics, every
        /// step is identity or on the table (the independent reading
        /// above), and `fold` lands where the walked `transition` lands.
        #[test]
        fn arbitrary_sequences_fold_lawfully(events in prop::collection::vec(arb_event(), 0..40)) {
            let mut state = FiringState::Planned;
            for event in &events {
                let next = transition(state, event);
                prop_assert!(
                    lawful(state, event, next),
                    "off-table: {state:?} + {event:?} → {next:?}"
                );
                state = next;
            }
            prop_assert_eq!(fold(&events), state, "fold == the walked transition");
        }
    }
}
