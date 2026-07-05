// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared vocabulary of the `nika:tts_generate` module family (stdlib
//! §Audio) — the closed enums the arg parser produces and every provider
//! adapter consumes, plus the spec error codes.

/// 001 — invalid arguments (empty text · over-cap · bad enum · mask-class
/// refusals have no analogue here).
pub(crate) const C_ARGS: &str = "NIKA-BUILTIN-TTS_GENERATE-001";
/// 002 — provider unavailable (missing API key · tts plane not wired).
pub(crate) const C_PROVIDER_UNAVAILABLE: &str = "NIKA-BUILTIN-TTS_GENERATE-002";
/// 003 — provider request failed (transport · HTTP status · rate limit).
/// Transient per the spec status table (5xx/408/429 + timeout/connection).
pub(crate) const C_REQUEST: &str = "NIKA-BUILTIN-TTS_GENERATE-003";
/// 004 — the provider returned no audio / a malformed audio payload.
pub(crate) const C_NO_AUDIO: &str = "NIKA-BUILTIN-TTS_GENERATE-004";
/// 005 — content policy block.
pub(crate) const C_POLICY: &str = "NIKA-BUILTIN-TTS_GENERATE-005";
/// 006 — saving the audio or the manifest failed.
pub(crate) const C_SAVE: &str = "NIKA-BUILTIN-TTS_GENERATE-006";
/// 007 — decoded audio failed validation (magic bytes · empty payload).
pub(crate) const C_VALIDATE: &str = "NIKA-BUILTIN-TTS_GENERATE-007";

/// The v1 TTS providers — a closed set, sovereign path first (the image
/// family's Rule-3 lesson applied from day one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provider {
    /// A user-run LOCAL `OpenAI`-speech-compatible server (`LocalAI` ·
    /// Kokoro-FastAPI · Speaches · openedai-speech — one wire). The base
    /// URL is ENGINE CONFIG (`NIKA_TTS_LOCAL_URL`), never workflow data.
    Local,
    /// `OpenAI` Audio API (`POST /v1/audio/speech`).
    Openai,
    /// `ElevenLabs` (`POST /v1/text-to-speech/{voice_id}`).
    Elevenlabs,
    /// Deterministic in-process mock — a real WAV, zero network, zero key.
    Mock,
}

impl Provider {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Openai => "openai",
            Self::Elevenlabs => "elevenlabs",
            Self::Mock => "mock",
        }
    }

    /// Research-verified defaults (2026-07).
    pub(crate) const fn default_model(self) -> &'static str {
        match self {
            // The LocalAI convention (its bundled voice models register
            // under simple names; other compat servers differ — set
            // `model:` explicitly alongside your base URL).
            Self::Local => "tts-1",
            Self::Openai => "gpt-4o-mini-tts",
            Self::Elevenlabs => "eleven_multilingual_v2",
            Self::Mock => "mock-tts-1",
        }
    }

    /// The default voice per provider (a REQUIRED axis on every real
    /// wire — `alloy` is `OpenAI`'s documented default · Rachel is
    /// `ElevenLabs`' public default voice id).
    pub(crate) const fn default_voice(self) -> &'static str {
        match self {
            Self::Local | Self::Openai => "alloy",
            Self::Elevenlabs => "21m00Tcm4TlvDq8ikWAM",
            Self::Mock => "sine",
        }
    }
}

/// Output audio containers (closed · `format:` arg).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioFormat {
    Mp3,
    Wav,
}

impl AudioFormat {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
        }
    }

    pub(crate) const fn mime(self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::Wav => "audio/wav",
        }
    }

    pub(crate) const fn extension(self) -> &'static str {
        self.name()
    }
}

/// What a provider adapter returns for one synthesis call.
#[derive(Debug, Default)]
pub(crate) struct ProviderAudio {
    pub(crate) bytes: Vec<u8>,
    /// Real spend in USD when computable exactly — v1 providers are
    /// subscription/token-priced without a response cost field → `None`
    /// (honest · the shape matches the image family for the ledger seam).
    pub(crate) cost_usd: Option<f64>,
    /// The host the synthesis came from (provenance — load-bearing for
    /// `local`). `None` for the in-process mock.
    pub(crate) endpoint_host: Option<String>,
    pub(crate) warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canon_ids_defaults_and_formats_hold() {
        for (p, id) in [
            (Provider::Local, "local"),
            (Provider::Openai, "openai"),
            (Provider::Elevenlabs, "elevenlabs"),
            (Provider::Mock, "mock"),
        ] {
            assert_eq!(p.id(), id);
            assert!(!p.default_model().is_empty());
            assert!(!p.default_voice().is_empty());
        }
        assert_eq!(AudioFormat::Mp3.mime(), "audio/mpeg");
        assert_eq!(AudioFormat::Wav.extension(), "wav");
        assert!(C_ARGS.ends_with("-001") && C_VALIDATE.ends_with("-007"));
    }
}
