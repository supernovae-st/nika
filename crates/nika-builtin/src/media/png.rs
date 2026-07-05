// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! PNG byte-level primitives shared by the mock renderer and the
//! provenance embedder — integrity math homed HERE so production code
//! never depends on a mock module (review P3.1).

/// CRC-32 (ISO 3309 · the PNG chunk checksum) — table-free bitwise form,
/// plenty for the small chunks we seal (tEXt provenance · mock IHDR/IDAT).
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_png_test_vectors() {
        // The canonical check value for "123456789" (ISO 3309 / PNG).
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
        assert_eq!(
            crc32(b"IEND"),
            0xAE42_6082,
            "the constant every PNG ends with"
        );
    }
}
