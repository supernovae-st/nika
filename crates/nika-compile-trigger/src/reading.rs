// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a trigger phrase states: its words, its coarsest cadence and its time of day.

use crate::words::{AT, BETWEEN, NAMED_TIMES, TIME_UNITS};
use nika_compile_reader::trigger_words::{DAILY, HOURLY, MINUTELY, MONTHLY, WEEKDAYS, WEEKLY};
use nika_compile_reader::words::day_part_compound;

/// The words of a folded trigger phrase or cadence answer (a clock keeps its `:`).
#[must_use]
pub fn phrase_words(folded: &str) -> Vec<&str> {
    folded
        .split(|c: char| !c.is_alphanumeric() && c != ':')
        .filter(|w| !w.is_empty())
        .collect()
}

/// The cadence and the time of day the words state, each when they state one.
#[must_use]
pub fn stated_cadence(words: &[&str]) -> (Option<&'static str>, Option<String>) {
    let (at, consumed) = time_of_day(words);
    (cadence(words, &consumed), at)
}

/// The coarsest cadence the words state, the named day or working day winning over the
/// day it also names ("every monday morning" is weekly).
fn cadence(words: &[&str], consumed: &[usize]) -> Option<&'static str> {
    let free: Vec<&str> = words
        .iter()
        .enumerate()
        .filter(|(i, _)| !consumed.contains(i))
        .map(|(_, w)| *w)
        .collect();
    let has = |table: &[&str]| {
        free.iter().any(|w| {
            table.contains(w) || day_part_compound(w).is_some_and(|(day, _)| table.contains(&day))
        })
    };
    if has(WEEKDAYS) {
        Some("weekdays")
    } else if has(WEEKLY) {
        Some("weekly")
    } else if has(MONTHLY) {
        Some("monthly")
    } else if has(DAILY) {
        Some("daily")
    } else if has(HOURLY) {
        Some("hourly")
    } else if has(MINUTELY) {
        Some("minutely")
    } else {
        None
    }
}

/// The time of day the words state after an introducer ("at 9", "at 9:30 pm", "à 9h30",
/// "a las 8", "um 9 uhr") or by name ("noon"), as `HH:MM`, with the indices of the words
/// the time consumed (a unit after the number is the time's, never a cadence).
#[must_use]
pub fn time_of_day(words: &[&str]) -> (Option<String>, Vec<usize>) {
    for (i, word) in words.iter().enumerate() {
        if let Some((_, time)) = NAMED_TIMES.iter().find(|(name, _)| name == word) {
            return (Some((*time).to_owned()), vec![i]);
        }
        if !AT.contains(word) {
            continue;
        }
        let mut k = i + 1;
        while words.get(k).is_some_and(|w| BETWEEN.contains(w)) {
            k += 1;
        }
        let Some(number) = words.get(k) else {
            continue;
        };
        let mut consumed = vec![i, k];
        let mut meridiem = None;
        let mut token = (*number).to_owned();
        for suffix in ["am", "pm"] {
            if let Some(stem) = token.strip_suffix(suffix) {
                meridiem = Some(suffix);
                token = stem.to_owned();
            }
        }
        if let Some(next) = words.get(k + 1) {
            if matches!(*next, "am" | "pm") {
                meridiem = Some(*next);
                consumed.push(k + 1);
            } else if TIME_UNITS.contains(next) {
                consumed.push(k + 1);
            }
        }
        let Some((hour, minute)) = clock(&token) else {
            continue;
        };
        let hour = match (meridiem, hour) {
            (Some("pm"), h) if h < 12 => h + 12,
            (Some("am"), 12) => 0,
            (_, h) => h,
        };
        if hour > 23 || minute > 59 {
            continue;
        }
        return (Some(format!("{hour:02}:{minute:02}")), consumed);
    }
    (None, Vec::new())
}

/// `9` · `09` · `9:30` · `9h` · `9h30` → (hour, minute); anything else is not a clock.
fn clock(token: &str) -> Option<(u32, u32)> {
    let (hour, minute) = match token.split_once([':', 'h']) {
        Some((hour, "")) => (hour, "0"),
        Some((hour, minute)) => (hour, minute),
        None => (token, "0"),
    };
    if hour.is_empty() || hour.len() > 2 || minute.len() > 2 {
        return None;
    }
    Some((hour.parse().ok()?, minute.parse().ok()?))
}
