// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The trigger a request names, read from its words: the cadence and the time of day a phrase
//! states, the period multiples five cron fields cannot hold, the cron fields a phrase states
//! whole (with the last day of a month and an interval of weeks from its start date, the two
//! forms the arming grammar holds beyond plain fields), and the form of a trigger clause (a
//! distribution, a sequence, a schedule or an event). Pure over the words: nothing here binds a
//! requirement, asks a question or reads a request; `nika-compile` turns what these read into
//! its `requested_trigger`.
//!
//! Ascended from `nika-compile` at the 15k prod-LOC wall (ADR-142 · the ADR-141 precedent):
//! per D-2026-07-09-N1 this is ONE architectural unit in several workspace members. The
//! dependency runs `nika-compile` → `nika-compile-trigger` → `nika-compile-reader`, never
//! back. The words only this reading reads came up from the reader with it (`words`); the
//! cadence tables the reader reads itself stay in `nika_compile_reader::trigger_words`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod form;
mod forms;
pub mod multiple;
mod reading;
pub mod schedule;
pub mod words;

pub use form::{TriggerForm, arriving, classify};
pub use reading::{phrase_words, stated_cadence, time_of_day};
