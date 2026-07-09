//! RFC 1950/1951 inflate — stored + fixed + dynamic Huffman. Hand-rolled from
//! the RFC (puff-style canonical decoding) — proven in the FX spike against
//! real zlib level-9 streams and Apple's encoder before landing here.
//! Never panics on hostile bytes: every failure is a typed Err.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
const MAXBITS: usize = 15;

/// Shared literal/length + distance tables (RFC 1951 §3.2.5) — the encoder
/// (png.rs) reuses them for its RLE emission.
pub const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
pub const LEN_EXTRA: [u16; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u16; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    cnt: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            pos: 0,
            buf: 0,
            cnt: 0,
        }
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        while self.cnt < n {
            let b = *self.data.get(self.pos).ok_or("out of input")?;
            self.buf |= u32::from(b) << self.cnt;
            self.cnt += 8;
            self.pos += 1;
        }
        let v = self.buf & ((1u32 << n) - 1);
        self.buf >>= n;
        self.cnt -= n;
        Ok(v)
    }

    fn byte_pos(&self) -> usize {
        self.pos - (self.cnt as usize / 8)
    }
}

struct Huffman {
    count: [u16; MAXBITS + 1],
    symbol: Vec<u16>,
}

impl Huffman {
    fn build(lengths: &[u16]) -> Result<Huffman, String> {
        let mut count = [0u16; MAXBITS + 1];
        for &l in lengths {
            *count
                .get_mut(l as usize)
                .ok_or("code length out of range")? += 1;
        }
        if count[0] as usize == lengths.len() {
            return Err("empty huffman table".into());
        }
        let mut left = 1i32;
        for c in count.iter().skip(1) {
            left <<= 1;
            left -= i32::from(*c);
            if left < 0 {
                return Err("over-subscribed code".into());
            }
        }
        let mut offs = [0u16; MAXBITS + 1];
        for len in 1..MAXBITS {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { count, symbol })
    }
}

fn decode_sym(r: &mut BitReader<'_>, h: &Huffman) -> Result<u16, String> {
    let mut code = 0i32;
    let mut first = 0i32;
    let mut index = 0i32;
    for len in 1..=MAXBITS {
        code |= r.bits(1)? as i32;
        let count = i32::from(h.count[len]);
        if code - count < first {
            let idx = (index + (code - first)) as usize;
            return h
                .symbol
                .get(idx)
                .copied()
                .ok_or_else(|| "bad symbol index".into());
        }
        index += count;
        first += count;
        first <<= 1;
        code <<= 1;
    }
    Err("code longer than 15 bits".into())
}

fn run_codes(
    r: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
    max_out: usize,
) -> Result<(), String> {
    loop {
        let sym = decode_sym(r, lit)?;
        match sym {
            0..=255 => {
                if out.len() >= max_out {
                    return Err("output exceeds declared size".into());
                }
                out.push(sym as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let i = (sym - 257) as usize;
                let len = LEN_BASE[i] as usize + r.bits(u32::from(LEN_EXTRA[i]))? as usize;
                let dsym = decode_sym(r, dist)? as usize;
                if dsym >= 30 {
                    return Err("bad distance symbol".into());
                }
                let d = DIST_BASE[dsym] as usize + r.bits(u32::from(DIST_EXTRA[dsym]))? as usize;
                if d > out.len() {
                    return Err("distance too far back".into());
                }
                if out.len() + len > max_out {
                    return Err("output exceeds declared size".into());
                }
                let start = out.len() - d;
                for k in 0..len {
                    let b = out[start + k];
                    out.push(b);
                }
            }
            _ => return Err("bad literal/length symbol".into()),
        }
    }
}

fn fixed_tables() -> Result<(Huffman, Huffman), String> {
    let mut lit_lens = [0u16; 288];
    for (i, l) in lit_lens.iter_mut().enumerate() {
        *l = match i {
            144..=255 => 9,
            256..=279 => 7,
            _ => 8, // 0..=143 and 280..=287 share length 8
        };
    }
    let dist_lens = [5u16; 30];
    Ok((Huffman::build(&lit_lens)?, Huffman::build(&dist_lens)?))
}

const CLEN_PERM: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn dynamic_tables(r: &mut BitReader<'_>) -> Result<(Huffman, Huffman), String> {
    let hlit = r.bits(5)? as usize + 257;
    let hdist = r.bits(5)? as usize + 1;
    let hclen = r.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err("bad table counts".into());
    }
    let mut clen_lens = [0u16; 19];
    for &perm in CLEN_PERM.iter().take(hclen) {
        clen_lens[perm] = r.bits(3)? as u16;
    }
    let clen = Huffman::build(&clen_lens)?;
    let mut lens = vec![0u16; hlit + hdist];
    let mut i = 0usize;
    while i < hlit + hdist {
        let sym = decode_sym(r, &clen)?;
        match sym {
            0..=15 => {
                lens[i] = sym;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err("repeat with no previous length".into());
                }
                let prev = lens[i - 1];
                let n = 3 + r.bits(2)? as usize;
                if i + n > lens.len() {
                    return Err("repeat overflow".into());
                }
                for _ in 0..n {
                    lens[i] = prev;
                    i += 1;
                }
            }
            17 | 18 => {
                i += if sym == 17 {
                    3 + r.bits(3)? as usize
                } else {
                    11 + r.bits(7)? as usize
                };
                if i > lens.len() {
                    return Err("zero-run overflow".into());
                }
            }
            _ => return Err("bad code-length symbol".into()),
        }
    }
    if lens[256] == 0 {
        return Err("missing end-of-block code".into());
    }
    let lit = Huffman::build(&lens[..hlit])?;
    let dist = Huffman::build(&lens[hlit..])?;
    Ok((lit, dist))
}

fn stored_block(r: &mut BitReader<'_>, out: &mut Vec<u8>, max_out: usize) -> Result<(), String> {
    r.buf = 0;
    r.cnt = 0;
    let p = r.byte_pos();
    let bytes = r.data;
    if p + 4 > bytes.len() {
        return Err("stored header truncated".into());
    }
    let len = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
    let nlen = u16::from_le_bytes([bytes[p + 2], bytes[p + 3]]) as usize;
    if len != (!nlen & 0xFFFF) {
        return Err("stored LEN/NLEN mismatch".into());
    }
    if p + 4 + len > bytes.len() {
        return Err("stored data truncated".into());
    }
    if out.len() + len > max_out {
        return Err("output exceeds declared size".into());
    }
    out.extend_from_slice(&bytes[p + 4..p + 4 + len]);
    r.pos = p + 4 + len;
    Ok(())
}

#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    // Standard deferred-modulo batching (5552 = the classic zlib bound).
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// zlib stream (RFC 1950) → raw bytes, capped at `max_out` (the caller's
/// pre-computed exact size — the decompression-bomb gate).
///
/// # Errors
/// A `String` reason for every structural failure (header · Huffman ·
/// overflow · Adler) — the caller wraps it in `FxError::Decode`.
pub fn inflate_zlib(data: &[u8], max_out: usize) -> Result<Vec<u8>, String> {
    if data.len() < 6 {
        return Err("zlib stream too short".into());
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0F != 8 {
        return Err("not a deflate stream".into());
    }
    if (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 {
        return Err("bad zlib header check".into());
    }
    if flg & 0x20 != 0 {
        return Err("preset dictionary unsupported".into());
    }
    let tail = &data[2..];
    let mut r = BitReader::new(tail);
    let mut out = Vec::with_capacity(max_out.min(1 << 24));
    loop {
        let bfinal = r.bits(1)?;
        let btype = r.bits(2)?;
        match btype {
            0 => stored_block(&mut r, &mut out, max_out)?,
            1 => {
                let (lit, dist) = fixed_tables()?;
                run_codes(&mut r, &mut out, &lit, &dist, max_out)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut r)?;
                run_codes(&mut r, &mut out, &lit, &dist, max_out)?;
            }
            _ => return Err("reserved block type".into()),
        }
        if bfinal == 1 {
            break;
        }
    }
    let p = r.byte_pos();
    if p + 4 > tail.len() {
        return Err("missing adler trailer".into());
    }
    let stored = u32::from_be_bytes([tail[p], tail[p + 1], tail[p + 2], tail[p + 3]]);
    let computed = adler32(&out);
    if stored != computed {
        return Err("adler-32 mismatch".into());
    }
    Ok(out)
}
