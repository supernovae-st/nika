// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow secret resolution — the `secrets:` namespace seam (MINOR-B).
//!
//! The envelope's `secrets:` block declares each value as a STORE REFERENCE
//! (`{ source, key }` · never an inline literal · spec 01 §secrets). At run
//! time the engine resolves those references into the values the
//! `${{ secrets.X }}` namespace exposes — but resolution READS a store
//! (env var · file · vault), which is an EFFECT. So the runtime (L3) never
//! reads env/files itself: the COMPOSER (L4 · nika-cli, the sanctioned
//! `std::env` boundary) injects a [`WorkflowSecretResolver`].
//!
//! ## Sovereignty + masking
//!
//! A resolved value flows ONLY where the static IFC (`nika check`
//! secret-flow analysis · ADR-092) sanctions it, and is NEVER written to the
//! event stream (notes carry the program/model name, not field values). The
//! resolved values live in the [`Scope`](crate::expr::Scope)'s `secrets`
//! map for the duration of the run and are dropped with it.
//!
//! The "never written" half is enforced DYNAMICALLY (S1 · journal
//! hygiene): the IFC sees declared flows, not a value that surfaces
//! through a side channel — an `exec` that cats a file-sourced secret,
//! an mcp tool that echoes its input. Such a value lands in a task's
//! OUTPUT and would ride the terminal frame's `outcome`/`output`
//! payload into `.nika/traces/*.ndjson` in plaintext. [`RedactingSink`]
//! wraps the run's ONE sink seam and rewrites every occurrence of a
//! resolved value to [`REDACTED`] before any lane (journal · `--json` ·
//! the live fold) sees the event. Redaction follows PROVENANCE — any
//! value the map resolved, down to one byte (P0-19 · the PIN/OTP
//! class) — never a length floor; a short needle only narrows its blast
//! radius to the value-payload fields (see [`WIDE_SCRUB_MIN`]).
//!
//! ## Fail-closed
//!
//! A reference that does not resolve (env var unset · file missing) is
//! simply OMITTED from the namespace — so `${{ secrets.X }}` then raises the
//! loud unresolved class (`NIKA-1702`), a clean typed error, BEFORE the
//! value is ever needed. It is NEVER a panic, and NEVER a silent empty value
//! (which would turn `${{ secrets.X }}` into the literal string "null"). A
//! workflow that declares a secret it never references is unaffected (the
//! omission only bites a reference) — the same posture as today, where every
//! `secrets.X` was unresolved.

use std::borrow::Cow;
use std::collections::BTreeMap;

use nika_event::Event;
use nika_schema::types::{SecretRef, SecretSource};
use nika_types::resource::Value as FieldValue;
use serde_json::Value;

use crate::EventSink;

/// Resolve a workflow `secrets:` reference into its value.
///
/// The composer (L4) implements this over the real stores (env · file ·
/// vault). The runtime calls it once per declared secret at run start.
pub trait WorkflowSecretResolver: Send + Sync {
    /// Resolve one `{ source, key }` reference to its plaintext value.
    ///
    /// The implementor MUST NOT log the returned value. `name` is the
    /// `secrets.<name>` key (for diagnostics only · safe to log).
    ///
    /// # Errors
    ///
    /// [`SecretResolveError`] when the reference cannot be resolved (store
    /// miss · source not supported by this resolver). The run fails cleanly.
    fn resolve(&self, name: &str, reference: &SecretRef) -> Result<String, SecretResolveError>;
}

/// A workflow secret reference that did not resolve (MINOR-B).
///
/// Surfaced by a [`WorkflowSecretResolver`] when a store lookup misses. The
/// runtime treats it as « leave this `secrets.<name>` unbound » → a later
/// `${{ secrets.<name>` reference raises `NIKA-1702` (the loud unresolved
/// class · clean typed error · never a panic). The message NEVER contains a
/// value (there is none — resolution failed) nor the store contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretResolveError {
    /// The `secrets.<name>` that failed (safe to surface · not the value).
    pub name: String,
    /// Why it failed (store miss · unsupported source · safe to surface).
    pub reason: String,
}

impl std::fmt::Display for SecretResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "secrets.{} did not resolve · {}", self.name, self.reason)
    }
}

impl std::error::Error for SecretResolveError {}

/// The no-op resolver — the default when the composer injects none. Every
/// `secrets.X` then resolves to `None` (NIKA-1702 · the prior behavior ·
/// secrets fail CLOSED). Used by tests + any headless run with no store.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSecrets;

impl WorkflowSecretResolver for NoSecrets {
    fn resolve(&self, name: &str, _reference: &SecretRef) -> Result<String, SecretResolveError> {
        Err(SecretResolveError {
            name: name.to_owned(),
            reason: "no secret resolver is configured for this run".to_owned(),
        })
    }
}

/// Resolve every declared `secrets:` reference into the `secrets` namespace
/// map (`<name>` → the value as a JSON string · MINOR-B).
///
/// Called once at run start. A reference that does not resolve is OMITTED
/// (fail-closed · the later `${{ secrets.X }}` reference then raises
/// `NIKA-1702`, never reads "null") — a declared-but-unreferenced secret is
/// therefore harmless. The returned map is bound in the run's
/// [`Scope`](crate::expr::Scope).
///
/// `declared` is `wf.secrets` (`<name>` → reference).
#[must_use]
pub(crate) fn resolve_secrets<R: WorkflowSecretResolver + ?Sized>(
    resolver: &R,
    declared: &[(
        nika_schema::Spanned<String>,
        nika_schema::Spanned<SecretRef>,
    )],
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for (name, reference) in declared {
        // A miss leaves the secret UNBOUND (omitted) — fail-closed: the
        // reference site raises NIKA-1702, never reads a silent "null".
        if let Ok(value) = resolver.resolve(&name.value, &reference.value) {
            out.insert(name.value.clone(), Value::String(value));
        }
    }
    out
}

/// Whether a [`SecretSource`] is runtime-resolvable by the canonical
/// composer resolver today (`env` · `file`). `vault` is not yet wired — the
/// checker WARNs rather than letting a green check fail at runtime
/// (NIKA-1702). Exposed so the CLI's `check` can surface the warning.
#[must_use]
pub fn source_is_runtime_resolvable(source: SecretSource) -> bool {
    matches!(source, SecretSource::Env | SecretSource::File)
}

// ─────────────────────── S1 · journal hygiene ───────────────────────

/// The redaction marker — a resolved secret value that reaches an event
/// payload is rewritten to these exact bytes. Fixed width: the marker
/// must not encode anything about the value it hides, not even a length.
pub(crate) const REDACTED: &str = "***";

/// The wide-scrub threshold (P0-19). Redaction follows PROVENANCE —
/// every value resolved through the `secrets` map joins the scrub set,
/// down to ONE byte (the PIN/OTP class) — never a length floor. What a
/// needle's length decides is its BLAST RADIUS: at this size or beyond
/// the needle is distinctive enough to scrub every String field of the
/// frame; below it the needle bites only [`PAYLOAD_FIELDS`], because a
/// handful of characters rides inside hashes · ids · ordinary words and
/// a frame-wide scrub would mangle the journal.
const WIDE_SCRUB_MIN: usize = 8;

/// The String fields that can carry a resolved secret's VALUE — the
/// terminal frame's `outcome` payload (spec 13 · serialized task data)
/// and the ADR-099 `output` rehydration text. Short needles scrub ONLY
/// these; digests · ids · labels stay outside by construction.
const PAYLOAD_FIELDS: [&str; 2] = ["outcome", crate::resume::fields::OUTPUT];

/// The dynamic-flow backstop (S1). The static IFC sanctions every
/// DECLARED secret flow; it cannot see a value that reaches a task's
/// output through a side channel (an `exec` catting a file-sourced
/// secret · an mcp tool echoing its input), and the terminal frame's
/// `outcome`/`output` payloads would carry that value into the journal
/// in plaintext. The runtime knows the resolved map, so the run's ONE
/// sink seam is wrapped and every event's string fields are scrubbed
/// (needles under [`WIDE_SCRUB_MIN`] · the [`PAYLOAD_FIELDS`] only)
/// before any lane (journal · `--json` · the live fold) sees them.
pub(crate) struct RedactingSink<'a> {
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
    pub(crate) fn new(inner: &'a mut dyn EventSink, resolved: &BTreeMap<String, Value>) -> Self {
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
    use nika_schema::Spanned;
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
        event
            .fields
            .iter()
            .find(|kv| kv.key == "outcome")
            .and_then(|kv| match &kv.value {
                FieldValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .expect("the outcome field rides")
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
        let secret = "ab\"cd1234"; // ≥ the floor · carries a quote
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
        let integrity = events[0]
            .fields
            .iter()
            .find(|kv| kv.key == "integrity")
            .and_then(|kv| match &kv.value {
                FieldValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .expect("the integrity field rides");
        assert_eq!(
            integrity, "attested",
            "a short needle never touches non-payload fields"
        );
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
