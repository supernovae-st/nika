// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run-time half of the W3 type contract (spec `09-types.md`) ·
//!
//! - **`decode:`** — the captured raw bytes become a value
//!   (`raw bytes → decode → value` · never through a lossy string) ·
//!   `text` strict UTF-8 (trailing-newline trimmed as today) · `json` ·
//!   `jsonl` (one value per non-empty line → array) · `bytes` (base64
//!   at the JSON boundary). A stream that does not decode settles the
//!   task `failure` (NIKA-1705 · the `NIKA-EXEC` lane · inside
//!   `on_error:` scope like every verb-stage failure).
//! - **the fit** — `fits(decoded, returns, named)` (the ONE judgment
//!   from `nika-types` · never re-implemented) · a violation is the
//!   spec-plane `NIKA-TYPE-101` ([`RuntimeError::ContractViolation`]).
//!
//! `infer:`/`agent:` never pass here — their `returns:` compiles to
//! `lower(returns)` on the EXISTING structured-output lane and a
//! violation stays `NIKA-INFER-002`-class (one voice · spec 09 §errors).

use std::collections::BTreeMap;

use nika_schema::raw::RawTask;
use nika_types::types::{NikaType, fits, lower};

use crate::errors::RuntimeError;

// (parse_type is reached by full path in `TaskContract::of` — the one
// grammar entry, same door the checker uses.)

/// A task's parsed `returns:` contract, resolved once per task lane —
/// the named-type environment rides along for `Ref` resolution.
pub struct TaskContract<'a> {
    /// The parsed `returns:` type.
    pub ty: NikaType,
    /// The workflow's acyclic named types (`types:` · shared per run).
    pub named: &'a BTreeMap<String, NikaType>,
}

impl<'a> TaskContract<'a> {
    /// Resolve a task's contract — `None` when the task declares no
    /// `returns:` (gradual: the output stays `Unknown`) or when the
    /// expression does not parse (impossible past a clean check —
    /// `run()` gates on `is_clean`, under which the named map carries
    /// EVERY declaration — kept `None` for belt-and-braces: never
    /// guess a contract).
    #[must_use]
    pub fn of(task: &RawTask, named: &'a BTreeMap<String, NikaType>) -> Option<Self> {
        let ret = task.returns.as_ref()?;
        let declared: std::collections::BTreeSet<String> = named.keys().cloned().collect();
        let ty = nika_types::types::parse_type(
            &ret.value,
            &declared,
            &format!("tasks.{}.returns", task.id.value),
        )
        .ok()?;
        Some(Self { ty, named })
    }

    /// The JSON-Schema projection (`lower(returns)`) — what the
    /// structured-output lane of `infer:`/`agent:` compiles (spec 09
    /// §returns · « the same enforcement lane as `schema:` »).
    #[must_use]
    pub fn lowered(&self) -> serde_json::Value {
        lower(&self.ty, self.named)
    }

    /// The run-time fit (`NIKA-TYPE-101` on violation) — `task` names
    /// the offender in the diagnostic.
    ///
    /// # Errors
    /// The typed contract violation (`NIKA-TYPE-101`), naming the task.
    pub fn check_fit(
        &self,
        task_note: &str,
        value: &serde_json::Value,
    ) -> Result<(), RuntimeError> {
        if fits(value, &self.ty, self.named) {
            return Ok(());
        }
        Err(RuntimeError::ContractViolation {
            message: format!(
                "{task_note} · the decoded value does not fit `returns:` — got {} \
                 (align the contract with what the command emits, or fix the command)",
                value_class(value)
            ),
        })
    }
}

/// `raw bytes → decode → value` (spec 09 §decode · normative table).
///
/// # Errors
/// The decode refusal for the mode (bytes that are not the declared form).
pub fn decode_bytes(
    mode: nika_schema::DecodeMode,
    raw: &[u8],
) -> Result<serde_json::Value, RuntimeError> {
    use nika_schema::DecodeMode as D;
    match mode {
        D::Text => match std::str::from_utf8(raw) {
            // « trailing newline trimmed as today » — the same trim_end
            // the lossy text path applies (one behavior).
            Ok(text) => Ok(serde_json::Value::String(text.trim_end().to_owned())),
            Err(e) => Err(RuntimeError::Decode {
                message: format!(
                    "decode: text — the captured output is not valid UTF-8 (byte {}) · \
                     an author who wants the octets says decode: bytes",
                    e.valid_up_to()
                ),
            }),
        },
        D::Json => serde_json::from_slice(raw).map_err(|e| RuntimeError::Decode {
            message: format!("decode: json — the captured output does not parse · {e}"),
        }),
        D::Jsonl => {
            let mut values = Vec::new();
            for (i, line) in raw.split(|&b| b == b'\n').enumerate() {
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue; // one value per NON-EMPTY line
                }
                let value = serde_json::from_slice(line).map_err(|e| RuntimeError::Decode {
                    message: format!(
                        "decode: jsonl — line {} does not parse as one JSON value · {e}",
                        i + 1
                    ),
                })?;
                values.push(value);
            }
            Ok(serde_json::Value::Array(values))
        }
        // CLOSED vocabulary (nika-vocab) — a future decode mode is a
        // spec change that must land HERE explicitly, never guessed.
        D::Bytes => Ok(serde_json::Value::String(base64_encode(raw))),
    }
}

/// RFC 4648 standard-alphabet base64 with padding — hand-rolled like
/// the `nika:hash`/`nika:read` helpers (zero new runtime deps · the
/// known-vector tests below pin it against RFC 4648 §10).
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A one-word class of a JSON value for diagnostics (never the value
/// itself — an exec output may carry secrets).
fn value_class(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_schema::DecodeMode as D;
    use serde_json::json;

    #[test]
    fn text_is_strict_utf8_and_trims_like_today() {
        let v = decode_bytes(D::Text, b"42\n").expect("valid utf-8");
        assert_eq!(v, json!("42"), "trailing newline trimmed as today");

        let err = decode_bytes(D::Text, &[0x66, 0x6f, 0xff, 0x0a]).expect_err("invalid utf-8");
        let msg = err.to_string();
        assert!(
            msg.contains("not valid UTF-8") && msg.contains("byte 2"),
            "names the byte + the fix: {msg}"
        );
        assert!(
            msg.contains("decode: bytes"),
            "teaches the octets door: {msg}"
        );
        assert_eq!(err.spec_code(), "NIKA-1705");
    }

    #[test]
    fn json_parses_one_document_from_the_bytes() {
        let v = decode_bytes(D::Json, b"{\"count\": 3, \"mean\": 1.5}\n").expect("parses");
        assert_eq!(v, json!({"count": 3, "mean": 1.5}));
        let err = decode_bytes(D::Json, b"{nope").expect_err("bad json");
        assert!(err.to_string().contains("decode: json"), "{err}");
    }

    #[test]
    fn jsonl_is_one_value_per_non_empty_line() {
        let v = decode_bytes(D::Jsonl, b"{\"a\":1}\n\n  \n{\"a\":2}\n").expect("parses");
        assert_eq!(v, json!([{"a":1},{"a":2}]), "empty lines skipped");
        let err = decode_bytes(D::Jsonl, b"{\"a\":1}\nBROKEN\n").expect_err("bad line");
        assert!(
            err.to_string().contains("line 2"),
            "names the offending line: {err}"
        );
    }

    #[test]
    fn bytes_is_base64_of_the_raw_octets() {
        // RFC 4648 §10 test vectors — the padding arms included.
        assert_eq!(decode_bytes(D::Bytes, b"").unwrap(), json!(""));
        assert_eq!(decode_bytes(D::Bytes, b"f").unwrap(), json!("Zg=="));
        assert_eq!(decode_bytes(D::Bytes, b"fo").unwrap(), json!("Zm8="));
        assert_eq!(decode_bytes(D::Bytes, b"foo").unwrap(), json!("Zm9v"));
        assert_eq!(decode_bytes(D::Bytes, b"foob").unwrap(), json!("Zm9vYg=="));
        assert_eq!(decode_bytes(D::Bytes, b"fooba").unwrap(), json!("Zm9vYmE="));
        assert_eq!(
            decode_bytes(D::Bytes, b"foobar").unwrap(),
            json!("Zm9vYmFy")
        );
        // invalid UTF-8 is FINE under bytes — the octets are the value
        let v = decode_bytes(D::Bytes, &[0xff, 0x00]).expect("bytes never refuse");
        assert_eq!(v, json!("/wA="));
    }

    #[test]
    fn fit_violation_is_type_101_and_never_leaks_the_value() {
        let named = BTreeMap::new();
        let contract = TaskContract {
            ty: NikaType::Prim(nika_types::types::Primitive::Integer),
            named: &named,
        };
        contract
            .check_fit("exec · jq", &json!(3))
            .expect("3 fits integer");
        let err = contract
            .check_fit("exec · jq", &json!("s3cret-value"))
            .expect_err("a string is not an integer");
        assert_eq!(err.spec_code(), "NIKA-TYPE-101");
        let msg = err.to_string();
        assert!(
            msg.contains("does not fit `returns:`") && msg.contains("a string"),
            "{msg}"
        );
        assert!(
            !msg.contains("s3cret-value"),
            "the VALUE never rides the diagnostic: {msg}"
        );
    }
}
