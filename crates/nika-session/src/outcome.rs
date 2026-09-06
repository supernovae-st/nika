//! The machine's typed answers (ADR-133 · the portable session): what a
//! host renders, and what a REMOTE host judges by identity — the proposal
//! a consent names, the gate an answer names, the class of a refusal. The
//! terminal door reads the same types; no host parses prose.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::change::{ChangeError, Witness};

/// The identity of one proposal: the witness of the exact preview the
/// human saw. A consent names it; a consent naming another proposal is
/// stale and applies nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProposalId(String);

impl ProposalId {
    /// The id of a preview's exact bytes.
    #[must_use]
    pub fn of(preview: &str) -> Self {
        Self(Witness::of(preview.as_bytes()).0)
    }

    /// The digest, hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProposalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.get(..12).unwrap_or(&self.0))
    }
}

/// The identity of one paused gate: the trace that paused and the task
/// that asked. An answer names it; the same gate answers once.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GateId {
    /// The paused trace (the resume handle).
    pub trace: PathBuf,
    /// The gate's task id.
    pub task: String,
}

impl GateId {
    /// Construct.
    #[must_use]
    pub fn new(trace: &Path, task: &str) -> Self {
        Self {
            trace: trace.to_path_buf(),
            task: task.to_owned(),
        }
    }
}

impl fmt::Display for GateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` in {}", self.task, self.trace.display())
    }
}

/// Why a turn was refused — a class a host acts on, never a string it
/// parses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RefusalClass {
    /// No conversational intelligence serves this session; the facts
    /// still answer.
    NoIntelligence,
    /// The intelligence, or the choice of one, refused; the previous
    /// choice stands.
    IntelligenceRefused,
    /// The turn reached outside what the session may write.
    NotAllowed,
    /// Nothing is in the state this turn needs: no proposal pending, no
    /// gate waiting, no census to re-choose from.
    WrongState,
    /// The turn named a proposal or a gate that is not the one waiting,
    /// or the file changed since the preview.
    StaleRevision,
    /// The named proposal or gate was already decided: its effect
    /// happened once and will not happen again.
    AlreadyConsumed,
    /// A gate needs an answer; an empty line is none.
    EmptyAnswer,
    /// The file system refused; nothing else was written.
    Io,
}

impl RefusalClass {
    /// The machine word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoIntelligence => "no_intelligence",
            Self::IntelligenceRefused => "intelligence_refused",
            Self::NotAllowed => "not_allowed",
            Self::WrongState => "wrong_state",
            Self::StaleRevision => "stale_revision",
            Self::AlreadyConsumed => "already_consumed",
            Self::EmptyAnswer => "empty_answer",
            Self::Io => "io",
        }
    }
}

/// A refusal: its class, and the sentence that names the fix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The class a host acts on.
    pub class: RefusalClass,
    /// The reason and the fix, for a human.
    pub text: String,
}

impl Refusal {
    /// Construct.
    #[must_use]
    pub fn new(class: RefusalClass, text: impl Into<String>) -> Self {
        Self {
            class,
            text: text.into(),
        }
    }

    /// The class of what a change set could not become.
    #[must_use]
    pub fn from_change(e: &ChangeError) -> Self {
        let class = match e {
            ChangeError::Stale(_) => RefusalClass::StaleRevision,
            ChangeError::OutsideRoot(_) | ChangeError::Unnamed(_) => RefusalClass::NotAllowed,
            ChangeError::Io(..) => RefusalClass::Io,
        };
        Self::new(class, e.to_string())
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_the_bytes_and_the_place() {
        assert_eq!(ProposalId::of("a"), ProposalId::of("a"));
        assert_ne!(ProposalId::of("a"), ProposalId::of("b"));
        assert_eq!(ProposalId::of("a").as_str().len(), 64);
        let gate = GateId::new(Path::new("/t/x.ndjson"), "gate");
        assert_eq!(gate, GateId::new(Path::new("/t/x.ndjson"), "gate"));
        assert_ne!(gate, GateId::new(Path::new("/t/y.ndjson"), "gate"));
        assert!(gate.to_string().contains("`gate`"));
    }

    #[test]
    fn a_change_error_carries_its_class() {
        let stale = Refusal::from_change(&ChangeError::Stale("a.nika.yaml".to_owned()));
        assert_eq!(stale.class, RefusalClass::StaleRevision);
        assert!(stale.to_string().contains("changed since this preview"));
        let out = Refusal::from_change(&ChangeError::OutsideRoot("../x".to_owned()));
        assert_eq!(out.class, RefusalClass::NotAllowed);
        assert_eq!(RefusalClass::AlreadyConsumed.as_str(), "already_consumed");
    }
}
