// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The source basis of a proposal, bound where a compile outcome is proposed and judged again at
//! its yes (C9 · F4). The compiler records the source facts a candidate's program relies on
//! ([`Basis::sources`]); a yes observes exactly those sources again through the host's one
//! bounded observer and the compiler judges them for the request that compiled the bytes, kept
//! with the observation its round read, never one made later ([`basis_for`]). A moved or
//! unjudgeable basis withdraws the proposal before any write, consent record or money effect: its
//! bytes stay in the conversation as evidence, the goal stays, and saying the request again
//! builds it over the project as it is. Rows added, removed or reordered never move it. A
//! proposal no compile bound (a kept draft proposed again) takes its basis from a zero-call
//! deterministic compile of its request only when that compile gives its exact bytes; otherwise
//! a workflow that reads project files is withdrawn with the request to say it again. What is not
//! judged is said, never presented as fresh. A conversation's proposal has no compile behind its
//! bytes: its basis is the witness of each project file it reads, as its author read it or as it
//! was when proposed, and any change to those bytes withdraws it at the yes ([`Read`]).

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

use nika_fs::OwnedDir;
use nika_onboard::compile::{Basis, CompileOutcome, CompileRequest, basis_for};
use serde_json::{Map, Value, json};

use super::authoring::DETERMINISTIC;
use super::{Refusal, RefusalClass, SessionRuntime, TurnOutcome};
use crate::authoring::{Reading, compile_in};
use crate::change::{ProjectChangeSet, Witness};
use crate::outcome::ProposalId;

/// The source basis a compile outcome gave the proposal it was proposed as.
pub(super) struct ProposalBasis {
    /// The proposal it was bound to.
    id: ProposalId,
    /// The exact bytes it justifies (a money-only amendment names the same bytes anew).
    bytes: Vec<Witness>,
    /// The exact request its compile round read, and its decision record (`None`: none).
    request: CompileRequest,
    decision: Option<Value>,
    /// A conversation's proposal: the files it reads, witnessed; `None` for a compile's.
    read: Option<Read>,
}

/// The project files a conversation's proposal reads: each with the witness of its bytes (`None`:
/// absent or unreadable then), and the path patterns no witness covers.
#[derive(Clone)]
struct Read {
    files: Vec<(String, Option<Witness>)>,
    patterns: Vec<String>,
}

/// The bytes of a project file a witness reads before it folds in the file's length instead.
const FILE_BOUND: u64 = 64 * 1024 * 1024;

/// The witness of the project file a workflow names `path` as its bytes are now under `root`:
/// `None` when it is absent or cannot be read there. Past [`FILE_BOUND`] it witnesses the first
/// bytes and the file's length.
fn file_witness(root: &Path, path: &str) -> Option<Witness> {
    let relative = Path::new(path.trim_start_matches("./"));
    let file = OwnedDir::open(root).ok()?.open_relative(relative).ok()?;
    let length = file.metadata().ok()?.len();
    let mut bytes = Vec::new();
    file.take(FILE_BOUND).read_to_end(&mut bytes).ok()?;
    if length > FILE_BOUND {
        bytes.extend_from_slice(format!("\n(length {length})").as_bytes());
    }
    Some(Witness::of(&bytes))
}

/// The witnesses of a set's exact bytes, in set order.
fn witnesses(set: &ProjectChangeSet) -> Vec<Witness> {
    set.changes
        .iter()
        .map(|change| Witness::of(change.content().as_bytes()))
        .collect()
}

impl SessionRuntime {
    /// Bind what `out` records of its sources to the proposal `id` made of `set`, before any yes;
    /// nothing when the request its round read cannot be rebuilt (the yes then finds none bound).
    pub(super) fn bind_basis(
        &mut self,
        id: &ProposalId,
        set: &ProjectChangeSet,
        request: Option<CompileRequest>,
        out: &CompileOutcome,
    ) {
        if let Some(source) = out.candidate.as_deref() {
            nika_onboard::compile::program_records::remember(
                &mut self.programs,
                nika_onboard::compile::program_records::Place::Proposal(&id.to_string()),
                source,
                out.provenance.plan.as_ref(),
                &|text| crate::broker::redact(text).0,
            );
        }
        self.basis = request.map(|request| ProposalBasis {
            id: id.clone(),
            bytes: witnesses(set),
            request,
            decision: out.provenance.decision.clone(),
            read: None,
        });
    }

    /// Bind the files a conversation's proposal `id` of `set` reads, before any yes: no compile
    /// recorded what its program relies on, so the yes judges these exact bytes. A file the
    /// author read through the desk keeps the witness of what it read (`seen`).
    pub(super) fn bind_read_basis(
        &mut self,
        id: &ProposalId,
        set: &ProjectChangeSet,
        seen: &BTreeMap<String, Witness>,
    ) {
        let (mut files, mut patterns) = (Vec::new(), Vec::new());
        for path in set.project_reads() {
            if path.contains(['*', '?', '[']) {
                patterns.push(path);
                continue;
            }
            let held =
                (seen.get(&path).cloned()).or_else(|| file_witness(&self.snapshot.root, &path));
            files.push((path, held));
        }
        self.basis = Some(ProposalBasis {
            id: id.clone(),
            bytes: witnesses(set),
            request: CompileRequest::create(&set.goal),
            decision: None,
            read: Some(Read { files, patterns }),
        });
    }

    /// At a yes, before anything lands: what the landed report says of a rehearsed world and of
    /// the sources, or the withdrawal of a proposal whose world or sources moved or cannot be
    /// judged. A rehearsed copy's world is judged first (`rehearsed.rs`), then the sources as for
    /// every proposal.
    pub(super) fn basis_at_yes(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
    ) -> Result<Option<String>, TurnOutcome> {
        let rehearsed = self.rehearsed_at_yes(set, id)?;
        let sources = self.sources_at_yes(set, id)?;
        Ok(match (rehearsed, sources) {
            (Some(rehearsed), Some(sources)) => Some(format!("{rehearsed}\n  {sources}")),
            (rehearsed, sources) => rehearsed.or(sources),
        })
    }

    /// The sources a proposal was built on, judged again before anything lands.
    fn sources_at_yes(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
    ) -> Result<Option<String>, TurnOutcome> {
        let bound = self
            .basis
            .take()
            .filter(|b| b.id == *id || b.bytes == witnesses(set));
        if let Some(read) = bound.as_ref().and_then(|b| b.read.clone()) {
            return self.read_at_yes(id, &read);
        }
        let reads = set.project_reads();
        let (decision, request, derived) = match bound {
            Some(b) => (b.decision, b.request, false),
            None => match self.derive(set) {
                Some((out, request)) => (out.provenance.decision, request, true),
                None if reads.is_empty() => {
                    return Ok(Some(
                        "source freshness not judged: it reads no project file".to_owned(),
                    ));
                }
                None => {
                    return Err(self.withdraw(
                        id,
                        &format!(
                            "this proposal reads {}, but its source basis was not kept and its request, compiled again without AI, does not give its exact bytes: its sources cannot be judged",
                            quoted(&reads)
                        ),
                    ));
                }
            },
        };
        let again = if derived {
            "source basis derived again before writing: its request, compiled again without AI, gives these exact bytes · "
        } else {
            "sources judged again before writing: "
        };
        match self.judge(decision.as_ref(), &request) {
            Basis::Holds(n) => Ok(Some(format!(
                "{again}{n} recorded fact(s) of {} hold (rows added, removed or reordered never move them)",
                quoted(&Basis::sources(decision.as_ref()))
            ))),
            Basis::Moved(why) => Err(self.withdraw(
                id,
                &format!(
                    "the sources this proposal was built on changed: {}",
                    why.join(" · ")
                ),
            )),
            Basis::None if reads.is_empty() => {
                Ok(derived.then(|| format!("{again}it reads no project file")))
            }
            Basis::None => Ok(Some(format!(
                "source freshness not judged: the compiler recorded no source fact these bytes rely on (they read {})",
                quoted(&reads)
            ))),
            Basis::Unjudged(why) => Err(self.withdraw(
                id,
                &format!(
                    "the sources this proposal was built on cannot be judged again: {}",
                    why.join(" · ")
                ),
            )),
            _ => Err(self.withdraw(
                id,
                "the sources this proposal was built on cannot be judged by this engine",
            )),
        }
    }

    /// A conversation's proposal at its yes: each file it reads judged by the witness of the
    /// bytes it was proposed over; a changed, new or missing file withdraws it.
    fn read_at_yes(&mut self, id: &ProposalId, read: &Read) -> Result<Option<String>, TurnOutcome> {
        let root = self.snapshot.root.clone();
        let moved: Vec<String> = (read.files.iter())
            .filter(|(path, held)| file_witness(&root, path) != *held)
            .map(|(path, _)| path.clone())
            .collect();
        if !moved.is_empty() {
            let why = format!(
                "the files this proposal reads changed since it was shown: {}",
                quoted(&moved)
            );
            return Err(self.withdraw(id, &why));
        }
        let (held, unread): (Vec<_>, Vec<_>) =
            (read.files.iter()).partition(|(_, at)| at.is_some());
        let paths = |files: Vec<&(String, Option<Witness>)>| -> Vec<String> {
            files.into_iter().map(|(path, _)| path.clone()).collect()
        };
        let (held, unread) = (paths(held), paths(unread));
        let mut said = Vec::new();
        if !held.is_empty() {
            said.push(format!(
                "sources judged again before writing: the exact bytes it was shown over still hold for {}",
                quoted(&held)
            ));
        }
        if !unread.is_empty() {
            said.push(format!(
                "source freshness not judged: {} (absent, or not readable under the project)",
                quoted(&unread)
            ));
        }
        if !read.patterns.is_empty() {
            let patterns = quoted(&read.patterns);
            said.push(format!(
                "source freshness not judged: {patterns} (a pattern)"
            ));
        }
        if said.is_empty() {
            said.push("source freshness not judged: it reads no project file".to_owned());
        }
        Ok(Some(said.join(" · ")))
    }

    /// The compiler's judgement of the request's recorded sources, observed again now.
    fn judge(&self, decision: Option<&Value>, request: &CompileRequest) -> Basis {
        let fresh = self.observe_sources(&Basis::sources(decision));
        basis_for(request, decision, fresh.as_ref())
    }

    /// Each source observed again, alone, by the host's one bounded observer under the root.
    fn observe_sources(&self, sources: &[String]) -> Option<Value> {
        let (mut rows, mut kinds) = (Vec::new(), Map::new());
        for source in sources {
            let stated = if source.chars().any(char::is_whitespace) {
                format!("\"{source}\"")
            } else {
                source.clone()
            };
            let Some(world) = nika_cli_host::compile::observe::world(&self.snapshot.root, &stated)
            else {
                continue;
            };
            rows.extend(world["observed"].as_array().cloned().unwrap_or_default());
            kinds.extend(world["kinds"].as_object().cloned().unwrap_or_default());
        }
        (!rows.is_empty()).then(|| json!({"observed": rows, "kinds": kinds}))
    }

    /// A proposal no compile bound: its request compiled again without any provider, kept (with
    /// that request) only when it gives exactly the proposal's bytes.
    fn derive(&self, set: &ProjectChangeSet) -> Option<(CompileOutcome, CompileRequest)> {
        let request = CompileRequest::create(&set.goal);
        let out = compile_in(&DETERMINISTIC, &self.project_context(), &request, &set.goal).ok()?;
        let Reading::Ready(out) = Reading::of(out) else {
            return None;
        };
        let again = crate::review::propose(&self.snapshot.root, &set.goal, &out).ok()?;
        (witnesses(&again) == witnesses(set)).then_some((out, request))
    }

    /// Withdraw the proposal a yes answered, before any write, consent or money effect: why, in
    /// the conversation's record, and the way on.
    pub(super) fn withdraw(&mut self, id: &ProposalId, why: &str) -> TurnOutcome {
        let text = format!(
            "{why} · the proposal {id} was withdrawn: nothing was written, no consent or money was recorded · say the request again to build it over the project as it is now"
        );
        self.remember("(consent)", &text);
        TurnOutcome::Refusal(Refusal::new(RefusalClass::StaleRevision, text))
    }
}

/// Paths as a conversation names them.
fn quoted(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| format!("`{p}`"))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod fresh_tests;
