// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The DYNAMIC scrub half of the secrets seam — it implements the run's
//! `EventSink` (the stamp seam this member re-exports · ADR-127) and is
//! the custody's home; the resolution half descended to `nika-secret`
//! 2026-08-06.

use std::borrow::Cow;
use std::collections::BTreeMap;

use nika_event::Event;
use nika_types::resource::Value as FieldValue;
use serde_json::Value;

pub use nika_secret::{REDACTED, resolve_secrets};

use crate::stamp::EventSink;

/// Below this length a needle scrubs ONLY the payload fields: a
/// handful of characters rides inside hashes · ids · ordinary words
/// and a frame-wide scrub would mangle the journal.
const WIDE_SCRUB_MIN: usize = 8;

/// The event fields whose payload may carry a resolved value (the
/// terminal frame's outcome/output and the failure detail). `why` rides
/// the unwind lane's outcome frames — a failed cleanup's error message
/// can embed a stderr tail, the same class `detail` carries.
const PAYLOAD_FIELDS: [&str; 4] = [
    "outcome",
    crate::resume_fields::fields::OUTPUT,
    "detail",
    "why",
];

/// The dynamic-flow backstop (S1). The static IFC sanctions every
/// DECLARED secret flow; it cannot see a value that reaches a task's
/// output through a side channel (an `exec` catting a file-sourced
/// secret · an mcp tool echoing its input), and the terminal frame's
/// `outcome`/`output` payloads would carry that value into the journal
/// in plaintext. The runtime knows the resolved map, so the run's ONE
/// sink seam is wrapped and every event's string fields are scrubbed
/// (needles under `WIDE_SCRUB_MIN` · the `PAYLOAD_FIELDS` only)
/// before any lane (journal · `--json` · the live fold) sees them.
pub struct RedactingSink<'a> {
    /// The lane this scrub rides (the run verb's whole tee in production).
    inner: &'a mut dyn EventSink,
    /// (raw · json-escaped) needle pairs, deduped — empty on the common
    /// no-secrets run, where emit is a zero-cost forward. The escaped
    /// form catches the value INSIDE a payload that carries serialized
    /// JSON text (the `outcome`/`output` fields), where it appears
    /// escaped.
    needles: Vec<(String, String)>,
}

impl<'a> RedactingSink<'a> {
    /// Wrap the run's sink with the scrub set derived from the RESOLVED
    /// secrets map (the same map the run's [`Scope`](crate::expr::Scope)
    /// binds).
    pub fn new(inner: &'a mut dyn EventSink, resolved: &BTreeMap<String, Value>) -> Self {
        let needles = resolved
            .values()
            .filter_map(|v| match v {
                // PROVENANCE, not length (P0-19): every resolved value is
                // a needle, down to one byte — only the EMPTY string is
                // refused (replacing it would detonate every field).
                Value::String(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>() // two secrets may share a value
            .into_iter()
            .map(|raw| {
                let escaped = json_escaped(&raw);
                (raw, escaped)
            })
            .collect();
        Self { inner, needles }
    }

    /// Rewrite every needle occurrence in `text` to the marker — the
    /// escaped form first (when the pair differs it is the more specific
    /// needle), then the raw bytes. Borrowed when nothing matched (the
    /// common field), owned only on a hit. A needle shorter than
    /// [`WIDE_SCRUB_MIN`] bites only when `key` is a [`PAYLOAD_FIELDS`]
    /// member — the over-redaction bound (P0-19).
    fn scrub<'t>(&self, key: &str, text: &'t str) -> Cow<'t, str> {
        let mut out = Cow::Borrowed(text);
        for (raw, escaped) in &self.needles {
            if raw.len() < WIDE_SCRUB_MIN && !PAYLOAD_FIELDS.contains(&key) {
                continue;
            }
            if out.contains(escaped.as_str()) {
                out = Cow::Owned(out.replace(escaped.as_str(), REDACTED));
            }
            if raw != escaped && out.contains(raw.as_str()) {
                out = Cow::Owned(out.replace(raw.as_str(), REDACTED));
            }
        }
        out
    }
}

/// Scrub the run's resolved `outputs:` map with the same needles the
/// sink uses.
///
/// [`RedactingSink`] wraps the EVENT lane, and the outputs map is not an
/// event: it rides `RunOutcome` straight to `--output json`, where the
/// CLI serializes it verbatim. So the identical bytes were redacted in
/// `.nika/traces` and printed in the clear on stdout, in the same run
/// (2026-08-02 · found by an adversarial pass over the day's work).
///
/// The static IFC refuses a DECLARED egress (`outputs: ${{ secrets.x }}`
/// is `NIKA-SEC-007`, and a red file never runs). This closes the side
/// channel the runtime's own doc names as the reason this backstop
/// exists: an `exec` catting a file-sourced secret, an mcp tool echoing
/// its input — a value that never mentions `secrets.` and so is
/// invisible to the checker.
///
/// Every output is a value payload by nature, so the short-needle bound
/// does not apply here: a six-byte token in a returned value is exactly
/// the leak, not a false positive.
pub fn scrub_outputs(outputs: &mut BTreeMap<String, Value>, resolved: &BTreeMap<String, Value>) {
    let needles: Vec<(String, String)> = resolved
        .values()
        .filter_map(|v| match v {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|raw| {
            let escaped = json_escaped(&raw);
            (raw, escaped)
        })
        .collect();
    if needles.is_empty() {
        return;
    }
    for value in outputs.values_mut() {
        scrub_value(value, &needles);
    }
}

/// Rewrite every needle inside a JSON value, at any depth · a secret
/// nested in a returned object or array leaks exactly as loudly as one
/// at the top.
fn scrub_value(value: &mut Value, needles: &[(String, String)]) {
    match value {
        Value::String(s) => {
            for (raw, escaped) in needles {
                if s.contains(escaped.as_str()) {
                    *s = s.replace(escaped.as_str(), REDACTED);
                }
                if raw != escaped && s.contains(raw.as_str()) {
                    *s = s.replace(raw.as_str(), REDACTED);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                scrub_value(item, needles);
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                scrub_value(v, needles);
            }
        }
        _ => {}
    }
}

impl EventSink for RedactingSink<'_> {
    fn emit(&mut self, mut event: Event) {
        if !self.needles.is_empty() {
            for kv in &mut event.fields {
                // Only a String field can carry a value (the enum's other
                // arms are numbers/bools) — and the marker swaps whole
                // bytes, never the field's shape.
                if let FieldValue::String(text) = &mut kv.value
                    && let Cow::Owned(scrubbed) = self.scrub(&kv.key, text)
                {
                    *text = scrubbed;
                }
            }
        }
        self.inner.emit(event);
    }
}

/// The value as it appears INSIDE a serialized JSON text (the shape the
/// `outcome`/`output` payload fields carry): its JSON string literal
/// minus the quotes. Serializing a String is infallible in practice — a
/// failure degrades to the raw form (the scrub stays total, never
/// panics).
fn json_escaped(raw: &str) -> String {
    match serde_json::to_string(raw) {
        Ok(quoted) => quoted
            .strip_prefix('"')
            .and_then(|q| q.strip_suffix('"'))
            .unwrap_or(&quoted)
            .to_owned(),
        Err(_) => raw.to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_schema::{SecretRef, SecretSource, Spanned};
    use nika_secret::{
        NoSecrets, SecretResolveError, WorkflowSecretResolver, source_is_runtime_resolvable,
    };
    use proptest::prelude::*;

    /// A scripted resolver mapping `name → value` (the composer's role,
    /// without touching env/files).
    struct MapResolver(BTreeMap<String, String>);

    impl WorkflowSecretResolver for MapResolver {
        fn resolve(
            &self,
            name: &str,
            _reference: &SecretRef,
        ) -> Result<String, SecretResolveError> {
            self.0.get(name).cloned().ok_or_else(|| SecretResolveError {
                name: name.to_owned(),
                reason: "absent".to_owned(),
            })
        }
    }

    fn declared(
        name: &str,
        source: SecretSource,
        key: &str,
    ) -> Vec<(Spanned<String>, Spanned<SecretRef>)> {
        vec![(
            Spanned::new(name.to_owned(), nika_schema::Span::default()),
            Spanned::new(SecretRef::new(source, key), nika_schema::Span::default()),
        )]
    }

    #[test]
    fn resolves_each_declared_secret_into_the_namespace() {
        let resolver = MapResolver(BTreeMap::from([(
            "api_key".to_owned(),
            "sk-123".to_owned(),
        )]));
        let map = resolve_secrets(&resolver, &declared("api_key", SecretSource::Env, "MY_KEY"));
        assert_eq!(
            map.get("api_key"),
            Some(&Value::String("sk-123".to_owned()))
        );
    }

    #[test]
    fn unresolved_reference_is_omitted_fail_closed() {
        // A miss leaves the secret UNBOUND (omitted) so the reference site
        // raises NIKA-1702 — never a silent "null", never a panic.
        let resolver = MapResolver(BTreeMap::new());
        let map = resolve_secrets(
            &resolver,
            &declared("api_key", SecretSource::Env, "MISSING"),
        );
        assert!(
            map.is_empty(),
            "an unresolved secret is omitted, not 'null'"
        );
    }

    #[test]
    fn no_secrets_resolver_always_fails_closed() {
        let err = NoSecrets
            .resolve("k", &SecretRef::new(SecretSource::Env, "K"))
            .expect_err("no resolver → closed");
        assert_eq!(err.name, "k");
        // The whole namespace is empty under NoSecrets (prior behavior).
        assert!(resolve_secrets(&NoSecrets, &declared("k", SecretSource::Env, "K")).is_empty());
    }

    #[test]
    fn empty_declaration_resolves_to_empty_map() {
        let map = resolve_secrets(&NoSecrets, &[]);
        assert!(map.is_empty());
    }

    #[test]
    fn source_resolvability_matches_the_wired_set() {
        assert!(source_is_runtime_resolvable(SecretSource::Env));
        assert!(source_is_runtime_resolvable(SecretSource::File));
        assert!(!source_is_runtime_resolvable(SecretSource::Vault));
    }

    // ───────────────── S1 · the redaction seam ─────────────────

    use nika_event::{Event, EventKind};
    use nika_types::id::EventId;
    use nika_types::resource::KeyValue;
    use nika_types::timestamp::Timestamp;

    /// One terminal-shaped event whose `outcome` field carries `payload`
    /// verbatim — the frame the journal-mirror test pins semantically.
    fn outcome_event(payload: &str) -> Event {
        Event::new(
            EventId::new(uuid::Uuid::nil()),
            Timestamp::from_unix_ms(0),
            EventKind::TaskCompleted,
        )
        .with_field(KeyValue::new(
            "outcome",
            FieldValue::String(payload.to_owned()),
        ))
        .with_field(KeyValue::new("tokens", FieldValue::Int(7)))
    }

    /// One failure-shaped event whose `detail` field carries `payload`
    /// verbatim — the settle.rs `error.code · error.message` frame,
    /// where the message embeds up to 1024 bytes of raw process stderr
    /// (the audit's 6-byte OTP leak).
    fn detail_event(payload: &str) -> Event {
        Event::new(
            EventId::new(uuid::Uuid::nil()),
            Timestamp::from_unix_ms(0),
            EventKind::TaskFailed,
        )
        .with_field(KeyValue::new(
            "detail",
            FieldValue::String(payload.to_owned()),
        ))
    }

    fn field_text<'e>(event: &'e Event, key: &str) -> &'e str {
        event
            .fields
            .iter()
            .find(|kv| kv.key == key)
            .and_then(|kv| match &kv.value {
                FieldValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the {key} field rides"))
    }

    fn resolved(secret: &str) -> BTreeMap<String, Value> {
        BTreeMap::from([("tok".to_owned(), Value::String(secret.to_owned()))])
    }

    /// Drive one event through the scrub and return the collected stream.
    fn scrubbed(secret: &str, event: Event) -> Vec<Event> {
        let mut inner = crate::VecSink::new();
        {
            let mut sink = RedactingSink::new(&mut inner, &resolved(secret));
            sink.emit(event);
        }
        inner.into_events()
    }

    fn outcome_of(event: &Event) -> &str {
        field_text(event, "outcome")
    }

    /// The audit's leak: a resolved value surfaced inside a task output
    /// and rides the terminal frame's JSON-text payload — the scrub
    /// rewrites it to the marker, leaving the payload valid JSON.
    #[test]
    fn redacting_sink_rewrites_a_raw_occurrence() {
        let secret = "sk-live-9f2c7e4a1b6d";
        let events = scrubbed(secret, outcome_event(&format!("echo {secret} done")));
        let outcome = outcome_of(&events[0]);
        assert!(!outcome.contains(secret), "the value is gone: {outcome}");
        assert!(outcome.contains(REDACTED), "the marker stands: {outcome}");
        // Non-string fields are never touched (numbers stay numbers).
        assert!(
            events[0]
                .fields
                .iter()
                .any(|kv| kv.key == "tokens" && kv.value == FieldValue::Int(7)),
            "a numeric field is out of the scrub's scope"
        );
    }

    /// The double-encoded case: inside a payload that carries serialized
    /// JSON text, the value appears ESCAPED — the escaped needle is the
    /// one that bites, and the payload stays parseable afterwards.
    #[test]
    fn redacting_sink_rewrites_the_json_escaped_form() {
        let secret = "ab\"cd1234"; // carries a quote — provenance governs, never length
        let payload = serde_json::json!({"class": "ok", "value": secret}).to_string();
        let events = scrubbed(secret, outcome_event(&payload));
        let outcome = outcome_of(&events[0]);
        assert!(
            !outcome.contains("ab\\\"cd1234") && !outcome.contains(secret),
            "neither the escaped nor the raw form survives: {outcome}"
        );
        let parsed: serde_json::Value =
            serde_json::from_str(outcome).expect("the payload stays valid JSON");
        assert_eq!(parsed["value"], serde_json::json!(REDACTED));
    }

    proptest! {
        /// P0-19 · redaction follows PROVENANCE, never a length threshold:
        /// a value resolved through the `secrets` map — down to ONE byte
        /// (the PIN/OTP class the audit named) — never survives in an event
        /// payload, whatever its length. The marker stands in its place.
        /// (Alphanumeric alphabet: those bytes JSON-escape to themselves,
        /// so the raw and escaped needles coincide.)
        #[test]
        fn redacting_sink_redacts_a_secret_of_any_length(
            secret in "[a-zA-Z0-9]{1,7}",
        ) {
            let events = scrubbed(&secret, outcome_event(&format!("echo {secret} done")));
            let outcome = outcome_of(&events[0]);
            prop_assert!(
                !outcome.contains(&secret),
                "the short value is gone: {outcome}"
            );
            prop_assert!(
                outcome.contains(REDACTED),
                "the marker stands: {outcome}"
            );
        }
    }

    /// P0-19 · the over-redaction bound: a short needle scrubs the
    /// VALUE-PAYLOAD fields (`outcome` · `output`) but leaves every
    /// other String field alone — a 1-byte needle over the whole frame
    /// would mangle ids · hashes · labels (`attested` carries the
    /// needle `a` and must ride through untouched).
    #[test]
    fn short_secret_scrubs_only_the_payload_fields() {
        let secret = "a";
        let event = outcome_event("echo a done").with_field(KeyValue::new(
            "integrity",
            FieldValue::String("attested".to_owned()),
        ));
        let events = scrubbed(secret, event);
        let outcome = outcome_of(&events[0]);
        assert!(outcome.contains(REDACTED), "payload scrubbed: {outcome}");
        let integrity = field_text(&events[0], "integrity");
        assert_eq!(
            integrity, "attested",
            "a short needle never touches non-payload fields"
        );
    }

    /// One completion event whose `output` field carries `payload` —
    /// the field a task's own result lands in, and the third member of
    /// [`PAYLOAD_FIELDS`].
    fn output_event(payload: &str) -> Event {
        Event::new(
            EventId::new(uuid::Uuid::nil()),
            Timestamp::from_unix_ms(0),
            EventKind::TaskCompleted,
        )
        .with_field(KeyValue::new(
            crate::resume_fields::fields::OUTPUT,
            FieldValue::String(payload.to_owned()),
        ))
    }

    /// `output` is where a task's own result lands, so it is where a
    /// secret reaching a value through a side channel arrives — an
    /// `exec` catting a file-sourced credential, an mcp tool echoing
    /// its input. It is the whole reason this backstop exists.
    ///
    /// It was the one member of [`PAYLOAD_FIELDS`] no test named:
    /// removing it from the array left the suite green (2026-08-02),
    /// while its two siblings were each caught by name. The sibling
    /// above even documents covering it.
    #[test]
    fn short_secret_scrubs_the_output_field() {
        let secret = "827351"; // under WIDE_SCRUB_MIN, like the OTP leak
        let events = scrubbed(secret, output_event(&format!("token: {secret}")));
        let out = field_text(&events[0], crate::resume_fields::fields::OUTPUT);
        assert!(
            !out.contains(secret),
            "a secret reaching a task output must not ride the journal: {out}"
        );
        assert!(
            out.contains(REDACTED),
            "and it is redacted, not dropped: {out}"
        );
    }

    /// Every payload field, from the const — so a member added later
    /// inherits the proof instead of waiting for someone to notice.
    /// (The floor that keeps one from LEAVING is the array's own length,
    /// which the compiler checks.)
    #[test]
    fn every_payload_field_scrubs_a_short_needle() {
        let secret = "827351";
        for field in PAYLOAD_FIELDS {
            let event = Event::new(
                EventId::new(uuid::Uuid::nil()),
                Timestamp::from_unix_ms(0),
                EventKind::TaskCompleted,
            )
            .with_field(KeyValue::new(
                field,
                FieldValue::String(format!("carries {secret} inside")),
            ));
            let events = scrubbed(secret, event);
            let text = field_text(&events[0], field);
            assert!(
                !text.contains(secret),
                "payload field `{field}` let a short needle through: {text}"
            );
        }
    }

    /// The audit's `detail` leak (2026-07-31): a failed exec journals
    /// `error.code · error.message` as the `detail` field — and the
    /// message embeds raw process stderr, so a 6-byte OTP rides the
    /// trace in plaintext while the `outcome` copy is scrubbed.
    /// `detail` is a value-payload field: the needle must not survive.
    #[test]
    fn short_secret_scrubs_the_detail_field() {
        let secret = "827351"; // the 6-byte OTP — under WIDE_SCRUB_MIN
        let events = scrubbed(
            secret,
            detail_event(&format!(
                "NIKA-EXEC-001 · command exited with status 3: {secret}"
            )),
        );
        let detail = field_text(&events[0], "detail");
        assert!(
            !detail.contains(secret),
            "the detail copy carries no plaintext: {detail}"
        );
        assert!(
            detail.contains(REDACTED),
            "the marker stands in its place: {detail}"
        );
    }

    proptest! {
        /// The `detail` sibling of the length property above: a needle
        /// of ANY length under the wide-scrub threshold is rewritten in
        /// the failure frame's detail payload, never just in `outcome`.
        #[test]
        fn redacting_sink_redacts_a_short_secret_in_detail(
            secret in "[a-zA-Z0-9]{1,7}",
        ) {
            let events = scrubbed(
                &secret,
                detail_event(&format!("NIKA-EXEC-001 · boom: {secret}")),
            );
            let detail = field_text(&events[0], "detail");
            prop_assert!(
                !detail.contains(&secret),
                "the short value is gone from detail: {detail}"
            );
            prop_assert!(
                detail.contains(REDACTED),
                "the marker stands: {detail}"
            );
        }
    }

    /// An EMPTY resolved value never joins the scrub set — replacing
    /// the empty needle would detonate the field (a marker between
    /// every byte).
    #[test]
    fn empty_secret_value_never_becomes_a_needle() {
        let events = scrubbed("", outcome_event("echo done"));
        assert_eq!(
            outcome_of(&events[0]),
            "echo done",
            "the empty needle is no needle"
        );
    }

    /// No declared secrets (the common run): the event crosses the seam
    /// byte-unchanged.
    #[test]
    fn redacting_sink_without_secrets_is_a_passthrough() {
        let event = outcome_event("echo sk-live-9f2c7e4a1b6d done");
        let mut inner = crate::VecSink::new();
        {
            let mut sink = RedactingSink::new(&mut inner, &BTreeMap::new());
            sink.emit(event.clone());
        }
        assert_eq!(
            inner.into_events(),
            vec![event],
            "zero needles · zero rewrites"
        );
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod outputs_scrub_tests {
    use super::{REDACTED, scrub_outputs};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn resolved(v: &str) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("db_pass".to_owned(), Value::String(v.to_owned()));
        m
    }

    /// The side channel the runtime's own doc names: a task reads the
    /// secret's FILE by an ordinary tool, so nothing ever mentions
    /// `secrets.` and the static IFC has nothing to refuse. The value
    /// then rides `outputs:` to `--output json` stdout, where the
    /// redacting sink — an EVENT wrapper — never looked (2026-08-02).
    #[test]
    fn a_secret_reaching_outputs_by_a_side_channel_is_redacted() {
        let secrets = resolved("hunter2-secret");
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "result".to_owned(),
            Value::String("hunter2-secret\n".to_owned()),
        );
        scrub_outputs(&mut outputs, &secrets);
        let text = outputs["result"].as_str().expect("string");
        assert!(
            !text.contains("hunter2-secret"),
            "the secret rode out: {text}"
        );
        assert!(text.contains(REDACTED), "redacted, not dropped: {text}");
    }

    /// Depth is not a hiding place: an object or array member leaks as
    /// loudly as a top-level string.
    #[test]
    fn a_nested_secret_is_redacted_at_any_depth() {
        let secrets = resolved("hunter2-secret");
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "report".to_owned(),
            json!({"rows": [{"creds": "user:hunter2-secret@host"}], "n": 1}),
        );
        scrub_outputs(&mut outputs, &secrets);
        let dumped = serde_json::to_string(&outputs).expect("json");
        assert!(
            !dumped.contains("hunter2-secret"),
            "nested secret rode out: {dumped}"
        );
        assert!(dumped.contains(REDACTED), "{dumped}");
    }

    /// A short needle IS scrubbed here. Every output is a value payload
    /// by nature, so the six-byte OTP in a returned value is the leak,
    /// not a false positive — the bound that protects ids and labels on
    /// the event lane has nothing to protect here.
    #[test]
    fn a_short_needle_is_scrubbed_in_an_output() {
        let secrets = resolved("827351");
        let mut outputs = BTreeMap::new();
        outputs.insert("code".to_owned(), Value::String("otp 827351".to_owned()));
        scrub_outputs(&mut outputs, &secrets);
        assert_eq!(outputs["code"].as_str(), Some("otp ***"));
    }

    /// A run with no secrets pays nothing and changes nothing.
    #[test]
    fn no_secrets_leaves_every_output_untouched() {
        let mut outputs = BTreeMap::new();
        outputs.insert("a".to_owned(), Value::String("plain text".to_owned()));
        let before = outputs.clone();
        scrub_outputs(&mut outputs, &BTreeMap::new());
        assert_eq!(outputs, before);
    }
}
