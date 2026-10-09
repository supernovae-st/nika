// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run admission: an explicit run line (« run it » · « run brief.nika with a ceiling of 0.05 »)
//! becomes a run request only after a clean check on disk, the money admitted and every declared
//! input bound, one question per missing input (moved whole out of `authoring.rs`).

use std::fmt::Write as _;
use std::path::PathBuf;

use nika_onboard::routing::run_options::{self, inline_vars, input_question, run_line_is_plain};

use super::authoring::run_prefix;
use super::{SessionRuntime, TurnOutcome, ceiling_in, named_files};
use crate::authoring::is_cancel;
use crate::change::{RunRequest, Witness, check_with_access};
use crate::outcome::{Refusal, RefusalClass};
use crate::turn::{SessionPhase, TurnAct};

impl SessionRuntime {
    /// Whether this turn reaches only Run validation, without Session cognition.
    pub(super) fn local_run_line(&self, input: &str) -> bool {
        !self.waiting_cost_choice()
            && self.authoring.is_none()
            && self.run_inputs.is_none()
            && self.activation.is_none()
            && run_prefix(input).is_some_and(|_| {
                ceiling_in(input).is_err()
                    || run_options::parse(input)
                        .is_none_or(|(line, _)| run_line_is_plain(&line.to_lowercase()))
            })
    }

    /// An explicit run line — `run it` · `run brief.nika with a ceiling of
    /// 0.05` · `test it now` — runs a workflow the human named or the one
    /// last accepted, only when its check on disk is clean. `None` when
    /// the line is not a run line.
    pub(super) fn run_turn(&mut self, input: &str) -> Option<TurnOutcome> {
        run_prefix(input)?;
        let Some((line, access_pin)) = run_options::parse(input) else {
            return Some(self.refuse_run_money(input, "invalid or duplicate Run option — use --access <pin> and --max-cost-usd <amount> once each"));
        };
        let lower = line.to_lowercase();
        // A label from the conversational router cannot turn an invalid
        // amount (or an unqualified time/count) into permission to run.
        let ceiling = match ceiling_in(input) {
            Ok(ceiling) => ceiling,
            Err(reason) => return Some(self.refuse_run_money(input, reason)),
        };
        if let Some(refused) = self.run_refused(input, ceiling) {
            return Some(refused);
        }
        // The run grammar is closed: the verb, the workflow named or « it »,
        // a ceiling. A line that carries more (« run it, but only on
        // Fridays ») is not a run: its act is a bounded decision, and a
        // change comes before any run.
        if !run_line_is_plain(&lower) {
            if let Some(held) = self.hold_for_knowledge(input, self.routes_by_model()) {
                return Some(held);
            }
            return match self.classify(SessionPhase::Idle, input).act {
                TurnAct::RequestRun => Some(self.run_plain(&line, ceiling, access_pin)),
                TurnAct::Modify | TurnAct::Mixed => Some(TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::WrongState,
                    "a run with a change in it — say the change first (in a sentence), review the new workflow, then « run it »",
                ))),
                _ => None,
            };
        }
        Some(self.run_plain(&line, ceiling, access_pin))
    }

    /// The closed run line: the verb, the file or the last accepted
    /// workflow, the ceiling.
    fn run_plain(
        &mut self,
        input: &str,
        ceiling: Option<f64>,
        access_pin: Option<String>,
    ) -> TurnOutcome {
        let root = self.snapshot.root.clone();
        let named = named_files(input)
            .into_iter()
            .map(PathBuf::from)
            .find(|p| root.join(p).is_file());
        let Some(workflow) = named.or_else(|| self.last_workflow.clone()) else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "nothing to run — name a workflow file (« run brief.nika »), or describe the work and Nika builds one first",
            ));
        };
        self.admit_run(input, workflow, ceiling, access_pin, inline_vars(input))
    }

    /// A run refused before any check: a paused gate waits, or a restored exposure has no stated
    /// ceiling to reconfirm it.
    fn run_refused(&mut self, said: &str, ceiling: Option<f64>) -> Option<TurnOutcome> {
        if self.pending_gate.is_some() || self.money.gate.is_some() {
            let why = "a paused gate waits; answer it before requesting another Run";
            return Some(self.refuse_run_money(said, why));
        }
        (self.money.reconfirm && ceiling.is_none()).then(|| {
            let why = self.restored_refusal();
            self.refuse_run_money(said, &why)
        })
    }

    /// While a run's cost review waits, a `line` that answers another state (a consent, a gate,
    /// by any door) is refused as `submit` refuses it: the review answers first
    /// ([`Self::waiting`]), so nothing is saved, requested or resumed past it, and everything
    /// keeps waiting. Leaving stays one line away.
    pub(super) fn review_first(&self, line: &str) -> Option<TurnOutcome> {
        let refused = Refusal::new(RefusalClass::StaleRevision, super::work::REVIEW_NOT_SHOWN);
        (self.waiting_review().is_some() && !super::is_quit(line))
            .then_some(TurnOutcome::Refusal(refused))
    }

    /// The run of `workflow` under the one admission every run request meets, typed: whatever
    /// asked it (a run line, a `save & run`), its values reach the check, the money and the inputs
    /// exactly. A rehearsed copy runs only over its rehearsed world, the check on disk is clean
    /// under `access_pin`, the money is admitted (a stated `ceiling`, else the saved decision), and
    /// each declared input not `given` is asked; then the run request. `said` is the human's line.
    pub(super) fn admit_run(
        &mut self,
        said: &str,
        workflow: PathBuf,
        ceiling: Option<f64>,
        access_pin: Option<String>,
        given: Vec<String>,
    ) -> TurnOutcome {
        if let Some(refused) = self.run_refused(said, ceiling) {
            return refused;
        }
        let root = self.snapshot.root.clone();
        // A rehearsed copy runs only over the bytes and the world it was rehearsed on.
        if let Some(withdrawn) = self.rehearsed_at_run(&workflow) {
            return withdrawn;
        }
        let audit = check_with_access(&root, &workflow, access_pin.as_deref());
        if !audit.clean {
            let mut text = format!(
                "check · `{}` · findings ✖ — the run was not started",
                workflow.display()
            );
            for f in &audit.findings {
                text.push_str("\n  · ");
                text.push_str(f);
            }
            return TurnOutcome::Facts(text);
        }
        let max_cost_usd = match self.run_money(said, &workflow, ceiling) {
            Ok(amount) => amount,
            Err(refusal) => return refusal,
        };
        self.last_workflow = Some(workflow.clone());
        // The workflow's own declared inputs: a required one with no
        // default is asked, in the product, before the run is requested —
        // the engine would refuse the launch (NIKA-1708) otherwise.
        let needed: Vec<String> = required_inputs_of(&root, &workflow)
            .into_iter()
            .filter(|name| !given.iter().any(|v| v.starts_with(&format!("{name}="))))
            .collect();
        let inputs = RunInputs {
            workflow,
            max_cost_usd,
            access_pin,
            needed,
            given,
            world: audit.world,
            checked: (audit.bytes, audit.closure),
        };
        self.remember(said, "(run requested)");
        self.request_or_ask(inputs)
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
        let mut report = if inputs.given.is_empty() {
            format!("check · `{}` · clean ✔", inputs.workflow.display())
        } else {
            format!(
                "check · `{}` · clean ✔ · inputs {}",
                inputs.workflow.display(),
                inputs.given.join(" · ")
            )
        };
        if let Some(pin) = &inputs.access_pin {
            let _ = write!(report, " · access {pin} (explicit)");
        }
        self.requested_run = Some(crate::work::RequestedRun::new(
            inputs.workflow.clone(),
            &inputs.given,
            std::mem::take(&mut inputs.world),
        ));
        TurnOutcome::RunRequested {
            report,
            run: RunRequest {
                workflow: inputs.workflow,
                vars: inputs.given,
                max_cost_usd: inputs.max_cost_usd,
                access_pin: inputs.access_pin,
                bytes: inputs.checked.0.map(Box::new),
                closure: inputs.checked.1.map(Box::new),
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
        // Its « why? » is the turn's (`read_only_turn`): it never reaches here.
        if is_cancel(line) {
            self.intent.unresolved.clear();
            self.remember(line, "(run request discarded)");
            return TurnOutcome::Facts(
                "run discarded · nothing ran · say « run it » again when the inputs are ready"
                    .to_owned(),
            );
        }
        // A command-shaped line is never an input's value.
        if let Some(text) = super::protocol::unserved_command(line) {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "{text}\n  the input still waits · reply on the next line · `cancel` drops the run"
                ),
            ));
        }
        let value = line.trim();
        if value.is_empty() {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::EmptyAnswer,
                "the input needs a value — nothing answers for you (`cancel` drops the run)",
            ));
        }
        // A value in quotes is its content: the escape for a word the protocol would take.
        let value = serde_json::from_str::<String>(value).unwrap_or_else(|_| value.to_owned());
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
    access_pin: Option<String>,
    needed: Vec<String>,
    given: Vec<String>,
    /// Where the bytes the check cleared for this request reach (`work().requested`).
    world: crate::world::World,
    /// The witness of those exact bytes and the closure of their world: the run is bound to both.
    checked: (Option<Witness>, Option<crate::change::Closure>),
}

impl RunInputs {
    /// The input the next line binds, when one is still needed.
    pub(super) fn first_needed(&self) -> Option<&str> {
        self.needed.first().map(String::as_str)
    }

    /// The workflow the run waits to start.
    pub(super) fn workflow(&self) -> &std::path::Path {
        &self.workflow
    }

    /// How many inputs still wait, this one included.
    pub(super) fn remaining(&self) -> usize {
        self.needed.len()
    }
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
