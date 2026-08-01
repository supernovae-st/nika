// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The three semantic doors — the ONE catalog behind every taught
//! first-contact command.
//!
//! `try` (discover · see without owning) · `new` (create · take a
//! slug or route plain words) · `init` (project · found a repo). The
//! concierge rows, the JSON mirror and the metrics classification all
//! derive their door commands from here, and the parse ratchet in
//! `nika-cli` replays every taught command against the live clap
//! tree. The 0.107 train renamed the showroom door (`examples` →
//! `try`) while the concierge kept teaching the dead verb — a rename
//! now lands in ONE constant and every projection follows, or the
//! ratchet refuses the build before a human can paste a ghost.

/// The stable semantic identity of a first-contact door. The id is
/// the contract: labels and commands may specialize per surface and
/// state, the MEANING never diverges. Exhaustive by design — three
/// doors is a product law (no fourth door, no `nika start`), so a
/// new variant is a product decision, not an additive patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorId {
    /// See a canonical workflow work — nothing written, nothing owned.
    Discover,
    /// Take a workflow for yourself — a slug, or plain words routed.
    Create,
    /// Found (or re-enter) a repo — wiring, rules, the wizard.
    Project,
}

impl DoorId {
    /// Every door, in teaching order (discover → create → project).
    pub const ALL: [Self; 3] = [Self::Discover, Self::Create, Self::Project];

    /// The canonical CLI command behind this door (bare form — a
    /// surface may specialize it with a slug or a path, never rename
    /// the verb).
    #[must_use]
    pub const fn command(self) -> &'static str {
        match self {
            Self::Discover => "nika try",
            Self::Create => "nika new",
            Self::Project => "nika init",
        }
    }

    /// The ≤26-char dim blurb the concierge prints beside the bare
    /// command (welcome's row-comment budget).
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Discover => "the showroom · offline",
            Self::Create => "guided first workflow",
            Self::Project => "found this repo (wizard)",
        }
    }

    /// The concierge row for this door — command + blurb, the shape
    /// `start_moves` speaks.
    #[must_use]
    pub fn row(self) -> (String, &'static str) {
        (self.command().to_owned(), self.blurb())
    }
}

#[cfg(test)]
mod tests {
    use super::DoorId;

    #[test]
    fn the_three_doors_speak_live_verbs_only() {
        for door in DoorId::ALL {
            let cmd = door.command();
            assert!(cmd.starts_with("nika "), "{cmd} speaks the binary name");
            let verb = cmd.trim_start_matches("nika ");
            assert!(
                ["try", "new", "init"].contains(&verb),
                "{verb} is one of the three doors"
            );
        }
    }

    #[test]
    fn blurbs_hold_the_welcome_row_budget() {
        for door in DoorId::ALL {
            assert!(
                door.blurb().chars().count() <= 26,
                "{door:?} blurb must stay ≤26 chars"
            );
        }
    }
}
