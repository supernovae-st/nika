// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Core builtins (6) — log · emit · assert · prompt · done · wait
//! (stdlib §Core · required for execution). Contracts cited, not restated.

use std::time::Duration;

use nika_kernel::io::clock::ClockDyn;

use crate::{Args, BuiltinFailure, BuiltinOutcome, Emitter, Prompter, opt_str, req_str};

/// `nika:log` — best-effort human diagnostic onto the event stream.
/// A log that cannot land NEVER fails the task (stdlib §log).
#[allow(clippy::unnecessary_wraps)] // uniform BuiltinOutcome · the dispatcher matches all 22 alike
pub(crate) fn log<E: Emitter>(emitter: &E, args: &Args) -> BuiltinOutcome {
    let level = args
        .get("level")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("info");
    let payload = serde_json::json!({
        "level": level,
        "message": args.get("message").cloned().unwrap_or(serde_json::Value::Null),
        "data": args.get("data").cloned().unwrap_or(serde_json::Value::Null),
    });
    emitter.emit("log", payload);
    Ok(serde_json::Value::Null)
}

/// `nika:emit` — a custom machine event (shape-gated · stdlib §emit).
pub(crate) fn emit<E: Emitter>(emitter: &E, args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-EMIT-001";
    let event_type = req_str(args, "event_type", C)?;
    if !is_event_type(event_type) {
        return Err(BuiltinFailure::new(
            C,
            format!("`event_type:` `{event_type}` must match ^[a-z][a-z0-9_.-]*$"),
        ));
    }
    let payload = args
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    emitter.emit(event_type, payload);
    Ok(serde_json::Value::Null)
}

/// The `event_type:` grammar `^[a-z][a-z0-9_.-]*$` (no regex dep — the
/// shape is tiny and this stays a pure char check).
fn is_event_type(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
}

/// `nika:assert` — fail-fast guard. `condition:` arrives CEL-resolved as a
/// boolean; false → `NIKA-BUILTIN-ASSERT-001` (stdlib §assert).
pub(crate) fn assert(args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-ASSERT-001";
    let condition = args
        .get("condition")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| BuiltinFailure::new(C, "`condition:` must resolve to a boolean"))?;
    if condition {
        Ok(serde_json::Value::Bool(true))
    } else {
        let message = opt_str(args, "message", C)?.unwrap_or("assertion failed");
        Err(BuiltinFailure::new(C, message.to_owned()))
    }
}

/// `nika:prompt` — interactive human-in-the-loop · 3 modes · the
/// normative non-interactive contract (stdlib §prompt).
pub(crate) fn prompt<P: Prompter>(prompter: &P, args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-PROMPT-001";
    const C2: &str = "NIKA-BUILTIN-PROMPT-002";
    let mode = opt_str(args, "mode", C1)?.unwrap_or("confirm");
    let message = req_str(args, "message", C1)?;

    match mode {
        "confirm" => match prompter.confirm(message) {
            Some(answer) => Ok(serde_json::Value::Bool(answer)),
            None => default_or_fail(args, C1, |d| d.as_bool().map(serde_json::Value::Bool)),
        },
        "input" => match prompter.input(message) {
            Some(text) => Ok(serde_json::Value::String(text)),
            None => default_or_fail(args, C1, |d| {
                d.as_str().map(|s| serde_json::Value::String(s.to_owned()))
            }),
        },
        "choice" => {
            let choices = string_choices(args, C2)?;
            if let Some(default) = args.get("default").and_then(serde_json::Value::as_str)
                && !choices.iter().any(|c| c == default)
            {
                return Err(BuiltinFailure::new(
                    C2,
                    format!("`default:` `{default}` is not one of `choices:`"),
                ));
            }
            match prompter.choice(message, &choices) {
                Some(chosen) => Ok(serde_json::Value::String(chosen)),
                None => default_or_fail(args, C1, |d| {
                    d.as_str().map(|s| serde_json::Value::String(s.to_owned()))
                }),
            }
        }
        other => Err(BuiltinFailure::new(C1, format!("unknown mode `{other}`"))),
    }
}

/// No human answered: use `default:` when present, else the normative
/// `NIKA-BUILTIN-PROMPT-001` (never hang, never invent · stdlib §prompt).
fn default_or_fail(
    args: &Args,
    code: &'static str,
    pick: impl Fn(&serde_json::Value) -> Option<serde_json::Value>,
) -> BuiltinOutcome {
    match args.get("default").and_then(pick) {
        Some(value) => Ok(value),
        None => Err(BuiltinFailure::new(
            code,
            "non-interactive and no `default:` — cannot answer without a human",
        )),
    }
}

fn string_choices(args: &Args, code: &'static str) -> Result<Vec<String>, BuiltinFailure> {
    let array = args
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| BuiltinFailure::new(code, "`choices:` must be a non-empty array"))?;
    array
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| BuiltinFailure::new(code, "every choice must be a string"))
        })
        .collect()
}

/// `nika:done` — the agent-loop sentinel. Reaching the dispatcher means it
/// was invoked OUTSIDE a loop → `NIKA-BUILTIN-DONE-001` (stdlib §done).
#[allow(clippy::unnecessary_wraps)] // uniform BuiltinOutcome · always the same rejection
pub(crate) fn done() -> BuiltinOutcome {
    Err(BuiltinFailure::new(
        "NIKA-BUILTIN-DONE-001",
        "`nika:done` is valid only inside an `agent:` loop's tool whitelist",
    ))
}

/// `nika:wait` — temporal pause · relative `duration:` XOR absolute
/// `until:` (exactly-one-of · stdlib §wait). Rides the injected clock.
pub(crate) async fn wait<C: ClockDyn>(clock: &C, args: &Args) -> BuiltinOutcome {
    const E1: &str = "NIKA-BUILTIN-WAIT-001";
    const E3: &str = "NIKA-BUILTIN-WAIT-003";
    let duration = opt_str(args, "duration", E3)?;
    let until = opt_str(args, "until", E3)?;
    match (duration, until) {
        (Some(d), None) => {
            let dur = parse_go_duration(d)
                .ok_or_else(|| BuiltinFailure::new(E1, format!("unparseable duration `{d}`")))?;
            clock.sleep(dur).await;
            Ok(serde_json::Value::Null)
        }
        // Absolute wait needs a wall-clock comparison the kernel `ClockDyn`
        // does not yet expose (its `now()` is `Instant`); the L3 scheduler
        // owns absolute waits today. Honest until the seam grows wall-now.
        (None, Some(_)) => Err(BuiltinFailure::new(
            E1,
            "absolute `until:` waits are scheduled by the engine, not this builtin (v0.1)",
        )),
        _ => Err(BuiltinFailure::new(
            E3,
            "exactly one of `duration:` or `until:` is required",
        )),
    }
}

/// Parse a Go-duration string (`"500ms"`/`"30s"`/`"5m"`/`"1h30m"`). Returns
/// `None` on any malformed input (the caller maps it to the spec code).
fn parse_go_duration(input: &str) -> Option<Duration> {
    if input.is_empty() {
        return None;
    }
    let mut total = Duration::ZERO;
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if i == start {
            return None; // a unit with no number
        }
        let value: f64 = input[start..i].parse().ok()?;
        let unit_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_digit() && bytes[i] != b'.' {
            i += 1;
        }
        let nanos = match &input[unit_start..i] {
            "ns" => value,
            "us" | "µs" => value * 1_000.0,
            "ms" => value * 1_000_000.0,
            "s" => value * 1_000_000_000.0,
            "m" => value * 60.0 * 1_000_000_000.0,
            "h" => value * 3_600.0 * 1_000_000_000.0,
            _ => return None,
        };
        // MUTATION (equivalent): the `||` arm and the `!is_finite()` reject
        // states a parsed digit-string can't reach (nanos is non-negative ×
        // positive constants); the `< 0.0` half is pinned by the "0s" test,
        // the rest is defensive depth against a future float path.
        if nanos < 0.0 || !nanos.is_finite() {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // guarded ≥0 + finite above
        let nanos_u64 = nanos as u64;
        total = total.checked_add(Duration::from_nanos(nanos_u64))?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NonInteractive, NullEmitter, Prompter};
    use nika_kernel_mock::MockClock;

    /// A prompter that always answers — exercises the happy paths the
    /// `NonInteractive` default cannot reach.
    struct AlwaysPrompter;
    impl Prompter for AlwaysPrompter {
        fn confirm(&self, _m: &str) -> Option<bool> {
            Some(true)
        }
        fn input(&self, _m: &str) -> Option<String> {
            Some("typed".to_owned())
        }
        fn choice(&self, _m: &str, choices: &[String]) -> Option<String> {
            choices.first().cloned()
        }
    }

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    #[test]
    fn log_is_best_effort_null() {
        let out = log(&NullEmitter, &args(serde_json::json!({ "message": "hi" }))).expect("ok");
        assert_eq!(out, serde_json::Value::Null);
    }

    #[test]
    fn emit_gates_the_event_type_shape() {
        assert!(
            emit(
                &NullEmitter,
                &args(serde_json::json!({ "event_type": "custom.thing" }))
            )
            .is_ok()
        );
        let bad = emit(
            &NullEmitter,
            &args(serde_json::json!({ "event_type": "Bad Type!" })),
        );
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-EMIT-001"));
        assert!(is_event_type("a"));
        assert!(is_event_type("a.b_c-d.1"));
        assert!(!is_event_type(""));
        assert!(!is_event_type("1bad"));
        assert!(!is_event_type("UP"));
    }

    #[test]
    fn assert_passes_true_fails_false() {
        assert_eq!(
            assert(&args(serde_json::json!({ "condition": true }))).expect("ok"),
            serde_json::Value::Bool(true)
        );
        let f = assert(&args(
            serde_json::json!({ "condition": false, "message": "boom" }),
        ));
        assert!(matches!(f, Err(e) if e.message == "boom"));
        // a non-bool condition is a shape failure.
        assert!(assert(&args(serde_json::json!({ "condition": "yes" }))).is_err());
    }

    #[test]
    fn prompt_non_interactive_uses_default_else_fails() {
        let ok = prompt(
            &NonInteractive,
            &args(serde_json::json!({ "message": "ok?", "default": true })),
        )
        .expect("default used");
        assert_eq!(ok, serde_json::Value::Bool(true));

        let fail = prompt(
            &NonInteractive,
            &args(serde_json::json!({ "message": "ok?" })),
        );
        assert!(matches!(fail, Err(f) if f.code == "NIKA-BUILTIN-PROMPT-001"));

        // a real human picks the first choice (exercises string_choices + the choice arm).
        let picked = prompt(
            &AlwaysPrompter,
            &args(serde_json::json!({
                "mode": "choice", "message": "pick", "choices": ["alpha", "beta"]
            })),
        )
        .expect("chosen");
        assert_eq!(picked, serde_json::Value::String("alpha".to_owned()));
        // input mode returns the typed string.
        let typed = prompt(
            &AlwaysPrompter,
            &args(serde_json::json!({ "mode": "input", "message": "?" })),
        )
        .expect("typed");
        assert_eq!(typed, serde_json::Value::String("typed".to_owned()));
        // an empty choices array is PROMPT-002 even with a human.
        let empty = prompt(
            &AlwaysPrompter,
            &args(serde_json::json!({ "mode": "choice", "message": "p", "choices": [] })),
        );
        assert!(matches!(empty, Err(f) if f.code == "NIKA-BUILTIN-PROMPT-002"));

        // choice default not in choices → PROMPT-002.
        let bad = prompt(
            &NonInteractive,
            &args(serde_json::json!({
                "mode": "choice", "message": "pick", "choices": ["a", "b"], "default": "z"
            })),
        );
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-PROMPT-002"));
    }

    #[test]
    fn done_always_rejects() {
        assert!(matches!(done(), Err(f) if f.code == "NIKA-BUILTIN-DONE-001"));
    }

    #[tokio::test]
    async fn wait_relative_sleeps_absolute_deferred_neither_fails() {
        let clock = MockClock::new();
        assert!(
            wait(&clock, &args(serde_json::json!({ "duration": "5ms" })))
                .await
                .is_ok()
        );
        let abs = wait(
            &clock,
            &args(serde_json::json!({ "until": "2026-01-01T00:00:00Z" })),
        )
        .await;
        assert!(matches!(abs, Err(f) if f.code == "NIKA-BUILTIN-WAIT-001"));
        let both = wait(
            &clock,
            &args(serde_json::json!({ "duration": "1s", "until": "x" })),
        )
        .await;
        assert!(matches!(both, Err(f) if f.code == "NIKA-BUILTIN-WAIT-003"));
        let neither = wait(&clock, &args(serde_json::json!({}))).await;
        assert!(matches!(neither, Err(f) if f.code == "NIKA-BUILTIN-WAIT-003"));
    }

    #[test]
    fn go_duration_parser_covers_the_grammar() {
        assert_eq!(parse_go_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_go_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_go_duration("1h30m"), Some(Duration::from_secs(5400)));
        assert_eq!(parse_go_duration("2m15s"), Some(Duration::from_secs(135)));
        assert_eq!(parse_go_duration("250ns"), Some(Duration::from_nanos(250)));
        assert_eq!(parse_go_duration("3us"), Some(Duration::from_micros(3)));
        assert_eq!(parse_go_duration("2h"), Some(Duration::from_secs(7200)));
        assert_eq!(
            parse_go_duration("0s"),
            Some(Duration::ZERO),
            "zero is valid (pins the < 0.0 guard)"
        );
        assert_eq!(parse_go_duration(""), None);
        assert_eq!(parse_go_duration("s"), None, "unit with no number");
        assert_eq!(parse_go_duration("10"), None, "number with no unit");
        assert_eq!(parse_go_duration("5x"), None);
        assert_eq!(parse_go_duration("abc"), None);
    }
}
