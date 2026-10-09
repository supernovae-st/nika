// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The one budget every fixture of a round and of a turn spends: fixtures started, attempts,
//! bytes copied in and read back, and elapsed time. The host measures each fixture and stops
//! when the budget says so; this pure account totals what it is told and refuses to count the
//! evidence of a fixture run past a limit.

/// The limits of a round or of a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    pub fixtures: u32,
    pub attempts: u32,
    /// Bytes copied into rooms plus bytes read back from them.
    pub bytes: u64,
    pub elapsed_ms: u64,
}

impl Limits {
    /// The limits on each axis.
    #[must_use]
    pub const fn new(fixtures: u32, attempts: u32, bytes: u64, elapsed_ms: u64) -> Self {
        Self {
            fixtures,
            attempts,
            bytes,
            elapsed_ms,
        }
    }
}

/// What one fixture, or a total of them, spent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Usage {
    pub fixtures: u32,
    pub attempts: u32,
    pub copied_bytes: u64,
    pub read_back_bytes: u64,
    pub elapsed_ms: u64,
}

impl Usage {
    /// What one fixture spent.
    #[must_use]
    pub const fn new(
        fixtures: u32,
        attempts: u32,
        copied_bytes: u64,
        read_back_bytes: u64,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            fixtures,
            attempts,
            copied_bytes,
            read_back_bytes,
            elapsed_ms,
        }
    }

    /// Both totals added, saturating.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        Self {
            fixtures: self.fixtures.saturating_add(other.fixtures),
            attempts: self.attempts.saturating_add(other.attempts),
            copied_bytes: self.copied_bytes.saturating_add(other.copied_bytes),
            read_back_bytes: self.read_back_bytes.saturating_add(other.read_back_bytes),
            elapsed_ms: self.elapsed_ms.saturating_add(other.elapsed_ms),
        }
    }

    /// Bytes copied in and read back.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.copied_bytes.saturating_add(self.read_back_bytes)
    }

    /// The first axis on which this total passes `limits`.
    fn beyond(&self, limits: &Limits) -> Option<Axis> {
        if self.fixtures > limits.fixtures {
            Some(Axis::Fixtures)
        } else if self.attempts > limits.attempts {
            Some(Axis::Attempts)
        } else if self.bytes() > limits.bytes {
            Some(Axis::Bytes)
        } else if self.elapsed_ms > limits.elapsed_ms {
            Some(Axis::Time)
        } else {
            None
        }
    }

    /// The first axis on which this total has reached `limits`: no further fixture may start.
    fn reached(&self, limits: &Limits) -> Option<Axis> {
        if self.fixtures >= limits.fixtures {
            Some(Axis::Fixtures)
        } else if self.attempts >= limits.attempts {
            Some(Axis::Attempts)
        } else if self.bytes() >= limits.bytes {
            Some(Axis::Bytes)
        } else if self.elapsed_ms >= limits.elapsed_ms {
            Some(Axis::Time)
        } else {
            None
        }
    }
}

/// An axis of the budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Axis {
    Fixtures,
    Attempts,
    Bytes,
    Time,
}

/// Whether the budget admits more work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Admission {
    /// Another fixture may start.
    Open,
    /// The round's budget is spent on this axis: stop and cancel what runs.
    RoundSpent(Axis),
    /// The turn's budget is spent on this axis.
    TurnSpent(Axis),
}

/// The running totals of one round against its limits and its turn's.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Budget {
    round_limits: Limits,
    turn_limits: Limits,
    round: Usage,
    turn: Usage,
}

impl Budget {
    /// A new round of `round` limits, in a turn of `turn` limits that already spent
    /// `turn_spent` in its earlier rounds.
    #[must_use]
    pub fn new(round: Limits, turn: Limits, turn_spent: Usage) -> Self {
        Self {
            round_limits: round,
            turn_limits: turn,
            round: Usage::default(),
            turn: turn_spent,
        }
    }

    /// Whether another fixture may start: every total still under its round and turn limit.
    #[must_use]
    pub fn admission(&self) -> Admission {
        if let Some(axis) = self.round.reached(&self.round_limits) {
            return Admission::RoundSpent(axis);
        }
        self.turn
            .reached(&self.turn_limits)
            .map_or(Admission::Open, Admission::TurnSpent)
    }

    /// Charge what one fixture spent. The answer is `Open` when the totals stay within every
    /// limit; otherwise the fixture ran past the budget and its evidence must not count.
    pub fn charge(&mut self, spent: &Usage) -> Admission {
        self.round = self.round.plus(spent);
        self.turn = self.turn.plus(spent);
        if let Some(axis) = self.round.beyond(&self.round_limits) {
            return Admission::RoundSpent(axis);
        }
        self.turn
            .beyond(&self.turn_limits)
            .map_or(Admission::Open, Admission::TurnSpent)
    }

    /// What the round spent so far.
    #[must_use]
    pub fn round(&self) -> Usage {
        self.round
    }

    /// What the turn spent so far, this round included.
    #[must_use]
    pub fn turn(&self) -> Usage {
        self.turn
    }
}
