// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's activity, typed: what Nika is doing NOW and what just
//! finished, from the machine's own truth (the compiler's reading, the
//! seat call, the check, the run) — never a percentage, never a phase
//! read back from prose. A door prints `line()`; the renderer's busy row
//! keeps the last completed phase beside the current one.

/// The human-level phase of a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Phase {
    /// The request is being read (the deterministic reading, then a seat).
    Understanding,
    /// Knowledge is being selected for it (only when the compiler reports one).
    Knowledge,
    /// The workflow is being built by a seat.
    Authoring,
    /// The workflow is being checked.
    Checking,
    /// A finding is being repaired, or a stronger model reads the request.
    Repairing,
}

/// One activity: a phase, its note in words, and whether it is done.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Activity {
    /// The phase.
    pub phase: Phase,
    /// The note in the human's words (« understood 6 requirements »).
    pub note: String,
    /// Finished (« ✓ ») or under way (« ● » · « ↻ »).
    pub done: bool,
}

impl Activity {
    /// An activity under way.
    #[must_use]
    pub fn now(phase: Phase, note: impl Into<String>) -> Self {
        Self {
            phase,
            note: note.into(),
            done: false,
        }
    }

    /// A finished activity.
    #[must_use]
    pub fn done(phase: Phase, note: impl Into<String>) -> Self {
        Self {
            phase,
            note: note.into(),
            done: true,
        }
    }

    /// The glyph: ✓ done · ↻ repairing · ● working (the renderer's busy
    /// row and the plain loop share it; the loader turns beside ● only).
    #[must_use]
    pub const fn glyph(&self) -> char {
        if self.done {
            '✓'
        } else if matches!(self.phase, Phase::Repairing) {
            '↻'
        } else {
            '●'
        }
    }

    /// The line a door prints: the glyph and the note.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{} {}", self.glyph(), self.note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A finished phase reads ✓, a repair ↻, work under way ●; the line is
    /// the glyph and the words, nothing else (no percentage, no code).
    #[test]
    fn an_activity_reads_as_its_glyph_and_its_words() {
        assert_eq!(
            Activity::done(Phase::Understanding, "understood 6 requirements").line(),
            "✓ understood 6 requirements"
        );
        assert_eq!(
            Activity::now(Phase::Authoring, "authoring · openai/gpt-5.2").line(),
            "● authoring · openai/gpt-5.2"
        );
        assert_eq!(
            Activity::now(Phase::Repairing, "a stronger model reads it").line(),
            "↻ a stronger model reads it"
        );
        assert_eq!(Activity::now(Phase::Checking, "checking").glyph(), '●');
    }
}
