// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The fortress decoder (F-P1 · NEP-0012) — every artifact the verifier
//! reads is UNTRUSTED INPUT, and the bounds are code, never folklore.
//!
//! Four laws (NEP-0012):
//!
//! 1. **Bounds are constants** — input size · nesting depth · proof-node
//!    count · identifier length live here as named constants; an artifact
//!    beyond a bound earns a TYPED refusal ([`DecodeRefusal`]), never a
//!    truncation-and-continue (the shotgun-parser class).
//! 2. The refusal is TOTAL — no partial [`Value`] ever escapes.
//! 3. The depth guard runs BEFORE `serde_json` sees the bytes (a byte
//!    scanner, string-aware) — the unbounded-recursion class
//!    (CVE-2026-26209) dies at the door, on every build profile
//!    (CVE-2026-29013: bounds carried by debug assertions are not bounds).
//! 4. The malicious corpus (`tests/receipts/malicious/`) is born with
//!    this law (ratchet NEP-0000) and is pinned by the unit tests below;
//!    the `receipt_decode` fuzz target rides the same entry point.

use serde_json::Value;

/// Maximum artifact size the verifier decodes — a memory-safety bound,
/// not a policy limit (the nika-schema `MAX_SOURCE_BYTES` idiom): a real
/// receipt is kilobytes; 1 MiB is loud headroom.
pub const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;

/// Maximum journal size the verifier reads WHOLE — same memory-safety
/// class, a different artifact: a run journal is event-bounded (~128 B
/// per line · millions of events stay far under this) and a file beyond
/// it is not a run this engine produced. 256 MiB is loud headroom that
/// still fails closed before memory exhaustion.
pub const MAX_JOURNAL_BYTES: usize = 256 * 1024 * 1024;

/// Maximum JSON nesting depth (objects + arrays) — the
/// CVE-2026-26209 anchor: deep nesting is an attack, never a receipt.
pub const MAX_JSON_DEPTH: usize = 32;

/// Maximum entries in a proof-bearing array (`assertions` · `proof`) —
/// a receipt carries a handful; 64 is loud headroom.
pub const MAX_PROOF_NODES: usize = 64;

/// Maximum byte length of an identifier-class string (`proves` ·
/// `lock_digest` · the digest field) — hex digests are 64–71 bytes;
/// 256 is loud headroom.
pub const MAX_ID_BYTES: usize = 256;

/// A typed decode refusal — the fortress names the bound it enforced
/// and what it observed. Total: no partial value accompanies it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DecodeRefusal {
    /// The artifact exceeds [`MAX_ARTIFACT_BYTES`].
    #[error(
        "artifact is {got} bytes — the verifier decodes at most {max} (oversized is an attack, never a receipt)"
    )]
    Oversized {
        /// Observed byte length.
        got: usize,
        /// The enforced bound.
        max: usize,
    },
    /// Nesting deeper than [`MAX_JSON_DEPTH`] (measured pre-parse).
    #[error(
        "nesting reaches depth {got} — the verifier admits at most {max} (the unbounded-recursion class)"
    )]
    TooDeep {
        /// Observed depth at refusal.
        got: usize,
        /// The enforced bound.
        max: usize,
    },
    /// The bytes are not one valid JSON document (truncated · trailing
    /// garbage · not JSON) — recognized, never repaired.
    #[error("not one valid JSON document: {detail}")]
    Malformed {
        /// The parser's sentence (already display-safe: ASCII).
        detail: String,
    },
    /// A proof-bearing array exceeds [`MAX_PROOF_NODES`].
    #[error("`{field}` carries {got} nodes — the verifier admits at most {max}")]
    ProofFlood {
        /// The offending field.
        field: &'static str,
        /// Observed entry count.
        got: usize,
        /// The enforced bound.
        max: usize,
    },
    /// An identifier-class string exceeds [`MAX_ID_BYTES`].
    #[error("`{field}` is {got} bytes — an identifier is at most {max}")]
    IdOverflow {
        /// The offending field.
        field: &'static str,
        /// Observed byte length.
        got: usize,
        /// The enforced bound.
        max: usize,
    },
}

/// String-aware depth scan — counts `{`/`[` nesting OUTSIDE string
/// literals, refusing at [`MAX_JSON_DEPTH`]. Runs before `serde_json`
/// touches the bytes, so the recursion bound holds on every profile.
fn depth_scan(raw: &str) -> Result<(), DecodeRefusal> {
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escaped = false;
    for byte in raw.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > MAX_JSON_DEPTH {
                    return Err(DecodeRefusal::TooDeep {
                        got: depth,
                        max: MAX_JSON_DEPTH,
                    });
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

/// Structural bounds on the DECODED value — proof-bearing arrays and
/// identifier-class strings, wherever they sit in the document (the
/// scan is total, so a nested flood cannot hide).
fn structural_scan(value: &Value) -> Result<(), DecodeRefusal> {
    const PROOF_FIELDS: [&str; 3] = ["assertions", "proof", "covers"];
    const ID_FIELDS: [&str; 4] = ["proves", "digest", "lock_digest", "workflow_semantic"];
    let mut stack = vec![value];
    while let Some(node) = stack.pop() {
        match node {
            Value::Object(map) => {
                for (key, child) in map {
                    if let Some(field) = PROOF_FIELDS.iter().find(|f| *f == key)
                        && let Value::Array(items) = child
                        && items.len() > MAX_PROOF_NODES
                    {
                        return Err(DecodeRefusal::ProofFlood {
                            field,
                            got: items.len(),
                            max: MAX_PROOF_NODES,
                        });
                    }
                    if let Some(field) = ID_FIELDS.iter().find(|f| *f == key)
                        && let Value::String(s) = child
                        && s.len() > MAX_ID_BYTES
                    {
                        return Err(DecodeRefusal::IdOverflow {
                            field,
                            got: s.len(),
                            max: MAX_ID_BYTES,
                        });
                    }
                    stack.push(child);
                }
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    Ok(())
}

/// Decode ONE untrusted JSON artifact under the fortress bounds — the
/// single entry point the verifier surfaces (and the `receipt_decode`
/// fuzz target) ride. Total: `Ok` carries the whole document, `Err`
/// carries a typed refusal and nothing else.
///
/// # Errors
///
/// A [`DecodeRefusal`] naming the enforced bound: [`Oversized`]
/// (length · pre-parse) · [`TooDeep`] (nesting · pre-parse) ·
/// [`Malformed`] (not one valid JSON document) · [`ProofFlood`] /
/// [`IdOverflow`] (structural bounds on the decoded value).
///
/// [`Oversized`]: DecodeRefusal::Oversized
/// [`TooDeep`]: DecodeRefusal::TooDeep
/// [`Malformed`]: DecodeRefusal::Malformed
/// [`ProofFlood`]: DecodeRefusal::ProofFlood
/// [`IdOverflow`]: DecodeRefusal::IdOverflow
pub fn decode_untrusted_json(raw: &str) -> Result<Value, DecodeRefusal> {
    if raw.len() > MAX_ARTIFACT_BYTES {
        return Err(DecodeRefusal::Oversized {
            got: raw.len(),
            max: MAX_ARTIFACT_BYTES,
        });
    }
    depth_scan(raw)?;
    let value: Value = serde_json::from_str(raw).map_err(|e| DecodeRefusal::Malformed {
        // The parser's message is ASCII (line/column + class) — safe to
        // carry; the RAW BYTES never ride the refusal.
        detail: e.to_string(),
    })?;
    structural_scan(&value)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// The golden positive — a real receipt shape decodes whole.
    #[test]
    fn a_real_receipt_decodes_whole() {
        let golden = include_str!("../tests/receipts/golden.json");
        let value = decode_untrusted_json(golden).expect("the golden receipt decodes");
        assert!(value.get("proves").is_some(), "the shape survived");
    }

    /// The seven malicious classes refuse TYPED — recognized, never
    /// repaired (langsec · the corpus is born with the law).
    #[test]
    fn the_malicious_corpus_refuses_typed() {
        let deep = include_str!("../tests/receipts/malicious/deep.json");
        assert!(matches!(
            decode_untrusted_json(deep),
            Err(DecodeRefusal::TooDeep { .. })
        ));
        let truncated = include_str!("../tests/receipts/malicious/truncated.json");
        assert!(matches!(
            decode_untrusted_json(truncated),
            Err(DecodeRefusal::Malformed { .. })
        ));
        let trailing = include_str!("../tests/receipts/malicious/trailing.json");
        assert!(matches!(
            decode_untrusted_json(trailing),
            Err(DecodeRefusal::Malformed { .. })
        ));
        let flood = include_str!("../tests/receipts/malicious/proof-flood.json");
        assert!(matches!(
            decode_untrusted_json(flood),
            Err(DecodeRefusal::ProofFlood {
                field: "assertions",
                ..
            })
        ));
        let id = include_str!("../tests/receipts/malicious/id-overflow.json");
        assert!(matches!(
            decode_untrusted_json(id),
            Err(DecodeRefusal::IdOverflow {
                field: "proves",
                ..
            })
        ));
        let dupkeys = include_str!("../tests/receipts/malicious/duplicate-keys.json");
        // serde_json keeps the LAST duplicate (documented) — the decode
        // ADMITS the document; the duplicate never smuggles extra depth
        // or flood past the scans, and digest verification refuses the
        // mutated body downstream. Recognized for what it is.
        assert!(decode_untrusted_json(dupkeys).is_ok());
        let escapes = include_str!("../tests/receipts/malicious/escape-bearing.json");
        // Escape sequences are LEGAL JSON — the decode admits them; the
        // TERMINAL HYGIENE law (the render side) escapes them at print.
        let v = decode_untrusted_json(escapes).expect("legal JSON admits");
        let s = v["proves"].as_str().expect("string field");
        assert!(
            s.contains('\u{1b}'),
            "the raw escape survives DECODE — the render law owns display"
        );
    }

    /// Oversized refuses on length alone — the bytes are never parsed.
    #[test]
    fn oversized_refuses_before_the_parser() {
        let big = format!("{{\"pad\":\"{}\"}}", "x".repeat(MAX_ARTIFACT_BYTES));
        assert!(matches!(
            decode_untrusted_json(&big),
            Err(DecodeRefusal::Oversized { .. })
        ));
    }

    /// The depth guard is string-aware: braces INSIDE strings never
    /// count (no false refusal on brace-rich content).
    #[test]
    fn braces_inside_strings_never_count() {
        let braces = format!("{{\"note\":\"{}\"}}", "{[".repeat(200));
        assert!(decode_untrusted_json(&braces).is_ok());
    }

    /// The bound sits exactly at the constant: depth == MAX admits,
    /// MAX+1 refuses (off-by-one pinned).
    #[test]
    fn the_depth_bound_is_exact() {
        let at = format!(
            "{}1{}",
            "[".repeat(MAX_JSON_DEPTH),
            "]".repeat(MAX_JSON_DEPTH)
        );
        assert!(decode_untrusted_json(&at).is_ok(), "depth == MAX admits");
        let over = format!(
            "{}1{}",
            "[".repeat(MAX_JSON_DEPTH + 1),
            "]".repeat(MAX_JSON_DEPTH + 1)
        );
        assert!(matches!(
            decode_untrusted_json(&over),
            Err(DecodeRefusal::TooDeep { .. })
        ));
    }
}
