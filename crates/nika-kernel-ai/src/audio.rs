// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Audio traits — async speech-to-text · text-to-speech · voice-activity
//! detection (the R6 modality seam).
//!
//! Seam reservation per the 2026-06-10 Olympus capability-parity audit
//! (`nika/02-engineering/architecture/blueprint/2026-06-10-olympus-
//! capability-parity-and-algo-research.md` §2 R6 · audio was the ONE
//! modality with zero trait / zero builtin / zero roadmap line while
//! vision · screen · ocr · a11y · input all had seams). Contracts land
//! NOW so stdlib v0.x audio impls arrive ADDITIVELY (no breaking change
//! · forever-v0.x discipline) · L1 impls land post-announce (whisper /
//! kokoro / piper class via `candle` or in-process runtimes · the
//! Olympus voice pipeline is the working prototype per the build-twice
//! asymmetric pattern D-2026-06-01-N2 · Olympus prototypes · Nika
//! crafts · ZERO code crosses per cross-flow D-2026-05-08-N1).
//!
//! Reserved error codes **NIKA-1600..1699** (next free L1 family range
//! after the ADR-081 computer-use block 1000..1599 · ledger row added
//! to `docs/architecture/forward-compat-invariants.md` Gate 12 same
//! commit).
//!
//! ADR-006 monolithic-kernel-spirit (traits + DTOs · no proc macro ·
//! no async runtime dep). ADR-016 ISP discipline (1 trait = 1
//! capability · STT ⊥ TTS ⊥ VAD are 3 distinct concerns · L1 impls
//! split per backend). ADR-037 bottom-up layer (L0.5 traits-only ·
//! zero tokio · zero whisper / kokoro / candle deps here).
//!
//! Canonical payload · `AudioClip` carries **PCM s16le interleaved**
//! samples in zero-copy `bytes::Bytes` (sister of `io::screen::Frame`'s
//! canonical RGBA8 · ONE wire shape so STT ↔ TTS ↔ VAD compose without
//! per-backend format coercion · resampling / transcoding is an L1
//! concern at the edge). `Transcript` segments carry millisecond
//! timestamps + per-segment confidence + BCP-47 language tags (cohérent
//! `io::ocr::TextRegion::language` precedent).

use serde::{Deserialize, Serialize};

/// Canonical audio payload · PCM s16le interleaved in zero-copy bytes.
///
/// `data` length MUST equal `frames × channels × 2` bytes (s16le) ·
/// L1 impls validate at the boundary and reject malformed clips before
/// inference. `sample_rate_hz` is the playback/capture rate (16 000 ·
/// 24 000 · 44 100 · 48 000 typical) · `channels` is the interleave
/// count (1 = mono · 2 = stereo · STT impls MAY downmix internally).
/// `timestamp_ms` anchors the clip on the capture clock (epoch
/// milliseconds · 0 for synthesized clips with no real-time anchor).
///
/// `#[non_exhaustive]` keeps the struct extensible (future fields land
/// additively · e.g. `encoding` enum when a second wire format is
/// justified · `device_id` for multi-mic capture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AudioClip {
    /// PCM s16le interleaved samples · zero-copy.
    pub data: bytes::Bytes,
    /// Sample rate in hertz (16 000 · 24 000 · 44 100 · 48 000 typical).
    pub sample_rate_hz: u32,
    /// Interleaved channel count · 1 = mono · 2 = stereo.
    pub channels: u16,
    /// Capture anchor · epoch milliseconds · 0 when synthesized.
    pub timestamp_ms: u64,
}

impl AudioClip {
    /// Construct a new audio clip.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(data: bytes::Bytes, sample_rate_hz: u32, channels: u16, timestamp_ms: u64) -> Self {
        Self {
            data,
            sample_rate_hz,
            channels,
            timestamp_ms,
        }
    }

    /// Frame count implied by the payload length (frames = samples per
    /// channel). Returns 0 for degenerate clips (zero channels — which
    /// L1 boundary validation rejects · the guard here keeps the
    /// arithmetic total).
    #[must_use]
    pub fn frames(&self) -> usize {
        let stride = usize::from(self.channels) * 2;
        if stride == 0 {
            return 0;
        }
        self.data.len() / stride
    }
}

/// One time-aligned transcript segment.
///
/// `confidence` is a probability score in `[0.0, 1.0]` (L1 impls SHOULD
/// clamp upstream values · cohérent `vision::BoundingBox::confidence` +
/// `io::ocr::TextRegion::confidence`). `PartialEq` only (no `Eq`) per
/// DEV-1 · `f32` lacks total equality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TranscriptSegment {
    /// Recognized text for this segment.
    pub text: String,
    /// Segment start offset · milliseconds from clip start.
    pub start_ms: u64,
    /// Segment end offset · milliseconds from clip start.
    pub end_ms: u64,
    /// Model confidence score in `[0.0, 1.0]`.
    pub confidence: f32,
}

impl TranscriptSegment {
    /// Construct a new transcript segment.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(text: String, start_ms: u64, end_ms: u64, confidence: f32) -> Self {
        Self {
            text,
            start_ms,
            end_ms,
            confidence,
        }
    }
}

/// Speech-to-text response · full text + time-aligned segments.
///
/// `text` is the full concatenated transcript (always populated ·
/// empty string for silent clips). `segments` carry per-utterance
/// timing + confidence · empty when the L1 impl runs a fast path with
/// no alignment pass. `language` is the detected (or hinted) BCP-47
/// tag · `None` when the backend does not report one (cohérent
/// `io::ocr::TextRegion::language` precedent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Transcript {
    /// Full concatenated transcript · empty for silent clips.
    pub text: String,
    /// Time-aligned segments · empty when no alignment pass ran.
    pub segments: Vec<TranscriptSegment>,
    /// Detected or hinted language · BCP-47 · `None` when unreported.
    pub language: Option<String>,
}

impl Transcript {
    /// Construct a new transcript record.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(text: String, segments: Vec<TranscriptSegment>, language: Option<String>) -> Self {
        Self {
            text,
            segments,
            language,
        }
    }
}

/// One detected speech region (voice-activity detection output).
///
/// Offsets are milliseconds from clip start · `confidence` in
/// `[0.0, 1.0]` per the canonical confidence convention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SpeechSegment {
    /// Region start offset · milliseconds from clip start.
    pub start_ms: u64,
    /// Region end offset · milliseconds from clip start.
    pub end_ms: u64,
    /// Detector confidence score in `[0.0, 1.0]`.
    pub confidence: f32,
}

impl SpeechSegment {
    /// Construct a new speech-segment record.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(start_ms: u64, end_ms: u64, confidence: f32) -> Self {
        Self {
            start_ms,
            end_ms,
            confidence,
        }
    }
}

/// Speech-to-text trait · async transcription of a captured clip.
///
/// CANCEL SAFETY: cancel-safe · transcription is read-only inference
/// over an `AudioClip` and produces no side effects. L1 impls SHOULD
/// wrap synchronous runtimes (whisper.cpp · candle-whisper) in
/// `spawn_blocking` + cancel-token shims · dropping the future abandons
/// the inference · partial transcripts MUST NOT leak to the caller.
///
/// `#[trait_variant::make(SpeechToTextDyn: Send)]` generates the `Send`
/// companion for generic constraints (cohérent `vision::VisionModelDyn`
/// + the `io::*Dyn` family · companion futures are `impl Future + Send`
/// · NOT dyn-compatible · intentional · L1 impls wrap via `Arc<T>`).
#[trait_variant::make(SpeechToTextDyn: Send)]
pub trait SpeechToText: Send + Sync {
    /// Transcribe a clip · optional BCP-47 language hint.
    ///
    /// `language_hint: None` lets the backend auto-detect · `Some` pins
    /// the decode language for backends that support it (backends
    /// without language pinning MAY ignore the hint · the response
    /// `language` field reports what was actually used/detected).
    ///
    /// CANCEL SAFETY: cancel-safe (read-only inference · no side
    /// effects · partial transcripts MUST NOT leak on cancel).
    async fn transcribe(
        &self,
        clip: &AudioClip,
        language_hint: Option<&str>,
    ) -> std::io::Result<Transcript>;
}

/// Text-to-speech trait · async synthesis to the canonical PCM clip.
///
/// CANCEL SAFETY: cancel-safe · synthesis writes nothing outside the
/// returned clip. L1 impls SHOULD wrap synchronous runtimes (kokoro ·
/// piper class) in `spawn_blocking` + cancel-token shims · dropping
/// the future abandons synthesis · partial audio MUST NOT leak.
#[trait_variant::make(TextToSpeechDyn: Send)]
pub trait TextToSpeech: Send + Sync {
    /// Synthesize `text` with the backend voice id `voice`.
    ///
    /// `voice` is a backend-declared identifier (model card taxonomy ·
    /// e.g. a kokoro voice pack id) · empty string requests the backend
    /// default voice. The returned clip is canonical PCM s16le at the
    /// backend's native sample rate (callers resample at the edge when
    /// a target rate is required).
    ///
    /// CANCEL SAFETY: cancel-safe (no side effects · partial audio
    /// MUST NOT leak on cancel).
    async fn synthesize(&self, text: &str, voice: &str) -> std::io::Result<AudioClip>;
}

/// Voice-activity-detection trait · async speech-region segmentation.
///
/// CANCEL SAFETY: cancel-safe · detection is read-only analysis over
/// an `AudioClip` · no side effects · partial segment lists MUST NOT
/// leak on cancel. VAD backends are typically cheap synchronous DSP
/// (silero class) · L1 impls still wrap in `spawn_blocking` when the
/// model graph is non-trivial.
#[trait_variant::make(VoiceActivityDyn: Send)]
pub trait VoiceActivity: Send + Sync {
    /// Detect speech regions in a clip.
    ///
    /// Returns time-ordered non-overlapping segments above the
    /// detector's confidence threshold · empty `Vec` means the clip is
    /// silence (or all regions fell below threshold).
    ///
    /// CANCEL SAFETY: cancel-safe (read-only analysis · no side
    /// effects).
    async fn detect_speech(&self, clip: &AudioClip) -> std::io::Result<Vec<SpeechSegment>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn synthetic_clip() -> AudioClip {
        // 4 frames of stereo s16le silence · 4 × 2 ch × 2 bytes = 16 bytes.
        AudioClip::new(Bytes::from(vec![0u8; 16]), 16_000, 2, 1_700_000_000_000)
    }

    #[test]
    fn audio_clip_serde_roundtrip() {
        let clip = synthetic_clip();
        let json = serde_json::to_string(&clip).expect("serialize clip");
        let back: AudioClip = serde_json::from_str(&json).expect("deserialize clip");
        assert_eq!(back, clip);
    }

    #[test]
    fn audio_clip_frames_arithmetic() {
        let clip = synthetic_clip();
        assert_eq!(clip.frames(), 4, "16 bytes / (2 ch × 2 bytes) = 4 frames");

        let mono = AudioClip::new(Bytes::from(vec![0u8; 16]), 16_000, 1, 0);
        assert_eq!(mono.frames(), 8, "16 bytes / (1 ch × 2 bytes) = 8 frames");

        // Degenerate zero-channel clip · guard returns 0 (boundary
        // validation rejects these upstream · arithmetic stays total).
        let degenerate = AudioClip::new(Bytes::from(vec![0u8; 16]), 16_000, 0, 0);
        assert_eq!(degenerate.frames(), 0);
    }

    #[test]
    fn transcript_roundtrip_with_segments() {
        let t = Transcript::new(
            "hello world".to_string(),
            vec![
                TranscriptSegment::new("hello".to_string(), 0, 480, 0.97),
                TranscriptSegment::new("world".to_string(), 520, 990, 0.93),
            ],
            Some("en".to_string()),
        );
        let json = serde_json::to_string(&t).expect("serialize transcript");
        let back: Transcript = serde_json::from_str(&json).expect("deserialize transcript");
        assert_eq!(back, t);
        assert_eq!(back.segments.len(), 2);
        assert_eq!(back.language.as_deref(), Some("en"));
    }

    #[test]
    fn transcript_silent_clip_shape() {
        // Silent clips · full text empty · zero segments · no language.
        let t = Transcript::new(String::new(), vec![], None);
        assert!(t.text.is_empty());
        assert!(t.segments.is_empty());
        assert!(t.language.is_none());
    }

    #[test]
    fn speech_segment_constructor_matches_literal() {
        let from_new = SpeechSegment::new(100, 900, 0.88);
        let from_lit = SpeechSegment {
            start_ms: 100,
            end_ms: 900,
            confidence: 0.88,
        };
        assert_eq!(from_new, from_lit);
    }

    /// DEV-2 generic-bound compile check · ensures the 3 trait shapes
    /// are usable as generic constraints via their `Send` companions.
    /// The inner functions never run · the type-check is the assertion.
    /// Cohérent the `vision::VisionModelDyn` + `io::*Dyn` pattern.
    #[test]
    fn audio_traits_generic_bound_compile_check() {
        fn _accepts_stt<T: SpeechToTextDyn>(_: &T) {}
        fn _accepts_tts<T: TextToSpeechDyn>(_: &T) {}
        fn _accepts_vad<T: VoiceActivityDyn>(_: &T) {}
    }

    /// Sanity-check the synthetic clip builder used across test cases.
    #[test]
    fn synthetic_clip_constructs_cleanly() {
        let clip = synthetic_clip();
        assert_eq!(clip.sample_rate_hz, 16_000);
        assert_eq!(clip.channels, 2);
    }
}
