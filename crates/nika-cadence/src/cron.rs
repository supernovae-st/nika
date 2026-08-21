// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The hand-counted 5-field cron — zero library, on purpose, and ONE
//! encoding per field: a bitset.
//!
//! Three scars own this file:
//!
//! - **#6** · a library that silently accepts SIX fields fires at the
//!   wrong time (the draft scheduler's, verbatim). So the field COUNT is
//!   the TYPE — `parse_cron_fields` takes `&[&str; 5]`; the compiler,
//!   not the discipline, refuses the sixth.
//! - **the Vixie OR** · `dom` and `dow` restricted together match when
//!   EITHER does — the classic mis-fire trap. This grammar refuses the
//!   combination, judged on the SETS (a `1-31` dom is every day, so only
//!   the dow side restricts — semantically, never textually).
//! - **the two encodings** · the pre-bitset struct expanded `minutes`/
//!   `hours` but read « empty means all » for `dom`/`months`/`dow` —
//!   every check carried two arms, and `describe("* * * * *")` rendered
//!   244 characters (the expanded minutes). [`Field`] is ONE encoding:
//!   bit `v - LO` set ⇔ value `v` present · `*` is all bits set ·
//!   8 bytes · `Copy` · zero alloc.
//!
//! The weekday origin is NAMED: `0` and `7` are dimanche/Sunday, `1` is
//! lundi/Monday (`0 9 * * 1` reads as Monday on two hosts, Sunday on a
//! third — not here).

use crate::error::{CadenceError, CadenceErrorKind};

/// One cron field as a bitset over `LO..=HI` — ONE encoding for one
/// field. `*` is all bits set; there is no empty-means-all second form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field<const LO: u8, const HI: u8> {
    bits: u64,
}

impl<const LO: u8, const HI: u8> Field<LO, HI> {
    /// No bit set — the parser's starting point (`*` never produces it).
    fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Every value set — the `*` field.
    fn full() -> Self {
        // The bitset holds a field whose width fits a u64 — the grammar's
        // five fields are ≤60 wide; a wider instantiation is a type bug.
        debug_assert!(LO <= HI && HI - LO < 64);
        let width = HI - LO + 1;
        Self {
            bits: if width >= 64 {
                u64::MAX
            } else {
                (1u64 << width) - 1
            },
        }
    }

    /// Mark value `v` present. The parser guarantees `LO <= v <= HI`.
    fn set(&mut self, v: u8) {
        debug_assert!(LO <= HI && HI - LO < 64);
        self.bits |= 1u64 << (v - LO);
    }

    /// Is value `v` in this field?
    #[must_use]
    pub fn contains(&self, v: u8) -> bool {
        v >= LO && v <= HI && self.bits & (1u64 << (v - LO)) != 0
    }

    /// All values set (`*` or a full range — the display prints `*`).
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.bits == Self::full().bits
    }

    /// No value set — a parse error, never a valid field.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// How many values are set.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.bits.count_ones()
    }

    /// The values, ascending (FCI-014 — an iterator, never a Vec).
    /// Double-ended by construction (a filtered inclusive range), so the
    /// backwards walk of `CronSpec::prev_before` rides `.rev()`.
    #[must_use]
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = u8> {
        (LO..=HI).filter(move |v| self.contains(*v))
    }

    /// The single value when exactly one is set (the readable form's test).
    #[must_use]
    pub fn single(&self) -> Option<u8> {
        if self.len() == 1 {
            self.iter().next()
        } else {
            None
        }
    }
}

/// The five cron fields — one bitset each, 40 bytes, `Copy`, zero alloc.
/// Every field carries ALL its values (`*` is all bits set): matching is
/// five `contains`, with no empty-means-all special case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CronSpec {
    minutes: Field<0, 59>,
    hours: Field<0, 23>,
    dom: Field<1, 31>,
    months: Field<1, 12>,
    dow: Field<0, 6>,
}

impl CronSpec {
    /// The minute field (`0..=59`).
    #[must_use]
    pub fn minutes(&self) -> &Field<0, 59> {
        &self.minutes
    }

    /// The hour field (`0..=23`).
    #[must_use]
    pub fn hours(&self) -> &Field<0, 23> {
        &self.hours
    }

    /// The day-of-month field (`1..=31`) — full means every day.
    #[must_use]
    pub fn dom(&self) -> &Field<1, 31> {
        &self.dom
    }

    /// The month field (`1..=12`) — full means every month.
    #[must_use]
    pub fn months(&self) -> &Field<1, 12> {
        &self.months
    }

    /// The weekday field (`0..=6`, Sunday `0`) — full means every day.
    #[must_use]
    pub fn dow(&self) -> &Field<0, 6> {
        &self.dow
    }

    /// Build from already-validated fields (the readable form's path).
    pub(crate) fn weekly(hour: u8, minute: u8, weekday: u8) -> Self {
        let mut minutes = Field::empty();
        minutes.set(minute);
        let mut hours = Field::empty();
        hours.set(hour);
        let mut dow = Field::empty();
        dow.set(weekday);
        Self {
            minutes,
            hours,
            dom: Field::full(),
            months: Field::full(),
            dow,
        }
    }

    /// Does this civil date carry a slot? (Field matching only — the
    /// day/month/weekday predicates, never the clock.)
    pub(crate) fn covers(&self, date: jiff::civil::Date) -> bool {
        let Ok(month) = u8::try_from(date.month()) else {
            return false;
        };
        let Ok(day) = u8::try_from(date.day()) else {
            return false;
        };
        if !self.months.contains(month) || !self.dom.contains(day) {
            return false;
        }
        // jiff offsets Sunday at 1 — this grammar NAMES Sunday at 0.
        let Ok(dow) = u8::try_from((date.weekday().to_sunday_one_offset() + 6) % 7) else {
            return false;
        };
        self.dow.contains(dow)
    }
}

const MONTH_NAMES: [(&str, u8); 12] = [
    ("jan", 1),
    ("feb", 2),
    ("mar", 3),
    ("apr", 4),
    ("may", 5),
    ("jun", 6),
    ("jul", 7),
    ("aug", 8),
    ("sep", 9),
    ("oct", 10),
    ("nov", 11),
    ("dec", 12),
];

const DOW_NAMES: [(&str, u8); 7] = [
    ("sun", 0),
    ("mon", 1),
    ("tue", 2),
    ("wed", 3),
    ("thu", 4),
    ("fri", 5),
    ("sat", 6),
];

/// Parse five tokens — the COUNT is the array type (scar #6 is the
/// compiler's, not a discipline's). Each field then validates its shape
/// and ranges. Errors carry the field label so the caller can attach
/// the byte span of the faulty token.
pub(crate) fn parse_cron_fields(tokens: &[&str; 5]) -> Result<CronSpec, CadenceError> {
    let minutes = parse_field(tokens[0], &[], "minute")?;
    let hours = parse_field(tokens[1], &[], "heure")?;
    let dom = parse_field(tokens[2], &[], "jour-du-mois")?;
    let months = parse_field(tokens[3], &MONTH_NAMES, "mois")?;
    let dow7: Field<0, 7> = parse_field(tokens[4], &DOW_NAMES, "jour-de-semaine")?;
    // 7 is dimanche — folded into 0, the NAMED origin.
    let mut dow = Field::empty();
    for v in dow7.iter() {
        dow.set(if v == 7 { 0 } else { v });
    }
    if !dom.is_full() && !dow.is_full() {
        return Err(CadenceError::file(
            CadenceErrorKind::DomDowOr,
            "jour-du-mois ET jour-de-semaine restreints ensemble — le OR de Vixie entre les deux est le piège classique du tir raté",
            "garde un seul côté · `* * * * 1` (chaque lundi) ou `* * 1 * *` (chaque 1er), jamais les deux",
        ));
    }
    // A (dom, months) pair no calendar can satisfy parses as five well-formed
    // fields, clears the Vixie guard, and then dies quietly: covers() is true
    // on no day, next_after walks its whole horizon, and the beat never fires
    // without a word. `31 4` is a banal typo. This grammar names every refusal
    // and teaches its fix; a silent None is the one outcome that contract
    // forbids, so the impossibility is caught HERE, at parse, not discovered
    // by an operator wondering why nothing ran.
    //
    // Judged on the SETS, like the Vixie guard above: one satisfiable month is
    // enough for the whole set. An unrestricted side can never make the pair
    // impossible, so no special case is needed for `*` on either side.
    if !months
        .iter()
        .any(|m| dom.iter().any(|d| d <= longest_day_of(m)))
    {
        return Err(CadenceError::file(
            CadenceErrorKind::DateImpossible,
            "aucun jour demandé n'existe dans les mois demandés · le beat parserait bien et ne tirerait JAMAIS",
            "vérifie la paire jour/mois · avril juin septembre novembre s'arrêtent à 30, février à 29",
        ));
    }

    Ok(CronSpec {
        minutes,
        hours,
        dom,
        months,
        dow,
    })
}

/// The longest day a month can have, ever.
///
/// February takes 29, not 28: a leap year makes the 29th real, so `29 2` is a
/// beat that fires rarely rather than an impossible one. Only 30 and 31 are
/// impossible there. Refusing the 29th would be the mirror of the fault this
/// guard exists to close.
/// A month outside 1..=12 answers 0, which is FAIL-CLOSED: `d <= 0` is false
/// for every `d` a `Field<1, 31>` can hold, so an impossible month
/// contributes nothing to the satisfiability check instead of admitting
/// everything. The arm is unreachable today (`months.iter()` walks a
/// `Field<1, 12>`), and its DIRECTION still matters: a `_ => 31` default
/// handed the most permissive bound to the one input nobody vetted, which is
/// the silent pass this guard exists to kill. Named by a Rust review,
/// 2026-08-13.
pub(crate) const fn longest_day_of(month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 29,
        _ => 0,
    }
}

/// One field: `*` · `*/n` · `a` · `a-b` · `a-b/n` · `a/n` · comma-joined.
fn parse_field<const LO: u8, const HI: u8>(
    text: &str,
    names: &[(&str, u8)],
    label: &'static str,
) -> Result<Field<LO, HI>, CadenceError> {
    let mut out = Field::empty();
    for part in text.split(',') {
        let (range_text, step) = match part.split_once('/') {
            Some((base, stride)) => {
                let step: u8 = stride.parse().ok().filter(|n| *n >= 1).ok_or_else(|| {
                    CadenceError::file(
                        CadenceErrorKind::FieldSyntax,
                        format!("champ {label} `{part}` · un pas vaut au moins 1"),
                        format!("`{label}` · ex. `*/15`"),
                    )
                    .on_field(label)
                })?;
                (base, step)
            }
            None => (part, 1),
        };
        let has_step = part.contains('/');
        if range_text == "*" {
            set_range(&mut out, LO, HI, step);
        } else if let Some((a, b)) = range_text.split_once('-') {
            let from = field_value::<LO, HI>(a, names, label)?;
            let to = field_value::<LO, HI>(b, names, label)?;
            if from > to {
                return Err(CadenceError::file(
                    CadenceErrorKind::FieldRange,
                    format!("champ {label} `{part}` · borne basse au-dessus de la haute"),
                    format!("`{label}` entre {LO} et {HI}"),
                )
                .on_field(label));
            }
            set_range(&mut out, from, to, step);
        } else if has_step {
            let from = field_value::<LO, HI>(range_text, names, label)?;
            set_range(&mut out, from, HI, step);
        } else {
            let v = field_value::<LO, HI>(range_text, names, label)?;
            out.set(v);
        }
    }
    Ok(out)
}

fn set_range<const LO: u8, const HI: u8>(out: &mut Field<LO, HI>, from: u8, to: u8, step: u8) {
    let mut v = from;
    loop {
        out.set(v);
        let Some(next) = v.checked_add(step) else {
            break;
        };
        if next > to {
            break;
        }
        v = next;
    }
}

fn field_value<const LO: u8, const HI: u8>(
    text: &str,
    names: &[(&str, u8)],
    label: &'static str,
) -> Result<u8, CadenceError> {
    if let Some((_, v)) = names
        .iter()
        .find(|(name, _)| text.eq_ignore_ascii_case(name))
    {
        // A named alias takes the SAME bound as a numeric one. Today every
        // table sits inside its field's range so this changes no behaviour,
        // but the numeric path filtered and this one did not, and the day a
        // table gains an out-of-range entry `Field::set` would shift by
        // `v - LO` on an UNDERFLOWED u8. Found by an adversarial review of
        // the DateImpossible guard, 2026-08-13: not the target, but the mine
        // next to it. An out-of-bound alias now falls through to the numeric
        // path, fails to parse, and refuses with the field's own bounds.
        if *v >= LO && *v <= HI {
            return Ok(*v);
        }
    }
    text.parse::<u8>()
        .ok()
        .filter(|v| *v >= LO && *v <= HI)
        .ok_or_else(|| {
            CadenceError::file(
                CadenceErrorKind::FieldRange,
                format!("champ {label} `{text}` · hors bornes ou illisible"),
                format!("`{label}` entre {LO} et {HI}"),
            )
            .on_field(label)
        })
}
