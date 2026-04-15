// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Content-addressable storage hash types.

use std::fmt;

use serde::{Deserialize, Serialize};

/// BLAKE3 hash (32 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Blake3Hash {
    /// Raw 32-byte hash.
    pub bytes: [u8; 32],
}

impl Blake3Hash {
    /// Create from raw bytes.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Zero hash.
    #[must_use]
    pub fn zero() -> Self {
        Self { bytes: [0; 32] }
    }
}

impl fmt::Display for Blake3Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blake3:")?;
        for byte in &self.bytes {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for Blake3Hash {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Blake3Hash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let hex = s
            .strip_prefix("blake3:")
            .ok_or_else(|| serde::de::Error::custom("expected 'blake3:' prefix"))?;
        if hex.len() != 64 {
            return Err(serde::de::Error::custom("blake3 hash must be 64 hex chars"));
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let h = std::str::from_utf8(chunk)
                .map_err(|e| serde::de::Error::custom(format!("invalid utf8: {e}")))?;
            bytes[i] = u8::from_str_radix(h, 16)
                .map_err(|e| serde::de::Error::custom(format!("invalid hex: {e}")))?;
        }
        Ok(Self { bytes })
    }
}

/// Content digest string (algorithm-prefixed, e.g. `"sha256:abcd..."`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ContentDigest {
    /// The full digest string.
    pub value: String,
}

impl ContentDigest {
    /// Create a new content digest.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

/// Blob reference string (CAS key).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BlobRef {
    /// The reference value.
    pub value: String,
}

impl BlobRef {
    /// Create a new blob reference.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl fmt::Display for BlobRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blake3_zero() {
        let h = Blake3Hash::zero();
        assert_eq!(h.bytes, [0; 32]);
    }

    #[test]
    fn blake3_display() {
        let h = Blake3Hash::new([0xff; 32]);
        let s = h.to_string();
        assert!(s.starts_with("blake3:"));
        assert_eq!(s.len(), 7 + 64); // "blake3:" + 64 hex chars
    }

    #[test]
    fn blake3_serde_roundtrip() {
        let h = Blake3Hash::new([0xab; 32]);
        let json = serde_json::to_string(&h).expect("serialize");
        let back: Blake3Hash = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(h, back);
    }

    #[test]
    fn content_digest_display() {
        let d = ContentDigest::new("sha256:abcdef0123456789");
        assert_eq!(d.to_string(), "sha256:abcdef0123456789");
    }

    #[test]
    fn blob_ref_display() {
        let r = BlobRef::new("cas/abc123");
        assert_eq!(r.to_string(), "cas/abc123");
    }

    #[test]
    fn content_digest_serde_roundtrip() {
        let d = ContentDigest::new("sha256:test");
        let json = serde_json::to_string(&d).expect("serialize");
        let back: ContentDigest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn hash_types_are_send_sync() {
        _assert_send_sync::<Blake3Hash>();
        _assert_send_sync::<ContentDigest>();
        _assert_send_sync::<BlobRef>();
    }
}
