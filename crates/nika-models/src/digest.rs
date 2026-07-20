// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pull integrity gate — multi-gigabyte GGUF weights verify against
//! the Hub-declared sha256 BEFORE they land in the models dir (a
//! size-confirm gate says the COUNT is right; only a digest says the
//! BYTES are).
//!
//! The digest of record, in order of authority:
//! 1. `x-linked-etag` on the download response — the Hub puts the LFS
//!    sha256 on the resolve redirect; a transport that surfaces the hop
//!    hands it straight over;
//! 2. the tree listing's `lfs.oid` — THE production lane: `nika-http`
//!    follows the redirect and answers the FINAL hop's headers, where
//!    the CDN's own `etag` is the Xet/md5 hash, never the file's
//!    sha256 (verified live 2026-07-20);
//! 3. a bare `etag`, only when it is exactly a 64-hex sha256.
//!
//! Anything else declares nothing: the pull then SUCCEEDS unverified —
//! skip-with-loud-note, never fail (a small non-LFS `tokenizer.json`
//! carries no sha256 at all). A DECLARED digest that mismatches is the
//! opposite arm: HARD-REFUSE with both digests named, and nothing left
//! behind — no artifact, no `.part` (a re-pull must not resume onto
//! tampered bytes). Hashing happens ONCE at completion, over the whole
//! `.part` from disk: an incremental hash would miss the bytes an
//! earlier interrupted process wrote, so a resumed pull and a fresh
//! pull share this one gate.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

use crate::pull::{Refusal, refuse};
use crate::store;

/// What the integrity gate did with a completed transfer — the pull
/// seam's loud note and the store record both read this verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Integrity {
    /// The Hub declared a sha256 and the completed bytes matched it;
    /// the digest records beside the file (`nika model list` shows it).
    Verified(String),
    /// The Hub declared no sha256 for this file — the pull succeeded
    /// size-checked only; the caller surfaces the loud note.
    NotDeclared,
}

/// The completed bytes failed the Hub-declared sha256 — typed at the
/// gate so expected and actual cannot blur into prose (the registry
/// client's `HashMismatch` arm, in this crate's plain-refusal voice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DigestMismatch {
    /// The file whose bytes lied.
    pub file: String,
    /// The Hub-declared sha256 (64-hex).
    pub expected: String,
    /// What the downloaded bytes actually hash to.
    pub actual: String,
}

impl DigestMismatch {
    /// The hard refusal: both digests named, the cleanup stated.
    pub(crate) fn refusal(&self) -> Refusal {
        refuse(format!(
            "model pull: {} failed the integrity check — the bytes do not match the Hub's \
             declared sha256\n  expected: {}\n  actual:   {}\n  nothing was installed (the \
             partial file was deleted — a re-pull starts clean)\n  fix: re-run the pull; if \
             it fails again the mirror or the network is tampering — fetch the file from \
             huggingface.co directly\n",
            self.file, self.expected, self.actual
        ))
    }
}

/// The Hub-declared sha256 for one download (the module doc names the
/// order of authority). `lfs_oid` is the tree listing's pointer; the
/// headers are the download response's.
pub(crate) fn declared_sha256(
    headers: &BTreeMap<String, String>,
    lfs_oid: Option<&str>,
) -> Option<String> {
    header_sha256(headers, "x-linked-etag")
        .or_else(|| {
            lfs_oid
                .filter(|oid| store::is_sha256_hex(oid))
                .map(str::to_ascii_lowercase)
        })
        .or_else(|| header_sha256(headers, "etag"))
}

/// A response header carrying a sha256? Values arrive quoted (the Hub
/// spells `x-linked-etag: "abcd…"`); a weak-validator `W/` prefix drops
/// too. Anything not exactly 64 hex (a CDN's Xet/md5 etag) is not a
/// sha256 and declares nothing.
fn header_sha256(headers: &BTreeMap<String, String>, name: &str) -> Option<String> {
    let raw = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))?
        .1;
    let trimmed = raw.trim();
    let unquoted = trimmed
        .strip_prefix("W/")
        .unwrap_or(trimmed)
        .trim_matches('"');
    store::is_sha256_hex(unquoted).then(|| unquoted.to_ascii_lowercase())
}

/// The completion gate, run ONCE per finished transfer (the module doc
/// names the arms): verify → land → record. The `.part` renames into
/// place ONLY after the check — a mismatch never produces a named
/// artifact.
pub(crate) fn finish_write(
    declared: Option<String>,
    part: &Path,
    dest: &Path,
    file: &str,
) -> Result<Integrity, Refusal> {
    let integrity = match declared {
        Some(expected) => {
            let actual = sha256_file(part)?;
            if actual != expected {
                let _ = std::fs::remove_file(part);
                let _ = std::fs::remove_file(dest);
                return Err(DigestMismatch {
                    file: file.to_owned(),
                    expected,
                    actual,
                }
                .refusal());
            }
            Integrity::Verified(expected)
        }
        None => Integrity::NotDeclared,
    };
    std::fs::rename(part, dest).map_err(|e| {
        refuse(format!(
            "model pull: cannot move {} into place ({e})\n  fix: check the models dir\n",
            part.display()
        ))
    })?;
    if let Integrity::Verified(digest) = &integrity {
        record_digest(dest, digest)?;
    }
    Ok(integrity)
}

/// `<file>.sha256` beside the verified GGUF — the store metadata
/// `nika model list` reads (the registry's digest-record precedent, one
/// plain hex line). A record that will not write refuses honestly: the
/// verified file IS in place and the message says so.
fn record_digest(dest: &Path, digest: &str) -> Result<(), Refusal> {
    std::fs::write(store::digest_sidecar(dest), format!("{digest}\n")).map_err(|e| {
        refuse(format!(
            "model pull: {} is verified and in place, but its digest record would not write \
             ({e})\n  fix: check the models dir\n",
            dest.display()
        ))
    })
}

/// sha256 the completed file in one streaming pass (1 MiB chunks — the
/// GGUF is gigabytes, never load it whole). Hex-lowercase, the Hub's
/// spelling.
fn sha256_file(path: &Path) -> Result<String, Refusal> {
    use sha2::Digest as _;
    let unreadable = |e: std::io::Error| {
        refuse(format!(
            "model pull: cannot re-read {} for the integrity check ({e})\n  fix: check the \
             models dir\n",
            path.display()
        ))
    };
    let mut file = std::fs::File::open(path).map_err(&unreadable)?;
    let mut hasher = sha2::Sha256::new();
    // 1 MiB chunks, heap-held (the GGUF is gigabytes — never load it
    // whole, never blow the stack either).
    let mut buf = vec![0u8; 1024 * 1024].into_boxed_slice();
    loop {
        let read = file.read(&mut buf).map_err(&unreadable)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

/// sha256 of a byte slice, hex-lowercase — the test-facing spelling of
/// the same primitive (the pull integration tests declare their
/// expected etags through it, never a hand-computed constant).
#[cfg(test)]
pub(crate) fn sha256_of(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    hex_lower(&sha2::Sha256::digest(bytes))
}

/// Lowercase-hex encode (no hex crate — two lines, zero deps; the
/// registry client's exact idiom).
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    // The pull/store tests' exact fixture idiom.
    #[allow(clippy::disallowed_methods)]
    fn temp_root(name: &str) -> PathBuf {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR").map_or_else(
            || {
                std::env::current_dir()
                    .expect("current dir")
                    .join("target")
                    .join("tmp")
            },
            PathBuf::from,
        );
        let dir = base.join(format!("nika-digest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    fn headers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    /// A real Hub sha256 shape (the `SmolLM2` `Q4_K_M` x-linked-etag).
    const GOOD: &str = "2e8040ceae7815abe0dcb3540b9995eaa1fa0d2ca9e797d0a635ae4433c68c2d";

    /// The order of authority: `x-linked-etag`, then the tree's
    /// `lfs.oid`, then a bare sha256-shaped `etag`; anything weaker
    /// declares nothing. Quoted / weak-validator / uppercase forms
    /// normalize; a 32-hex md5 etag is NOT a sha256.
    #[test]
    fn declared_sha256_walks_the_order_of_authority() {
        // The redirect-hop header wins when a transport surfaces it.
        let quoted = format!("\"{GOOD}\"");
        let h = headers(&[("x-linked-etag", quoted.as_str())]);
        assert_eq!(declared_sha256(&h, None).as_deref(), Some(GOOD));

        // The tree pointer is the production lane: the CDN's own etag
        // (here a 64-hex Xet hash) must NOT shadow it.
        let xet = format!("\"{}\"", "f".repeat(64));
        let cdn = headers(&[("etag", xet.as_str())]);
        assert_eq!(declared_sha256(&cdn, Some(GOOD)).as_deref(), Some(GOOD));

        // No tree pointer: a 64-hex etag is the fallback…
        let f64 = "f".repeat(64);
        assert_eq!(declared_sha256(&cdn, None).as_deref(), Some(f64.as_str()));

        // …but a 32-hex md5 etag is not a sha256 — nothing declared.
        let md5 = headers(&[("etag", "\"9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d\"")]);
        assert_eq!(declared_sha256(&md5, None), None);

        // Weak-validator + uppercase normalize to the lowercase digest.
        let weak = format!("W/\"{}\"", GOOD.to_uppercase());
        let odd = headers(&[("x-linked-etag", weak.as_str())]);
        assert_eq!(declared_sha256(&odd, None).as_deref(), Some(GOOD));

        // A garbage tree pointer declares nothing on its own.
        assert_eq!(declared_sha256(&BTreeMap::new(), Some("not-hex")), None);
    }

    /// A declared match lands the file + its digest record; the verdict
    /// carries the verified digest.
    #[test]
    fn finish_write_verifies_lands_and_records() {
        let root = temp_root("gate-happy");
        let part = root.join("w.gguf.part");
        let dest = root.join("w.gguf");
        std::fs::write(&part, b"weights").expect("part");
        let digest = sha256_file(&part).expect("hash");
        let verdict =
            finish_write(Some(digest.clone()), &part, &dest, "w.gguf").expect("gate passes");
        assert_eq!(verdict, Integrity::Verified(digest.clone()));
        assert_eq!(std::fs::read(&dest).expect("dest"), b"weights");
        assert!(!part.exists(), "the .part renamed into place");
        assert_eq!(
            std::fs::read_to_string(store::digest_sidecar(&dest)).expect("record"),
            format!("{digest}\n"),
            "the verified digest records beside the GGUF"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A mismatch HARD-refuses naming both digests — and leaves
    /// NOTHING: no artifact, no .part, no record.
    #[test]
    fn finish_write_hard_refuses_a_mismatch_and_leaves_nothing() {
        let root = temp_root("gate-mismatch");
        let part = root.join("w.gguf.part");
        let dest = root.join("w.gguf");
        std::fs::write(&part, b"tampered").expect("part");
        let expected = "ab".repeat(32);
        let refusal = finish_write(Some(expected.clone()), &part, &dest, "w.gguf")
            .expect_err("a mismatch refuses");
        assert!(refusal.contains(&expected), "expected named: {refusal}");
        let actual = {
            use sha2::Digest as _;
            let mut hasher = sha2::Sha256::new();
            hasher.update(b"tampered");
            hex_lower(&hasher.finalize())
        };
        assert!(refusal.contains(&actual), "actual named: {refusal}");
        assert!(!dest.exists(), "no artifact lands");
        assert!(
            !part.exists(),
            "the .part dies — a re-pull must not resume onto tampered bytes"
        );
        assert!(!store::digest_sidecar(&dest).exists(), "no digest record");
        let _ = std::fs::remove_dir_all(root);
    }

    /// No declaration: the file lands unverified (the caller's loud
    /// note is the surface) and nothing records.
    #[test]
    fn finish_write_without_a_declaration_lands_unverified() {
        let root = temp_root("gate-undeclared");
        let part = root.join("w.gguf.part");
        let dest = root.join("w.gguf");
        std::fs::write(&part, b"small").expect("part");
        let verdict = finish_write(None, &part, &dest, "w.gguf").expect("lands");
        assert_eq!(verdict, Integrity::NotDeclared);
        assert_eq!(std::fs::read(&dest).expect("dest"), b"small");
        assert!(!store::digest_sidecar(&dest).exists(), "nothing to record");
        let _ = std::fs::remove_dir_all(root);
    }
}
