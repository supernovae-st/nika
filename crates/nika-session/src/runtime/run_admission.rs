// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run admission: an explicit run line (« run it » · « run brief.nika with a ceiling of 0.05 »)
//! becomes a run request only after a clean check on disk, the money admitted and every declared
//! input bound, one question per missing input (moved whole out of `authoring.rs`). A run refused
//! only because no ceiling can be proved asks the person for its ceiling first (`ceiling.rs`).

mod ceiling;
mod choices;

use std::fmt::Write as _;
use std::path::PathBuf;

use nika_onboard::routing::run_options::{self, inline_vars, input_question, run_line_is_plain};

use super::authoring::run_prefix;
use super::money_gate::Unadmitted;
use super::{SessionRuntime, TurnOutcome, ceiling_in, named_files};
use crate::authoring::is_cancel;
use crate::change::{RunRequest, Witness, check_with_access};
use crate::outcome::{Refusal, RefusalClass};
use crate::turn::{SessionPhase, TurnAct};

impl SessionRuntime {
    /// Whether this turn reaches only Run validation, without Session cognition: a plain run line
    /// typed at a run's ceiling question too, a request of its own (`ceiling.rs`).
    pub(super) fn local_run_line(&self, input: &str) -> bool {
        !self.waiting_cost_choice()
            && self.authoring.is_none()
            && (self.run_inputs.is_none() || self.pending_run_ceiling().is_some())
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
        if let Some(refused) = self.run_held(input) {
            return Some(refused);
        }
        // The run grammar is closed: the verb, the workflow named or « it »,
        // a ceiling. A line that carries more (« run it, but only on
        // Fridays ») is not a run: its act is a bounded decision, and a
        // change comes before any run.
        if !run_line_is_plain(&lower) {
            // That decision reaches cognition: a restored exposure refuses it first.
            if let Some(refused) = self.run_unconfirmed(input, ceiling) {
                return Some(refused);
            }
            if let Some(held) = self.hold_for_knowledge(input, self.routes_by_model()) {
                return Some(held);
            }
            let decision = self.classify(SessionPhase::Idle, input);
            return match decision.act {
                TurnAct::RequestRun if decision.secondary.is_empty() => {
                    Some(self.run_plain(&line, ceiling, access_pin))
                }
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
        let references = named_files(input);
        let named = references
            .iter()
            .map(PathBuf::from)
            .find(|p| root.join(p).is_file());
        let target = named
            .or_else(|| self.last_workflow.clone())
            .map(|path| (path, None))
            .or_else(|| {
                references
                    .is_empty()
                    .then(|| self.observed_run_target())
                    .flatten()
                    .map(|(path, witness)| (path, Some(witness)))
            });
        let Some(target) = target else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "nothing to run — name a workflow file (« run brief.nika »), or describe the work and Nika builds one first",
            ));
        };
        self.admit_bound_run(input, target, ceiling, access_pin, inline_vars(input))
    }

    /// Where the ceiling a run was just admitted under comes from, in words: the amount stated
    /// for this run (`stated`), else the decision the money law restored or defaulted.
    pub(super) fn ceiling_source(&self, stated: Option<f64>) -> &'static str {
        use crate::money::MonetarySource;
        if stated.is_some() {
            return "stated for this run";
        }
        if (self.money.current.as_ref()).is_some_and(|current| {
            (self.money.next_runs.as_ref())
                .is_some_and(|(_, set)| set.effective_usd == current.effective_usd)
        }) {
            return "the one you set for its next runs";
        }
        match self.money.current.as_ref().map(|decision| decision.source) {
            // Beside the file, never in it (live gauntlet wave 6 r09 B2, 2026-10-18: « saved with
            // the workflow » read against « a workflow never holds a dollar ceiling »).
            Some(MonetarySource::Explicit | MonetarySource::Override) => {
                "saved beside the workflow"
            }
            Some(MonetarySource::ProjectDefault) => "the project's default",
            _ => "the Session's default",
        }
    }

    /// A run its own last run says would exceed its ceiling, refused before it starts: the last
    /// run of these exact bytes in this conversation cost more than the ceiling, so this one
    /// would stop partway and leave its work undone (live gauntlet r09, 2026-10-17: a run under
    /// $0.001 spent $0.0035 before it stopped, and wrote nothing). Other bytes, or no run of
    /// them observed, are no evidence.
    fn over_its_last_run(&self, workflow: &std::path::Path, ceiling: f64) -> Option<TurnOutcome> {
        let (ran, sha, spent) = self.last_spend.as_ref()?;
        let root = &self.snapshot.root;
        if root.join(ran) != root.join(workflow) || *spent <= ceiling {
            return None;
        }
        let bytes = std::fs::read(root.join(workflow)).ok()?;
        if nika_event::source_id::sha256_hex(&bytes) != *sha {
            return None;
        }
        let lang = self.language;
        let offered =
            super::ceilings::offered(Some(super::ceilings::Evidence::LastRun(*spent)), lang);
        let suggest = super::ceilings::recommended(&offered)
            .unwrap_or("0.25")
            .to_owned();
        let usd = |cost: f64| nika_display::vocab::stated_usd(cost);
        let said = match lang {
            super::Lang::Fr => format!(
                "pas lancé : le dernier run de `{}` a coûté {} $, plus que le plafond de {} $ · celui-ci s'arrêterait en cours de route sans aller au bout · relevez le plafond (« lance-le avec un plafond de {} $ ») ou rendez-le moins cher",
                workflow.display(),
                usd(*spent).replace('.', ","),
                usd(ceiling).replace('.', ","),
                suggest.replace('.', ",")
            ),
            _ => format!(
                "not started: the last run of `{}` cost ${}, more than the ${} ceiling · this one would stop partway · raise the ceiling (« run it with a ceiling of ${suggest} ») or make it cheaper",
                workflow.display(),
                usd(*spent),
                usd(ceiling)
            ),
        };
        Some(TurnOutcome::Refusal(Refusal::new(
            RefusalClass::NotAllowed,
            said,
        )))
    }

    /// A run refused before any check: a paused gate waits.
    fn run_held(&mut self, said: &str) -> Option<TurnOutcome> {
        (self.pending_gate.is_some() || self.money.gate.is_some()).then(|| {
            let why = "a paused gate waits; answer it before requesting another Run";
            self.refuse_run_money(said, why)
        })
    }

    /// A run line bound for cognition, or a typed request (`save & run`), while a restored
    /// exposure has no stated ceiling to reconfirm it. A fresh plain local Run instead meets this
    /// law in its money admission, after its file is resolved and checked ([`Self::run_money`]).
    fn run_unconfirmed(&mut self, said: &str, ceiling: Option<f64>) -> Option<TurnOutcome> {
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
        // A typed request is not a fresh plain Run of an earlier Save: a paused gate holds first,
        // and a restored exposure needs a stated ceiling, asked once its file checks clean.
        if let Some(refused) = self.run_held(said) {
            return refused;
        }
        let target = (workflow, None);
        self.admit_checked(said, target, ceiling, access_pin, given, true)
    }

    /// The same admission, optionally restricted to the exact bytes a retained Run observed.
    /// Compare against the check's witness before adopting a target or asking for its inputs.
    pub(super) fn admit_bound_run(
        &mut self,
        said: &str,
        target: (PathBuf, Option<Witness>),
        ceiling: Option<f64>,
        access_pin: Option<String>,
        given: Vec<String>,
    ) -> TurnOutcome {
        self.admit_checked(said, target, ceiling, access_pin, given, false)
    }

    /// The admission itself; `typed` marks a typed request (`save & run`), which a saved
    /// decision cannot stand for under a restored exposure. A Run refused only because no
    /// ceiling can be proved asks the person for one; every other refusal stands.
    fn admit_checked(
        &mut self,
        said: &str,
        (workflow, expected): (PathBuf, Option<Witness>),
        ceiling: Option<f64>,
        access_pin: Option<String>,
        given: Vec<String>,
        typed: bool,
    ) -> TurnOutcome {
        if let Some(refused) = self.run_held(said) {
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
        if expected
            .as_ref()
            .is_some_and(|seen| audit.bytes.as_ref() != Some(seen))
        {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::StaleRevision,
                "the observed workflow changed since that run; name the current file explicitly before requesting another Run",
            ));
        }
        let money = match typed.then(|| self.run_unconfirmed(said, ceiling)).flatten() {
            Some(unconfirmed) => Err(Unadmitted::Unproven(unconfirmed)),
            None => self.run_money(said, &workflow, ceiling),
        };
        let ceiling = match money {
            Ok(amount) => {
                self.last_workflow = Some(workflow.clone());
                Ceiling::Admitted(amount, self.ceiling_source(ceiling))
            }
            // The person states the ceiling no saved decision can stand for: nothing runs before.
            Err(Unadmitted::Unproven(refused)) => {
                let root = &self.snapshot.root;
                let lang = self.language;
                Ceiling::Asked(ceiling::Asked::new(root, &workflow, said, &refused, lang))
            }
            Err(refused) => return refused.outcome(),
        };
        if let Ceiling::Admitted(amount, _) = &ceiling
            && let Some(refused) = self.over_its_last_run(&workflow, *amount)
        {
            return refused;
        }
        // The workflow's own declared inputs: a required one with no
        // default is asked, in the product, before the run is requested —
        // the engine would refuse the launch (NIKA-1708) otherwise.
        let needed: Vec<String> = required_inputs_of(&root, &workflow)
            .into_iter()
            .filter(|name| !given.iter().any(|v| v.starts_with(&format!("{name}="))))
            .collect();
        if matches!(ceiling, Ceiling::Admitted(..)) {
            self.remember(said, "(run requested)");
        }
        let inputs = RunInputs {
            admission: choices::next_admission(),
            said: said.to_owned(),
            workflow,
            ceiling,
            access_pin,
            needed,
            given,
            world: audit.world,
            checked: (audit.bytes, audit.closure),
        };
        self.request_or_ask(inputs)
    }

    /// The run request when its ceiling is admitted and every declared input is bound; the
    /// ceiling's question first, then the next input's (the next line answers it).
    fn request_or_ask(&mut self, mut inputs: RunInputs) -> TurnOutcome {
        let (max_cost_usd, source) = match &inputs.ceiling {
            Ceiling::Admitted(amount, source) => (*amount, *source),
            Ceiling::Asked(_) => return self.ask_ceiling(inputs),
        };
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
        // The report reads in the person's language (live gauntlet wave 6 r09, 2026-10-18:
        // « check · … · clean ✔ · ceiling $0.10 (saved with the workflow) » in French).
        let lang = self.language;
        let mut report = format!(
            "{} · `{}` · {}",
            lang.pick("check", "vérification"),
            inputs.workflow.display(),
            lang.pick("clean ✔", "propre ✔")
        );
        if !inputs.given.is_empty() {
            let named = lang.pick("inputs", "entrées");
            let _ = write!(report, " · {named} {}", inputs.given.join(" · "));
        }
        if let Some(pin) = &inputs.access_pin {
            let access = lang.pick("access", "accès");
            let explicit = lang.pick("explicit", "explicite");
            let _ = write!(report, " · {access} {pin} ({explicit})");
        }
        // Every run says the ceiling it runs under, as it was set, and where it comes from.
        let stated = nika_display::vocab::stated_usd(max_cost_usd);
        let source = if lang == crate::Lang::Fr {
            source_fr(source)
        } else {
            source
        };
        let ceiling = lang.pick("ceiling", "plafond");
        let _ = write!(report, " · {ceiling} ${stated} ({source})");
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
                max_cost_usd,
                access_pin: inputs.access_pin,
                bytes: inputs.checked.0.map(Box::new),
                closure: inputs.checked.1.map(Box::new),
            },
        }
    }

    /// The declared input the next line binds, when a run waits on one.
    #[must_use]
    pub fn pending_input(&self) -> Option<&str> {
        self.run_inputs.as_ref().and_then(RunInputs::first_needed)
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
        // Its ceiling is asked before any input.
        if inputs.asked().is_some() {
            return self.answer_ceiling_unrecorded(inputs, line);
        }
        // Its « why? » is the turn's (`read_only_turn`): it never reaches here.
        if is_cancel(line) {
            self.intent.unresolved.clear();
            self.remember(line, "(run request discarded)");
            return TurnOutcome::Facts(
                self.language
                    .pick(
                        "run discarded · nothing ran · say « run it » again when the inputs are ready",
                        "run abandonné · rien n'a tourné · dites « lance-le » quand les entrées sont prêtes",
                    )
                    .to_owned(),
            );
        }
        // A command-shaped line is never an input's value.
        if let Some(text) = super::protocol::unserved_command(line) {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "{text}\n  {}",
                    self.language.pick(
                        "the input still waits · reply on the next line · `cancel` drops the run",
                        "l'entrée attend toujours · répondez sur la ligne suivante · « annuler » abandonne le run",
                    )
                ),
            ));
        }
        let value = line.trim();
        if value.is_empty() {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::EmptyAnswer,
                self.language.pick(
                    "the input needs a value — nothing answers for you (`cancel` drops the run)",
                    "l'entrée a besoin d'une valeur — rien ne répond à votre place (« annuler » abandonne le run)",
                ),
            ));
        }
        // Finite declarations keep their canonical JSON literal through Run parsing.
        // Open inputs keep the existing quoted-protocol escape and raw-text behavior.
        let value = match inputs.literal(&self.snapshot.root, value) {
            Ok(value) => value,
            Err(refusal) => {
                self.run_inputs = Some(inputs);
                return TurnOutcome::Refusal(refusal);
            }
        };
        let name = inputs.needed.remove(0);
        inputs.given.push(format!("{name}={value}"));
        self.intent.unresolved.clear();
        self.remember(line, &format!("(input {name} bound)"));
        self.request_or_ask(inputs)
    }
}

/// A run's ceiling: the one the money law admitted (its amount, and where it comes from in the
/// person's words), or the one asked of the person because none could be proved
/// ([`crate::work::Waiting::RunCeiling`]).
#[derive(Clone, Debug, PartialEq)]
enum Ceiling {
    Admitted(f64, &'static str),
    Asked(ceiling::Asked),
}

/// A run request waiting for its ceiling, or for the values of the workflow's own declared
/// inputs (required, no default): the next lines bind them, in order.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RunInputs {
    /// One admitted request, even when another request has identical content.
    /// Presentation identity only; never persisted or used to authorize a Run.
    admission: u64,
    /// The line that asked the run: the money law reads it again with the ceiling the person
    /// states, through its explicit path.
    said: String,
    workflow: PathBuf,
    ceiling: Ceiling,
    access_pin: Option<String>,
    needed: Vec<String>,
    given: Vec<String>,
    /// Where the bytes the check cleared for this request reach (`work().requested`).
    world: crate::world::World,
    /// The witness of those exact bytes and the closure of their world: the run is bound to both.
    checked: (Option<Witness>, Option<crate::change::Closure>),
}

impl RunInputs {
    /// The input the next line binds, when one is still needed and the ceiling is not asked.
    pub(super) fn first_needed(&self) -> Option<&str> {
        match self.ceiling {
            Ceiling::Admitted(..) => self.needed.first().map(String::as_str),
            Ceiling::Asked(_) => None,
        }
    }

    /// The ceiling asked of the person, while it is.
    fn asked(&self) -> Option<&ceiling::Asked> {
        match &self.ceiling {
            Ceiling::Asked(asked) => Some(asked),
            Ceiling::Admitted(..) => None,
        }
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

/// Where a run's ceiling comes from ([`SessionRuntime::ceiling_source`]), in French.
fn source_fr(source: &'static str) -> &'static str {
    match source {
        "stated for this run" => "indiqué pour ce run",
        "the one you set for its next runs" => "celui que vous avez fixé pour ses prochains runs",
        "saved beside the workflow" => "enregistré à côté du workflow, hors de son fichier",
        "the project's default" => "le défaut du projet",
        "the Session's default" => "le défaut de la session",
        other => other,
    }
}
