// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Time + identity builtins (2) — `nika:date` and `nika:uuid` ride the
//! clock/entropy edges (split from `data.rs` at the 1500-LOC file cap).
//!
//! Every contract — codes · defaults · the exactly-one-output law — is
//! cited from `nika-spec stdlib/builtins-v0.1.md`, never restated.

use nika_kernel::io::clock::ClockDyn;

use crate::{Args, BuiltinFailure, BuiltinOutcome, opt_str, req_str};

// ─── nika:uuid ──────────────────────────────────────────────────────────

/// Generate a UUID (v7 default · sortable · or v4 random).
pub(crate) fn uuid(args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-UUID-001";
    let version = opt_str(args, "version", C)?.unwrap_or("v7");
    let id = match version {
        "v7" => uuid::Uuid::now_v7(),
        "v4" => uuid::Uuid::new_v4(),
        other => {
            return Err(BuiltinFailure::new(
                C,
                format!("`version:` must be v7|v4, got {other}"),
            ));
        }
    };
    Ok(serde_json::Value::String(id.to_string()))
}

// ─── nika:date · timestamp arithmetic (op-discriminated) ────────────────

/// The `nika:date` code (stdlib §date · unparseable input / unknown op /
/// bad tz · `validation_error`).
const DATE_CODE: &str = "NIKA-BUILTIN-DATE-001";

/// `op`-discriminated time builtin — the spec's full six (now · add ·
/// subtract · format · parse · diff). `now` rides the injected
/// [`ClockDyn`] wall clock (`system_now`) for test hermeticity;
/// `format`/`parse` speak the strftime field grammar.
pub(crate) fn date<C: ClockDyn>(clock: &C, args: &Args) -> BuiltinOutcome {
    let op = req_str(args, "op", DATE_CODE)?;
    match op {
        "now" => date_now(clock, args),
        "add" | "subtract" => date_shift(op, args),
        "format" => date_format(args),
        "parse" => date_parse(args),
        "diff" => date_diff(args),
        other => Err(BuiltinFailure::new(
            DATE_CODE,
            format!("unknown op `{other}` (now|add|subtract|format|parse|diff)"),
        )),
    }
}

fn parse_ts(args: &Args, key: &str) -> Result<jiff::Timestamp, BuiltinFailure> {
    req_str(args, key, DATE_CODE)?
        .parse()
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("`{key}:` unparseable: {e}")))
}

/// `op: now { tz }` — the injected wall clock, ISO 8601 out (UTC `Z`
/// form by default · offset form in an IANA `tz:`).
fn date_now<C: ClockDyn>(clock: &C, args: &Args) -> BuiltinOutcome {
    let now = jiff::Timestamp::try_from(clock.system_now())
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("clock out of range: {e}")))?;
    match opt_str(args, "tz", DATE_CODE)? {
        None => Ok(serde_json::Value::String(now.to_string())),
        Some(tz) => {
            let zoned = now
                .in_tz(tz)
                .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("bad `tz:` `{tz}`: {e}")))?;
            let text = jiff::fmt::strtime::format("%Y-%m-%dT%H:%M:%S%.f%:z", &zoned)
                .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("render failed: {e}")))?;
            Ok(serde_json::Value::String(text))
        }
    }
}

/// `op: add|subtract { base, duration, tz }` — ISO 8601 span arithmetic.
/// The span is applied through a [`jiff::Zoned`] (the `tz:` arg · default
/// UTC) rather than the bare [`jiff::Timestamp`]: a `Timestamp` has no
/// calendar/zone context, so its arithmetic only supports units ≤ hours
/// and rejects weeks/days/months/years. Routing through `Zoned` makes BOTH
/// clock and calendar units work and is DST-aware (`add 2 weeks` lands on
/// the civil-correct instant across a DST boundary). The result converts
/// back to a `Timestamp` for the canonical ISO 8601 (`Z`) output.
fn date_shift(op: &str, args: &Args) -> BuiltinOutcome {
    let zoned = date_shift_base_zoned(args)?;
    let span: jiff::Span = req_str(args, "duration", DATE_CODE)?
        .parse()
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("`duration:` unparseable: {e}")))?;
    let out = if op == "add" {
        zoned.checked_add(span)
    } else {
        zoned.checked_sub(span)
    }
    .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("{op} overflow: {e}")))?;
    Ok(serde_json::Value::String(out.timestamp().to_string()))
}

/// The `base:` instant zoned for calendar-aware shifting — `tz:` (IANA ·
/// default UTC), mirroring [`date_format`]'s zone resolution.
fn date_shift_base_zoned(args: &Args) -> Result<jiff::Zoned, BuiltinFailure> {
    let ts = parse_ts(args, "base")?;
    match opt_str(args, "tz", DATE_CODE)? {
        None => Ok(ts.to_zoned(jiff::tz::TimeZone::UTC)),
        Some(tz) => ts
            .in_tz(tz)
            .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("bad `tz:` `{tz}`: {e}"))),
    }
}

/// `op: format { input, format, tz }` — render an instant through the
/// strftime grammar (`%Y-%m-%d`). Fields render in `tz:` (IANA ·
/// default UTC) — the `ToolDef` declared `tz:` all along; the impl
/// silently hardcoded UTC (a Paris-display request got UTC fields with
/// no error · the ambition-audit #5 fix).
fn date_format(args: &Args) -> BuiltinOutcome {
    let ts = parse_ts(args, "input")?;
    let fmt = req_str(args, "format", DATE_CODE)?;
    let zoned = match opt_str(args, "tz", DATE_CODE)? {
        None => ts.to_zoned(jiff::tz::TimeZone::UTC),
        Some(tz) => ts
            .in_tz(tz)
            .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("bad `tz:` `{tz}`: {e}")))?,
    };
    let text = jiff::fmt::strtime::format(fmt, &zoned)
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("`format:` failed: {e}")))?;
    Ok(serde_json::Value::String(text))
}

/// `op: parse { input, format }` — strftime → ISO 8601 instant. An
/// input that carries no offset is read as UTC (the spec default tz).
fn date_parse(args: &Args) -> BuiltinOutcome {
    let input = req_str(args, "input", DATE_CODE)?;
    let fmt = req_str(args, "format", DATE_CODE)?;
    let broken = jiff::fmt::strtime::parse(fmt, input)
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("`parse` failed: {e}")))?;
    let ts = broken.to_timestamp().or_else(|_| {
        broken
            .to_datetime()
            .and_then(|dt| dt.to_zoned(jiff::tz::TimeZone::UTC))
            .map(|z| z.timestamp())
    });
    let ts =
        ts.map_err(|e| BuiltinFailure::new(DATE_CODE, format!("parsed fields incomplete: {e}")))?;
    Ok(serde_json::Value::String(ts.to_string()))
}

/// `op: diff { start, end, unit }` — an integer in `unit:` (seconds
/// default · negative when `end` precedes `start`).
fn date_diff(args: &Args) -> BuiltinOutcome {
    let start = parse_ts(args, "start")?;
    let end = parse_ts(args, "end")?;
    let dur = end.duration_since(start);
    let unit = opt_str(args, "unit", DATE_CODE)?.unwrap_or("seconds");
    let value = match unit {
        "seconds" => dur.as_secs(),
        "milliseconds" => i64::try_from(dur.as_millis())
            .map_err(|_| BuiltinFailure::new(DATE_CODE, "diff out of i64 millisecond range"))?,
        "minutes" => dur.as_secs() / 60,
        "hours" => dur.as_secs() / 3600,
        "days" => dur.as_secs() / 86_400,
        // weeks is a fixed 7-day span (a calendar-independent unit · like
        // days). months/years are deliberately absent — they are not a
        // fixed Duration (a reference instant decides their length), so
        // diff cannot answer them; `add`/`subtract` take ISO 8601 P1M/P1Y.
        "weeks" => dur.as_secs() / 604_800,
        other => {
            return Err(BuiltinFailure::new(
                DATE_CODE,
                format!("unknown unit `{other}` (seconds|milliseconds|minutes|hours|days|weeks)"),
            ));
        }
    };
    Ok(serde_json::Value::Number(value.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    #[test]
    fn uuid_format_and_version() {
        let v7 = uuid(&args(serde_json::json!({}))).expect("ok");
        let s = v7.as_str().expect("string");
        assert_eq!(s.len(), 36, "canonical hyphenated");
        assert_eq!(&s[14..15], "7", "version nibble");
        let v4 = uuid(&args(serde_json::json!({ "version": "v4" }))).expect("ok");
        assert_eq!(&v4.as_str().expect("s")[14..15], "4");
        assert!(uuid(&args(serde_json::json!({ "version": "v9" }))).is_err());
    }

    #[test]
    fn date_add_and_diff_are_deterministic() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let added = date(
            &clock,
            &args(serde_json::json!({
                "op": "add", "base": "2026-01-01T00:00:00Z", "duration": "PT1h"
            })),
        )
        .expect("ok");
        assert_eq!(added.as_str().expect("s"), "2026-01-01T01:00:00Z");

        let diff = date(
            &clock,
            &args(serde_json::json!({
                "op": "diff", "start": "2026-01-01T00:00:00Z", "end": "2026-01-01T00:01:00Z"
            })),
        )
        .expect("ok");
        assert_eq!(diff, serde_json::json!(60));

        assert!(date(&clock, &args(serde_json::json!({ "op": "warp" }))).is_err());
    }

    #[test]
    fn date_now_rides_the_injected_clock() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let parse = |v: serde_json::Value| -> jiff::Timestamp {
            v.as_str().expect("string").parse().expect("ISO 8601")
        };
        let t0 = parse(date(&clock, &args(serde_json::json!({ "op": "now" }))).expect("ok"));
        clock.advance(std::time::Duration::from_secs(3600));
        let t1 = parse(date(&clock, &args(serde_json::json!({ "op": "now" }))).expect("ok"));
        // The mock clock IS the time source — exactly the advanced hour.
        assert_eq!(t1.duration_since(t0).as_secs(), 3600);

        // tz renders the offset form (fixed-offset zone = deterministic).
        let zoned = date(
            &clock,
            &args(serde_json::json!({ "op": "now", "tz": "Etc/GMT-2" })),
        )
        .expect("ok");
        assert!(zoned.as_str().expect("s").ends_with("+02:00"), "{zoned}");
        let bad_tz = date(
            &clock,
            &args(serde_json::json!({ "op": "now", "tz": "Mars/Olympus" })),
        );
        assert!(matches!(bad_tz, Err(f) if f.code == "NIKA-BUILTIN-DATE-001"));
    }

    #[test]
    fn date_format_and_parse_speak_strftime() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let formatted = date(
            &clock,
            &args(serde_json::json!({
                "op": "format", "input": "2026-01-02T03:04:05Z", "format": "%Y-%m-%d %H:%M"
            })),
        )
        .expect("ok");
        assert_eq!(formatted, serde_json::json!("2026-01-02 03:04"));

        // parse without an offset reads as UTC (spec default tz).
        let parsed = date(
            &clock,
            &args(serde_json::json!({
                "op": "parse", "input": "2026-01-02", "format": "%Y-%m-%d"
            })),
        )
        .expect("ok");
        assert_eq!(parsed, serde_json::json!("2026-01-02T00:00:00Z"));

        // parse WITH an offset is exact-instant.
        let offset = date(
            &clock,
            &args(serde_json::json!({
                "op": "parse", "input": "2026-01-02 03:00 +0200", "format": "%Y-%m-%d %H:%M %z"
            })),
        )
        .expect("ok");
        assert_eq!(offset, serde_json::json!("2026-01-02T01:00:00Z"));

        // format honors `tz:` (the ToolDef declared it all along — the
        // impl hardcoded UTC silently · ambition-audit #5). A fixed-
        // offset zone keeps the pin deterministic.
        let paris_ish = date(
            &clock,
            &args(serde_json::json!({
                "op": "format", "input": "2026-01-02T03:04:05Z",
                "format": "%Y-%m-%d %H:%M", "tz": "Etc/GMT-2"
            })),
        )
        .expect("ok");
        assert_eq!(paris_ish, serde_json::json!("2026-01-02 05:04"));
        let bad_tz = date(
            &clock,
            &args(serde_json::json!({
                "op": "format", "input": "2026-01-02T03:04:05Z",
                "format": "%H", "tz": "Mars/Olympus"
            })),
        );
        assert!(
            matches!(&bad_tz, Err(f) if f.code == "NIKA-BUILTIN-DATE-001"),
            "{bad_tz:?}"
        );

        let bad = date(
            &clock,
            &args(serde_json::json!({
                "op": "parse", "input": "abc", "format": "%Y-%m-%d"
            })),
        );
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-DATE-001"));
    }

    #[test]
    fn date_diff_units_are_the_closed_set() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let diff_in = |unit: &str| {
            date(
                &clock,
                &args(serde_json::json!({
                    "op": "diff", "start": "2026-01-01T00:00:00Z",
                    "end": "2026-01-02T01:30:00Z", "unit": unit
                })),
            )
        };
        assert_eq!(diff_in("seconds").expect("ok"), serde_json::json!(91_800));
        assert_eq!(
            diff_in("milliseconds").expect("ok"),
            serde_json::json!(91_800_000)
        );
        assert_eq!(diff_in("minutes").expect("ok"), serde_json::json!(1530));
        assert_eq!(diff_in("hours").expect("ok"), serde_json::json!(25));
        assert_eq!(diff_in("days").expect("ok"), serde_json::json!(1));
        // weeks is a fixed 7-day span (this 25h30m fixture floors to 0).
        assert_eq!(diff_in("weeks").expect("ok"), serde_json::json!(0));
        assert!(diff_in("fortnights").is_err());
        // A genuine multi-week span floors to whole weeks.
        let three_weeks = date(
            &clock,
            &args(serde_json::json!({
                "op": "diff", "start": "2026-01-01T00:00:00Z",
                "end": "2026-01-23T00:00:00Z", "unit": "weeks"
            })),
        )
        .expect("ok");
        assert_eq!(three_weeks, serde_json::json!(3), "22 days = 3 whole weeks");
        // Negative when end precedes start (signed integer semantics).
        let negative = date(
            &clock,
            &args(serde_json::json!({
                "op": "diff", "start": "2026-01-02T00:00:00Z", "end": "2026-01-01T00:00:00Z"
            })),
        )
        .expect("ok");
        assert_eq!(negative, serde_json::json!(-86_400));
    }

    #[test]
    fn date_shift_handles_calendar_units() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let shift = |op: &str, duration: &str| -> serde_json::Value {
            date(
                &clock,
                &args(serde_json::json!({
                    "op": op, "base": "2026-01-01T00:00:00Z", "duration": duration
                })),
            )
            .expect("ok")
        };
        // Calendar units (the bug): a bare Timestamp rejected these because
        // weeks/days/months/years have no fixed length without a zone.
        // Routing through a Zoned makes them all land.
        assert_eq!(
            shift("add", "2 weeks"),
            serde_json::json!("2026-01-15T00:00:00Z")
        );
        assert_eq!(
            shift("add", "14 days"),
            serde_json::json!("2026-01-15T00:00:00Z")
        );
        assert_eq!(
            shift("add", "1 month"),
            serde_json::json!("2026-02-01T00:00:00Z")
        );
        // Mixed calendar + clock units.
        assert_eq!(
            shift("add", "1 day 2 hours"),
            serde_json::json!("2026-01-02T02:00:00Z")
        );
        // Clock-only still works (the path that worked before the fix).
        assert_eq!(
            shift("add", "48 hours"),
            serde_json::json!("2026-01-03T00:00:00Z")
        );
        assert_eq!(
            shift("add", "PT1h"),
            serde_json::json!("2026-01-01T01:00:00Z")
        );
        // Subtract symmetry — add then subtract the same span round-trips.
        assert_eq!(
            shift("subtract", "1 month"),
            serde_json::json!("2025-12-01T00:00:00Z")
        );
        assert_eq!(
            shift("subtract", "2 weeks"),
            serde_json::json!("2025-12-18T00:00:00Z")
        );
    }

    #[test]
    fn date_shift_is_dst_aware_in_a_tz() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        // 2026-03-08T05:00:00Z is 2026-03-08T00:00:00-05:00 in New York,
        // hours before the spring-forward (DST starts 02:00 local that day).
        // Adding ONE CALENDAR DAY lands on the next civil midnight, now in
        // EDT (-04:00) → the instant is only 23h later: DST-correct.
        let civil = date(
            &clock,
            &args(serde_json::json!({
                "op": "add", "base": "2026-03-08T05:00:00Z",
                "duration": "1 day", "tz": "America/New_York"
            })),
        )
        .expect("ok");
        assert_eq!(civil, serde_json::json!("2026-03-09T04:00:00Z"));
        // Contrast: a fixed 24-CLOCK-HOUR span is a flat instant offset
        // (DST-blind) → 24h later exactly. Proves the Zoned path is doing
        // the calendar work, not measuring a flat duration.
        let clockwise = date(
            &clock,
            &args(serde_json::json!({
                "op": "add", "base": "2026-03-08T05:00:00Z",
                "duration": "24 hours", "tz": "America/New_York"
            })),
        )
        .expect("ok");
        assert_eq!(clockwise, serde_json::json!("2026-03-09T05:00:00Z"));
        // A bad tz: surfaces the canonical date code (not a silent UTC fallback).
        let bad_tz = date(
            &clock,
            &args(serde_json::json!({
                "op": "add", "base": "2026-03-08T05:00:00Z",
                "duration": "1 day", "tz": "Mars/Olympus"
            })),
        );
        assert!(
            matches!(&bad_tz, Err(f) if f.code == "NIKA-BUILTIN-DATE-001"),
            "{bad_tz:?}"
        );
    }
}
