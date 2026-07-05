// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The deterministic in-process TTS mock — a REAL, playable WAV file
//! (16-bit mono PCM · 16 kHz · a text-seeded sine sweep) with zero
//! network and zero keys, mirroring the image mock's honesty: real
//! decodable bytes, byte-stable for goldens.

use super::args::TtsArgs;
use super::types::ProviderAudio;

const SAMPLE_RATE: u32 = 16_000;

/// Render the mock WAV for the parsed args (duration scales with text
/// length · pitch seeds from the text so different inputs sound — and
/// hash — different).
pub(crate) fn generate(args: &TtsArgs) -> ProviderAudio {
    ProviderAudio {
        bytes: render_wav(&args.text, args.speed_permille()),
        cost_usd: None,
        endpoint_host: None, // in-process — no wire was crossed
        warnings: Vec::new(),
    }
}

/// Pure WAV synthesis (also the sniffer's test fixture).
pub(crate) fn render_wav(text: &str, speed_permille: u32) -> Vec<u8> {
    // ~55ms per character, clamped to [0.4s, 10s], scaled by speed.
    let base_ms = (text.chars().count() as u64 * 55).clamp(400, 10_000);
    let ms = (base_ms * 1000 / u64::from(speed_permille.max(250))).clamp(200, 20_000);
    #[allow(clippy::cast_possible_truncation)] // ≤ 20_000 by clamp
    let samples = (u64::from(SAMPLE_RATE) * ms / 1000) as u32;

    // Text-seeded fundamental in a speech-ish band (110–330 Hz).
    let seed = text.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let f0 = 110.0 + f64::from(seed % 220);

    let mut pcm = Vec::with_capacity(samples as usize * 2);
    for i in 0..samples {
        let t = f64::from(i) / f64::from(SAMPLE_RATE);
        // A slow sweep + a quiet third harmonic → obviously synthetic,
        // clearly audible, never mistaken for real speech.
        let sweep = f0 * (1.0 + 0.15 * (t * 1.3).sin());
        let v = (t * sweep * std::f64::consts::TAU).sin() * 0.6
            + (t * sweep * 3.0 * std::f64::consts::TAU).sin() * 0.15;
        // Fade the edges to avoid clicks.
        let total = f64::from(samples) / f64::from(SAMPLE_RATE);
        let env = (t / 0.02)
            .min(1.0)
            .min(((total - t) / 0.02).min(1.0))
            .max(0.0);
        #[allow(clippy::cast_possible_truncation)] // |v·env| ≤ 0.75 → in i16 range
        let s = (v * env * f64::from(i16::MAX) * 0.8) as i16;
        pcm.extend_from_slice(&s.to_le_bytes());
    }

    let byte_rate = SAMPLE_RATE * 2;
    #[allow(clippy::cast_possible_truncation)] // ≤ 640_000 by the sample clamp
    let data_len = pcm.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits/sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&pcm);
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_wav_is_deterministic_text_sensitive_and_speed_aware() {
        let a = render_wav("bonjour le monde", 1000);
        let b = render_wav("bonjour le monde", 1000);
        assert_eq!(a, b, "byte-stable");
        assert_ne!(a, render_wav("hello world", 1000), "text-seeded");
        let fast = render_wav("bonjour le monde", 2000);
        assert!(fast.len() < a.len(), "speed shortens");
        assert_eq!(&a[..4], b"RIFF");
    }
}
