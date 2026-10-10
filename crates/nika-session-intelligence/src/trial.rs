// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A conversation's candidate, tried before it is proposed. Each public GET source the candidate
//! names is observed once through the host's observer (by default the guarded one,
//! [`crate::observe::capture`]: no credentials, no private address, bounded) and kept for the
//! conversation. The observed room is lent those captures as a replay trial
//! (`nika_service_execution::replay`): a GET of exactly a captured address is answered with it,
//! nothing leaves the room, and a step the trial cannot exercise (a model step, a request other
//! than GET, an effect outside the room) is named, never run as itself. The trial is accounted
//! as a native door's rehearsal ([`native::Scoped`]), so its proof binds the exact bytes and
//! the files it ran on; [`Trial::words`] say what it ran on and what it did not run.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nika_onboard::compile::copy::{Allowance, Limits, Usage, native};
use nika_onboard::compile::room::{JqHelper, ObservedRoom};
pub use nika_service_execution::replay::Capture;
use nika_service_execution::replay::{self, Captures};

/// How a host observes one public page for a trial: the guarded GET, or a host's own pages.
pub type Observer = Arc<dyn Fn(&str) -> Result<Capture, String> + Send + Sync>;

/// The pages a conversation captured, by the address its candidate names.
pub type Observed = BTreeMap<String, Capture>;

const MIB: u64 = 1024 * 1024;

/// One trial: its three attempts at most, within the bytes and the run time its room may spend.
const TRIAL: Limits = Limits::new(3, 3, 6 * MIB, 30_000);

/// The pages one conversation holds for its trials, at most.
const HELD_PAGES: usize = 64;

/// How long one trial keeps observing new pages, in all.
const OBSERVING: Duration = Duration::from_secs(45);

/// One candidate's trial, ready to lend a verification: the room it runs in, accounted, and the
/// words a person reads of it.
#[non_exhaustive]
pub struct Trial {
    /// The observed room, lent the captures, as the verification's rehearsal host.
    pub scoped: native::Scoped,
    /// What the trial ran on and what it did not run, one line each.
    pub words: String,
}

/// The guarded observer: [`crate::observe::capture`].
#[must_use]
pub fn guarded() -> Observer {
    Arc::new(crate::observe::capture)
}

/// The trial of `candidate`, stated for `stated`, in the observed room of the project at
/// `root` (its `nika:jq` steps run by `jq`): each source `held` does not hold yet observed by
/// `observe` and kept there, the room lent every source `held` holds for the candidate.
#[must_use]
pub fn prepare(
    candidate: &str,
    stated: &str,
    (root, jq): (PathBuf, Option<JqHelper>),
    held: &mut Observed,
    observe: &Observer,
) -> Trial {
    let workflow = replay::parsed(candidate);
    let sources = workflow.as_ref().map(replay::sources).unwrap_or_default();
    let mut captures = Captures::new();
    let mut unobserved = Vec::new();
    let clock = Instant::now();
    for url in sources {
        let bound = if held.len() >= HELD_PAGES {
            Some(format!(
                "this conversation holds {HELD_PAGES} pages already"
            ))
        } else {
            (clock.elapsed() > OBSERVING).then(|| "the trial's observing time ran out".to_owned())
        };
        if let (Some(why), false) = (&bound, held.contains_key(&url)) {
            unobserved.push(format!("{url} ({why})"));
            continue;
        }
        if !held.contains_key(&url) {
            match observe(&url) {
                Ok(page) => {
                    held.insert(url.clone(), page);
                }
                Err(why) => unobserved.push(format!("{url} ({why})")),
            }
        }
        if let Some(page) = held.get(&url)
            && let Err(refused) = captures.insert(page.clone())
        {
            unobserved.push(format!("{url} ({refused})"));
        }
    }
    let screen = workflow
        .as_ref()
        .map(|workflow| replay::screen(workflow, &captures));
    let words = words(&captures, screen.as_ref(), &unobserved);
    let project = root.clone();
    let mut trial_room = ObservedRoom::new(&root).located(move |candidate| {
        let landing = nika_session_change::review::destination(&project, candidate)?;
        Some(landing.display().to_string())
    });
    if let Some(helper) = jq {
        trial_room = trial_room.with_jq_helper(helper);
    }
    let trial_room = trial_room.with_captures(captures);
    let allowance = Allowance::new(TRIAL, TRIAL, Usage::default());
    let scoped = native::Scoped::new(root, Box::new(trial_room), allowance).stating(stated);
    Trial { scoped, words }
}

/// What a person reads of a trial over `captures`: the pages it ran on and when they were
/// observed, the steps it did not run, and the sources it could not observe.
fn words(
    captures: &Captures,
    screen: Option<&replay::ReplayScreen>,
    unobserved: &[String],
) -> String {
    let mut lines = String::new();
    let pages: Vec<String> = (captures.iter())
        .map(|page| format!("{} ({} bytes)", page.url(), page.body().len()))
        .collect();
    let at = captures.iter().map(Capture::captured_at_ms).min();
    match at {
        Some(at) => {
            let _ = writeln!(
                lines,
                "tried on the pages observed at {}: {}",
                clock(at),
                pages.join(", ")
            );
        }
        None => lines.push_str("tried on no page observed from the network\n"),
    }
    let mut kept_out: Vec<String> = (screen.into_iter())
        .flat_map(|screen| screen.unexercised.iter())
        .map(|(task, need)| format!("{need} ({task})"))
        .collect();
    kept_out.push("any effect outside the room".to_owned());
    let _ = writeln!(lines, "not run: {}", kept_out.join(" · "));
    if !unobserved.is_empty() {
        let _ = writeln!(lines, "not observed: {}", unobserved.join(", "));
    }
    lines
}

/// `at` (milliseconds since the Unix epoch) as a time of day in UTC.
fn clock(at: u64) -> String {
    let seconds = at / 1000;
    format!(
        "{:02}:{:02} UTC",
        (seconds / 3600) % 24,
        (seconds / 60) % 60
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
