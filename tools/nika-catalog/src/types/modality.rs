// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Modality enum for model input/output capability declaration.
//!
//! Introduced in Session 2b to replace the coarse `supports_vision: bool`
//! with a precise per-modality list. Session 3 decommissions `supports_vision`
//! entirely in favour of `input_modalities.contains(Modality::Image)`.

/// A data modality a model can consume (input) or produce (output).
///
/// **Declaration order = `Ord` order = emitted slice order** (enforced by
/// `build.rs`). Slices stored in [`crate::types::ModelCapabilities`] are
/// sorted by declaration order so `binary_search` using the derived `Ord`
/// works correctly on generated slices.
///
/// `#[non_exhaustive]` — new modalities (e.g. structured JSON, embedding
/// vectors) may be added without a major version bump. Downstream match
/// statements must include a `_ =>` arm.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Modality {
    /// Plain text — always present in `input_modalities` for chat-capable models.
    Text,
    /// Raster images — PNG, JPEG, WEBP, GIF.
    Image,
    /// Audio input/output (speech, music, sound effects).
    Audio,
    /// Video (frames + optional audio track). Distinct from a sequence of
    /// images in that providers accept this modality as a single binary
    /// asset with its own frame-rate metadata.
    Video,
    /// Native PDF documents — processed by the model end-to-end, not OCR'd
    /// by the caller. Confirmed on Anthropic Claude (all active models) and
    /// Google Gemini 2.5 Pro.
    Pdf,
}

impl Modality {
    /// Canonical kebab-case string representation.
    ///
    /// This value is what appears in `data/model-capabilities.toml` (e.g.
    /// `input_modalities = ["text", "image"]`) and what
    /// [`Modality`]'s `serde` impl produces.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Pdf => "pdf",
        }
    }
}

impl core::fmt::Display for Modality {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failure when parsing a [`Modality`] from its canonical string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseModalityError {
    /// The raw input that failed to match.
    pub input: String,
}

impl ParseModalityError {
    /// Explicit constructor — required because the struct is
    /// `#[non_exhaustive]` (invariant #19).
    #[must_use]
    pub const fn new(input: String) -> Self {
        Self { input }
    }
}

impl core::fmt::Display for ParseModalityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "unknown modality {:?} — expected one of: text, image, audio, video, pdf",
            self.input
        )
    }
}

impl std::error::Error for ParseModalityError {}

impl core::str::FromStr for Modality {
    type Err = ParseModalityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Self::Text),
            "image" => Ok(Self::Image),
            "audio" => Ok(Self::Audio),
            "video" => Ok(Self::Video),
            "pdf" => Ok(Self::Pdf),
            _ => Err(ParseModalityError::new(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Modality, ParseModalityError};
    use core::str::FromStr;

    #[test]
    fn as_str_roundtrip_for_every_variant() {
        for m in [
            Modality::Text,
            Modality::Image,
            Modality::Audio,
            Modality::Video,
            Modality::Pdf,
        ] {
            let s = m.as_str();
            let parsed = Modality::from_str(s).expect("round-trip must succeed");
            assert_eq!(parsed, m, "as_str({m:?}) → {s:?} did not round-trip");
        }
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", Modality::Text), "text");
        assert_eq!(format!("{}", Modality::Image), "image");
        assert_eq!(format!("{}", Modality::Pdf), "pdf");
    }

    #[test]
    fn ord_matches_declaration_order() {
        assert!(Modality::Text < Modality::Image);
        assert!(Modality::Image < Modality::Audio);
        assert!(Modality::Audio < Modality::Video);
        assert!(Modality::Video < Modality::Pdf);
    }

    #[test]
    fn from_str_unknown_returns_error_with_input() {
        let err = Modality::from_str("hologram").unwrap_err();
        assert_eq!(err, ParseModalityError::new("hologram".to_string()));
        let msg = format!("{err}");
        assert!(msg.contains("hologram"), "error must echo input: {msg}");
        assert!(msg.contains("text"), "error must list expected values: {msg}");
    }

    #[test]
    fn from_str_is_case_sensitive() {
        // TOML rules emit lowercase only; uppercase must fail to catch
        // authoring typos at build time rather than resolving silently.
        assert!(Modality::from_str("Text").is_err());
        assert!(Modality::from_str("PDF").is_err());
    }

    #[test]
    fn copy_and_hash_are_derived() {
        fn assert_copy<T: Copy>() {}
        fn assert_hash<T: core::hash::Hash>() {}
        assert_copy::<Modality>();
        assert_hash::<Modality>();
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_kebab_case() {
        let m = Modality::Image;
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, "\"image\"");
        let back: Modality = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
