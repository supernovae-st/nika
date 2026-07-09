//! PNG decode + encode, hand-rolled from W3C PNG-3.
//!
//! Decode: depth-8 · color types 0/2/4/6 (palette → typed hint) · no Adam7 ·
//! all 5 filters · per-chunk CRC verified · pixel budget gated from IHDR
//! BEFORE any inflate (the decompression-bomb answer to sniff.rs's law).
//!
//! Encode: color type 2 · filter Up · fixed-Huffman DEFLATE with distance-1
//! RLE (the chart encoder recipe, rewritten) · optional `nika` tEXt recipe
//! chunk (no timestamp — byte determinism holds forever).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
#![allow(clippy::many_single_char_names)] // r/g/b · x/y · a/b/c (Paeth) ARE the domain vocabulary
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use super::inflate::{self, LEN_BASE, LEN_EXTRA};
use crate::error::FxError;
use crate::pix::PixBuf;

pub const SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
/// Hard budget: 64 Mi-pixels (2^26) — a 8192×8192 fits, hostile dims don't.
pub const MAX_PIXELS: u64 = 1 << 26;
/// Per-dimension cap (mirrors the `image_generate` law).
pub const MAX_DIMENSION: u32 = 16_384;

#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn paeth(a: i32, b: i32, c: i32) -> i32 {
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

struct Ihdr {
    w: u32,
    h: u32,
    color: u8,
    channels: usize,
}

fn parse_ihdr(body: &[u8]) -> Result<Ihdr, FxError> {
    if body.len() != 13 {
        return Err(FxError::Decode("IHDR length != 13".into()));
    }
    let w = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
    let h = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
    let depth = body[8];
    let color = body[9];
    let interlace = body[12];
    if w == 0 || h == 0 {
        return Err(FxError::Decode("zero dimensions".into()));
    }
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(FxError::Budget {
            pixels: u64::from(w) * u64::from(h),
            max: MAX_PIXELS,
        });
    }
    if w > MAX_DIMENSION || h > MAX_DIMENSION {
        return Err(FxError::Decode(format!(
            "dimension {w}x{h} exceeds the {MAX_DIMENSION} per-side cap"
        )));
    }
    if depth != 8 {
        return Err(FxError::UnsupportedFormat(format!(
            "bit depth {depth} (v1 accepts depth 8 — re-export or convert upstream)"
        )));
    }
    if interlace != 0 {
        return Err(FxError::UnsupportedFormat(
            "Adam7 interlaced PNG (v1 accepts non-interlaced)".into(),
        ));
    }
    let channels = match color {
        0 => 1, // grayscale
        2 => 3, // rgb
        4 => 2, // gray + alpha
        6 => 4, // rgba
        3 => {
            return Err(FxError::UnsupportedFormat(
                "palette-indexed PNG (v1 accepts gray/RGB/RGBA — re-export truecolor)".into(),
            ));
        }
        other => {
            return Err(FxError::UnsupportedFormat(format!(
                "PNG color type {other}"
            )));
        }
    };
    Ok(Ihdr {
        w,
        h,
        color,
        channels,
    })
}

/// Decode a PNG into an RGB8 buffer (alpha discarded · gray replicated).
///
/// # Errors
/// `FxError::UnsupportedFormat` (not a PNG we accept) · `FxError::Decode`
/// (structural corruption · CRC/Adler) · `FxError::Budget` (pixel budget,
/// gated from IHDR before any inflate).
pub fn decode(data: &[u8]) -> Result<PixBuf, FxError> {
    if data.len() < 8 || data[..8] != SIG {
        return Err(FxError::UnsupportedFormat(
            "not a PNG (magic bytes) — image_fx v1 accepts PNG input; produce PNG \
             upstream (image_generate format: png) or convert once via exec"
                .into(),
        ));
    }
    let mut pos = 8usize;
    let mut ihdr: Option<Ihdr> = None;
    let mut idat: Vec<u8> = Vec::new();
    while pos + 12 <= data.len() {
        let len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let ty_end = pos + 8;
        let body_end = ty_end
            .checked_add(len)
            .ok_or_else(|| FxError::Decode("chunk length overflow".into()))?;
        if body_end + 4 > data.len() {
            return Err(FxError::Decode("chunk truncated".into()));
        }
        let ty = &data[pos + 4..ty_end];
        let body = &data[ty_end..body_end];
        let stored_crc = u32::from_be_bytes([
            data[body_end],
            data[body_end + 1],
            data[body_end + 2],
            data[body_end + 3],
        ]);
        let mut crc_in = Vec::with_capacity(4 + len);
        crc_in.extend_from_slice(ty);
        crc_in.extend_from_slice(body);
        if crc32(&crc_in) != stored_crc {
            return Err(FxError::Decode(format!(
                "chunk crc mismatch on {}",
                String::from_utf8_lossy(ty)
            )));
        }
        match ty {
            b"IHDR" => ihdr = Some(parse_ihdr(body)?),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        pos = body_end + 4;
    }
    let ihdr = ihdr.ok_or_else(|| FxError::Decode("missing IHDR".into()))?;
    let (w, h, bpp) = (ihdr.w as usize, ihdr.h as usize, ihdr.channels);
    let stride = w * bpp;
    let expected = h * (stride + 1);
    let raw = inflate::inflate_zlib(&idat, expected).map_err(FxError::Decode)?;
    if raw.len() != expected {
        return Err(FxError::Decode(format!(
            "decompressed size {} != expected {expected}",
            raw.len()
        )));
    }
    let mut prev = vec![0u8; stride];
    let mut cur = vec![0u8; stride];
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = y * (stride + 1);
        let ft = raw[row];
        let src = &raw[row + 1..row + 1 + stride];
        unfilter_row(ft, src, &prev, &mut cur, bpp).map_err(FxError::Decode)?;
        push_row_rgb(&cur, ihdr.color, w, &mut px);
        std::mem::swap(&mut prev, &mut cur);
    }
    PixBuf::from_pixels(w, h, px).ok_or_else(|| FxError::Decode("pixel count mismatch".into()))
}

fn unfilter_row(ft: u8, src: &[u8], prev: &[u8], cur: &mut [u8], bpp: usize) -> Result<(), String> {
    for i in 0..src.len() {
        let x = i32::from(src[i]);
        let a = if i >= bpp { i32::from(cur[i - bpp]) } else { 0 };
        let b = i32::from(prev[i]);
        let c = if i >= bpp {
            i32::from(prev[i - bpp])
        } else {
            0
        };
        let v = match ft {
            0 => x,
            1 => x + a,
            2 => x + b,
            3 => x + i32::midpoint(a, b),
            4 => x + paeth(a, b, c),
            _ => return Err(format!("bad filter type {ft}")),
        };
        cur[i] = (v & 0xFF) as u8;
    }
    Ok(())
}

fn push_row_rgb(cur: &[u8], color: u8, w: usize, px: &mut Vec<[u8; 3]>) {
    match color {
        0 => {
            for &g in cur.iter().take(w) {
                px.push([g, g, g]);
            }
        }
        4 => {
            for x in 0..w {
                let g = cur[2 * x];
                px.push([g, g, g]);
            }
        }
        2 => {
            for x in 0..w {
                px.push([cur[3 * x], cur[3 * x + 1], cur[3 * x + 2]]);
            }
        }
        _ => {
            // color type 6 — the only remaining accepted type
            for x in 0..w {
                px.push([cur[4 * x], cur[4 * x + 1], cur[4 * x + 2]]);
            }
        }
    }
}

// ── Encoder · filter Up · fixed-Huffman + distance-1 RLE ─────────────────

struct BitWriter {
    out: Vec<u8>,
    buf: u32,
    cnt: u32,
}

impl BitWriter {
    fn new() -> BitWriter {
        BitWriter {
            out: Vec::new(),
            buf: 0,
            cnt: 0,
        }
    }

    /// LSB-first raw bits (headers · extra bits).
    fn bits(&mut self, v: u32, n: u32) {
        self.buf |= v << self.cnt;
        self.cnt += n;
        while self.cnt >= 8 {
            self.out.push((self.buf & 0xFF) as u8);
            self.buf >>= 8;
            self.cnt -= 8;
        }
    }

    /// Huffman code — emitted MSB-first per RFC 1951 §3.1.1.
    fn code(&mut self, code: u32, n: u32) {
        for k in (0..n).rev() {
            self.bits((code >> k) & 1, 1);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.cnt > 0 {
            self.out.push((self.buf & 0xFF) as u8);
        }
        self.out
    }
}

fn fixed_lit(w: &mut BitWriter, sym: u16) {
    match sym {
        0..=143 => w.code(0x30 + u32::from(sym), 8),
        144..=255 => w.code(0x190 + (u32::from(sym) - 144), 9),
        256..=279 => w.code(u32::from(sym) - 256, 7),
        _ => w.code(0xC0 + (u32::from(sym) - 280), 8),
    }
}

fn emit_run(w: &mut BitWriter, len: usize) {
    // Longest length code whose base ≤ len, then extra bits.
    let mut idx = LEN_BASE.len() - 1;
    while LEN_BASE[idx] as usize > len {
        idx -= 1;
    }
    fixed_lit(w, 257 + idx as u16);
    let extra = u32::from(LEN_EXTRA[idx]);
    if extra > 0 {
        w.bits((len - LEN_BASE[idx] as usize) as u32, extra);
    }
    w.code(0, 5); // distance code 0 = distance 1
}

/// Fixed-Huffman zlib with distance-1 RLE — small, fast, fully deterministic.
fn zlib_fixed_rle(raw: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.bits(1, 1); // BFINAL
    w.bits(1, 2); // BTYPE = 01 fixed
    let mut i = 0usize;
    while i < raw.len() {
        if i > 0 && raw[i] == raw[i - 1] {
            let mut run = 1usize;
            while i + run < raw.len() && raw[i + run] == raw[i - 1] && run < 258 {
                run += 1;
            }
            if run >= 3 {
                emit_run(&mut w, run);
                i += run;
                continue;
            }
        }
        fixed_lit(&mut w, u16::from(raw[i]));
        i += 1;
    }
    fixed_lit(&mut w, 256);
    let deflate = w.finish();
    let mut z = Vec::with_capacity(deflate.len() + 6);
    z.extend_from_slice(&[0x78, 0x01]);
    z.extend_from_slice(&deflate);
    z.extend_from_slice(&inflate::adler32(raw).to_be_bytes());
    z
}

fn chunk(out: &mut Vec<u8>, ty: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&ty);
    out.extend_from_slice(data);
    let mut crc_in = Vec::with_capacity(4 + data.len());
    crc_in.extend_from_slice(&ty);
    crc_in.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
}

/// Encode RGB8 → PNG (color 2 · filter Up) with an optional `nika` tEXt
/// recipe payload (ASCII JSON · written before IDAT · no timestamps).
#[must_use]
pub fn encode(img: &PixBuf, recipe_json: Option<&str>) -> Vec<u8> {
    let stride = img.w * 3;
    let mut raw = Vec::with_capacity(img.h * (stride + 1));
    let mut prev = vec![0u8; stride];
    let mut cur = vec![0u8; stride];
    for y in 0..img.h {
        for (x, chunk) in cur.chunks_exact_mut(3).enumerate() {
            let p = img.get(x, y);
            chunk.copy_from_slice(&p);
        }
        raw.push(2); // filter Up
        for i in 0..stride {
            raw.push(cur[i].wrapping_sub(prev[i]));
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(img.w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(img.h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = Vec::new();
    out.extend_from_slice(&SIG);
    chunk(&mut out, *b"IHDR", &ihdr);
    if let Some(json) = recipe_json {
        let mut text = Vec::with_capacity(5 + json.len());
        text.extend_from_slice(b"nika");
        text.push(0);
        text.extend_from_slice(json.as_bytes());
        chunk(&mut out, *b"tEXt", &text);
    }
    chunk(&mut out, *b"IDAT", &zlib_fixed_rle(&raw));
    chunk(&mut out, *b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_img() -> PixBuf {
        let mut img = PixBuf::new(61, 37); // odd dims on purpose
        for y in 0..37 {
            for x in 0..61 {
                let g = ((x * 255) / 60) as u8;
                img.set(x, y, [g, (y * 6) as u8, 128]);
            }
        }
        img
    }

    #[test]
    fn encode_decode_roundtrip_pixel_exact() {
        let img = test_img();
        let png = encode(&img, Some("{\"kind\":\"image_fx\",\"v\":1}"));
        let back = decode(&png).expect("roundtrip decode");
        assert_eq!(back.w, img.w);
        assert_eq!(back.h, img.h);
        assert_eq!(back.pixels(), img.pixels());
    }

    #[test]
    fn double_encode_is_byte_identical() {
        let img = test_img();
        assert_eq!(encode(&img, None), encode(&img, None));
    }

    #[test]
    fn recipe_chunk_changes_bytes_deterministically() {
        let img = test_img();
        let a = encode(&img, Some("{\"a\":1}"));
        let b = encode(&img, Some("{\"a\":1}"));
        let c = encode(&img, Some("{\"a\":2}"));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn rejects_non_png() {
        let e = decode(b"JFIF nope").unwrap_err();
        assert!(matches!(e, FxError::UnsupportedFormat(_)));
    }

    #[test]
    fn rejects_oversized_dims_before_inflate() {
        // Hand-build sig + IHDR claiming 60000x60000 — must fail on budget,
        // never allocate.
        let mut data = Vec::new();
        data.extend_from_slice(&SIG);
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&60000u32.to_be_bytes());
        ihdr.extend_from_slice(&60000u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut data, *b"IHDR", &ihdr);
        let e = decode(&data).unwrap_err();
        assert!(matches!(e, FxError::Budget { .. }));
    }

    #[test]
    fn hostile_bytes_never_panic() {
        // Seeded byte-mangling fuzz-lite over a valid PNG.
        let img = test_img();
        let png = encode(&img, None);
        let mut rng = crate::det::Pcg32::new(7);
        for _ in 0..10_000 {
            let mut m = png.clone();
            let flips = 1 + rng.below(8) as usize;
            for _ in 0..flips {
                let i = rng.below(m.len() as u32) as usize;
                m[i] ^= (1 + rng.below(255)) as u8;
            }
            let _ = decode(&m); // Ok or Err — never panic
        }
    }

    #[test]
    fn gray_and_alpha_color_types_decode() {
        // Hand-encode a tiny grayscale (color 0) PNG via our chunk machinery.
        let (w, h) = (4usize, 2usize);
        let mut raw = Vec::new();
        for y in 0..h {
            raw.push(0);
            for x in 0..w {
                raw.push((x * 60 + y * 10) as u8);
            }
        }
        let mut data = Vec::new();
        data.extend_from_slice(&SIG);
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
        chunk(&mut data, *b"IHDR", &ihdr);
        chunk(&mut data, *b"IDAT", &super::zlib_fixed_rle(&raw));
        chunk(&mut data, *b"IEND", &[]);
        let img = decode(&data).expect("gray decode");
        assert_eq!((img.w, img.h), (w, h));
        assert_eq!(img.get(1, 0), [60, 60, 60]);
    }
}
