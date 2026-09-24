//! The machine's typed answers (ADR-133 · the portable session): what a
//! host renders, and what a REMOTE host judges by identity — the proposal
//! a consent names, the gate or the question an answer names, the class of
//! a refusal. The terminal door reads the same types; no host parses prose.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

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

/// The identity of one authoring question as ONE session asked it: the
/// witness of the question, of the request revision it belongs to and of
/// the intelligence that reads its answer, held with the session that
/// asked it. Only a session hands one out
/// (`SessionRuntime::pending_question_id`); an answer that names it answers
/// that question in that session, or nothing. It grants no consent and no
/// run, and nothing keeps it: the same question asked again — a new
/// revision, another intelligence, a restarted session — is another
/// identity. The text names the question and its revision, never the
/// session: two sessions that ask the same question share the text, not
/// the identity. The key the question fills stays the compiler's.
#[derive(Clone)]
#[non_exhaustive]
pub struct QuestionId {
    witness: String,
    asker: Weak<Incarnation>,
}

/// One session, told apart from every other by identity alone: no clock,
/// no randomness, no counter shared between sessions.
#[derive(Debug, Default)]
pub(crate) struct Incarnation;

impl QuestionId {
    /// The identity of `witness` as `asker` asked it.
    pub(crate) fn new(witness: String, asker: &Arc<Incarnation>) -> Self {
        Self {
            witness,
            asker: Arc::downgrade(asker),
        }
    }

    /// The witness, hex: the question, its revision and the intelligence
    /// that reads its answer — not the session that asked it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.witness
    }

    /// Whether `asker` is the session that asked this question.
    pub(crate) fn asked_by(&self, asker: &Arc<Incarnation>) -> bool {
        Weak::ptr_eq(&self.asker, &Arc::downgrade(asker))
    }
}

impl PartialEq for QuestionId {
    fn eq(&self, other: &Self) -> bool {
        self.witness == other.witness && Weak::ptr_eq(&self.asker, &other.asker)
    }
}

impl Eq for QuestionId {}

impl Hash for QuestionId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.witness.hash(state);
    }
}

impl fmt::Debug for QuestionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("QuestionId").field(&self.witness).finish()
    }
}

impl fmt::Display for QuestionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.witness.get(..12).unwrap_or(&self.witness))
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
    /// The file system refused; the sentence names what this call wrote.
    Io,
    /// The compiler refused the authoring request under its own policy,
    /// or its machinery failed; nothing was written or substituted.
    AuthoringRefused,
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
            Self::AuthoringRefused => "authoring_refused",
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

    /// A question's identity is its witness AND the session that asked it:
    /// the same text asked by another session — a restart — is another
    /// identity, whether or not the first session still lives.
    #[test]
    fn a_question_identity_is_its_witness_and_the_session_that_asked_it() {
        let first = Arc::new(Incarnation);
        let second = Arc::new(Incarnation);
        let witness = Witness::of(b"question").0;
        let asked = QuestionId::new(witness.clone(), &first);
        assert_eq!(asked, asked.clone());
        assert_eq!(asked, QuestionId::new(witness.clone(), &first));
        assert_ne!(asked, QuestionId::new(Witness::of(b"other").0, &first));
        let elsewhere = QuestionId::new(witness.clone(), &second);
        assert_eq!(
            elsewhere.as_str(),
            asked.as_str(),
            "the text never names the session"
        );
        assert_ne!(asked, elsewhere, "the identity does");
        assert!(asked.asked_by(&first) && !asked.asked_by(&second));
        assert_eq!(asked.as_str().len(), 64);
        assert_eq!(asked.to_string(), &witness[..12]);
        drop(first);
        let reopened = Arc::new(Incarnation);
        assert!(!asked.asked_by(&reopened), "a later session never asked it");
        assert_ne!(asked, QuestionId::new(witness, &reopened));
    }

    #[test]
    fn a_change_error_carries_its_class() {
        let stale = Refusal::from_change(&ChangeError::Stale("a.nika".to_owned()));
        assert_eq!(stale.class, RefusalClass::StaleRevision);
        assert!(stale.to_string().contains("changed since this preview"));
        let out = Refusal::from_change(&ChangeError::OutsideRoot("../x".to_owned()));
        assert_eq!(out.class, RefusalClass::NotAllowed);
        assert_eq!(RefusalClass::AlreadyConsumed.as_str(), "already_consumed");
    }
}
