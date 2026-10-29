// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The execution constraint of an applied Save, bound to that journal entry,
//! file identity and exact bytes. Reading it restores no consent or inference account.

use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use nika_fs::OwnedDir;
use serde::{Deserialize, Serialize};

use super::{CapKnowledge, MonetaryDecision, MonetarySource, SavedMoney, SessionRuntime, Witness};
use crate::consent::{ConsentDecision, ConsentRecord};
use crate::money::InferenceEnforcement;

const FILE: &str = "saved-money.ndjson";
const UNREADABLE: &str = "saved monetary evidence is unreadable — state an explicit Run ceiling or prepare and review the workflow again; no default was substituted";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    proposal: String,
    consent_at: String,
    bindings: Vec<Binding>,
}

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    path: PathBuf,
    resolved: PathBuf,
    witness: String,
    original_intent: String,
    input: String,
    effective_usd: f64,
    source: MonetarySource,
    explicit_amount: Option<String>,
    project_default_usd: Option<f64>,
    project_file: Option<PathBuf>,
}

impl Binding {
    fn of(saved: &SavedMoney) -> Option<Self> {
        let decision = &saved.decision;
        if decision.refusal.is_some() || decision.source == MonetarySource::Rejected {
            return None;
        }
        Some(Self {
            path: saved.path.clone(),
            resolved: saved.resolved.clone()?,
            witness: saved.witness.0.clone(),
            original_intent: crate::broker::redact(&decision.original_intent).0,
            input: crate::broker::redact(&decision.input).0,
            effective_usd: decision.effective_usd?,
            source: decision.source,
            explicit_amount: decision.explicit_amount.clone(),
            project_default_usd: decision.project_default_usd,
            project_file: decision.project_file.clone(),
        })
    }

    fn decision(&self) -> Result<MonetaryDecision, &'static str> {
        if !self.effective_usd.is_finite()
            || self.effective_usd < 0.0
            || self
                .project_default_usd
                .is_some_and(|n| !n.is_finite() || n < 0.0)
        {
            return Err(UNREADABLE);
        }
        let valid = match (
            self.source,
            self.explicit_amount.is_some(),
            self.project_default_usd,
        ) {
            (MonetarySource::SessionDefault, false, None)
            | (MonetarySource::Explicit, true, None)
            | (MonetarySource::Override, true, Some(_)) => true,
            (MonetarySource::ProjectDefault, false, Some(n)) => {
                n.to_bits() == self.effective_usd.to_bits()
            }
            _ => false,
        };
        if !valid {
            return Err(UNREADABLE);
        }
        Ok(MonetaryDecision {
            original_intent: self.original_intent.clone(),
            input: self.input.clone(),
            effective_usd: Some(self.effective_usd),
            source: self.source,
            explicit_amount: self.explicit_amount.clone(),
            project_default_usd: self.project_default_usd,
            project_file: self.project_file.clone(),
            policy_cap: CapKnowledge::Unknown {
                reason: "saved execution constraint proves no independent policy cap".into(),
            },
            machine_cap: CapKnowledge::Unknown {
                reason: "saved execution constraint proves no independent machine cap".into(),
            },
            inference: InferenceEnforcement::NotMetered,
            admission: None,
            observed_cost_usd: None,
            proposal: None,
            refusal: None,
        })
    }
}

impl SessionRuntime {
    /// Called only after the exact consent record was successfully appended.
    pub(in crate::runtime) fn save_money_evidence(&self, consent: &ConsentRecord) -> String {
        if consent.decision != ConsentDecision::Applied {
            return String::new();
        }
        let bindings = self
            .money
            .saved
            .iter()
            .filter(|saved| {
                saved
                    .decision
                    .proposal
                    .as_ref()
                    .is_some_and(|p| p.as_str() == consent.proposal)
                    && consent.written.contains(&saved.path)
                    && consent
                        .witnesses
                        .iter()
                        .any(|w| w.path == saved.path && w.after == saved.witness.0)
            })
            .filter_map(Binding::of)
            .collect::<Vec<_>>();
        if bindings.is_empty() {
            return String::new();
        }
        let record = Record {
            version: 1,
            proposal: consent.proposal.clone(),
            consent_at: consent.at.clone(),
            bindings,
        };
        let write = || -> io::Result<()> {
            for binding in &record.bindings {
                binding.decision().map_err(io::Error::other)?;
            }
            let dir = OwnedDir::open(&self.snapshot.root)?.create_below(&[".nika"])?;
            dir.append_line(FILE, &serde_json::to_string(&record)?)
        };
        match write() {
            Ok(()) => String::new(),
            Err(error) => format!(
                "\n  ⚠ the saved execution constraint was not recorded (.nika/{FILE}): {error}; after reopening, state an explicit Run ceiling or prepare and review again"
            ),
        }
    }

    /// Receipt a ceiling picked for a Save with no durable execution constraint. An existing
    /// valid receipt stays authoritative; changed bytes and unreadable evidence still refuse.
    pub(in crate::runtime) fn receipt_picked_ceiling(
        &mut self,
        workflow: &Path,
        checked: Option<&Witness>,
        proposal: &crate::outcome::ProposalId,
    ) -> Result<(), String> {
        let checked = checked.ok_or(UNREADABLE)?;
        let resolved = self
            .snapshot
            .root
            .join(workflow)
            .canonicalize()
            .map_err(|_| UNREADABLE)?;
        let now =
            Witness::of(&std::fs::read(self.snapshot.root.join(workflow)).map_err(|_| UNREADABLE)?);
        if &now != checked {
            return Err("the saved workflow changed since its execution constraint was recorded — prepare and review the revision before running".into());
        }
        let records = ConsentRecord::read_all(&self.snapshot.root).map_err(|_| UNREADABLE)?;
        let consent = records
            .iter()
            .rev()
            .find(|record| {
                record
                    .written
                    .iter()
                    .any(|path| self.names(path, workflow, &resolved))
            })
            .ok_or(UNREADABLE)?;
        let valid = records.iter().all(|r| r.version == ConsentRecord::VERSION)
            && consent.decision == ConsentDecision::Applied
            && consent.proposal == proposal.as_str()
            && consent.written.iter().any(|path| path == workflow)
            && consent
                .witnesses
                .iter()
                .any(|w| w.path == workflow && w.after == now.0);
        if !valid {
            return Err(UNREADABLE.into());
        }
        if restore(&self.snapshot.root, consent, &resolved, Some(&now))?.is_some() {
            return Ok(());
        }
        let mut decision = self.money.current.clone().ok_or(UNREADABLE)?;
        if decision.explicit_amount.is_none() {
            decision.explicit_amount = Some(decision.input.trim().to_owned());
        }
        decision.proposal = Some(proposal.clone());
        self.money.saved.retain(|saved| saved.path != workflow);
        self.money.saved.push(SavedMoney {
            resolved: Some(resolved),
            path: workflow.to_path_buf(),
            witness: now,
            decision,
        });
        let warning = self.save_money_evidence(consent);
        if warning.is_empty() {
            Ok(())
        } else {
            self.money.saved.pop();
            Err(warning.trim().to_owned())
        }
    }
}

/// The constraint of this exact latest Save, never an earlier matching byte string.
pub(super) fn restore(
    root: &Path,
    consent: &ConsentRecord,
    resolved: &Path,
    witness: Option<&Witness>,
) -> Result<Option<MonetaryDecision>, &'static str> {
    if consent.decision != ConsentDecision::Applied {
        return Err(
            "the latest Save is incomplete — prepare and review the workflow again before running",
        );
    }
    let file = match OwnedDir::open(root)
        .and_then(|dir| dir.open_relative(&Path::new(".nika").join(FILE)))
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(UNREADABLE),
    };
    let mut found: Option<Binding> = None;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|_| UNREADABLE)?;
        if line.trim().is_empty() {
            continue;
        }
        let record: Record = serde_json::from_str(&line).map_err(|_| UNREADABLE)?;
        if record.version != 1 {
            return Err(UNREADABLE);
        }
        if record.proposal != consent.proposal || record.consent_at != consent.at {
            continue;
        }
        for binding in record.bindings {
            if !consent.written.contains(&binding.path) {
                return Err(UNREADABLE);
            }
            let claims = consent
                .witnesses
                .iter()
                .any(|w| w.path == binding.path && w.after == binding.witness);
            if !claims {
                return Err(UNREADABLE);
            }
            let same_path = root
                .join(&binding.path)
                .canonicalize()
                .is_ok_and(|p| p == resolved);
            if !same_path && binding.resolved != resolved {
                continue;
            }
            if !same_path
                || binding.resolved != resolved
                || witness.map(|w| w.0.as_str()) != Some(binding.witness.as_str())
            {
                return Err(
                    "the saved workflow changed since its execution constraint was recorded — prepare and review the revision before running",
                );
            }
            binding.decision()?;
            if let Some(previous) = &found {
                // Consent timestamps have second precision. Repeating the exact
                // same evidence is harmless; conflicting provenance is not.
                if previous != &binding
                    || previous.effective_usd.to_bits() != binding.effective_usd.to_bits()
                    || previous.project_default_usd.map(f64::to_bits)
                        != binding.project_default_usd.map(f64::to_bits)
                {
                    return Err(UNREADABLE);
                }
            } else {
                found = Some(binding);
            }
        }
    }
    found.map(|binding| binding.decision()).transpose()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
