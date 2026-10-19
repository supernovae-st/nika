// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The facts of a run (spec 04 §`run`): the instant the run serves, where that
//! instant came from, and the silence a catch-up run covers. The caller tells
//! them (a firer its slot, `nika test --at` its instant, a calling run its own);
//! the engine completes them at the run's opening (an instant nobody told is the
//! run's own start, as the run clock gives it) and they never change during the
//! run. `${{ run.scheduled_for }}` and `${{ run.missed }}` read them, the opening
//! frame journals them (spec 17 §the prologue), and a child workflow the run
//! calls is told them.

use nika_types::resource::Value as FieldValue;
use nika_types::timestamp::Timestamp;
use serde_json::{Value, json};

/// Where a run's `scheduled_for` came from (spec 17 §the prologue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduledForSource {
    /// The run was fired for a schedule slot.
    Slot,
    /// `nika test --at` named the instant.
    At,
    /// The run's own start, as the run clock gives it.
    Start,
}

impl ScheduledForSource {
    /// The journal's spelling (`slot` · `at` · `start`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Slot => "slot",
            Self::At => "at",
            Self::Start => "start",
        }
    }
}

/// The silence a catch-up run covers (`manqué: rattraper-une-fois`): the first
/// and the last missed slot, and how many slots it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MissedSlots {
    /// The first slot of the silence.
    pub first: Timestamp,
    /// The last slot of the silence.
    pub last: Timestamp,
    /// How many slots the silence holds.
    pub count: u32,
}

impl MissedSlots {
    /// Construct (INV-019).
    #[must_use]
    pub const fn new(first: Timestamp, last: Timestamp, count: u32) -> Self {
        Self { first, last, count }
    }

    /// The object `${{ run.missed }}` reads.
    fn value(self) -> Value {
        json!({
            "first": self.first.to_string(),
            "last": self.last.to_string(),
            "count": self.count,
        })
    }
}

/// What the caller tells a run about the instant it serves. The default tells
/// nothing: the run serves its own start, and covers no silence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunFacts {
    /// The instant the run serves and where it came from; `None` = the run's
    /// own start.
    pub scheduled_for: Option<(Timestamp, ScheduledForSource)>,
    /// The silence the run covers; `None` outside a catch-up.
    pub missed: Option<MissedSlots>,
}

impl RunFacts {
    /// A run fired for the schedule slot `at`.
    #[must_use]
    pub const fn slot(at: Timestamp) -> Self {
        Self {
            scheduled_for: Some((at, ScheduledForSource::Slot)),
            missed: None,
        }
    }

    /// A run `nika test --at` places at the instant `at`.
    #[must_use]
    pub const fn at(at: Timestamp) -> Self {
        Self {
            scheduled_for: Some((at, ScheduledForSource::At)),
            missed: None,
        }
    }

    /// The silence the run covers.
    #[must_use]
    pub const fn with_missed(mut self, missed: MissedSlots) -> Self {
        self.missed = Some(missed);
        self
    }

    /// The facts as the run starts with them, its opening stamp at `start`.
    #[must_use]
    pub(crate) fn resolved(self, start: Timestamp) -> ResolvedRunFacts {
        let (scheduled_for, source) = self
            .scheduled_for
            .unwrap_or((start, ScheduledForSource::Start));
        let value = json!({
            "scheduled_for": scheduled_for.to_string(),
            "missed": self.missed.map_or(Value::Null, MissedSlots::value),
        });
        ResolvedRunFacts {
            told: Self {
                scheduled_for: Some((scheduled_for, source)),
                missed: self.missed,
            },
            value,
        }
    }
}

/// The facts a run started with, fixed for the whole run.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedRunFacts {
    /// Every fact named: the instant, its source, the silence.
    told: RunFacts,
    /// The object `${{ run }}` reads (`{ scheduled_for, missed }`).
    value: Value,
}

impl ResolvedRunFacts {
    /// The object `${{ run }}` reads.
    pub(crate) const fn value(&self) -> &Value {
        &self.value
    }

    /// What a child workflow the run calls is told (spec 04 §`run`): the same
    /// instant from the same source, and the same silence.
    pub(crate) const fn told(&self) -> RunFacts {
        self.told
    }

    /// The opening frame's fields (spec 17 §the prologue): `scheduled_for` and
    /// its source on every run, and `missed` (compact JSON) only when the run
    /// covers a silence.
    pub(crate) fn prologue_fields(&self) -> Vec<(&'static str, FieldValue)> {
        let mut fields = Vec::new();
        if let Some((at, source)) = self.told.scheduled_for {
            fields.push(("scheduled_for", FieldValue::String(at.to_string())));
            fields.push((
                "scheduled_for_source",
                FieldValue::String(source.as_str().to_owned()),
            ));
        }
        if let Some(missed) = self.told.missed {
            fields.push(("missed", FieldValue::String(missed.value().to_string())));
        }
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT: Timestamp = Timestamp::from_unix_ns(1_791_536_400_000_000_000);
    const START: Timestamp = Timestamp::from_unix_ns(1_791_540_000_000_000_000);

    fn field<'f>(fields: &'f [(&'static str, FieldValue)], key: &str) -> Option<&'f FieldValue> {
        fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
    }

    #[test]
    fn a_run_told_nothing_serves_its_own_start() {
        let facts = RunFacts::default().resolved(START);
        assert_eq!(
            facts.value(),
            &json!({"scheduled_for": START.to_string(), "missed": null})
        );
        let fields = facts.prologue_fields();
        assert_eq!(
            field(&fields, "scheduled_for_source"),
            Some(&FieldValue::String("start".to_owned()))
        );
        assert_eq!(field(&fields, "missed"), None, "no silence, no claim");
    }

    #[test]
    fn a_fired_run_serves_its_slot_and_names_the_silence_it_covers() {
        let missed = MissedSlots::new(Timestamp::from_unix_ns(1_791_277_200_000_000_000), SLOT, 4);
        let facts = RunFacts::slot(SLOT).with_missed(missed).resolved(START);
        assert_eq!(facts.value()["scheduled_for"], json!(SLOT.to_string()));
        assert_eq!(facts.value()["missed"]["count"], json!(4));
        assert_eq!(facts.value()["missed"]["last"], json!(SLOT.to_string()));
        let fields = facts.prologue_fields();
        assert_eq!(
            field(&fields, "scheduled_for"),
            Some(&FieldValue::String(SLOT.to_string()))
        );
        assert_eq!(
            field(&fields, "scheduled_for_source"),
            Some(&FieldValue::String("slot".to_owned()))
        );
        assert_eq!(
            field(&fields, "missed"),
            Some(&FieldValue::String(facts.value()["missed"].to_string()))
        );
    }

    #[test]
    fn a_test_run_at_an_instant_says_so_and_a_child_is_told_the_same() {
        let facts = RunFacts::at(SLOT).resolved(START);
        assert_eq!(facts.value()["scheduled_for"], json!(SLOT.to_string()));
        let child = facts.told().resolved(Timestamp::EPOCH);
        assert_eq!(
            child.value(),
            facts.value(),
            "the child serves the same instant"
        );
        assert_eq!(
            field(&child.prologue_fields(), "scheduled_for_source"),
            Some(&FieldValue::String("at".to_owned()))
        );
        let started = RunFacts::default().resolved(START);
        assert_eq!(
            started.told().resolved(Timestamp::EPOCH).value(),
            started.value(),
            "a caller's own start is its child's instant too"
        );
    }
}
