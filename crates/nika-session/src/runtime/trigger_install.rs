// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trigger a declared schedule needs, installed on this Mac on the person's own yes (D1,
//! decided by Thibaut on 2026-10-16). Once a declaration lands in `nika.yaml`, the Session asks
//! « Installer le déclencheur sur ce Mac ? », naming the workflow, its cadence and zone, the
//! missed policy, the ceiling per run and the effect: one launchd unit run by the program that
//! wrote the workflow. « Installer et activer » writes the unit through the same path as
//! `arm --emit launchd --write` and loads it, then says when it fires next; « Garder déclaré
//! seulement » leaves the schedule declared. « désactive-le » unloads and removes it. Nothing is
//! installed without that yes, and nothing for a schedule kept declared only.
//!
//! Every live gauntlet persona who scheduled a workflow on 2026-10-16 ended at « Déclaré · pas
//! actif » and a terminal command they could not type.

use std::path::{Path, PathBuf};

use super::{Lang, SessionRuntime, TurnOutcome};
use crate::outcome::{Refusal, RefusalClass};
use crate::work::Offer;

/// The key the install decision is asked under.
pub(super) const INSTALL_KEY: &str = "trigger.install";

/// The schedule a landed declaration names, offered for its trigger.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct InstallOffer {
    /// The workflow it runs, project-relative.
    pub(super) workflow: PathBuf,
    /// The person's words for when it runs.
    pub(super) when: String,
    /// The cadence declared (`TZ=… m h dom mon dow`).
    pub(super) cadence: String,
    /// The missed-run policy, as declared.
    pub(super) missed: String,
    /// The ceiling per scheduled run, in USD, as declared.
    pub(super) ceiling: String,
    /// The language it is asked in.
    pub(super) lang: Lang,
    /// The API keys the workflow's models read from the environment that a unit started by
    /// launchd would not see, when no environment file is given to this project's units.
    pub(super) gap: Option<Vec<String>>,
}

impl InstallOffer {
    /// The decision as the person reads it: what installing does, and the two acts.
    pub(super) fn question(&self, program: &str) -> String {
        let workflow = self.workflow.display();
        let cadence = super::schedule::cadence_words(&self.cadence, self.lang);
        let missed = super::schedule::missed_words(&self.missed, self.lang);
        let (when, ceiling) = (&self.when, &self.ceiling);
        let warning = match (&self.gap, self.lang) {
            (None, _) => String::new(),
            (Some(keys), Lang::Fr) => format!(
                "\n  attention · un run planifié ne voit pas les clés API de votre terminal ({}) : il échouera tant que les unités de ce projet ne chargent pas un fichier qui les contient",
                keys.join(", ")
            ),
            (Some(keys), _) => format!(
                "\n  warning · a scheduled run does not see your terminal's API keys ({}): it fails until this project's units load a file that holds them",
                keys.join(", ")
            ),
        };
        match self.lang {
            Lang::Fr => format!(
                "Installer le déclencheur sur ce Mac ?\n  `{workflow}` · {when} · {cadence}\n  si manqué · {missed} · plafond · {ceiling} $ par run planifié\n  effet · une unité launchd lancée par {program}, qui fera tourner le workflow tout seul{warning}\n  « désactive-le » la retire\n  1 Installer et activer · 2 Garder déclaré seulement"
            ),
            _ => format!(
                "Install the trigger on this Mac?\n  `{workflow}` · {when} · {cadence}\n  if missed · {missed} · ceiling · ${ceiling} per scheduled run\n  effect · one launchd unit run by {program}, which runs the workflow on its own{warning}\n  « deactivate it » removes it\n  1 Install and activate · 2 Keep declared only"
            ),
        }
    }

    /// The two acts, installing first: recommended, unless a scheduled run would not see the
    /// workflow's keys, when keeping it declared is.
    pub(super) fn choices(&self) -> Vec<Offer> {
        let (install, keep) = match self.lang {
            Lang::Fr => ("Installer et activer", "Garder déclaré seulement"),
            _ => ("Install and activate", "Keep declared only"),
        };
        let gap = self.gap.is_some();
        vec![
            Offer::new("install", install, !gap),
            Offer::new("declared", keep, gap),
        ]
    }
}

/// What the person's answer to the install decision is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    Install,
    Keep,
}

/// The answer a line gives the install decision: its option, its number, a yes or a no, said
/// the way people say them; `None` for anything else, which is asked again.
fn answer_of(line: &str) -> Option<Answer> {
    let folded = line
        .trim()
        .trim_end_matches(['.', '!', ' '])
        .replace('\u{2019}', "'")
        .to_lowercase();
    let install = [
        "1",
        "install",
        "installer",
        "installe",
        "installe-le",
        "installer et activer",
        "install and activate",
        "active",
        "active-le",
        "activate",
        "activate it",
    ];
    let keep = [
        "2",
        "declared",
        "garder",
        "garde",
        "garde-le",
        "garder déclaré seulement",
        "keep declared only",
        "keep",
        "annuler",
        "annule",
        "cancel",
    ];
    if install.contains(&folded.as_str())
        || super::decision::is_yes(line)
        || super::decision::is_plain_yes(line)
    {
        Some(Answer::Install)
    } else if keep.contains(&folded.as_str()) || super::decision::is_no(line) {
        Some(Answer::Keep)
    } else {
        None
    }
}

/// What installs, loads and removes a schedule's unit on this machine. The live host runs the
/// program's own `arm --emit launchd --write` and `launchctl`; a test lends one that touches no
/// system.
pub trait TriggerHost: Send {
    /// Whether this host installs triggers at all. One that does not leaves a landed declaration
    /// declared, with the command that installs it, and asks nothing.
    fn installs(&self) -> bool {
        true
    }

    /// Write the unit of the declared schedule `beat` (its label), run by `program`, every other
    /// schedule's unit left as it is: the unit files written.
    ///
    /// # Errors
    /// Why no unit was written, in the emitter's words.
    fn emit(&self, root: &Path, program: &Path, beat: &str) -> Result<Vec<PathBuf>, String>;

    /// Load one written unit, replacing a loaded one of the same name.
    ///
    /// # Errors
    /// Why the unit is not loaded.
    fn load(&self, unit: &Path) -> Result<(), String>;

    /// Unload one unit and remove its file.
    ///
    /// # Errors
    /// Why the unit stays.
    fn remove(&self, unit: &Path) -> Result<(), String>;

    /// Whether this machine holds the unit of the declared schedule `beat` of the project at
    /// `root`: its launchd file in `~/Library/LaunchAgents`, by default. A file there is not
    /// proof that launchd loaded it.
    fn installed(&self, root: &Path, beat: &str) -> bool {
        launch_agent(root, beat).is_some_and(|unit| unit.is_file())
    }

    /// Where this host keeps the unit of the declared schedule `beat` of the project at `root`,
    /// when it says: what « désactive-le » removes of a unit an earlier conversation installed.
    /// `None` by default: such a host removes only the units this conversation installed.
    fn unit_of(&self, _root: &Path, _beat: &str) -> Option<PathBuf> {
        None
    }
}

/// The launchd file of the declared schedule `beat` of the project at `root`, in
/// `~/Library/LaunchAgents`: where `arm --emit launchd --write` writes it.
pub(super) fn launch_agent(root: &Path, beat: &str) -> Option<PathBuf> {
    let home = std::env::home_dir()?;
    let fingerprint = nika_runtime::project_root_fingerprint(root)?;
    let key = nika_cadence::emit::project_key(&fingerprint);
    let unit = format!("{}.plist", nika_cadence::emit::beat_unit(&key, beat));
    Some(home.join("Library/LaunchAgents").join(unit))
}

/// A door that installs no trigger (a machine host, a test): a landed declaration stays
/// declared, said with the command that installs it. Every Session starts with it; a door that
/// runs on the person's own Mac lends [`LaunchdHost`].
#[derive(Debug, Default)]
pub struct DeclaredOnly;

impl TriggerHost for DeclaredOnly {
    fn installs(&self) -> bool {
        false
    }

    fn emit(&self, _: &Path, _: &Path, _: &str) -> Result<Vec<PathBuf>, String> {
        Err("this door installs no trigger".to_owned())
    }

    fn load(&self, _: &Path) -> Result<(), String> {
        Err("this door installs no trigger".to_owned())
    }

    fn remove(&self, _: &Path) -> Result<(), String> {
        Err("this door installs no trigger".to_owned())
    }
}

/// The live host: the running program's own emitter, then `launchctl` in the person's GUI domain.
#[derive(Debug, Default)]
pub struct LaunchdHost;

impl LaunchdHost {
    /// The person's GUI domain (`gui/<uid>`), read from the owner of their home folder.
    fn domain() -> Result<String, String> {
        use std::os::unix::fs::MetadataExt as _;
        let home = std::env::home_dir().ok_or("no home folder")?;
        let uid = std::fs::metadata(&home).map_err(|e| e.to_string())?.uid();
        Ok(format!("gui/{uid}"))
    }

    // disallowed_types: `std::process::Command` — installing a launchd unit is a synchronous
    // one-shot at the Session's surface, on the person's explicit yes (D1); the kernel
    // ShellExecutor seam is async and serves workflow tasks (the nika-cli-host git.rs class).
    #[allow(clippy::disallowed_types)]
    fn launchctl(args: &[&str]) -> Result<(), String> {
        let out = std::process::Command::new("launchctl")
            .args(args)
            .output()
            .map_err(|e| format!("launchctl: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(format!("launchctl {}: {said}", args.join(" ")))
    }
}

impl TriggerHost for LaunchdHost {
    fn unit_of(&self, root: &Path, beat: &str) -> Option<PathBuf> {
        launch_agent(root, beat)
    }

    #[allow(clippy::disallowed_types)] // the same one-shot as `launchctl` above
    fn emit(&self, root: &Path, program: &Path, beat: &str) -> Result<Vec<PathBuf>, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let out = std::process::Command::new(exe)
            .args([
                "arm",
                "--emit",
                "launchd",
                "--write",
                "--beat",
                beat,
                "--nika-bin",
            ])
            .arg(program)
            .current_dir(root)
            .output()
            .map_err(|e| format!("arm --emit: {e}"))?;
        let said = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("{} {}", said.trim(), err.trim()).trim().to_owned());
        }
        let written: Vec<PathBuf> = (said.lines())
            .filter_map(|line| line.trim().strip_prefix("écrit "))
            .map(PathBuf::from)
            .filter(|path| path.extension().is_some_and(|e| e == "plist"))
            .collect();
        if written.is_empty() {
            return Err(format!("no unit was written: {}", said.trim()));
        }
        Ok(written)
    }

    fn load(&self, unit: &Path) -> Result<(), String> {
        let domain = Self::domain()?;
        let unit = unit.to_string_lossy();
        if Self::launchctl(&["bootstrap", &domain, &unit]).is_ok() {
            return Ok(());
        }
        // Loaded already under that name: replaced by these bytes.
        let _ = Self::launchctl(&["bootout", &domain, &unit]);
        Self::launchctl(&["bootstrap", &domain, &unit])
    }

    fn remove(&self, unit: &Path) -> Result<(), String> {
        let domain = Self::domain()?;
        let _ = Self::launchctl(&["bootout", &domain, &unit.to_string_lossy()]);
        match std::fs::remove_file(unit) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", unit.display())),
        }
    }
}

/// The program a unit runs, by an absolute path that survives an update: the one the person
/// ran Nika as (`nika-dev` through its proxy), found on `PATH` when it was named alone, else the
/// running executable.
fn program_path() -> Option<PathBuf> {
    let first = std::env::args_os().next().map(PathBuf::from)?;
    if first.is_absolute() {
        return Some(first);
    }
    let name = super::schedule::program();
    #[allow(clippy::disallowed_methods)] // PATH, a list of folders, never a secret
    let path = std::env::var_os("PATH")?;
    (std::env::split_paths(&path))
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
        .or_else(|| std::env::current_exe().ok())
}

impl SessionRuntime {
    /// Offer the trigger of the declaration that just landed, when it named one: the decision the
    /// next line answers.
    pub(super) fn offer_install(&mut self, report: String) -> TurnOutcome {
        let Some(mut offer) = self.declared.take() else {
            return TurnOutcome::Facts(report);
        };
        if !self.trigger_host.installs() {
            return TurnOutcome::Facts(report);
        }
        offer.gap = unit_access_gap(&self.snapshot.root, &offer.workflow);
        let question = format!(
            "{report}\n\n{}",
            offer.question(&super::schedule::program())
        );
        self.install_offer = Some(offer);
        TurnOutcome::Question {
            key: INSTALL_KEY.to_owned(),
            question,
        }
    }

    /// The install decision's rows, while it waits.
    pub(super) fn install_choices(&self) -> Option<(&'static str, String, Vec<Offer>, bool)> {
        let offer = self.install_offer.as_ref()?;
        let sentence = offer.question(&super::schedule::program());
        Some((INSTALL_KEY, sentence, offer.choices(), false))
    }

    /// The person's answer to the install decision: install and load the unit, keep the schedule
    /// declared, or ask again.
    pub(super) fn answer_install(&mut self, line: &str) -> TurnOutcome {
        let Some(offer) = self.install_offer.clone() else {
            return TurnOutcome::Facts(String::new());
        };
        let lang = offer.lang;
        match answer_of(line) {
            // Another line (« pourquoi ? », « ça installe quoi ? ») is answered with what the
            // install does, and the decision still waits: nothing is installed without its answer.
            None => {
                let words = super::schedule::cadence_words(&offer.cadence, lang);
                let shown = offer.workflow.display();
                TurnOutcome::Question {
                    key: INSTALL_KEY.to_owned(),
                    question: match lang {
                        Lang::Fr => format!(
                            "Installer ajoute à ce Mac une unité launchd qui lance `{shown}` {words} ; « désactive-le » la retire. Rien n'est installé sans votre réponse : 1 pour installer et activer le déclencheur, 2 pour garder la planification déclarée seulement."
                        ),
                        _ => format!(
                            "Installing adds a launchd unit to this Mac that runs `{shown}` {words}; « deactivate it » removes it. Nothing is installed without your answer: 1 to install and activate the trigger, 2 to keep the schedule declared only."
                        ),
                    },
                }
            }
            Some(Answer::Keep) => {
                self.install_offer = None;
                TurnOutcome::Facts(
                    lang.pick(
                        "kept declared · nothing was installed · « activate it » offers the trigger again",
                        "gardé déclaré · rien n'a été installé · « active-le » propose à nouveau le déclencheur",
                    )
                    .to_owned(),
                )
            }
            Some(Answer::Install) => {
                self.install_offer = None;
                self.install_trigger(&offer)
            }
        }
    }

    /// Write and load the unit of `offer`'s schedule, then say when it fires next.
    fn install_trigger(&mut self, offer: &InstallOffer) -> TurnOutcome {
        let lang = offer.lang;
        let root = self.snapshot.root.clone();
        let refused = |why: &str| {
            let said = lang.pick(
                "the trigger was not installed: {why} · the schedule stays declared",
                "le déclencheur n'a pas été installé : {why} · la planification reste déclarée",
            );
            TurnOutcome::Refusal(Refusal::new(RefusalClass::Io, said.replace("{why}", why)))
        };
        let Some(program) = program_path() else {
            return refused("the program to run was not found");
        };
        let Some(beat) = super::schedule_ledger::label_of(&root, &offer.workflow) else {
            return refused("`nika.yaml` no longer declares this schedule");
        };
        let units = match self.trigger_host.emit(&root, &program, &beat) {
            Ok(units) => units,
            Err(why) => return refused(&why),
        };
        for unit in &units {
            if let Err(why) = self.trigger_host.load(unit) {
                return refused(&why);
            }
        }
        self.installed.extend(units.iter().cloned());
        let next = next_fire(&offer.cadence);
        self.note_for_author(&format!(
            "Nika (not the person) installed the trigger of `{}` on this Mac on the person's yes: a launchd unit now runs it on its own{}. When the person asks whether it is active, say it is installed and active. « désactive-le » removes it: the Session's own door does it, never you.",
            offer.workflow.display(),
            next.as_deref().map_or(String::new(), |next| format!(" (next run {next})"))
        ));
        let said = match (lang, next) {
            (Lang::Fr, Some(next)) => format!(
                "Installé et actif · `{}` tournera tout seul sur ce Mac · prochain run : {next}\n  « désactive-le » le retire",
                offer.workflow.display()
            ),
            (Lang::Fr, None) => format!(
                "Installé et actif · `{}` tournera tout seul sur ce Mac\n  « désactive-le » le retire",
                offer.workflow.display()
            ),
            (_, Some(next)) => format!(
                "Installed and active · `{}` runs on its own on this Mac · next run: {next}\n  « deactivate it » removes it",
                offer.workflow.display()
            ),
            (_, None) => format!(
                "Installed and active · `{}` runs on its own on this Mac\n  « deactivate it » removes it",
                offer.workflow.display()
            ),
        };
        TurnOutcome::Facts(said)
    }

    /// « désactive-le »: unload and remove the units this conversation installed.
    pub(super) fn deactivate_turn(&mut self) -> TurnOutcome {
        let lang = self.language;
        let root = self.snapshot.root.clone();
        // Every unit of this project's schedules the host holds now, a reopened conversation's
        // included (live gauntlet r08, 2026-10-17: after a reopen, « désactive-le » found none).
        if let Some((_, registry)) = super::schedule_ledger::registry(&root) {
            for label in nika_cadence::emit::labels(&registry) {
                if self.trigger_host.installed(&root, &label)
                    && let Some(unit) = self.trigger_host.unit_of(&root, &label)
                    && !self.installed.contains(&unit)
                {
                    self.installed.push(unit);
                }
            }
        }
        if self.installed.is_empty() {
            // How to remove the declaration itself, which « désactive-le » never does (live
            // gauntlet wave 7 r01, 2026-10-19: « retirez cette planification » led nowhere).
            return TurnOutcome::Facts(
                lang.pick(
                    "no trigger installed by this conversation · nothing runs on its own · the schedule stays declared in `nika.yaml` · « remove the schedule » removes it from the file",
                    "aucun déclencheur installé par cette conversation · rien ne tourne tout seul · la planification reste déclarée dans `nika.yaml` · « retire la planification » la retire du fichier",
                )
                .to_owned(),
            );
        }
        let units = std::mem::take(&mut self.installed);
        for unit in &units {
            if let Err(why) = self.trigger_host.remove(unit) {
                self.installed.clone_from(&units);
                let said = lang.pick(
                    "the trigger was not removed: {why}",
                    "le déclencheur n'a pas été retiré : {why}",
                );
                return TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::Io,
                    said.replace("{why}", &why),
                ));
            }
        }
        let journaled = super::schedule_ledger::journal_disarms(&self.snapshot.root, &units, lang);
        let said = lang.pick(
            "deactivated · the trigger was removed from this Mac · the schedule stays declared in `nika.yaml`",
            "désactivé · le déclencheur a été retiré de ce Mac · la planification reste déclarée dans `nika.yaml`",
        );
        self.note_for_author(
            "Nika (not the person) removed the trigger from this Mac on the person's « désactive-le »: its launchd unit is unloaded and deleted, and nothing fires the schedule any more; `nika.yaml` still declares it. When the person asks whether it is active, say it is no longer active.",
        );
        TurnOutcome::Facts(format!("{said}{journaled}"))
    }
}

/// The API keys `workflow`'s models read from the environment that a unit started by launchd
/// would not see: none when its models need no key (a mock, a local server) or when this
/// project's units load an environment file (`arm --emit --env-file`). A provider the catalogue
/// does not know is named, since its access cannot be judged.
fn unit_access_gap(root: &Path, workflow: &Path) -> Option<Vec<String>> {
    let source = std::fs::read_to_string(root.join(workflow)).ok()?;
    let wf = crate::review::parse(&source)?;
    let default = wf.model.as_ref().map(|model| model.value.clone());
    // The names one provider's key can go by, per provider.
    let mut needed: Vec<Vec<String>> = Vec::new();
    for task in &wf.tasks {
        let model = match &task.value.action {
            nika_schema::raw::RawAction::Infer(infer) => infer.model.as_ref(),
            nika_schema::raw::RawAction::Agent(agent) => agent.model.as_ref(),
            _ => continue,
        };
        let Some(model) = model.map(|m| m.value.clone()).or_else(|| default.clone()) else {
            continue;
        };
        let provider = model.split('/').next().unwrap_or(&model).to_owned();
        let names = match nika_catalog::find_provider(&provider) {
            Some(row) if !row.requires_key => continue,
            Some(row) => {
                let mut names = vec![format!("NIKA_{}_API_KEY", row.id.to_uppercase())];
                if !row.env_var.is_empty() {
                    names.push(row.env_var.to_owned());
                }
                names
            }
            None if provider == "mock" => continue,
            None => vec![format!("`{provider}`")],
        };
        if !needed.contains(&names) {
            needed.push(names);
        }
    }
    let given = units_env_names(root);
    let missing: Vec<String> = (needed.into_iter())
        .filter(|names| !names.iter().any(|name| given.contains(name)))
        .flatten()
        .collect();
    (!missing.is_empty()).then_some(missing)
}

/// The variable names the environment file this project's units load assigns (`KEY=value`,
/// `export KEY=value`): its names only, never a value. None without such a file.
fn units_env_names(root: &Path) -> Vec<String> {
    let Some(file) = units_env_file(root) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Vec::new();
    };
    (text.lines())
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (name, _) = line.split_once('=')?;
            let name = name.trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

/// Whether this project's units load an environment file: one given to `arm --emit` and kept
/// for the project, or the one its existing units already source.
fn units_env_file(root: &Path) -> Option<std::path::PathBuf> {
    let (file, _) = nika_vocab::project::discover(root).ok().flatten()?;
    let fingerprint = nika_runtime::project_root_fingerprint(root)?;
    let project =
        nika_arm::unit_io::ProjectUnits::new(nika_cadence::emit::project_key(&fingerprint), file);
    let agents = std::env::home_dir().map(|home| home.join("Library/LaunchAgents"));
    nika_arm::unit_io::resolve_env_file(None, root, root, agents.as_deref(), &project)
        .ok()
        .flatten()
}

/// When a declared cadence fires next, in its own zone, to the minute.
pub(super) fn next_fire(cadence: &str) -> Option<String> {
    let parsed = nika_cadence::registry::Cadence::parse(cadence).ok()?;
    let slot = parsed.next_after(&jiff::Zoned::now())?;
    Some(slot.at.strftime("%Y-%m-%d %H:%M %Z").to_string())
}

/// Whether a line asks to remove the installed trigger: « désactive-le », « retire le
/// déclencheur de ce Mac », « deactivate it », never a line about anything else (« désactive le
/// mode verbeux »).
pub(super) fn is_deactivate(line: &str) -> bool {
    let words = deactivation_words(line);
    let said = match words.iter().position(|w| PLACE.contains(&w.as_str())) {
        // « … de ce Mac », « … from this Mac »: where it is removed from, nothing more.
        Some(at) if at > 0 && names_this_mac(&words[at], &words[at + 1..]) => &words[..at],
        Some(_) => return false,
        None => &words[..],
    };
    let said = said.join(" ");
    let alone = matches!(
        said.as_str(),
        "désactive-le"
            | "désactive-la"
            | "désactivez-le"
            | "désactivez-la"
            | "désactive"
            | "désactiver"
            | "desactive-le"
            | "desactive-la"
            | "desactive"
            | "desactiver"
            | "désinstalle-le"
            | "désinstallez-le"
            | "retire-le"
            | "retirez-le"
            | "enlève-le"
            | "enlevez-le"
            | "deactivate"
            | "deactivate it"
            | "disarm it"
            | "uninstall it"
            | "remove it"
            | "turn it off"
    );
    let trigger = said.split_once(' ').is_some_and(|(verb, object)| {
        REMOVE.contains(&verb)
            && matches!(
                object,
                "le déclencheur"
                    | "ce déclencheur"
                    | "son déclencheur"
                    | "la planification"
                    | "the trigger"
                    | "its trigger"
                    | "the schedule"
            )
    });
    alone || trigger
}

/// « de ce Mac », « du Mac », « from this Mac ».
fn names_this_mac(place: &str, rest: &[String]) -> bool {
    match rest {
        [this, mac] => THIS.contains(&this.as_str()) && mac == "mac",
        [mac] => place == "du" && mac == "mac",
        _ => false,
    }
}

/// The verbs that remove a trigger, before what they remove.
const REMOVE: &[&str] = &[
    "désactive",
    "désactivez",
    "desactive",
    "retire",
    "retirez",
    "enlève",
    "enlevez",
    "supprime",
    "supprimez",
    "désinstalle",
    "désinstallez",
    "deactivate",
    "disarm",
    "remove",
    "uninstall",
];

/// The word before « ce Mac » / « this Mac ».
const PLACE: &[&str] = &["de", "du", "sur", "from", "on"];

/// « ce Mac », « this Mac ».
const THIS: &[&str] = &["ce", "this"];

/// The words around a deactivation that ask nothing more.
const DEACTIVATION_COURTESY: &[&str] = &[
    "stp",
    "svp",
    "please",
    "merci",
    "maintenant",
    "now",
    "alors",
    "oui",
];

/// A line's lowered words, its courtesy and punctuation aside.
fn deactivation_words(line: &str) -> Vec<String> {
    let lowered = line.replace('\u{2019}', "'").to_lowercase();
    // A period ends a word, never splits `alpha.nika`.
    let mut words: Vec<String> = (lowered
        .split(|c: char| c.is_whitespace() || "!,?;:«»".contains(c)))
    .map(|word| word.trim_end_matches('.'))
    .filter(|word| !word.is_empty())
    .map(str::to_owned)
    .collect();
    while words
        .first()
        .is_some_and(|w| DEACTIVATION_COURTESY.contains(&w.as_str()))
    {
        words.remove(0);
    }
    while words
        .last()
        .is_some_and(|w| DEACTIVATION_COURTESY.contains(&w.as_str()))
    {
        words.pop();
    }
    words
}

impl SessionRuntime {
    /// Whether a line asks to remove a trigger: [`is_deactivate`], or « désactive » with the
    /// name of a workflow `nika.yaml` declares or the conversation saved (« désactive
    /// journal-citation-histoire », live gauntlet r08, 2026-10-17).
    pub(super) fn deactivates(&self, line: &str) -> bool {
        if is_deactivate(line) {
            return true;
        }
        let words = deactivation_words(line);
        let [verb, name] = words.as_slice() else {
            return false;
        };
        // Only a verb that deactivates: « supprime alpha » may ask to delete the workflow.
        if ![
            "désactive",
            "désactivez",
            "desactive",
            "deactivate",
            "disarm",
        ]
        .contains(&verb.as_str())
        {
            return false;
        }
        let name = name.trim_start_matches("./");
        let names = |workflow: &Path| {
            let path = workflow.to_string_lossy().to_lowercase();
            let stem = path.trim_end_matches(".nika").to_owned();
            path == name || stem == name || stem.rsplit('/').next() == Some(name)
        };
        let declared = nika_vocab::project::discover(&self.snapshot.root)
            .ok()
            .flatten()
            .is_some_and(|(_, project)| {
                project
                    .arm()
                    .iter()
                    .any(|entry| names(Path::new(&entry.workflow)))
            });
        declared || self.last_workflow.as_deref().is_some_and(names)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use super::{Answer, INSTALL_KEY, TriggerHost, answer_of, is_deactivate};
    use crate::runtime::TurnOutcome;
    use crate::runtime::tests::{saved_daily_copy, tree};

    /// A host that writes no system: it records each gesture and names one unit.
    #[derive(Clone, Default)]
    struct Recorded(Arc<Mutex<Vec<String>>>);

    impl Recorded {
        fn seen(&self) -> Vec<String> {
            self.0.lock().expect("record").clone()
        }
        fn push(&self, said: String) {
            self.0.lock().expect("record").push(said);
        }
    }

    /// The unit `arm --emit` names for `beat` of the project at `root`.
    fn unit(root: &Path, beat: &str) -> PathBuf {
        let fingerprint = nika_runtime::project_root_fingerprint(root).expect("the root resolves");
        let key = nika_cadence::emit::project_key(&fingerprint);
        PathBuf::from("/units").join(format!(
            "{}.plist",
            nika_cadence::emit::beat_unit(&key, beat)
        ))
    }

    impl TriggerHost for Recorded {
        fn emit(&self, root: &Path, program: &Path, beat: &str) -> Result<Vec<PathBuf>, String> {
            assert!(
                root.join("nika.yaml").is_file(),
                "emitted from the declared project"
            );
            assert!(
                program.is_absolute(),
                "an absolute program: {}",
                program.display()
            );
            self.push(format!("emit {beat}"));
            Ok(vec![unit(root, beat)])
        }
        fn load(&self, unit: &Path) -> Result<(), String> {
            self.push(format!("load {}", unit.display()));
            Ok(())
        }
        fn remove(&self, unit: &Path) -> Result<(), String> {
            self.push(format!("remove {}", unit.display()));
            Ok(())
        }
        fn unit_of(&self, root: &Path, beat: &str) -> Option<PathBuf> {
            Some(unit(root, beat))
        }
        fn installed(&self, root: &Path, beat: &str) -> bool {
            let loaded = format!("load {}", unit(root, beat).display());
            let removed = format!("remove {}", unit(root, beat).display());
            let seen = self.seen();
            let last_load = seen.iter().rposition(|said| *said == loaded);
            let last_removal = seen.iter().rposition(|said| *said == removed);
            last_load.is_some_and(|at| last_removal.is_none_or(|gone| gone < at))
        }
    }

    /// The install decision reads its options, their numbers and a yes or a no.
    #[test]
    fn the_install_answer_reads_the_ways_people_say_it() {
        for line in [
            "1",
            "Installer et activer",
            "oui",
            "Oui, allez-y",
            "installe-le",
            "yes",
        ] {
            assert_eq!(answer_of(line), Some(Answer::Install), "{line}");
        }
        for line in [
            "2",
            "Garder déclaré seulement",
            "non",
            "annuler",
            "garde-le",
        ] {
            assert_eq!(answer_of(line), Some(Answer::Keep), "{line}");
        }
        for line in ["peut-être", "et si je change l'heure ?", ""] {
            assert_eq!(answer_of(line), None, "{line}");
        }
        for line in [
            "Désactive-le.",
            "deactivate it",
            "désactive-le stp",
            "retire le déclencheur de ce Mac",
            "Retire le déclencheur",
            "supprime le déclencheur de ce mac",
            "désactive-le de ce Mac",
            "désactive la planification",
            "remove the trigger from this Mac",
            "turn it off",
        ] {
            assert!(is_deactivate(line), "{line}");
        }
        for line in [
            "désactive le mode verbeux",
            "supprime le fichier rapport.md",
            "retire le déclencheur de ce dossier",
            "retire la ligne 3",
        ] {
            assert!(!is_deactivate(line), "{line}");
        }
    }

    /// The French daily copy declared, under a host that installs: the declaration asks for its
    /// trigger, which nothing installs before the yes.
    fn declared(host: &Recorded) -> (tempfile::TempDir, crate::SessionRuntime) {
        let dir = tree();
        let mut s = saved_daily_copy(dir.path());
        s.with_trigger_host(Box::new(host.clone()));
        let _ = s.turn("active-le");
        let _ = s.turn("Europe/Paris");
        let _ = s.turn("1");
        let TurnOutcome::Proposal { .. } = s.turn("0.20") else {
            panic!("the declaration is proposed");
        };
        let TurnOutcome::Question { key, question } = s.consent("oui") else {
            panic!("the landed declaration asks for its trigger");
        };
        assert_eq!(key, INSTALL_KEY);
        assert!(
            question.contains("Déclaré dans `nika.yaml`")
                && question.contains("Installer le déclencheur sur ce Mac ?")
                && question.contains("une unité launchd")
                && question.contains("« désactive-le » la retire"),
            "{question}"
        );
        assert_eq!(s.pending_activation(), Some(INSTALL_KEY));
        let (_, _, rows, _) = s.waiting_activation_choices().expect("its two acts");
        let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(keys, ["install", "declared"]);
        assert!(rows[0].recommended && !rows[1].recommended);
        assert!(host.seen().is_empty(), "nothing installed before the yes");
        (dir, s)
    }

    /// A declared schedule moves to another zone in the person's words, the hour, missed policy
    /// and ceiling kept, its trigger offered again once it lands; nothing is installed before
    /// the yes (live gauntlet wave 7 r01, 2026-10-19).
    #[test]
    fn a_declared_schedule_moves_to_the_zone_the_person_names() {
        let host = Recorded::default();
        let (dir, mut s) = declared(&host);
        let _ = s.turn("declared");
        let TurnOutcome::Proposal { preview, .. } =
            s.turn("Je suis à Maurice en fait. Mettez plutôt l'heure de Maurice s'il vous plaît.")
        else {
            panic!("the zone change is proposed");
        };
        assert!(
            preview.contains("changer le fuseau")
                && preview.contains("TZ=Indian/Mauritius 0 8 * * *"),
            "{preview}"
        );
        let TurnOutcome::Question { key, .. } = s.consent("oui") else {
            panic!("the moved schedule offers its trigger");
        };
        assert_eq!(key, INSTALL_KEY);
        let file = std::fs::read_to_string(dir.path().join("nika.yaml")).expect("nika.yaml");
        assert!(
            file.contains("cadence: \"TZ=Indian/Mauritius 0 8 * * *\""),
            "{file}"
        );
        assert!(file.contains("plafond: 0.2"), "{file}");
        assert!(host.seen().is_empty(), "nothing installed before the yes");
    }

    /// « retirez cette planification » proposes `nika.yaml` without the entry; its yes removes
    /// it and says so; « désactive-le » with nothing installed says how (live gauntlet wave 7
    /// r01, 2026-10-19).
    #[test]
    fn a_declared_schedule_is_removed_in_the_persons_words() {
        let host = Recorded::default();
        let (dir, mut s) = declared(&host);
        let _ = s.turn("declared");
        let TurnOutcome::Facts(none) = s.turn("désactive-le") else {
            panic!("nothing to deactivate");
        };
        assert!(none.contains("« retire la planification »"), "{none}");
        let TurnOutcome::Proposal { preview, .. } =
            s.turn("Alors retirez cette planification, s'il vous plaît.")
        else {
            panic!("the removal is proposed");
        };
        assert!(preview.contains("retirer la planification"), "{preview}");
        let TurnOutcome::Facts(removed) = s.consent("oui") else {
            panic!("the removal lands");
        };
        assert!(removed.contains("Retiré de `nika.yaml`"), "{removed}");
        let workflow = s.last_workflow.clone().expect("saved");
        assert!(super::super::schedule::declared_arm_entry(dir.path(), &workflow).is_none());
        assert!(host.seen().is_empty());
    }

    /// A schedule this Mac fires is deactivated first: its unit holds the old declaration.
    #[test]
    fn a_fired_schedule_is_deactivated_before_it_changes() {
        let host = Recorded::default();
        let (_dir, mut s) = declared(&host);
        let _ = s.turn("install");
        let TurnOutcome::Facts(first) = s.turn("Mettez plutôt l'heure de Maurice s'il vous plaît.")
        else {
            panic!("refused in words");
        };
        assert!(first.contains("« désactive-le »"), "{first}");
        assert!(s.pending_proposal().is_none());
    }

    /// The declaration a host shows carries its schedule in the person's words: when it runs,
    /// what a missed run does and its ceiling, never the cron alone (supernovae-1c, live gauntlet
    /// r01 B6 and r08 B14: the raw cron on the nika.yaml card).
    #[test]
    fn a_declaration_candidate_says_its_schedule_in_words() {
        let dir = tree();
        let mut s = saved_daily_copy(dir.path());
        let _ = s.turn("active-le");
        let _ = s.turn("Europe/Paris");
        let _ = s.turn("1");
        let TurnOutcome::Proposal { .. } = s.turn("0.20") else {
            panic!("the declaration is proposed");
        };
        let candidate = s.work().candidate.expect("the declaration");
        let words = candidate.schedule.expect("its schedule words");
        assert_eq!(words.cadence, "tous les jours à 8h (heure de Paris)");
        assert!(
            words.missed.starts_with("si le Mac est éteint : "),
            "{}",
            words.missed
        );
        assert_eq!(words.ceiling, "plafond 0,20 $ par run");
    }

    /// « Installer et activer » writes and loads the unit and says when it fires next;
    /// « désactive-le » removes it.
    #[test]
    fn the_yes_installs_and_loads_the_trigger_and_deactivate_removes_it() {
        let host = Recorded::default();
        let (dir, mut s) = declared(&host);
        let TurnOutcome::Facts(installed) = s.turn("1") else {
            panic!("the install is said");
        };
        assert!(
            installed.starts_with("Installé et actif") && installed.contains("prochain run :"),
            "{installed}"
        );
        let history = s.schedule_history();
        let [schedule] = history.as_slice() else {
            panic!("one declared schedule: {history:?}");
        };
        let unit = unit(dir.path(), &schedule.label);
        assert_eq!(
            host.seen(),
            [
                format!("emit {}", schedule.label),
                format!("load {}", unit.display())
            ]
        );
        assert_eq!(s.pending_activation(), None, "nothing waits any more");
        let TurnOutcome::Facts(removed) = s.turn("désactive-le") else {
            panic!("the removal is said");
        };
        assert!(
            removed.starts_with("désactivé")
                && removed.contains(&format!(
                    "journalisé : disarmed dans `.nika/arm/{}/history.ndjson`",
                    schedule.label
                )),
            "{removed}"
        );
        assert_eq!(
            host.seen().last(),
            Some(&format!("remove {}", unit.display()))
        );
        let after = s.schedule_history();
        assert_eq!((after[0].disarmed, after[0].fired), (1, 0), "{after:?}");
    }

    /// A schedule kept declared, then « active-le » again: no unit on this Mac, so its trigger is
    /// offered at once, and the yes installs it; once installed, « active-le » says it is active
    /// (supernovae-1c, real use on 9b15de12f, 2026-10-17: a schedule declared the day before
    /// answered « déjà déclaré · pas prouvé actif » and offered nothing).
    #[test]
    fn activating_a_declared_schedule_offers_its_trigger() {
        let host = Recorded::default();
        let (dir, mut s) = declared(&host);
        let TurnOutcome::Facts(kept) = s.turn("2") else {
            panic!("kept declared");
        };
        assert!(kept.starts_with("gardé déclaré"), "{kept}");
        let TurnOutcome::Question { key, question } = s.turn("active-le") else {
            panic!("the trigger of the declared schedule is offered");
        };
        assert_eq!(key, INSTALL_KEY);
        assert!(
            question.contains("est déjà déclaré dans `nika.yaml`")
                && question.contains("aucun déclencheur sur ce Mac")
                && question.contains("Installer le déclencheur sur ce Mac ?"),
            "{question}"
        );
        assert!(host.seen().is_empty(), "nothing installed before the yes");
        let TurnOutcome::Facts(installed) = s.turn("oui") else {
            panic!("the install is said");
        };
        assert!(installed.starts_with("Installé et actif"), "{installed}");
        let label = s.schedule_history()[0].label.clone();
        let unit = unit(dir.path(), &label);
        assert_eq!(
            host.seen(),
            [format!("emit {label}"), format!("load {}", unit.display())]
        );
        let TurnOutcome::Facts(active) = s.turn("active-le") else {
            panic!("an installed schedule is said active");
        };
        assert!(
            active.contains("est déjà actif sur ce Mac") && active.contains("prochain run :"),
            "{active}"
        );
    }

    /// A reopened conversation knows nothing of how the schedule was asked: « active-le » still
    /// activates the one `nika.yaml` declares, in French (live gauntlet r01, 2026-10-17:
    /// « `bilan-octobre.nika` asked for no schedule » while /status said it was declared).
    #[test]
    fn a_reopened_conversation_activates_the_schedule_nika_yaml_declares() {
        let host = Recorded::default();
        let (dir, mut s) = declared(&host);
        let TurnOutcome::Facts(_) = s.turn("2") else {
            panic!("kept declared");
        };
        let mut reopened = crate::SessionRuntime::open(
            dir.path(),
            crate::runtime::tests::ready(
                crate::intelligence::IntelligenceKind::None,
                crate::intelligence::DataLocus::None,
            ),
            Box::new(crate::reasoner::NoReasoner),
        );
        reopened.with_trigger_host(Box::new(host.clone()));
        let TurnOutcome::Question { key, question } = reopened.turn("active-le") else {
            panic!("the declared schedule's trigger is offered");
        };
        assert_eq!(key, INSTALL_KEY);
        assert!(
            question.contains("est déjà déclaré dans `nika.yaml`")
                && question.contains("Installer le déclencheur sur ce Mac ?")
                && !question.contains("asked for no schedule"),
            "{question}"
        );
        assert!(host.seen().is_empty(), "nothing installed before the yes");
    }

    /// « est-ce que c'est actif ? » is answered by the Session from `nika.yaml` and this Mac, at
    /// every step: saved with a schedule asked, declared and kept declared, installed (live
    /// gauntlet r08 and r02, 2026-10-17: the model said « pas actif » right after the install).
    #[test]
    fn whether_it_runs_on_its_own_is_the_sessions_answer() {
        let dir = tree();
        let mut saved = saved_daily_copy(dir.path());
        let TurnOutcome::Facts(asked) = saved.turn("est-ce qu'il est activé ?") else {
            panic!("the Session answers");
        };
        assert!(
            asked.starts_with("Pas encore : `")
                && asked.contains(
                    "la planification demandée (« chaque matin à 8h ») n'est pas déclarée"
                ),
            "{asked}"
        );
        let host = Recorded::default();
        let (_dir, mut s) = declared(&host);
        let TurnOutcome::Facts(waiting) = s.turn("est-ce que c'est actif ?") else {
            panic!("the Session answers");
        };
        assert!(
            waiting.contains("la question d'installation attend votre réponse"),
            "{waiting}"
        );
        assert_eq!(
            s.pending_activation(),
            Some(INSTALL_KEY),
            "the decision still waits"
        );
        let TurnOutcome::Facts(_) = s.turn("2") else {
            panic!("kept declared");
        };
        let TurnOutcome::Facts(kept) = s.turn("Est-ce que c'est bien actif ?") else {
            panic!("the Session answers");
        };
        assert!(
            kept.starts_with("Pas encore : `")
                && kept.contains(
                    "est déclaré dans `nika.yaml` (tous les jours à 8h (heure de Paris))"
                )
                && kept.contains("« active-le » propose de l'installer"),
            "{kept}"
        );
        let _ = s.turn("active-le");
        let TurnOutcome::Facts(installed) = s.turn("1") else {
            panic!("installed");
        };
        assert!(installed.starts_with("Installé et actif"), "{installed}");
        let TurnOutcome::Facts(active) = s.turn("c'est actif maintenant ?") else {
            panic!("the Session answers");
        };
        assert!(
            active.starts_with("Oui : `")
                && active.contains("est installé sur ce Mac et tourne tout seul")
                && active.contains("prochain run :"),
            "{active}"
        );
    }

    /// A workflow whose model reads a key from the terminal's environment: the card says a
    /// scheduled run will not see it and recommends keeping the schedule declared; a mock model
    /// or an environment file given to the units leaves the card as it is (supernovae-1c,
    /// 2026-10-17).
    #[test]
    fn a_unit_that_would_not_see_the_keys_is_said_before_the_yes() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(dir.path().join("nika.yaml"), "nika: proj\n").expect("project");
        std::fs::write(
            dir.path().join("brief.nika"),
            "nika: brief\nmodel: anthropic/claude-sonnet-5\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n",
        )
        .expect("workflow");
        let gap = super::unit_access_gap(dir.path(), Path::new("brief.nika"));
        assert_eq!(
            gap.as_deref(),
            Some(
                &[
                    "NIKA_ANTHROPIC_API_KEY".to_owned(),
                    "ANTHROPIC_API_KEY".to_owned()
                ][..]
            )
        );
        let offer = super::InstallOffer {
            workflow: PathBuf::from("brief.nika"),
            when: "chaque matin à 8h".to_owned(),
            cadence: "TZ=Europe/Paris 0 8 * * *".to_owned(),
            missed: "sauter".to_owned(),
            ceiling: "0.2".to_owned(),
            lang: crate::runtime::Lang::Fr,
            gap,
        };
        let question = offer.question("nika-dev");
        assert!(
            question.contains("un run planifié ne voit pas les clés API de votre terminal (NIKA_ANTHROPIC_API_KEY, ANTHROPIC_API_KEY)"),
            "{question}"
        );
        let rows = offer.choices();
        assert!(
            !rows[0].recommended && rows[1].recommended,
            "keeping it declared is recommended"
        );
        std::fs::write(
            dir.path().join("mock.nika"),
            "nika: m\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n",
        )
        .expect("mock workflow");
        assert_eq!(
            super::unit_access_gap(dir.path(), Path::new("mock.nika")),
            None
        );
        std::fs::create_dir_all(dir.path().join(".nika/arm")).expect("arm dir");
        std::fs::write(dir.path().join("keys.env"), "OTHER_KEY=x\n").expect("env file");
        std::fs::write(dir.path().join(".nika/arm/env-file"), "keys.env\n").expect("sidecar");
        assert!(
            super::unit_access_gap(dir.path(), Path::new("brief.nika")).is_some(),
            "a file that names none of its keys does not cover the run"
        );
        std::fs::write(dir.path().join("keys.env"), "# keys\nANTHROPIC_API_KEY=x\n")
            .expect("env file");
        assert_eq!(
            super::unit_access_gap(dir.path(), Path::new("brief.nika")),
            None,
            "the units load a file that names its key"
        );
    }

    /// A trigger an earlier conversation installed is removed by « désactive-le » in a new one,
    /// and its disarm journaled (live gauntlet r08, 2026-10-17: after a reopen, nothing was
    /// removed while the model said nothing was installed).
    #[test]
    fn deactivate_removes_a_trigger_an_earlier_conversation_installed() {
        let host = Recorded::default();
        let (dir, mut s) = declared(&host);
        let TurnOutcome::Facts(_) = s.turn("1") else {
            panic!("installed");
        };
        let label = s.schedule_history()[0].label.clone();
        // A new Session in the same project knows nothing of the earlier one's install.
        let mut reopened = crate::SessionRuntime::open(
            dir.path(),
            crate::runtime::tests::ready(
                crate::intelligence::IntelligenceKind::None,
                crate::intelligence::DataLocus::None,
            ),
            Box::new(crate::reasoner::NoReasoner),
        );
        reopened.with_trigger_host(Box::new(host.clone()));
        // Its name says which: « désactive copie » (live gauntlet r08, 2026-10-17).
        let (_, project) = nika_vocab::project::discover(dir.path())
            .expect("nika.yaml")
            .expect("a project");
        let workflow = project.arm()[0].workflow.clone();
        let stem = workflow.trim_end_matches(".nika");
        assert!(reopened.deactivates(&format!("désactive {stem}")), "{stem}");
        assert!(
            reopened.deactivates(&format!("Désactive {workflow} stp")),
            "{workflow}"
        );
        assert!(!reopened.deactivates(&format!("supprime {stem}")), "{stem}");
        assert!(!reopened.deactivates("désactive autre-chose"));
        let TurnOutcome::Facts(removed) = reopened.turn("désactive-le") else {
            panic!("the removal is said");
        };
        assert!(removed.starts_with("désactivé"), "{removed}");
        assert_eq!(
            host.seen().last(),
            Some(&format!("remove {}", unit(dir.path(), &label).display()))
        );
    }

    /// While the install decision waits, every line belongs to it, never to the conversation's
    /// model: a row key, a label, a question; « désactive-le » and the engine's facts are the
    /// Session's own doors afterwards (live gauntlet wave 5, 2026-10-17: the model read the row
    /// key « declared » and the card came back after every answer).
    #[test]
    fn the_install_decision_and_the_deactivation_never_reach_the_model() {
        let host = Recorded::default();
        let (_dir, mut s) = declared(&host);
        for line in [
            "declared",
            "Garder déclaré seulement",
            "est-ce que c'est actif ?",
            "2",
        ] {
            assert!(s.agent_yields(line), "{line}");
        }
        let TurnOutcome::Facts(kept) = s.turn("declared") else {
            panic!("the row key keeps it declared");
        };
        assert!(kept.starts_with("gardé déclaré"), "{kept}");
        assert_eq!(
            s.pending_activation(),
            None,
            "the card is answered for good"
        );
        assert!(s.agent_yields("désactive-le"));
        assert!(s.agent_yields("est-ce que alpha.nika est valide ?"));
        assert!(!s.agent_yields("résume mes notes de la semaine"));
    }

    /// « Garder déclaré seulement » installs nothing, and the schedule stays declared. A question
    /// before it is answered with what the install does, and the decision still waits.
    #[test]
    fn keeping_it_declared_installs_nothing() {
        let host = Recorded::default();
        let (_dir, mut s) = declared(&host);
        let TurnOutcome::Question { key, question } = s.turn("pourquoi ? ça installe quoi ?")
        else {
            panic!("the decision still waits");
        };
        assert_eq!(key, INSTALL_KEY);
        assert!(
            question.contains("une unité launchd qui lance `")
                && question.contains("« désactive-le » la retire")
                && question.contains("Rien n'est installé sans votre réponse"),
            "{question}"
        );
        assert!(host.seen().is_empty(), "a question installs nothing");
        let TurnOutcome::Facts(kept) = s.turn("2") else {
            panic!("the choice is said");
        };
        assert!(kept.starts_with("gardé déclaré"), "{kept}");
        assert!(
            host.seen().is_empty(),
            "nothing installed: {:?}",
            host.seen()
        );
        assert_eq!(s.pending_activation(), None);
    }
}
