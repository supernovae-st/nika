// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The closed copy door: a request that is exactly `copy SOURCE as is to TARGET`, qualified by
//! what the assembler's two exact copies did when rehearsed, never by how they are written.
//!
//! - The contract is the request's alone: one required write of TARGET holding exactly the text
//!   of SOURCE. No candidate is read to state it.
//! - The candidates are the assembler's two lowerings of the same plan: the text read written
//!   back, and the bytes read as an envelope the write decodes. Each is assembled on an outcome
//!   of its own, Ready and checked, and must be the closed two-task copy with nothing more.
//! - The worlds are fixed before the first run: the user's own files, copied by the room, then
//!   two discriminating worlds this door makes in fixture roots of its own.
//! - The rehearsals run one candidate and one world at a time, each host call admitted by the
//!   budget first. A candidate is judged over its whole list of worlds only, each report bound to
//!   its exact bytes, and the first one certified is selected. The judge's account of the turn is
//!   the only one reported.
//!
//! Nothing here asks a model or composes a pack, and the request a round compiled is used as it
//! is. The user's files are only read, by the room and by the
//! [`Witness`](crate::compile::copy::Witness); the only files written are the fixture roots
//! this door creates and removes.

/// A bounded native dispatch over the same observed world and rehearsal budget.
pub mod native;
/// Pure words describing an existing copy qualification and world witness.
pub mod words;

mod rounds;
mod shape;
mod witness;
mod worlds;

#[cfg(test)]
mod tests;

use std::path::Path;

use nika_compile::surface::assemble::assemble_lowered;
use nika_compile::surface::sha256;
use nika_compile::{CompileError, CompileOutcome, CompileRequest, CompileStatus, initial};
use nika_compile_cognition::rehearse::Rehearse;
use nika_compile_fidelity::behavior::{
    Contract, Presence, Requirement, Written, contract_of_request, read_request,
};

pub use nika_compile::surface::assemble::CopyLowering;
pub use nika_compile_fidelity::behavior::{Admission, Budget, Limits, RunEnd, Usage};
pub use witness::{Seen, Witness, WorldBefore, same_project_path};

/// A rehearsal host over one world's root: the room over the user's project, or over a fixture
/// root this door made.
pub type Host<'a> = dyn Fn(&Path) -> Box<dyn Rehearse> + Sync + 'a;

/// How many characters of the result a preview's excerpt keeps.
pub const EXCERPT: usize = 120;

/// The rehearsal budget of one turn: each candidate is one round of `round` limits, in a turn of
/// `turn` limits that already spent `before` (zero only for a turn that rehearsed nothing yet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Allowance {
    pub round: Limits,
    pub turn: Limits,
    pub before: Usage,
}

impl Allowance {
    /// Rounds of `round` limits, in a turn of `turn` limits that already spent `before`.
    #[must_use]
    pub const fn new(round: Limits, turn: Limits, before: Usage) -> Self {
        Self {
            round,
            turn,
            before,
        }
    }
}

/// What the door decided for one request.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Qualification {
    /// Not the closed copy: the caller keeps its own path, unchanged.
    NotCopy,
    /// The first candidate the contract certified on every world.
    Qualified(Box<Qualified>),
    /// The closed copy, but no candidate qualified: why, in words, and what the turn spent.
    Refused { why: String, turn: Usage },
}

/// The selected candidate and what it was selected on.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Qualified {
    /// The selected candidate's own outcome: Ready, checked, its bytes the ones rehearsed.
    pub outcome: CompileOutcome,
    /// How the selected candidate lowers the source read.
    pub lowering: CopyLowering,
    /// What the user's own world showed.
    pub preview: Preview,
    /// The bytes and the world the selection is bound to.
    pub witness: Witness,
    /// What the turn spent, every candidate judged included.
    pub turn: Usage,
    /// Each judged candidate's lowering and verdict, in order.
    pub verdicts: Vec<String>,
}

/// What the user's own world showed, read back from its room: the result the request names.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Preview {
    /// The target, as the request names it.
    pub target: String,
    /// Whether the room's ledger recorded the run's publish of the target.
    pub published: bool,
    /// The length of the result.
    pub bytes: u64,
    /// The sha256 of the result (lowercase hex).
    pub sha256: String,
    /// The start of the result, escaped, at most [`EXCERPT`] characters of it.
    pub excerpt: String,
    /// The source, as the request names it.
    pub source: String,
    /// The length of the source the room copied.
    pub source_bytes: u64,
    /// The sha256 of the source the room copied.
    pub source_sha256: String,
    /// The target's length before the run, when a file was there.
    pub replaced: Option<u64>,
    /// The worlds the selected candidate was certified on, in order.
    pub worlds: Vec<String>,
}

/// One candidate the assembler gave: its lowering, its own outcome, its exact bytes.
struct Built {
    lowering: CopyLowering,
    outcome: CompileOutcome,
    bytes: String,
    sha256: String,
}

/// The closed copy a request states: its contract, and its two paths as the request names them.
struct Closed {
    contract: Contract,
    source: String,
    target: String,
}

/// Qualify the request's copy: the closed sentence only, its two candidates rehearsed on the
/// frozen worlds one at a time, and the first one certified selected.
///
/// `request` is the round's own request, its answers, money and continuation kept, and `intent`
/// the intent it was compiled from; the caller calls this for a creation only. `root` is the
/// user's project, `host` builds the room over a world's root, and `scratch` is where this door
/// makes its fixture roots. Every host call runs on a thread and an executor of the door's own,
/// never inside the caller's runtime.
#[must_use]
pub fn qualify(
    request: &CompileRequest,
    intent: &str,
    root: &Path,
    host: &Host<'_>,
    scratch: &Path,
    allowance: Allowance,
) -> Qualification {
    let refused = |why: String, turn: Usage| Qualification::Refused { why, turn };
    let (closed, built) = match read(request, intent) {
        None => return Qualification::NotCopy,
        Some(Err(why)) => return refused(why, allowance.before),
        Some(Ok(read)) => read,
    };
    let rehearsed = witness::on_worker(|| {
        rounds::rehearse(rounds::Cx {
            closed: &closed,
            built: &built,
            root,
            host,
            scratch,
            allowance,
        })
    });
    let selected = match rehearsed {
        None => {
            let why = "the rehearsals could not run on an executor of their own".to_owned();
            return refused(why, allowance.before);
        }
        Some(Err((why, turn))) => return refused(why, turn),
        Some(Ok(selected)) => selected,
    };
    let Some(chosen) = built.into_iter().nth(selected.index) else {
        return refused(
            "the selected candidate is not one built".to_owned(),
            selected.turn,
        );
    };
    Qualification::Qualified(Box::new(Qualified {
        outcome: chosen.outcome,
        lowering: chosen.lowering,
        preview: selected.preview,
        witness: selected.witness,
        turn: selected.turn,
        verdicts: selected.verdicts,
    }))
}

/// The closed copy `intent` states and the assembler's two candidates for it; `None` when the
/// request is not the sentence (the strict reading admits it, its production is the copy, and
/// its contract is one required copy of the one source it names), and why when the closed copy
/// has no candidate the door can rehearse.
fn read(request: &CompileRequest, intent: &str) -> Option<Result<(Closed, Vec<Built>), String>> {
    let (plan, provenance) = read_request(intent);
    let production = provenance
        .production
        .as_ref()
        .filter(|_| provenance.admitted())?;
    if production.written != Written::Copy {
        return None;
    }
    let contract = contract_of_request(intent, &request.answers);
    let (source, target) = copied(&contract)?;
    // The HOT door assembles from the intent with its apostrophes folded.
    let folded = folded(intent);
    let assemble = |lowering| {
        let mut out = initial();
        assemble_lowered(&plan, &folded, request, &[], false, lowering, &mut out).map(|()| out)
    };
    let mut built = Vec::with_capacity(2);
    for lowering in [CopyLowering::Text, CopyLowering::Bytes] {
        match one(lowering, assemble(lowering)) {
            Ok(candidate) => built.push(candidate),
            Err(why) => return Some(Err(why)),
        }
    }
    if let [text, bytes] = built.as_slice()
        && text.sha256 == bytes.sha256
    {
        return Some(Err(
            "the two copies are the same bytes: nothing tells them apart".to_owned(),
        ));
    }
    let closed = Closed {
        contract,
        source,
        target,
    };
    Some(Ok((closed, built)))
}

/// The one obligation a closed copy states: a required write of a target holding exactly the text
/// of the one source the request names.
fn copied(contract: &Contract) -> Option<(String, String)> {
    let [obligation] = contract.obligations.as_slice() else {
        return None;
    };
    let Requirement::CopyText { source, .. } = &obligation.requirement else {
        return None;
    };
    let target = obligation.target.as_ref()?;
    let [named] = contract.sources.as_slice() else {
        return None;
    };
    (obligation.presence == Presence::Required && same(named, source))
        .then(|| (source.clone(), target.path.clone()))
}

/// One assembled candidate the door can rehearse: Ready, its check clean, the closed two-task
/// copy under its lowering; else why.
fn one(
    lowering: CopyLowering,
    assembled: Result<CompileOutcome, CompileError>,
) -> Result<Built, String> {
    let word = lowering_word(lowering);
    let outcome =
        assembled.map_err(|error| format!("the {word} copy could not be assembled: {error}"))?;
    let checked = outcome
        .check_preview
        .as_ref()
        .is_some_and(|preview| preview.report.is_clean());
    if outcome.status != CompileStatus::Ready || !checked {
        return Err(format!("the {word} copy is not Ready with a clean check"));
    }
    let bytes = outcome
        .candidate
        .clone()
        .ok_or_else(|| format!("the {word} copy holds no candidate"))?;
    if !shape::closed(&bytes, lowering) {
        return Err(format!(
            "the {word} copy is not the closed two-task copy, so it is never rehearsed"
        ));
    }
    Ok(Built {
        lowering,
        sha256: sha256(&bytes),
        bytes,
        outcome,
    })
}

/// The lowering, in a word.
fn lowering_word(lowering: CopyLowering) -> &'static str {
    match lowering {
        CopyLowering::Text => "text",
        CopyLowering::Bytes => "byte",
        _ => "other",
    }
}

/// The intent with its typographic apostrophes as `'`, as the HOT door folds it.
fn folded(intent: &str) -> String {
    intent
        .chars()
        .map(|c| {
            if matches!(u32::from(c), 0x2018 | 0x2019) {
                '\''
            } else {
                c
            }
        })
        .collect()
}

/// Two paths name the same file: a leading `./` names the same relative path.
fn same(left: &str, right: &str) -> bool {
    left.trim_start_matches("./") == right.trim_start_matches("./")
}
