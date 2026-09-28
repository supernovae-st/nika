// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The source basis of a proposal, bound where a compile outcome is proposed and judged again at
//! its yes (C9 · F4). The compiler records the source facts a candidate's program relies on
//! ([`Basis::sources`]); a yes observes exactly those sources again through the host's one
//! bounded observer and the compiler judges them ([`basis`]). A moved or unjudgeable basis
//! withdraws the proposal before any write, consent record or money effect: its bytes stay in the
//! conversation as evidence, the goal stays, and saying the request again builds it over the
//! project as it is. Rows added, removed or reordered never move it. A proposal no compile bound
//! (a kept draft proposed again) takes its basis from a zero-call deterministic compile of its
//! request only when that compile gives its exact bytes; otherwise a workflow that reads project
//! files is withdrawn with the request to say it again. What is not judged is said, never
//! presented as fresh.

use nika_onboard::compile::{Basis, CompileOutcome, CompileRequest, basis};
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
    /// The request the compiler read, and its decision record (`None`: it recorded none).
    intent: String,
    decision: Option<Value>,
}

/// The witnesses of a set's exact bytes, in set order.
fn witnesses(set: &ProjectChangeSet) -> Vec<Witness> {
    set.changes
        .iter()
        .map(|change| Witness::of(change.content().as_bytes()))
        .collect()
}

impl SessionRuntime {
    /// Bind what `out` records of its sources to the proposal `id` made of `set`, before any yes.
    pub(super) fn bind_basis(
        &mut self,
        id: &ProposalId,
        set: &ProjectChangeSet,
        intent: &str,
        out: &CompileOutcome,
    ) {
        self.basis = Some(ProposalBasis {
            id: id.clone(),
            bytes: witnesses(set),
            intent: intent.to_owned(),
            decision: out.provenance.decision.clone(),
        });
    }

    /// At a yes, before anything lands: what the landed report says of the sources, or the
    /// withdrawal of a proposal whose sources moved or cannot be judged.
    pub(super) fn basis_at_yes(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
    ) -> Result<Option<String>, TurnOutcome> {
        let bound = self
            .basis
            .take()
            .filter(|b| b.id == *id || b.bytes == witnesses(set));
        let reads = set.project_reads();
        let (decision, intent, derived) = match bound {
            Some(b) => (b.decision, b.intent, false),
            None => match self.derive(set) {
                Some(out) => (out.provenance.decision, set.goal.clone(), true),
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
        match self.judge(decision.as_ref(), &intent) {
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

    /// The compiler's judgement of a decision's recorded sources, observed again now.
    fn judge(&self, decision: Option<&Value>, intent: &str) -> Basis {
        let fresh = self.observe_sources(&Basis::sources(decision));
        basis(decision, fresh.as_ref(), intent)
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

    /// A proposal no compile bound: its request compiled again without any provider, kept only
    /// when it gives exactly the proposal's bytes.
    fn derive(&self, set: &ProjectChangeSet) -> Option<CompileOutcome> {
        let request = CompileRequest::create(&set.goal);
        let out = compile_in(&DETERMINISTIC, &self.project_context(), &request, &set.goal).ok()?;
        let Reading::Ready(out) = Reading::of(out) else {
            return None;
        };
        let again = crate::review::propose(&self.snapshot.root, &set.goal, &out).ok()?;
        (witnesses(&again) == witnesses(set)).then_some(out)
    }

    /// Withdraw the proposal a yes answered, before any write, consent or money effect: why, in
    /// the conversation's record, and the way on.
    fn withdraw(&mut self, id: &ProposalId, why: &str) -> TurnOutcome {
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
