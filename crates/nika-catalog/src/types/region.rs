// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cloud deployment region for scope-gated capability and pricing rules.
//!
//! Used by [`super::capabilities::CapRule`] to restrict a rule to a
//! specific geographic area. `None` on the rule = global (no region
//! restriction). Phase E2 pricing migration uses this for Azure/Bedrock
//! regional rate differentiation.

use core::fmt;

/// Cloud deployment region.
///
/// # Example
///
/// ```
/// use nika_catalog::types::Region;
///
/// let r: Region = "eu".parse().unwrap();
/// assert_eq!(r, Region::Eu);
/// assert_eq!(r.as_str(), "eu");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Region {
    /// United States (us-east, us-west, etc.)
    Us,
    /// European Union / EEA
    Eu,
    /// Asia-Pacific
    Apac,
    /// Mainland China (often requires separate endpoint)
    China,
}

impl Region {
    /// Wire-format string for this region.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Us => "us",
            Self::Eu => "eu",
            Self::Apac => "apac",
            Self::China => "china",
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when parsing an unknown region string.
#[derive(Debug, Clone)]
pub struct ParseRegionError(pub String);

impl fmt::Display for ParseRegionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown region: {:?}", self.0)
    }
}

impl std::error::Error for ParseRegionError {}

impl core::str::FromStr for Region {
    type Err = ParseRegionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "us" => Ok(Self::Us),
            "eu" => Ok(Self::Eu),
            "apac" => Ok(Self::Apac),
            "china" => Ok(Self::China),
            _ => Err(ParseRegionError(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_all_variants() {
        for (s, expected) in [
            ("us", Region::Us),
            ("eu", Region::Eu),
            ("apac", Region::Apac),
            ("china", Region::China),
        ] {
            let parsed: Region = s.parse().unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), s);
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn unknown_region_errors() {
        let err = "antarctica".parse::<Region>().unwrap_err();
        assert!(err.to_string().contains("antarctica"));
    }

    #[test]
    fn display_matches_as_str() {
        for r in [Region::Us, Region::Eu, Region::Apac, Region::China] {
            assert_eq!(r.to_string(), r.as_str());
        }
    }
}
