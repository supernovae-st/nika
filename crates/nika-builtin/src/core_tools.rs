// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Core builtins (6) — log · emit · assert · prompt · done · wait
//! (stdlib §Core · required for execution). Contracts cited, not restated.

use std::time::Duration;

use nika_kernel::io::clock::ClockDyn;

use crate::{Args, BuiltinFailure, BuiltinOutcome, Emitter, Prompter, opt_str, req_str};

/// `nika:log` — best-effort human diagnostic onto the event stream.
/// A log that cannot land NEVER fails the task (stdlib §log).
#[allow(clippy::unnecessary_wraps)] // uniform BuiltinOutcome · the dispatcher matches all 23 alike
pub(crate) fn log<E: Emitter>(emitter: &E, args: &Args) -> BuiltinOutcome {
    // The level NORMALIZES to the spec enum (debug|info|warn|error ·
    // stdlib §log) — log never fails, so an out-of-enum level clamps to
    // `info` instead of leaking a non-canonical level onto the journal
    // (downstream aggregators key on the closed set).
    let level = args
        .get("level")
        .and_then(serde_json::Value::as_str)
        .filter(|l| matches!(*l, "debug" | "info" | "warn" | "error"))
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
            // The remedy NAMES the right fix (F6): a string `default:` is
            // the `input` mode's contract — `confirm` wants a yes/no. The
            // prior message said only "must be a boolean for this mode",
            // which sent the author to fix the default when the real fix is
            // `mode: input` (the misguided-DX footgun).
            None => default_or_fail(
                args,
                C1,
                "a boolean",
                "use `mode: input` for a string default, or give a boolean `default:` for confirm",
                |d| d.as_bool().map(serde_json::Value::Bool),
            ),
        },
        "input" => match prompter.input(message) {
            Some(text) => Ok(serde_json::Value::String(text)),
            None => default_or_fail(
                args,
                C1,
                "a string",
                "give a string `default:` (input mode answers with text)",
                |d| d.as_str().map(|s| serde_json::Value::String(s.to_owned())),
            ),
        },
        "choice" => {
            let choices = string_choices(args, C2)?;
            // `default:` (if present) is a PARSE-TIME contract: it MUST be a
            // string that is an element of `choices:` (stdlib §prompt ·
            // PROMPT-002). A wrong-TYPE default (a number/bool) is ALSO
            // -002, NOT a silent "no default" — the prior `and_then(as_str)`
            // skipped the check on a non-string and then lied downstream
            // ("no default" while a default was present).
            if let Some(default) = args.get("default") {
                let is_valid_member = default
                    .as_str()
                    .is_some_and(|d| choices.iter().any(|c| c == d));
                if !is_valid_member {
                    return Err(BuiltinFailure::new(
                        C2,
                        format!("`default:` must be one of `choices:` (got {default})"),
                    ));
                }
            }
            match prompter.choice(message, &choices) {
                Some(chosen) => Ok(serde_json::Value::String(chosen)),
                None => default_or_fail(
                    args,
                    C1,
                    "a string",
                    "give a `default:` that is one of `choices:`",
                    |d| d.as_str().map(|s| serde_json::Value::String(s.to_owned())),
                ),
            }
        }
        other => Err(BuiltinFailure::new(C1, format!("unknown mode `{other}`"))),
    }
}

/// No human answered: use `default:` when present, else the normative
/// `NIKA-BUILTIN-PROMPT-001` (never hang, never invent · stdlib §prompt).
/// Distinguishes a PRESENT-but-wrong-type default (its own honest message)
/// from an ABSENT one — the prior code conflated them and reported "no
/// `default:`" when a wrong-typed default WAS present (a silent lie that
/// gave a model-repair loop nothing to repair).
fn default_or_fail(
    args: &Args,
    code: &'static str,
    type_hint: &str,
    remedy: &str,
    pick: impl Fn(&serde_json::Value) -> Option<serde_json::Value>,
) -> BuiltinOutcome {
    match args.get("default") {
        Some(raw) => pick(raw).ok_or_else(|| {
            BuiltinFailure::new(
                code,
                format!("`default:` must be {type_hint} for this mode (got {raw}) — {remedy}"),
            )
        }),
        // Teach EVERY exit from the headless dead-end (stateful gauntlet
        // 2026-07-11 · re-taught 2026-07-31): the stdlib contract fails
        // here by design (never hang, never invent) — and the reference
        // CLI converts this branch to a durable pause on every lane
        // (ADR-099 rider), so the old "run with `--json`" recipe is
        // retired: the message names the routes that ship.
        None => Err(BuiltinFailure::new(
            code,
            "non-interactive and no `default:` — cannot answer without a human · \
             pre-answer with `--answer <task>=<value>` · declare `default:` \
             (unattended runs answer with it) · or let the run pause durably \
             and resume with `--resume <trace> --answer <task>=<value>`",
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

/// `nika:compose` — the agent loop's self-verification intrinsic. Like
/// `done`, it is LOOP-SERVED: the agent loop runs the static check and
/// feeds the verdict back; a standalone `invoke: nika:compose` has no
/// draft-repair loop, so it is rejected here → `NIKA-BUILTIN-COMPOSE-001`
/// (stdlib §compose · ADR-096). The model-facing def lives in `defs.rs`.
#[allow(clippy::unnecessary_wraps)] // uniform BuiltinOutcome · always the same rejection
pub(crate) fn compose() -> BuiltinOutcome {
    Err(BuiltinFailure::new(
        "NIKA-BUILTIN-COMPOSE-001",
        "`nika:compose` is valid only inside an `agent:` loop's tool whitelist",
    ))
}

/// `nika:wait` — temporal pause · relative `duration:` XOR absolute
/// `until:` (exactly-one-of · stdlib §wait). Both modes ride the
/// injected clock (`sleep` + `system_now` for the absolute comparison),
/// so waits are hermetic under a mock. Spec codes: `-001` absolute
/// timeout / invalid input · `-002` timestamp in the past · `-003`
/// exactly-one-of violated.
pub(crate) async fn wait<C: ClockDyn>(clock: &C, args: &Args) -> BuiltinOutcome {
    const E1: &str = "NIKA-BUILTIN-WAIT-001";
    const E2: &str = "NIKA-BUILTIN-WAIT-002";
    const E3: &str = "NIKA-BUILTIN-WAIT-003";
    // Wrong-TYPE values are input errors (E1) — E3 is strictly the
    // exactly-one-of contract.
    let duration = opt_str(args, "duration", E1)?;
    let until = opt_str(args, "until", E1)?;
    match (duration, until) {
        (Some(d), None) => {
            // `timeout:` is "cap for absolute wait (until: only)" (stdlib
            // §wait) — pairing it with `duration:` was silently IGNORED
            // before (the author believed something was capped). Loud
            // reject teaches the contract instead.
            if args.contains_key("timeout") {
                return Err(BuiltinFailure::new(
                    E1,
                    "`timeout:` caps ABSOLUTE waits only (`until:`) — a relative \
                     `duration:` is its own bound",
                ));
            }
            let dur = parse_go_duration(d)
                .ok_or_else(|| BuiltinFailure::new(E1, format!("unparseable duration `{d}`")))?;
            clock.sleep(dur).await;
            Ok(serde_json::Value::Null)
        }
        (None, Some(u)) => {
            let target: jiff::Timestamp = u
                .parse()
                .map_err(|e| BuiltinFailure::new(E1, format!("unparseable `until:` `{u}`: {e}")))?;
            let now = jiff::Timestamp::try_from(clock.system_now())
                .map_err(|e| BuiltinFailure::new(E1, format!("clock out of range: {e}")))?;
            if target <= now {
                return Err(BuiltinFailure::new(
                    E2,
                    format!("`until:` {u} is in the past (now: {now})"),
                ));
            }
            let delta = Duration::try_from(target.duration_since(now))
                .map_err(|e| BuiltinFailure::new(E1, format!("wait span out of range: {e}")))?;
            if let Some(t) = opt_str(args, "timeout", E1)? {
                let cap = parse_go_duration(t)
                    .ok_or_else(|| BuiltinFailure::new(E1, format!("unparseable timeout `{t}`")))?;
                if delta > cap {
                    return Err(BuiltinFailure::new(
                        E1,
                        format!("absolute wait ({delta:?}) exceeds `timeout:` {t}"),
                    ));
                }
            }
            clock.sleep(delta).await;
            Ok(serde_json::Value::Null)
        }
        _ => Err(BuiltinFailure::new(
            E3,
            "set exactly one of `duration:` (relative · e.g. \"30s\") or `until:` \
             (absolute · ISO 8601) — not zero, not both",
        )),
    }
}

/// Per-segment magnitude ceiling: 2^63 ns ≈ 292 years — anything past it
/// is a typo, not a wait. The half-open range check also rejects NaN/inf
/// and negatives, and it makes the `as u64` cast exact-safe (no silent
/// saturation on a "999…9h" digit wall).
const MAX_SEGMENT_NANOS: f64 = 9_223_372_036_854_775_808.0;

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
        if !(0.0..MAX_SEGMENT_NANOS).contains(&nanos) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // guarded 0 ≤ nanos < 2^63 above
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
    fn log_level_clamps_to_the_spec_enum() {
        // A recording emitter — the level that actually lands on the
        // stream is the contract, not the fn's return value.
        #[derive(Default)]
        struct Capture(std::sync::Mutex<Vec<serde_json::Value>>);
        impl Emitter for Capture {
            fn emit(&self, _kind: &str, payload: serde_json::Value) {
                self.0.lock().expect("lock").push(payload);
            }
        }
        let cap = Capture::default();
        // In-enum levels pass through; out-of-enum (typo · wrong case ·
        // wrong type) clamp to info — log never fails, and the journal
        // only ever carries the closed set (stdlib §log).
        for (given, expect) in [
            (serde_json::json!("debug"), "debug"),
            (serde_json::json!("error"), "error"),
            (serde_json::json!("DEBUG"), "info"),
            (serde_json::json!("catastrophic"), "info"),
            (serde_json::json!(5), "info"),
        ] {
            log(
                &cap,
                &args(serde_json::json!({ "level": given, "message": "x" })),
            )
            .expect("never fails");
            let last = cap
                .0
                .lock()
                .expect("lock")
                .last()
                .cloned()
                .expect("emitted");
            assert_eq!(last["level"], expect, "given {given}");
        }
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
        // The headless dead-end teaches BOTH exits (stateful gauntlet
        // 2026-07-11): declare `default:` OR run on a machine surface that
        // pauses durably (`--resume … --answer`).
        assert!(matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-PROMPT-001"));
        let msg = match fail {
            Err(f) => f.message,
            Ok(_) => unreachable!("headless prompt without default must fail"),
        };
        assert!(msg.contains("declare `default:`"), "{msg}");
        assert!(
            msg.contains("--resume") && msg.contains("--answer"),
            "{msg}"
        );

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
    fn prompt_wrong_type_default_is_not_a_silent_no_default_lie() {
        // The fixed silent bug: a PRESENT-but-wrong-type default was
        // conflated with an ABSENT one. Each mode now reports honestly.

        // choice · a NON-STRING default is a parse error (PROMPT-002),
        // not the old lying PROMPT-001 "no default" (a default WAS present).
        let choice_num = prompt(
            &NonInteractive,
            &args(serde_json::json!({
                "mode": "choice", "message": "pick", "choices": ["a", "b"], "default": 5
            })),
        );
        assert!(
            matches!(&choice_num, Err(f) if f.code == "NIKA-BUILTIN-PROMPT-002"
                && f.message.contains("must be one of")),
            "{choice_num:?}"
        );

        // confirm · a string default (wrong type for a yes/no) names the
        // type AND the right remedy (`mode: input` · F6) instead of just
        // telling the author to fix a default that confirm can't use.
        let confirm_str = prompt(
            &NonInteractive,
            &args(serde_json::json!({ "message": "ok?", "default": "yes" })),
        );
        assert!(
            matches!(&confirm_str, Err(f) if f.code == "NIKA-BUILTIN-PROMPT-001"
                && f.message.contains("must be a boolean")
                && f.message.contains("mode: input")),
            "the confirm-mode error names `mode: input` as the fix: {confirm_str:?}"
        );

        // input · a non-string default likewise names the type.
        let input_num = prompt(
            &NonInteractive,
            &args(serde_json::json!({ "mode": "input", "message": "?", "default": 42 })),
        );
        assert!(
            matches!(&input_num, Err(f) if f.code == "NIKA-BUILTIN-PROMPT-001"
                && f.message.contains("must be a string")),
            "{input_num:?}"
        );

        // And a CORRECT-type default still works (no regression).
        let ok = prompt(
            &NonInteractive,
            &args(serde_json::json!({
                "mode": "choice", "message": "pick", "choices": ["a", "b"], "default": "a"
            })),
        )
        .expect("valid default used");
        assert_eq!(ok, serde_json::Value::String("a".to_owned()));
    }

    #[test]
    fn done_always_rejects() {
        assert!(matches!(done(), Err(f) if f.code == "NIKA-BUILTIN-DONE-001"));
        assert!(matches!(compose(), Err(f) if f.code == "NIKA-BUILTIN-COMPOSE-001"));
    }

    #[tokio::test]
    async fn wait_relative_sleeps_and_xor_is_enforced() {
        let clock = MockClock::new();
        assert!(
            wait(&clock, &args(serde_json::json!({ "duration": "5ms" })))
                .await
                .is_ok()
        );
        let both = wait(
            &clock,
            &args(serde_json::json!({ "duration": "1s", "until": "x" })),
        )
        .await;
        assert!(matches!(both, Err(f) if f.code == "NIKA-BUILTIN-WAIT-003"));
        let neither = wait(&clock, &args(serde_json::json!({}))).await;
        assert!(matches!(neither, Err(f) if f.code == "NIKA-BUILTIN-WAIT-003"));
        // A wrong-TYPE arg is an input error (E1), not an XOR violation.
        let wrong_type = wait(&clock, &args(serde_json::json!({ "duration": 5 }))).await;
        assert!(matches!(wrong_type, Err(f) if f.code == "NIKA-BUILTIN-WAIT-001"));
        // `timeout:` with a RELATIVE duration was silently ignored before —
        // now a loud teaching reject (timeout caps absolute waits only).
        let paired = wait(
            &clock,
            &args(serde_json::json!({ "duration": "5ms", "timeout": "1h" })),
        )
        .await;
        assert!(
            matches!(&paired, Err(f) if f.code == "NIKA-BUILTIN-WAIT-001"
                && f.message.contains("ABSOLUTE")),
            "{paired:?}"
        );
    }

    #[tokio::test]
    async fn wait_absolute_covers_the_three_spec_codes() {
        let clock = MockClock::new();
        // Past timestamp → -002 (epoch is always behind the mock's base).
        let past = wait(
            &clock,
            &args(serde_json::json!({ "until": "1970-01-01T00:00:00Z" })),
        )
        .await;
        assert!(matches!(past, Err(f) if f.code == "NIKA-BUILTIN-WAIT-002"));
        // Future timestamp sleeps (mock sleep is instant) → Ok.
        let future = wait(
            &clock,
            &args(serde_json::json!({ "until": "2999-01-01T00:00:00Z" })),
        )
        .await;
        assert!(future.is_ok(), "{future:?}");
        // Future beyond the `timeout:` cap → -001 (absolute timeout).
        let capped = wait(
            &clock,
            &args(serde_json::json!({ "until": "2999-01-01T00:00:00Z", "timeout": "1h" })),
        )
        .await;
        assert!(matches!(capped, Err(f) if f.code == "NIKA-BUILTIN-WAIT-001"));
        // Unparseable until → -001 (input error).
        let bad = wait(&clock, &args(serde_json::json!({ "until": "not-a-time" }))).await;
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-WAIT-001"));
    }

    // ── Gate 6 · property test (crate spec §5 · parser totality) ────────

    proptest::proptest! {
        /// The Go-duration parser is TOTAL: arbitrary bytes (including
        /// multibyte UTF-8 across the digit/unit boundary) never panic,
        /// and any accepted value re-parses to the same duration when
        /// re-rendered through its seconds form.
        #[test]
        fn go_duration_parser_is_total(input in ".{0,24}") {
            if let Some(d) = parse_go_duration(&input) {
                // An accepted parse is canonical: rendering the total
                // seconds back through the grammar agrees.
                let rendered = format!("{}s", d.as_secs());
                if d.subsec_nanos() == 0 {
                    proptest::prop_assert_eq!(
                        parse_go_duration(&rendered),
                        Some(std::time::Duration::from_secs(d.as_secs()))
                    );
                }
            }
        }
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
        // A digit wall past 2^63 ns is rejected, never silently saturated
        // (the magnitude guard — kills the range-bound mutants).
        assert_eq!(parse_go_duration("9999999999999999999999h"), None);
        assert_eq!(parse_go_duration("99999999999999999999s"), None);
        // …while a large-but-sane value still parses exactly.
        assert_eq!(
            parse_go_duration("87600h"), // 10 years
            Some(Duration::from_secs(87_600 * 3600))
        );
    }

    #[tokio::test]
    async fn wait_absolute_timeout_boundary_is_inclusive() {
        // delta == cap PASSES (`>` not `>=`): MockClock's system_now is
        // constant between calls, so an `until:` computed from it is an
        // EXACT boundary.
        let clock = MockClock::new();
        let now = jiff::Timestamp::try_from(clock.system_now()).expect("in range");
        let target = now
            .checked_add(jiff::Span::new().hours(1))
            .expect("in range");
        let exact = wait(
            &clock,
            &args(serde_json::json!({ "until": target.to_string(), "timeout": "1h" })),
        )
        .await;
        assert!(exact.is_ok(), "delta == cap is allowed: {exact:?}");
        // …and one second past the cap is the timeout failure.
        let over = wait(
            &clock,
            &args(serde_json::json!({ "until": target.to_string(), "timeout": "59m" })),
        )
        .await;
        assert!(matches!(over, Err(f) if f.code == "NIKA-BUILTIN-WAIT-001"));
    }
}
