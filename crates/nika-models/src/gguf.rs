// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A header-only GGUF sniff — `general.architecture` in one buffered
//! read, kilobytes into a multi-gigabyte file.
//!
//! ADVISORY by construction: every failure path (not a GGUF · a future
//! version · a pathological header) answers `None`, never an error —
//! the callers (the pull receipt's per-family serve line · `serve`'s
//! fast pre-load refusal) degrade to their family-blind behavior. The
//! authoritative check stays in the loader (`nika-infer-local`
//! validates the same key at load); this sniff exists so a WRONG
//! family teaches in milliseconds, not after a full-file read.

use std::io::Read;
use std::path::Path;

/// `GGUF` little-endian magic.
const MAGIC: u32 = 0x4655_4747;
/// KV pairs scanned before giving up (the convention puts
/// `general.architecture` first; 64 tolerates exotic writers).
const MAX_KVS: u64 = 64;
/// Longest key/string value read (spec strings are short).
const MAX_STR: u64 = 4096;
/// Longest array skipped element-by-element before giving up.
const MAX_ARRAY: u64 = 65536;

/// The GGUF's declared `general.architecture` (`qwen3` · `llama` · …),
/// or `None` when the file does not sniff as a GGUF v2/v3 header that
/// carries one within the first [`MAX_KVS`] pairs.
#[must_use]
pub fn sniff_architecture(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut r = std::io::BufReader::new(file);
    if u32_le(&mut r)? != MAGIC {
        return None;
    }
    let version = u32_le(&mut r)?;
    if !(2..=3).contains(&version) {
        return None;
    }
    let _tensor_count = u64_le(&mut r)?;
    let kv_count = u64_le(&mut r)?.min(MAX_KVS);
    for _ in 0..kv_count {
        let key = string(&mut r)?;
        let value_type = u32_le(&mut r)?;
        if key == "general.architecture" {
            // The spec types it string (8); anything else is malformed
            // enough to stay unknown.
            return if value_type == 8 {
                string(&mut r)
            } else {
                None
            };
        }
        skip_value(&mut r, value_type, 0)?;
    }
    None
}

/// Skip one value of the given GGUF type. `None` = unskippable
/// (unknown future type · cap breach · crafted nesting) — the caller
/// gives up.
fn skip_value(r: &mut impl Read, value_type: u32, depth: u8) -> Option<()> {
    match value_type {
        0 | 1 | 7 => skip_bytes(r, 1),
        2 | 3 => skip_bytes(r, 2),
        4..=6 => skip_bytes(r, 4),
        10..=12 => skip_bytes(r, 8),
        8 => {
            let len = u64_le(r)?;
            if len > MAX_STR {
                return None;
            }
            skip_bytes(r, len)
        }
        9 => {
            // Arrays nest (element type 9) — the depth cap keeps a
            // crafted header from recursing the stack away.
            if depth >= 2 {
                return None;
            }
            let elem_type = u32_le(r)?;
            let count = u64_le(r)?;
            if count > MAX_ARRAY {
                return None;
            }
            for _ in 0..count {
                skip_value(r, elem_type, depth + 1)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn u32_le(r: &mut impl Read) -> Option<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b).ok()?;
    Some(u32::from_le_bytes(b))
}

fn u64_le(r: &mut impl Read) -> Option<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b).ok()?;
    Some(u64::from_le_bytes(b))
}

/// A GGUF string: u64 length + that many UTF-8 bytes.
fn string(r: &mut impl Read) -> Option<String> {
    let len = u64_le(r)?;
    if len > MAX_STR {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)] // len ≤ MAX_STR (4096)
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

fn skip_bytes(r: &mut impl Read, n: u64) -> Option<()> {
    std::io::copy(&mut r.take(n), &mut std::io::sink())
        .ok()
        .filter(|&copied| copied == n)
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::sniff_architecture;
    use std::io::Write as _;

    /// Build a minimal GGUF v3 header with the given KV pairs.
    fn gguf(kvs: &[(&str, u32, Vec<u8>)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&super::MAGIC.to_le_bytes());
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes()); // tensors
        b.extend_from_slice(&(kvs.len() as u64).to_le_bytes());
        for (key, value_type, value) in kvs {
            b.extend_from_slice(&(key.len() as u64).to_le_bytes());
            b.extend_from_slice(key.as_bytes());
            b.extend_from_slice(&value_type.to_le_bytes());
            b.extend_from_slice(value);
        }
        b
    }

    fn gguf_string(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u64).to_le_bytes().to_vec();
        v.extend_from_slice(s.as_bytes());
        v
    }

    fn stage(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("nika-gguf-sniff-{name}"));
        let mut f = std::fs::File::create(&path).expect("stage");
        f.write_all(bytes).expect("write");
        path
    }

    #[test]
    fn finds_the_architecture_first_or_later() {
        let first = stage(
            "first.gguf",
            &gguf(&[("general.architecture", 8, gguf_string("llama"))]),
        );
        assert_eq!(sniff_architecture(&first).as_deref(), Some("llama"));

        // Buried behind a string, a u32, and a string-array — the skip
        // walk earns its keep.
        let mut arr = 8u32.to_le_bytes().to_vec();
        arr.extend_from_slice(&2u64.to_le_bytes());
        arr.extend_from_slice(&gguf_string("a"));
        arr.extend_from_slice(&gguf_string("bb"));
        let later = stage(
            "later.gguf",
            &gguf(&[
                ("general.name", 8, gguf_string("SmolLM2")),
                ("general.file_type", 4, 7u32.to_le_bytes().to_vec()),
                ("tokenizer.ggml.tokens", 9, arr),
                ("general.architecture", 8, gguf_string("qwen3")),
            ]),
        );
        assert_eq!(sniff_architecture(&later).as_deref(), Some("qwen3"));
        let _ = std::fs::remove_file(first);
        let _ = std::fs::remove_file(later);
    }

    #[test]
    fn refuses_to_guess_on_anything_else() {
        let not_gguf = stage("not.gguf", b"MZ\x90\x00 definitely not a gguf");
        assert_eq!(sniff_architecture(&not_gguf), None);
        // v99: a future header this sniffer does not understand.
        let mut future = super::MAGIC.to_le_bytes().to_vec();
        future.extend_from_slice(&99u32.to_le_bytes());
        let future = stage("future.gguf", &future);
        assert_eq!(sniff_architecture(&future), None);
        // No architecture key at all.
        let keyless = stage(
            "keyless.gguf",
            &gguf(&[("general.name", 8, gguf_string("x"))]),
        );
        assert_eq!(sniff_architecture(&keyless), None);
        for p in [not_gguf, future, keyless] {
            let _ = std::fs::remove_file(p);
        }
    }
}
