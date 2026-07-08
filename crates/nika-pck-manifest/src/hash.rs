// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! 32-byte digests as validated lowercase-hex newtypes.
//!
//! Integrity primitives are NEVER total: a malformed hash is a hard
//! [`ManifestError::HashInvalid`] at parse — the exact opposite posture of
//! the taxonomy's totality, on purpose (a router routes; a trust anchor
//! refuses).

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ManifestError;

/// Validate + canonicalize a 64-hex-char digest (case-insensitive in,
/// lowercase stored — one canonical form so hash equality is string
/// equality everywhere downstream).
fn canon_hex64(s: &str, what: &'static str) -> Result<String, ManifestError> {
    if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(s.to_ascii_lowercase())
    } else {
        Err(ManifestError::HashInvalid {
            what,
            got: s.to_owned(),
        })
    }
}

macro_rules! hash_newtype {
    ($(#[$doc:meta])* $name:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[non_exhaustive]
        pub struct $name(String);

        impl $name {
            /// Parse + canonicalize (lowercase) a 64-hex-char digest.
            ///
            /// # Errors
            /// [`ManifestError::HashInvalid`] unless `s` is exactly 64 hex chars.
            pub fn new(s: &str) -> Result<Self, ManifestError> {
                canon_hex64(s, $what).map(Self)
            }

            /// The canonical lowercase-hex form.
            #[must_use]
            pub fn as_hex(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = ManifestError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::new(s)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = ManifestError;
            fn try_from(s: &str) -> Result<Self, Self::Error> {
                Self::new(s)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                ser.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                let s = String::deserialize(de)?;
                Self::new(&s).map_err(serde::de::Error::custom)
            }
        }
    };
}

hash_newtype!(
    /// A sha256 digest (per-file integrity rows · the `nika-pack` precedent).
    Sha256Hash,
    "sha256"
);
hash_newtype!(
    /// A blake3 digest (the artifact's IDENTITY · ADR-094 D1: an artifact
    /// IS its content hash; `nika-blob` computes, this type carries).
    Blake3Hash,
    "blake3"
);

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = "a3f5b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b";

    #[test]
    fn accepts_64_hex_and_canonicalizes_to_lowercase() {
        let lower = Blake3Hash::new(OK).unwrap();
        assert_eq!(lower.as_hex(), OK);
        let upper = Blake3Hash::new(&OK.to_ascii_uppercase()).unwrap();
        assert_eq!(upper, lower, "case-insensitive in, ONE canonical form");
        assert_eq!(upper.to_string(), OK, "Display is the lowercase form");
    }

    #[test]
    fn rejects_wrong_length_and_non_hex() {
        for (bad, why) in [
            (&OK[..63], "63 chars"),
            (
                "g3f5b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b",
                "non-hex g",
            ),
            ("", "empty"),
        ] {
            let err = Sha256Hash::new(bad).unwrap_err();
            assert!(
                matches!(err, ManifestError::HashInvalid { what: "sha256", ref got } if got == bad),
                "{why}: {err:?}"
            );
        }
        let long = format!("{OK}0");
        assert!(Sha256Hash::new(&long).is_err(), "65 chars");
    }

    #[test]
    fn serde_validates_on_the_way_in_and_emits_canonical_out() {
        let json = format!("\"{}\"", OK.to_ascii_uppercase());
        let h: Sha256Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&h).unwrap(), format!("\"{OK}\""));
        // a malformed digest is a hard serde error (never total)
        assert!(serde_json::from_str::<Sha256Hash>("\"beef\"").is_err());
    }

    #[test]
    fn error_display_names_the_digest_kind_and_input() {
        let msg = Blake3Hash::new("nope").unwrap_err().to_string();
        assert_eq!(msg, "invalid blake3: `nope` is not a 64-hex-char digest");
    }
}
