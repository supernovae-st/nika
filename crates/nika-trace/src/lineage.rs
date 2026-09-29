// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The lineage view: which journals of ONE trace directory continued a
//! paused run (#1462 · the `resumed_from` link). A read-only view derived
//! from [`nika_dap::store::survey`] — reading, recovery and identity facts
//! stay in `nika-dap`; this module only folds them, like [`crate::run_view`].
//!
//! Never an authority. [`Lineage::NoneObserved`] says what one read of one
//! directory held, never that the run was not continued: a continuation
//! before its first frame, one started after the survey, one retention or
//! `trace rm` removed, a copied or tampered journal and an unverified
//! resume are out of its sight. It verifies no chain, seal or signature,
//! admits nothing, and claims neither atomicity nor exactly-once: the
//! engine's resume admission (the approval ticket's single-use claim)
//! decides a race, within its own scope. Whatever the survey cannot vouch
//! for makes the verdict [`Lineage::Indeterminate`] with every reason;
//! continuations of one run are never ranked, and lists keep the survey's
//! directory order (nothing is chosen from them).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use nika_dap::liveness::Liveness;
use nika_dap::store::{DoubtWhy, Skipped, Survey, TraceMeta, TraceState, survey};

/// The longest chain folded before [`Undecided::TooLong`].
pub const MAX_LINKS: usize = 64;

/// The lineage of one paused journal within one survey.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Lineage {
    /// A complete survey in which no journal names the paused run as the
    /// one it continued — what one read held, never an authorization.
    NoneObserved,
    /// One linear chain of continuations, every link folded and agreeing.
    Chain {
        /// The continuations, from the paused journal's to the head.
        links: Vec<PathBuf>,
        /// Where the chain stands.
        head: Head,
    },
    /// The survey cannot decide: every reason found, none resolved by choice.
    Indeterminate(Vec<Undecided>),
}

/// Where a linear chain of continuations stands: its last link.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Head {
    /// It settled: succeeded, failed or cancelled.
    Settled(TraceState),
    /// It paused again, at this journal's own gate.
    Paused {
        /// The journal that paused.
        trace: PathBuf,
        /// The awaiting task, when the pause names it.
        task: Option<String>,
    },
    /// No terminal event: in flight, or stopped without settling (ADR-129).
    Running(Option<Liveness>),
}

/// Why a survey cannot decide a lineage.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Undecided {
    /// The directory or some entries could not be read: any of them may be
    /// a continuation.
    Unsurveyed {
        /// The directory's read errors.
        dir_errors: Vec<ErrorKind>,
        /// The entries that could not be folded.
        skipped: Vec<Skipped>,
    },
    /// The paused journal is not among the folded journals, or names no run.
    PausedUnidentified,
    /// The paused journal's own last terminal is not a pause.
    NotPaused {
        /// Its state.
        state: TraceState,
    },
    /// A journal this lineage depends on may be incomplete (a torn suffix),
    /// or any journal's link is unknown or ambiguous.
    Doubtful {
        /// The journal.
        path: PathBuf,
        /// The survey's doubt.
        why: DoubtWhy,
    },
    /// Several journals carry one identity of this lineage.
    Duplicate {
        /// The shared identity.
        run_id: String,
        /// Every journal carrying it.
        paths: Vec<PathBuf>,
    },
    /// Several journals continued one run: siblings are never ranked.
    Fork {
        /// The run they all continued.
        run_id: String,
        /// Every continuation with its state.
        successors: Vec<(PathBuf, TraceState)>,
    },
    /// A continuation names a journal already on the chain.
    Cycle {
        /// The identity met twice.
        run_id: String,
    },
    /// A continuation records another workflow, project or no run identity.
    Disagreement {
        /// The continuation's journal.
        path: PathBuf,
        /// `workflow` · `project` · `run_id`.
        field: &'static str,
    },
    /// The chain is longer than [`MAX_LINKS`].
    TooLong,
}

/// Where a paused run stands for a host that would offer its gate (C7): what
/// one read of one directory holds, named by journal file — never an
/// authorization, never atomic, never exactly-once. The host says it in its
/// own words and keeps its own way on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Standing {
    /// No journal continued it: its own pause stands.
    Stands,
    /// A single chain of continuations paused again, at this journal.
    PausedAgain(PathBuf),
    /// A continuation settled: its state and its journal's file name.
    Settled {
        /// How it ended.
        state: TraceState,
        /// The file name of the journal that settled it.
        trace: String,
    },
    /// A continuation has not settled: its liveness, when the lease names it.
    Running(Option<Liveness>),
    /// The journals cannot decide: every reason, in words.
    Undecided(Vec<String>),
}

impl Lineage {
    /// Where the paused run of `paused` stands. `own_doubt_stands` is for a
    /// host that observed this pause itself: when the only doubt is that
    /// pause's own journal (it names no run, it cannot be folded, or no trace
    /// store exists), no continuation can be followed and the pause stands.
    #[must_use]
    pub fn standing(&self, paused: &Path, own_doubt_stands: bool) -> Standing {
        match self {
            Self::NoneObserved => Standing::Stands,
            Self::Indeterminate(reasons)
                if own_doubt_stands && reasons.iter().all(|r| r.concerns_only(paused)) =>
            {
                Standing::Stands
            }
            Self::Indeterminate(reasons) => {
                Standing::Undecided(reasons.iter().map(ToString::to_string).collect())
            }
            Self::Chain { links, head } => match head {
                Head::Paused { trace, .. } => Standing::PausedAgain(trace.clone()),
                Head::Running(liveness) => Standing::Running(*liveness),
                Head::Settled(state) => Standing::Settled {
                    state: *state,
                    trace: links.last().map(|l| file_name(l)).unwrap_or_default(),
                },
            },
        }
    }
}

impl Undecided {
    /// Whether this doubt concerns only the paused journal itself: it names
    /// no run, it is the one entry that could not be folded, or no trace
    /// store exists at all.
    fn concerns_only(&self, paused: &Path) -> bool {
        match self {
            Self::PausedUnidentified => true,
            Self::Unsurveyed {
                dir_errors,
                skipped,
            } => {
                dir_errors.iter().all(|e| *e == ErrorKind::NotFound)
                    && skipped.iter().all(|s| s.path == paused)
            }
            _ => false,
        }
    }
}

impl std::fmt::Display for Undecided {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsurveyed { .. } => f.write_str("some journals could not be read"),
            Self::PausedUnidentified => f.write_str("the paused journal is not identified there"),
            Self::NotPaused { state } => {
                write!(f, "the paused journal now ends {}", state.as_str())
            }
            Self::Doubtful { path, .. } => {
                write!(f, "journal `{}` may be incomplete", file_name(path))
            }
            Self::Duplicate { .. } => f.write_str("several journals carry one run identity"),
            Self::Fork { successors, .. } => {
                write!(f, "{} continuations of one run", successors.len())
            }
            Self::Cycle { .. } => {
                f.write_str("a continuation names a journal already on the chain")
            }
            Self::Disagreement { path, field } => {
                write!(f, "journal `{}` records another {field}", file_name(path))
            }
            Self::TooLong => f.write_str("the chain of continuations is too long"),
        }
    }
}

/// A journal's file name, as a host names it.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Survey `dir` once and fold the lineage of `paused`, a journal inside it
/// named as the survey names it (`dir` joined with its file name); any
/// other spelling reads [`Undecided::PausedUnidentified`].
#[must_use]
pub fn lineage_of(dir: &Path, paused: &Path) -> Lineage {
    fold(&survey(dir), paused)
}

/// Fold the lineage of `paused` from a survey. Pure: no I/O, no clock, no
/// ranking; every reason accumulates.
#[must_use]
pub fn fold(survey: &Survey, paused: &Path) -> Lineage {
    let mut undecided = Vec::new();
    if !survey.complete() {
        undecided.push(Undecided::Unsurveyed {
            dir_errors: survey.dir_errors.clone(),
            skipped: survey.skipped.clone(),
        });
    }
    // A journal whose link is unknown or ambiguous may continue anything.
    let blocking = |why: &DoubtWhy| {
        matches!(
            why,
            DoubtWhy::NoOpeningFrame | DoubtWhy::ConflictingIdentity
        )
    };
    doubts_where(survey, &mut undecided, |d| blocking(&d.why));
    let start = survey.traces.iter().find(|t| t.path == paused);
    let Some((start, id)) = start.and_then(|t| Some((t, t.run_id.clone()?))) else {
        undecided.push(Undecided::PausedUnidentified);
        return Lineage::Indeterminate(undecided);
    };
    if start.state != TraceState::Paused {
        undecided.push(Undecided::NotPaused { state: start.state });
    }
    let walk = walk(survey, start, id);
    undecided.extend(walk.undecided);
    // A torn suffix matters on the journals this lineage depends on: the
    // paused one, every link and every sibling.
    doubts_where(survey, &mut undecided, |d| {
        matches!(d.why, DoubtWhy::TornSuffix(_)) && walk.relevant.contains(&d.path)
    });
    match (undecided.is_empty(), walk.links.is_empty()) {
        (false, _) => Lineage::Indeterminate(undecided),
        (true, true) => Lineage::NoneObserved,
        (true, false) => Lineage::Chain {
            links: walk.links,
            head: head_of(walk.last),
        },
    }
}

/// The survey's doubts that `keep`, as reasons.
fn doubts_where(
    survey: &Survey,
    undecided: &mut Vec<Undecided>,
    keep: impl Fn(&nika_dap::store::Doubt) -> bool,
) {
    let doubtful = survey.doubts.iter().filter(|d| keep(d));
    undecided.extend(doubtful.map(|d| Undecided::Doubtful {
        path: d.path.clone(),
        why: d.why.clone(),
    }));
}

/// One walk from the paused journal along single continuations.
struct Walk<'a> {
    links: Vec<PathBuf>,
    last: &'a TraceMeta,
    relevant: Vec<PathBuf>,
    undecided: Vec<Undecided>,
}

/// Follow single continuations from `start`; every doubt lands in the walk.
fn walk<'a>(survey: &'a Survey, start: &'a TraceMeta, mut id: String) -> Walk<'a> {
    let mut walk = Walk {
        links: Vec::new(),
        last: start,
        relevant: vec![start.path.clone()],
        undecided: duplicate(survey, &id).into_iter().collect(),
    };
    let mut seen = vec![id.clone()];
    loop {
        let next: Vec<&TraceMeta> = survey
            .traces
            .iter()
            .filter(|t| t.resumed_from.as_deref() == Some(id.as_str()))
            .collect();
        walk.relevant.extend(next.iter().map(|t| t.path.clone()));
        let [one] = next.as_slice() else {
            if next.len() > 1 {
                let successors = next.iter().map(|t| (t.path.clone(), t.state)).collect();
                walk.undecided.push(Undecided::Fork {
                    run_id: id,
                    successors,
                });
            }
            break;
        };
        for (field, same) in [
            ("workflow", one.workflow == start.workflow),
            ("project", one.project == start.project),
        ] {
            if !same {
                let path = one.path.clone();
                walk.undecided.push(Undecided::Disagreement { path, field });
            }
        }
        let Some(next_id) = one.run_id.clone() else {
            let path = one.path.clone();
            walk.undecided.push(Undecided::Disagreement {
                path,
                field: "run_id",
            });
            break;
        };
        if seen.contains(&next_id) {
            walk.undecided.push(Undecided::Cycle { run_id: next_id });
            break;
        }
        if walk.links.len() == MAX_LINKS {
            walk.undecided.push(Undecided::TooLong);
            break;
        }
        walk.undecided.extend(duplicate(survey, &next_id));
        seen.push(next_id.clone());
        walk.links.push(one.path.clone());
        (walk.last, id) = (one, next_id);
    }
    walk
}

/// Several journals carrying `id`, when they do.
fn duplicate(survey: &Survey, id: &str) -> Option<Undecided> {
    let paths: Vec<PathBuf> = survey
        .traces
        .iter()
        .filter(|t| t.run_id.as_deref() == Some(id))
        .map(|t| t.path.clone())
        .collect();
    (paths.len() > 1).then(|| Undecided::Duplicate {
        run_id: id.to_owned(),
        paths,
    })
}

/// Where the last link stands.
fn head_of(last: &TraceMeta) -> Head {
    match last.state {
        TraceState::Paused => Head::Paused {
            trace: last.path.clone(),
            task: last.paused_task.clone(),
        },
        TraceState::Running => Head::Running(last.liveness),
        settled => Head::Settled(settled),
    }
}
