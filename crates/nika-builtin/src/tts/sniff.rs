// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Audio magic-byte sniffing — the same authority model as the image
//! family: what the bytes SAY beats what the provider declared. WAV
//! additionally yields an exact duration from its header (no decode);
//! MP3 duration would need frame-walking → honestly `None` in v1.

use crate::BuiltinFailure;

use super::types::{AudioFormat, C_VALIDATE};

/// What the magic bytes established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sniffed {
    pub(crate) format: AudioFormat,
    /// Exact for WAV (header math) · `None` for MP3 (never a guess).
    pub(crate) duration_ms: Option<u64>,
}

/// Identify the payload or fail `-007` (a non-audio payload is a hard
/// error — the provider contract broke).
pub(crate) fn sniff(bytes: &[u8]) -> Result<Sniffed, BuiltinFailure> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Ok(Sniffed {
            format: AudioFormat::Wav,
            duration_ms: wav_duration_ms(bytes),
        });
    }
    // MP3: an ID3v2 tag, or a bare MPEG frame sync (0xFF Ex/Fx).
    let mp3 = bytes.len() >= 3
        && (&bytes[..3] == b"ID3" || (bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0));
    if mp3 {
        return Ok(Sniffed {
            format: AudioFormat::Mp3,
            duration_ms: None,
        });
    }
    Err(BuiltinFailure::new(
        C_VALIDATE,
        format!(
            "the provider returned a payload that is not WAV or MP3 ({} bytes · starts {:02x?})",
            bytes.len(),
            &bytes[..bytes.len().min(4)]
        ),
    ))
}

/// Duration from the `fmt ` byte-rate and the `data` chunk length —
/// pure header math, total (any malformed chunk → `None`, never a panic).
fn wav_duration_ms(bytes: &[u8]) -> Option<u64> {
    let mut off = 12usize;
    let mut byte_rate: Option<u64> = None;
    let mut data_len: Option<u64> = None;
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let len = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().ok()?) as usize;
        if id == b"fmt " && off + 8 + 16 <= bytes.len() {
            byte_rate = Some(u64::from(u32::from_le_bytes(
                bytes[off + 16..off + 20].try_into().ok()?,
            )));
        }
        if id == b"data" {
            data_len = Some(len as u64);
        }
        off = off.checked_add(8 + len + (len & 1))?;
    }
    let (rate, data) = (byte_rate?, data_len?);
    if rate == 0 {
        return None;
    }
    Some(data.saturating_mul(1000) / rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_mp3_and_junk_sniff_correctly() {
        let wav = super::super::mock::render_wav("hello", 42);
        let s = sniff(&wav).expect("wav");
        assert_eq!(s.format, AudioFormat::Wav);
        let ms = s.duration_ms.expect("header math");
        assert!((400..=2000).contains(&ms), "plausible duration: {ms}");

        let id3 = sniff(b"ID3\x04rest").expect("id3");
        assert_eq!((id3.format, id3.duration_ms), (AudioFormat::Mp3, None));
        let frame = sniff(&[0xFF, 0xFB, 0x90, 0x00]).expect("frame sync");
        assert_eq!(frame.format, AudioFormat::Mp3);

        assert!(sniff(b"").is_err(), "empty refused");
        assert!(sniff(b"PK\x03\x04zip").is_err(), "junk refused");
        assert!(
            sniff(b"RIFF\x00\x00\x00\x00AVI ").is_err(),
            "AVI is not WAVE"
        );
    }
}
