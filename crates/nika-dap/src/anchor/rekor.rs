// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Rekor v2 (rekor-tiles) client contract + its OFFLINE verification.
//!
//! The write path is ONE endpoint: `POST {shard}/api/v2/log/entries`
//! with a protobuf-JSON `CreateEntryRequest` carrying a
//! `hashedRekordRequestV002`. The exact wire semantics were read from
//! the SERVER source (`pkg/types/hashedrekord/hashedrekord.go` in
//! rekor-tiles) after the public shard rejected the first probe:
//!
//! - `digest` is the artifact's hash under the SIGNING KEY's associated
//!   digest algorithm — the entry's `data.algorithm` derives from the
//!   key type, not the request;
//! - the signature verifies against the DIGEST bytes (`options.
//!   WithDigest(hr.Digest)`), not the raw artifact;
//! - "hashedrekord only permits signing algorithms that prehash the
//!   data. Pure Ed25519 (`PKIX_ED25519`) does not" — the server refuses
//!   pure ed25519 by design, so the run key signs as **Ed25519ph**
//!   (`PKIX_ED25519_PH` · RFC 8032 §5.1 · SHA-512): `digest =
//!   sha512(artifact)`, `signature = Ed25519ph(digest)`. The artifact
//!   itself stays the journal's 32-byte chain head.
//!
//! The response is a `TransparencyLogEntry`: the canonicalized body,
//! the inclusion proof and a C2SP signed-note checkpoint. Contract
//! source: <https://github.com/sigstore/rekor-tiles/blob/main/CLIENTS.md>
//! (including the `hashedRekordV0_0_2` spec key the server emits).
//!
//! The read-back is OFFLINE: no search/verify API exists in v2, so the
//! sidecar carries everything and three independent checks bind the
//! entry to the journal head —
//!
//! 1. the canonicalized body's digest IS `sha512(head)` and its
//!    Ed25519ph signature verifies against the run key;
//! 2. the checkpoint is a signed note from the PINNED log key
//!    (ed25519 · C2SP signed-note + tlog-checkpoint);
//! 3. the RFC 6962 audit path recomputes the checkpoint's root from
//!    the leaf (`sha256(0x00 ‖ canonicalized_body)`) at the entry's
//!    index and tree size.
//!
//! ## The pinned log key
//!
//! `REKOR_ED25519_PUB` is the Sigstore public-good Rekor v2 shard
//! `log2025-1.rekor.sigstore.dev` log signing key, taken from the
//! TUF-distributed `TrustedRoot` (`tlogs[baseUrl == shard].publicKey.
//! rawBytes`, `PKIX_ED25519`), fetched 2026-07-20 from
//! `https://tuf-repo-cdn.sigstore.dev/targets/6494e21ea73fa7ee769f85f57d5a3e6a08725eae1e38c755fc3517c9e6bc0b66.trusted_root.json`
//! (sha256 pinned by `targets.json` v14 — the TUF snapshot chain
//! 165.snapshot.json → 14.targets.json). Its checkpoint key id per
//! C2SP signed-note §Ed25519 is `cf 11 99 15`, matching the shard's
//! live checkpoint signature lines. A shard rotation changes BOTH the
//! URL and the key — verification then fails closed (an honest
//! "unknown log" refusal), never silently passes.

use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use sha2::Digest as _;

/// The public-good Rekor v2 shard this client writes to (the v2 API is
/// NOT served by `rekor.sigstore.dev` — that host is the v1 log; the
/// v2 public instance is sharded, per the rekor-tiles docs). A flag
/// override exists for private deployments; verification below always
/// pins THIS operator's key, so a custom URL anchors without the
/// offline tier claiming Sigstore's attestation.
pub const DEFAULT_REKOR_URL: &str = "https://log2025-1.rekor.sigstore.dev";

/// The checkpoint's key NAME (the C2SP origin): the schema-less shard
/// host. The signed note is only ours when origin AND key id match.
pub const REKOR_ORIGIN: &str = "log2025-1.rekor.sigstore.dev";

/// The shard's raw 32-byte ed25519 log key (base64) — see the module
/// doc for the TUF provenance.
pub const REKOR_ED25519_PUB_B64: &str = "t8rlp1knGwjfbcXAYPYAkn0XiLz1x8O4t0YkEhie244=";

/// The RFC 8410 SPKI prefix for ed25519 (`SEQUENCE { SEQUENCE { OID
/// 1.3.101.112 }, BIT STRING }` — 12 fixed bytes) ahead of the raw key.
const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// The run key's PKIX SPKI DER (the `publicKey.rawBytes` the Rekor
/// entry carries) — the fixed RFC 8410 prefix + the raw ed25519 key.
#[must_use]
pub fn spki_der_ed25519(pk32: &[u8; 32]) -> Vec<u8> {
    let mut der = Vec::with_capacity(44);
    der.extend_from_slice(&ED25519_SPKI_PREFIX);
    der.extend_from_slice(pk32);
    der
}

/// The pinned log key, decoded once per call (a const array the tests
/// also fingerprint).
pub(crate) fn rekor_log_key() -> Result<VerifyingKey, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(REKOR_ED25519_PUB_B64)
        .map_err(|e| format!("the pinned Rekor log key is not base64: {e}"))?;
    let bytes: [u8; 32] = raw
        .try_into()
        .map_err(|_| "the pinned Rekor log key is not 32 bytes".to_owned())?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|e| format!("the pinned Rekor log key is not an ed25519 point: {e}"))
}

/// The artifact's entry digest — `sha512(artifact)`: the hash the
/// signing key's algorithm (`PKIX_ED25519_PH`) associates, per the
/// server's key-derived algorithm rule.
#[must_use]
pub fn entry_digest(artifact: &[u8; 32]) -> [u8; 64] {
    sha2::Sha512::digest(artifact).into()
}

/// The protobuf-JSON `CreateEntryRequest` body for `POST
/// {shard}/api/v2/log/entries` — `hashedRekordRequestV002` with the
/// sha512 artifact digest, the Ed25519ph signature over that digest,
/// and the run key's SPKI (`PKIX_ED25519_PH`).
#[must_use]
pub fn build_entry_request(artifact: &[u8; 32], signature: &[u8; 64], spki_der: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    serde_json::json!({
        "hashedRekordRequestV002": {
            "digest": b64.encode(entry_digest(artifact)),
            "signature": {
                "content": b64.encode(signature),
                "verifier": {
                    "keyDetails": "PKIX_ED25519_PH",
                    "publicKey": { "rawBytes": b64.encode(spki_der) },
                },
            },
        },
    })
    .to_string()
}

/// The parsed half of the `TransparencyLogEntry` response that the
/// sidecar persists (everything offline verification later needs —
/// v2 serves no read-back API).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedEntry {
    /// `logIndex` (decimal string — uint64 in a JSON string).
    pub log_index: String,
    /// `logId.keyId` (base64 — the checkpoint key id, v2 voice).
    pub log_id: String,
    /// `inclusionProof.treeSize` (decimal string).
    pub tree_size: String,
    /// `inclusionProof.hashes` (base64 audit-path nodes).
    pub proof_hashes: Vec<String>,
    /// `inclusionProof.checkpoint.envelope` (the C2SP signed note).
    pub checkpoint: String,
    /// `canonicalizedBody` (base64 JSON — the log's canonical entry).
    pub canonicalized_body_b64: String,
}

/// Parse the `TransparencyLogEntry` JSON — fail closed on any missing
/// or mistyped field (a hostile or drifting server shape is a submit
/// failure, never a partially-trusted sidecar).
///
/// # Errors
///
/// A reason string on any missing/mistyped field or an unsupported
/// kind/version.
pub fn parse_entry_response(body: &serde_json::Value) -> Result<SubmittedEntry, String> {
    fn at<'v>(v: &'v serde_json::Value, path: &[&str]) -> Result<&'v serde_json::Value, String> {
        let mut cur = v;
        for key in path {
            cur = cur
                .get(*key)
                .ok_or_else(|| format!("rekor response: missing {}", path.join(".")))?;
        }
        Ok(cur)
    }
    let string_at = |path: &[&str]| -> Result<String, String> {
        at(body, path)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("rekor response: {} is not a string", path.join(".")))
    };
    let kind = string_at(&["kindVersion", "kind"])?;
    let version = string_at(&["kindVersion", "version"])?;
    if kind != "hashedrekord" || version != "0.0.2" {
        return Err(format!(
            "rekor response: unsupported entry {}/{} (this client speaks hashedrekord/0.0.2)",
            crate::escape_tty(&kind),
            crate::escape_tty(&version)
        ));
    }
    let hashes = at(body, &["inclusionProof", "hashes"])?
        .as_array()
        .ok_or_else(|| "rekor response: inclusionProof.hashes is not an array".to_owned())?
        .iter()
        .map(|h| {
            h.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "rekor response: a proof hash is not a string".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SubmittedEntry {
        log_index: string_at(&["logIndex"])?,
        log_id: string_at(&["logId", "keyId"])?,
        tree_size: string_at(&["inclusionProof", "treeSize"])?,
        proof_hashes: hashes,
        checkpoint: string_at(&["inclusionProof", "checkpoint", "envelope"])?,
        canonicalized_body_b64: string_at(&["canonicalizedBody"])?,
    })
}

/// A verified checkpoint: the tree head the pinned log key signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// The tree size the checkpoint attests.
    pub tree_size: u64,
    /// The tree head hash at that size.
    pub root: [u8; 32],
}

/// Parse the checkpoint body WITHOUT trusting it: `(origin, size,
/// root)` from the note text. Trust arrives only via
/// [`verify_checkpoint`]; this is for consistency checks against an
/// unpinned operator's claim (a private shard) and for display.
///
/// # Errors
///
/// A reason string when the note text is not an
/// `origin ␤ size ␤ root-b64` checkpoint.
pub fn parse_checkpoint_body(envelope: &str) -> Result<(String, u64, [u8; 32]), String> {
    let (text, _sigs) = envelope
        .split_once("\n\n")
        .ok_or_else(|| "checkpoint: no blank line before the signatures".to_owned())?;
    let mut lines = text.lines();
    let origin = lines
        .next()
        .ok_or_else(|| "checkpoint: empty note text".to_owned())?
        .to_owned();
    let tree_size: u64 = lines
        .next()
        .and_then(|l| l.parse().ok())
        .ok_or_else(|| "checkpoint: the tree-size line is not a u64".to_owned())?;
    let root_b64 = lines
        .next()
        .ok_or_else(|| "checkpoint: missing the root-hash line".to_owned())?;
    let root_raw = base64::engine::general_purpose::STANDARD
        .decode(root_b64)
        .map_err(|e| format!("checkpoint: the root hash is not base64: {e}"))?;
    let root: [u8; 32] = root_raw
        .try_into()
        .map_err(|_| "checkpoint: the root hash is not 32 bytes".to_owned())?;
    Ok((origin, tree_size, root))
}

/// Verify a C2SP signed-note checkpoint against the pinned log key:
/// origin + key id must match, the ed25519 signature must verify over
/// the note text (C2SP signed-note §Format: the text INCLUDING its
/// final newline, EXCLUDING the blank separator line), and the body
/// must parse as a tlog-checkpoint (`origin ␤ size ␤ root-b64`, any
/// extension lines ignored per spec).
///
/// # Errors
///
/// A reason string on an unknown origin, a missing/mismatched key id,
/// a signature that does not verify, or an unparseable body.
pub fn verify_checkpoint(envelope: &str) -> Result<Checkpoint, String> {
    let (text, sig_lines) = envelope
        .split_once("\n\n")
        .ok_or_else(|| "checkpoint: no blank line before the signatures".to_owned())?;
    let (origin, tree_size, root) = parse_checkpoint_body(envelope)?;
    if origin != REKOR_ORIGIN {
        return Err(format!(
            "checkpoint: unknown log origin `{}` (pinned: {REKOR_ORIGIN})",
            crate::escape_tty(&origin)
        ));
    }

    let key = rekor_log_key()?;
    let mut verified = false;
    for line in sig_lines.lines() {
        let Some(rest) = line.strip_prefix('\u{2014}') else {
            return Err("checkpoint: a signature line does not start with `— `".to_owned());
        };
        let (name, sig_b64) = rest
            .trim_start_matches(' ')
            .split_once(' ')
            .ok_or_else(|| "checkpoint: a malformed signature line".to_owned())?;
        if name != REKOR_ORIGIN {
            continue; // unknown keys are ignored per the signed-note spec
        }
        let raw = base64::engine::general_purpose::STANDARD
            .decode(sig_b64.trim())
            .map_err(|e| format!("checkpoint: the signature is not base64: {e}"))?;
        if raw.len() != 68 {
            return Err("checkpoint: the signature is not 4+64 bytes".to_owned());
        }
        // The key id MUST match the pinned key's C2SP id before we
        // spend the verify (sha256(name ‖ 0x0A ‖ 0x01 ‖ pk32)[:4]).
        let mut id_preimage = Vec::with_capacity(REKOR_ORIGIN.len() + 34);
        id_preimage.extend_from_slice(REKOR_ORIGIN.as_bytes());
        id_preimage.push(0x0a);
        id_preimage.push(0x01);
        id_preimage.extend_from_slice(&key.to_bytes());
        if sha2::Sha256::digest(&id_preimage)[..4] != raw[..4] {
            continue; // a same-named stranger — ignored, never trusted
        }
        let sig = Signature::from_slice(&raw[4..])
            .map_err(|e| format!("checkpoint: the signature is not 64 bytes: {e}"))?;
        // The signed bytes: the note text INCLUDING its final newline.
        let mut signed = String::with_capacity(text.len() + 1);
        signed.push_str(text);
        signed.push('\n');
        key.verify(signed.as_bytes(), &sig)
            .map_err(|_| "checkpoint: the pinned log key's signature does not verify".to_owned())?;
        verified = true;
    }
    if !verified {
        return Err("checkpoint: no signature from the pinned log key".to_owned());
    }
    Ok(Checkpoint { tree_size, root })
}

/// The RFC 6962 §2.1.1 audit-path recomputation: fold the leaf hash up
/// the proof to the tree root — `None` when the path does not fit the
/// (index, size) shape (a truncated or over-long proof).
#[must_use]
pub fn rfc6962_audit_root(
    leaf_index: u64,
    tree_size: u64,
    leaf_hash: &[u8; 32],
    path: &[[u8; 32]],
) -> Option<[u8; 32]> {
    if leaf_index >= tree_size {
        return None;
    }
    let node = |l: &[u8; 32], r: &[u8; 32]| -> [u8; 32] {
        let mut h = sha2::Sha256::new();
        h.update([0x01]);
        h.update(l);
        h.update(r);
        h.finalize().into()
    };
    let mut fn_ = leaf_index;
    let mut sn = tree_size - 1;
    let mut r = *leaf_hash;
    for p in path {
        if (fn_ & 1) == 1 || fn_ == sn {
            r = node(p, &r);
            while (fn_ & 1) == 0 && fn_ != 0 {
                fn_ >>= 1;
                sn >>= 1;
            }
        } else {
            r = node(&r, p);
        }
        fn_ >>= 1;
        sn >>= 1;
    }
    (sn == 0).then_some(r)
}

/// The one check that binds a Rekor entry to THIS journal head and
/// THIS run key: the canonicalized body is a hashedrekord/0.0.2 entry
/// whose digest is `sha512(head)`, whose Ed25519ph signature verifies
/// against `pk32`, and whose logged public key IS that key.
///
/// # Errors
///
/// A reason string on any mismatch — the entry never passes on a
/// technicality.
pub fn verify_entry_binds_head(
    canonicalized_body: &[u8],
    head: &[u8; 32],
    pk32: &[u8; 32],
) -> Result<(), String> {
    let body: serde_json::Value = serde_json::from_slice(canonicalized_body)
        .map_err(|e| format!("rekor entry: the canonicalized body is not JSON: {e}"))?;
    if body.get("kind").and_then(|k| k.as_str()) != Some("hashedrekord")
        || body.get("apiVersion").and_then(|v| v.as_str()) != Some("0.0.2")
    {
        return Err("rekor entry: not a hashedrekord/0.0.2 body".to_owned());
    }
    // The spec's single entry is the payload (the server spells the key
    // `hashedRekordV0_0_2`; take the sole entry, never the spelling).
    let spec = body
        .get("spec")
        .and_then(|s| s.as_object())
        .filter(|o| o.len() == 1)
        .and_then(|o| o.values().next())
        .ok_or_else(|| "rekor entry: the spec does not hold exactly one payload".to_owned())?;
    let digest_b64 = spec
        .pointer("/data/digest")
        .and_then(|d| d.as_str())
        .ok_or_else(|| "rekor entry: missing data.digest".to_owned())?;
    let digest = base64::engine::general_purpose::STANDARD
        .decode(digest_b64)
        .map_err(|e| format!("rekor entry: data.digest is not base64: {e}"))?;
    if digest != entry_digest(head) {
        return Err("rekor entry: the logged digest is not sha512(this head)".to_owned());
    }
    if spec.pointer("/data/algorithm").and_then(|a| a.as_str()) != Some("SHA2_512") {
        return Err("rekor entry: data.algorithm is not SHA2_512".to_owned());
    }
    if spec
        .pointer("/signature/verifier/keyDetails")
        .and_then(|k| k.as_str())
        != Some("PKIX_ED25519_PH")
    {
        return Err("rekor entry: the verifier is not PKIX_ED25519_PH".to_owned());
    }
    let sig_b64 = spec
        .pointer("/signature/content")
        .and_then(|s| s.as_str())
        .ok_or_else(|| "rekor entry: missing signature.content".to_owned())?;
    let sig_raw = base64::engine::general_purpose::STANDARD
        .decode(sig_b64)
        .map_err(|e| format!("rekor entry: signature.content is not base64: {e}"))?;
    let sig =
        Signature::from_slice(&sig_raw).map_err(|e| format!("rekor entry: bad signature: {e}"))?;
    let key_spki = spec
        .pointer("/signature/verifier/publicKey/rawBytes")
        .and_then(|k| k.as_str())
        .ok_or_else(|| "rekor entry: missing the verifier public key".to_owned())?;
    let spki = base64::engine::general_purpose::STANDARD
        .decode(key_spki)
        .map_err(|e| format!("rekor entry: the verifier key is not base64: {e}"))?;
    if spki != spki_der_ed25519(pk32) {
        return Err("rekor entry: the logged public key is not the run key".to_owned());
    }
    verify_entry_signature(head, pk32, &sig)
}

/// The Ed25519ph check shared by the entry verification and the
/// anchor-side signature: `Ed25519ph(head)` — the ph input IS
/// `sha512(head)`, which is also the entry's digest (Go's
/// `VerifyWithOptions(Hash = SHA512)` over the logged digest).
pub(crate) fn verify_entry_signature(
    head: &[u8; 32],
    pk32: &[u8; 32],
    sig: &Signature,
) -> Result<(), String> {
    use sha2::Digest as _;
    let key = VerifyingKey::from_bytes(pk32)
        .map_err(|e| format!("rekor entry: the run key is not an ed25519 point: {e}"))?;
    let mut prehashed = sha2::Sha512::new();
    prehashed.update(head);
    ed25519_dalek::hazmat::raw_verify_prehashed::<sha2::Sha512, sha2::Sha512>(
        &key, prehashed, None, sig,
    )
    .map_err(|_| "rekor entry: the Ed25519ph signature does not verify over this head".to_owned())
}

/// sha256(0x00 ‖ body) — the RFC 6962 §2.1 leaf hash of the entry.
#[must_use]
pub fn leaf_hash(canonicalized_body: &[u8]) -> [u8; 32] {
    let mut h = sha2::Sha256::new();
    h.update([0x00]);
    h.update(canonicalized_body);
    h.finalize().into()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn hex(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex");
        }
        out
    }

    #[test]
    fn the_spki_is_the_rfc8410_prefix_plus_the_key() {
        let pk = [7u8; 32];
        let der = spki_der_ed25519(&pk);
        assert_eq!(der.len(), 44);
        assert_eq!(
            &der[..12],
            &[
                0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00
            ]
        );
        assert_eq!(&der[12..], &[7u8; 32]);
    }

    #[test]
    fn the_pinned_log_key_matches_its_c2sp_key_id() {
        let key = rekor_log_key().expect("the pinned key decodes");
        let mut pre = Vec::new();
        pre.extend_from_slice(REKOR_ORIGIN.as_bytes());
        pre.push(0x0a);
        pre.push(0x01);
        pre.extend_from_slice(&key.to_bytes());
        let kid = &sha2::Sha256::digest(&pre)[..4];
        // The shard's live checkpoint signature lines open with these
        // four bytes (base64 `zxGZFQ==`), and the TrustedRoot's
        // logId.keyId (`zxGZFVvd…`) shares the prefix per C2SP.
        assert_eq!(kid, [0xcf, 0x11, 0x99, 0x15]);
    }

    /// The live checkpoint fetched from the shard on 2026-07-20
    /// (tree size 34356369) — verified through openssl during the
    /// contract research; the pinned key must verify it here.
    const LIVE_CHECKPOINT: &str = "log2025-1.rekor.sigstore.dev\n34356369\ndA9mMr0smTXgo7nGsE01Zdc5KOOqTs2/zYfdSqxO+ks=\n\n— log2025-1.rekor.sigstore.dev zxGZFQZgTebN28s0D7vKP5gX8R+CsXCved700HCQAHgDHbdkaH3tfx7+DMPkccNhzsnSy7LijLzWMa+evZduylLs0wc=\n";

    #[test]
    fn the_live_checkpoint_verifies_against_the_pinned_key() {
        let cp = verify_checkpoint(LIVE_CHECKPOINT).expect("the live checkpoint verifies");
        assert_eq!(cp.tree_size, 34_356_369);
        let expected = base64::engine::general_purpose::STANDARD
            .decode("dA9mMr0smTXgo7nGsE01Zdc5KOOqTs2/zYfdSqxO+ks=")
            .expect("root b64");
        assert_eq!(cp.root, expected.as_slice());
    }

    #[test]
    fn a_forged_checkpoint_root_or_origin_fails_closed() {
        // One character flipped in the root — same length, decodes
        // fine, and the pinned key's signature no longer verifies.
        let forged = LIVE_CHECKPOINT.replacen(
            "dA9mMr0smTXgo7nGsE01Zdc5KOOqTs2/zYfdSqxO+ks=",
            "eA9mMr0smTXgo7nGsE01Zdc5KOOqTs2/zYfdSqxO+ks=",
            1,
        );
        assert!(verify_checkpoint(&forged).is_err(), "a swapped root");
        let stranger = LIVE_CHECKPOINT.replacen(
            "log2025-1.rekor.sigstore.dev",
            "evil-log.example.com",
            1, // the origin line only — the sig-line name stays, now orphan
        );
        assert!(verify_checkpoint(&stranger).is_err(), "a swapped origin");
        let unsigned = "log2025-1.rekor.sigstore.dev\n7\ndA9mMr0smTXgo7nGsE01Zdc5KOOqTs2/zYfdSqxO+ks=\n\n— other.example AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==\n";
        let err = verify_checkpoint(unsigned).expect_err("no pinned-key signature");
        assert!(
            err.contains("no signature from the pinned log key"),
            "{err}"
        );
    }

    /// The vectors are computed independently from the RFC 6962 §2.1
    /// MTH/PATH definitions (recursive reference, python hashlib).
    #[test]
    fn the_audit_path_recomputes_the_reference_roots() {
        let cases: &[(u64, u64, &str, &[&str], &str)] = &[
            (
                1,
                0,
                "7f9c9e31ac8256ca2f258583df262dbc7d6f68f2a03043d5c99a4ae5a7396ce9",
                &[],
                "7f9c9e31ac8256ca2f258583df262dbc7d6f68f2a03043d5c99a4ae5a7396ce9",
            ),
            (
                2,
                0,
                "7f9c9e31ac8256ca2f258583df262dbc7d6f68f2a03043d5c99a4ae5a7396ce9",
                &["dcffe786ded16d283c663846ad0c4ff26558fccde36ca9d30b2ea19eade9fc0e"],
                "28fb81e496897e0ce886f08602392e9239b65c659041e5202163e58ad898f444",
            ),
            (
                2,
                1,
                "dcffe786ded16d283c663846ad0c4ff26558fccde36ca9d30b2ea19eade9fc0e",
                &["7f9c9e31ac8256ca2f258583df262dbc7d6f68f2a03043d5c99a4ae5a7396ce9"],
                "28fb81e496897e0ce886f08602392e9239b65c659041e5202163e58ad898f444",
            ),
            (
                3,
                2,
                "cba8c596120bdb69debbd923d92cba948bde7c7d06a465a1bb7d98d3116038fa",
                &["28fb81e496897e0ce886f08602392e9239b65c659041e5202163e58ad898f444"],
                "ba8d94b7fbcecae7b81c4c80574fe24734a6917bf9c1ecd66ff3e0c34ead4620",
            ),
            (
                8,
                5,
                "f3ab555d06a67b08ab25039fdbe2a6fcb305c83bc165492ce81d3dea13ec1fbf",
                &[
                    "1da033bf8927ed69376d91533748494f7f5e88c20603dede2afc9bfd43d46f17",
                    "3ebd606fb49a3b46ea6025f6ca81438c59c6644eff2a753ae4b564ffbf0eb06a",
                    "fdea52008cdae79fa8bf806261959e23f5e11681646a2fa2bc9b5e56b32030a2",
                ],
                "f907f23f76aa01b755a614d31ef9832909f44638b4590073301e61e6d01f9a1d",
            ),
            (
                8,
                0,
                "7f9c9e31ac8256ca2f258583df262dbc7d6f68f2a03043d5c99a4ae5a7396ce9",
                &[
                    "dcffe786ded16d283c663846ad0c4ff26558fccde36ca9d30b2ea19eade9fc0e",
                    "fc264939b1ac77b06378c5ece54a7b57b6b6c821eb80627bb674d8785c8dc8ca",
                    "2e2d377b6f1faa1bbb10885d1232b4be13d48ed90a1db9d78ea9caf2c9e8ef43",
                ],
                "f907f23f76aa01b755a614d31ef9832909f44638b4590073301e61e6d01f9a1d",
            ),
            (
                8,
                7,
                "0a958d726efea0a71eb66b07c78738717b22d1fbd44756e82803a5ed461e13f9",
                &[
                    "511c6562982c9bfa05ba4145ca5f2bba85a11a178a4131b5cebde26dd9ffe704",
                    "f1c176552a35e1d035f843d463220b6c85a90ea7f6644980630a6f71a3330ed3",
                    "fdea52008cdae79fa8bf806261959e23f5e11681646a2fa2bc9b5e56b32030a2",
                ],
                "f907f23f76aa01b755a614d31ef9832909f44638b4590073301e61e6d01f9a1d",
            ),
        ];
        for (size, index, leaf, path, root) in cases {
            let path: Vec<[u8; 32]> = path.iter().map(|h| hex(h)).collect();
            assert_eq!(
                rfc6962_audit_root(*index, *size, &hex(leaf), &path),
                Some(hex(root)),
                "size {size} index {index}"
            );
        }
        // Shape failures refuse: index out of range, truncated proof.
        assert_eq!(
            rfc6962_audit_root(
                2,
                2,
                &hex("7f9c9e31ac8256ca2f258583df262dbc7d6f68f2a03043d5c99a4ae5a7396ce9"),
                &[]
            ),
            None
        );
        assert_eq!(
            rfc6962_audit_root(
                5,
                8,
                &hex("f3ab555d06a67b08ab25039fdbe2a6fcb305c83bc165492ce81d3dea13ec1fbf"),
                &[]
            ),
            None
        );
    }

    #[test]
    fn the_entry_request_speaks_the_server_shape() {
        let artifact = [1u8; 32];
        let sig = [2u8; 64];
        let body = build_entry_request(&artifact, &sig, &spki_der_ed25519(&[3u8; 32]));
        let v: serde_json::Value = serde_json::from_str(&body).expect("json");
        let b64 = base64::engine::general_purpose::STANDARD;
        assert_eq!(
            v.pointer("/hashedRekordRequestV002/digest")
                .and_then(|d| d.as_str()),
            Some(b64.encode(entry_digest(&artifact)).as_str())
        );
        assert_eq!(
            v.pointer("/hashedRekordRequestV002/signature/content")
                .and_then(|d| d.as_str()),
            Some(b64.encode(sig).as_str())
        );
        assert_eq!(
            v.pointer("/hashedRekordRequestV002/signature/verifier/keyDetails")
                .and_then(|d| d.as_str()),
            Some("PKIX_ED25519_PH")
        );
    }
}
