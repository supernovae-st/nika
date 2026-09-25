// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where the automation stands, as separate facts: DRAFT · SAVED ·
//! CHECKED · ACTIVE · RUN, each at its own stage. Declared is never
//! active, saved is never run, a check with findings is not a clean one.
//! The rail is compiled from the session's own facts (the proposal that
//! waits, the workflow saved at consent and its check, the schedule
//! declared in `nika.yaml`, the gate, the last run) and shown above the
//! status row at every turn; it never folds the five into one badge.

/// The stage of one rail field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stage {
    /// Not reached (○).
    Pending,
    /// Under way (●): a question or a value waits before the proposal.
    Working,
    /// Done and observed (✓).
    Done,
    /// Declared in the file, not proven by a firer (◐).
    Declared,
    /// Paused (⏸): a gate waits, or the declaration is suspended.
    Paused,
    /// Failed (×).
    Failed,
    /// Needs attention (!): findings, a refusal.
    Attention,
}

impl Stage {
    /// The glyph the rail shows.
    #[must_use]
    pub const fn glyph(self) -> char {
        match self {
            Self::Pending => '○',
            Self::Working => '●',
            Self::Done => '✓',
            Self::Declared => '◐',
            Self::Paused => '⏸',
            Self::Failed => '×',
            Self::Attention => '!',
        }
    }
}

/// The last run as the session holds it, or the gate that waits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunFact {
    /// Nothing has run.
    #[default]
    Nothing,
    /// The last run ended with this exit code.
    Exit(u8),
    /// A gate waits for the human's answer.
    GateWaits,
}

/// The facts the rail is compiled from, as the session holds them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LifecycleFacts {
    /// A proposal waits for consent.
    pub proposal_waits: bool,
    /// A question, an input value or an activation value waits.
    pub composing: bool,
    /// A workflow was saved at consent.
    pub saved: bool,
    /// The check at that consent: clean, or findings (`None`: nothing saved).
    pub check_clean: Option<bool>,
    /// A schedule is declared in `nika.yaml` for the saved workflow: active,
    /// or suspended (`None`: none declared).
    pub declared_active: Option<bool>,
    /// The last run, or the gate that waits for the human.
    pub run: RunFact,
}

impl LifecycleFacts {
    /// No fact yet: a fresh session.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The rail: five fields, each at its own stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Lifecycle {
    /// The draft: proposed or saved (done), being composed (working).
    pub draft: Stage,
    /// Saved at consent.
    pub saved: Stage,
    /// The check at that consent: clean (done) or findings (attention).
    pub checked: Stage,
    /// The schedule: declared (never « active » from the session) or suspended.
    pub active: Stage,
    /// The last run: done, failed, refused, or paused at a gate.
    pub run: Stage,
}

impl Lifecycle {
    /// Compile the rail from the facts.
    #[must_use]
    pub fn from_facts(facts: &LifecycleFacts) -> Self {
        let draft = if facts.saved || facts.proposal_waits {
            Stage::Done
        } else if facts.composing {
            Stage::Working
        } else {
            Stage::Pending
        };
        let saved = if facts.saved {
            Stage::Done
        } else {
            Stage::Pending
        };
        let checked = match facts.check_clean {
            Some(true) => Stage::Done,
            Some(false) => Stage::Attention,
            None => Stage::Pending,
        };
        let active = match facts.declared_active {
            Some(true) => Stage::Declared,
            Some(false) => Stage::Paused,
            None => Stage::Pending,
        };
        let run = match facts.run {
            RunFact::GateWaits | RunFact::Exit(4) => Stage::Paused,
            RunFact::Exit(0) => Stage::Done,
            RunFact::Exit(1) => Stage::Failed,
            RunFact::Exit(_) => Stage::Attention,
            RunFact::Nothing => Stage::Pending,
        };
        Self {
            draft,
            saved,
            checked,
            active,
            run,
        }
    }

    /// The rail line: « Draft ✓ · Saved ✓ · Checked ✓ · Active ○ · Run ○ ».
    #[must_use]
    pub fn rail(&self) -> String {
        format!(
            "Draft {} · Saved {} · Checked {} · Active {} · Run {}",
            self.draft.glyph(),
            self.saved.glyph(),
            self.checked.glyph(),
            self.active.glyph(),
            self.run.glyph()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rail(facts: LifecycleFacts) -> String {
        Lifecycle::from_facts(&facts).rail()
    }

    /// A fresh session shows five pending fields; a proposal marks the
    /// draft done while Saved stays pending; a question marks it working.
    #[test]
    fn the_draft_is_done_when_proposed_and_working_while_composed() {
        assert_eq!(
            rail(LifecycleFacts::new()),
            "Draft ○ · Saved ○ · Checked ○ · Active ○ · Run ○"
        );
        let proposed = LifecycleFacts {
            proposal_waits: true,
            ..LifecycleFacts::new()
        };
        assert_eq!(
            rail(proposed),
            "Draft ✓ · Saved ○ · Checked ○ · Active ○ · Run ○"
        );
        let composing = LifecycleFacts {
            composing: true,
            ..LifecycleFacts::new()
        };
        assert!(rail(composing).starts_with("Draft ● · Saved ○"));
    }

    /// Saved is not run and declared is never active: a saved, clean,
    /// declared workflow reads ✓ ✓ ✓ ◐ ○; findings read !; suspended ⏸.
    #[test]
    fn saved_is_not_run_and_declared_is_never_active() {
        let saved = LifecycleFacts {
            saved: true,
            check_clean: Some(true),
            declared_active: Some(true),
            ..LifecycleFacts::new()
        };
        assert_eq!(
            rail(saved),
            "Draft ✓ · Saved ✓ · Checked ✓ · Active ◐ · Run ○"
        );
        let findings = LifecycleFacts {
            saved: true,
            check_clean: Some(false),
            declared_active: Some(false),
            ..LifecycleFacts::new()
        };
        assert_eq!(
            rail(findings),
            "Draft ✓ · Saved ✓ · Checked ! · Active ⏸ · Run ○"
        );
    }

    /// The run field says what the last run did — and a gate that waits
    /// pauses it whatever the last exit was.
    #[test]
    fn the_run_field_reads_the_last_exit_and_a_waiting_gate() {
        let run = |last_exit: u8| {
            let facts = LifecycleFacts {
                saved: true,
                check_clean: Some(true),
                run: RunFact::Exit(last_exit),
                ..LifecycleFacts::new()
            };
            Lifecycle::from_facts(&facts).run
        };
        assert_eq!(run(0), Stage::Done);
        assert_eq!(run(1), Stage::Failed);
        assert_eq!(run(2), Stage::Attention);
        assert_eq!(run(4), Stage::Paused);
        let gated = LifecycleFacts {
            run: RunFact::GateWaits,
            ..LifecycleFacts::new()
        };
        assert_eq!(Lifecycle::from_facts(&gated).run, Stage::Paused);
    }
}
