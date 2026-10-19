// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A declared schedule moved to another zone, or removed, in the person's words, as a proposal
//! of `nika.yaml`'s exact bytes (live gauntlet wave 7 r01, 2026-10-19: four attempts to move a
//! declared schedule from Mauritius to Paris time reached the author, which cannot write
//! `nika.yaml`, and « retirez cette planification » was sent to « désactive-le », which removes a
//! trigger, never the declaration). « oui » writes the bytes shown; a zone moved offers the
//! trigger again once it lands (D1). A schedule this Mac fires is deactivated first, since its
//! unit holds the old declaration. The phrases, read by `nika_session_change::arm`, are NIK-204's corpus.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::schedule::{
    cadence_words, declared_arm_entry, sole_declared, stated_cadence, zone_in_line,
};
use super::{Lang, SessionRuntime, TurnOutcome};
use crate::change::{ProjectChange, ProjectChangeSet, Witness};
use crate::outcome::{Refusal, RefusalClass};
use nika_session_change::arm::{removes_the_schedule, with_cadence, without_entry};
use nika_vocab::project::ArmEntry;

/// What a line asks of the schedule `nika.yaml` declares for a saved workflow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Change {
    /// Its cadence in this other zone, the hour and the day as declared.
    Zone(PathBuf, String),
    /// Its entry removed.
    Remove(PathBuf),
}

impl SessionRuntime {
    /// What `line` asks of the schedule `nika.yaml` declares for the saved workflow (the sole
    /// declared one in a reopened conversation), while nothing else waits: another zone, the hour
    /// and the day kept, or its removal. A line that states another cadence states the schedule
    /// anew, and is the activation's (W17).
    pub(super) fn schedule_change(&self, line: &str) -> Option<Change> {
        if self.pending.is_some() || self.author_asks() {
            return None;
        }
        let root = &self.snapshot.root;
        let workflow = (self.last_workflow.clone()).or_else(|| sole_declared(root))?;
        let entry = declared_arm_entry(root, &workflow)?;
        if removes_the_schedule(line) {
            return Some(Change::Remove(workflow));
        }
        let zone = zone_in_line(line)?;
        let (declared, rest) = entry.cadence.strip_prefix("TZ=")?.split_once(' ')?;
        if declared == zone {
            return None;
        }
        if let Some(words) = stated_cadence(line) {
            let stated = nika_compile::trigger_of(&words).and_then(|t| t.cron);
            let kept = nika_compile::trigger_of(rest)
                .and_then(|t| t.cron)
                .unwrap_or_else(|| rest.trim().to_owned());
            if stated.as_deref() != Some(kept.as_str()) {
                return None;
            }
        }
        Some(Change::Zone(workflow, zone))
    }

    /// The change proposed: `nika.yaml`'s bytes with the entry's cadence in the new zone, or
    /// without the entry; refused in words when this Mac fires the schedule, or when Nika does
    /// not find the entry's lines.
    pub(super) fn schedule_change_turn(&mut self, change: &Change) -> TurnOutcome {
        let lang = self.language;
        let root = self.snapshot.root.clone();
        let (Change::Zone(workflow, _) | Change::Remove(workflow)) = change;
        let shown = workflow.display().to_string();
        if let Some(label) = super::schedule_ledger::label_of(&root, workflow)
            && self.trigger_host.installed(&root, &label)
        {
            return TurnOutcome::Facts(match lang {
                Lang::Fr => format!(
                    "`{shown}` tourne déjà tout seul sur ce Mac : dites d'abord « désactive-le », puis redemandez ce changement · rien n'a changé"
                ),
                _ => format!(
                    "`{shown}` already runs on its own on this Mac: say « deactivate it » first, then ask for this change again · nothing changed"
                ),
            });
        }
        let path = root.join("nika.yaml");
        let (Ok(bytes), Some(entry)) = (std::fs::read(&path), declared_arm_entry(&root, workflow))
        else {
            return refused(lang, &shown);
        };
        let before = String::from_utf8_lossy(&bytes).into_owned();
        let prepared = match change {
            Change::Zone(_, zone) => self.zone_moved(&before, workflow, &entry, zone),
            Change::Remove(_) => self.entry_removed(&before, workflow, &entry),
        };
        match prepared {
            Ok((content, header)) => self.propose_project_file(&root, &bytes, content, header),
            Err(outcome) => outcome,
        }
    }

    /// `nika.yaml` with `workflow`'s entry in `zone`, and the header that says so; its trigger is
    /// offered once it lands (D1).
    fn zone_moved(
        &mut self,
        before: &str,
        workflow: &Path,
        entry: &ArmEntry,
        zone: &str,
    ) -> Result<(String, String), TurnOutcome> {
        let lang = self.language;
        let shown = workflow.display().to_string();
        let Some(rest) = (entry.cadence.strip_prefix("TZ="))
            .and_then(|cadence| cadence.split_once(' '))
            .map(|(_, rest)| rest)
        else {
            return Err(refused(lang, &shown));
        };
        let cadence = format!("TZ={zone} {rest}");
        if let Err(why) = nika_cadence::registry::Cadence::parse(&cadence) {
            return Err(TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!("`{cadence}` is refused by the schedule grammar: {why}"),
            )));
        }
        let Some(content) = with_cadence(before, workflow, &cadence) else {
            return Err(refused(lang, &shown));
        };
        let (was, now) = (
            cadence_words(&entry.cadence, lang),
            cadence_words(&cadence, lang),
        );
        let header = match lang {
            Lang::Fr => format!(
                "Nika propose de changer le fuseau de la planification de `{shown}` :\nExécutions\n  {was} → {now}\n  dans `nika.yaml` : `{}` → `{cadence}`\n  si manqué et plafond inchangés\nChangements\n  ~ `nika.yaml`\nDéclaré n'est pas actif · une fois enregistré, Nika vous propose d'installer le déclencheur sur ce Mac\n",
                entry.cadence
            ),
            _ => format!(
                "Nika proposes to move the schedule of `{shown}` to another zone:\nRuns\n  {was} → {now}\n  in `nika.yaml`: `{}` → `{cadence}`\n  if missed and ceiling unchanged\nChanges\n  ~ `nika.yaml`\nDeclared is not active · once saved, Nika offers to install the trigger on this Mac\n",
                entry.cadence
            ),
        };
        self.declared = Some(super::trigger_install::InstallOffer {
            workflow: workflow.to_path_buf(),
            when: now,
            cadence,
            missed: entry.manque.as_str().to_owned(),
            ceiling: entry.plafond.to_string(),
            lang,
            gap: None,
        });
        self.unscheduled = None;
        Ok((content, header))
    }

    /// `nika.yaml` without `workflow`'s entry, and the header that says so.
    fn entry_removed(
        &mut self,
        before: &str,
        workflow: &Path,
        entry: &ArmEntry,
    ) -> Result<(String, String), TurnOutcome> {
        let lang = self.language;
        let shown = workflow.display().to_string();
        let Some(content) = without_entry(before, workflow) else {
            return Err(refused(lang, &shown));
        };
        let words = cadence_words(&entry.cadence, lang);
        let header = match lang {
            Lang::Fr => format!(
                "Nika propose de retirer la planification de `{shown}` de `nika.yaml` :\n  {words} · ne sera plus déclaré : `{shown}` tournera quand vous le demandez\nChangements\n  ~ `nika.yaml`\n"
            ),
            _ => format!(
                "Nika proposes to remove the schedule of `{shown}` from `nika.yaml`:\n  {words} · no longer declared: `{shown}` runs when you ask\nChanges\n  ~ `nika.yaml`\n"
            ),
        };
        self.declared = None;
        self.unscheduled = Some(workflow.to_path_buf());
        Ok((content, header))
    }

    /// The proposal of `nika.yaml`'s `content`, over the `bytes` it replaces: `header`, then the
    /// exact bytes and their identity; « oui » writes them.
    fn propose_project_file(
        &mut self,
        root: &Path,
        bytes: &[u8],
        content: String,
        header: String,
    ) -> TurnOutcome {
        let lang = self.language;
        let mut preview = header;
        let _ = writeln!(preview, "  ┌─ `nika.yaml`");
        for line in content.lines() {
            let _ = writeln!(preview, "  │ {line}");
        }
        let set = ProjectChangeSet::project_change(
            root,
            &self.intent.goal.clone().unwrap_or_default(),
            ProjectChange::UpdateProjectFile {
                before: Witness::of(bytes),
                content,
            },
        );
        let id = crate::ProposalId::of(&set.preview());
        let _ = writeln!(
            preview,
            "  └─\n  {}",
            lang.pick(
                "identity {id} · `yes` writes these exact bytes · `no` discards",
                "identité {id} · « oui » écrit exactement ces octets · « non » abandonne",
            )
            .replace("{id}", &id.to_string())
        );
        self.remember("(activation)", &format!("(proposed {id})"));
        self.activation_proposal = Some(id.clone());
        self.pending = Some(set);
        TurnOutcome::Proposal { id, preview }
    }

    /// The landed removal of `workflow`'s schedule, said: nothing is declared for it any more.
    pub(super) fn landed_removal(&mut self, workflow: &Path, mut report: String) -> TurnOutcome {
        let shown = workflow.display();
        let said = match self.language {
            Lang::Fr => format!(
                "\nRetiré de `nika.yaml` · `{shown}` n'a plus de planification : il tourne quand vous le demandez (« lance-le »)"
            ),
            _ => format!(
                "\nRemoved from `nika.yaml` · `{shown}` has no schedule any more: it runs when you ask (« run it »)"
            ),
        };
        report.push_str(&said);
        self.last_trigger = None;
        self.note_for_author(&format!(
            "Nika (not the person) removed the schedule of `{shown}` from `nika.yaml` on the person's yes: nothing declares it any more, and it runs only when the person asks. When the person asks whether it runs by itself, say it does not."
        ));
        TurnOutcome::Facts(report)
    }
}

/// The words of a change Nika could not prepare: the entry's lines were not found.
fn refused(lang: Lang, shown: &str) -> TurnOutcome {
    TurnOutcome::Facts(match lang {
        Lang::Fr => format!(
            "Nika ne retrouve pas l'entrée de `{shown}` dans `nika.yaml` telle qu'elle l'écrit · rien n'a changé · `{} arm` liste ce qui est déclaré",
            super::schedule::program()
        ),
        _ => format!(
            "Nika does not find the entry of `{shown}` in `nika.yaml` as it writes one · nothing changed · `{} arm` lists what is declared",
            super::schedule::program()
        ),
    })
}
