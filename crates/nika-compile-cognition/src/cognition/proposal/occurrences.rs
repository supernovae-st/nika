// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The occurrences a private plan cannot keep apart. `Plan::push_step` merges operations of one
//! kind into one step: two independent source → destination branches (two stated sources read by
//! distinct steps, two stated destinations written) would reach the assembler as one read. Such a
//! proposal is classified BEFORE that merge, while its occurrences and evidence still exist, and
//! only after it passed every check the merge applies (anchoring, policy, constraints): a proposal
//! the merge refuses stays refused. Typed rule sequences (filter, count, top-N) and several reads
//! feeding one destination keep the plan they have today.

use super::{Proposal, merge};
use crate::plan::{Op, Plan};
use crate::{CompileOutcome, lexicon::Reading};

/// The operations the plan would fold together: each occurrence's kind and its exact evidence.
pub(in crate::cognition) struct Composition {
    pub(in crate::cognition) occurrences: Vec<(Op, String)>,
}

/// What a proposal becomes at the merge.
pub(in crate::cognition) enum Merged {
    /// A plan the deterministic assembler represents.
    Plan(Plan),
    /// Refused by the merge's own laws (its findings are recorded).
    Refused,
    /// Lawful, but composed of branches the plan cannot keep apart: the sketch door's work.
    NeedsSketch(Composition),
}

impl Merged {
    /// The plan, when the proposal is one the plan represents.
    pub(in crate::cognition) fn into_plan(self) -> Option<Plan> {
        match self {
            Self::Plan(plan) => Some(plan),
            Self::Refused | Self::NeedsSketch(_) => None,
        }
    }
}

/// The proposal merged into the reading as before, or its composition when the plan would fold
/// two independent branches into one.
pub(in crate::cognition) fn merged(
    intent: &str,
    proposal: Proposal,
    reading: &Reading,
    out: &mut CompileOutcome,
) -> Merged {
    let composition = composition(intent, &proposal);
    match (merge(intent, proposal, reading, out), composition) {
        (None, _) => Merged::Refused,
        (Some(_), Some(composition)) => Merged::NeedsSketch(composition),
        (Some(plan), None) => Merged::Plan(plan),
    }
}

/// Two or more independent branches: source steps (`read`, `fetch`) each naming a distinct stated
/// path no write targets, write targets naming distinct stated paths, and a pairing the proposal's
/// own cited texts evidence (a source step's detail and evidence, or a write's target and
/// evidence, naming exactly one read source and exactly one written path) that matches every
/// read source to exactly one written path and back. Path counts alone are not branches: two
/// sources merged into one result written twice carry no such pairing and keep their plan (whose
/// own merge and judgment are unchanged; an unpaired plan is not thereby claimed sound). The
/// stated paths are the request's own (`hot::stated_sources` and `stated_destinations` together:
/// a connector heuristic may file a source among destinations); the steps and effects say which
/// is read and which is written.
fn composition(intent: &str, proposal: &Proposal) -> Option<Composition> {
    let mut stated = crate::hot::stated_sources(intent);
    stated.extend(crate::hot::stated_destinations(intent));
    let named = |text: &str| -> Vec<String> {
        stated
            .iter()
            .filter(|path| text.contains(path.as_str()))
            .cloned()
            .collect()
    };
    let writes: Vec<_> = proposal
        .effects
        .iter()
        .filter(|e| e.verb == "write")
        .collect();
    // A written path is a write's target; its evidence only claims a pairing (a natural clause
    // cites the source it copies, which must stay a read source).
    let mut written: Vec<String> = Vec::new();
    for path in writes.iter().flat_map(|e| named(&e.target)) {
        if !written.contains(&path) {
            written.push(path);
        }
    }
    let writes: Vec<String> = writes
        .iter()
        .map(|e| format!("{} {}", e.target, e.evidence))
        .collect();
    let mut read: Vec<String> = Vec::new();
    let mut cited: Vec<String> = writes.clone();
    let mut occurrences: Vec<(Op, String)> = Vec::new();
    for step in &proposal.steps {
        let Some(op) = Op::parse(&step.op).filter(|op| matches!(op, Op::Read | Op::Fetch)) else {
            continue;
        };
        let text = format!("{} {}", step.detail, step.evidence);
        let sources: Vec<String> = named(&text)
            .into_iter()
            .filter(|path| !written.contains(path))
            .collect();
        if let [source] = sources.as_slice()
            && !read.contains(source)
        {
            read.push(source.clone());
            occurrences.push((op, step.evidence.clone()));
        }
        cited.push(text);
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for text in &cited {
        let paths = named(text);
        let source: Vec<&String> = paths.iter().filter(|p| read.contains(p)).collect();
        let target: Vec<&String> = paths.iter().filter(|p| written.contains(p)).collect();
        if let ([source], [target]) = (source.as_slice(), target.as_slice()) {
            let pair = ((*source).clone(), (*target).clone());
            if !pairs.contains(&pair) {
                pairs.push(pair);
            }
        }
    }
    let matched = |paths: &[String], side: fn(&(String, String)) -> &String| {
        paths
            .iter()
            .all(|path| pairs.iter().filter(|pair| side(pair) == path).count() == 1)
    };
    let independent = read.len() >= 2
        && written.len() >= 2
        && matched(&read, |pair| &pair.0)
        && matched(&written, |pair| &pair.1);
    independent.then_some(Composition { occurrences })
}
