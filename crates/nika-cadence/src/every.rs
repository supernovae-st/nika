// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The anchored interval — `every N weeks from DATE HH:MM`.
//!
//! A cron cannot say « every other week »: its day fields keep no memory of
//! which week is on. This form keeps it in its ANCHOR, the first slot as a
//! civil date and time in the beat's zone. Every slot is the anchor plus a
//! whole number of periods in civil time, so 09:00 stays 09:00 across a
//! change, and each slot is resolved by N1 like any cron slot. The walk is
//! arithmetic (a period count), never a day loop: a 52-week beat ten years
//! on costs what a weekly one costs.
//!
//! ONE spelling: `every <N> week|weeks from <YYYY-MM-DD> <H:MM|HH:MM>`, `N`
//! in `1..=52`. The anchor is required: « every other week » without it
//! names two different beats, and choosing one would be a guess. The
//! compiler lowers the phrases to this form and asks for the anchor when a
//! request has none.

use jiff::Span;
use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;

use crate::cron::CronSpec;
use crate::error::{CadenceError, CadenceErrorKind};
use crate::next::{Shift, Slot, resolve};

/// The widest period, a year of weeks. A yearly beat on a calendar date is
/// a cron (`0 9 5 10 *`).
const MAX_WEEKS: u32 = 52;

/// The fix every refusal of this form teaches.
const FORM: &str = "Write `every <N> weeks from <YYYY-MM-DD> <HH:MM>` with N from 1 to 52, for example `TZ=Europe/Paris every 2 weeks from 2026-10-05 09:00`";

/// Parse the tokens after the zone. `spans` are the tokens' byte spans in
/// the whole expression and `whole` the span of the whole form, painted
/// when the missing piece has no token of its own.
pub(crate) fn parse(
    tokens: &[&str],
    spans: &[(usize, usize)],
    whole: (usize, usize),
) -> Result<(DateTime, u8), CadenceError> {
    let at = |i: usize| spans.get(i).copied().unwrap_or(whole);
    if tokens.first() != Some(&"every") {
        return Err(refusal(
            CadenceErrorKind::PhraseSyntax,
            "the interval form starts with `every`, in lowercase",
            at(0),
        ));
    }
    let weeks = parse_weeks(tokens.get(1).copied(), at(1))?;
    match tokens.get(2).copied() {
        Some("week" | "weeks") => {}
        Some(unit) => {
            return Err(refusal(
                CadenceErrorKind::PhraseSyntax,
                &format!(
                    "`{unit}`: only weeks have an anchored interval; a daily or monthly beat is a cron (`0 9 * * *`, `0 9 1 * *`)"
                ),
                at(2),
            ));
        }
        None => return Err(no_anchor(whole)),
    }
    match tokens.get(3).copied() {
        Some("from") => {}
        Some(word) => {
            return Err(refusal(
                CadenceErrorKind::PhraseSyntax,
                &format!("`{word}`: the anchor follows `from`"),
                at(3),
            ));
        }
        None => return Err(no_anchor(whole)),
    }
    let (Some(date), Some(time)) = (tokens.get(4).copied(), tokens.get(5).copied()) else {
        return Err(no_anchor(whole));
    };
    if tokens.len() > 6 {
        return Err(refusal(
            CadenceErrorKind::PhraseSyntax,
            "nothing follows the anchor time",
            at(6),
        ));
    }
    let date = parse_date(date, at(4))?;
    let (hour, minute) = parse_time(time, at(5))?;
    Ok((date.at(hour, minute, 0, 0), weeks))
}

/// The period: digits only, `1..=52`.
fn parse_weeks(token: Option<&str>, span: (usize, usize)) -> Result<u8, CadenceError> {
    let Some(text) = token else {
        return Err(refusal(
            CadenceErrorKind::PhraseSyntax,
            "`every` needs its period, a number of weeks",
            span,
        ));
    };
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refusal(
            CadenceErrorKind::PhraseSyntax,
            &format!(
                "`{text}`: the period is a number of weeks written in digits (`every 2 weeks`)"
            ),
            span,
        ));
    }
    match text.parse::<u32>() {
        Ok(n) if (1..=MAX_WEEKS).contains(&n) => {
            u8::try_from(n).map_err(|_| out_of_range(text, span))
        }
        _ => Err(out_of_range(text, span)),
    }
}

fn out_of_range(text: &str, span: (usize, usize)) -> CadenceError {
    refusal(
        CadenceErrorKind::FieldRange,
        &format!("`every {text} weeks`: the period is 1 to 52 weeks"),
        span,
    )
}

/// The anchor date: exactly `YYYY-MM-DD`, a day the calendar has.
fn parse_date(text: &str, span: (usize, usize)) -> Result<Date, CadenceError> {
    let shaped = text.len() == 10
        && text.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
    shaped
        .then(|| text.parse::<Date>().ok())
        .flatten()
        .ok_or_else(|| {
            refusal(
                CadenceErrorKind::PhraseSyntax,
                &format!(
                    "`{text}`: the anchor date is a civil date that exists, written YYYY-MM-DD"
                ),
                span,
            )
        })
}

/// The anchor time: `H:MM` or `HH:MM` on a 24-hour clock.
fn parse_time(text: &str, span: (usize, usize)) -> Result<(i8, i8), CadenceError> {
    let parsed = text.split_once(':').and_then(|(h, m)| {
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if !(digits(h) && h.len() <= 2 && digits(m) && m.len() == 2) {
            return None;
        }
        let hour = h.parse::<i8>().ok().filter(|v| (0..=23).contains(v))?;
        let minute = m.parse::<i8>().ok().filter(|v| (0..=59).contains(v))?;
        Some((hour, minute))
    });
    parsed.ok_or_else(|| {
        refusal(
            CadenceErrorKind::PhraseSyntax,
            &format!("`{text}`: the anchor time is HH:MM on a 24-hour clock"),
            span,
        )
    })
}

fn no_anchor(span: (usize, usize)) -> CadenceError {
    refusal(
        CadenceErrorKind::PhraseSyntax,
        "`every N weeks` needs its anchor, the date and time of the first slot: it says which week is on, and it is never guessed",
        span,
    )
}

fn refusal(kind: CadenceErrorKind, detail: &str, span: (usize, usize)) -> CadenceError {
    CadenceError::file(kind, format!("cadence {detail}"), FORM).with_span(span)
}

/// The canonical text: it parses back to the same cadence.
pub(crate) fn describe(tz: &str, anchor: DateTime, weeks: u8) -> String {
    let unit = if weeks == 1 { "week" } else { "weeks" };
    format!(
        "TZ={tz} every {weeks} {unit} from {} {:02}:{:02}",
        anchor.date(),
        anchor.hour(),
        anchor.minute()
    )
}

/// The weekly wake an OS unit can say: the anchor's weekday and time. On an
/// off week the firer finds the previous on-week slot already claimed.
pub(crate) fn weekly_wake(anchor: DateTime) -> CronSpec {
    let as_u8 = |v: i8| u8::try_from(v).unwrap_or(0);
    CronSpec::weekly(
        as_u8(anchor.hour()),
        as_u8(anchor.minute()),
        as_u8(anchor.weekday().to_sunday_zero_offset()),
    )
}

/// The first slot strictly after `from`.
pub(crate) fn next_after(
    tz: &TimeZone,
    anchor: DateTime,
    weeks: u8,
    from: &jiff::Zoned,
) -> Option<Slot> {
    let period = i64::from(weeks) * 7;
    let floor = periods_to(
        anchor.date(),
        from.with_time_zone(tz.clone()).date(),
        period,
    )?;
    // One period back: a resolved instant can sit a gap after its civil
    // time. Two on: the first candidate whose civil DATE is past `from`'s
    // day is past `from` itself.
    let first = floor.saturating_sub(1).max(0);
    (first..=floor.max(0) + 2)
        .filter_map(|k| slot(anchor, period, k, tz))
        .find(|candidate| &candidate.at > from)
}

/// The last slot at or before `from` — the mirror, under the same N1 law
/// as the cron's: a slot that never existed (a spring gap) is never
/// returned, [`next_after`] carries its advanced fire.
pub(crate) fn prev_before(
    tz: &TimeZone,
    anchor: DateTime,
    weeks: u8,
    from: &jiff::Zoned,
) -> Option<Slot> {
    let period = i64::from(weeks) * 7;
    let floor = periods_to(
        anchor.date(),
        from.with_time_zone(tz.clone()).date(),
        period,
    )?;
    if floor < 0 {
        return None;
    }
    (floor.saturating_sub(2).max(0)..=floor + 1)
        .rev()
        .filter_map(|k| slot(anchor, period, k, tz))
        .find(|candidate| candidate.shift != Shift::AdvancedFirstValid && &candidate.at <= from)
}

/// Whole periods from the anchor's date to `day`, floored (negative before
/// the anchor).
fn periods_to(anchor: Date, day: Date, period: i64) -> Option<i64> {
    let days = anchor.until(day).ok()?.get_days();
    Some(i64::from(days).div_euclid(period))
}

/// The `k`-th slot: the anchor plus `k` periods of civil days, resolved.
fn slot(anchor: DateTime, period: i64, k: i64, tz: &TimeZone) -> Option<Slot> {
    let days = Span::new().try_days(k.checked_mul(period)?).ok()?;
    resolve(anchor.checked_add(days).ok()?, tz)
}
