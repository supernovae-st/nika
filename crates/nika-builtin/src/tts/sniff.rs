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
///
/// The `data` length is the bytes actually PRESENT: a streamed WAV writes
/// its header before it knows the size and declares the `0xFFFFFFFF`
/// placeholder (`OpenAI`'s streamed narrations · `espeak-ng --stdout`), which
/// read verbatim was a 24.8-hour duration (gauntlet C-D5). A declared size
/// is clamped to what follows the chunk header, never trusted past it.
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
            let present = bytes.len() - (off + 8);
            data_len = Some(len.min(present) as u64);
        }
        // Checked end to end: the placeholder `len` overflows `8 + len` on a
        // 32-bit `usize`, and a chunk that runs past `usize` has nothing after it.
        let Some(next) = off
            .checked_add(8)
            .and_then(|o| o.checked_add(len))
            .and_then(|o| o.checked_add(len & 1))
        else {
            break;
        };
        off = next;
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

    /// A 16-bit mono PCM WAV whose RIFF and `data` sizes are `declared`
    /// (the streamed form writes the `0xFFFFFFFF` placeholder) over `pcm`.
    fn wav_with_declared(rate: u32, declared: u32, pcm: &[u8]) -> Vec<u8> {
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&declared.to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
        w.extend_from_slice(&2u16.to_le_bytes()); // block align
        w.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        w.extend_from_slice(b"data");
        w.extend_from_slice(&declared.to_le_bytes());
        w.extend_from_slice(pcm);
        w
    }

    /// Gauntlet C-D5: `OpenAI`'s streamed WAV declares `0xFFFFFFFF` and the
    /// duration read 89478485 ms (24.8 h). The bytes present are the truth.
    #[test]
    fn a_streamed_placeholder_size_counts_only_the_bytes_present() {
        // 24 kHz · 16-bit mono · 0.5 s of audio actually present.
        let pcm = vec![0u8; 24_000];
        let streamed = wav_with_declared(24_000, u32::MAX, &pcm);
        let s = sniff(&streamed).expect("wav");
        assert_eq!(s.format, AudioFormat::Wav);
        assert_eq!(
            s.duration_ms,
            Some(500),
            "the 24000 bytes present, never 0xFFFFFFFF"
        );
        // A truthful header keeps its exact math.
        let honest = wav_with_declared(24_000, 24_000, &pcm);
        assert_eq!(sniff(&honest).expect("wav").duration_ms, Some(500));
        // A declared size SMALLER than the payload stays the declared one
        // (trailing chunks or padding never inflate the duration).
        let mut tail = wav_with_declared(24_000, 12_000, &pcm);
        tail.extend_from_slice(b"LIST\x04\x00\x00\x00abcd");
        assert_eq!(sniff(&tail).expect("wav").duration_ms, Some(250));
    }

    /// macOS `say` writes an `FLLR` filler chunk between `fmt ` and `data`:
    /// the walk steps over it to the real `data` chunk.
    #[test]
    fn a_filler_chunk_before_data_is_stepped_over() {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF\x00\x00\x00\x00WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&22_050u32.to_le_bytes());
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"FLLR");
        wav.extend_from_slice(&6u32.to_le_bytes());
        wav.extend_from_slice(&[0u8; 6]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&vec![0u8; 44_100]);
        assert_eq!(sniff(&wav).expect("wav").duration_ms, Some(1_000));
    }
}
