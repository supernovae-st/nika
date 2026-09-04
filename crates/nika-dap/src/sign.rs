// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow author-binding (`nika sign` · `run --require-signature`)
//! — ONE detached minisign over the EXACT workflow bytes →
//! `<file>.minisig`, verified against the enrolled keys (the current
//! pub · every `retired.pub` ledger line) with a 16-hex key-id match.
//! A swapped `.nika.yaml` in a repo is a supply-chain vector; this is
//! the compute half — the verb (`verbs::sign`) and the run gate keep
//! the clap surface + the exit-code rendering and delegate here.
//! Descended from `nika-cli`'s `seal.rs` 2026-07-21 (the 15k wall —
//! compute descends, render stays; the key custody lives in
//! [`crate::seal`], ONE home for both).

use std::fmt::Write as _;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::seal::{
    KEYRING_USER_PUB, fallback_key_path, fingerprint, key_file_env, keyring_entry, retired_path,
};

#[must_use]
pub fn sidecar_path(workflow: &Path) -> PathBuf {
    let mut name = workflow.as_os_str().to_os_string();
    name.push(".minisig");
    PathBuf::from(name)
}

/// The enrolled pub (env · keychain · 0600 file) then retired-ledger lines.
fn enrolled_pubboxes() -> Vec<String> {
    let current = if let Ok(pf) = key_file_env("NIKA_RUN_PUB_FILE") {
        std::fs::read_to_string(pf).ok() // seam-bypass-ok: CLI custody read (L4 surface)
    } else if let Some(pk) =
        keyring_entry(KEYRING_USER_PUB).and_then(|entry| entry.get_password().ok())
    {
        Some(pk)
    } else {
        fallback_key_path().and_then(|p| std::fs::read_to_string(p.with_extension("pub")).ok()) // seam-bypass-ok: same custody read
    };
    let retired = retired_path().and_then(|p| std::fs::read_to_string(p).ok()); // seam-bypass-ok: same custody read
    enrolled_from_text(current.as_deref(), retired.as_deref())
}

fn enrolled_from_text(current: Option<&str>, retired: Option<&str>) -> Vec<String> {
    current
        .into_iter()
        .chain(retired)
        .flat_map(crate::seal::parse_public_boxes)
        .collect()
}
/// The workflow-signature verdict (each surface maps its own exit codes).
pub enum WorkflowSig {
    Valid(String),   // verified — the enrolled key's 16-hex TOFU fingerprint
    Invalid(String), // the sidecar exists but does not verify / unknown key
    MissingSidecar,  // no `<file>.minisig` beside the workflow
    NoEnrolledKey,   // nothing enrolled on this machine to verify against
}
/// The injected-key `nika sign` (hermetic tests drive it directly): ONE
/// detached minisign over the EXACT file bytes → `<file>.minisig`.
///
/// # Errors
///
/// A reason string when the file cannot be read, signed, or the
/// sidecar written.
pub fn sign_workflow_with(
    path: &Path,
    sk: &minisign::SecretKey,
    pk_box: &str,
) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?; // seam-bypass-ok: CLI reads the named workflow file
    let comment = format!("nika-fp:{}", fingerprint(pk_box));
    let sig = minisign::sign(None, sk, Cursor::new(&data), Some(&comment), None)
        .map_err(|e| format!("cannot sign {}: {e}", path.display()))?;
    let write = std::fs::write(sidecar_path(path), sig.into_string()); // seam-bypass-ok: CLI writes the sidecar beside the named file
    write.map_err(|e| format!("cannot write {}: {e}", sidecar_path(path).display()))?;
    Ok(fingerprint(pk_box))
}
/// `sign --check` / the `--require-signature` gate (`verify_sidecar`).
#[must_use]
pub fn check_workflow(path: &Path) -> WorkflowSig {
    let text = std::fs::read_to_string(sidecar_path(path)); // seam-bypass-ok: `nika sign --check` reads the sidecar beside the named file
    let Ok(text) = text else {
        return WorkflowSig::MissingSidecar;
    };
    let data = std::fs::read(path); // seam-bypass-ok: `nika sign --check` reads the named workflow file
    let Ok(data) = data else {
        return WorkflowSig::Invalid(format!("cannot read {}", path.display()));
    };
    verify_sidecar(&data, &text, &enrolled_pubboxes())
}

/// Verify the sidecar beside `path` against bytes already captured by the
/// caller. `nika run --require-signature` uses this form so the signature
/// judges the exact execution candidate, never a second pathname read.
#[must_use]
pub fn check_workflow_bytes(path: &Path, data: &[u8]) -> WorkflowSig {
    check_workflow_bytes_against(path, data, &enrolled_pubboxes())
}

fn check_workflow_bytes_against(path: &Path, data: &[u8], candidates: &[String]) -> WorkflowSig {
    let text = std::fs::read_to_string(sidecar_path(path)); // seam-bypass-ok: CLI reads the sidecar beside the named file
    let Ok(text) = text else {
        return WorkflowSig::MissingSidecar;
    };
    verify_sidecar(data, &text, candidates)
}
/// The pure half of `sign --check`: 16-hex key-id match, then minisign-verify.
fn verify_sidecar(data: &[u8], sig_text: &str, candidates: &[String]) -> WorkflowSig {
    let Ok(sig) = minisign::SignatureBox::from_string(sig_text) else {
        return WorkflowSig::Invalid("unparseable sidecar".to_owned());
    };
    if candidates.is_empty() {
        return WorkflowSig::NoEnrolledKey;
    }
    let pubs = candidates.iter().filter_map(|pk_box| {
        let pk = minisign::PublicKeyBox::from_string(pk_box).ok()?;
        pk.into_public_key().ok().map(|pk| (pk_box, pk))
    });
    let mut known_key = false;
    for (pk_box, pk) in pubs {
        if pk.keynum() != sig.keynum() {
            continue;
        }
        known_key = true;
        if minisign::verify(&pk, &sig, Cursor::new(data), true, false, false).is_ok() {
            return WorkflowSig::Valid(fingerprint(pk_box));
        }
    }
    WorkflowSig::Invalid(if known_key {
        "bad signature — the enrolled key does not verify these bytes".to_owned()
    } else {
        format!("unknown key {} — not enrolled", hex_lower(sig.keynum()))
    })
}
fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn keypair() -> (String, minisign::SecretKey) {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
        (pair.pk.to_box().expect("pk box").to_string(), pair.sk)
    }

    #[test]
    fn a_retired_key_still_verifies_after_the_current_key_changes() {
        let (old, secret) = keypair();
        let (current, _) = keypair();
        let bytes = b"synthetic signed bytes";
        let signature = minisign::sign(None, &secret, Cursor::new(bytes), None, None)
            .expect("synthetic signature")
            .into_string();
        let candidates = enrolled_from_text(Some(&current), Some(&old));
        assert_eq!(
            candidates,
            vec![current.trim().to_owned(), old.trim().to_owned()]
        );
        let WorkflowSig::Valid(found) = verify_sidecar(bytes, &signature, &candidates) else {
            panic!("the retired public box must remain whole and verifiable");
        };
        assert_eq!(found, fingerprint(old.trim()));
    }
    /// `sign → check` round-trip: a workflow signed with an injected key
    /// verifies against that key's enrolled pub, naming its TOFU
    /// fingerprint — and the workflow file itself is untouched.
    #[test]
    fn workflow_sign_then_check_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (pk, sk) = keypair();
        let wf = dir.path().join("flow.nika.yaml");
        let bytes = b"nika: signed\n";
        std::fs::write(&wf, bytes).expect("fixture");

        let fp = sign_workflow_with(&wf, &sk, pk.trim()).expect("signs");
        assert_eq!(fp, fingerprint(pk.trim()));
        assert_eq!(
            std::fs::read(&wf).expect("workflow still readable"),
            bytes,
            "the workflow file never changes (canonical bytes stay hashable)"
        );
        let sidecar = sidecar_path(&wf);
        assert!(sidecar.ends_with("flow.nika.yaml.minisig"));
        let text = std::fs::read_to_string(&sidecar).expect("sidecar written");
        assert!(
            text.contains("trusted comment: nika-fp:"),
            "authenticated provenance rides the trusted comment: {text}"
        );
        let WorkflowSig::Valid(got) = verify_sidecar(bytes, &text, &[pk.trim().to_owned()]) else {
            unreachable!("a fresh signature must verify")
        };
        assert_eq!(got, fp);
    }

    /// One flipped byte in the workflow turns the verdict into the
    /// bad-signature class (the key is known — the bytes are not his).
    #[test]
    fn tampered_workflow_is_rejected_as_bad_signature() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (pk, sk) = keypair();
        let wf = dir.path().join("flow.nika.yaml");
        std::fs::write(&wf, b"nika: honest\n").expect("fixture");
        sign_workflow_with(&wf, &sk, pk.trim()).expect("signs");
        let text = std::fs::read_to_string(sidecar_path(&wf)).expect("sidecar");

        let forged = b"nika: v1\nworkflow:\n  id: FORGED\n";
        let WorkflowSig::Invalid(why) = verify_sidecar(forged, &text, &[pk.trim().to_owned()])
        else {
            unreachable!("a tampered workflow must not verify")
        };
        assert!(why.contains("bad signature"), "{why}");
    }

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "the signature TOCTOU fixture uses an OS thread held at exact barriers"
    )]
    fn signature_gate_verifies_captured_b_not_reread_pathname_a() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (pk, sk) = keypair();
        let wf = dir.path().join("flow.nika.yaml");
        let captured = Arc::new(Barrier::new(2));
        let replaced = Arc::new(Barrier::new(2));
        std::fs::write(&wf, b"nika: B\n").expect("write B");

        let reader_path = wf.clone();
        let reader_captured = Arc::clone(&captured);
        let reader_replaced = Arc::clone(&replaced);
        let reader = std::thread::spawn(move || {
            let bytes = std::fs::read(reader_path).expect("capture B");
            reader_captured.wait();
            reader_replaced.wait();
            bytes
        });

        captured.wait();
        std::fs::write(&wf, b"nika: A\n").expect("replace pathname with A");
        sign_workflow_with(&wf, &sk, pk.trim()).expect("sign pathname A");
        replaced.wait();
        let bytes_b = reader.join().expect("reader");

        assert!(matches!(
            check_workflow_bytes_against(&wf, &bytes_b, &[pk.trim().to_owned()]),
            WorkflowSig::Invalid(why) if why.contains("bad signature")
        ));
        assert!(matches!(
            check_workflow_bytes_against(&wf, b"nika: A\n", &[pk.trim().to_owned()]),
            WorkflowSig::Valid(_)
        ));
    }

    /// A sidecar minted by a key this machine does not enroll names the
    /// unknown-key class (never a silent pass, never a confusing 'bad').
    #[test]
    fn unknown_signing_key_is_named() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (pk_a, sk_a) = keypair();
        let (pk_b, _) = keypair();
        let wf = dir.path().join("flow.nika.yaml");
        std::fs::write(&wf, b"nika: v1\n").expect("fixture");
        sign_workflow_with(&wf, &sk_a, pk_a.trim()).expect("signs");
        let text = std::fs::read_to_string(sidecar_path(&wf)).expect("sidecar");

        match verify_sidecar(b"nika: v1\n", &text, &[pk_b.trim().to_owned()]) {
            WorkflowSig::Invalid(why) => assert!(why.contains("unknown key"), "{why}"),
            _ => unreachable!("a key that never signed must not verify"),
        }
    }

    /// Honest empty states: no enrolled key is ENV-class, a missing
    /// sidecar is its own class, a garbage sidecar is invalid — never a
    /// panic, never a forged pass.
    #[test]
    fn verify_empty_states_are_honest() {
        assert!(matches!(
            verify_sidecar(b"x", "garbage", &[]),
            WorkflowSig::Invalid(_)
        ));
        let (_, sk) = keypair();
        let sig = minisign::sign(None, &sk, std::io::Cursor::new(b"x"), None, None)
            .expect("signs")
            .into_string();
        assert!(matches!(
            verify_sidecar(b"x", &sig, &[]),
            WorkflowSig::NoEnrolledKey
        ));
        let dir = tempfile::tempdir().expect("tempdir");
        let wf = dir.path().join("flow.nika.yaml");
        std::fs::write(&wf, b"nika: v1\n").expect("fixture");
        assert!(matches!(check_workflow(&wf), WorkflowSig::MissingSidecar));
    }
}
