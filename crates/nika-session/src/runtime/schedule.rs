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

use super::ceilings::{self, Evidence, last_cost};
use super::{Lang, SessionRuntime, TurnOutcome};
use crate::authoring::is_cancel;
use crate::change::{ProjectChange, ProjectChangeSet, Witness};
use crate::outcome::{Refusal, RefusalClass};
use crate::work::{Offer, Waiting};
pub(super) use nika_session_change::lang::schedule::{cadence_words, key_words, missed_words};
pub use nika_session_change::lang::schedule::{zone_place, zone_words};

use nika_session_change::lang::schedule::{
    CEILING, MISSED, TIMEZONE, ceiling_amount, delegates, missed_meaning,
};

/// An activation under way: the workflow, the trigger the compiler read,
/// the answers so far and the questions still open, in order.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Activation {
    pub(super) workflow: PathBuf,
    pub(super) trigger: TriggerRequirement,
    pub(super) answers: BTreeMap<&'static str, String>,
    pub(super) needed: Vec<&'static str>,
    /// What the newest succeeded run of the workflow cost, as its trace states it: the evidence
    /// a ceiling is recommended from, never a ceiling.
    pub(super) observed: Option<f64>,
    /// The language its questions are asked in: the person's.
    pub(super) lang: Lang,
}

impl Activation {
    fn new(
        workflow: PathBuf,
        trigger: TriggerRequirement,
        observed: Option<f64>,
        lang: Lang,
    ) -> Self {
        // A zone the person already chose for this schedule (the default they consented to with
        // its proposal, or their answer) is theirs: only what is still open is asked.
        let answers: BTreeMap<&'static str, String> = (trigger.timezone.iter())
            .map(|zone| (TIMEZONE, zone.clone()))
            .collect();
        let needed = [TIMEZONE, MISSED, CEILING]
            .into_iter()
            .filter(|key| !answers.contains_key(key))
            .collect();
        Self {
            workflow,
            trigger,
            answers,
            needed,
            observed,
            lang,
        }
    }

    /// The key the next line answers.
    pub(super) fn current(&self) -> Option<&'static str> {
        self.needed.first().copied()
    }

    /// « why? » beside the activation's question: what declaring does and does not do.
    pub(super) fn why(&self) -> String {
        let workflow = self.workflow.display();
        let nika = program();
        match self.lang {
            Lang::Fr => format!(
                "Activer déclare la planification dans `nika.yaml`, à côté de `{workflow}` — le workflow lui-même est déjà enregistré et vérifié. Déclaré n'est pas actif : un déclencheur doit tourner sur cette machine (`{nika} serve`, ou l'unité système `{nika} arm --emit launchd --write`).\n  la question attend toujours · répondez sur la ligne suivante · « annuler » abandonne l'activation"
            ),
            _ => format!(
                "Activating declares the schedule in `nika.yaml`, beside `{workflow}` — the workflow itself is already saved and checked. Declared is not active: a firer on this machine must run it (`{nika} serve`, or the OS unit `{nika} arm --emit launchd --write`).\n  the question still waits · reply on the next line · `cancel` drops the activation"
            ),
        }
    }

    /// The question for the current key, in the human's words, as a host that shows no options
    /// reads it: the sentence, each option or hint on its own line, the consequence of the
    /// answer, then how to reply.
    pub(super) fn question(&self) -> String {
        let (sentence, consequence) = self.said();
        let mut text = sentence;
        for line in self.lines() {
            let _ = write!(text, "\n  {line}");
        }
        let remaining = self.needed.len();
        let more = match (remaining > 1, self.lang) {
            (false, _) => String::new(),
            (true, Lang::Fr) => format!(" (encore {} après celle-ci)", remaining - 1),
            (true, _) => format!(" ({} more after this one)", remaining - 1),
        };
        let how = self.lang.pick(
            "reply on the next line{more} · `cancel` drops the activation · `why?` explains",
            "répondez sur la ligne suivante{more} · « annuler » abandonne l'activation · « pourquoi ? » explique",
        );
        let _ = write!(
            text,
            "\n  ({consequence})\n  {}",
            how.replace("{more}", &more)
        );
        text
    }

    /// The current key's sentence, and the consequence of its answer.
    fn said(&self) -> (String, String) {
        // A schedule with no stated hour is named in the person's language (live gauntlet wave 6
        // r05 B2: « il fixe quand tombe the scheduled time »).
        let when = (self.trigger.source_hint.as_deref())
            .unwrap_or_else(|| self.lang.pick("this schedule", "ce planning"));
        let at = (self.trigger.at.as_deref())
            .unwrap_or_else(|| self.lang.pick("the scheduled time", "l'heure prévue"));
        // The zone already chosen says where that time falls.
        let at = match self.answers.get(TIMEZONE) {
            Some(zone) => format!("{at} ({zone})"),
            None => at.to_owned(),
        };
        match (self.current(), self.lang) {
            (Some(TIMEZONE), Lang::Fr) => (
                format!("Quel fuseau horaire pour « {when} » ?"),
                format!("il fixe quand tombe {at}, et comment se lit un passage à l'heure d'été ou d'hiver"),
            ),
            (Some(TIMEZONE), _) => (
                format!("Which time zone should « {when} » follow?"),
                format!("this fixes when {at} falls, and how a summer/winter change is read"),
            ),
            (Some(MISSED), Lang::Fr) => (
                format!("Si cette machine est éteinte à {at}, que faut-il faire ?"),
                "cela change ce qui peut tourner plus tard · cela ne change pas le workflow lui-même"
                    .to_owned(),
            ),
            (Some(MISSED), _) => (
                format!("If this machine is off at {at}, what should happen?"),
                "this changes what may run later · it does not change the workflow itself"
                    .to_owned(),
            ),
            (Some(CEILING), Lang::Fr) => (
                "Quel plafond par run planifié, en dollars US ?".to_owned(),
                "un run qui coûterait plus est refusé avant de démarrer · le plafond est le vôtre, jamais une valeur par défaut".to_owned(),
            ),
            (Some(CEILING), _) => (
                "What is the ceiling per scheduled run, in USD?".to_owned(),
                "a run that would cost more is refused before it starts · the ceiling is yours, never a default".to_owned(),
            ),
            _ => (String::new(), String::new()),
        }
    }

    /// The current key's options and hints as text lines, for a host that shows no options.
    fn lines(&self) -> Vec<String> {
        match self.current() {
            Some(TIMEZONE) => vec![
                self.lang
                    .pick(
                        "answer an IANA name, e.g. Europe/Paris · this machine's clock is not assumed",
                        "répondez un fuseau, par exemple Paris ou Europe/Paris · l'horloge de cette machine n'est pas supposée",
                    )
                    .to_owned(),
            ],
            Some(MISSED) => (missed(self.lang).iter())
                .map(|offer| format!("{}  {}", offer.key, offer.label.to_lowercase()))
                .collect(),
            Some(CEILING) => {
                let mut lines: Vec<String> = (self.ceilings().into_iter())
                    .map(|(amount, why)| match why {
                        Some(why) => format!("{amount}  recommended · {why}"),
                        None => amount,
                    })
                    .collect();
                lines.push(
                    self.lang
                        .pick(
                            "or type another amount, e.g. 0.20",
                            "ou tapez un autre montant, par exemple 0.20",
                        )
                        .to_owned(),
                );
                lines
            }
            _ => Vec::new(),
        }
    }

    /// The finite choice the current question offers a host that shows its options natively:
    /// its key, the sentence and one short line (the options are the host's rows, never
    /// repeated as text), the options, and whether another typed value is admitted. A typed
    /// time zone offers none.
    pub(super) fn choices(&self) -> Option<(&'static str, String, Vec<Offer>, bool)> {
        let key = self.current()?;
        let (options, typed) = match key {
            MISSED => (missed(self.lang), false),
            CEILING => {
                let offers = (self.ceilings().into_iter())
                    .map(|(amount, why)| match why {
                        Some(why) => Offer::new(amount.clone(), format!("${amount} · {why}"), true),
                        None => Offer::new(amount.clone(), format!("${amount}"), false),
                    })
                    .collect();
                (offers, true)
            }
            _ => return None,
        };
        let (sentence, consequence) = self.said();
        let remaining = self.needed.len();
        let more = match (remaining > 1, self.lang) {
            (false, _) => String::new(),
            (true, Lang::Fr) => format!(" · encore {} après celle-ci", remaining - 1),
            (true, _) => format!(" · {} more after this one", remaining - 1),
        };
        Some((
            key,
            format!("{sentence}\n  {consequence}{more}"),
            options,
            typed,
        ))
    }

    /// The ceilings offered per scheduled run, recommended from the workflow's last run.
    fn ceilings(&self) -> Vec<(String, Option<String>)> {
        ceilings::offered(self.observed.map(Evidence::LastRun), self.lang)
    }

    /// Bind the current key to a line; the refusal names the fix.
    pub(super) fn answer(&mut self, line: &str) -> Result<(), String> {
        let key = self
            .current()
            .ok_or_else(|| "no activation question waits".to_owned())?;
        let value = line.trim();
        // « choisis pour moi », « fais au mieux »: the choice left to Nika takes the run-once
        // policy and the recommended ceiling, both shown in the declaration to consent to
        // (live gauntlet, 2026-10-16: the delegation was honoured for the zone, then dropped).
        // A zone left to Nika is this machine's, said in the declaration to consent to: the
        // person's choice, never an assumption (live gauntlet r02 B10, 2026-10-17: « choisis pour
        // moi » was refused with `Area/City` syntax).
        if delegates(value) && key == TIMEZONE {
            let machine = jiff::tz::TimeZone::system();
            let zone = machine.iana_name().ok_or_else(|| {
                "this machine names no time zone — answer one, e.g. Europe/Paris".to_owned()
            })?;
            self.answers.insert(key, zone.to_owned());
            self.needed.remove(0);
            return Ok(());
        }
        if delegates(value) && key != TIMEZONE {
            let chosen = if key == MISSED {
                "rattraper-une-fois".to_owned()
            } else {
                let offered = ceilings::offered(self.observed.map(Evidence::LastRun), self.lang);
                (ceilings::recommended(&offered))
                    .ok_or_else(|| "no ceiling could be recommended".to_owned())?
                    .to_owned()
            };
            self.answers.insert(key, chosen);
            self.needed.remove(0);
            return Ok(());
        }
        let bound = match key {
            TIMEZONE => {
                // The schedule grammar is the one judge of a zone, asked now rather than
                // after two more answers (`/Europe/Paris` has a path's shape, no zone's). A
                // place the person names (« Paris », « heure de Paris ») is its zone.
                let probe = format!("TZ={value} 0 0 * * *");
                if nika_cadence::registry::Cadence::parse(&probe).is_ok() {
                    value.to_owned()
                } else if let Some(zone) = zone_named(value) {
                    zone
                } else {
                    return Err(match self.lang {
                        Lang::Fr => format!(
                            "« {value} » n'est pas un fuseau que je connais — répondez une ville (« Paris ») ou un nom comme Europe/Paris · « choisis pour moi » prend celui de cette machine"
                        ),
                        _ => format!(
                            "`{value}` is not a time zone the schedule knows — answer a city (« Paris ») or a name like Europe/Paris · « choose for me » takes this machine's"
                        ),
                    });
                }
            }
            MISSED
                if missed_meaning(value).is_some() && !matches!(value.trim(), "1" | "2" | "3") =>
            {
                missed_meaning(value)
                    .unwrap_or("rattraper-une-fois")
                    .to_owned()
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
                if value.contains('€') || value.to_lowercase().contains("euro") {
                    return Err(format!(
                        "`{value}`: the ceiling is in US dollars, what providers bill — answer it in USD, e.g. 0.10 for ten cents"
                    ));
                }
                match ceiling_amount(value) {
                    Some(v) if v.is_finite() && v > 0.0 => format!("{v}"),
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
    /// The trigger this activation's declaration offers once it lands (D1), declared at `cadence`.
    pub(super) fn install_offer(&self, cadence: &str) -> super::trigger_install::InstallOffer {
        let answer = |key: &str| self.answers.get(key).cloned().unwrap_or_default();
        super::trigger_install::InstallOffer {
            workflow: self.workflow.clone(),
            when: (self.trigger.source_hint.clone()).unwrap_or_default(),
            cadence: cadence.to_owned(),
            missed: answer(MISSED),
            ceiling: answer(CEILING),
            lang: self.lang,
            gap: None,
        }
    }

    pub(super) fn cadence(&self) -> Option<String> {
        let tz = self.answers.get(TIMEZONE)?;
        Some(format!("TZ={tz} {}", self.trigger.cron.as_deref()?))
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

/// A `nika.yaml` created for the project under `root` with its first `arm:` entry: named after
/// the folder, in kebab case (`project` when the folder names nothing).
fn new_project_file(root: &std::path::Path, entry: &str) -> (ProjectChange, String) {
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
}

/// Whether the `arm:` list of `content` ends the file: every line after `arm:` is indented or
/// blank, so an entry appended at the end of the file joins that list.
fn arm_list_ends_the_file(content: &str) -> bool {
    let mut lines = content.lines().skip_while(|line| line.trim_end() != "arm:");
    lines.next().is_some()
        && lines.all(|line| line.trim().is_empty() || line.starts_with([' ', '\t']))
}

/// What « missed » may mean, as the cadence grammar's three policies: none is recommended, the
/// choice is the person's.
fn missed(lang: Lang) -> Vec<Offer> {
    vec![
        Offer::new(
            "1",
            lang.pick(
                "Run once when it comes back",
                "Le faire une fois à son retour",
            ),
            false,
        ),
        Offer::new(
            "2",
            lang.pick("Skip the missed run", "Sauter le run manqué"),
            false,
        ),
        Offer::new(
            "3",
            lang.pick(
                "Replay every missed occurrence",
                "Rejouer chaque run manqué",
            ),
            false,
        ),
    ]
}

/// The trigger the person's `words` state, read as the compiler reads a request's, in the
/// `zone` a proposal shows: a schedule whose day and hour the words state, valid in that zone,
/// never one Nika completes.
pub(super) fn scheduled(words: &str, zone: &str) -> Result<TriggerRequirement, String> {
    let complete = |t: &TriggerRequirement| t.kind == TriggerKind::Schedule && t.cron.is_some();
    // The words cited may say more than when (« qu'il tourne toutes les heures », « tous les
    // soirs à 20h, heure de Maurice »): the one schedule inside them is read (live gauntlet r05
    // and r08, 2026-10-16: both refused as incomplete, and each author looped asking again).
    let read = (nika_compile::trigger_of(words).filter(complete)).or_else(|| {
        stated_cadence(words)
            .and_then(|cadence| nika_compile::trigger_of(&cadence))
            .filter(complete)
    });
    let Some(mut trigger) = read else {
        return Err(format!(
            "« {words} » is not a complete schedule: cite only the person's words that say when it runs (« chaque lundi à 9h », « le 1er de chaque mois à 8h », \"every Friday at 10\"); when they state no day or no hour, ask them, never assume one. A day of the month is read from 1 to 28: a 29th, 30th or 31st would skip the months without one, so ask whether they mean the last day (« le dernier jour de chaque mois à 8h »). Never bind another cadence and check the date inside the workflow"
        ));
    };
    let cadence = format!("TZ={zone} {}", trigger.cron.as_deref().unwrap_or_default());
    if let Err(why) = nika_cadence::registry::Cadence::parse(&cadence) {
        return Err(format!(
            "`{cadence}` is refused by the schedule grammar: {why}"
        ));
    }
    trigger.timezone = Some(zone.to_owned());
    Ok(trigger)
}

impl SessionRuntime {
    /// A line that activates the saved workflow's schedule, when it is one. « activate »: the
    /// schedule the last saved workflow asked for becomes a declaration to review, never by a
    /// `yes`, never by saving. « active-le tous les matins à 7h »: the cadence the line states is
    /// the one declared. « déclare-la à côté, tous les jours à 8h », « ok tous les jours à 8h du
    /// mat alors »: a schedule stated for the saved workflow is its activation, never its
    /// rewriting (W17).
    pub(super) fn activation_line(&mut self, input: &str) -> Option<TurnOutcome> {
        if is_activate(input) || self.activates_saved_period(input) {
            return Some(self.activate_turn());
        }
        let cadence = activation_cadence(input).or_else(|| self.schedules_the_saved(input))?;
        Some(self.activate_with(&cadence, input))
    }

    /// An activation line that states its cadence (« active-le tous les matins à 7h »): that
    /// cadence is the one declared, the person's own words, and the activation asks only what it
    /// leaves open. A cadence the last workflow already asked for keeps its consented zone.
    pub(super) fn activate_with(&mut self, words: &str, line: &str) -> TurnOutcome {
        let stated = nika_compile::trigger_of(words)
            .filter(|t| t.kind == TriggerKind::Schedule && t.cron.is_some());
        if self.last_workflow.is_some()
            && let Some(stated) = stated
            && (self.last_trigger.as_ref()).is_none_or(|kept| kept.cron != stated.cron)
        {
            self.last_trigger = Some(stated);
        }
        // The zone the line names is the person's answer, never asked again (W5).
        if let Some(zone) = zone_in_line(line)
            && let Some(trigger) = self.last_trigger.as_mut()
        {
            trigger.timezone = Some(zone);
        }
        self.activate_turn()
    }

    /// The cadence a line states for the saved workflow to run on its own (see
    /// [`automation_cadence`]), when a workflow is saved and no proposal waits.
    pub(super) fn schedules_the_saved(&self, line: &str) -> Option<String> {
        (self.last_workflow.is_some() && self.pending.is_none())
            .then(|| automation_cadence(line))
            .flatten()
    }

    /// « activate » — the schedule the last accepted workflow asked for
    /// becomes a declaration to review: the three values the sentence did
    /// not state are asked first, one line each.
    pub(super) fn activate_turn(&mut self) -> TurnOutcome {
        let lang = self.language;
        // A reopened conversation that saved nothing yet still activates the one schedule
        // `nika.yaml` declares (live gauntlet r01, 2026-10-17).
        let Some(workflow) =
            (self.last_workflow.clone()).or_else(|| sole_declared(&self.snapshot.root))
        else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                lang.pick(
                    "nothing to activate — describe the work with its schedule, save the workflow, then activate",
                    "rien à activer — décrivez le travail et quand il doit tourner, enregistrez le workflow, puis activez-le",
                ),
            ));
        };
        // A workflow saved without a schedule, then asked to run on one in the conversation
        // (« je voudrais qu'il tourne toutes les heures »): the person's latest words that state
        // a cadence are the schedule to declare (live gauntlet r05, 2026-10-16: « activate »
        // answered « asked for no schedule » after three hourly requests).
        if self.last_trigger.is_none()
            && let Some(words) =
                (self.person_lines_newest_first().iter()).find_map(|line| stated_cadence(line))
        {
            self.last_trigger = nika_compile::trigger_of(&words)
                .filter(|t| t.kind == TriggerKind::Schedule && t.cron.is_some());
        }
        // The zone the line that stated the schedule named (« chaque matin à 8h heure de Paris »)
        // answers the zone question (live gauntlet r08 and r02, 2026-10-16: it was asked again).
        if (self.last_trigger.as_ref()).is_some_and(|trigger| trigger.timezone.is_none()) {
            let named = (self.person_lines_newest_first().iter())
                .filter(|line| stated_cadence(line).is_some())
                .find_map(|line| zone_in_line(line));
            if let Some(trigger) = self.last_trigger.as_mut() {
                trigger.timezone = named;
            }
        }
        // A schedule `nika.yaml` already declares is the one to activate, whatever this
        // conversation remembers of how it was asked (a reopened one knows none of it).
        if self.last_trigger.is_none()
            && let Some(outcome) = self.already_declared(&workflow, None)
        {
            return outcome;
        }
        let Some(trigger) = self.last_trigger.clone() else {
            return TurnOutcome::Facts(match lang {
                Lang::Fr => format!(
                    "`{}` n'a pas de planification : il tourne quand vous le demandez (« lance-le ») · pour le planifier, dites quand il doit tourner (« chaque matin à 8h », « le 1er de chaque mois à 8h ») et Nika le déclare à côté du workflow",
                    workflow.display()
                ),
                _ => format!(
                    "`{}` asked for no schedule: it runs when you ask (« run it ») · to schedule it, say when in your request (« every weekday at 8 », « the 1st of every month at 08:00 ») and Nika keeps that beside the workflow",
                    workflow.display()
                ),
            });
        };
        if trigger.kind != TriggerKind::Schedule || trigger.status != TriggerStatus::RequiresBinding
        {
            return TurnOutcome::Facts(match lang {
                Lang::Fr => format!(
                    "la demande décrit un déclencheur que Nika ne sait pas encore déclarer dans `nika.yaml` ({:?}) · le workflow tourne quand vous le demandez",
                    trigger.kind
                ),
                _ => format!(
                    "the request asked for a trigger Nika cannot declare in `nika.yaml` yet ({:?}) · the workflow runs when you ask",
                    trigger.kind
                ),
            });
        }
        if trigger.cron.is_none() {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                lang.pick(
                    "The requested schedule is incomplete, conflicting or unsupported; nothing was declared. Restate the work with one explicit supported cadence: every Tuesday at 09:00, every day at 18:00, the 1st of every month at 08:00, or every 2 hours. Daily/weekly schedules need an explicit time, and weekly needs a weekday. No hour or weekday is assumed.",
                    "La planification demandée est incomplète, contradictoire ou pas encore prise en charge ; rien n'a été déclaré. Redites-la avec une cadence explicite : chaque mardi à 9h, tous les jours à 18h, le 1er de chaque mois à 8h, ou toutes les 2 heures. Une cadence quotidienne ou hebdomadaire a besoin d'une heure, et une hebdomadaire d'un jour. Aucune heure ni aucun jour n'est supposé.",
                ),
            ));
        }
        // A workflow already declared is said so at once, never asked again only to be refused
        // (live gauntlet, 2026-10-16: the missed policy and the ceiling were asked a second time,
        // then « nika.yaml already declares an arm: list »).
        if let Some(outcome) = self.already_declared(&workflow, Some(&trigger)) {
            return outcome;
        }
        let observed = last_cost(&self.snapshot.root, &workflow);
        let activation = Activation::new(workflow, trigger, observed, self.language);
        let question = activation.question();
        let key = activation.current().unwrap_or(TIMEZONE).to_owned();
        self.intent.unresolved =
            vec!["the values the schedule's declaration still needs".to_owned()];
        self.activation = Some(activation);
        TurnOutcome::Question { key, question }
    }

    /// The finite choice the activation question waiting now offers, for a host that shows its
    /// options natively ([`Waiting::Activation`]): its key, the question as its sentence and one
    /// short line (the options are the host's rows, never repeated as text), each option, and
    /// whether another typed value is admitted. A typed time zone offers none. The ceiling's
    /// options are amounts, one recommended from the workflow's last observed run when there is
    /// one, with why in its label. Picking an option answers as typing its key does, through the
    /// same door and grammar; nothing is applied without that answer. Read-only: it answers,
    /// applies and authorizes nothing.
    #[must_use]
    pub fn waiting_activation_choices(&self) -> Option<(&'static str, String, Vec<Offer>, bool)> {
        let choices = match &self.activation {
            Some(activation) => activation.choices()?,
            None => self.install_choices()?,
        };
        let waits = Waiting::Activation {
            key: choices.0.to_owned(),
        };
        (self.waiting() == waits).then_some(choices)
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
        if is_cancel(line) {
            self.intent.unresolved.clear();
            return TurnOutcome::Facts(
                self.language
                    .pick(
                        "activation dropped · nothing was declared · the workflow stays saved and runs when you ask",
                        "activation abandonnée · rien n'a été déclaré · le workflow reste enregistré et tourne quand vous le demandez",
                    )
                    .to_owned(),
            );
        }
        // A command-shaped line is never an activation value (`/bogus` is no `Area/City`).
        if let Some(text) = super::protocol::unserved_command(line) {
            self.activation = Some(activation);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "{text}\n  {}",
                    self.language.pick(
                        "the question still waits · reply on the next line · `cancel` drops the activation",
                        "la question attend toujours · répondez sur la ligne suivante · « annuler » abandonne l'activation",
                    )
                ),
            ));
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
                "the words « {} » cannot be turned into a cadence yet — write the `cadence:` line yourself in `nika.yaml` (`TZ=Europe/Paris 0 8 * * *`), Nika lists it with `{} arm`",
                activation
                    .trigger
                    .source_hint
                    .as_deref()
                    .unwrap_or("the schedule"),
                program()
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
            let listed = content.lines().any(|l| l.trim_end() == "arm:");
            let workflow = format!("- workflow: {}", activation.workflow.display());
            if listed && content.lines().any(|l| l.trim() == workflow) {
                return TurnOutcome::Facts(format!(
                    "`nika.yaml` already declares a schedule for `{}` · `{} arm` lists it · to change it, edit its entry",
                    activation.workflow.display(),
                    program()
                ));
            }
            if listed && !arm_list_ends_the_file(&content) {
                return TurnOutcome::Facts(format!(
                    "`nika.yaml` declares an `arm:` list followed by other settings — Nika appends only to a list that ends the file. Add this entry under it yourself, then `{nika} arm` lists it:\n{entry}",
                    nika = program()
                ));
            }
            if !content.ends_with('\n') {
                content.push('\n');
            }
            // A list that ends the file takes the entry at its end, as the first one was written
            // (live gauntlet, 2026-10-16: Nika refused to extend the list it wrote 13 minutes
            // earlier and sent a retired teacher to edit YAML by hand).
            if listed {
                content.push_str(&entry);
            } else {
                let _ = write!(content, "arm:\n{entry}");
            }
            (
                ProjectChange::UpdateProjectFile {
                    before: Witness::of(&bytes),
                    content: content.clone(),
                },
                content,
            )
        } else {
            new_project_file(&root, &entry)
        };
        let set = ProjectChangeSet::project_change(
            &root,
            &self.intent.goal.clone().unwrap_or_default(),
            change,
        );
        let bytes = set.preview();
        let id = crate::ProposalId::of(&bytes);
        let installs = self.trigger_host.installs();
        let mut preview = declaration_header(activation, &cadence, path.exists(), installs);
        let _ = writeln!(preview, "  ┌─ `nika.yaml`");
        for line in content.lines() {
            let _ = writeln!(preview, "  │ {line}");
        }
        let _ = writeln!(
            preview,
            "  └─\n  {}",
            activation
                .lang
                .pick(
                    "identity {id} · `yes` writes these exact bytes · `no` discards",
                    "identité {id} · « oui » écrit exactement ces octets · « non » abandonne",
                )
                .replace("{id}", &id.to_string())
        );
        self.remember("(activation)", &format!("(proposed {id})"));
        self.activation_proposal = Some(id.clone());
        // Once it lands, its trigger is offered (D1).
        self.declared = Some(activation.install_offer(&cadence));
        self.pending = Some(set);
        TurnOutcome::Proposal { id, preview }
    }
}

impl SessionRuntime {
    /// « active-le » for a workflow `nika.yaml` already declares: its trigger is offered, or it is
    /// said active ([`Self::activate_declared`]), or its declared state is said with how to change
    /// it. `None` when `nika.yaml` declares no schedule for it.
    fn already_declared(
        &mut self,
        workflow: &Path,
        stated: Option<&TriggerRequirement>,
    ) -> Option<TurnOutcome> {
        let root = self.snapshot.root.clone();
        let host = &self.trigger_host;
        let state = declared_state(&root, workflow, self.language, &|label| {
            host.installed(&root, label)
        })?;
        // D1 for a schedule declared earlier: its trigger is offered, or it is said active,
        // unless the line restates another cadence, which is a change of its entry.
        if let Some(outcome) = self.activate_declared(workflow, stated) {
            return Some(outcome);
        }
        let (lead, rest) = match self.language {
            Lang::Fr => (
                "est déjà déclaré dans `nika.yaml`",
                "pour changer sa planification, modifiez son entrée",
            ),
            _ => (
                "is already declared in `nika.yaml`",
                "to change its schedule, edit its entry",
            ),
        };
        Some(TurnOutcome::Facts(format!(
            "`{}` {lead} · {state} · {rest}",
            workflow.display()
        )))
    }

    /// « active-le » for a schedule `nika.yaml` already declares, on a host that installs
    /// triggers: its trigger is offered when this Mac holds no unit of it, and it is said active,
    /// with its next run, when it does. `None` keeps the declared state as it is said today: a
    /// host that installs nothing, a suspended entry, or a line restating another cadence than
    /// the declared one (a change of the entry).
    fn activate_declared(
        &mut self,
        workflow: &Path,
        stated: Option<&TriggerRequirement>,
    ) -> Option<TurnOutcome> {
        if !self.trigger_host.installs() {
            return None;
        }
        let root = self.snapshot.root.clone();
        let entry = declared_arm_entry(&root, workflow)?;
        if entry.actif == Some(false) {
            return None;
        }
        let declared = entry
            .cadence
            .split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" ");
        if stated
            .and_then(|trigger| trigger.cron.as_deref())
            .is_some_and(|stated| stated != declared)
        {
            return None;
        }
        let lang = self.language;
        let words = cadence_words(&entry.cadence, lang);
        let label = super::schedule_ledger::label_of(&root, workflow)?;
        let shown = workflow.display();
        if self.trigger_host.installed(&root, &label) {
            let next = super::trigger_install::next_fire(&entry.cadence);
            let said = match (lang, next) {
                (Lang::Fr, Some(next)) => {
                    format!("`{shown}` est déjà actif sur ce Mac · {words} · prochain run : {next}")
                }
                (Lang::Fr, None) => format!("`{shown}` est déjà actif sur ce Mac · {words}"),
                (_, Some(next)) => {
                    format!("`{shown}` is already active on this Mac · {words} · next run: {next}")
                }
                (_, None) => format!("`{shown}` is already active on this Mac · {words}"),
            };
            return Some(TurnOutcome::Facts(said));
        }
        let when = (stated.and_then(|trigger| trigger.source_hint.clone()))
            .unwrap_or_else(|| words.clone());
        self.declared = Some(super::trigger_install::InstallOffer {
            workflow: workflow.to_path_buf(),
            when,
            cadence: entry.cadence.clone(),
            missed: entry.manque.as_str().to_owned(),
            ceiling: entry.plafond.to_string(),
            lang,
            gap: None,
        });
        let lead = match lang {
            Lang::Fr => format!(
                "`{shown}` est déjà déclaré dans `nika.yaml` · {words} · aucun déclencheur sur ce Mac"
            ),
            _ => format!(
                "`{shown}` is already declared in `nika.yaml` · {words} · no trigger on this Mac"
            ),
        };
        Some(self.offer_install(lead))
    }
}

/// The workflow of the one schedule `nika.yaml` declares, when it declares exactly one.
fn sole_declared(root: &Path) -> Option<PathBuf> {
    let (_, project) = nika_vocab::project::discover(root).ok().flatten()?;
    match project.arm() {
        [entry] => Some(PathBuf::from(&entry.workflow)),
        _ => None,
    }
}

/// The entry that declares `workflow` in `nika.yaml`, whole.
pub(super) fn declared_arm_entry(
    root: &Path,
    workflow: &Path,
) -> Option<nika_vocab::project::ArmEntry> {
    let (_, project) = nika_vocab::project::discover(root).ok().flatten()?;
    (project.arm().iter())
        .find(|entry| Path::new(&entry.workflow) == workflow)
        .cloned()
}

/// The declaration's header as the person reads it, in their language: when it runs, how the
/// cadence reads, the missed policy and the ceiling, the file it changes, and that declared is
/// not active.
fn declaration_header(
    activation: &Activation,
    cadence: &str,
    exists: bool,
    installs: bool,
) -> String {
    let when = activation
        .trigger
        .source_hint
        .as_deref()
        .unwrap_or("the schedule");
    let lang = activation.lang;
    let words = cadence_words(cadence, lang);
    let missed = missed_words(
        activation.answers.get(MISSED).map_or("", String::as_str),
        lang,
    );
    let ceiling = activation.answers.get(CEILING).map_or("", String::as_str);
    let mark = if exists { "~" } else { "+" };
    let nika = program();
    let after = match (lang, installs) {
        (Lang::Fr, true) => "Déclaré n'est pas actif · une fois enregistré, Nika vous propose d'installer le déclencheur sur ce Mac".to_owned(),
        (Lang::Fr, false) => format!(
            "Déclaré n'est pas actif · après l'enregistrement, un déclencheur doit tourner sur cette machine : `{nika} serve` (résident), ou l'unité système `{nika} arm --emit launchd --write` · `{nika} arm` liste ce qui est déclaré et prouve ce qui s'est déclenché"
        ),
        (_, true) => "Declared is not active · once saved, Nika offers to install the trigger on this Mac".to_owned(),
        (_, false) => format!(
            "Declared is not active · after saving, a firer on this machine must run it: `{nika} serve` (resident), or the OS unit `{nika} arm --emit launchd --write` · `{nika} arm` lists what is declared and proves what fired"
        ),
    };
    match lang {
        Lang::Fr => format!(
            "Nika propose de déclarer la planification dans `nika.yaml` :\nExécutions\n  {when} · {words}\n  dans `nika.yaml` : `{cadence}`\n  si manqué · {missed}\n  plafond · {ceiling} $ par run planifié\nChangements\n  {mark} `nika.yaml`\n{after}\n"
        ),
        _ => format!(
            "Nika proposes to declare the schedule in `nika.yaml`:\nRuns\n  {when} · {words}\n  in `nika.yaml`: `{cadence}`\n  if missed · {missed}\n  ceiling · ${ceiling} per scheduled run\nChanges\n  {mark} `nika.yaml`\n{after}\n"
        ),
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

/// The words of a line that ask to activate a schedule beside its cadence (« active-le tous les
/// matins à 7h », « programme-le chaque lundi à 9h », « activate it every day at 18:00 »).
const ACTIVATION_WORDS: &[&str] = &[
    "active-le",
    "active-la",
    "activez-le",
    "activez-la",
    "active",
    "activez",
    "activer",
    "l'activer",
    "arme-le",
    "programme-le",
    "programmez-le",
    "planifie-le",
    "planifiez-le",
    "activate",
    "schedule",
];

/// The cadence an activation line states beside its activation words: the longest run of its
/// other words the schedule reader reads whole (live gauntlet r02 and r05, 2026-10-16:
/// « Maintenant active-le tous les matins à 7h stp » re-proposed the saved bytes, then asked
/// for the English word « activate »). `None` when the line holds no activation word or no
/// cadence.
pub(super) fn activation_cadence(line: &str) -> Option<String> {
    let folded = line.replace('\u{2019}', "'").to_lowercase();
    let words: Vec<&str> = (folded
        .split(|c: char| c.is_whitespace() || matches!(c, '!' | ',' | '?' | ';')))
    .map(|word| word.trim_end_matches('.'))
    .filter(|word| !word.is_empty())
    .collect();
    let at = words
        .iter()
        .position(|word| ACTIVATION_WORDS.contains(word))?;
    let rest: Vec<&str> = (words.iter().enumerate())
        .filter(|(i, _)| *i != at)
        .map(|(_, word)| *word)
        .collect();
    cadence_in(&rest)
}

/// The longest run of `words` that states a whole schedule (a cadence and its time), as the
/// compiler reads a request's trigger.
fn cadence_in(words: &[&str]) -> Option<String> {
    let (start, end) = cadence_span(words)?;
    // Two schedules stated apart are no one schedule (« chaque lundi à 9h et chaque vendredi à
    // 9h »): none is read, and the person is asked.
    let other = |part: &[&str]| cadence_span(part).is_some();
    if other(&words[..start]) || other(&words[end..]) {
        return None;
    }
    // A word the reader let dangle at the end (« à 8h heure » before « de Maurice ») is dropped
    // while the schedule reads the same without it.
    let cron = |part: &[&str]| nika_compile::trigger_of(&part.join(" ")).and_then(|t| t.cron);
    let mut end = end;
    while end > start + 1
        && matches!(
            words[end - 1],
            "heure" | "heures" | "de" | "d'" | "à" | "le" | "la"
        )
        && cron(&words[start..end - 1]) == cron(&words[start..end])
    {
        end -= 1;
    }
    Some(words[start..end].join(" "))
}

/// Where the longest run of `words` that states a whole schedule lies, as a word span.
fn cadence_span(words: &[&str]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for start in 0..words.len() {
        for end in (start + 1..=words.len()).rev() {
            if best.is_some_and(|(a, b)| b - a >= end - start) {
                break;
            }
            let phrase = words[start..end].join(" ");
            let read = nika_compile::trigger_of(&phrase);
            if read.is_some_and(|t| t.kind == TriggerKind::Schedule && t.cron.is_some()) {
                best = Some((start, end));
                break;
            }
        }
    }
    best
}

/// The words that ask for the saved workflow to run on its own on a schedule (« déclare-la à côté,
/// tous les jours à 8h », « je voudrais que ce bilan se fasse tout seul le 1er de chaque mois à
/// 8h », « make it run automatically every day at 18:00 »).
const AUTOMATION_CUES: &[&str] = &[
    "déclare-la",
    "déclare-le",
    "déclarez-la",
    "déclarez-le",
    "déclarer",
    "déclare",
    "tout seul",
    "toute seule",
    "automatiquement",
    "se fasse",
    "se fassent",
    "se lance",
    "se lancer",
    "qu'il tourne",
    "qu'elle tourne",
    "le faire tourner",
    "la faire tourner",
    "fais-le tourner",
    "fais-la tourner",
    "automatically",
    "on its own",
    "by itself",
    "declare it",
];

/// The words around a schedule that ask nothing more of it (« ok tous les jours à 8h du mat
/// alors »).
const CADENCE_FILLERS: &[&str] = &[
    "ok", "okay", "alors", "bon", "donc", "du", "mat", "matin", "heure", "heures", "de", "d'", "à",
    "a", "ça", "ca", "c'est", "plutôt", "plutot", "disons", "ben", "euh", "oui", "yes", "stp",
    "svp", "merci", "please", "then", "so", "le", "la", "l'", "s'il", "vous", "te", "plait",
    "plaît", "fuseau", "time",
];

/// The words that make a line another request than scheduling the saved workflow: a second
/// task, or a new workflow (« un workflow qui tourne tout seul chaque lundi pour … »).
const OTHER_REQUEST: &[&str] = &[
    "et", "and", "puis", "aussi", "also", "qui", "which", "that", "pour", "crée", "créer", "créez",
    "nouveau", "nouvelle", "autre", "workflow", "create", "new", "another", "build",
];

/// The cadence a line states for the saved workflow when it asks it to run on its own, or is
/// that schedule alone with its fillers: a whole cadence with an automation cue (« déclare-la à
/// côté, tous les jours à 8h heure de Maurice »), or a whole cadence with only fillers and a zone
/// around it (« ok tous les jours à 8h du mat alors »), and no other request beside. `None` for
/// anything else, which keeps its route (live gauntlet r01, r04, 2026-10-16: such lines reached
/// the author, which tried to write the schedule into the workflow: « propose failed » after
/// several minutes each).
pub(super) fn automation_cadence(line: &str) -> Option<String> {
    let folded = line.replace('\u{2019}', "'").to_lowercase();
    let words: Vec<&str> = (folded
        .split(|c: char| c.is_whitespace() || matches!(c, '!' | ',' | '?' | ';' | '(' | ')')))
    .map(|word| word.trim_end_matches('.'))
    .filter(|word| !word.is_empty())
    .collect();
    let (start, end) = cadence_span(&words)?;
    let cadence = cadence_in(&words)?;
    let rest: Vec<&str> = (words.iter().enumerate())
        .filter(|(i, _)| *i < start || *i >= end)
        .map(|(_, word)| *word)
        .collect();
    if rest.iter().any(|word| OTHER_REQUEST.contains(word)) {
        return None;
    }
    let spaced = format!(" {} ", rest.join(" "));
    let cued = (AUTOMATION_CUES.iter()).any(|cue| spaced.contains(&format!(" {cue} ")));
    let alone = !folded.contains('?')
        && rest.len() <= 6
        && (rest.iter()).all(|word| CADENCE_FILLERS.contains(word) || zone_named(word).is_some());
    (cued || alone).then_some(cadence)
}

/// The zone a line names beside its schedule (« … à 8h heure de Maurice », « 9h (Europe/Paris) »,
/// « at 8 Paris time »), read as the zone question reads its answer.
pub(super) fn zone_in_line(line: &str) -> Option<String> {
    let folded = line.replace('\u{2019}', "'").to_lowercase();
    let words: Vec<&str> = (folded
        .split(|c: char| c.is_whitespace() || matches!(c, '!' | ',' | '?' | ';' | '(' | ')')))
    .map(|word| word.trim_end_matches('.'))
    .filter(|word| !word.is_empty())
    .collect();
    for (at, word) in words.iter().enumerate() {
        if word.contains('/')
            && let Some(zone) = iana_named(word)
        {
            return Some(zone);
        }
        if matches!(*word, "heure" | "fuseau") {
            for len in [4, 3, 2] {
                if let Some(phrase) = words.get(at..at + len)
                    && let Some(zone) = zone_named(&phrase.join(" "))
                {
                    return Some(zone);
                }
            }
        }
        if *word == "time"
            && let Some(place) = at.checked_sub(1).and_then(|before| words.get(before))
            && let Some(zone) = zone_named(place)
        {
            return Some(zone);
        }
    }
    None
}

/// An IANA zone written in any case (`europe/paris`, `America/New_York`), when the schedule
/// grammar knows it.
fn iana_named(word: &str) -> Option<String> {
    let title = |part: &str| {
        let mut chars = part.chars();
        chars.next().map_or_else(String::new, |first| {
            first
                .to_uppercase()
                .chain(chars.flat_map(char::to_lowercase))
                .collect::<String>()
        })
    };
    let zone: Vec<String> = (word.split('/'))
        .map(|segment| segment.split('_').map(title).collect::<Vec<_>>().join("_"))
        .collect();
    let zone = zone.join("/");
    nika_cadence::registry::Cadence::parse(&format!("TZ={zone} 0 0 * * *"))
        .is_ok()
        .then_some(zone)
}

/// The cadence a person's line states, anywhere in it.
pub(super) fn stated_cadence(line: &str) -> Option<String> {
    let folded = line.replace('\u{2019}', "'").to_lowercase();
    let words: Vec<&str> = (folded
        .split(|c: char| c.is_whitespace() || matches!(c, '!' | ',' | '?' | ';')))
    .map(|word| word.trim_end_matches('.'))
    .filter(|word| !word.is_empty())
    .collect();
    cadence_in(&words)
}

/// The part of a saved cadence a period word names.
#[derive(Clone, Copy)]
enum Period {
    Day,
    Morning,
    Evening,
    Week,
    Month,
}

/// The periods an activation line may restate beside its activation word, each with the part of
/// the saved cadence it names.
const PERIODS: &[(&str, Period)] = &[
    ("tous les matins", Period::Morning),
    ("chaque matin", Period::Morning),
    ("le matin", Period::Morning),
    ("every morning", Period::Morning),
    ("each morning", Period::Morning),
    ("in the morning", Period::Morning),
    ("tous les soirs", Period::Evening),
    ("chaque soir", Period::Evening),
    ("le soir", Period::Evening),
    ("every evening", Period::Evening),
    ("each evening", Period::Evening),
    ("every night", Period::Evening),
    ("in the evening", Period::Evening),
    ("tous les jours", Period::Day),
    ("chaque jour", Period::Day),
    ("quotidiennement", Period::Day),
    ("every day", Period::Day),
    ("each day", Period::Day),
    ("daily", Period::Day),
    ("toutes les semaines", Period::Week),
    ("chaque semaine", Period::Week),
    ("every week", Period::Week),
    ("weekly", Period::Week),
    ("tous les mois", Period::Month),
    ("chaque mois", Period::Month),
    ("every month", Period::Month),
    ("monthly", Period::Month),
];

/// The period an activation line restates beside its activation word, the courtesy around them
/// aside (« active-le tous les matins stp »); `None` when the line says anything else.
fn activation_period(line: &str) -> Option<Period> {
    let line = line.replace('\u{2019}', "'").to_lowercase();
    let words: Vec<&str> = (line
        .split(|c: char| c.is_whitespace() || matches!(c, '!' | '.' | ',' | '?')))
    .filter(|word| !word.is_empty())
    .collect();
    let at = words
        .iter()
        .position(|word| ACTIVATION_WORDS.contains(word))?;
    let rest: Vec<&str> = (words.iter().enumerate())
        .filter(|(i, word)| *i != at && !COURTESY.contains(word))
        .map(|(_, word)| *word)
        .collect();
    let said = rest.join(" ");
    (PERIODS.iter())
        .find(|(phrase, _)| *phrase == said)
        .map(|(_, period)| *period)
}

/// Whether a saved cadence (`cron`, five fields) runs on the part of time `period` names: a
/// morning one between 05:00 and 11:59, an evening one between 17:00 and 23:59, a day one every
/// day, a weekly one on its weekdays, a monthly one on its day of the month.
fn runs_on(cron: &str, period: Period) -> bool {
    let fields: Vec<&str> = cron.split_whitespace().collect();
    let [_, hour, dom, _, dow] = fields.as_slice() else {
        return false;
    };
    let hour = hour.parse::<u32>().ok();
    match period {
        Period::Day => *dom == "*" && *dow == "*" && hour.is_some(),
        Period::Morning => *dom == "*" && hour.is_some_and(|h| (5..12).contains(&h)),
        Period::Evening => *dom == "*" && hour.is_some_and(|h| (17..24).contains(&h)),
        Period::Week => *dom == "*" && *dow != "*",
        Period::Month => *dom != "*",
    }
}

impl SessionRuntime {
    /// « active-le tous les matins »: an activation word with only the period the saved schedule
    /// already runs on asks to activate that schedule, as « active-le » does (NIK-196, a real
    /// French journey, 2026-10-16: the line reached the author, which proposed the saved bytes
    /// again). Another period (« tous les soirs » for a morning schedule) is a change, never this
    /// door, and a workflow with no schedule has none to restate.
    pub(super) fn activates_saved_period(&self, line: &str) -> bool {
        let Some(period) = activation_period(line) else {
            return false;
        };
        let saved = (self.last_trigger.as_ref()).and_then(|t| t.cron.clone());
        let cron = saved.or_else(|| {
            let stated = (self.person_lines_newest_first().iter()).find_map(|l| stated_cadence(l));
            stated.and_then(|words| nika_compile::trigger_of(&words)?.cron)
        });
        self.last_workflow.is_some() && cron.is_some_and(|cron| runs_on(&cron, period))
    }
}

/// The few words that ask to activate the schedule.
#[must_use]
pub(super) fn is_activate(line: &str) -> bool {
    let line = line.replace('\u{2019}', "'").to_lowercase();
    // « active-le alors s'il vous plait » (live gauntlet, 2026-10-15: it ran the workflow once
    // more): a courtesy or a filler around the words leaves them the same request.
    let mut words: Vec<&str> = (line
        .split(|c: char| c.is_whitespace() || matches!(c, '!' | '.' | ',' | '?')))
    .filter(|word| !word.is_empty())
    .collect();
    while words.first().is_some_and(|word| COURTESY.contains(word)) {
        words.remove(0);
    }
    while words.last().is_some_and(|word| COURTESY.contains(word)) {
        words.pop();
    }
    matches!(
        words.join(" ").as_str(),
        "activate"
            | "/activate"
            | "activate it"
            | "activate the schedule"
            | "arm it"
            | "arm the schedule"
            | "active"
            | "active-le"
            | "active-la"
            | "active le"
            | "active la planification"
            | "active la répétition"
            | "activez-le"
            | "activez le"
            | "activez"
            | "activer"
            | "activez la planification"
            | "l'activer"
            | "pouvez l'activer"
            | "pouvez-vous l'activer"
            | "tu peux l'activer"
            | "peux-tu l'activer"
            | "peux tu l'activer"
            | "est-ce que vous pouvez l'activer"
            | "est-ce que tu peux l'activer"
            | "can you activate it"
            | "could you activate it"
            | "arme"
            | "arme-le"
    )
}

/// The program the person runs Nika as (`nika-dev`, `nika`): the name every hint gives, so a
/// command copied from it reaches this same binary (live gauntlet, 2026-10-16: the hints said
/// `nika`, which on that machine was an older release that refused the workflow).
pub(crate) fn program() -> String {
    std::env::args_os()
        .next()
        .and_then(|first| {
            Path::new(&first)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .filter(|name| {
            name == "nika"
                || (name.starts_with("nika-")
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.')))
        })
        .unwrap_or_else(|| "nika".to_owned())
}

/// The time zone a person names by its place (« Paris », « heure de Paris », « Montréal »,
/// « Maurice »): the zone of the time zone database whose city it is, as the schedule grammar
/// knows it. `None` when no zone has that name.
fn zone_named(answer: &str) -> Option<String> {
    let lower = answer.replace('\u{2019}', "'").to_lowercase();
    let mut place = lower.trim().trim_end_matches(['.', '!', '?']).to_owned();
    for lead in [
        "heure de ",
        "heure d'",
        "fuseau de ",
        "fuseau ",
        "l'heure de ",
        "time in ",
        "time of ",
        "the ",
        "à ",
        "a ",
    ] {
        if let Some(rest) = place.strip_prefix(lead) {
            place = rest.trim().to_owned();
        }
    }
    let aliases: &[(&str, &str)] = &[
        ("france", "Europe/Paris"),
        ("lyon", "Europe/Paris"),
        ("marseille", "Europe/Paris"),
        ("toulouse", "Europe/Paris"),
        ("bordeaux", "Europe/Paris"),
        ("lille", "Europe/Paris"),
        ("nantes", "Europe/Paris"),
        ("maurice", "Indian/Mauritius"),
        ("île maurice", "Indian/Mauritius"),
        ("ile maurice", "Indian/Mauritius"),
        ("londres", "Europe/London"),
        ("bruxelles", "Europe/Brussels"),
        ("genève", "Europe/Zurich"),
        ("geneve", "Europe/Zurich"),
        ("montréal", "America/Montreal"),
        ("québec", "America/Toronto"),
        ("quebec", "America/Toronto"),
        ("new york", "America/New_York"),
        ("san francisco", "America/Los_Angeles"),
        ("la réunion", "Indian/Reunion"),
        ("réunion", "Indian/Reunion"),
    ];
    if let Some((_, zone)) = aliases.iter().find(|(name, _)| *name == place) {
        return Some((*zone).to_owned());
    }
    let city: String = (place.split([' ', '-']).filter(|w| !w.is_empty()))
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<Vec<_>>()
        .join("_");
    let plain: String = city
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            'à' | 'â' => 'a',
            'î' | 'ï' => 'i',
            'ô' => 'o',
            'û' | 'ù' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect();
    if plain.is_empty() || !plain.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    [
        "Europe",
        "America",
        "Asia",
        "Africa",
        "Australia",
        "Pacific",
        "Indian",
        "Atlantic",
    ]
    .iter()
    .map(|area| format!("{area}/{plain}"))
    .find(|zone| nika_cadence::registry::Cadence::parse(&format!("TZ={zone} 0 0 * * *")).is_ok())
}

/// The words around « active-le » that ask nothing more: a courtesy, a filler, « now », a yes
/// (live gauntlet, 2026-10-16: « oui activez le s'il vous plait » ran the workflow instead).
const COURTESY: &[&str] = &[
    "alors",
    "directement",
    "ici",
    "tout",
    "de",
    "suite",
    "oui",
    "yes",
    "d'accord",
    "pour",
    "moi",
    "ok",
    "bon",
    "vas-y",
    "maintenant",
    "stp",
    "svp",
    "s'il",
    "vous",
    "te",
    "plait",
    "plaît",
    "merci",
    "please",
    "now",
    "then",
];

/// Where a declared beat stands: declared in the file, and whether this
/// machine's firer left a record of it — from the registry the file IS and
/// the sidecar the firer writes, never from the session's memory — and whether its unit is
/// installed, as the Session's trigger host says (`installed`, by label).
pub(super) fn declared_state(
    root: &Path,
    workflow: &Path,
    lang: Lang,
    installed: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let (path, active, cadence) = declared_entry(root, workflow)?;
    if !active {
        return Some(match lang {
            Lang::Fr => format!("Déclaré · suspendu (`actif: false`) · `{path}`"),
            _ => format!("Declared · suspended (`actif: false`) · `{path}`"),
        });
    }
    let words = cadence_words(&cadence, lang);
    let label = super::schedule_ledger::label_of(root, workflow);
    let history = super::schedule_ledger::histories(root, installed);
    let seen = label.and_then(|label| history.into_iter().find(|h| h.label == label));
    let last = seen.as_ref().and_then(|seen| seen.last.as_ref());
    let installed = seen.as_ref().is_some_and(|seen| seen.unit_installed);
    let last_run = last.map(|last| {
        let at = last_slot(&last.slot, &cadence);
        let kind = match (last.kind, lang) {
            ("fired", Lang::Fr) => "réussi",
            ("skipped", Lang::Fr) => "sauté",
            ("paused", Lang::Fr) => "en pause",
            ("failed", Lang::Fr) => "échoué",
            ("fired", _) => "succeeded",
            (other, _) => other,
        };
        match lang {
            Lang::Fr => format!("dernier run {at} · {kind}"),
            _ => format!("last run {at} · {kind}"),
        }
    });
    Some(match (lang, installed, last_run) {
        (Lang::Fr, true, Some(run)) => {
            format!("Actif · `{path}` · {words} · déclencheur installé sur ce Mac · {run}")
        }
        (Lang::Fr, true, None) => format!(
            "Déclaré · `{path}` · {words} · déclencheur installé sur ce Mac · pas encore déclenché"
        ),
        (Lang::Fr, false, Some(run)) => format!("Déclaré · `{path}` · {words} · {run}"),
        (Lang::Fr, false, None) => format!(
            "Déclaré · `{path}` · {words} · pas prouvé actif : un déclencheur doit tourner sur cette machine"
        ),
        (_, true, Some(run)) => {
            format!("Active · `{path}` · {words} · trigger installed on this Mac · {run}")
        }
        (_, true, None) => {
            format!("Declared · `{path}` · {words} · trigger installed on this Mac · not fired yet")
        }
        (_, false, Some(run)) => format!("Declared · `{path}` · {words} · {run}"),
        (_, false, None) => format!(
            "Declared · `{path}` · {words} · not proven active: a firer must run on this machine"
        ),
    })
}

/// A decided slot (RFC 3339) on the cadence's own clock, to the minute.
fn last_slot(slot: &str, cadence: &str) -> String {
    let zone = (cadence.strip_prefix("TZ="))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|zone| jiff::tz::TimeZone::get(zone).ok());
    match (slot.parse::<jiff::Timestamp>(), zone) {
        (Ok(at), Some(zone)) => at.to_zoned(zone).strftime("%Y-%m-%d %H:%M").to_string(),
        _ => slot.to_owned(),
    }
}

/// Every schedule `nika.yaml` declares, as one phrase (« `a.nika` (TZ=… 0 20 * * *) and
/// `b.nika` (…) »), each suspended one said so; « no schedule » when it declares none.
pub(super) fn declared_schedules(root: &Path) -> String {
    let Some((_, project)) = nika_vocab::project::discover(root).ok().flatten() else {
        return "no schedule".to_owned();
    };
    let each: Vec<String> = (project.arm().iter())
        .map(|entry| {
            let suspended = if entry.actif == Some(false) {
                ", suspended"
            } else {
                ""
            };
            format!("`{}` ({}{suspended})", entry.workflow, entry.cadence)
        })
        .collect();
    if each.is_empty() {
        "no schedule".to_owned()
    } else {
        each.join(" and ")
    }
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

#[cfg(test)]
mod scheduled_tests {
    use super::scheduled;

    /// A numbered day of every month is a monthly schedule in the zone shown, never a daily one;
    /// a day some months lack is asked, never bound (live gauntlet, 2026-10-16).
    #[test]
    fn a_day_of_every_month_is_a_monthly_schedule() {
        let trigger = scheduled("le 1er de chaque mois à 8h", "Europe/Paris");
        let read = trigger.as_ref().ok();
        assert_eq!(
            read.and_then(|t| t.cron.as_deref()),
            Some("0 8 1 * *"),
            "{trigger:?}"
        );
        assert_eq!(read.and_then(|t| t.cadence.as_deref()), Some("monthly"));
        assert_eq!(
            read.and_then(|t| t.timezone.as_deref()),
            Some("Europe/Paris")
        );
        // Words that say more than when: the one schedule inside them is read.
        for (words, cron) in [
            ("qu'il tourne toutes les heures", "0 * * * *"),
            ("tous les soirs à 20h, heure de Maurice", "0 20 * * *"),
        ] {
            let read = scheduled(words, "Indian/Mauritius");
            assert_eq!(
                read.as_ref().ok().and_then(|t| t.cron.as_deref()),
                Some(cron),
                "{words}: {read:?}"
            );
        }
        // Two schedules stated apart are asked, never one of them kept.
        assert!(scheduled("chaque lundi à 9h et chaque vendredi à 9h", "Europe/Paris").is_err());
        let refused = scheduled("le 31 de chaque mois à 8h", "Europe/Paris")
            .err()
            .unwrap_or_default();
        assert!(refused.contains("from 1 to 28"), "{refused}");
        assert!(
            refused.contains("le dernier jour de chaque mois"),
            "{refused}"
        );
    }
}

#[cfg(test)]
mod answer_reading_tests {
    use super::{
        activation_cadence, automation_cadence, ceiling_amount, delegates, missed_meaning,
        stated_cadence, zone_in_line, zone_named,
    };

    /// A schedule stated for the saved workflow, with an automation cue or alone with its fillers,
    /// is its activation; a second request or a new workflow beside it is not (live gauntlet r01,
    /// r04, 2026-10-16).
    #[test]
    fn a_schedule_stated_for_the_saved_workflow_is_its_activation() {
        for (line, cadence) in [
            (
                "déclare-la à côté, tous les jours à 8h heure de Maurice",
                "tous les jours à 8h",
            ),
            ("ok tous les jours à 8h du mat alors", "tous les jours à 8h"),
            (
                "Je voudrais que ce bilan se fasse tout seul le 1er de chaque mois à 8h",
                "le 1er de chaque mois à 8h",
            ),
            (
                "make it run automatically every day at 18:00",
                "every day at 18:00",
            ),
        ] {
            assert_eq!(automation_cadence(line).as_deref(), Some(cadence), "{line}");
        }
        for line in [
            "crée un workflow qui tourne tout seul chaque lundi à 9h",
            "qu'il se fasse tout seul chaque lundi à 9h et qu'il envoie aussi un mail",
            "tous les jours à 8h, c'est possible de changer le titre ?",
            "résume mes mails tous les jours à 8h",
            "déclare-la",
        ] {
            assert_eq!(automation_cadence(line), None, "{line}");
        }
        assert_eq!(
            zone_in_line("tous les jours à 8h heure de Maurice").as_deref(),
            Some("Indian/Mauritius")
        );
        assert_eq!(
            zone_in_line("chaque matin à 9h (Europe/Paris)").as_deref(),
            Some("Europe/Paris")
        );
        assert_eq!(
            zone_in_line("every day at 8 Paris time").as_deref(),
            Some("Europe/Paris")
        );
        assert_eq!(zone_in_line("tous les jours à 8h"), None);
    }

    /// An activation line's cadence, wherever it stands; none without an activation word or a
    /// cadence (live gauntlet r02/r05, 2026-10-16).
    #[test]
    fn an_activation_line_states_its_cadence() {
        for (line, cadence) in [
            (
                "Maintenant active-le tous les matins à 7h stp",
                "tous les matins à 7h",
            ),
            ("active-le chaque lundi à 9h", "chaque lundi à 9h"),
            (
                "Activate it every day at 18:00, please",
                "every day at 18:00",
            ),
            (
                "programme-le le 1er de chaque mois à 8h",
                "le 1er de chaque mois à 8h",
            ),
        ] {
            assert_eq!(activation_cadence(line).as_deref(), Some(cadence), "{line}");
        }
        for line in [
            "active-le",
            "tous les matins à 7h",
            "active le mode verbeux",
        ] {
            assert_eq!(activation_cadence(line), None, "{line}");
        }
        assert_eq!(
            stated_cadence("je voudrais qu'il tourne toutes les heures, et active-le stp")
                .as_deref(),
            Some("toutes les heures")
        );
    }

    /// The person's words for the three activation values (live gauntlet r08, r05).
    #[test]
    fn activation_answers_are_read_as_the_person_says_them() {
        assert!(delegates("je ne comprends pas, choisissez pour moi"));
        assert!(!delegates("je ne comprends pas"));
        for (said, meant) in [
            (
                "si l'ordinateur est éteint, le faire une fois quand il se rallume",
                Some("rattraper-une-fois"),
            ),
            ("run it once when it comes back", Some("rattraper-une-fois")),
            ("ne rien faire", Some("sauter")),
            ("rattrape tout, tous les runs manqués", Some("rattraper")),
            ("une fois, ou bien tous les runs", None),
        ] {
            assert_eq!(missed_meaning(said), meant, "{said}");
        }
        for (said, amount) in [
            ("0,10 $", Some(0.10)),
            ("$0.25", Some(0.25)),
            ("0.5 USD", Some(0.5)),
            ("10 centimes", Some(0.10)),
            ("celui à 10 cents", Some(0.10)),
            ("mets 0.10 dollar par run", Some(0.10)),
            ("entre 0.10 dollar et 0.20 dollar", None),
            ("deux", None),
        ] {
            assert_eq!(ceiling_amount(said), amount, "{said}");
        }
        for (said, zone) in [
            ("Paris", Some("Europe/Paris")),
            ("heure de Paris", Some("Europe/Paris")),
            ("Lyon", Some("Europe/Paris")),
            ("Maurice", Some("Indian/Mauritius")),
            ("Montréal", Some("America/Montreal")),
            ("new york", Some("America/New_York")),
            ("nulle part", None),
        ] {
            assert_eq!(zone_named(said).as_deref(), zone, "{said}");
        }
    }
}

#[cfg(test)]
mod activate_tests {
    use super::is_activate;

    /// The activation words, with the courtesy a person adds around them; a request that asks
    /// anything more keeps its route.
    #[test]
    fn a_polite_activation_is_still_the_activation() {
        for line in [
            "active-le alors s'il vous plait.",
            "Active-le, s\u{2019}il vous plaît !",
            "activate it please",
            "ok active la planification",
            "activez-le maintenant",
            "activate",
            // Live gauntlet, 2026-10-16: a yes before it, a space for the hyphen, a question.
            "oui activez le s'il vous plait",
            "vous pouvez l'activer ?",
            "Est-ce que vous pouvez l'activer ?",
            "peux-tu l'activer pour moi ?",
            "Can you activate it, please?",
            "Vous pouvez l'activer pour moi, directement ici ?",
            "active-le tout de suite",
        ] {
            assert!(is_activate(line), "{line}");
        }
        for line in [
            "active-le chaque lundi à 9h",
            "active le mode verbeux",
            "s'il vous plait",
            "lance-le",
        ] {
            assert!(!is_activate(line), "{line}");
        }
    }
}

#[cfg(test)]
mod arm_list_tests {
    use super::arm_list_ends_the_file;

    /// Nika appends to an `arm:` list only when it ends the file: the list it wrote itself.
    #[test]
    fn only_a_list_that_ends_the_file_takes_an_appended_entry() {
        let written = "nika: q08\narm:\n  - workflow: a.nika\n    cadence: \"0 20 * * *\"\n\n";
        assert!(arm_list_ends_the_file(written));
        let followed = "nika: q08\narm:\n  - workflow: a.nika\ntraces:\n  keep: 10\n";
        assert!(!arm_list_ends_the_file(followed));
        assert!(!arm_list_ends_the_file("nika: q08\n"));
    }
}

#[cfg(test)]
mod delegation_tests {
    use super::delegates;

    /// A choice left to Nika on an activation question, with a yes or a reason around it; a
    /// value or another sentence is no delegation.
    #[test]
    fn a_choice_left_to_nika_is_read_as_one() {
        for answer in [
            "choisis pour moi",
            "choisis pour toi stp, ça coûte presque rien",
            "Choisis pour moi, fais au mieux",
            "oui, fais au mieux",
            "you decide",
            "Let Nika choose",
            // Live gauntlet wave 6 r08 B5/B6, 2026-10-18.
            "comme la dernière fois, vous choisissez",
            "le recommandé, celui à 10 cents",
            "the recommended one",
        ] {
            assert!(delegates(answer), "{answer}");
        }
        for answer in ["0.10", "2", "skip", "choisissons demain", "je choisis 0.25"] {
            assert!(!delegates(answer), "{answer}");
        }
    }
}
