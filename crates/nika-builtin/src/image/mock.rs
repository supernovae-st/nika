// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The deterministic mock image provider — zero network · zero key ·
//! byte-stable forever (golden-test grade, the `mock/echo` precedent).
//!
//! Renders a seeded RGB gradient as a REAL PNG (stored-deflate zlib —
//! dependency-free, decodable by any PNG reader) so the whole pipeline
//! downstream of the provider (sniff → save → manifest) exercises
//! production paths in offline CI. Same (prompt · seed · index · size) →
//! same bytes, always.

use crate::BuiltinFailure;

use super::args::ImageArgs;
use super::types::{C_ARGS, ImageFormat, ProviderBatch, RawImage, SizeSpec, Usage};

use crate::media::png::crc32;

/// Mock renders uncompressed rows (stored deflate) — cap the edge so one
/// mock image stays well under the per-image byte ceiling.
const MOCK_MAX_EDGE: u32 = 2_048;

/// Run the mock provider for a parsed request.
pub(crate) fn generate(
    args: &ImageArgs,
    inputs: &[super::types::InputImage],
) -> Result<ProviderBatch, BuiltinFailure> {
    let (width, height) = dimensions(&args.size)?;
    let mut warnings = Vec::new();
    // In edit mode the render seeds from the FIRST input's hash so the
    // output visibly + deterministically DERIVES from the source (a real,
    // decodable, byte-stable "edit" — the mock's honesty contract).
    let edit_seed: Option<i64> = inputs.first().map(|img| {
        img.sha256.bytes().take(8).fold(0i64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(i64::from(b))
        })
    });
    let render_seed = edit_seed.or(args.seed);
    warnings.push("rehearsal · not a real image".to_owned());
    if args.format != ImageFormat::Png {
        warnings.push(format!(
            "mock_format: the mock provider always renders png — requested `{}` \
             ignored (filenames follow the actual format)",
            args.format.name()
        ));
    }
    let mut images = Vec::with_capacity(args.n as usize);
    for index in 0..args.n {
        let bytes = render_png(width, height, &args.prompt, render_seed, index);
        images.push(RawImage {
            bytes,
            revised_prompt: None,
            seed: args.seed,
        });
    }
    // Deterministic pseudo-usage (prompt-length derived) — golden-stable.
    let per_image: u64 = 1_120;
    let input = (args.prompt.chars().count() as u64).div_ceil(4).max(1);
    let output = per_image * u64::from(args.n);
    Ok(ProviderBatch {
        images,
        usage: Usage {
            input_tokens: Some(input),
            output_tokens: Some(output),
            total_tokens: Some(input + output),
            thoughts_tokens: None,
        },
        cost_usd: None,
        endpoint_host: None, // in-process — no wire was crossed
        provider_text: None,
        warnings,
        raw_debug: args.debug.then(|| {
            serde_json::json!({
                "mock": true,
                "width": width,
                "height": height,
                "note": "deterministic in-process render — no provider was called",
            })
        }),
    })
}

/// Resolve the mock render size — small by default (mock assets are
/// plumbing proofs, not production art).
fn dimensions(size: &SizeSpec) -> Result<(u32, u32), BuiltinFailure> {
    Ok(match size {
        SizeSpec::Auto => (256, 256),
        SizeSpec::Exact { width, height } => {
            if *width > MOCK_MAX_EDGE || *height > MOCK_MAX_EDGE {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    format!(
                        "the mock provider caps edges at {MOCK_MAX_EDGE}px (it renders \
                         uncompressed rows) — ask a real provider for {width}x{height}"
                    ),
                ));
            }
            (*width, *height)
        }
        SizeSpec::Aspect(ratio) => {
            // ~256 on the short side · every pair divisible by 4.
            // Exhaustive on the ENUM: a new AspectRatio variant must fail
            // compilation here, never silently render 1:1 in mock while
            // the real adapters honor it.
            use super::types::AspectRatio;
            match ratio {
                AspectRatio::Square => (256, 256),
                AspectRatio::Wide => (448, 252),
                AspectRatio::Tall => (252, 448),
                AspectRatio::Standard => (320, 240),
                AspectRatio::StandardFlip => (240, 320),
                AspectRatio::Photo => (384, 256),
                AspectRatio::PhotoFlip => (256, 384),
                AspectRatio::Cinematic => (588, 252),
            }
        }
    })
}

/// FNV-1a over the identity inputs — the deterministic pixel seed.
fn identity_hash(prompt: &str, seed: Option<i64>, index: u32) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(prompt.as_bytes());
    eat(&seed.unwrap_or(0).to_le_bytes());
    eat(&index.to_le_bytes());
    hash
}

/// Render the seeded RGB gradient as a stored-deflate PNG.
fn render_png(width: u32, height: u32, prompt: &str, seed: Option<i64>, index: u32) -> Vec<u8> {
    let hash = identity_hash(prompt, seed, index);
    #[allow(clippy::cast_possible_truncation)] // deliberate byte-mixing of the hash
    let (hx, hy, hz) = (hash as u8, (hash >> 8) as u8, (hash >> 16) as u8);
    let w_max = width.saturating_sub(1).max(1);
    let h_max = height.saturating_sub(1).max(1);

    // A visible red band on the last rows so a mock OG card cannot
    // be pasted as a real image (persona 16 · C08 leftover).
    let banner = height.saturating_mul(8).div_ceil(100).clamp(8, 24);
    // Raw scanline stream: per row, a filter byte (0 = None) + RGB triples.
    let mut raw = Vec::with_capacity((height as usize) * (1 + 3 * width as usize));
    for y in 0..height {
        raw.push(0u8);
        for x in 0..width {
            #[allow(clippy::cast_possible_truncation)] // ·255/max ≤ 255 by construction
            let (gx, gy, gd) = (
                (x * 255 / w_max) as u8,
                (y * 255 / h_max) as u8,
                (((x + y) * 255) / (w_max + h_max)) as u8,
            );
            if y + banner >= height {
                raw.push(220);
                raw.push(32);
                raw.push(32);
                continue;
            }
            raw.push(gx ^ hx);
            raw.push(gy ^ hy);
            raw.push(gd ^ hz);
        }
    }

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit · RGB · deflate · none · none
    push_chunk(&mut png, *b"IHDR", &ihdr);
    push_chunk(&mut png, *b"IDAT", &zlib_stored(&raw));
    push_chunk(&mut png, *b"IEND", &[]);
    png
}

/// Append one PNG chunk: length · type · data · CRC32(type + data).
fn push_chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    #[allow(clippy::cast_possible_truncation)] // chunk data ≪ u32::MAX by construction
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// A zlib stream of STORED (uncompressed) deflate blocks — no compressor
/// dependency, decodable everywhere, byte-stable forever.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // CMF/FLG · (0x7801 % 31 == 0)
    let mut chunks = raw.chunks(65_535).peekable();
    if raw.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xff, 0xff]); // final empty block
    }
    while let Some(chunk) = chunks.next() {
        out.push(u8::from(chunks.peek().is_none())); // BFINAL · BTYPE=00
        #[allow(clippy::cast_possible_truncation)] // chunks() caps at 65535
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

/// zlib Adler-32 over the UNCOMPRESSED stream.
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::super::args;
    use super::super::sniff::sniff;
    use super::super::types::Provider;
    use super::*;

    fn parsed(v: serde_json::Value) -> ImageArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "mock", "prompt": "a butterfly over a nebula",
            "output_dir": "./out"
        }) else {
            unreachable!()
        };
        if let serde_json::Value::Object(patch) = v {
            for (k, val) in patch {
                map.insert(k, val);
            }
        }
        args::parse(&map).expect("valid test args")
    }

    #[test]
    fn crc32_and_adler32_match_known_vectors() {
        // CRC-32 of "123456789" is the classic 0xCBF43926 check value.
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
        // Adler-32 of "Wikipedia" is the documented 0x11E60398.
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn mock_png_is_a_real_decodable_png_with_the_declared_dimensions() {
        // Decoded by the independent `png` crate (dev-dep) — proves the
        // hand-rolled encoder produces a spec-valid file, not just bytes
        // our own sniffer accepts.
        let batch = generate(&parsed(serde_json::json!({ "size": "64x48" })), &[]).expect("mock");
        let bytes = &batch.images[0].bytes;
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("valid PNG header");
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("decodable pixel data");
        assert_eq!((info.width, info.height), (64, 48));
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        // …and our own sniffer agrees.
        let sniffed = sniff(bytes).expect("sniffs");
        assert_eq!((sniffed.width, sniffed.height), (64, 48));
        assert_eq!(sniffed.format, ImageFormat::Png);
    }

    #[test]
    fn mock_is_byte_deterministic_and_seed_index_sensitive() {
        let a1 = generate(
            &parsed(serde_json::json!({ "seed": 7, "size": "32x32" })),
            &[],
        )
        .expect("mock");
        let a2 = generate(
            &parsed(serde_json::json!({ "seed": 7, "size": "32x32" })),
            &[],
        )
        .expect("mock");
        assert_eq!(
            a1.images[0].bytes, a2.images[0].bytes,
            "same inputs → same bytes"
        );
        let b = generate(
            &parsed(serde_json::json!({ "seed": 8, "size": "32x32" })),
            &[],
        )
        .expect("mock");
        assert_ne!(a1.images[0].bytes, b.images[0].bytes, "seed varies content");
        let two = generate(
            &parsed(serde_json::json!({ "seed": 7, "n": 2, "size": "32x32" })),
            &[],
        )
        .expect("mock");
        assert_eq!(two.images.len(), 2);
        assert_ne!(
            two.images[0].bytes, two.images[1].bytes,
            "index varies content (A/B variants differ)"
        );
    }

    #[test]
    fn mock_size_policy_auto_aspect_exact_and_cap() {
        let auto = generate(&parsed(serde_json::json!({})), &[]).expect("mock");
        let s = sniff(&auto.images[0].bytes).expect("sniffs");
        assert_eq!((s.width, s.height), (256, 256));
        let wide =
            generate(&parsed(serde_json::json!({ "aspect_ratio": "16:9" })), &[]).expect("mock");
        let s = sniff(&wide.images[0].bytes).expect("sniffs");
        assert_eq!((s.width, s.height), (448, 252));
        let exact = generate(&parsed(serde_json::json!({ "size": "100x40" })), &[]).expect("mock");
        let s = sniff(&exact.images[0].bytes).expect("sniffs");
        assert_eq!((s.width, s.height), (100, 40));
        let over = generate(&parsed(serde_json::json!({ "size": "4096x4096" })), &[]);
        assert!(
            matches!(over, Err(ref f) if f.code == C_ARGS && f.message.contains("2048")),
            "{over:?}"
        );
    }

    #[test]
    fn mock_batch_names_rehearsal() {
        let batch = generate(&parsed(serde_json::json!({})), &[]).expect("mock");
        assert!(
            batch
                .warnings
                .iter()
                .any(|w| w.contains("rehearsal · not a real image")),
            "{:?}",
            batch.warnings
        );
        // Stored zlib keeps the scanline RGB; a pasteable mock cannot
        // hide the red band (persona 16 · C08).
        let rgb = [220u8, 32, 32];
        assert!(
            batch.images[0].bytes.windows(3).any(|w| w == rgb),
            "mock PNG must carry a visible red banner"
        );
    }

    #[test]
    fn mock_warns_on_non_png_format_and_reports_deterministic_usage() {
        let batch = generate(&parsed(serde_json::json!({ "format": "webp" })), &[]).expect("mock");
        assert!(
            batch.warnings.iter().any(|w| w.starts_with("mock_format:")),
            "{:?}",
            batch.warnings
        );
        let usage = batch.usage;
        assert_eq!(usage.output_tokens, Some(1_120));
        assert_eq!(usage.thoughts_tokens, None);
        assert_eq!(
            usage.total_tokens,
            Some(usage.input_tokens.expect("input") + 1_120)
        );
    }

    #[test]
    fn mock_debug_carries_a_sanitized_raw_and_default_does_not() {
        let quiet = generate(&parsed(serde_json::json!({})), &[]).expect("mock");
        assert!(quiet.raw_debug.is_none());
        let debug = generate(&parsed(serde_json::json!({ "debug": true })), &[]).expect("mock");
        let raw = debug.raw_debug.expect("debug raw");
        assert_eq!(raw["mock"], true);
    }

    #[test]
    fn zlib_stored_spans_multiple_blocks_past_64k() {
        // A 200x120 RGB raw stream is ~72KB → 2 stored blocks; the PNG
        // still decodes (the block-chaining path, not just the small case).
        let batch = generate(&parsed(serde_json::json!({ "size": "200x120" })), &[]).expect("mock");
        let bytes = &batch.images[0].bytes;
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("valid");
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("decodes across blocks");
        assert_eq!((info.width, info.height), (200, 120));
    }

    #[test]
    fn provider_enum_default_matches_mock_model() {
        assert_eq!(Provider::Mock.default_model(), "mock-image-1");
    }
}
