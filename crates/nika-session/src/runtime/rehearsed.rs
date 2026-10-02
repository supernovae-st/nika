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
//! - **Change.** A change said at a rehearsed proposal never carries the rehearsal: the proposal
//!   waits as it was.
//!
//! The rehearsals of one turn share one budget: zero at the start of a turn that rehearsed
//! nothing, never reset inside it. Authoring calls and their costs keep their own account.

use std::path::{Component, Path, PathBuf};

use nika_event::source_id::sha256_hex;
use nika_onboard::compile::copy::{
    self, Allowance, CopyLowering, Limits, Qualification, Qualified, Seen, Usage, Witness,
};
use nika_onboard::compile::rehearse::Rehearse;
use nika_onboard::compile::room::ObservedRoom;

use super::{SessionRuntime, TurnOutcome};
use crate::authoring::AuthoringRound;
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
    pending: Option<(ProposalId, Proof)>,
    saved: Option<(PathBuf, Result<Witness, String>)>,
    #[cfg(test)]
    host: Option<std::sync::Arc<TestHost>>,
}

/// A pending proposal's proof: the witness its rehearsal is bound to, and the lines its preview
/// said of that rehearsal, shown again unchanged when only its money changes.
struct Proof {
    witness: Witness,
    lines: String,
}

impl Rehearsals {
    /// A new turn: nothing rehearsed in it yet, and the pending proposal, discarded with it, no
    /// longer proven.
    pub(super) fn new_turn(&mut self) {
        self.turn = Usage::default();
        self.pending = None;
    }
}

/// The observed room over a world's root: the production host.
fn room(world: &Path) -> Box<dyn Rehearse> {
    Box::new(ObservedRoom::new(world))
}

impl SessionRuntime {
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
        preview: &mut String,
    ) {
        let Some(q) = qualified else {
            self.rehearsals.pending = None;
            return;
        };
        let lines = rehearsed_lines(q);
        *preview = preview.replacen(crate::review::NOTHING_RAN, &lines, 1);
        self.last_outcome = Some(q.outcome.clone());
        let witness = q.witness.clone();
        self.rehearsals.pending = Some((id.clone(), Proof { witness, lines }));
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
        if !self.rehearsed_pending(&id) {
            return None;
        }
        self.remember("(change)", HELD);
        Some(self.hold_pending(set.clone(), id, HELD))
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

/// The preview's line on execution for a rehearsed copy: where it ran and what it read back
/// there, from the user's own world only.
fn rehearsed_lines(q: &Qualified) -> String {
    let p = &q.preview;
    let published = if p.published {
        "published by the run"
    } else {
        "not published by the run"
    };
    let before = p.replaced.map_or_else(
        || format!("`{}` did not exist", p.target),
        |bytes| format!("replaces `{}` ({bytes} B)", p.target),
    );
    let lowering = match q.lowering {
        CopyLowering::Text => "text",
        CopyLowering::Bytes => "byte",
        _ => "closed",
    };
    format!(
        "Rehearsed once on a copy of your files · nothing ran on the originals · `yes` saves these exact bytes and checks them · running is its own line (« run it »)\n  read back · `{}` {published} · {} B · sha256 {} · the whole text\n    « {} »\n  from `{}` · {} B · sha256 {} · {before}\n  the {lowering} copy held on every world: {}\n",
        p.target,
        p.bytes,
        short(&p.sha256),
        p.excerpt,
        p.source,
        p.source_bytes,
        short(&p.source_sha256),
        p.worlds.join(" · ")
    )
}

/// What the yes found of the rehearsed world, in words.
fn held_words(witness: &Witness) -> String {
    let parts: Vec<String> = witness
        .world()
        .iter()
        .map(|(path, seen)| match seen {
            Seen::File(digest) => format!("`{path}` the same {} B", digest.bytes),
            Seen::Absent => format!("`{path}` still absent"),
            _ => format!("`{path}` as rehearsed"),
        })
        .collect();
    format!(
        "the rehearsed world holds: {} (the bytes are the selected ones)",
        parts.join(" · ")
    )
}

/// Whether `set` is exactly one workflow whose bytes are the ones `witness` selected.
fn selected(set: &ProjectChangeSet, witness: &Witness) -> bool {
    matches!(set.changes.as_slice(),
        [change] if sha256_hex(change.content().as_bytes()) == witness.candidate_sha256())
}

/// The first twelve characters of a digest.
fn short(sha256: &str) -> &str {
    sha256.get(..12).unwrap_or(sha256)
}

/// Whether two paths name the same file of the project at `root`: `.` components dropped, an
/// absolute path read under the root.
fn same_file(root: &Path, left: &Path, right: &Path) -> bool {
    let normal = |path: &Path| -> PathBuf {
        let path = path.strip_prefix(root).unwrap_or(path);
        path.components()
            .filter(|part| !matches!(part, Component::CurDir))
            .collect()
    };
    normal(left) == normal(right)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod rehearsed_tests;
