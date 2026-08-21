// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The launchd half — one plist per beat, `StartCalendarInterval` as
//! the cartesian product of the cadence's RESTRICTED fields (a wildcard
//! is omitted: launchd reads an absent key as « every », so `* * * * *`
//! is one empty dict, firing every minute).
//!
//! The budget law: the product is COUNTED before it is built — a
//! cadence past [`MAX_INTERVALS`] refuses with its count, it never
//! allocates it. Weekday origin: launchd's `0` (and `7`) is Sunday, the
//! same origin the grammar NAMES.

use std::fmt::Write as _;

use super::{EmitCtx, EmitRefusal, MAX_INTERVALS, Target, Unit, header, sh_quote, xml_escape};
use crate::cron::{CronSpec, Field};

/// One interval dict: a component is present only when its field is
/// restricted (`None` = the wildcard, omitted from the XML).
struct Interval {
    minute: Option<u8>,
    hour: Option<u8>,
    day: Option<u8>,
    month: Option<u8>,
    weekday: Option<u8>,
}

/// How many dicts the product would write (a full field contributes a
/// factor of 1 — it is omitted, not expanded).
fn dict_count(spec: &CronSpec) -> usize {
    fn restricted<const LO: u8, const HI: u8>(field: Field<LO, HI>) -> usize {
        if field.is_full() {
            1
        } else {
            usize::try_from(field.len()).unwrap_or(0)
        }
    }
    restricted(*spec.minutes())
        .saturating_mul(restricted(*spec.hours()))
        .saturating_mul(restricted(*spec.dom()))
        .saturating_mul(restricted(*spec.months()))
        .saturating_mul(restricted(*spec.dow()))
}

/// The restricted values of one field (`[None]` for a wildcard).
fn values<const LO: u8, const HI: u8>(field: Field<LO, HI>) -> Vec<Option<u8>> {
    if field.is_full() {
        vec![None]
    } else {
        field.iter().map(Some).collect()
    }
}

/// The cartesian product, months outermost and minutes innermost — the
/// dicts read in the order a calendar would say them.
fn intervals(spec: &CronSpec) -> Vec<Interval> {
    let months = values(*spec.months());
    let days = values(*spec.dom());
    let weekdays = values(*spec.dow());
    let hours = values(*spec.hours());
    let minutes = values(*spec.minutes());
    let mut out = Vec::new();
    for &month in &months {
        for &day in &days {
            for &weekday in &weekdays {
                for &hour in &hours {
                    for &minute in &minutes {
                        out.push(Interval {
                            minute,
                            hour,
                            day,
                            month,
                            weekday,
                        });
                    }
                }
            }
        }
    }
    out
}

/// A beat's plist — one unit, the interval budget enforced.
pub(crate) fn per_beat(ctx: &EmitCtx, label: &str, spec: &CronSpec) -> Result<Unit, EmitRefusal> {
    let count = dict_count(spec);
    if count > MAX_INTERVALS {
        return Err(EmitRefusal::TooManyIntervals {
            beat: label.to_owned(),
            n: count,
        });
    }
    let unit_label = format!("nika.arm.{label}");
    let words = ["arm", "fire", label];
    let intervals = intervals(spec);
    Ok(Unit {
        file_name: format!("{unit_label}.plist"),
        body: plist(
            ctx,
            &unit_label,
            &format!("beat {label}"),
            &words,
            Some(&intervals),
            false,
            label,
        ),
    })
}

/// The `nika.serve` plist — the daemon lands in W5, the unit is
/// emitted by name today (`KeepAlive`: launchd re-launches it).
pub(crate) fn serve(ctx: &EmitCtx) -> Unit {
    Unit {
        file_name: "nika.serve.plist".to_owned(),
        body: plist(
            ctx,
            "nika.serve",
            "serve",
            &["serve"],
            None,
            true,
            "nika.serve",
        ),
    }
}

/// The plist text. `words` are the nika CLI words after the binary
/// (D2: `arm fire <label>` · `serve` — never `run`).
fn plist(
    ctx: &EmitCtx,
    unit_label: &str,
    what: &str,
    words: &[&str],
    intervals: Option<&[Interval]>,
    keep_alive: bool,
    log_name: &str,
) -> String {
    let mut body = String::new();
    let _ = writeln!(body, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(
        body,
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">"
    );
    let _ = writeln!(body, "<!-- {} -->", header(ctx, what, Target::Launchd));
    let _ = writeln!(body, "<plist version=\"1.0\">");
    let _ = writeln!(body, "<dict>");
    let _ = writeln!(body, "\t<key>Label</key>");
    let _ = writeln!(body, "\t<string>{}</string>", xml_escape(unit_label));
    let _ = writeln!(body, "\t<key>ProgramArguments</key>");
    let _ = writeln!(body, "\t<array>");
    for arg in program_args(ctx, words) {
        let _ = writeln!(body, "\t\t<string>{}</string>", xml_escape(&arg));
    }
    let _ = writeln!(body, "\t</array>");
    let _ = writeln!(body, "\t<key>WorkingDirectory</key>");
    let _ = writeln!(
        body,
        "\t<string>{}</string>",
        xml_escape(&ctx.project_root.display().to_string())
    );
    if let Some(intervals) = intervals {
        let _ = writeln!(body, "\t<key>StartCalendarInterval</key>");
        let _ = writeln!(body, "\t<array>");
        for interval in intervals {
            interval_xml(&mut body, interval);
        }
        let _ = writeln!(body, "\t</array>");
    }
    if keep_alive {
        let _ = writeln!(body, "\t<key>KeepAlive</key>");
        let _ = writeln!(body, "\t<true/>");
    }
    let _ = writeln!(body, "\t<key>StandardOutPath</key>");
    let _ = writeln!(
        body,
        "\t<string>{}</string>",
        xml_escape(&format!("{}/{log_name}.out", ctx.log_dir.display()))
    );
    let _ = writeln!(body, "\t<key>StandardErrorPath</key>");
    let _ = writeln!(
        body,
        "\t<string>{}</string>",
        xml_escape(&format!("{}/{log_name}.err", ctx.log_dir.display()))
    );
    let _ = writeln!(body, "</dict>");
    let _ = writeln!(body, "</plist>");
    body
}

/// The argv the unit runs — D2's one firer, D7's env wrap. Without an
/// env file: the binary + the words. With one: a `/bin/sh -c` wrapper
/// sources the file and `exec`s — the unit names the env PATH only.
fn program_args(ctx: &EmitCtx, words: &[&str]) -> Vec<String> {
    let bin = ctx.nika_bin.display().to_string();
    match &ctx.env_file {
        None => std::iter::once(bin)
            .chain(words.iter().map(|w| (*w).to_owned()))
            .collect(),
        Some(env) => {
            let quoted: Vec<String> = words.iter().map(|w| sh_quote(w)).collect();
            vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                format!(
                    ". {} && exec {} {}",
                    sh_quote(&env.display().to_string()),
                    sh_quote(&bin),
                    quoted.join(" ")
                ),
            ]
        }
    }
}

/// One interval dict — the keys in alphabetical order (the plist
/// canonical form), a wildcard simply absent. The all-wildcard beat is
/// one EMPTY dict (« every minute »).
fn interval_xml(body: &mut String, interval: &Interval) {
    let entries = [
        ("Day", interval.day),
        ("Hour", interval.hour),
        ("Minute", interval.minute),
        ("Month", interval.month),
        ("Weekday", interval.weekday),
    ];
    if entries.iter().all(|(_, v)| v.is_none()) {
        let _ = writeln!(body, "\t\t<dict/>");
        return;
    }
    let _ = writeln!(body, "\t\t<dict>");
    for (key, value) in entries {
        if let Some(value) = value {
            let _ = writeln!(body, "\t\t\t<key>{key}</key>");
            let _ = writeln!(body, "\t\t\t<integer>{value}</integer>");
        }
    }
    let _ = writeln!(body, "\t\t</dict>");
}
