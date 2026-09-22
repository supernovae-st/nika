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
    /// The intelligence was asked and could not answer (a failed call, a
    /// blank answer): UNKNOWN, everything kept as is, said as such.
    Failed,
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
        match self.reasoner.reason_label(&routing_prompt(context, raw)) {
            Ok(reply) => TurnDecision::new(TurnAct::parse(&reply.text), RoutingMethod::Model),
            Err(_) => TurnDecision::new(TurnAct::Unknown, RoutingMethod::Failed),
        }
    }
}

/// The bounded prompt the session's intelligence answers with one label.
/// Its shape follows what measurably helps a single-label zero-shot
/// classifier (2024-2026 literature, see the lane's routing note): the
/// labels bullet-listed in a fixed order, each with its deciding cue and
/// one contrast pair per confusion the corpus showed (a capability
/// question vs a change · a yes with a change); MIXED decided by clauses;
/// UNKNOWN on decidable conditions, never as a comfortable default; no
/// reasoning asked (chain-of-thought lowers label accuracy).
#[must_use]
pub fn routing_prompt(context: &TurnContext, raw: &str) -> String {
    let mut p = String::from(
        "You route ONE line a human typed in a conversation with Nika, an automation tool. Answer with exactly one label from the list and nothing else — no reasoning, no punctuation.\n",
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
    p.push_str(concat!(
        "Labels, in order:\n",
        "- DISCUSS: a question or a remark about the current automation or about Nika; if Nika answered it, nothing about the automation would change. Cue: asks what/whether/why, or comments. « Can it write outside the project? » is DISCUSS; « Can you write it to ./out/final.md instead? » is MODIFY.\n",
        "- MODIFY: asks for a change to the current automation, however it is phrased — as a question, a wish, a correction, a negation. Cue: names something that should be different (a destination, a schedule, a step, a recipient). « What I actually want is ./out/final.md » is MODIFY.\n",
        "- NEW_WORK: describes a new automation unrelated to the current one.\n",
        "- ANSWER: gives the value the last question asked for (a model name, a path, a number, a choice), nothing more.\n",
        "- REQUEST_RUN: asks to run the current automation as it is, with no change in the same line.\n",
        "- MIXED: the line carries two distinct acts, e.g. an approval or a refusal AND a change (« yes, but change the file first »), or a run AND a change (« run it, but only on Fridays »). An approval with a change is never a plain approval.\n",
        "- UNKNOWN: only when no label fits the line, or two labels fit it equally after the cues above.\n",
        "Label:",
    ));
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
        assert!(p.contains("- MODIFY: asks for a change") && p.ends_with("Label:"));
        assert!(p.contains("no reasoning"), "no chain-of-thought is asked");
        let fallback = ConservativeFallback.classify(&ctx, "anything");
        assert_eq!(fallback.act, TurnAct::Unknown);
        assert_eq!(fallback.method, RoutingMethod::Fallback);
        // An intelligence that cannot answer is a FAILED route, not « none available ».
        let failed = ReasonerClassifier::new(Box::new(crate::reasoner::NoReasoner))
            .classify(&ctx, "anything");
        assert_eq!(failed.act, TurnAct::Unknown);
        assert_eq!(failed.method, RoutingMethod::Failed);
        let record = RouteRecord::new(SessionPhase::Idle, "hello", &fallback);
        assert_eq!(record.raw_hash.len(), 12);
        assert!(record.line().contains("UNKNOWN"));
    }

    /// The Arena routing benchmark seam, LIVE (ignored by default): the
    /// corpus at `NIKA_ROUTING_CORPUS` (JSONL: id · state · line · expected
    /// · optional `automation` / `last_prompt` / `or`, a second act the row
    /// accepts) is routed by the real
    /// `ReasonerClassifier` over `NIKA_ROUTING_MODEL` (`<provider>/<model>`,
    /// the key from the environment); one receipt line per row is printed
    /// and, when `NIKA_ROUTING_RECEIPT` names a file, written as JSONL. The
    /// routing addendum's first milestone (`milestone: true` rows) must
    /// route exactly; the rest is measured, never asserted.
    #[test]
    #[ignore = "a real seat and a corpus file, by env"]
    #[allow(
        clippy::disallowed_methods,
        clippy::disallowed_macros,
        clippy::print_stdout,
        reason = "a live harness: the corpus, the model and the receipt come by env; the receipt is printed"
    )]
    fn routing_corpus_under_a_real_seat() {
        let corpus = std::env::var("NIKA_ROUTING_CORPUS").expect("NIKA_ROUTING_CORPUS");
        let model = std::env::var("NIKA_ROUTING_MODEL").expect("NIKA_ROUTING_MODEL");
        let text = std::fs::read_to_string(&corpus).expect("the corpus file");
        let mut classifier = ReasonerClassifier::new(Box::new(crate::reasoner::ProviderReasoner {
            model: model.clone(),
            label: model.clone(),
        }));
        let demo = "Every weekday read the new support tickets in ./tickets.json, group the open ones by topic, draft a short brief, and ask me before sending it to Slack";
        let mut receipts = Vec::new();
        let (mut right, mut total, mut milestone_wrong) = (0usize, 0usize, Vec::new());
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let row: serde_json::Value = serde_json::from_str(line).expect("a JSONL row");
            let id = row["id"].as_str().unwrap_or("?").to_owned();
            let raw = row["line"].as_str().expect("line");
            let expected = row["expected"].as_str().expect("expected");
            let state = row["state"].as_str().unwrap_or("idle");
            let phase = match state {
                "proposal_pending" => SessionPhase::ProposalPending,
                "question_pending" => SessionPhase::QuestionPending,
                "gate_pending" => SessionPhase::GatePending,
                _ => SessionPhase::Idle,
            };
            let automation = row["automation"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| (state != "idle").then(|| demo.to_owned()));
            let last_prompt =
                row["last_prompt"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| match phase {
                        SessionPhase::QuestionPending => {
                            Some("Which model should draft the brief? (provider/model)".to_owned())
                        }
                        SessionPhase::GatePending => {
                            Some("Send the brief to Slack now? (yes / no)".to_owned())
                        }
                        SessionPhase::ProposalPending => {
                            Some("the proposal, waiting for yes or no".to_owned())
                        }
                        _ => None,
                    });
            let ctx = TurnContext {
                phase,
                automation,
                last_prompt,
            };
            let decision = classifier.classify(&ctx, raw);
            let got = decision.act.label();
            // A row may accept a second act (« yes but… » is MIXED or
            // MODIFY: never a consent either way); the receipt keeps both.
            let ok = got == expected || row["or"].as_str() == Some(got);
            total += 1;
            right += usize::from(ok);
            if row["milestone"].as_bool() == Some(true) && !ok {
                milestone_wrong.push(format!("{id} «{raw}» expected {expected} got {got}"));
            }
            println!(
                "{} {id:<5} {state:<17} {expected:<11} → {got:<11} «{raw}»",
                if ok { "✓" } else { "✖" }
            );
            receipts.push(serde_json::json!({
                "id": id, "state": state, "expected": expected, "got": got, "ok": ok,
                "method": format!("{:?}", decision.method), "model": model,
            }));
        }
        println!("routing corpus · {right}/{total} as expected · model {model}");
        if let Ok(path) = std::env::var("NIKA_ROUTING_RECEIPT") {
            let body: Vec<String> = receipts.iter().map(ToString::to_string).collect();
            std::fs::write(&path, body.join("\n") + "\n").expect("the receipt file");
        }
        assert!(
            milestone_wrong.is_empty(),
            "milestone rows misrouted: {milestone_wrong:?}"
        );
    }
}
