// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trace anchor (the verifiable-run wave · S3) — the journal head,
//! notarized OUTSIDE the journal. Descended from `nika-cli` to the
//! trace-forensics plane 2026-07-20 (the 15k wall — compute descends,
//! render stays; the `trace anchor` / `trace verify` verbs compose it).
//!

//!
//! The seal (S2) made a rewritten journal detectable by anyone holding
//! the run key. The anchor closes the remaining hole: an attacker who
//! rewrites the journal AND re-seals with a key they claim is yours
//! still cannot forge a head the PUBLIC transparency log notarized
//! before the rewrite. Anchoring is an explicit network act — this
//! module is invoked only by `nika trace anchor` (the opt-in IS the
//! verb) and by `nika trace verify`'s ANCHORED tier (offline).
//!
//! Two independent notaries bind the same 32-byte head:
//!
//! - **Rekor v2** (the tile-backed Sigstore log): a `hashedrekord`
//!   0.0.2 entry — `sha512(head)` + an Ed25519ph signature over that
//!   digest with the run key (the server refuses pure ed25519 — see
//!   [`rekor`]). The log's answer (canonicalized body · inclusion
//!   proof · C2SP checkpoint) is verified BEFORE it is trusted, then
//!   persisted.
//! - **RFC 3161** (the Sigstore TSA): a signed timestamp whose imprint
//!   IS the head — Rekor v2 carries no integrated time by design, so
//!   this is the anchor's trusted clock. See [`rfc3161`] for the pins.
//!
//! ## The sidecar (`<trace>.anchor.json` · `anchor_format: 1`)
//!
//! A DETACHED JSON document — the journal is never touched. It carries
//! everything offline verification needs (Rekor v2 serves no read-back
//! API): the head, the entry + proof + checkpoint, and the TSA token.
//! It is written ATOMICALLY (temp file + rename) and only after every
//! verification passes — a failed submission leaves no partial anchor.
//!
//! What the sidecar deliberately does NOT carry: the run public key.
//! Verification resolves keys from custody (`~/.nika/keys/…` ·
//! `--key`), never from the artifact under test — a forged journal
//! would simply ship its own key otherwise.

pub mod rekor;
pub mod rfc3161;
pub mod run;
pub mod tier;

#[cfg(test)]
pub(crate) mod fixtures;

use std::path::{Path, PathBuf};

use base64::Engine as _;
use nika_kernel::http::{HttpPostDyn, HttpRequest};
use serde::{Deserialize, Serialize};

pub use rekor::DEFAULT_REKOR_URL;
pub use rfc3161::DEFAULT_TSA_URL;

/// The sidecar's own chain-head validation is the caller's (the walk's
/// verdict) — this envelope binds exactly these 32 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorSidecar {
    /// The envelope version (`1`) — unknown versions refuse to load.
    pub anchor_format: u32,
    /// The post-seal chain head, lowercase hex (the artifact both
    /// notaries bind — the raw bytes, hex-decoded, are what the Rekor
    /// signature covers and what the TSA imprint contains).
    pub head: String,
    /// The Rekor v2 notarization.
    pub rekor: RekorAnchor,
    /// The RFC 3161 trusted timestamp.
    pub rfc3161: TsaAnchor,
    /// Local observation time (RFC 3339 · UNTRUSTED — the TSA's
    /// `gen_time` is the anchor's clock).
    pub anchored_at: String,
    /// The engine that minted the anchor.
    pub engine: String,
}

/// The Rekor half of the sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorAnchor {
    /// The shard the entry was submitted to.
    pub url: String,
    /// The run key's fingerprint (first 16 hex of the pubkey's
    /// sha256 — the seal's `key_id` voice; verification matches keys,
    /// never trusts a key from the sidecar).
    pub key_id: String,
    /// `TransparencyLogEntry.logIndex` (decimal string).
    pub log_index: String,
    /// `logId.keyId` (base64 — the checkpoint key id in v2).
    pub log_id: String,
    /// The checkpoint's tree size (decimal string).
    pub tree_size: String,
    /// Always 0 under Rekor v2 (no integrated time — the RFC 3161
    /// token is the trusted time; kept so the v1 semantic has a home).
    pub integrated_time: u64,
    /// The canonicalized body (base64 JSON — the log's own entry).
    pub canonicalized_body_b64: String,
    /// The C2SP signed-note checkpoint (the log's tree-head signature).
    pub checkpoint: String,
    /// The RFC 6962 audit path (base64 sibling hashes).
    pub proof_hashes: Vec<String>,
}

/// The RFC 3161 half of the sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsaAnchor {
    /// The authority that signed the token.
    pub tsa_url: String,
    /// The `TimeStampResp` token (base64 DER — self-contained: the
    /// signer certificate rides inside).
    pub token_b64: String,
    /// The `TSTInfo` `genTime` (RFC 3339 — the trusted time; re-derived
    /// from the token at verification, this field is the display copy).
    pub gen_time: String,
}

/// The sidecar's canonical location: `<trace>.anchor.json`, beside the
/// journal it notarizes.
#[must_use]
pub fn sidecar_path(trace: &str) -> PathBuf {
    PathBuf::from(format!("{trace}.anchor.json"))
}

/// Load + validate a sidecar (version gate included) — every parse
/// failure is a reason string, never a panic.
///
/// # Errors
///
/// A reason string when the file cannot be read, does not parse, or
/// speaks a newer `anchor_format`.
pub fn load_sidecar(path: &Path) -> Result<AnchorSidecar, String> {
    let raw = std::fs::read_to_string(path) // seam-bypass-ok: L4 verb reading its own sidecar (same idiom as trace_verify's journal read)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let sidecar: AnchorSidecar = serde_json::from_str(&raw)
        .map_err(|e| format!("{}: not an anchor sidecar: {e}", path.display()))?;
    if sidecar.anchor_format != 1 {
        return Err(format!(
            "{}: anchor_format {} is newer than this engine speaks (1)",
            path.display(),
            sidecar.anchor_format
        ));
    }
    Ok(sidecar)
}

/// The fail-closed write: serialize to a sibling temp file, then
/// rename — a crash mid-write never leaves a partial anchor behind
/// (an absent anchor is honest; a truncated one is a forgery-shaped
/// hazard).
///
/// # Errors
///
/// A reason string when the temp write or the rename fails (no
/// partial sidecar is ever left behind).
pub fn write_sidecar(path: &Path, sidecar: &AnchorSidecar) -> Result<(), String> {
    let json = serde_json::to_string_pretty(sidecar)
        .map_err(|e| format!("cannot serialize the anchor: {e}"))?;
    let tmp = path.with_extension("anchor.json.tmp");
    std::fs::write(&tmp, json) // seam-bypass-ok: L4 verb writing its own sidecar (atomic temp+rename)
        .map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp); // seam-bypass-ok: the same write's cleanup
        format!(
            "cannot rename {} over {}: {e}",
            tmp.display(),
            path.display()
        )
    })
}

/// The run key's raw ed25519 material, derived from the minisign
/// custody pair — the Rekor entry needs the RAW signature (Rekor v2
/// dropped minisign verifiers), so the seed crosses here and nowhere
/// else.
///
/// The extraction is self-checking: the dalek signing key's public
/// half MUST equal the minisign public key's raw bytes, or the layout
/// drifted (an honest error, never a silently wrong key).
pub struct RunKeyMaterial {
    /// The raw 32-byte ed25519 public key.
    pub pk32: [u8; 32],
    /// The dalek signing half (seed ‖ pk, from the minisign box).
    signing: ed25519_dalek::SigningKey,
    /// The minisign public box (the fingerprint input).
    pub pk_box: String,
}

impl RunKeyMaterial {
    /// Sign the artifact with the Ed25519ph voice (RFC 8032 §5.1 —
    /// the only ed25519 the Rekor v2 server accepts). The subtlety:
    /// ONE 64-byte value plays two roles on the wire — the entry's
    /// `digest` is `sha512(artifact)`, and Go's
    /// `VerifyWithOptions(Hash = SHA512)` expects that same value as
    /// the prehashed ph input. So the ph message is the ARTIFACT
    /// itself: `Ed25519ph(artifact)` with `sha512(artifact)` riding as
    /// the entry digest.
    ///
    /// # Errors
    ///
    /// A reason string when the signing itself fails (never silently
    /// unsigned).
    pub fn sign(&self, artifact: &[u8; 32]) -> Result<[u8; 64], String> {
        use sha2::Digest as _;
        let mut prehashed = sha2::Sha512::new();
        prehashed.update(artifact);
        let esk = ed25519_dalek::hazmat::ExpandedSecretKey::from(&self.signing.to_bytes());
        let sig = ed25519_dalek::hazmat::raw_sign_prehashed::<sha2::Sha512, sha2::Sha512>(
            &esk,
            prehashed,
            &self.signing.verifying_key(),
            None,
        )
        .map_err(|e| format!("the run key failed to sign the anchor: {e}"))?;
        Ok(sig.to_bytes())
    }
}

/// The raw ed25519 half of a minisign public box — `None` on any
/// parse or layout surprise (candidate keys fail soft; the seal check
/// is the one that fails hard).
pub fn pk32_of_box(pk_box: &str) -> Option<[u8; 32]> {
    let pk = minisign::PublicKeyBox::from_string(pk_box.trim())
        .and_then(minisign::PublicKey::from_box)
        .ok()?;
    let bytes = pk.to_bytes();
    if bytes.len() != 42 {
        return None;
    }
    bytes[10..42].try_into().ok()
}

/// Derive the raw material from a minisign custody pair.
///
/// The layouts are the pinned `minisign` 0.7 serializations:
/// `PublicKey::to_bytes` = `sig_alg(2) ‖ keynum(8) ‖ pk(32)` and
/// `SecretKey::to_bytes` = `…keynum(8) ‖ sk(64) ‖ chk(32)` with the
/// ed25519 secret at offset 62 — both guarded by the self-check.
///
/// # Errors
///
/// A reason string on any layout drift or a secret/public custody
/// mismatch.
pub fn run_key_material(sk: &minisign::SecretKey, pk_box: &str) -> Result<RunKeyMaterial, String> {
    let pk = minisign::PublicKeyBox::from_string(pk_box)
        .and_then(minisign::PublicKey::from_box)
        .map_err(|e| format!("the run public key does not parse: {e}"))?;
    let pk_bytes = pk.to_bytes();
    if pk_bytes.len() != 42 {
        return Err(format!(
            "the minisign public key serializes to {} bytes (expected 42 — a minisign layout drift?)",
            pk_bytes.len()
        ));
    }
    let pk32: [u8; 32] = pk_bytes[10..42]
        .try_into()
        .map_err(|_| "the minisign public key's raw half is not 32 bytes".to_owned())?;
    let sk_bytes = sk.to_bytes();
    if sk_bytes.len() != 158 {
        return Err(format!(
            "the minisign secret key serializes to {} bytes (expected 158 — a minisign layout drift?)",
            sk_bytes.len()
        ));
    }
    let sk64: &[u8; 64] = sk_bytes[62..126]
        .try_into()
        .map_err(|_| "the minisign secret key's raw half is not 64 bytes".to_owned())?;
    let signing = ed25519_dalek::SigningKey::from_keypair_bytes(sk64).map_err(|e| {
        format!("the minisign secret's ed25519 half failed the self-check (a layout drift?): {e}")
    })?;
    if signing.verifying_key().to_bytes() != pk32 {
        return Err(
            "the run key failed its self-check (secret's public half ≠ the public box — a custody mix-up?)"
                .to_owned(),
        );
    }
    Ok(RunKeyMaterial {
        pk32,
        signing,
        pk_box: pk_box.trim().to_owned(),
    })
}

/// Submit the head to both notaries and assemble the verified sidecar —
/// the seam takes `HttpPostDyn` so tests drive it with `MockHttp`
/// (no network in tests; production hands `ReqwestHttp`).
///
/// Fail-closed at every step: the log's answer is verified (entry
/// binds the head · checkpoint signed by the pinned key · inclusion
/// recomputes) BEFORE it is persisted, and the TSA token is verified
/// before the sidecar exists.
///
/// # Errors
///
/// A reason string on any transport, parse, or verification failure
/// (nothing unverified is ever assembled).
pub async fn submit(
    http: &impl HttpPostDyn,
    rekor_url: &str,
    tsa_url: &str,
    head: &[u8; 32],
    key: &RunKeyMaterial,
) -> Result<AnchorSidecar, String> {
    let rekor = submit_rekor(http, rekor_url, head, key).await?;
    let tsa = submit_tsa(http, tsa_url, head).await?;
    Ok(AnchorSidecar {
        anchor_format: 1,
        head: hex_encode(head),
        rekor,
        rfc3161: tsa,
        anchored_at: jiff::Timestamp::now().to_string(),
        engine: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

/// The Rekor leg: sign → POST → parse → verify the answer end-to-end.
async fn submit_rekor(
    http: &impl HttpPostDyn,
    rekor_url: &str,
    head: &[u8; 32],
    key: &RunKeyMaterial,
) -> Result<RekorAnchor, String> {
    let signature = key.sign(head)?;
    let spki = rekor::spki_der_ed25519(&key.pk32);
    let body = rekor::build_entry_request(head, &signature, &spki);
    let url = format!("{}/api/v2/log/entries", rekor_url.trim_end_matches('/'));
    let response = post(http, &url, "application/json", body.into_bytes()).await?;
    let parsed: serde_json::Value = serde_json::from_slice(&response)
        .map_err(|e| format!("rekor: the response is not JSON: {e}"))?;
    let entry = rekor::parse_entry_response(&parsed)?;
    // The answer is verified BEFORE it is trusted (a compromised or
    // buggy log must not mint a worthless sidecar): the entry binds
    // THIS head + key, the checkpoint is the pinned log's signature,
    // and the inclusion proof recomputes the checkpoint's root. A
    // CUSTOM shard (a private rekor-tiles deployment) cannot offer the
    // pinned Sigstore signature — it gets the consistency half only,
    // and the verb says so (the ANCHORED tier stays out of reach).
    let anchor = RekorAnchor {
        url: rekor_url.to_owned(),
        key_id: crate::seal::fingerprint(&key.pk_box),
        log_index: entry.log_index,
        log_id: entry.log_id,
        tree_size: entry.tree_size,
        integrated_time: 0,
        canonicalized_body_b64: entry.canonicalized_body_b64,
        checkpoint: entry.checkpoint,
        proof_hashes: entry.proof_hashes,
    };
    let (origin, _, _) = rekor::parse_checkpoint_body(&anchor.checkpoint)
        .map_err(|e| format!("rekor: the log's answer failed verification: {e}"))?;
    if origin == rekor::REKOR_ORIGIN {
        verify_rekor_half(&anchor, head, &key.pk32)
            .map_err(|e| format!("rekor: the log's answer failed verification: {e}"))?;
    } else {
        verify_rekor_consistent(&anchor, head, &key.pk32)
            .map_err(|e| format!("rekor: the log's answer failed verification: {e}"))?;
    }
    Ok(anchor)
}

/// The TSA leg: query → POST → verify the token binds the head.
async fn submit_tsa(
    http: &impl HttpPostDyn,
    tsa_url: &str,
    head: &[u8; 32],
) -> Result<TsaAnchor, String> {
    let nonce: [u8; 8] = uuid::Uuid::new_v4().into_bytes()[..8]
        .try_into()
        .map_err(|_| "the nonce source did not yield 8 bytes".to_owned())?;
    let query = rfc3161::build_query(head, &nonce);
    let token = post(http, tsa_url, "application/timestamp-query", query).await?;
    let verified = rfc3161::verify_token(&token, head)
        .map_err(|e| format!("tsa: the token failed verification: {e}"))?;
    Ok(TsaAnchor {
        tsa_url: tsa_url.to_owned(),
        token_b64: base64::engine::general_purpose::STANDARD.encode(token),
        gen_time: verified.gen_time,
    })
}

/// One POST through the kernel seam — non-2xx is an honest failure
/// with a bounded body excerpt.
async fn post(
    http: &impl HttpPostDyn,
    url: &str,
    content_type: &str,
    body: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let mut request = HttpRequest::post(url);
    request
        .headers
        .insert("content-type".to_owned(), content_type.to_owned());
    request.body = Some(bytes::Bytes::from(body));
    let response = http
        .post(request)
        .await
        .map_err(|e| format!("POST {url}: {e}"))?;
    if !(200..300).contains(&response.status) {
        let excerpt: String = String::from_utf8_lossy(&response.body)
            .chars()
            .take(200)
            .collect();
        return Err(format!("POST {url}: HTTP {} — {excerpt}", response.status));
    }
    Ok(response.body.to_vec())
}

/// What offline verification attests (the verify tier renders it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAnchor {
    /// The Rekor log index the head was notarized at.
    pub log_index: String,
    /// The checkpoint's tree size.
    pub tree_size: String,
    /// The TSA's trusted time (RFC 3339).
    pub gen_time: String,
}

/// Offline verification of a loaded sidecar against a recomputed head
/// and a custody-resolved run key — every check in [`rekor`] and
/// [`rfc3161`]'s module docs, in order. `key_id` must match too: an
/// anchor minted by another key is not this key's anchor.
///
/// # Errors
///
/// The first failing check's reason string — a mismatch anywhere is
/// a refusal, never a pass.
pub fn verify_offline(
    sidecar: &AnchorSidecar,
    head: &[u8; 32],
    pk32: &[u8; 32],
    key_id: &str,
) -> Result<VerifiedAnchor, String> {
    if sidecar.head != hex_encode(head) {
        return Err("the sidecar's head is not this journal's head".to_owned());
    }
    if sidecar.rekor.key_id != key_id {
        return Err(format!(
            "the anchor was minted by key {} — not the seal's key {key_id}",
            sidecar.rekor.key_id
        ));
    }
    verify_rekor_half(&sidecar.rekor, head, pk32)?;
    let token = base64::engine::general_purpose::STANDARD
        .decode(&sidecar.rfc3161.token_b64)
        .map_err(|e| format!("tsa: the sidecar's token is not base64: {e}"))?;
    let verified = rfc3161::verify_token(&token, head)?;
    if verified.gen_time != sidecar.rfc3161.gen_time {
        return Err("the sidecar's gen_time does not match its token".to_owned());
    }
    Ok(VerifiedAnchor {
        log_index: sidecar.rekor.log_index.clone(),
        tree_size: sidecar.rekor.tree_size.clone(),
        gen_time: verified.gen_time,
    })
}

/// The Rekor checks shared by submit-time and offline verification:
/// the entry binds the head + key, the checkpoint is the pinned log's,
/// and the audit path recomputes the checkpoint's root.
fn verify_rekor_half(anchor: &RekorAnchor, head: &[u8; 32], pk32: &[u8; 32]) -> Result<(), String> {
    let body = base64::engine::general_purpose::STANDARD
        .decode(&anchor.canonicalized_body_b64)
        .map_err(|e| format!("rekor: the canonicalized body is not base64: {e}"))?;
    rekor::verify_entry_binds_head(&body, head, pk32)?;
    let checkpoint = rekor::verify_checkpoint(&anchor.checkpoint)?;
    verify_inclusion(anchor, &body, checkpoint.tree_size, &checkpoint.root)
}

/// The consistency half for a CUSTOM shard: everything but the pinned
/// checkpoint signature (the entry still binds the head + key, and the
/// audit path still recomputes the checkpoint's CLAIMED root — the
/// log's honesty is just not vouched by a pinned key).
fn verify_rekor_consistent(
    anchor: &RekorAnchor,
    head: &[u8; 32],
    pk32: &[u8; 32],
) -> Result<(), String> {
    let body = base64::engine::general_purpose::STANDARD
        .decode(&anchor.canonicalized_body_b64)
        .map_err(|e| format!("rekor: the canonicalized body is not base64: {e}"))?;
    rekor::verify_entry_binds_head(&body, head, pk32)?;
    let (_, tree_size, root) = rekor::parse_checkpoint_body(&anchor.checkpoint)?;
    verify_inclusion(anchor, &body, tree_size, &root)
}

/// The inclusion-proof check against a (verified or claimed) tree head.
fn verify_inclusion(
    anchor: &RekorAnchor,
    body: &[u8],
    tree_size: u64,
    root: &[u8; 32],
) -> Result<(), String> {
    let recorded_size: u64 = anchor
        .tree_size
        .parse()
        .map_err(|_| "rekor: the sidecar's tree size is not a u64".to_owned())?;
    if tree_size != recorded_size {
        return Err("rekor: the checkpoint's tree size is not the sidecar's".to_owned());
    }
    let log_index: u64 = anchor
        .log_index
        .parse()
        .map_err(|_| "rekor: the sidecar's log index is not a u64".to_owned())?;
    let path = anchor
        .proof_hashes
        .iter()
        .map(|h| {
            base64::engine::general_purpose::STANDARD
                .decode(h)
                .ok()
                .and_then(|raw| <[u8; 32]>::try_from(raw.as_slice()).ok())
                .ok_or_else(|| "rekor: a proof hash is not base64(32 bytes)".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let computed = rekor::rfc6962_audit_root(log_index, tree_size, &rekor::leaf_hash(body), &path)
        .ok_or_else(|| "rekor: the inclusion proof does not fit the tree shape".to_owned())?;
    if &computed != root {
        return Err("rekor: the inclusion proof does not reach the checkpoint's root".to_owned());
    }
    Ok(())
}

/// Why a journal refuses an anchor — the compute half of the
/// refusal; the CLI maps each class to its exit taxonomy (broken and
/// torn are its FILE class, the rest its ENV).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeadRefusal {
    /// The chain does not verify (a forgery-class finding).
    Broken {
        /// FILE line number (1-based).
        line: usize,
    },
    /// A crash mid-write — the anchor would notarize a truncated head.
    TornTail {
        /// Verified event-line count (the torn line excluded).
        events: usize,
    },
    /// A pre-chain journal — there is no head to anchor.
    Unchained,
    /// No events at all.
    Empty,
    /// Not even JSON — not a journal.
    Unreadable {
        /// FILE line number (1-based).
        line: usize,
    },
    /// A verdict class newer than this crate learned.
    Unknown,
}

/// The head one may anchor: the walk's `Intact` verdict ONLY. A broken
/// chain is refused (notarizing a forgery), a torn tail is refused
/// (the anchor would notarize a head that excludes live bytes), and
/// the pre-chain / garbage classes have no head to offer.
///
/// # Errors
///
/// The [`HeadRefusal`] class the walk's verdict maps to.
pub fn head_of(raw: &str) -> Result<(String, [u8; 32], usize), HeadRefusal> {
    let (head, events) = match crate::chain::walk(raw) {
        crate::chain::Verdict::Intact { events, head, .. } => (head, events),
        crate::chain::Verdict::Broken { line, .. } => return Err(HeadRefusal::Broken { line }),
        crate::chain::Verdict::TornTail { events, .. } => {
            return Err(HeadRefusal::TornTail { events });
        }
        crate::chain::Verdict::Unchained => return Err(HeadRefusal::Unchained),
        crate::chain::Verdict::Empty => return Err(HeadRefusal::Empty),
        crate::chain::Verdict::Unreadable { line, .. } => {
            return Err(HeadRefusal::Unreadable { line });
        } // The verdict is #[non_exhaustive] ACROSS crates — a consumer
          // matches the classes above; a newer class lands nowhere else
          // (this arm is the compiler's proof the list is complete TODAY).
    };
    let Some(head32) = hex_decode(&head) else {
        return Err(HeadRefusal::Unknown);
    };
    Ok((head, head32, events))
}

/// Lowercase hex of the 32-byte head (the sidecar + journal voice).
#[must_use]
pub fn hex_encode(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// The inverse of [`hex_encode`] — strict (64 lowercase-hex chars).
#[must_use]
pub fn hex_decode(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_refuses_non_hex() {
        let bytes = [0xabu8; 32];
        let hex = hex_encode(&bytes);
        assert_eq!(hex.len(), 64);
        assert_eq!(hex_decode(&hex), Some(bytes));
        assert_eq!(hex_decode("ab"), None);
        assert_eq!(hex_decode(&"z".repeat(64)), None);
    }

    #[test]
    fn the_sidecar_round_trips_and_the_version_gate_refuses_newer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.ndjson.anchor.json");
        let sidecar = AnchorSidecar {
            anchor_format: 1,
            head: hex_encode(&[7u8; 32]),
            rekor: RekorAnchor {
                url: DEFAULT_REKOR_URL.to_owned(),
                key_id: "0123456789abcdef".to_owned(),
                log_index: "42".to_owned(),
                log_id: "b64".to_owned(),
                tree_size: "43".to_owned(),
                integrated_time: 0,
                canonicalized_body_b64: "b64".to_owned(),
                checkpoint: "note".to_owned(),
                proof_hashes: vec!["b64".to_owned()],
            },
            rfc3161: TsaAnchor {
                tsa_url: DEFAULT_TSA_URL.to_owned(),
                token_b64: "b64".to_owned(),
                gen_time: "2026-07-20T17:24:54Z".to_owned(),
            },
            anchored_at: "2026-07-20T17:25:01Z".to_owned(),
            engine: "0.105.0".to_owned(),
        };
        write_sidecar(&path, &sidecar).expect("write");
        assert_eq!(load_sidecar(&path).expect("load"), sidecar);
        // The version gate: anchor_format 2 refuses with a teaching error.
        let newer = serde_json::to_string(&sidecar)
            .expect("json")
            .replace("\"anchor_format\":1", "\"anchor_format\":2");
        std::fs::write(&path, newer).expect("overwrite");
        let err = load_sidecar(&path).expect_err("v2 refuses");
        assert!(err.contains("anchor_format 2"), "{err}");
    }

    #[test]
    fn the_key_material_self_check_binds_minisign_and_dalek() {
        let pair =
            minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair");
        let pk_box = pair.pk.to_box().expect("pk box").to_string();
        let material = run_key_material(&pair.sk, &pk_box).expect("material");
        // The Ed25519ph signature verifies against the public half —
        // the exact property the Rekor entry relies on.
        let artifact = [9u8; 32];
        let sig = material.sign(&artifact).expect("signs");
        rekor::verify_entry_signature(
            &artifact,
            &material.pk32,
            &ed25519_dalek::Signature::from_bytes(&sig),
        )
        .expect("the Ed25519ph signature verifies");
        // A custody mix-up (another key's box) is refused, not signed.
        let other =
            minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair");
        let other_box = other.pk.to_box().expect("pk box").to_string();
        assert!(run_key_material(&pair.sk, &other_box).is_err());
    }

    /// The headline property: REAL Sigstore artifacts (Rekor entry
    /// 34612959 + the TSA token), captured once through the live verb,
    /// verify fully offline against the pinned keys. The frozen
    /// journal's walked head IS the sidecar's head — the fixture is
    /// coherent end to end.
    #[test]
    fn the_frozen_live_anchor_verifies_offline() {
        let crate::chain::Verdict::Intact { head: walked, .. } =
            crate::chain::walk(fixtures::JOURNAL)
        else {
            unreachable!("the frozen journal must walk intact")
        };
        assert_eq!(walked, fixtures::sidecar().head);
        let verified = verify_offline(
            &fixtures::sidecar(),
            &fixtures::head32(),
            &fixtures::pk32(),
            fixtures::KEY_ID,
        )
        .expect("the frozen live anchor verifies offline");
        assert_eq!(verified.log_index, "34612959");
        assert_eq!(verified.gen_time, "2026-07-20T20:46:49Z");
    }

    /// Every demotion is honest: one wrong byte anywhere — head,
    /// checkpoint, token, key — and the anchor refuses, naming its leg.
    #[test]
    fn a_tampered_anchor_fails_closed_at_each_leg() {
        let sidecar = fixtures::sidecar();
        let head = fixtures::head32();
        let pk = fixtures::pk32();
        // A different head (a rewritten journal).
        let err = verify_offline(&sidecar, &[0u8; 32], &pk, fixtures::KEY_ID)
            .expect_err("a rewritten journal's head");
        assert!(err.contains("not this journal's head"), "{err}");
        // A different run key (a re-sealed forgery).
        let err = verify_offline(&sidecar, &head, &[0u8; 32], fixtures::KEY_ID)
            .expect_err("another key's anchor claim");
        assert!(err.contains("not the run key"), "{err}");
        // A key-id mismatch (custody knows no such key).
        let err = verify_offline(&sidecar, &head, &pk, "0000000000000000")
            .expect_err("a key-id mismatch");
        assert!(err.contains("not the seal's key"), "{err}");
        // A forged checkpoint.
        let mut forged = sidecar.clone();
        forged.rekor.checkpoint = forged.rekor.checkpoint.replacen("34612964", "34612965", 1);
        let err =
            verify_offline(&forged, &head, &pk, fixtures::KEY_ID).expect_err("a forged checkpoint");
        assert!(err.contains("checkpoint"), "{err}");
        // A forged TSA token.
        let mut forged = sidecar.clone();
        forged.rfc3161.token_b64 = base64::engine::general_purpose::STANDARD.encode([0u8; 64]);
        let err =
            verify_offline(&forged, &head, &pk, fixtures::KEY_ID).expect_err("a forged token");
        assert!(err.contains("tsa"), "{err}");
    }

    /// The submit path through `MockHttp`: the frozen key signs, the
    /// mock answers with the frozen Rekor response + TSA token, and
    /// the assembled sidecar matches the live-captured one (the
    /// verification-before-trust runs on this path too).
    #[test]
    fn submit_assembles_a_verified_sidecar_through_the_seam() {
        let mock = nika_kernel_mock::http::MockHttp::new()
            .enqueue_ok(200, fixtures::rekor_response_json())
            .enqueue_ok(200, fixtures::tsa_token_bytes());
        let sidecar = block_on(submit(
            &mock,
            DEFAULT_REKOR_URL,
            DEFAULT_TSA_URL,
            &fixtures::head32(),
            &fixtures::key_material(),
        ))
        .expect("the mocked submit verifies and assembles");
        let frozen = fixtures::sidecar();
        assert_eq!(sidecar.anchor_format, 1);
        assert_eq!(sidecar.head, frozen.head);
        assert_eq!(sidecar.rekor, frozen.rekor);
        assert_eq!(sidecar.rfc3161, frozen.rfc3161);
        // The two POSTs hit the documented endpoints.
        let sent = mock.sent_requests();
        assert_eq!(sent.len(), 2);
        assert_eq!(
            sent[0].url,
            "https://log2025-1.rekor.sigstore.dev/api/v2/log/entries"
        );
        assert_eq!(sent[1].url, DEFAULT_TSA_URL);
    }

    /// A log answer that fails verification kills the submit — no
    /// sidecar is ever assembled from unverified bytes.
    #[test]
    fn a_bad_log_answer_fails_the_submit_closed() {
        let forged = fixtures::rekor_response_json().replacen("34612964", "34612965", 1);
        let mock = nika_kernel_mock::http::MockHttp::new()
            .enqueue_ok(200, forged)
            .enqueue_ok(200, fixtures::tsa_token_bytes());
        let err = block_on(submit(
            &mock,
            DEFAULT_REKOR_URL,
            DEFAULT_TSA_URL,
            &fixtures::head32(),
            &fixtures::key_material(),
        ))
        .expect_err("a forged checkpoint in the log's answer");
        assert!(err.contains("failed verification"), "{err}");
        // And a transport failure is just as closed.
        let mock = nika_kernel_mock::http::MockHttp::new().enqueue_ok(500, "boom");
        assert!(
            block_on(submit(
                &mock,
                DEFAULT_REKOR_URL,
                DEFAULT_TSA_URL,
                &fixtures::head32(),
                &fixtures::key_material(),
            ))
            .is_err()
        );
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }
}
