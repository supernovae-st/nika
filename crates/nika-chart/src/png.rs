//! Minimal deterministic PNG encoder · zero-dep · hand-rolled from primary
//! sources (R3 annex · all fetched 2026-07-09): W3C PNG-3 (§5.2 signature ·
//! §5.3 chunks · §5.5 CRC · §11.2.1 IHDR · §9 filters) · RFC 1950 (zlib
//! header + Adler-32) · RFC 1951 (fixed-Huffman deflate · §3.1.1 bit order ·
//! §3.2.5/§3.2.6 tables).
//!
//! Strategy for charts: filter Up per scanline (identical rows → all-zero
//! filtered bytes) + fixed-Huffman deflate with distance-1 RLE matches.
//! No clock · no rand · no ancillary chunks — IHDR+IDAT+IEND only, fully
//! valid per spec, byte-deterministic by construction.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // byte/bit packing: u32↔u8 truncation is the wire format
/// CRC-32 (PNG §5.5) · reflected 0xEDB88320 · init all-1s · final complement.
fn crc32(data: &[u8]) -> u32 {
    let mut c: u32 = 0xFFFF_FFFF;
    for byte in data {
        c ^= u32::from(*byte);
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    c ^ 0xFFFF_FFFF
}

/// Adler-32 (RFC 1950) · s1=1 s2=0 · mod 65521 · over UNCOMPRESSED bytes.
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for byte in data {
        s1 = (s1 + u32::from(*byte)) % MOD;
        s2 = (s2 + s1) % MOD;
    }
    (s2 << 16) | s1
}

/// LSB-first bit accumulator (RFC 1951 §3.1.1). Non-Huffman elements enter
/// LSB-first as-is; Huffman codes are bit-reversed before insertion
/// (they pack MSB-of-code-first).
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    nbits: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            acc: 0,
            nbits: 0,
        }
    }

    fn write_bits(&mut self, value: u32, n: u32) {
        self.acc |= value << self.nbits;
        self.nbits += n;
        while self.nbits >= 8 {
            self.out.push((self.acc & 0xFF) as u8);
            self.acc >>= 8;
            self.nbits -= 8;
        }
    }

    /// Huffman code: reverse `n` bits, then pack LSB-first.
    fn write_huff(&mut self, code: u32, n: u32) {
        let mut rev = 0u32;
        for i in 0..n {
            rev |= ((code >> i) & 1) << (n - 1 - i);
        }
        self.write_bits(rev, n);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.nbits > 0 {
            self.out.push((self.acc & 0xFF) as u8);
        }
        self.out
    }
}

/// Fixed-Huffman literal/length symbol (RFC 1951 §3.2.6).
fn write_litlen(bw: &mut BitWriter, sym: u32) {
    match sym {
        0..=143 => bw.write_huff(0x30 + sym, 8),
        144..=255 => bw.write_huff(0x190 + (sym - 144), 9),
        256..=279 => bw.write_huff(sym - 256, 7),
        _ => bw.write_huff(0xC0 + (sym - 280), 8),
    }
}

/// Length → (symbol, extra-bit count, extra value) per RFC 1951 §3.2.5.
/// 258 MUST use code 285 (zero extra) — the 284+31 encoding is invalid.
fn length_symbol(len: u32) -> (u32, u32, u32) {
    const T: [(u32, u32, u32); 28] = [
        (257, 0, 3),
        (258, 0, 4),
        (259, 0, 5),
        (260, 0, 6),
        (261, 0, 7),
        (262, 0, 8),
        (263, 0, 9),
        (264, 0, 10),
        (265, 1, 11),
        (266, 1, 13),
        (267, 1, 15),
        (268, 1, 17),
        (269, 2, 19),
        (270, 2, 23),
        (271, 2, 27),
        (272, 2, 31),
        (273, 3, 35),
        (274, 3, 43),
        (275, 3, 51),
        (276, 3, 59),
        (277, 4, 67),
        (278, 4, 83),
        (279, 4, 99),
        (280, 4, 115),
        (281, 5, 131),
        (282, 5, 163),
        (283, 5, 195),
        (284, 5, 227),
    ];
    if len >= 258 {
        return (285, 0, 0);
    }
    let mut best = (257u32, 0u32, 3u32);
    for (sym, extra, base) in T {
        if base <= len {
            best = (sym, extra, base);
        } else {
            break;
        }
    }
    (best.0, best.1, len - best.2)
}

/// One fixed-Huffman deflate block · literals + distance-1 RLE matches.
fn deflate_fixed(data: &[u8]) -> Vec<u8> {
    let mut bw = BitWriter::new();
    bw.write_bits(1, 1); // BFINAL
    bw.write_bits(1, 2); // BTYPE=01 fixed
    let mut i = 0usize;
    while i < data.len() {
        // Distance-1 run: bytes equal to data[i-1].
        if i > 0 {
            let prev = data.get(i - 1).copied().unwrap_or(0);
            let mut run = 0usize;
            while run < 258 && data.get(i + run).copied() == Some(prev) {
                run += 1;
            }
            if run >= 3 {
                let (sym, extra, val) = length_symbol(run as u32);
                write_litlen(&mut bw, sym);
                if extra > 0 {
                    bw.write_bits(val, extra);
                }
                bw.write_huff(0, 5); // distance code 0 = distance 1 · no extra
                i += run;
                continue;
            }
        }
        write_litlen(&mut bw, u32::from(data.get(i).copied().unwrap_or(0)));
        i += 1;
    }
    write_litlen(&mut bw, 256); // end of block
    bw.finish()
}

fn push_chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(&kind);
    body.extend_from_slice(data);
    let crc = crc32(&body);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Encode 8-bit RGB rows (len = w*h*3) into a complete PNG byte stream.
/// Filter: Up on every row (row 0's prior is zeros ⇒ Up ≡ raw values).
#[must_use]
pub fn encode_rgb(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let stride = (w as usize) * 3;
    debug_assert_eq!(rgb.len(), stride * (h as usize));

    // Filtered stream: one filter byte (2 = Up) + per-byte delta to the row above.
    let mut filtered = Vec::with_capacity((stride + 1) * (h as usize));
    for row in 0..(h as usize) {
        filtered.push(2u8);
        for x in 0..stride {
            let cur = rgb.get(row * stride + x).copied().unwrap_or(0);
            let prior = if row == 0 {
                0
            } else {
                rgb.get((row - 1) * stride + x).copied().unwrap_or(0)
            };
            filtered.push(cur.wrapping_sub(prior));
        }
    }

    // zlib: 0x78 0x01 (CM=8 CINFO=7 · FCHECK ⇒ ×31) + deflate + Adler-32 BE.
    let mut idat = Vec::with_capacity(filtered.len() / 4 + 16);
    idat.push(0x78);
    idat.push(0x01);
    idat.extend_from_slice(&deflate_fixed(&filtered));
    idat.extend_from_slice(&adler32(&filtered).to_be_bytes());

    let mut out = Vec::with_capacity(idat.len() + 64);
    out.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // depth 8 · RGB · deflate · adaptive · no interlace
    push_chunk(&mut out, *b"IHDR", &ihdr);
    push_chunk(&mut out, *b"IDAT", &idat);
    push_chunk(&mut out, *b"IEND", &[]);
    out
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;

    #[test]
    fn crc32_check_vector() {
        // The canonical CRC-32 check value.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn adler32_check_vector() {
        // RFC 1950 algorithm · "Wikipedia" = 0x11E60398 (worked example).
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn length_symbol_boundaries() {
        assert_eq!(length_symbol(3), (257, 0, 0));
        assert_eq!(length_symbol(10), (264, 0, 0));
        assert_eq!(length_symbol(11), (265, 1, 0));
        assert_eq!(length_symbol(12), (265, 1, 1));
        assert_eq!(length_symbol(257), (284, 5, 30));
        assert_eq!(length_symbol(258), (285, 0, 0)); // NEVER 284+31
    }

    #[test]
    fn png_structure_and_determinism() {
        let rgb: Vec<u8> = (0..12u8).collect(); // 2×2 RGB
        let a = encode_rgb(2, 2, &rgb);
        let b = encode_rgb(2, 2, &rgb);
        assert_eq!(a, b);
        assert_eq!(&a[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&a[12..16], b"IHDR");
        assert_eq!(
            &a[a.len() - 12..],
            &[0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]
        );
    }

    #[test]
    fn ihdr_encodes_dimensions() {
        let rgb = vec![0u8; 5 * 3 * 7];
        let png = encode_rgb(5, 7, &rgb);
        assert_eq!(&png[16..20], &5u32.to_be_bytes());
        assert_eq!(&png[20..24], &7u32.to_be_bytes());
        assert_eq!(&png[24..29], &[8, 2, 0, 0, 0]);
    }
}
