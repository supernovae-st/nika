//! Terminal inline-image escapes (R3 Q5 · vendor docs fetched 2026-07-09) ·
//! pure string builders — the CALLER decides to print (presentation seam).
//! kitty graphics protocol: `ESC_G f=100,a=T ; <b64> ESC\` chunked ≤4096 ·
//! iTerm2: `ESC] 1337;File=inline=1:<b64> BEL`.

use std::fmt::Write as _;

/// RFC 4648 standard alphabet · hand-rolled (zero-dep).
#[must_use]
pub fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut chunks = data.chunks_exact(3);
    let at = |i: usize| char::from(*A.get(i & 63).unwrap_or(&b'A'));
    for c in chunks.by_ref() {
        let &[b0, b1, b2] = c else { continue };
        let n = (usize::from(b0) << 16) | (usize::from(b1) << 8) | usize::from(b2);
        out.push(at(n >> 18));
        out.push(at(n >> 12));
        out.push(at(n >> 6));
        out.push(at(n));
    }
    match *chunks.remainder() {
        [a] => {
            let n = usize::from(a) << 16;
            out.push(at(n >> 18));
            out.push(at(n >> 12));
            out.push_str("==");
        }
        [a, b] => {
            let n = (usize::from(a) << 16) | (usize::from(b) << 8);
            out.push(at(n >> 18));
            out.push(at(n >> 12));
            out.push(at(n >> 6));
            out.push('=');
        }
        _ => {}
    }
    out
}

/// kitty graphics escape · f=100 (PNG) · a=T (transmit+display) ·
/// base64 chunked ≤4096 · m=1 continuation / m=0 final.
#[must_use]
pub fn kitty(png: &[u8]) -> String {
    let b64 = base64(png);
    let chunks: Vec<&str> = b64
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    let mut out = String::with_capacity(b64.len() + chunks.len() * 16);
    for (i, chunk) in chunks.iter().enumerate() {
        let last = i + 1 == chunks.len();
        if i == 0 {
            let m = i32::from(!last);
            let _ = write!(out, "\x1b_Gf=100,a=T,m={m};{chunk}\x1b\\");
        } else {
            let m = i32::from(!last);
            let _ = write!(out, "\x1b_Gm={m};{chunk}\x1b\\");
        }
    }
    out
}

/// iTerm2 OSC 1337 inline image (single-part · inline=1).
#[must_use]
pub fn iterm2(png: &[u8], name: &str) -> String {
    format!(
        "\x1b]1337;File=name={};size={};inline=1:{}\x07",
        base64(name.as_bytes()),
        png.len(),
        base64(png)
    )
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
    fn base64_vectors() {
        // RFC 4648 §10 test vectors.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn kitty_first_chunk_carries_keys() {
        let esc = kitty(&[1, 2, 3]);
        assert!(esc.starts_with("\x1b_Gf=100,a=T,m=0;"));
        assert!(esc.ends_with("\x1b\\"));
    }

    #[test]
    fn iterm2_shape() {
        let esc = iterm2(&[1, 2, 3], "c.png");
        assert!(esc.starts_with("\x1b]1337;File=name="));
        assert!(esc.contains(";inline=1:"));
        assert!(esc.ends_with('\x07'));
    }
}
