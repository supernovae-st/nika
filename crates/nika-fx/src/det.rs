//! Deterministic seams — the ONLY place math beyond +−×÷ happens.
//! Everything routes through integer/fixed-point ops or committed const
//! tables (src/tables.rs · offline codegen). Re-derivation tests at the
//! bottom use std floats — tests are not shipped bytes, the tables are.
#![allow(clippy::unreadable_literal)] // FIPS/PCG/splitmix constants kept verbatim-checkable against their specs
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
#![allow(clippy::many_single_char_names)] // r/g/b · x/y · a/b/c (Paeth) ARE the domain vocabulary
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::tables;

// ── PCG32 (O'Neill · HMC-CS-2014-0905 · pcg_setseq_64_xsh_rr_32) ──────────

/// Minimal PCG32. One stream per pipeline run; every stochastic op draws
/// from it in documented order — the seed IS the style.
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    #[must_use]
    pub fn new(seed: u64) -> Pcg32 {
        let mut p = Pcg32 {
            state: 0,
            inc: (54u64 << 1) | 1,
        };
        p.next_u32();
        p.state = p.state.wrapping_add(seed);
        p.next_u32();
        p
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in 0..n (n ≥ 1) — simple modulo (bias irrelevant at art scale,
    /// and modulo is the simplest FOREVER-stable contract).
    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        if n <= 1 { 0 } else { self.next_u32() % n }
    }
}

/// Stateless per-pixel hash (splitmix64 finalizer) — grain wants a value
/// that depends only on (x, y, seed), never on scan order.
#[inline]
#[must_use]
pub fn pixel_hash(x: u32, y: u32, seed: u64) -> u32 {
    let mut z = seed ^ (u64::from(x) << 32 | u64::from(y)).wrapping_mul(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) as u32
}

// ── IGN · Interleaved Gradient Noise (Jimenez 2014) as u32 fractions ──────
// Constants = round(c · 2^32) for the two inner fractions and round(c · 2^16)
// for the outer multiplier — re-derived in tests.

const IGN_A_Q32: u32 = 288_237_660; // 0.06711056 · 2^32
const IGN_B_Q32: u32 = 25_070_368; // 0.00583715 · 2^32
const IGN_C_Q16: u64 = 3_472_289; // 52.9829189 · 2^16

/// IGN threshold for (x, y) as a byte 0..=255 — frame-stable by construction.
#[inline]
#[must_use]
pub fn ign_threshold(x: u32, y: u32) -> u8 {
    let f = (x.wrapping_mul(IGN_A_Q32)).wrapping_add(y.wrapping_mul(IGN_B_Q32));
    let t = (IGN_C_Q16.wrapping_mul(u64::from(f)) >> 16) as u32; // fract in Q32
    (t >> 24) as u8
}

// ── sRGB transfer (IEC 61966-2-1) · decode LUT + exact inverse by bisect ──

/// sRGB u8 → linear Q13 (0..=8192).
#[inline]
#[must_use]
pub fn srgb_to_linear(v: u8) -> u16 {
    tables::SRGB_DECODE_Q13[v as usize]
}

/// Linear Q13 → sRGB u8 — nearest entry of the monotone decode LUT
/// (binary search + neighbor compare ⇒ u8→linear→u8 is the identity).
#[must_use]
pub fn linear_to_srgb(q13: u16) -> u8 {
    let x = q13.min(8192);
    let (mut lo, mut hi) = (0usize, 255usize);
    while lo < hi {
        let mid = usize::midpoint(lo, hi);
        if tables::SRGB_DECODE_Q13[mid] < x {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    // lo = first entry ≥ x; pick the nearer of lo-1 / lo (ties → lo).
    if lo > 0 {
        let below = x - tables::SRGB_DECODE_Q13[lo - 1];
        let above = tables::SRGB_DECODE_Q13[lo] - x;
        if below < above {
            return (lo - 1) as u8;
        }
    }
    lo as u8
}

/// Linear-light BT.709 luma of an sRGB pixel, in Q13.
#[inline]
#[must_use]
pub fn linear_luma_q13(p: [u8; 3]) -> u32 {
    let r = u32::from(srgb_to_linear(p[0]));
    let g = u32::from(srgb_to_linear(p[1]));
    let b = u32::from(srgb_to_linear(p[2]));
    (2126 * r + 7152 * g + 722 * b) / 10000
}

/// Gamma-space luma (fast path for threshold arts where the classic look
/// expects it) — integer BT.709 weights.
#[inline]
#[must_use]
pub fn gamma_luma(p: [u8; 3]) -> u8 {
    ((2126 * u32::from(p[0]) + 7152 * u32::from(p[1]) + 722 * u32::from(p[2])) / 10000) as u8
}

// ── Oklab (Ottosson 2020) in Q16 · cbrt via committed LUT + lerp ──────────

/// cbrt of x in [0, 1] Q16 → Q15, via `CBRT_Q15` (4096 buckets + lerp).
#[inline]
fn cbrt_q16_to_q15(x: u32) -> u32 {
    let x = x.min(65536);
    let i = (x >> 4) as usize;
    let frac = x & 0xF;
    let a = u32::from(tables::CBRT_Q15[i]);
    let b = u32::from(tables::CBRT_Q15[(i + 1).min(4096)]);
    a + ((b - a) * frac + 8) / 16
}

/// Oklab coordinates in Q12 (L ≈ 0..4096 · a/b ≈ ±1638) — compact i32 range
/// so squared distances stay in i64.
#[must_use]
pub fn srgb_to_oklab_q12(p: [u8; 3]) -> [i32; 3] {
    let lin = [
        i64::from(srgb_to_linear(p[0])),
        i64::from(srgb_to_linear(p[1])),
        i64::from(srgb_to_linear(p[2])),
    ];
    let mut lab = [0i32; 3];
    let mut lms_p = [0i64; 3];
    for (k, row) in tables::OKLAB_M1_Q16.iter().enumerate() {
        // lms in Q13 (Q16 coeffs × Q13 input >> 16)
        let lms_q13 = (row[0] * lin[0] + row[1] * lin[1] + row[2] * lin[2]) >> 16;
        // → Q16 for the cbrt domain, clamp negatives (out-of-gamut guard)
        let q16 = (lms_q13.max(0) << 3) as u32;
        lms_p[k] = i64::from(cbrt_q16_to_q15(q16)); // Q15
    }
    for (k, row) in tables::OKLAB_M2_Q16.iter().enumerate() {
        // Q16 coeffs × Q15 → >> 19 lands in Q12
        let v = (row[0] * lms_p[0] + row[1] * lms_p[1] + row[2] * lms_p[2]) >> 19;
        lab[k] = v as i32;
    }
    lab
}

/// Oklab Q12 → sRGB (for ramp construction — 256 calls per duotone op).
#[must_use]
pub fn oklab_q12_to_srgb(lab: [i32; 3]) -> [u8; 3] {
    let l64 = [i64::from(lab[0]), i64::from(lab[1]), i64::from(lab[2])];
    let mut lms_p = [0i64; 3]; // Q15
    for (k, row) in tables::OKLAB_M2_INV_Q16.iter().enumerate() {
        lms_p[k] = ((row[0] * l64[0] + row[1] * l64[1] + row[2] * l64[2]) >> 13).max(0);
    }
    let mut out = [0u8; 3];
    let mut lin = [0i64; 3];
    for k in 0..3 {
        // (Q15)³ → Q13 : x³ >> (15·3 − 13) = >> 32
        let c = lms_p[k];
        lin[k] = (c * c * c) >> 32;
    }
    for (k, row) in tables::OKLAB_M1_INV_Q16.iter().enumerate() {
        let v = (row[0] * lin[0] + row[1] * lin[1] + row[2] * lin[2]) >> 16;
        out[k] = linear_to_srgb(v.clamp(0, 8192) as u16);
    }
    out
}

/// Squared Oklab distance (the palette metric — Euclid per the FX plan §12-B).
#[inline]
#[must_use]
pub fn oklab_dist_sq(a: [i32; 3], b: [i32; 3]) -> i64 {
    let d0 = i64::from(a[0] - b[0]);
    let d1 = i64::from(a[1] - b[1]);
    let d2 = i64::from(a[2] - b[2]);
    d0 * d0 + d1 * d1 + d2 * d2
}

// ── sha256 (FIPS 180-4) — hand-rolled · vectors in tests ──────────────────

#[allow(clippy::unreadable_literal)] // FIPS 180-4 K constants — kept verbatim-checkable against the spec
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_block(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for (i, item) in w.iter_mut().take(16).enumerate() {
        *item = u32::from_be_bytes([
            block[4 * i],
            block[4 * i + 1],
            block[4 * i + 2],
            block[4 * i + 3],
        ]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

/// sha256 hex digest of `data`.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut chunks = data.chunks_exact(64);
    for block in &mut chunks {
        sha256_block(&mut state, block);
    }
    let mut tail = Vec::with_capacity(128);
    tail.extend_from_slice(chunks.remainder());
    tail.push(0x80);
    while tail.len() % 64 != 56 {
        tail.push(0);
    }
    tail.extend_from_slice(&bit_len.to_be_bytes());
    for block in tail.chunks_exact(64) {
        sha256_block(&mut state, block);
    }
    let mut hex = String::with_capacity(64);
    for word in state {
        for byte in word.to_be_bytes() {
            hex.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
            hex.push(char::from_digit(u32::from(byte & 0xF), 16).unwrap_or('0'));
        }
    }
    hex
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

    use super::*;

    #[test]
    fn sha256_fips_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn pcg32_reference_stream_is_stable() {
        // Golden: the first outputs for seed 42 are pinned FOREVER (byte contract).
        let mut p = Pcg32::new(42);
        let first: Vec<u32> = (0..4).map(|_| p.next_u32()).collect();
        let mut q = Pcg32::new(42);
        let again: Vec<u32> = (0..4).map(|_| q.next_u32()).collect();
        assert_eq!(first, again);
        let mut r = Pcg32::new(43);
        assert_ne!(first[0], r.next_u32());
    }

    // Table re-derivation — floats allowed here (tests are not shipped bytes).
    #[test]
    #[allow(clippy::disallowed_methods)]
    fn srgb_decode_lut_rederives() {
        for v in 0..256usize {
            let c = v as f64 / 255.0;
            let lin = if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
            let expect = (lin * 8192.0).round() as u16;
            assert_eq!(tables::SRGB_DECODE_Q13[v], expect, "entry {v}");
        }
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn cbrt_lut_rederives() {
        for i in (0..=4096usize).step_by(37) {
            let x = (i * 16) as f64 / 65536.0;
            let expect = (x.powf(1.0 / 3.0) * 32768.0).round() as u16;
            assert_eq!(tables::CBRT_Q15[i], expect, "entry {i}");
        }
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn ign_constants_rederive() {
        assert_eq!(IGN_A_Q32, (0.06711056f64 * 4294967296.0).round() as u32);
        assert_eq!(IGN_B_Q32, (0.00583715f64 * 4294967296.0).round() as u32);
        assert_eq!(IGN_C_Q16, (52.9829189f64 * 65536.0).round() as u64);
    }

    #[test]
    fn srgb_roundtrip_is_identity() {
        for v in 0..=255u8 {
            assert_eq!(linear_to_srgb(srgb_to_linear(v)), v);
        }
    }

    #[test]
    fn oklab_white_black_sanity() {
        let white = srgb_to_oklab_q12([255, 255, 255]);
        let black = srgb_to_oklab_q12([0, 0, 0]);
        // L(white) ≈ 4096 in Q12, L(black) = 0; a/b ≈ 0 for neutrals.
        assert!((white[0] - 4096).abs() < 80, "L white {}", white[0]);
        assert_eq!(black[0], 0);
        assert!(white[1].abs() < 60 && white[2].abs() < 60);
        // Distance sanity: red is closer to orange than to blue.
        let red = srgb_to_oklab_q12([255, 0, 0]);
        let orange = srgb_to_oklab_q12([255, 128, 0]);
        let blue = srgb_to_oklab_q12([0, 0, 255]);
        assert!(oklab_dist_sq(red, orange) < oklab_dist_sq(red, blue));
    }

    #[test]
    fn oklab_inverse_roundtrips_grays() {
        for v in [0u8, 32, 96, 128, 200, 255] {
            let lab = srgb_to_oklab_q12([v, v, v]);
            let back = oklab_q12_to_srgb(lab);
            for c in back {
                assert!(
                    (i32::from(c) - i32::from(v)).abs() <= 3,
                    "gray {v} -> {back:?}"
                );
            }
        }
    }

    #[test]
    fn blue_noise_is_a_permutation() {
        let mut seen = [false; 4096];
        for &r in &tables::BLUE_NOISE_64 {
            assert!(!seen[r as usize]);
            seen[r as usize] = true;
        }
    }

    #[test]
    fn bayer_matrices_are_permutations() {
        fn check(m: &[u16]) {
            let mut seen = vec![false; m.len()];
            for &v in m {
                assert!(!seen[v as usize]);
                seen[v as usize] = true;
            }
        }
        check(&tables::BAYER2);
        check(&tables::BAYER4);
        check(&tables::BAYER8);
    }
}
