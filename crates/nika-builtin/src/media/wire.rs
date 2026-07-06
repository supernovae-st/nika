// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared HTTP-wire scaffold for the media provider adapters — the parts
//! that were copied verbatim across image/{openai,gemini,local,xai} and
//! tts/{openai,local,elevenlabs} (architect review P1.1 · the 7× set).
//!
//! Re-homing only: the exact behavior every adapter had, named ONCE.
//! The transient matrix is a SPEC-NORMATIVE
//! rule (« Transient per the spec status table · 5xx/408/429 ») — inlined
//! seven times is seven places to miss when the spec's table changes.

/// Is this HTTP status transient (worth a retry)? The spec's status table:
/// 5xx server errors + 408 Request Timeout + 429 Too Many Requests. THE
/// single authority — `retry.on_codes` correctness rides on it matching
/// the spec across every media provider.
pub(crate) fn transient_status(status: u16) -> bool {
    matches!(status, 500..=599 | 408 | 429)
}

/// One field of a `multipart/form-data` body — a text value or a named
/// file part (the edit wires: openai/local `/v1/images/edits`).
pub(crate) enum Part<'a> {
    Text {
        name: &'a str,
        value: &'a str,
    },
    File {
        name: &'a str,
        filename: &'a str,
        mime: &'a str,
        bytes: &'a [u8],
    },
}

/// A fixed, collision-safe multipart boundary. It only has to not occur
/// inside the parts; a studio-constant token is fine (payloads are image
/// bytes + short fields — never this exact ASCII run), and a constant
/// keeps request bytes deterministic (golden-friendly · no RNG).
const MULTIPART_BOUNDARY: &str = "----nika-media-boundary-7f3a9c2e";

/// Build a `multipart/form-data` body + the matching `content-type`
/// header value. Parts are emitted in the given order — callers control
/// ordering (xai's edit wire documents `file` LAST).
pub(crate) fn multipart(parts: &[Part<'_>]) -> (Vec<u8>, String) {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{MULTIPART_BOUNDARY}\r\n").as_bytes());
        match part {
            Part::Text { name, value } => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(value.as_bytes());
            }
            Part::File {
                name,
                filename,
                mime,
                bytes,
            } => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(format!("Content-Type: {mime}\r\n\r\n").as_bytes());
                body.extend_from_slice(bytes);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{MULTIPART_BOUNDARY}--\r\n").as_bytes());
    (
        body,
        format!("multipart/form-data; boundary={MULTIPART_BOUNDARY}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_frames_text_and_file_parts_with_the_boundary() {
        let (body, ct) = multipart(&[
            Part::Text {
                name: "model",
                value: "gpt-image-2",
            },
            Part::File {
                name: "image[]",
                filename: "src.png",
                mime: "image/png",
                bytes: b"\x89PNG",
            },
        ]);
        assert!(ct.contains("boundary=----nika-media-boundary"));
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("name=\"model\"\r\n\r\ngpt-image-2"));
        assert!(s.contains("filename=\"src.png\"") && s.contains("Content-Type: image/png"));
        assert!(s.trim_end().ends_with("--"), "closing delimiter");
        // raw file bytes ride verbatim (multipart files are NOT base64).
        assert!(body.windows(4).any(|w| w == b"\x89PNG"));
    }

    #[test]
    fn transient_matches_the_spec_status_table() {
        for s in [500, 502, 503, 599, 408, 429] {
            assert!(transient_status(s), "{s} is transient");
        }
        for s in [200, 400, 401, 403, 404, 422] {
            assert!(!transient_status(s), "{s} is terminal");
        }
    }
}
