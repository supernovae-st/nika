// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The systemd half — a `.timer` + `.service` pair per beat. The zone
//! TRAVELS: `OnCalendar=` carries the IANA suffix (systemd.time(7)), so
//! the D10 refusal is launchd's, never this file's.
//!
//! The translation is field by field from the parsed [`CronSpec`] —
//! never a re-parse: each component becomes `*` (a wildcard) or the
//! comma list of its values, zero-padded to systemd's normalized form.
//! `Persistent=` carries the `manqué:` law: anything but `sauter` asks
//! the timer to catch a miss up.

use super::{EmitCtx, Target, Unit, header};
use crate::cron::{CronSpec, Field};
use crate::registry::{Beat, MissPolicy};

/// The weekday names, the grammar's NAMED origin (Sunday = 0).
const DOW_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// `*` for a wildcard, else the zero-padded comma list of the values.
fn list<const LO: u8, const HI: u8>(field: Field<LO, HI>) -> String {
    if field.is_full() {
        "*".to_owned()
    } else {
        field
            .iter()
            .map(|v| format!("{v:02}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// The `OnCalendar=` value: `[<dow> ]*-<months>-<dom> <hours>:<minutes>:00 <tz>`.
/// Seconds are always `00` — the grammar's fields stop at the minute.
fn on_calendar(tz: &str, spec: &CronSpec) -> String {
    let dow = if spec.dow().is_full() {
        String::new()
    } else {
        let days = spec
            .dow()
            .iter()
            .map(|v| DOW_NAMES.get(usize::from(v)).copied().unwrap_or("Sun"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{days} ")
    };
    format!(
        "{dow}*-{}-{} {}:{}:00 {tz}",
        list(*spec.months()),
        list(*spec.dom()),
        list(*spec.hours()),
        list(*spec.minutes())
    )
}

/// The unit-file escape: `%` introduces a systemd specifier, so a
/// literal one doubles (`%%`).
fn esc(text: &str) -> String {
    text.replace('%', "%%")
}

/// An `ExecStart=` token — double-quoted when it carries whitespace.
fn token(text: &str) -> String {
    let escaped = esc(text);
    if escaped.chars().any(char::is_whitespace) {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

/// A beat's pair: the timer (the calendar) and the service (the shot).
/// The env file rides as `EnvironmentFile=` on the SERVICE (D7 — the
/// systemd-native form, no shell wrap here).
pub(crate) fn per_beat(
    ctx: &EmitCtx,
    label: &str,
    beat: &Beat,
    tz: &str,
    spec: &CronSpec,
) -> [Unit; 2] {
    let unit_label = format!("nika.arm.{label}");
    let head = header(ctx, &format!("beat {label}"), Target::SystemdUser);
    let calendar = on_calendar(tz, spec);
    // `manqué: sauter` lets a miss die; anything else catches it up.
    let persistent = !matches!(beat.manque, Some(MissPolicy::Sauter));
    let timer = format!(
        "# {head}\n\
         [Unit]\n\
         Description=nika arm · beat {label} ({})\n\
         \n\
         [Timer]\n\
         OnCalendar={calendar}\n\
         Persistent={persistent}\n\
         \n\
         [Install]\n\
         WantedBy=timers.target\n",
        esc(&beat.workflow)
    );
    let service = service(
        ctx,
        &head,
        &format!("nika arm fire · {label}"),
        &["arm", "fire", label],
        false,
    );
    [
        Unit {
            file_name: format!("{unit_label}.timer"),
            body: timer,
        },
        Unit {
            file_name: format!("{unit_label}.service"),
            body: service,
        },
    ]
}

/// The `nika.serve` service — a simple daemon, re-launched on failure
/// (the serve verb lands in W5; the unit is emitted by name today).
pub(crate) fn serve(ctx: &EmitCtx) -> Unit {
    let head = header(ctx, "serve", Target::SystemdUser);
    Unit {
        file_name: "nika.serve.service".to_owned(),
        body: service(
            ctx,
            &head,
            "nika serve · the machine edge",
            &["serve"],
            true,
        ),
    }
}

/// A `.service` text: `Type=oneshot` for a tick, `Type=simple` +
/// `Restart=on-failure` for the daemon. D2: the words are
/// `arm fire <label>` or `serve` — never `run`.
fn service(ctx: &EmitCtx, head: &str, description: &str, words: &[&str], daemon: bool) -> String {
    let mut exec = token(&ctx.nika_bin.display().to_string());
    for word in words {
        exec.push(' ');
        exec.push_str(&token(word));
    }
    let kind = if daemon {
        "Type=simple\nRestart=on-failure"
    } else {
        "Type=oneshot"
    };
    let env = ctx.env_file.as_ref().map_or(String::new(), |file| {
        format!("EnvironmentFile={}\n", esc(&file.display().to_string()))
    });
    let install = if daemon {
        "\n[Install]\nWantedBy=default.target\n"
    } else {
        ""
    };
    format!(
        "# {head}\n\
         [Unit]\n\
         Description={description}\n\
         \n\
         [Service]\n\
         {kind}\n\
         ExecStart={exec}\n\
         WorkingDirectory={}\n\
         {env}{install}",
        token(&ctx.project_root.display().to_string())
    )
}
