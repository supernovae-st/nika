// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `run:` — the run declares its entropy and clock (F-P3 · LOT-1).
//!
//! One envelope block, two closed axes:
//!
//! ```yaml
//! run:
//!   entropy: none | ambient | { seeded: <u64> }
//!   clock: system | virtual
//! ```
//!
//! The block is OPTIONAL. Absent means the status quo, unchanged:
//! `entropy: ambient` + `clock: system` (live event stamps · the real
//! clock). `entropy: ambient` written out loud is equally legal and
//! honest — the declaration never PUNISHES the ambient choice, it makes
//! the run's entropy VISIBLE to the contract. The ladder:
//!
//! - `ambient` — the run consumes ambient entropy (`UUIDv7` ids · wall
//!   clock) and says so. The honest default.
//! - `seeded(N)` — replayability: every determinism seam is forced
//!   (deterministic event stamps · the retry jitter stream keyed by `N`
//!   · the virtual clock). Two runs of the SAME file produce
//!   byte-identical journals.
//! - `none` — strict determinism DEMANDED: the same forced seams with
//!   the jitter stream fixed at 0, and `nika check` refuses the file if
//!   the body still uses a structural entropy source (a `retry:` jitter
//!   · the `nika:uuid` builtin).
//!
//! `clock: virtual` is the simulated-clock declaration (the FDB/VOPR law
//! · one run = ONE clock): the pure virtual clock of `nika-clock`
//! (deterministic · never `tokio::time::pause`). Under `entropy:
//! none | seeded(N)` the virtual clock is implied — a run that demands
//! deterministic journals cannot let task durations ride the wall clock
//! — so an explicit `clock: system` beside them is a contradiction, and
//! `entropy: ambient` beside `clock: virtual` is the mirror one; both
//! are refused at parse.
//!
//! The F-N10 receipt-side enums (`time_source` / `time_scale`) are OUT
//! of scope here by decision — they stamp receipts, this block declares
//! seams.
//!
//! The form mirrors the `assert:` vocabulary idiom: the parameterless
//! values are bare scalars, the one parameterized value is a single-key
//! map (`{ seeded: 42 }`) — no new micro-grammar for one number.

/// The `run.entropy` declaration (F-P3) — where the run's randomness
/// comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RunEntropy {
    /// `none` — strict determinism demanded: deterministic seams AND a
    /// static refusal when the body still uses a structural entropy
    /// source (the check's `run_decl` lane owns that judgment).
    None,
    /// `{ seeded: N }` — replayable: deterministic seams, the retry
    /// jitter stream keyed by `N` (`(seed, task, attempt)` ·
    /// replay-stable by construction).
    Seeded(u64),
    /// `ambient` — the honest status quo: live event stamps (`UUIDv7` ·
    /// wall clock). The default when `run:` is absent.
    Ambient,
}

impl RunEntropy {
    /// The wire spelling (`none` · `ambient`) — `seeded` carries its
    /// payload, so only the bare forms round-trip a name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Seeded(_) => "seeded",
            Self::Ambient => "ambient",
        }
    }

    /// Whether the declaration forces the deterministic seams (event
    /// stamps · virtual clock · keyed jitter stream) — `none` and
    /// `seeded` do, `ambient` never does.
    #[must_use]
    pub const fn is_deterministic(&self) -> bool {
        !matches!(self, Self::Ambient)
    }

    /// The retry jitter seed this declaration keys (`none` and `ambient`
    /// pin the zero stream — `ambient` keeps today's default exactly).
    #[must_use]
    pub const fn jitter_seed(&self) -> u64 {
        match self {
            Self::Seeded(n) => *n,
            Self::None | Self::Ambient => 0,
        }
    }
}

/// The `run.clock` declaration (F-P3) — which clock the run's deadlines,
/// sleeps and durations measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RunClock {
    /// `system` — the production wall/monotonic clock (`SystemClock`).
    /// The default when `run:` is absent.
    System,
    /// `virtual` — the pure simulated clock (`VirtualClock`): instant
    /// sleeps, time moved only by the engine, replay-stable. Never
    /// `tokio::time::pause` (the deadline race must stay a RACE the
    /// engine observes, not a scheduler it drives — the F-P3 bound).
    Virtual,
}

impl RunClock {
    /// The wire spelling.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Virtual => "virtual",
        }
    }
}

/// The parsed `run:` envelope block (F-P3) — each axis `Some` only when
/// EXPLICITLY authored: the refusal (`entropy: ambient` × `clock:
/// virtual`) and the seam resolution both distinguish « declared » from
/// « defaulted », so the block never pre-fills its defaults here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct RunDecl {
    /// `entropy:` — absent = the ambient default (status quo).
    pub entropy: Option<RunEntropy>,
    /// `clock:` — absent = the system default (status quo) — forced
    /// `virtual` at seam resolution when `entropy` demands determinism.
    pub clock: Option<RunClock>,
}

impl RunDecl {
    /// The declaration as authored (INV-019 · `new()` on every
    /// `#[non_exhaustive]` struct) — each axis `Some` only when EXPLICIT.
    #[must_use]
    pub const fn new(entropy: Option<RunEntropy>, clock: Option<RunClock>) -> Self {
        Self { entropy, clock }
    }

    /// The effective entropy (absent = `ambient`, the status quo).
    #[must_use]
    pub const fn entropy_or_default(&self) -> RunEntropy {
        match self.entropy {
            Some(e) => e,
            None => RunEntropy::Ambient,
        }
    }

    /// The effective clock the declaration resolves to: an explicit
    /// `clock:` wins; otherwise the entropy axis decides — deterministic
    /// entropy forces the virtual clock (durations may not ride the wall
    /// clock when journals must replay byte-identical), ambient entropy
    /// keeps the system clock (the status quo).
    #[must_use]
    pub const fn clock_or_default(&self) -> RunClock {
        match self.clock {
            Some(c) => c,
            None => {
                if self.entropy_or_default().is_deterministic() {
                    RunClock::Virtual
                } else {
                    RunClock::System
                }
            }
        }
    }

    /// The declared-contradiction law (F-P3 · the parse-level refusal):
    /// a determinism DEMAND (`entropy: none | seeded` · `clock:
    /// virtual`) may not share the block with a declared non-deterministic
    /// source on the other axis (`entropy: ambient` · `clock: system`).
    /// Defaults never contradict — only two EXPLICIT values can.
    #[must_use]
    pub const fn contradiction(&self) -> Option<&'static str> {
        match (self.entropy, self.clock) {
            (Some(RunEntropy::Ambient), Some(RunClock::Virtual)) => Some(
                "`entropy: ambient` declares live entropy but `clock: virtual` demands a \
                 simulated clock — the run cannot be both ambient and simulated",
            ),
            (Some(e), Some(RunClock::System)) if e.is_deterministic() => Some(
                "`entropy: none | seeded` forces the deterministic seams (byte-identical \
                 journals) but `clock: system` lets task durations ride the wall clock — \
                 drop `clock: system` (the virtual clock is implied) or declare `clock: virtual`",
            ),
            _ => None,
        }
    }
}

impl Default for RunDecl {
    /// Both axes defaulted — the ambient + system status quo an absent
    /// `run:` block means.
    fn default() -> Self {
        Self::new(None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_block_is_the_status_quo() {
        let decl = RunDecl::default();
        assert_eq!(decl.entropy_or_default(), RunEntropy::Ambient);
        assert_eq!(decl.clock_or_default(), RunClock::System);
        assert_eq!(decl.entropy_or_default().jitter_seed(), 0);
        assert!(decl.contradiction().is_none());
    }

    #[test]
    fn deterministic_entropy_forces_the_virtual_clock() {
        for e in [RunEntropy::None, RunEntropy::Seeded(42)] {
            let decl = RunDecl {
                entropy: Some(e),
                clock: None,
            };
            assert_eq!(decl.clock_or_default(), RunClock::Virtual);
            assert!(decl.contradiction().is_none());
        }
    }

    #[test]
    fn the_two_explicit_contradictions_are_named() {
        let ambient_virtual = RunDecl {
            entropy: Some(RunEntropy::Ambient),
            clock: Some(RunClock::Virtual),
        };
        assert!(ambient_virtual.contradiction().is_some());
        let seeded_system = RunDecl {
            entropy: Some(RunEntropy::Seeded(42)),
            clock: Some(RunClock::System),
        };
        assert!(seeded_system.contradiction().is_some());
        // Redundant but coherent: explicit virtual beside seeded.
        let coherent = RunDecl {
            entropy: Some(RunEntropy::Seeded(42)),
            clock: Some(RunClock::Virtual),
        };
        assert!(coherent.contradiction().is_none());
        // The explicit status quo: ambient + system, said out loud.
        let spelled = RunDecl {
            entropy: Some(RunEntropy::Ambient),
            clock: Some(RunClock::System),
        };
        assert!(spelled.contradiction().is_none());
        // One explicit axis alone never contradicts (defaults compose).
        let ambient_alone = RunDecl {
            entropy: Some(RunEntropy::Ambient),
            clock: None,
        };
        assert!(ambient_alone.contradiction().is_none());
        let virtual_alone = RunDecl {
            entropy: None,
            clock: Some(RunClock::Virtual),
        };
        assert!(virtual_alone.contradiction().is_none());
    }

    #[test]
    fn the_seed_keys_only_the_seeded_stream() {
        assert_eq!(RunEntropy::Seeded(42).jitter_seed(), 42);
        assert_eq!(RunEntropy::None.jitter_seed(), 0);
        assert_eq!(RunEntropy::Ambient.jitter_seed(), 0);
        assert_eq!(RunEntropy::None.name(), "none");
        assert_eq!(RunEntropy::Seeded(1).name(), "seeded");
        assert_eq!(RunClock::Virtual.name(), "virtual");
        assert_eq!(RunClock::System.name(), "system");
    }
}
