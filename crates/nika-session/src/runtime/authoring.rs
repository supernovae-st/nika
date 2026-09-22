// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring turns: a free-text intent reaches the ONE compiler; its
//! typed question becomes the next line's meaning; a Ready candidate
//! becomes the proposal the consent line answers; an explicit `run …`
//! line runs an accepted workflow through the door. Each pending state
//! gives the next line exactly one typed meaning — a `yes` never crosses
//! from an authoring answer to a consent to a gate.

use std::fmt::Write as _;
use std::path::PathBuf;

use nika_onboard::compile::{CompileOutcome, CompileQuestion};

use super::{DEFAULT_CEILING_USD, SessionRuntime, TurnOutcome, ceiling_in, named_files};
use crate::authoring::{
    AuthoringError, AuthoringRound, AuthoringSeat, Reading, compile_deterministic, compile_through,
    is_cancel, is_greeting, looks_like_discussion, reasons,
};
use crate::change::{RunRequest, check_on_disk};
use crate::outcome::{ProposalId, Refusal, RefusalClass};
use crate::review;

impl SessionRuntime {
    /// The authoring question the next line answers, when one is open.
    #[must_use]
    pub fn pending_question(&self) -> Option<&CompileQuestion> {
        self.authoring.as_ref().and_then(AuthoringRound::current)
    }

    /// The seat authoring reasons with (the banner names it).
    #[must_use]
    pub fn authoring_seat(&self) -> &AuthoringSeat {
        &self.seat
    }

    /// Re-derive the seat from the reasoner in place (open · `/intelligence`).
    pub(super) fn refresh_seat(&mut self) {
        self.seat = AuthoringSeat::from_reasoner(self.reasoner.as_ref(), &self.intelligence);
    }

    /// A free-text line as work to build: the deterministic ladder first
    /// (zero calls · the compiler's own order), then the seat when the
    /// reader read work it cannot settle alone. `None` when nothing in
    /// the line reads as work — the conversation owns that line.
    pub(super) fn author_unrecorded(&mut self, intent: &str) -> Option<TurnOutcome> {
        // A lone greeting is the conversation's before any door reads it:
        // the compiler's exact-skeleton door would take `hello` literally.
        if is_greeting(intent) {
            return None;
        }
        let round = AuthoringRound::new(intent);
        let out = match compile_deterministic(&round.request()) {
            Ok(out) => out,
            Err(e) => return Some(machinery(&e)),
        };
        match Reading::of(out) {
            Reading::NotWork(_) => {
                let seat_reads = matches!(self.seat, AuthoringSeat::Provider { .. });
                if seat_reads && !looks_like_discussion(intent) && named_files(intent).is_empty() {
                    Some(self.compile_under_seat(round))
                } else {
                    None
                }
            }
            Reading::Unsettled(out) => Some(match &self.seat {
                AuthoringSeat::Provider { .. } => self.compile_under_seat(round),
                // No intelligence chosen yet: the first screen is asked here,
                // in context, and the request resumes under the choice.
                AuthoringSeat::Deterministic { .. } if !self.chosen => {
                    self.ask_for_intelligence(intent, super::Need::Authoring)
                }
                AuthoringSeat::Deterministic { why } => {
                    TurnOutcome::Facts(honest_incomplete(&out, why.as_deref()))
                }
            }),
            reading => Some(self.settle(round, reading)),
        }
    }

    /// The same Compile, under the seat the human permitted, for work the
    /// deterministic policy could not settle. How long it takes and how
    /// hard it thinks is the compiler's; the human only learns that Nika
    /// is working (a truthful line, no invented detail).
    fn compile_under_seat(&mut self, round: AuthoringRound) -> TurnOutcome {
        self.progress("Working through this workflow…");
        match compile_through(&self.seat, &round.request()) {
            Ok(out) => match Reading::of(out) {
                Reading::Unsettled(out) | Reading::NotWork(out) => {
                    TurnOutcome::Facts(honest_incomplete(&out, None))
                }
                reading => self.settle(round, reading),
            },
            Err(e) => machinery(&e),
        }
    }

    /// What a reading becomes for the human: a proposal, a question, an
    /// honest incomplete, a refusal.
    fn settle(&mut self, mut round: AuthoringRound, reading: Reading) -> TurnOutcome {
        match reading {
            Reading::Ready(out) => self.propose(&round.intent, &out),
            Reading::Questions(out) => {
                round.absorb(&out);
                let Some(question) = round.current() else {
                    return TurnOutcome::Facts(honest_incomplete(&out, None));
                };
                let key = question.key.clone();
                let mut text = question_text(question, &round.reasons);
                if key == "model"
                    && let AuthoringSeat::Provider { model } = &self.seat
                {
                    let _ = write!(
                        text,
                        "\n  Enter takes your seat `{model}` · or name another <provider>/<model>"
                    );
                }
                self.intent.unresolved = vec![question.label.clone()];
                self.remember(&round.intent, &text);
                self.authoring = Some(round);
                TurnOutcome::Question {
                    key,
                    question: text,
                }
            }
            Reading::Unsettled(out) | Reading::NotWork(out) => {
                TurnOutcome::Facts(honest_incomplete(&out, None))
            }
            Reading::BudgetExhausted(_) => TurnOutcome::Facts(
                "I couldn't finish a workflow I trust within the current authoring budget — nothing was written; say it again to try once more, or narrow the request"
                    .to_owned(),
            ),
            Reading::ProviderFailed(out) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::IntelligenceRefused,
                format!(
                    "the authoring model did not answer — {} · nothing was substituted; say it again to retry, or `/intelligence` to change the model",
                    reasons(&out).join(" · ")
                ),
            )),
            Reading::Refused(out) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::AuthoringRefused,
                format!(
                    "the compiler refused this request — {}",
                    reasons(&out).join(" · ")
                ),
            )),
        }
    }

    /// The Ready candidate as the proposal the consent line answers:
    /// exact bytes, a fresh destination, the same check facade.
    fn propose(&mut self, goal: &str, out: &CompileOutcome) -> TurnOutcome {
        match review::propose(&self.snapshot.root, goal, out) {
            Ok(set) => {
                let bytes = set.preview();
                let id = ProposalId::of(&bytes);
                let preview = review::render(&set, out, &bytes);
                self.authoring = None;
                self.intent.unresolved.clear();
                self.remember(goal, &format!("(proposed {id})"));
                self.pending = Some(set);
                TurnOutcome::Proposal { id, preview }
            }
            Err(e) => TurnOutcome::Refusal(Refusal::from_change(&e)),
        }
    }

    /// The human's line as the answer to the open question: typed to its
    /// shape, bound to its key, and the same plan replayed. A cancel word
    /// drops the round; an empty line is not an answer.
    pub(super) fn answer_question_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(mut round) = self.authoring.take() else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no authoring question waits",
            ));
        };
        if is_cancel(line) {
            self.intent.unresolved.clear();
            self.remember(line, "(authoring discarded)");
            return TurnOutcome::Facts(
                "authoring discarded · nothing was written · describe the work again when ready"
                    .to_owned(),
            );
        }
        // An empty line takes the offered default and nothing else: the
        // `model` question's default is the seat the human already chose.
        let seat_default = match (round.current().map(|q| q.key.as_str()), &self.seat) {
            (Some("model"), AuthoringSeat::Provider { model }) => Some(model.clone()),
            _ => None,
        };
        let line = if line.trim().is_empty() {
            let Some(default) = seat_default else {
                self.authoring = Some(round);
                return TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::EmptyAnswer,
                    "the question needs an answer — nothing answers for you (`cancel` drops it)",
                ));
            };
            default
        } else {
            line.to_owned()
        };
        let line = line.as_str();
        let Some(key) = round.answer_current(line) else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no authoring question waits",
            ));
        };
        self.remember(line, &format!("(answered {key})"));
        match compile_through(&self.seat, &round.request()) {
            Ok(out) => {
                let reading = Reading::of(out);
                self.settle(round, reading)
            }
            Err(e) => machinery(&e),
        }
    }

    /// An explicit run line — `run it` · `run brief.nika with a ceiling of
    /// 0.05` · `test it now` — runs a workflow the human named or the one
    /// last accepted, only when its check on disk is clean. `None` when
    /// the line is not a run line.
    pub(super) fn run_turn(&mut self, input: &str) -> Option<TurnOutcome> {
        let lower = input.trim().to_lowercase();
        let first = lower
            .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
            .next()?;
        if !matches!(
            first,
            "run" | "execute" | "test" | "lance" | "exécute" | "teste" | "run:"
        ) {
            return None;
        }
        let root = self.snapshot.root.clone();
        let named = named_files(input)
            .into_iter()
            .map(PathBuf::from)
            .find(|p| root.join(p).is_file());
        let Some(workflow) = named.or_else(|| self.last_workflow.clone()) else {
            return Some(TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "nothing to run — name a workflow file (« run brief.nika »), or describe the work and Nika builds one first",
            )));
        };
        let audit = check_on_disk(&root, &workflow);
        if !audit.clean {
            let mut text = format!(
                "check · `{}` · findings ✖ — the run was not started",
                workflow.display()
            );
            for f in &audit.findings {
                text.push_str("\n  · ");
                text.push_str(f);
            }
            return Some(TurnOutcome::Facts(text));
        }
        let max_cost_usd = ceiling_in(input)
            .or(self.snapshot.ceiling)
            .unwrap_or(DEFAULT_CEILING_USD);
        self.last_workflow = Some(workflow.clone());
        // The workflow's own declared inputs: a required one with no
        // default is asked, in the product, before the run is requested —
        // the engine would refuse the launch (NIKA-1708) otherwise.
        let given = inline_vars(input);
        let needed: Vec<String> = required_inputs_of(&root, &workflow)
            .into_iter()
            .filter(|name| !given.iter().any(|v| v.starts_with(&format!("{name}="))))
            .collect();
        let inputs = RunInputs {
            workflow: workflow.clone(),
            max_cost_usd,
            needed,
            given,
        };
        self.remember(input, "(run requested)");
        Some(self.request_or_ask(inputs))
    }

    /// The run request when every declared input is bound; the next
    /// input's question otherwise (the next line answers it).
    fn request_or_ask(&mut self, mut inputs: RunInputs) -> TurnOutcome {
        if let Some(name) = inputs.needed.first().cloned() {
            let question = input_question(&inputs.workflow, &name, inputs.needed.len());
            self.intent.unresolved =
                vec![format!("input `{name}` of `{}`", inputs.workflow.display())];
            self.run_inputs = Some(inputs);
            return TurnOutcome::Question {
                key: format!("input.{name}"),
                question,
            };
        }
        inputs.needed.clear();
        let report = if inputs.given.is_empty() {
            format!("check · `{}` · clean ✔", inputs.workflow.display())
        } else {
            format!(
                "check · `{}` · clean ✔ · inputs {}",
                inputs.workflow.display(),
                inputs.given.join(" · ")
            )
        };
        TurnOutcome::RunRequested {
            report,
            run: RunRequest {
                workflow: inputs.workflow,
                vars: inputs.given,
                max_cost_usd: inputs.max_cost_usd,
            },
        }
    }

    /// The declared input the next line binds, when a run waits on one.
    #[must_use]
    pub fn pending_input(&self) -> Option<&str> {
        self.run_inputs
            .as_ref()
            .and_then(|r| r.needed.first())
            .map(String::as_str)
    }

    /// The human's line as the value of the input the run waits on; a
    /// cancel word drops the run request; an empty line is not a value.
    pub(super) fn answer_input_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(mut inputs) = self.run_inputs.take() else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no run waits on an input",
            ));
        };
        if is_cancel(line) {
            self.intent.unresolved.clear();
            self.remember(line, "(run request discarded)");
            return TurnOutcome::Facts(
                "run discarded · nothing ran · say « run it » again when the inputs are ready"
                    .to_owned(),
            );
        }
        let value = line.trim();
        if value.is_empty() {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::EmptyAnswer,
                "the input needs a value — nothing answers for you (`cancel` drops the run)",
            ));
        }
        let name = inputs.needed.remove(0);
        inputs.given.push(format!("{name}={value}"));
        self.intent.unresolved.clear();
        self.remember(line, &format!("(input {name} bound)"));
        self.request_or_ask(inputs)
    }
}

/// A run request waiting for the values of the workflow's own declared
/// inputs (required, no default): the next lines bind them, in order.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RunInputs {
    workflow: PathBuf,
    max_cost_usd: f64,
    needed: Vec<String>,
    given: Vec<String>,
}

/// The declared inputs the run must bind — from the engine's parser over
/// the bytes on disk, the same list `nika check` warns about.
fn required_inputs_of(root: &std::path::Path, workflow: &std::path::Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(root.join(workflow)) else {
        return Vec::new();
    };
    let Ok(wf) = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) else {
        return Vec::new();
    };
    nika_cli_host::display::check_render::required_inputs(&wf)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// `name=value` pairs the human wrote on the run line itself.
fn inline_vars(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .filter(|token| {
            token.split_once('=').is_some_and(|(k, v)| {
                !k.is_empty()
                    && !v.is_empty()
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        })
        .map(|token| token.trim_matches(|c| c == ',' || c == ';').to_owned())
        .collect()
}

/// The question for one declared input, in the product's words.
fn input_question(workflow: &std::path::Path, name: &str, remaining: usize) -> String {
    let more = if remaining > 1 {
        format!(" ({} more after this one)", remaining - 1)
    } else {
        String::new()
    };
    format!(
        "`{}` declares an input it needs before it runs: `{name}`{more}\n  reply on the next line with its value (`input.{name}`) · `cancel` drops the run",
        workflow.display()
    )
}

/// The question as the human reads it: the compiler's label, why it
/// cannot invent the value, what it could not settle, how to abandon.
fn question_text(question: &CompileQuestion, reasons: &[String]) -> String {
    let mut text = question.label.clone();
    if !question.why.is_empty() {
        text.push_str("\n  (");
        text.push_str(&question.why);
        text.push(')');
    }
    if !reasons.is_empty() {
        text.push_str("\n  what I could not settle:");
        for reason in reasons {
            text.push_str("\n    · ");
            text.push_str(reason);
        }
    }
    let _ = write!(
        text,
        "\n  reply on the next line (`{}`) · `cancel` drops this",
        question.key
    );
    text
}

/// An incomplete the human can act on: what the reader could not settle,
/// and the next safe step — never a substitute workflow.
fn honest_incomplete(out: &CompileOutcome, why: Option<&str>) -> String {
    let mut text = "I read this as work but cannot build it yet:".to_owned();
    let reasons = human_reasons(reasons(out));
    if reasons.is_empty() {
        text.push_str("\n  · the request names no operation I can read");
    }
    for reason in reasons {
        text.push_str("\n  · ");
        text.push_str(&reason);
    }
    text.push_str("\n  ");
    text.push_str(why.unwrap_or(
        "rephrase with what to read, what to produce and where to write it, e.g. « read ./docs, draft a digest and write it to ./digest.md »",
    ));
    text
}

/// The compiler's reasons a human can act on: its machine sentences (the
/// plan's own vocabulary, an unmapped part with nothing after the colon)
/// dropped, duplicates folded, the rest verbatim.
pub(crate) fn human_reasons(reasons: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for reason in reasons {
        let r = reason.trim();
        let machine = r.contains("semantic plan") || r.ends_with(": .") || r.ends_with(':');
        if machine || r.is_empty() || kept.iter().any(|k| k == r) {
            continue;
        }
        kept.push(r.to_owned());
    }
    kept
}

fn machinery(error: &AuthoringError) -> TurnOutcome {
    let class = match error {
        AuthoringError::Seat(_) => RefusalClass::IntelligenceRefused,
        AuthoringError::Compiler(_) | AuthoringError::Runtime(_) => RefusalClass::AuthoringRefused,
    };
    TurnOutcome::Refusal(Refusal::new(
        class,
        format!("{error} · nothing was written and nothing was substituted"),
    ))
}
