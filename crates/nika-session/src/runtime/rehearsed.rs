// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rehearsed copy beside the proposal it qualified, and then beside the workflow saved from it.
//!
//! - **Propose.** A creation that is the closed copy is proposed only as the candidate the copy
//!   door selected. Its preview says the rehearsal ran on a copy of the user's files, what it read
//!   back there, and that nothing ran on the originals. A closed copy no candidate qualified is
//!   refused, and nothing is proposed.
//! - **Witness.** The bytes and the world the selection is bound to are kept privately with the
//!   proposal, then with the workflow its yes saved.
//! - **Yes.** The pending bytes must be the selected ones and the world unchanged (each file the
//!   same bytes, each absence still an absence) before the sources are judged as for every
//!   proposal. Otherwise the proposal is withdrawn.
//! - **Run.** A run of that saved workflow first judges its bytes and the world again. A drift
//!   withdraws the rehearsal and its authority, and the run is refused, again at each later run
//!   line; saying the request again rehearses the project as it is.
//! - **Change.** The closed copy waits as rehearsed. A native proposal may enter a fresh EDIT;
//!   its old proof waits aside, and only returns with unchanged bytes and world if the edit fails
//!   or is cancelled. The revised candidate needs its own live decision.
//!
//! The rehearsals of one turn share one budget: zero at the start of a turn that rehearsed
//! nothing, never reset inside it. Authoring calls and their costs keep their own account.

use std::path::{Path, PathBuf};

use nika_event::source_id::sha256_hex;
use nika_onboard::compile::copy::{
    self, Allowance, Limits, Qualification, Qualified, Usage, Witness, native,
    same_project_path as same_file,
    words::{held_words, rehearsed_lines},
};
use nika_onboard::compile::rehearse::Rehearse;
use nika_onboard::compile::room::ObservedRoom;

use super::{SessionRuntime, TurnOutcome};
use crate::authoring::{AuthoringError, AuthoringRound};
use nika_onboard::compile::{CompileOutcome, CompileStatus};

use crate::change::ProjectChangeSet;
use crate::outcome::{ProposalId, Refusal, RefusalClass};

const MIB: u64 = 1024 * 1024;
/// One candidate's round: its three worlds, one attempt each, within the bytes and the run time
/// three rooms may spend.
const ROUND: Limits = Limits::new(3, 3, 6 * MIB, 30_000);
/// The turn: both candidates' rounds.
const TURN: Limits = Limits::new(6, 6, 12 * MIB, 60_000);

/// The words a change at a rehearsed proposal is answered with.
const HELD: &str = "a change never carries a rehearsal: this proposal waits as it was rehearsed · `no` first, then say the whole new request to rehearse it";

/// A test's rehearsal host, in place of the observed room.
#[cfg(test)]
pub(super) type TestHost = dyn Fn(&Path) -> Box<dyn Rehearse> + Send + Sync;

/// The rehearsals of the conversation: what this turn's rehearsals spent, the proof of the pending
/// proposal by its identity, and that of the saved workflow by its path (or why it was withdrawn).
#[derive(Default)]
pub(super) struct Rehearsals {
    turn: Usage,
    native: Option<(String, native::Preview)>,
    pending: Option<(ProposalId, Proof)>,
    revising: Option<SuspendedProof>,
    saved: Option<(PathBuf, Result<Witness, String>)>,
    #[cfg(test)]
    host: Option<std::sync::Arc<TestHost>>,
}

/// A pending proposal's proof: the witness its rehearsal is bound to, and the lines its preview
/// said of that rehearsal, shown again unchanged when only its money changes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProofOrigin {
    Copy,
    Native,
}

struct SuspendedProof {
    id: ProposalId,
    proof: Proof,
    basis: Option<super::fresh::ProposalBasis>,
}

struct Proof {
    origin: ProofOrigin,
    witness: Witness,
    lines: String,
}

impl Rehearsals {
    pub(super) fn clear_native(&mut self) {
        self.native = None;
    }

    /// A fresh request starts an account; answers and revision questions keep the existing
    /// account and suspended proof. The active proposal and transient preview always expire.
    pub(super) fn new_turn(&mut self, continuing: bool) {
        if !continuing {
            self.turn = Usage::default();
            self.revising = None;
        }
        self.pending = None;
        self.native = None;
    }

    /// The words of the proof bound to `id`, pending or suspended by a revision.
    pub(super) fn lines_of(&self, id: &ProposalId) -> Option<&str> {
        let pending = (self.pending.as_ref()).filter(|(proven, _)| proven == id);
        let suspended = (self.revising.as_ref()).filter(|s| &s.id == id);
        let proof = pending.map(|(_, p)| p).or(suspended.map(|s| &s.proof))?;
        Some(proof.lines.as_str())
    }

    /// Expire pending authority without erasing already spent rehearsal usage.
    pub(super) fn expire_pending(&mut self) {
        self.pending = None;
        self.native = None;
        self.revising = None;
    }
}

/// The observed room over a world's root: the production host.
fn room(world: &Path) -> Box<dyn Rehearse> {
    Box::new(ObservedRoom::new(world))
}

impl SessionRuntime {
    /// A single dispatch returns its account even on a compiler/admission error. Only its live
    /// final-candidate preview may reach the subsequent proposal; JSON provenance is not proof.
    pub(super) fn rehearse_dispatch(
        &mut self,
        intent: &str,
        compile: impl FnOnce(&Self, &dyn Rehearse) -> Result<CompileOutcome, AuthoringError>,
    ) -> Result<CompileOutcome, AuthoringError> {
        self.rehearsals.clear_native();
        let scoped = native::Scoped::new(
            self.snapshot.root.clone(),
            self.native_host(),
            Allowance::new(ROUND, TURN, self.rehearsals.turn),
        );
        let out = compile(self, &scoped);
        let (turn, preview) = scoped.finish().into_parts();
        self.rehearsals.turn = turn;
        let out = out?;
        let preview = preview.map_err(AuthoringError::Runtime)?;
        if out.status == CompileStatus::Ready {
            let preview = preview.ok_or_else(|| {
                AuthoringError::Runtime(
                    "the ready candidate has no live passed or source-only rehearsal decision"
                        .to_owned(),
                )
            })?;
            let exact = out.candidate.as_ref().is_some_and(|candidate| {
                sha256_hex(candidate.as_bytes()) == preview.candidate_sha256()
            });
            if !exact {
                return Err(AuthoringError::Runtime(
                    "the returned candidate differs from the live rehearsal".to_owned(),
                ));
            }
            self.rehearsals.native = Some((intent.to_owned(), preview));
        }
        Ok(out)
    }

    fn native_host(&self) -> Box<dyn Rehearse> {
        #[cfg(test)]
        if let Some(host) = self.rehearsals.host.as_ref() {
            return host(&self.snapshot.root);
        }
        room(&self.snapshot.root)
    }

    /// The copy door over a creation round that compiled Ready. `Ok(None)` when it is no closed
    /// copy (the proposal path is unchanged), the selection when one qualified, or the refusal of
    /// a closed copy no candidate qualified: nothing proposed, nothing written.
    pub(super) fn rehearse_copy(
        &mut self,
        round: &AuthoringRound,
    ) -> Result<Option<Qualified>, TurnOutcome> {
        if round.edit.is_some() {
            return Ok(None);
        }
        let allowance = Allowance::new(ROUND, TURN, self.rehearsals.turn);
        let why = match self.qualify(round, allowance) {
            Qualification::NotCopy => return Ok(None),
            Qualification::Qualified(qualified) => {
                self.rehearsals.turn = qualified.turn;
                return Ok(Some(*qualified));
            }
            Qualification::Refused { why, turn } => {
                self.rehearsals.turn = turn;
                why
            }
            _ => "the copy door answered in a way this session does not know".to_owned(),
        };
        let text = format!(
            "I could not prove a copy that keeps the text exactly: {why} · nothing was proposed and nothing was written · any rehearsals were confined to copies; nothing ran on your files"
        );
        self.remember(&round.intent, &text);
        Err(TurnOutcome::Refusal(Refusal::new(
            RefusalClass::AuthoringRefused,
            text,
        )))
    }

    /// The copy door's answer over the observed room, or a test's host in its place.
    fn qualify(&self, round: &AuthoringRound, allowance: Allowance) -> Qualification {
        let (request, root, scratch) = (round.request(), &self.snapshot.root, std::env::temp_dir());
        #[cfg(test)]
        {
            if let Some(host) = self.rehearsals.host.clone() {
                return copy::qualify(&request, &round.intent, root, &*host, &scratch, allowance);
            }
        }
        copy::qualify(&request, &round.intent, root, &room, &scratch, allowance)
    }

    /// Bind the selection to the proposal `id`, describe it as the one awaiting consent, and make
    /// its preview exact about where it ran. Without a selection, the pending proposal has no
    /// proof.
    pub(super) fn bind_rehearsal(
        &mut self,
        id: &ProposalId,
        qualified: Option<&Qualified>,
        round: &AuthoringRound,
        out: &CompileOutcome,
        preview: &mut String,
    ) -> Result<(), TurnOutcome> {
        let native = self.rehearsals.native.take();
        let proof = if let Some(q) = qualified {
            // Copy may have replaced the native candidate. Its witness wins, its cost does not
            // erase earlier work, and no proof of the earlier candidate is carried across.
            self.last_outcome = Some(q.outcome.clone());
            Some(Proof {
                origin: ProofOrigin::Copy,
                witness: q.witness.clone(),
                lines: rehearsed_lines(q),
            })
        } else if let Some((intent, native)) = native {
            let exact = intent == round.effective_intent()
                && out.candidate.as_ref().is_some_and(|candidate| {
                    sha256_hex(candidate.as_bytes()) == native.candidate_sha256()
                });
            if !exact {
                return Err(
                    self.withdraw(id, "the live rehearsal names another request or candidate")
                );
            }
            self.last_outcome = Some(out.clone());
            let (witness, lines) = native.into_parts();
            if let Some(witness) = witness {
                Some(Proof {
                    origin: ProofOrigin::Native,
                    witness,
                    lines,
                })
            } else {
                *preview = preview.replacen(crate::review::NOTHING_RAN, &lines, 1);
                None
            }
        } else {
            None
        };
        self.rehearsals.pending = proof.map(|proof| {
            *preview = preview.replacen(crate::review::NOTHING_RAN, &proof.lines, 1);
            (id.clone(), proof)
        });
        Ok(())
    }

    /// A money-only amendment of the pending proposal `old`, now `new` over the same set. Its
    /// proof follows only when it proves exactly `old` and the very bytes it selected, and the new
    /// preview shows that same rehearsal beside the new budget: no rehearsal runs, no model is
    /// asked. A proof of another identity or of other bytes withdraws the proposal; a proposal no
    /// rehearsal proves is left as it is.
    pub(super) fn rebind_rehearsal(
        &mut self,
        old: &ProposalId,
        new: &ProposalId,
        set: &ProjectChangeSet,
        preview: &mut String,
    ) -> Result<(), TurnOutcome> {
        let Some((proven, proof)) = self.rehearsals.pending.take() else {
            return Ok(());
        };
        if proven != *old || !selected(set, &proof.witness) {
            return Err(self.withdraw(
                old,
                "a change of money cannot carry a rehearsal of another proposal or of other bytes",
            ));
        }
        preview.push_str(&proof.lines);
        self.rehearsals.pending = Some((new.clone(), proof));
        Ok(())
    }

    /// Whether the pending proposal `id` is a rehearsed one.
    pub(super) fn rehearsed_pending(&self, id: &ProposalId) -> bool {
        self.rehearsals
            .pending
            .as_ref()
            .is_some_and(|(proven, _)| proven == id)
    }

    /// At a yes, before the sources are judged: a rehearsed proposal's bytes must be the
    /// selected ones and its world unchanged, otherwise it is withdrawn. What held, when it did.
    pub(super) fn rehearsed_at_yes(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
    ) -> Result<Option<String>, TurnOutcome> {
        let Some((proven, proof)) = self.rehearsals.pending.take() else {
            return Ok(None);
        };
        // A proof is never dropped in silence: one of another identity withdraws the proposal.
        if proven != *id {
            return Err(self.withdraw(
                id,
                "its rehearsal proved another proposal identity than this one",
            ));
        }
        if !selected(set, &proof.witness) {
            return Err(self.withdraw(
                id,
                "the proposal's bytes are not the ones its rehearsal selected",
            ));
        }
        if let Some(why) = proof.witness.drift(&self.snapshot.root, None) {
            return Err(self.withdraw(
                id,
                &format!("the files this proposal was rehearsed on changed: {why}"),
            ));
        }
        let held = held_words(&proof.witness);
        self.rehearsals.pending = Some((proven, proof));
        Ok(Some(held))
    }

    /// After the yes landed `id`: a rehearsed proposal's proof moves to the workflow it was saved
    /// as, and another workflow landed at a proven path drops that path's proof. Whether the
    /// landed proposal was rehearsed.
    pub(super) fn land_rehearsal(&mut self, id: &ProposalId, workflow: Option<&Path>) -> bool {
        let proof = self
            .rehearsals
            .pending
            .take()
            .filter(|(proven, _)| proven == id);
        let Some(workflow) = workflow else {
            return false;
        };
        let Some((_, proof)) = proof else {
            if self
                .rehearsals
                .saved
                .as_ref()
                .is_some_and(|(path, _)| same_file(&self.snapshot.root, path, workflow))
            {
                self.rehearsals.saved = None;
            }
            return false;
        };
        self.rehearsals.saved = Some((workflow.to_path_buf(), Ok(proof.witness)));
        true
    }

    /// Before a run of `workflow`: when a rehearsal proves it, its bytes and its world are judged
    /// again. A drift withdraws the rehearsal and its authority, and the run is refused; a
    /// withdrawn rehearsal keeps refusing. `None` when the run goes on.
    pub(super) fn rehearsed_at_run(&mut self, workflow: &Path) -> Option<TurnOutcome> {
        let (path, proof) = self.rehearsals.saved.as_ref()?;
        if !same_file(&self.snapshot.root, path, workflow) {
            return None;
        }
        let why = match proof {
            Ok(witness) => witness.drift(&self.snapshot.root, Some(path.as_path()))?,
            Err(why) => why.clone(),
        };
        let path = path.clone();
        self.rehearsals.saved = Some((path.clone(), Err(why.clone())));
        let text = format!(
            "{why} · the rehearsal of `{}` was withdrawn: it no longer describes a run · nothing was run · say the request again to rehearse it over the project as it is now",
            path.display()
        );
        self.remember("(run)", &text);
        Some(TurnOutcome::Refusal(Refusal::new(
            RefusalClass::StaleRevision,
            text,
        )))
    }

    /// A change said at a rehearsed proposal: the proposal waits as it was, with its own proof.
    /// None when no rehearsal proves it: the caller keeps its set.
    pub(super) fn rehearsed_change(&mut self, set: &ProjectChangeSet) -> Option<TurnOutcome> {
        let id = self.proposal_id(set);
        let held = self
            .rehearsals
            .pending
            .as_ref()
            .is_some_and(|(proven, proof)| proven == &id && proof.origin == ProofOrigin::Copy);
        if !held {
            return None;
        }
        self.remember("(change)", HELD);
        Some(self.hold_pending(set.clone(), id, HELD))
    }

    /// Suspend a native proposal's own proof while an EDIT asks for a fresh candidate.
    /// The old bytes/world are checked now and again before any restoration; no proof is copied.
    pub(super) fn suspend_native_rehearsal(
        &mut self,
        set: &ProjectChangeSet,
    ) -> Result<(), TurnOutcome> {
        if self.rehearsals.revising.is_some() {
            let id = self.proposal_id(set);
            return Err(self.withdraw(&id, "another native revision still owns a suspended proof"));
        }
        if self.rehearsals.pending.is_none() {
            return Ok(());
        }
        let id = self.proposal_id(set);
        self.rehearsed_at_yes(set, &id)?;
        let Some((proven, proof)) = self.rehearsals.pending.take() else {
            return Ok(());
        };
        if proof.origin != ProofOrigin::Native {
            self.rehearsals.pending = Some((proven, proof));
            return Err(self.withdraw(&id, "this proof does not admit a native revision"));
        }
        self.rehearsals.revising = Some(SuspendedProof {
            id: proven,
            proof,
            basis: self.basis.take(),
        });
        Ok(())
    }

    /// Restore only the old proposal's proof and source basis after checking its exact bytes,
    /// identity and old world. A changed monetary identity cannot inherit the old witness.
    pub(super) fn restore_native_rehearsal(
        &mut self,
        set: &ProjectChangeSet,
    ) -> Result<(), TurnOutcome> {
        let Some(suspended) = self.rehearsals.revising.take() else {
            return Ok(());
        };
        self.rehearsals.pending = None;
        self.rehearsals.native = None;
        self.pending = None;
        self.basis = None;
        let id = ProposalId::of(&self.draft_preview(set));
        if suspended.id != id || !selected(set, &suspended.proof.witness) {
            return Err(self.withdraw(&id, "the suspended rehearsal names another proposal"));
        }
        if let Some(why) = suspended.proof.witness.drift(&self.snapshot.root, None) {
            return Err(self.withdraw(
                &id,
                &format!("the suspended rehearsal's world changed: {why}"),
            ));
        }
        self.basis = suspended.basis;
        self.bind_proposal_money(&id);
        self.rehearsals.pending = Some((id, suspended.proof));
        Ok(())
    }

    /// A new proposal supersedes the suspended proof; its own binding is already installed.
    pub(super) fn finish_native_revision(&mut self) {
        self.rehearsals.revising = None;
    }

    /// The landed words of a saved workflow: where its rehearsal ran, when it was rehearsed.
    pub(super) fn landed_words(rehearsed: bool) -> &'static str {
        if rehearsed {
            "\nSaved · checked · not active · rehearsed on a copy of your files, nothing has run on them\n  say « run it » to run it once (a ceiling is announced first)"
        } else {
            "\nSaved · checked · not active · nothing has run\n  say « run it » to run it once (a ceiling is announced first)"
        }
    }

    /// A test's rehearsal host in place of the observed room.
    #[cfg(test)]
    pub(super) fn with_rehearsal_host(&mut self, host: std::sync::Arc<TestHost>) {
        self.rehearsals.host = Some(host);
    }
}

/// Whether `set` is exactly one workflow whose bytes are the ones `witness` selected.
fn selected(set: &ProjectChangeSet, witness: &Witness) -> bool {
    matches!(set.changes.as_slice(),
        [change] if sha256_hex(change.content().as_bytes()) == witness.candidate_sha256())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod rehearsed_tests;
