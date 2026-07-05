// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Magic-byte + dimension sniffing — HEADER-ONLY, by design.
//!
//! The pipeline never decodes pixels: dimensions come from the PNG IHDR /
//! JPEG SOF / WebP VP8-VP8L-VP8X headers, so a hostile « decompression
//! bomb » (a tiny file declaring enormous pixel data) costs O(header) to
//! inspect and is refused by the dimension/byte bounds before anything
//! downstream touches it. Magic bytes are the AUTHORITY over any declared
//! mime/format — a provider mislabel is a warning, a non-image payload is
//! a hard `NIKA-BUILTIN-IMAGE_GENERATE-007`.
//!
//! Every access is bounds-checked (`get`) — arbitrary bytes must never
//! panic (property-tested).

use crate::BuiltinFailure;

use super::types::{C_VALIDATE, ImageFormat};

/// Per-image decoded-payload ceiling. The http plane already caps a whole
/// response at 64 MiB; this bounds a single decoded image so one artifact
/// cannot exhaust the batch budget.
pub(crate) const MAX_IMAGE_BYTES: usize = 50 * 1024 * 1024;
/// Dimension sanity window — providers top out at 4K-class output; 16384
/// leaves generous headroom while refusing absurd declared dimensions.
const MAX_DIMENSION: u32 = 16_384;

/// What the headers say about an image payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sniffed {
    pub(crate) format: ImageFormat,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Identify + measure an image payload from its headers.
pub(crate) fn sniff(bytes: &[u8]) -> Result<Sniffed, BuiltinFailure> {
    if bytes.is_empty() {
        return Err(BuiltinFailure::new(
            C_VALIDATE,
            "provider returned an empty image payload",
        ));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(BuiltinFailure::new(
            C_VALIDATE,
            format!(
                "image payload is {} bytes — over the {MAX_IMAGE_BYTES}-byte per-image \
                 ceiling",
                bytes.len()
            ),
        ));
    }
    let sniffed = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png_dimensions(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg_dimensions(bytes)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        webp_dimensions(bytes)
    } else {
        return Err(BuiltinFailure::new(
            C_VALIDATE,
            "payload is not a PNG/JPEG/WebP image (magic bytes do not match any \
             supported format)",
        ));
    };
    let sniffed = sniffed.ok_or_else(|| {
        BuiltinFailure::new(
            C_VALIDATE,
            "could not read the image dimensions — truncated or malformed header",
        )
    })?;
    if sniffed.width == 0
        || sniffed.height == 0
        || sniffed.width > MAX_DIMENSION
        || sniffed.height > MAX_DIMENSION
    {
        return Err(BuiltinFailure::new(
            C_VALIDATE,
            format!(
                "image declares {}x{} — outside the sane 1..={MAX_DIMENSION} window",
                sniffed.width, sniffed.height
            ),
        ));
    }
    Ok(sniffed)
}

/// PNG: the IHDR chunk is mandatory-first — width/height are the two
/// big-endian u32s right after the 8-byte signature + 8-byte chunk header.
fn png_dimensions(bytes: &[u8]) -> Option<Sniffed> {
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    Some(Sniffed {
        format: ImageFormat::Png,
        width: u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?),
        height: u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?),
    })
}

/// JPEG: walk the marker stream to the first `SOFn` frame header (the
/// C0..CF family minus DHT C4 · JPG C8 · DAC CC), which carries
/// `precision(1) height(2be) width(2be)` after its length.
fn jpeg_dimensions(bytes: &[u8]) -> Option<Sniffed> {
    let mut i = 2usize; // past FFD8
    while i + 1 < bytes.len() {
        if *bytes.get(i)? != 0xff {
            return None; // marker stream desync — malformed
        }
        // Fill bytes: consecutive FFs pad between markers.
        let mut j = i + 1;
        while *bytes.get(j)? == 0xff {
            j += 1;
        }
        let marker = *bytes.get(j)?;
        match marker {
            // Standalone markers (no length): RST0-7 · SOI · TEM.
            0xd0..=0xd7 | 0xd8 | 0x01 => {
                i = j + 1;
            }
            // EOI or SOS before any SOF — no frame header to read.
            0xd9 | 0xda => return None,
            // The SOF family carrying dimensions.
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                return Some(Sniffed {
                    format: ImageFormat::Jpeg,
                    height: u32::from(u16::from_be_bytes(
                        bytes.get(j + 4..j + 6)?.try_into().ok()?,
                    )),
                    width: u32::from(u16::from_be_bytes(
                        bytes.get(j + 6..j + 8)?.try_into().ok()?,
                    )),
                });
            }
            // Every other segment: skip by its declared length.
            _ => {
                let len = u16::from_be_bytes(bytes.get(j + 1..j + 3)?.try_into().ok()?);
                if len < 2 {
                    return None;
                }
                i = j + 1 + usize::from(len);
            }
        }
    }
    None
}

/// WebP: dispatch on the first chunk fourcc — lossy `VP8 ` (frame tag +
/// sync code + 14-bit dims), lossless `VP8L` (packed 14-bit minus-one
/// dims), extended `VP8X` (24-bit minus-one canvas dims).
fn webp_dimensions(bytes: &[u8]) -> Option<Sniffed> {
    let make = |width, height| {
        Some(Sniffed {
            format: ImageFormat::Webp,
            width,
            height,
        })
    };
    match bytes.get(12..16)? {
        b"VP8 " => {
            // Key-frame layout: 3-byte frame tag @20, sync 9D 01 2A @23,
            // then u16le width/height (low 14 bits each).
            if bytes.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            let w = u16::from_le_bytes(bytes.get(26..28)?.try_into().ok()?) & 0x3fff;
            let h = u16::from_le_bytes(bytes.get(28..30)?.try_into().ok()?) & 0x3fff;
            make(u32::from(w), u32::from(h))
        }
        b"VP8L" => {
            // Signature byte 0x2F @20, then 28 bits of (w-1, h-1) LE-packed.
            if *bytes.get(20)? != 0x2f {
                return None;
            }
            let packed = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
            make((packed & 0x3fff) + 1, ((packed >> 14) & 0x3fff) + 1)
        }
        b"VP8X" => {
            // Canvas size: u24le minus-one pairs at offsets 24 and 27.
            let u24 = |s: &[u8]| -> Option<u32> {
                Some(
                    u32::from(*s.first()?)
                        | u32::from(*s.get(1)?) << 8
                        | u32::from(*s.get(2)?) << 16,
                )
            };
            make(u24(bytes.get(24..27)?)? + 1, u24(bytes.get(27..30)?)? + 1)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A minimal valid PNG header for the given dimensions (signature +
    /// IHDR prefix — enough for the sniffer, not a full file).
    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 2, 0, 0, 0]); // depth · color · misc
        v
    }

    /// A minimal JPEG: SOI · APP0 stub · SOF0 with dims · (truncated).
    fn jpeg_header(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0xff, 0xd8]; // SOI
        v.extend_from_slice(&[0xff, 0xe0, 0x00, 0x04, 0x4a, 0x46]); // APP0 len 4
        v.extend_from_slice(&[0xff, 0xc0, 0x00, 0x0b, 0x08]); // SOF0 len 11 · precision
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]); // 1 component stub
        v
    }

    fn webp_lossy_header(w: u16, h: u16) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&40u32.to_le_bytes());
        v.extend_from_slice(b"WEBPVP8 ");
        v.extend_from_slice(&24u32.to_le_bytes()); // chunk size
        v.extend_from_slice(&[0x30, 0x01, 0x00]); // frame tag
        v.extend_from_slice(&[0x9d, 0x01, 0x2a]); // sync code
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v
    }

    fn webp_lossless_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&20u32.to_le_bytes());
        v.extend_from_slice(b"WEBPVP8L");
        v.extend_from_slice(&10u32.to_le_bytes());
        v.push(0x2f);
        let packed = (w - 1) | ((h - 1) << 14);
        v.extend_from_slice(&packed.to_le_bytes());
        v
    }

    fn webp_extended_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&30u32.to_le_bytes());
        v.extend_from_slice(b"WEBPVP8X");
        v.extend_from_slice(&10u32.to_le_bytes());
        v.extend_from_slice(&[0, 0, 0, 0]); // flags + reserved
        let wm = w - 1;
        let hm = h - 1;
        #[allow(clippy::cast_possible_truncation)]
        v.extend_from_slice(&[wm as u8, (wm >> 8) as u8, (wm >> 16) as u8]);
        #[allow(clippy::cast_possible_truncation)]
        v.extend_from_slice(&[hm as u8, (hm >> 8) as u8, (hm >> 16) as u8]);
        v
    }

    #[test]
    fn png_jpeg_webp_headers_measure_correctly() {
        assert_eq!(
            sniff(&png_header(1536, 864)).expect("png"),
            Sniffed {
                format: ImageFormat::Png,
                width: 1536,
                height: 864
            }
        );
        assert_eq!(
            sniff(&jpeg_header(1024, 768)).expect("jpeg"),
            Sniffed {
                format: ImageFormat::Jpeg,
                width: 1024,
                height: 768
            }
        );
        assert_eq!(
            sniff(&webp_lossy_header(800, 600)).expect("webp lossy"),
            Sniffed {
                format: ImageFormat::Webp,
                width: 800,
                height: 600
            }
        );
        assert_eq!(
            sniff(&webp_lossless_header(1200, 630)).expect("webp lossless"),
            Sniffed {
                format: ImageFormat::Webp,
                width: 1200,
                height: 630
            }
        );
        assert_eq!(
            sniff(&webp_extended_header(2048, 1152)).expect("webp extended"),
            Sniffed {
                format: ImageFormat::Webp,
                width: 2048,
                height: 1152
            }
        );
    }

    #[test]
    fn non_image_payloads_are_007() {
        for bad in [
            &b""[..],
            b"GIF89a rest",
            b"<html>not an image</html>",
            b"\x89PNG\r\n\x1a", // truncated signature
            b"{\"error\": \"json\"}",
        ] {
            let err = sniff(bad).expect_err("refused");
            assert_eq!(err.code, C_VALIDATE, "{bad:?}");
        }
    }

    #[test]
    fn truncated_headers_are_007_not_panics() {
        let png = png_header(100, 100);
        for cut in [9, 12, 15, 17, 21] {
            assert!(sniff(&png[..cut]).is_err(), "cut at {cut}");
        }
        let jpeg = jpeg_header(100, 100);
        for cut in [3, 5, 9, 12] {
            assert!(sniff(&jpeg[..cut]).is_err(), "cut at {cut}");
        }
        let webp = webp_lossy_header(100, 100);
        for cut in [11, 13, 19, 25] {
            assert!(sniff(&webp[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn absurd_dimensions_are_refused() {
        // A 60000x60000 declared PNG (the decompression-bomb shape) is
        // refused from the HEADER — no pixel work happened to find out.
        let bomb = png_header(60_000, 60_000);
        let err = sniff(&bomb).expect_err("bomb refused");
        assert!(err.message.contains("16384"), "{}", err.message);
        assert!(sniff(&png_header(0, 100)).is_err(), "zero width");
    }

    #[test]
    fn oversized_payload_is_refused_before_parsing() {
        let mut huge = png_header(64, 64);
        huge.resize(MAX_IMAGE_BYTES + 1, 0);
        let err = sniff(&huge).expect_err("too big");
        assert!(err.message.contains("ceiling"), "{}", err.message);
    }

    #[test]
    fn jpeg_without_sof_or_desynced_stream_is_refused() {
        // SOI then EOI — no frame header.
        assert!(sniff(&[0xff, 0xd8, 0xff, 0xd9]).is_err());
        // SOI then garbage (not a marker).
        assert!(sniff(&[0xff, 0xd8, 0x00, 0x00, 0x00]).is_err());
        // Segment declaring an impossible length (<2).
        assert!(sniff(&[0xff, 0xd8, 0xff, 0xe0, 0x00, 0x01]).is_err());
    }

    proptest! {
        /// The sniffer NEVER panics — arbitrary bytes (including ones
        /// starting with valid magics then garbage) always return Ok/Err.
        #[test]
        fn sniff_total_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let _ = sniff(&bytes);
        }

        /// Ditto with each valid magic prefix grafted on random tails —
        /// exercises the per-format parsers' bounds checks specifically.
        #[test]
        fn sniff_total_on_magic_prefixed_tails(
            prefix in prop::sample::select(vec![
                b"\x89PNG\r\n\x1a\n".to_vec(),
                vec![0xff, 0xd8],
                b"RIFF\x28\x00\x00\x00WEBP".to_vec(),
            ]),
            tail in proptest::collection::vec(any::<u8>(), 0..256),
        ) {
            let mut bytes = prefix;
            bytes.extend(tail);
            let _ = sniff(&bytes);
        }

        /// Round-trip: a synthesized header for any in-window dimension
        /// pair measures back exactly.
        #[test]
        fn header_roundtrip(w in 1u32..4096, h in 1u32..4096) {
            let png = sniff(&png_header(w, h)).expect("png");
            prop_assert_eq!((png.width, png.height), (w, h));
            let wl = sniff(&webp_lossless_header(w, h)).expect("vp8l");
            prop_assert_eq!((wl.width, wl.height), (w, h));
            let wx = sniff(&webp_extended_header(w, h)).expect("vp8x");
            prop_assert_eq!((wx.width, wx.height), (w, h));
            #[allow(clippy::cast_possible_truncation)]
            let (jw, jh) = (w.min(65_535) as u16, h.min(65_535) as u16);
            let jp = sniff(&jpeg_header(jw, jh)).expect("jpeg");
            prop_assert_eq!((jp.width, jp.height), (u32::from(jw), u32::from(jh)));
        }
    }
}
