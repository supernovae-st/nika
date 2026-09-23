// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Activation: the schedule a request asked for becomes a DECLARED beat in
//! `nika.yaml` only through the human's own gestures — the values the
//! request did not state are asked (the time zone · what « missed » means
//! · the ceiling per run), the declaration is reviewed as a project change
//! and saved on consent. Declared is never active: a firer on this machine
//! must run it (`nika serve`, or the OS unit `nika arm --emit … --write`),
//! and only the firer's own record proves a beat fired. Saving a workflow
//! activates nothing; saving the declaration activates nothing either.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_onboard::compile::{TriggerKind, TriggerRequirement, TriggerStatus};

use super::{SessionRuntime, TurnOutcome};
use crate::authoring::{is_cancel, is_why};
use crate::change::{ProjectChange, ProjectChangeSet, Witness};
use crate::outcome::{Refusal, RefusalClass};

/// The three values a declared beat needs that a sentence rarely states.
const TIMEZONE: &str = "project.timezone";
const MISSED: &str = "project.missed";
const CEILING: &str = "project.ceiling";

/// An activation under way: the workflow, the trigger the compiler read,
/// the answers so far and the questions still open, in order.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Activation {
    pub(super) workflow: PathBuf,
    pub(super) trigger: TriggerRequirement,
    pub(super) answers: BTreeMap<&'static str, String>,
    pub(super) needed: Vec<&'static str>,
}

impl Activation {
    fn new(workflow: PathBuf, trigger: TriggerRequirement) -> Self {
        Self {
            workflow,
            trigger,
            answers: BTreeMap::new(),
            needed: vec![TIMEZONE, MISSED, CEILING],
        }
    }

    /// The key the next line answers.
    pub(super) fn current(&self) -> Option<&'static str> {
        self.needed.first().copied()
    }

    /// The question for the current key, in the human's words, with the
    /// consequence of the answer.
    pub(super) fn question(&self) -> String {
        let when = self
            .trigger
            .source_hint
            .as_deref()
            .unwrap_or("this schedule");
        let at = self.trigger.at.as_deref().unwrap_or("the scheduled time");
        let text = match self.current() {
            Some(TIMEZONE) => format!(
                "Which time zone should « {when} » follow?\n  answer an IANA name, e.g. Europe/Paris · this machine's clock is not assumed\n  (this fixes when {at} falls, and how a summer/winter change is read)"
            ),
            Some(MISSED) => format!(
                "If this machine is off at {at}, what should happen?\n  1  run once when it comes back\n  2  skip the missed run\n  3  replay every missed occurrence\n  (this changes what may run later · it does not change the workflow itself)"
            ),
            Some(CEILING) => "What is the ceiling per scheduled run, in USD? (e.g. 0.20)\n  a run that would cost more is refused before it starts · the ceiling is yours, never a default".to_owned(),
            _ => String::new(),
        };
        let remaining = self.needed.len();
        let more = if remaining > 1 {
            format!(" ({} more after this one)", remaining - 1)
        } else {
            String::new()
        };
        format!(
            "{text}\n  reply on the next line{more} · `cancel` drops the activation · `why?` explains"
        )
    }

    /// Bind the current key to a line; the shape refusal names the fix.
    pub(super) fn answer(&mut self, line: &str) -> Result<(), String> {
        let key = self
            .current()
            .ok_or_else(|| "no activation question waits".to_owned())?;
        let value = line.trim();
        let bound = match key {
            TIMEZONE => {
                let ok = value.contains('/')
                    && value
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+'));
                if !ok {
                    return Err(format!(
                        "`{value}` is not a time zone name — the form is `Area/City`, e.g. Europe/Paris, America/Montreal"
                    ));
                }
                value.to_owned()
            }
            MISSED => match value.to_lowercase().as_str() {
                "1" | "once" | "run once" | "catch up once" | "rattraper-une-fois"
                | "rattraper une fois" => "rattraper-une-fois".to_owned(),
                "2" | "skip" | "skip it" | "sauter" | "saute" => "sauter".to_owned(),
                "3" | "replay" | "replay every" | "replay all" | "rattraper" | "tout rattraper" => {
                    "rattraper".to_owned()
                }
                other => {
                    return Err(format!(
                        "`{other}` is not one of the three — answer 1 (run once when back), 2 (skip) or 3 (replay every missed one)"
                    ));
                }
            },
            CEILING => {
                let cleaned = value.trim_start_matches('$').replace(',', ".");
                match cleaned.parse::<f64>() {
                    Ok(v) if v.is_finite() && v > 0.0 => format!("{v}"),
                    _ => {
                        return Err(format!(
                            "`{value}` is not a positive amount in USD — e.g. 0.20"
                        ));
                    }
                }
            }
            _ => return Err("no activation question waits".to_owned()),
        };
        self.answers.insert(key, bound);
        self.needed.remove(0);
        Ok(())
    }

    /// The cron form of the trigger the compiler read, with the answered
    /// zone in the expression (the cadence grammar wants it there); `None`
    /// when the words cannot be turned into a cadence yet.
    pub(super) fn cadence(&self) -> Option<String> {
        let tz = self.answers.get(TIMEZONE)?;
        let (hour, minute) = self
            .trigger
            .at
            .as_deref()
            .and_then(|at| {
                let (h, m) = at.split_once(':')?;
                Some((h.parse::<u8>().ok()?, m.parse::<u8>().ok()?))
            })
            .unwrap_or((8, 0));
        let days = match self.trigger.cadence.as_deref()? {
            "daily" | "every day" | "quotidien" => "*",
            "weekdays" | "every weekday" | "semaine" => "1-5",
            "weekly" | "hebdomadaire" => "1",
            "hourly" => return Some(format!("TZ={tz} 0 * * * *")),
            _ => return None,
        };
        Some(format!("TZ={tz} {minute} {hour} * * {days}"))
    }

    /// The `arm:` entry, as the project file's grammar writes it.
    pub(super) fn entry(&self, cadence: &str) -> String {
        let plafond = self.answers.get(CEILING).map_or("0", String::as_str);
        let manque = self.answers.get(MISSED).map_or("sauter", String::as_str);
        format!(
            "  - workflow: {}\n    cadence: \"{cadence}\"\n    plafond: {plafond}\n    manqué: {manque}\n",
            self.workflow.display()
        )
    }
}

impl SessionRuntime {
    /// « activate » — the schedule the last accepted workflow asked for
    /// becomes a declaration to review: the three values the sentence did
    /// not state are asked first, one line each.
    pub(super) fn activate_turn(&mut self) -> TurnOutcome {
        let Some(workflow) = self.last_workflow.clone() else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "nothing to activate — describe the work with its schedule, save the workflow, then activate",
            ));
        };
        let Some(trigger) = self.last_trigger.clone() else {
            return TurnOutcome::Facts(format!(
                "`{}` asked for no schedule: it runs when you ask (« run it ») · to schedule it, say when in your request (« chaque matin à 8h », « every weekday at 8 ») and Nika keeps that beside the workflow",
                workflow.display()
            ));
        };
        if trigger.kind != TriggerKind::Schedule || trigger.status != TriggerStatus::RequiresBinding
        {
            return TurnOutcome::Facts(format!(
                "the request asked for a trigger Nika cannot declare in `nika.yaml` yet ({:?}) · the workflow runs when you ask",
                trigger.kind
            ));
        }
        let activation = Activation::new(workflow, trigger);
        let question = activation.question();
        self.intent.unresolved =
            vec!["the schedule's time zone, missed policy and ceiling".to_owned()];
        self.activation = Some(activation);
        TurnOutcome::Question {
            key: TIMEZONE.to_owned(),
            question,
        }
    }

    /// The human's line as the value of the activation question that
    /// waits; `cancel` drops the activation; `why?` explains and holds.
    pub(super) fn answer_activation_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(mut activation) = self.activation.take() else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no activation question waits",
            ));
        };
        if is_why(line) {
            let text = format!(
                "Activating declares the schedule in `nika.yaml`, beside `{}` — the workflow itself is already saved and checked. Declared is not active: a firer on this machine must run it (`nika serve`, or the OS unit `nika arm --emit launchd --write`).\n  the question still waits · reply on the next line · `cancel` drops the activation",
                activation.workflow.display()
            );
            self.activation = Some(activation);
            return TurnOutcome::Aside(text);
        }
        if is_cancel(line) {
            self.intent.unresolved.clear();
            return TurnOutcome::Facts(
                "activation dropped · nothing was declared · the workflow stays saved and runs when you ask".to_owned(),
            );
        }
        if let Err(why) = activation.answer(line) {
            self.activation = Some(activation);
            return TurnOutcome::Refusal(Refusal::new(RefusalClass::EmptyAnswer, why));
        }
        if let Some(key) = activation.current() {
            let question = activation.question();
            self.activation = Some(activation);
            return TurnOutcome::Question {
                key: key.to_owned(),
                question,
            };
        }
        self.intent.unresolved.clear();
        self.propose_declaration(&activation)
    }

    /// The declaration as a project change to review: the `arm:` entry
    /// appended to `nika.yaml` (created when absent), the exact bytes, and
    /// what a yes does NOT do.
    fn propose_declaration(&mut self, activation: &Activation) -> TurnOutcome {
        let Some(cadence) = activation.cadence() else {
            return TurnOutcome::Facts(format!(
                "the words « {} » cannot be turned into a cadence yet — write the `cadence:` line yourself in `nika.yaml` (`TZ=Europe/Paris 0 8 * * *`), Nika lists it with `nika arm`",
                activation
                    .trigger
                    .source_hint
                    .as_deref()
                    .unwrap_or("the schedule")
            ));
        };
        if let Err(why) = nika_cadence::registry::Cadence::parse(&cadence) {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "the cadence Nika derived (`{cadence}`) is refused by the schedule grammar: {why}"
                ),
            ));
        }
        let entry = activation.entry(&cadence);
        let root = self.snapshot.root.clone();
        let path = root.join("nika.yaml");
        let (change, content) = if let Ok(bytes) = std::fs::read(&path) {
            let mut content = String::from_utf8_lossy(&bytes).into_owned();
            if content.lines().any(|l| l.trim_end() == "arm:") {
                return TurnOutcome::Facts(format!(
                    "`nika.yaml` already declares an `arm:` list — Nika does not rewrite a list it did not write. Add this entry under it yourself, then `nika arm` lists it:\n{entry}"
                ));
            }
            if !content.ends_with('\n') {
                content.push('\n');
            }
            let _ = write!(content, "arm:\n{entry}");
            (
                ProjectChange::UpdateProjectFile {
                    before: Witness::of(&bytes),
                    content: content.clone(),
                },
                content,
            )
        } else {
            let name = root
                .file_name()
                .and_then(|n| n.to_str())
                .map(kebab)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "project".to_owned());
            let content = format!("nika: {name}\narm:\n{entry}");
            (
                ProjectChange::CreateProjectFile {
                    content: content.clone(),
                },
                content,
            )
        };
        let set = ProjectChangeSet::project_change(
            &root,
            &self.intent.goal.clone().unwrap_or_default(),
            change,
        );
        let bytes = set.preview();
        let id = crate::ProposalId::of(&bytes);
        let mut preview = format!(
            "Nika proposes to declare the schedule in `nika.yaml`:\nRuns\n  {} · {cadence}\n  if missed · {}\n  ceiling · ${} per scheduled run\nChanges\n  {} `nika.yaml`\nDeclared is not active · after saving, a firer on this machine must run it: `nika serve` (resident), or the OS unit `nika arm --emit launchd --write` · `nika arm` lists what is declared and proves what fired\n",
            activation
                .trigger
                .source_hint
                .as_deref()
                .unwrap_or("the schedule"),
            activation.answers.get(MISSED).map_or("", String::as_str),
            activation.answers.get(CEILING).map_or("", String::as_str),
            if path.exists() { "~" } else { "+" }
        );
        let _ = writeln!(preview, "  ┌─ `nika.yaml`");
        for line in content.lines() {
            let _ = writeln!(preview, "  │ {line}");
        }
        let _ = writeln!(
            preview,
            "  └─\n  identity {id} · `yes` writes these exact bytes · `no` discards"
        );
        self.remember("(activation)", &format!("(proposed {id})"));
        self.pending = Some(set);
        TurnOutcome::Proposal { id, preview }
    }
}

/// A kebab-case project name from a folder name.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// The few words that ask to activate the schedule.
#[must_use]
pub(super) fn is_activate(line: &str) -> bool {
    let word = line.trim().trim_end_matches(['!', '.', ' ']).to_lowercase();
    matches!(
        word.as_str(),
        "activate"
            | "/activate"
            | "activate it"
            | "activate the schedule"
            | "arm it"
            | "arm the schedule"
            | "active"
            | "active-le"
            | "active le"
            | "active la planification"
            | "arme"
            | "arme-le"
    )
}

/// Where a declared beat stands: declared in the file, and whether this
/// machine's firer left a record of it — from the registry the file IS and
/// the sidecar the firer writes, never from the session's memory.
pub(super) fn declared_state(root: &Path, workflow: &Path) -> Option<String> {
    let (path, active, cadence) = declared_entry(root, workflow)?;
    let base = if active {
        format!(
            "Declared · `{path}` · cadence {cadence} · not proven active: a firer must run on this machine"
        )
    } else {
        format!("Declared · suspended (`actif: false`) · `{path}`")
    };
    Some(base)
}

/// The entry that declares a workflow in `nika.yaml`: the project file's
/// path, whether the beat is active (`actif`, true unless written false)
/// and its cadence — `None` when nothing declares it.
pub(super) fn declared_entry(root: &Path, workflow: &Path) -> Option<(String, bool, String)> {
    let (path, project) = nika_vocab::project::discover(root).ok().flatten()?;
    let entry = project
        .arm()
        .iter()
        .find(|e| Path::new(&e.workflow) == workflow)?;
    Some((
        path.display().to_string(),
        entry.actif.unwrap_or(true),
        entry.cadence.clone(),
    ))
}
