// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! In-file provenance — a `nika` tEXt chunk embedded into saved PNGs.
//!
//! The sidecar manifest answers « where does this asset come from? » —
//! until the file is COPIED somewhere and the sidecar stays behind. The
//! image-native world solved this years ago (`ComfyUI` embeds the workflow
//! JSON · `InvokeAI` its `invokeai_metadata` · A1111 its parameters — all
//! in PNG text chunks); no general workflow engine does it. This module
//! closes that: the DETERMINISTIC provenance core (tool · engine version
//! · provider · model · clamped prompt · seed) rides a `nika` tEXt chunk
//! inserted right after `IHDR`, so provenance survives `cp`.
//!
//! Deliberately DETERMINISTIC (no timestamp — the manifest carries time):
//! the mock provider's byte-stable golden contract holds through the
//! whole save path. Pure ASCII payload (`\uXXXX`-escaped JSON), so the
//! Latin-1 tEXt constraint is satisfied by construction. PNG only —
//! JPEG/WebP have no equally-universal text container (documented, not
//! silently faked).

use super::args::ImageArgs;

/// The prompt clamp inside the chunk — provenance, not a payload channel
/// (the full prompt lives in the sidecar manifest).
const PROMPT_CHARS: usize = 500;

/// Embed the `nika` tEXt provenance chunk into a PNG payload, right
/// after the IHDR chunk. Returns the input unchanged when it is not a
/// well-formed PNG prefix (defense — callers already sniffed it).
pub(crate) fn embed_provenance(bytes: Vec<u8>, args: &ImageArgs, seed: Option<i64>) -> Vec<u8> {
    // signature(8) + IHDR length(4) + "IHDR"(4) + data(13) + crc(4) = 33.
    const IHDR_END: usize = 33;
    if bytes.len() < IHDR_END || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return bytes;
    }
    let payload = provenance_json(args, seed);
    let mut chunk_data = Vec::with_capacity(5 + payload.len());
    chunk_data.extend_from_slice(b"nika");
    chunk_data.push(0); // keyword / text separator
    chunk_data.extend_from_slice(payload.as_bytes());

    let mut out = Vec::with_capacity(bytes.len() + chunk_data.len() + 12);
    out.extend_from_slice(&bytes[..IHDR_END]);
    push_text_chunk(&mut out, &chunk_data);
    out.extend_from_slice(&bytes[IHDR_END..]);
    out
}

/// Append one tEXt chunk (`length · "tEXt" · data · crc32("tEXt"+data)`).
fn push_text_chunk(out: &mut Vec<u8>, data: &[u8]) {
    #[allow(clippy::cast_possible_truncation)] // ≤ ~600 bytes by construction
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(b"tEXt");
    out.extend_from_slice(data);
    let mut crc_input = b"tEXt".to_vec();
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&super::mock::crc32(&crc_input).to_be_bytes());
}

/// The deterministic provenance core as pure-ASCII JSON (`\uXXXX`-escaped
/// so the Latin-1 tEXt constraint holds for ANY prompt).
fn provenance_json(args: &ImageArgs, seed: Option<i64>) -> String {
    let clamped: String = args.prompt.chars().take(PROMPT_CHARS).collect();
    let json = serde_json::json!({
        "tool": "nika:image_generate",
        "engine": env!("CARGO_PKG_VERSION"),
        "provider": args.provider.id(),
        "model": args.model,
        "prompt": clamped,
        "seed": seed,
    })
    .to_string();
    json.chars()
        .flat_map(|c| {
            if c.is_ascii() {
                c.to_string().chars().collect::<Vec<_>>()
            } else {
                let mut buf = [0u16; 2];
                c.encode_utf16(&mut buf)
                    .iter()
                    .flat_map(|u| format!("\\u{u:04x}").chars().collect::<Vec<_>>())
                    .collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::args;
    use super::super::sniff::sniff;
    use super::*;

    fn parsed(v: serde_json::Value) -> ImageArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "mock", "prompt": "a butterfly over a nebula 🦋 — café",
            "output_dir": "./out"
        }) else {
            unreachable!()
        };
        if let serde_json::Value::Object(p) = v {
            for (k, val) in p {
                map.insert(k, val);
            }
        }
        args::parse(&map).expect("valid")
    }

    fn mock_png() -> Vec<u8> {
        let batch = super::super::mock::generate(&parsed(serde_json::json!({
            "size": "32x24", "seed": 7
        })))
        .expect("mock");
        batch.images.into_iter().next().expect("one").bytes
    }

    #[test]
    fn embedded_chunk_survives_an_independent_decoder_and_the_sniffer() {
        let original = mock_png();
        let embedded = embed_provenance(original.clone(), &parsed(serde_json::json!({})), Some(7));
        assert!(embedded.len() > original.len(), "the chunk was inserted");
        // Our own sniffer still measures the IHDR untouched.
        let sniffed = sniff(&embedded).expect("sniffs");
        assert_eq!((sniffed.width, sniffed.height), (32, 24));
        // The independent `png` crate decodes pixels AND reads the chunk.
        let decoder = png::Decoder::new(std::io::Cursor::new(&embedded));
        let mut reader = decoder.read_info().expect("valid PNG");
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        reader.next_frame(&mut buf).expect("decodable pixels");
        let text = &reader.info().uncompressed_latin1_text;
        let chunk = text
            .iter()
            .find(|c| c.keyword == "nika")
            .expect("the nika tEXt chunk is present");
        let parsed_json: serde_json::Value =
            serde_json::from_str(&chunk.text).expect("chunk text is valid JSON");
        assert_eq!(parsed_json["tool"], "nika:image_generate");
        assert_eq!(parsed_json["provider"], "mock");
        assert_eq!(parsed_json["seed"], 7);
        assert!(
            parsed_json["prompt"]
                .as_str()
                .expect("prompt")
                .contains("butterfly"),
            "the clamped prompt rides along"
        );
        // The unicode survived the ASCII escape round-trip.
        assert!(parsed_json["prompt"].as_str().expect("s").contains('🦋'));
        assert!(
            chunk.text.is_ascii(),
            "tEXt payload is pure ASCII (Latin-1 safe)"
        );
    }

    #[test]
    fn embedding_is_deterministic_and_non_png_is_untouched() {
        let a = embed_provenance(mock_png(), &parsed(serde_json::json!({})), None);
        let b = embed_provenance(mock_png(), &parsed(serde_json::json!({})), None);
        assert_eq!(a, b, "no timestamp — golden-stable by design");
        let jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0];
        assert_eq!(
            embed_provenance(jpeg.clone(), &parsed(serde_json::json!({})), None),
            jpeg,
            "non-PNG payloads pass through untouched"
        );
        let truncated = b"\x89PNG\r\n\x1a\n".to_vec();
        assert_eq!(
            embed_provenance(truncated.clone(), &parsed(serde_json::json!({})), None),
            truncated,
            "a bare signature is left alone (defense)"
        );
    }

    #[test]
    fn long_prompts_clamp_inside_the_chunk() {
        let long = "x".repeat(2_000);
        let args = parsed(serde_json::json!({ "prompt": long }));
        let json = provenance_json(&args, None);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid");
        assert_eq!(
            value["prompt"].as_str().expect("s").chars().count(),
            PROMPT_CHARS
        );
    }
}
