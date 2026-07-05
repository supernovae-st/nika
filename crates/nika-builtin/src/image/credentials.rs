// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Content-credentials DETECTION — the provenance signals upstream
//! generators embed in the bytes they return (C2PA "Content Credentials"
//! · the CAI/Adobe/OpenAI ecosystem standard).
//!
//! No workflow engine even LOOKS at these octets: re-encodes and naive
//! pipelines strip them silently, which is exactly how provenance dies
//! between the generator and the published asset. Nika's contract is the
//! opposite and three-fold:
//!
//! 1. **DETECT** — conservative, exact-signature byte scans (a PNG `caBX`
//!    chunk · a JPEG APP11 JUMBF segment · a WebP `C2PA` RIFF chunk).
//! 2. **PRESERVE** — C2PA hard bindings hash the file's byte ranges, so
//!    inserting OUR `nika` tEXt chunk into a signed PNG would INVALIDATE
//!    the upstream signature. When credentials are detected, the embed
//!    step stands down (their signed manifest outranks our informal
//!    chunk) and the sidecar manifest records both facts.
//! 3. **SURFACE** — `content_credentials: "detected" | "none"` rides the
//!    output, the manifest and a dedicated event. Honesty rule: this is
//!    presence DETECTION, never VALIDATION — the label never says
//!    « verified » (offline verification is the reserved
//!    `nika-media-provenance` crate's future).

/// What the byte scan established for one image payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CredentialSignal {
    /// A C2PA manifest store is present (format-specific carriage).
    C2pa,
}

impl CredentialSignal {
    /// The stable wire label (output · manifest · events).
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::C2pa => "c2pa",
        }
    }
}

/// Scan one image payload for embedded content credentials. Conservative
/// by design: exact container signatures only — a miss means « none
/// SEEN », never « none present » (the docs say "detected", not
/// "verified", for the same honesty reason).
pub(crate) fn detect(bytes: &[u8]) -> Option<CredentialSignal> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png_has_cabx(bytes).then_some(CredentialSignal::C2pa);
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return jpeg_has_app11_jumbf(bytes).then_some(CredentialSignal::C2pa);
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return riff_has_c2pa_chunk(bytes).then_some(CredentialSignal::C2pa);
    }
    None
}

/// Scan one AUDIO payload for embedded content credentials (WAV rides
/// RIFF `C2PA` chunks · MP3 rides an `ID3v2` `GEOB` frame with the
/// `application/c2pa` MIME — the carriage Google's Lyria and the
/// `ElevenLabs` compliance stack ship today).
pub(crate) fn detect_audio(bytes: &[u8]) -> Option<CredentialSignal> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return riff_has_c2pa_chunk(bytes).then_some(CredentialSignal::C2pa);
    }
    if bytes.starts_with(b"ID3") {
        return id3_has_c2pa_geob(bytes).then_some(CredentialSignal::C2pa);
    }
    None
}

/// `ID3v2`: a `GEOB` frame whose payload names the `application/c2pa`
/// MIME type, searched within the declared tag bounds only.
fn id3_has_c2pa_geob(bytes: &[u8]) -> bool {
    if bytes.len() < 10 {
        return false;
    }
    // syncsafe 28-bit tag size at offset 6.
    let size = bytes[6..10]
        .iter()
        .fold(0usize, |acc, b| (acc << 7) | usize::from(b & 0x7F));
    let end = 10usize.saturating_add(size).min(bytes.len());
    let tag = &bytes[10..end];
    let has = |token: &[u8]| tag.windows(token.len()).any(|w| w == token);
    has(b"GEOB") && has(b"application/c2pa")
}

/// Walk PNG chunks for `caBX` (the C2PA-in-PNG carriage chunk).
fn png_has_cabx(bytes: &[u8]) -> bool {
    let mut off = 8usize;
    while off + 8 <= bytes.len() {
        let Some(len_bytes) = bytes
            .get(off..off + 4)
            .and_then(|s| <[u8; 4]>::try_from(s).ok())
        else {
            return false;
        };
        let len = u32::from_be_bytes(len_bytes) as usize;
        let Some(kind) = bytes.get(off + 4..off + 8) else {
            return false;
        };
        if kind == b"caBX" {
            return true;
        }
        if kind == b"IEND" {
            return false;
        }
        // length + type + data + crc
        let Some(next) = off.checked_add(12 + len) else {
            return false;
        };
        off = next;
    }
    false
}

/// Walk JPEG segments for APP11 (0xFFEB) carrying a JUMBF box — the
/// C2PA-in-JPEG carriage (JPEG XT box format · the box type reads
/// `jumb` after the `JP` CI header).
fn jpeg_has_app11_jumbf(bytes: &[u8]) -> bool {
    let mut off = 2usize; // past SOI
    while off + 4 <= bytes.len() {
        if bytes[off] != 0xFF {
            return false; // lost sync — stop scanning, never guess
        }
        let marker = bytes[off + 1];
        // Standalone markers without a length.
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            off += 2;
            continue;
        }
        let Some(len_bytes) = bytes
            .get(off + 2..off + 4)
            .and_then(|s| <[u8; 2]>::try_from(s).ok())
        else {
            return false;
        };
        let seg_len = u16::from_be_bytes(len_bytes) as usize;
        if seg_len < 2 {
            return false;
        }
        if marker == 0xEB {
            // APP11 payload: "JP" CI + box instance/sequence, then a JUMBF
            // superbox (`jumb`) labeled `c2pa` (spec 2.4 carriage — both
            // tokens must appear · exact-signature conservatism).
            let seg = bytes.get(off + 4..off + 2 + seg_len).unwrap_or(&[]);
            let has = |token: &[u8]| seg.windows(token.len()).any(|w| w == token);
            if seg.starts_with(b"JP") && has(b"jumb") && has(b"c2pa") {
                return true;
            }
        }
        if marker == 0xDA {
            return false; // entropy-coded data begins — credentials live before SOS
        }
        let Some(next) = off.checked_add(2 + seg_len) else {
            return false;
        };
        off = next;
    }
    false
}

/// Walk RIFF chunks (`WebP` · WAV · AVI) for a `C2PA` fourcc.
fn riff_has_c2pa_chunk(bytes: &[u8]) -> bool {
    let mut off = 12usize;
    while off + 8 <= bytes.len() {
        let Some(fourcc) = bytes.get(off..off + 4) else {
            return false;
        };
        if fourcc == b"C2PA" {
            return true;
        }
        let Some(len_bytes) = bytes
            .get(off + 4..off + 8)
            .and_then(|s| <[u8; 4]>::try_from(s).ok())
        else {
            return false;
        };
        let len = u32::from_le_bytes(len_bytes) as usize;
        // chunks are padded to even sizes
        let Some(next) = off.checked_add(8 + len + (len & 1)) else {
            return false;
        };
        off = next;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PNG with one extra chunk of the given type spliced after
    /// IHDR (CRC deliberately zero — detection reads types, not CRCs).
    fn png_with_chunk(kind: [u8; 4]) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        // IHDR (13 bytes of data · zero CRC — fine for the walker)
        out.extend_from_slice(&13u32.to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&[0u8; 13]);
        out.extend_from_slice(&[0u8; 4]);
        // the probe chunk
        let payload = b"anything";
        #[allow(clippy::cast_possible_truncation)] // tiny test fixture
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&kind);
        out.extend_from_slice(payload);
        out.extend_from_slice(&[0u8; 4]);
        // IEND
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&[0u8; 4]);
        out
    }

    #[test]
    fn png_cabx_detected_and_clean_png_is_none() {
        assert_eq!(
            detect(&png_with_chunk(*b"caBX")),
            Some(CredentialSignal::C2pa)
        );
        assert_eq!(
            detect(&png_with_chunk(*b"tEXt")),
            None,
            "our own chunk is not C2PA"
        );
        let mock = super::super::mock::generate(&{
            let serde_json::Value::Object(map) = serde_json::json!({
                "provider": "mock", "prompt": "clean", "output_dir": "./out"
            }) else {
                unreachable!()
            };
            super::super::args::parse(&map).expect("valid")
        })
        .expect("mock");
        assert_eq!(
            detect(&mock.images[0].bytes),
            None,
            "mock renders are unsigned"
        );
    }

    #[test]
    fn jpeg_app11_jumbf_detected_and_plain_jpeg_is_none() {
        // SOI + APP11("JP" + filler + "jumb") + SOS-ish tail
        let mut jpeg = vec![0xFF, 0xD8];
        let mut seg = b"JP".to_vec();
        seg.extend_from_slice(&[0, 1, 0, 0, 0, 1]); // CI boxes filler
        seg.extend_from_slice(&[0, 0, 0, 32]);
        seg.extend_from_slice(b"jumb");
        seg.extend_from_slice(&[0, 0, 0, 17]);
        seg.extend_from_slice(b"jumdc2pa"); // the c2pa-labeled description box
        #[allow(clippy::cast_possible_truncation)] // tiny test fixture
        let seg_len = (seg.len() + 2) as u16;
        jpeg.extend_from_slice(&[0xFF, 0xEB]);
        jpeg.extend_from_slice(&seg_len.to_be_bytes());
        jpeg.extend_from_slice(&seg);
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0, 2]);
        assert_eq!(detect(&jpeg), Some(CredentialSignal::C2pa));

        let plain = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xDA, 0, 2];
        assert_eq!(detect(&plain), None);
        // an APP11 that is NOT JUMBF (no "JP"/"jumb") stays none — exact
        // signatures only.
        let mut odd = vec![0xFF, 0xD8, 0xFF, 0xEB, 0, 6];
        odd.extend_from_slice(b"XXXX");
        assert_eq!(detect(&odd), None);
    }

    /// The 28-bit syncsafe size encoding (test fixture helper).
    fn syncsafe(n: usize) -> [u8; 4] {
        #[allow(clippy::cast_possible_truncation)] // 7-bit masked
        [
            ((n >> 21) & 0x7F) as u8,
            ((n >> 14) & 0x7F) as u8,
            ((n >> 7) & 0x7F) as u8,
            (n & 0x7F) as u8,
        ]
    }

    #[test]
    fn audio_wav_riff_and_mp3_geob_detected() {
        // WAV with a C2PA RIFF chunk.
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&40u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"C2PA");
        wav.extend_from_slice(&4u32.to_le_bytes());
        wav.extend_from_slice(b"data");
        assert_eq!(detect_audio(&wav), Some(CredentialSignal::C2pa));

        // A clean mock WAV is none.
        let clean = super::super::super::tts::mock::render_wav("clean", 1000);
        assert_eq!(detect_audio(&clean), None);

        // MP3 with an ID3v2 GEOB frame carrying application/c2pa.
        let mut body = b"GEOB".to_vec();
        body.extend_from_slice(&[0, 0, 0, 40, 0, 0, 0]);
        body.extend_from_slice(b"application/c2pa\0payload");
        let mut mp3 = b"ID3\x04\x00\x00".to_vec();
        mp3.extend_from_slice(&syncsafe(body.len()));
        mp3.extend_from_slice(&body);
        assert_eq!(detect_audio(&mp3), Some(CredentialSignal::C2pa));

        // A GEOB with a different MIME is none.
        let mut other = b"ID3\x04\x00\x00".to_vec();
        let body2 = b"GEOB\x00\x00\x00\x10\x00\x00\x00image/png\0x";
        other.extend_from_slice(&syncsafe(body2.len()));
        other.extend_from_slice(body2);
        assert_eq!(detect_audio(&other), None);
        assert_eq!(detect_audio(b"ID3"), None, "truncated tag is total");
    }

    #[test]
    fn webp_c2pa_chunk_detected_and_truncation_is_total() {
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&40u32.to_le_bytes());
        webp.extend_from_slice(b"WEBP");
        webp.extend_from_slice(b"C2PA");
        webp.extend_from_slice(&4u32.to_le_bytes());
        webp.extend_from_slice(b"data");
        assert_eq!(detect(&webp), Some(CredentialSignal::C2pa));

        // Truncated/adversarial payloads never panic, never false-positive.
        for junk in [
            &b""[..],
            &b"\x89PNG\r\n\x1a\n"[..],
            &[0xFF, 0xD8, 0xFF][..],
            &b"RIFF\x00\x00WEBP"[..],
        ] {
            assert_eq!(detect(junk), None);
        }
        // A PNG whose chunk length overflows must stop cleanly.
        let mut evil = b"\x89PNG\r\n\x1a\n".to_vec();
        evil.extend_from_slice(&u32::MAX.to_be_bytes());
        evil.extend_from_slice(b"IHDR");
        assert_eq!(detect(&evil), None);
    }
}
