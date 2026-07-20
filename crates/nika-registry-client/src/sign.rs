// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The registry-v0.2 signature verification (the ADR-106 follow-up): the
//! AUTHENTICITY half the digest can never give. `sha256(bytes) == pinned`
//! proves the registry agrees with itself; only a publisher signature
//! proves the bytes are THEIRS.
//!
//! The trust shape, in one breath: the entry carries a `[signature]`
//! block — a detached minisign over the ARTIFACT BYTES (the strongest
//! binding — content, not metadata) plus the publisher's ed25519 public
//! key — and this machine anchors that key TOFU in
//! `~/.nika/registry/keys/<publisher>.pub`. The FIRST key seen is
//! recorded only after it verifies; any later key that DIFFERS is a hard
//! refusal ([`ErrKind::KeyChanged`]) — a rewritten index cannot re-key a
//! publisher this machine already trusts. An unsigned entry keeps the
//! v0.1 behavior (digest only · the operator key ceremony decides when
//! the floor flips — see ADR-106's follow-up note).

use std::io::Cursor;
use std::path::Path;

use crate::{ErrKind, RegistryError};

/// The entry's `[signature]` block (registry-v0.2 · closed key set).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SignatureBlock {
    /// The detached minisign over the artifact bytes (the SignatureBox
    /// string, `untrusted comment: …` header included).
    pub signature: String,
    /// The publisher's public key (the PublicKeyBox string).
    pub pubkey: String,
}

/// The `[signature]` keys (closed set — an unknown key is a smuggling
/// channel, refused with the same law the entry's own set carries).
const SIGNATURE_KEYS: [&str; 2] = ["signature", "pubkey"];

/// Parse + vet the `[signature]` table of an entry doc — `None` when the
/// entry predates v0.2 (unsigned · the digest floor alone).
pub(crate) fn parse_signature_block(
    table: Option<&toml_edit::Item>,
) -> Result<Option<SignatureBlock>, RegistryError> {
    let shape = |why: String| RegistryError::new(ErrKind::IndexShape { why });
    let Some(table) = table else {
        return Ok(None);
    };
    let table = table
        .as_table()
        .ok_or_else(|| shape("`[signature]` must be a table".to_owned()))?;
    for (key, _) in table.iter() {
        if !SIGNATURE_KEYS.contains(&key) {
            return Err(shape(format!("unknown [signature] field `{key}`")));
        }
    }
    let get = |key: &str| {
        table
            .get(key)
            .and_then(toml_edit::Item::as_str)
            .map(str::to_owned)
    };
    let (Some(signature), Some(pubkey)) = (get("signature"), get("pubkey")) else {
        return Err(shape(
            "a `[signature]` block needs BOTH `signature` and `pubkey`".to_owned(),
        ));
    };
    Ok(Some(SignatureBlock { signature, pubkey }))
}

/// The TOFU anchor: the first key seen for `publisher` is recorded (only
/// after it verified — never let an unverified key anchor trust); a later
/// key that differs is a hard refusal.
pub(crate) fn tofu_check_and_record(
    keys_dir: &Path,
    publisher: &str,
    pubkey: &str,
) -> Result<(), RegistryError> {
    let path = keys_dir.join(format!("{publisher}.pub"));
    match fs_read(&path) {
        Ok(recorded) => {
            if recorded.trim() == pubkey.trim() {
                Ok(())
            } else {
                Err(RegistryError::new(ErrKind::KeyChanged {
                    publisher: publisher.to_owned(),
                }))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs_mkdir(keys_dir).map_err(|e| {
                RegistryError::env(format!("cannot create {}: {e}", keys_dir.display()))
            })?;
            fs_write(&path, &format!("{pubkey}\n"))
                .map_err(|e| RegistryError::env(format!("cannot write {}: {e}", path.display())))
        }
        Err(e) => Err(RegistryError::env(format!(
            "cannot read the TOFU record {}: {e}",
            path.display()
        ))),
    }
}

/// The one marked fs touch of the TOFU store (the seam checker's exempt
/// line — a local pinned record, like the artifact cache).
fn fs_read(path: &Path) -> std::io::Result<String> {
    #[allow(clippy::let_and_return)] // the seam marker must ride the call line
    let r = std::fs::read_to_string(path); // seam-bypass-ok: local TOFU store · #512
    r
}

/// [`fs_read`]'s twin for the anchor write.
fn fs_write(path: &Path, text: &str) -> std::io::Result<()> {
    #[allow(clippy::let_and_return)] // the seam marker must ride the call line
    let r = std::fs::write(path, text); // seam-bypass-ok: local TOFU store · #512
    r
}

/// [`fs_read`]'s twin for the store directory.
fn fs_mkdir(path: &Path) -> std::io::Result<()> {
    #[allow(clippy::let_and_return)] // the seam marker must ride the call line
    let r = std::fs::create_dir_all(path); // seam-bypass-ok: local TOFU store · #512
    r
}

/// Verify the detached minisign over the artifact bytes — a bad box, a
/// wrong key, or a byte changed anywhere in the stream all fail CLOSED.
pub(crate) fn verify_detached(
    coordinate: &str,
    block: &SignatureBlock,
    bytes: &[u8],
) -> Result<(), RegistryError> {
    let invalid = |why: String| {
        RegistryError::new(ErrKind::SignatureInvalid {
            coordinate: coordinate.to_owned(),
            why,
        })
    };
    let sig_box = minisign::SignatureBox::from_string(&block.signature)
        .map_err(|e| invalid(format!("the signature box does not parse: {e}")))?;
    let pk_box = minisign::PublicKeyBox::from_string(&block.pubkey)
        .map_err(|e| invalid(format!("the public key box does not parse: {e}")))?;
    let pk = pk_box
        .into_public_key()
        .map_err(|e| invalid(format!("the public key does not decode: {e}")))?;
    minisign::verify(&pk, &sig_box, Cursor::new(bytes), true, false, false)
        .map_err(|e| invalid(format!("the signature does not verify: {e}")))
}
