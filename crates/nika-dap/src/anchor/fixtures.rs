// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The frozen LIVE anchor fixture — real Sigstore artifacts, captured
//! ONCE (2026-07-20) so the offline verification tests never touch the
//! network:
//!
//! - `journal.ndjson` — a 6-event mock-workflow journal, sealed with
//!   the fixture run key (chain head
//!   `c79630a5e194211bcb2b3a38d4057dc69fbb2bc248fe25448bd7d2b18b20e400`);
//! - `sidecar.json` — the anchor minted from that head through the REAL
//!   verb: Rekor v2 entry 34612959 on `log2025-1.rekor.sigstore.dev`
//!   (checkpoint + inclusion proof verified at capture) plus an RFC
//!   3161 token from `timestamp.sigstore.dev` (`gen_time`
//!   2026-07-20T20:46:49Z);
//! - `run-signing.{key,pub}` — the THROWAWAY fixture run key (minisign
//!   boxes, empty password). It anchors nothing but test data and is
//!   committed deliberately: the tests re-derive key material from it
//!   to drive the submit path through `MockHttp`. Its fingerprint is
//!   `1e772a7b922d7be3`.
//!
//! Regeneration procedure (when the pinned TSA leaf nears its 2035
//! expiry, or the shard rotates): run a mock workflow with
//! `NIKA_RUN_KEY_FILE`/`NIKA_RUN_PUB_FILE` pointing at a fresh
//! throwaway key, `nika trace anchor` the journal, and copy the four
//! files back here.
//!
//! This module is compiled under `#[cfg(test)]` only (the gate sits at
//! the `mod fixtures;` declaration); the file-level allow is the
//! hygiene vector's marker that frozen-fixture `.expect()`s are test
//! code, never production paths.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use base64::Engine as _;

use super::{AnchorSidecar, hex_decode};

/// The frozen journal (sealed · 6 events).
#[cfg(test)]
pub(crate) const JOURNAL: &str = include_str!("fixtures/journal.ndjson");
/// The frozen sidecar (verified live at capture).
#[cfg(test)]
pub(crate) const SIDECAR_JSON: &str = include_str!("fixtures/sidecar.json");
/// The throwaway fixture key's secret box (empty password).
#[cfg(test)]
pub(crate) const SECRET_BOX: &str = include_str!("fixtures/run-signing.key");
/// The throwaway fixture key's public box.
#[cfg(test)]
pub(crate) const PUBLIC_BOX: &str = include_str!("fixtures/run-signing.pub");

/// The fixture run key's fingerprint (the seal's `key_id`).
#[cfg(test)]
pub(crate) const KEY_ID: &str = "1e772a7b922d7be3";

/// The parsed fixture sidecar.
#[cfg(test)]
pub(crate) fn sidecar() -> AnchorSidecar {
    serde_json::from_str(SIDECAR_JSON).expect("the frozen sidecar parses")
}

/// The fixture head as raw bytes.
#[cfg(test)]
pub(crate) fn head32() -> [u8; 32] {
    hex_decode(&sidecar().head).expect("the frozen head is 64 hex")
}

/// The fixture run key's raw ed25519 public half.
#[cfg(test)]
pub(crate) fn pk32() -> [u8; 32] {
    let pk = minisign::PublicKeyBox::from_string(PUBLIC_BOX)
        .and_then(minisign::PublicKey::from_box)
        .expect("the frozen public box parses");
    let bytes = pk.to_bytes();
    bytes[10..42].try_into().expect("the frozen key's raw half")
}

/// The fixture run key's signing material (secret box · empty
/// password — see the module doc for why this key is committed). The
/// box is a rs-minisign 0.7 LEGACY shape (kdf-marked, plaintext
/// material): it doubles as the regression vector for the legacy open.
#[cfg(test)]
pub(crate) fn key_material() -> super::RunKeyMaterial {
    let sk = minisign::SecretKeyBox::from_string(SECRET_BOX)
        .ok()
        .and_then(|b| crate::seal::open_fixture_box(&b))
        .expect("the frozen secret box opens");
    super::run_key_material(&sk, PUBLIC_BOX).expect("the frozen key's material")
}

/// A Rekor `TransparencyLogEntry` response body rebuilt from the frozen
/// sidecar's fields — the `MockHttp` answer for the submit-path test
/// (shaped exactly as `parse_entry_response` expects).
#[cfg(test)]
pub(crate) fn rekor_response_json() -> String {
    let sidecar = sidecar();
    let (_, _, root) = super::rekor::parse_checkpoint_body(&sidecar.rekor.checkpoint)
        .expect("the frozen checkpoint parses");
    serde_json::json!({
        "logIndex": sidecar.rekor.log_index,
        "logId": { "keyId": sidecar.rekor.log_id },
        "kindVersion": { "kind": "hashedrekord", "version": "0.0.2" },
        "integratedTime": "0",
        "inclusionPromise": null,
        "inclusionProof": {
            "logIndex": sidecar.rekor.log_index,
            "rootHash": base64::engine::general_purpose::STANDARD.encode(root),
            "treeSize": sidecar.rekor.tree_size,
            "hashes": sidecar.rekor.proof_hashes,
            "checkpoint": { "envelope": sidecar.rekor.checkpoint },
        },
        "canonicalizedBody": sidecar.rekor.canonicalized_body_b64,
    })
    .to_string()
}

/// The TSA token bytes (the `MockHttp` answer for the TSA leg).
#[cfg(test)]
pub(crate) fn tsa_token_bytes() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(&sidecar().rfc3161.token_b64)
        .expect("the frozen token is base64")
}
