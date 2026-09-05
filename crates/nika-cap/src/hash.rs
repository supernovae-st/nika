// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The closed argument choices for `nika:hash` (stdlib §Data).
//! Shared by literal judgment, runtime parsing and model-facing schemas.

/// A content digest algorithm supported by `nika:hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum HashAlgorithm {
    /// BLAKE3, the default content digest.
    #[default]
    Blake3,
    /// SHA-256.
    Sha256,
    /// SHA-512.
    Sha512,
}

impl HashAlgorithm {
    /// Every supported algorithm, in schema order.
    pub const ALL: [Self; 3] = [Self::Blake3, Self::Sha256, Self::Sha512];

    /// The exact, case-sensitive argument spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blake3 => "blake3",
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }

    /// Parse an explicit choice; omission is handled by the caller.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.as_str() == value)
    }
}

/// The digest output encoding supported by `nika:hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum HashEncoding {
    /// Lowercase hexadecimal, the default encoding.
    #[default]
    Hex,
    /// Standard padded base64.
    Base64,
}

impl HashEncoding {
    /// Every supported encoding, in schema order.
    pub const ALL: [Self; 2] = [Self::Hex, Self::Base64];

    /// The exact, case-sensitive argument spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Base64 => "base64",
        }
    }

    /// Parse an explicit choice; omission is handled by the caller.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.as_str() == value)
    }
}
