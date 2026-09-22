// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The semantic act of one free line — a bounded routing decision, never
//! a lexicon. Closed protocol tokens (`/quit` · `yes` · `cancel` · `why`)
//! are matched whole, deterministically, before this. Everything else is
//! open language: the typed session state and the RAW line go to a
//! bounded classifier (the session's intelligence, one label; a decision
//! seat later) that says which act it is — discuss, modify, new work,
//! answer, a run, mixed, or unknown — and the runtime acts on the act
//! with the human's own words, never a paraphrase. A classification is
//! never a consent: authority stays with the protocol tokens and the
//! typed state. When no intelligence can judge, the fallback is UNKNOWN
//! and the runtime keeps the automation unchanged and says so.

use std::fmt::{self, Write as _};

/// The bounded set of conversational acts (small on purpose: a routing
/// decision, not an intent ontology).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnAct {
    /// A question or a remark about the current automation or about Nika.
    Discuss,
    /// A change to the current automation (the proposal, the saved workflow).
    Modify,
    /// A new automation to build, unrelated to the current one.
    NewWork,
    /// The answer to the question Nika asked.
    Answer,
    /// A request to run the current automation.
    RequestRun,
    /// Several acts in one line (« yes but change the file first »).
    Mixed,
    /// The classifier could not tell — the runtime keeps everything as is.
    Unknown,
}

impl TurnAct {
    /// The label the bounded classifier answers with.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Discuss => "DISCUSS",
            Self::Modify => "MODIFY",
            Self::NewWork => "NEW_WORK",
            Self::Answer => "ANSWER",
            Self::RequestRun => "REQUEST_RUN",
            Self::Mixed => "MIXED",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Every act, in the order the classifier is told them.
    pub const ALL: [Self; 7] = [
        Self::Discuss,
        Self::Modify,
        Self::NewWork,
        Self::Answer,
        Self::RequestRun,
        Self::Mixed,
        Self::Unknown,
    ];

    /// The first label found in a reply, whole word, in the reply's order.
    #[must_use]
    pub fn parse(reply: &str) -> Self {
        let upper = reply.to_ascii_uppercase();
        let mut best: Option<(usize, Self)> = None;
        for act in Self::ALL {
            if let Some(at) = upper.find(act.label()) {
                let before = upper[..at].chars().next_back();
                let after = upper[at + act.label().len()..].chars().next();
                let whole = !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
                if whole && best.is_none_or(|(b, _)| at < b) {
                    best = Some((at, act));
                }
            }
        }
        best.map_or(Self::Unknown, |(_, act)| act)
    }
}

impl fmt::Display for TurnAct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// The typed state the routing decision is made in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionPhase {
    /// Nothing waits: a new line may be work, a question, or a run.
    Idle,
    /// A proposal waits for its consent.
    ProposalPending,
    /// A typed question (the compiler's, a declared input, the activation's) waits.
    QuestionPending,
    /// A run's human gate waits.
    GatePending,
}

impl SessionPhase {
    /// The phase in the classifier's words.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Idle => "nothing waits (idle)",
            Self::ProposalPending => "a proposed automation waits for the human's yes or no",
            Self::QuestionPending => "Nika asked the human a question and waits for its answer",
            Self::GatePending => "a run is paused at a human gate (yes or no)",
        }
    }
}

/// What the classifier is given beside the raw line — typed state, never
/// a paraphrase of the line.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TurnContext {
    /// The phase.
    pub phase: SessionPhase,
    /// The current automation in one line (the request it came from), when one exists.
    pub automation: Option<String>,
    /// The last thing Nika asked or showed the human, when relevant.
    pub last_prompt: Option<String>,
}

/// How the route was decided — recorded for `/details` and the proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RoutingMethod {
    /// A whole-line protocol token.
    Protocol,
    /// A closed fast path (a `?` line while nothing waits).
    FastPath,
    /// The session's intelligence, one bounded label.
    Model,
    /// No intelligence could judge: UNKNOWN, everything kept as is.
    Fallback,
}

/// The decision: the act, the other acts a mixed line carries, the method.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TurnDecision {
    /// The primary act.
    pub act: TurnAct,
    /// Further acts in the same line, when the classifier named them.
    pub secondary: Vec<TurnAct>,
    /// How it was decided.
    pub method: RoutingMethod,
}

impl TurnDecision {
    /// A decision by one method.
    #[must_use]
    pub const fn new(act: TurnAct, method: RoutingMethod) -> Self {
        Self {
            act,
            secondary: Vec::new(),
            method,
        }
    }
}

/// A bounded classifier of open language: the session's intelligence, a
/// decision seat, a scripted one in tests, or the conservative fallback.
pub trait TurnClassifier: Send {
    /// The act of `raw` in `context`; UNKNOWN when it cannot tell.
    fn classify(&mut self, context: &TurnContext, raw: &str) -> TurnDecision;
}

/// No intelligence: every open line is UNKNOWN, and the runtime keeps
/// the automation unchanged and says intelligence is unavailable.
#[derive(Debug, Default)]
pub struct ConservativeFallback;

impl TurnClassifier for ConservativeFallback {
    fn classify(&mut self, _context: &TurnContext, _raw: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback)
    }
}

/// The session's intelligence as the classifier: the routing prompt, one
/// label read whole; a path that cannot answer is the fallback (UNKNOWN).
pub struct ReasonerClassifier {
    reasoner: Box<dyn crate::reasoner::SessionReasoner>,
}

impl ReasonerClassifier {
    /// Over one reasoner (a door builds it from the same factory as the
    /// conversation's, so the route follows the chosen intelligence).
    #[must_use]
    pub fn new(reasoner: Box<dyn crate::reasoner::SessionReasoner>) -> Self {
        Self { reasoner }
    }
}

impl TurnClassifier for ReasonerClassifier {
    fn classify(&mut self, context: &TurnContext, raw: &str) -> TurnDecision {
        match self.reasoner.reason(&routing_prompt(context, raw)) {
            Ok(reply) => TurnDecision::new(TurnAct::parse(&reply.text), RoutingMethod::Model),
            Err(_) => TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback),
        }
    }
}

/// The bounded prompt the session's intelligence answers with one label.
#[must_use]
pub fn routing_prompt(context: &TurnContext, raw: &str) -> String {
    let mut p = String::from(
        "You route ONE line a human typed in a conversation with Nika, an automation tool. Answer with exactly one label and nothing else.\n",
    );
    let _ = writeln!(p, "State: {}.", context.phase.describe());
    if let Some(automation) = &context.automation {
        let _ = writeln!(
            p,
            "Current automation (the request it came from): «{automation}»."
        );
    }
    if let Some(prompt) = &context.last_prompt {
        let _ = writeln!(p, "The last thing Nika asked or showed: «{prompt}».");
    }
    let _ = writeln!(p, "The human's line: «{}».", raw.trim());
    p.push_str(
        "Labels: DISCUSS (a question or remark about the current automation or about Nika; nothing changes) · MODIFY (a change to the current automation, however it is phrased, even as a question) · NEW_WORK (a new, unrelated automation to build) · ANSWER (the answer to the question Nika asked) · REQUEST_RUN (asks to run it as it is) · MIXED (two of these in one line, e.g. a yes and a change) · UNKNOWN (cannot tell).\nLabel:",
    );
    p
}

/// A record of one route, for `/details` and the proof (never the line
/// itself: its hash).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RouteRecord {
    /// The phase the line arrived in.
    pub phase: SessionPhase,
    /// The line's blake3, first 12 hex characters.
    pub raw_hash: String,
    /// The act decided.
    pub act: TurnAct,
    /// How.
    pub method: RoutingMethod,
}

impl RouteRecord {
    /// One record.
    #[must_use]
    pub fn new(phase: SessionPhase, raw: &str, decision: &TurnDecision) -> Self {
        let hash = blake3::hash(raw.trim().as_bytes()).to_hex();
        Self {
            phase,
            raw_hash: hash[..12].to_owned(),
            act: decision.act,
            method: decision.method,
        }
    }

    /// The record's line (`/details`).
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{:?} · {} · {:?} · {}",
            self.phase, self.act, self.method, self.raw_hash
        )
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The label is read whole, first in the reply's order; an unknown reply is UNKNOWN.
    #[test]
    fn a_label_is_parsed_whole_and_first() {
        assert_eq!(TurnAct::parse("MODIFY"), TurnAct::Modify);
        assert_eq!(TurnAct::parse("Label: discuss."), TurnAct::Discuss);
        assert_eq!(
            TurnAct::parse("I think MIXED (a yes and MODIFY)"),
            TurnAct::Mixed
        );
        assert_eq!(TurnAct::parse("NEW_WORK"), TurnAct::NewWork);
        assert_eq!(TurnAct::parse("REQUEST_RUN"), TurnAct::RequestRun);
        assert_eq!(
            TurnAct::parse("MODIFYING"),
            TurnAct::Unknown,
            "not a whole word"
        );
        assert_eq!(TurnAct::parse("no idea"), TurnAct::Unknown);
    }

    /// The prompt carries the typed state and the raw line, and asks for one label.
    #[test]
    fn the_prompt_carries_the_state_and_the_raw_line() {
        let ctx = TurnContext {
            phase: SessionPhase::ProposalPending,
            automation: Some("Read ./sales.csv and write the total to ./out/total.md".to_owned()),
            last_prompt: None,
        };
        let p = routing_prompt(&ctx, "  can you write it to ./out/final.md instead?  ");
        assert!(p.contains("waits for the human's yes or no"));
        assert!(p.contains("«can you write it to ./out/final.md instead?»"));
        assert!(p.contains("MODIFY (a change") && p.ends_with("Label:"));
        let fallback = ConservativeFallback.classify(&ctx, "anything");
        assert_eq!(fallback.act, TurnAct::Unknown);
        assert_eq!(fallback.method, RoutingMethod::Fallback);
        let record = RouteRecord::new(SessionPhase::Idle, "hello", &fallback);
        assert_eq!(record.raw_hash.len(), 12);
        assert!(record.line().contains("UNKNOWN"));
    }
}
