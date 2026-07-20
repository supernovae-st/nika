// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run seal (the verifiable-run wave · S2) — ONE ed25519 signature,
//! emitted as the journal's last line, binding the whole chain to the
//! run-key that minted it. The sha256 chain was already tamper-evident;
//! the seal makes it attributable: forging a signed journal now requires
//! the key, not just write access to the file.
//!
//! ## The envelope (`seal_format: 1`)
//!
//! A journal event like any other — `kind: run_sealed`, chained — whose
//! fields carry:
//!
//! - `covers` — `{ head, events, workflow }`: the chain head BEFORE the
//!   seal line (so the seal commits to every prior line) and the
//!   workflow's semantic hash (the proof layer's Merkle commitment), so
//!   journal ↔ workflow bind into one certificate;
//! - `key_id` — the first 16 hex of the public key's sha256 (the TOFU
//!   fingerprint `nika key trust` prints);
//! - `alg` — `"ed25519"` (the envelope is algorithm-versioned from day
//!   one: a PQ migration lands as `seal_format: 2`, never a flag day);
//! - `sig` — the detached minisign over `proof::preimage(HashDomain::
//!   Trace, 1, covers)` — the proof layer's ONE canonicalization voice,
//!   so a second evaluator re-derives the exact bytes.
//!
//! ## Custody
//!
//! OS keychain first (entry `nika/run-signing-key`, the minisign secret
//! box as text); `~/.nika/keys/run-signing.key` (0600) as the no-backend
//! fallback (CI without a session keyring). `nika key init|trust|rotate`
//! manages the lifecycle; an absent key keeps the journal unsigned and
//! says so — sealing is additive, never a gate.

use std::fmt::Write as _;
use std::io::{Cursor, Write as IoWrite};
use std::path::{Path, PathBuf};

use nika_event::{Event, EventKind};
use nika_types::id::EventId;
use nika_types::timestamp::Timestamp;

/// The keychain entries the run-key lives under (the public half rides
/// beside it — `SecretKey` cannot re-derive it).
const KEYRING_SERVICE: &str = "nika";
const KEYRING_USER: &str = "run-signing-key";
const KEYRING_USER_PUB: &str = "run-signing-key.pub";

/// The local custody fallback (no session keyring on this host).
// Env reads are CONFIG paths (HOME only — no secret value crosses here ·
// the scoped `env_flag`/`link_host` precedent).
#[allow(clippy::disallowed_methods)]
fn fallback_key_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| {
            PathBuf::from(home)
                .join(".nika")
                .join("keys")
                .join("run-signing.key")
        })
}

/// The retired-pubkeys ledger (rotation keeps old journals verifiable).
// Same scoped exemption — a config path, never a secret read.
#[allow(clippy::disallowed_methods)]
fn retired_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| {
            PathBuf::from(home)
                .join(".nika")
                .join("keys")
                .join("retired.pub")
        })
}

/// The custody password (v1): `NIKA_RUN_KEY_PASSWORD`, empty allowed —
/// the minisign box is encrypted with it end to end; the OS keychain is
/// the primary protection, this is the file-fallback's own lock.
// The custody password is operator config (a box password, not a
// workflow secret) — the scoped exemption holds.
#[allow(clippy::disallowed_methods)]
fn key_password() -> String {
    std::env::var("NIKA_RUN_KEY_PASSWORD").unwrap_or_default()
}

/// The CI/test key-file override (config path · scoped exemption).
#[allow(clippy::disallowed_methods)]
fn key_file_env(name: &str) -> Result<String, std::env::VarError> {
    std::env::var(name)
}

/// The public fingerprint of a public-key box string — the first 16 hex
/// of its sha256 (what `nika key trust` prints and the seal's `key_id`
/// records).
#[must_use]
pub(crate) fn fingerprint(pubkey_box: &str) -> String {
    let digest = sha256_hex(pubkey_box.as_bytes());
    digest[..16].to_owned()
}

/// The signing half of the run-key, when one exists on this machine
/// (keychain first · the 0600 fallback file second).
pub(crate) fn load_signing_key() -> Option<(minisign::SecretKey, String)> {
    // An explicit key file wins (CI injects keys this way · hermetic tests
    // too — the OS keychain is never the only door).
    if let (Ok(kf), Ok(pf)) = (
        key_file_env("NIKA_RUN_KEY_FILE"),
        key_file_env("NIKA_RUN_PUB_FILE"),
    ) && let Some(pair) = load_from_files(Path::new(&kf), Path::new(&pf))
    {
        return Some(pair);
    }
    // The keychain holds the minisign secret box as text (custody IS the
    // keychain's own encryption).
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        && let Ok(text) = entry.get_password()
        && let Ok(boxed) = minisign::SecretKeyBox::from_string(&text)
        && let Ok(sk) = boxed.into_secret_key(Some(key_password()))
        && let Ok(pub_entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER_PUB)
        && let Ok(pk_box) = pub_entry.get_password()
    {
        return Some((sk, pk_box.trim().to_owned()));
    }
    // Fallback: the 0600 files (unencrypted in v1 — the encrypted upgrade
    // lands with the password/keychain-hydration pass).
    let path = fallback_key_path()?;
    load_from_files(&path, &path.with_extension("pub"))
}

/// Load a (secret box, public box) pair from two files — `None` on any
/// miss (absent file · unparseable box).
fn load_from_files(key: &Path, pub_: &Path) -> Option<(minisign::SecretKey, String)> {
    let text = std::fs::read_to_string(key).ok()?;
    let boxed = minisign::SecretKeyBox::from_string(&text).ok()?;
    let sk = boxed.into_secret_key(Some(key_password())).ok()?;
    let pk_box = std::fs::read_to_string(pub_).ok()?;
    Some((sk, pk_box.trim().to_owned()))
}

/// `nika key init` — generate + store a run-key (idempotent: refuses to
/// clobber an existing one without `--force`).
///
/// # Errors
///
/// A refusal string when a key already exists without `--force`, or when
/// generation/storage fails (keychain unavailable AND the 0600 fallback
/// unwritable).
pub fn key_init(force: bool) -> Result<String, String> {
    if load_signing_key().is_some() && !force {
        return Err(
            "a run-signing key already exists — `nika key trust` prints it, `--force` rotates it"
                .to_owned(),
        );
    }
    let pair = minisign::KeyPair::generate_encrypted_keypair(Some(key_password()))
        .map_err(|e| format!("key generation failed: {e}"))?;
    let sk_box = pair
        .sk
        .to_box(None)
        .map_err(|e| format!("cannot box the signing key: {e}"))?
        .to_string();
    let pk_box = pair
        .pk
        .to_box()
        .map_err(|e| format!("cannot box the public key: {e}"))?
        .to_string()
        .trim()
        .to_owned();
    store_key_boxes(&sk_box, &pk_box)?;
    Ok(fingerprint(&pk_box))
}

/// `nika key trust` — the public key + fingerprint to enroll elsewhere.
#[must_use]
pub fn key_trust() -> Option<(String, String)> {
    let (_, pk_box) = load_signing_key()?;
    Some((pk_box.clone(), fingerprint(&pk_box)))
}

/// `nika key rotate` — retire the current pubkey to the ledger, then
/// generate a fresh key (old journals stay verifiable against the
/// ledger).
///
/// # Errors
///
/// A refusal string when no key exists to rotate, or when the ledger or
/// the new key cannot be written.
pub fn key_rotate() -> Result<String, String> {
    let (_, old_pk) = load_signing_key()
        .ok_or_else(|| "no run-signing key to rotate — `nika key init` first".to_owned())?;
    if let Some(path) = retired_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        writeln!(file, "{old_pk}").map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    key_init(true)
}

/// Store both boxes — the env override first (init writes where load
/// reads), then the keychain, then the 0600 files as fallback.
fn store_key_boxes(sk_box: &str, pk_box: &str) -> Result<(), String> {
    if let (Ok(kf), Ok(pf)) = (
        key_file_env("NIKA_RUN_KEY_FILE"),
        key_file_env("NIKA_RUN_PUB_FILE"),
    ) {
        if let Some(parent) = Path::new(&kf).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        write_0600(Path::new(&kf), sk_box)?;
        return write_0600(Path::new(&pf), pk_box);
    }
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        && entry.set_password(sk_box).is_ok()
        && let Ok(pub_entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER_PUB)
        && pub_entry.set_password(pk_box).is_ok()
    {
        return Ok(());
    }
    let path = fallback_key_path().ok_or("no HOME for the key fallback".to_owned())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    write_0600(&path, sk_box)?;
    write_0600(&path.with_extension("pub"), pk_box)
}

fn write_0600(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    file.write_all(text.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The seal event for one finished run — the terminal line of a signed
/// journal. `covers` binds head + event count + the workflow's semantic
/// hash; the signature rides the proof layer's ONE canonicalization.
pub(crate) fn seal_event(
    run_id: EventId,
    at: Timestamp,
    head: &str,
    events: usize,
    workflow_hash: &str,
    engine: &str,
    sk: &minisign::SecretKey,
    pk_box: &str,
) -> Option<Event> {
    let covers = serde_json::json!({
        "head": head,
        "events": events,
        "workflow": workflow_hash,
        "engine": engine,
    });
    let preimage =
        nika_runtime::proof::preimage(nika_runtime::proof::HashDomain::Trace, 1, &covers);
    let sig_box = minisign::sign(None, sk, Cursor::new(preimage.as_bytes()), None, None).ok()?;
    let fields = vec![
        nika_types::resource::KeyValue::new("seal_format", nika_types::resource::Value::Int(1)),
        nika_types::resource::KeyValue::new(
            "covers",
            nika_types::resource::Value::String(covers.to_string()),
        ),
        nika_types::resource::KeyValue::new(
            "key_id",
            nika_types::resource::Value::String(fingerprint(pk_box)),
        ),
        nika_types::resource::KeyValue::new(
            "alg",
            nika_types::resource::Value::String("ed25519".to_owned()),
        ),
        nika_types::resource::KeyValue::new(
            "sig",
            nika_types::resource::Value::String(sig_box.into_string()),
        ),
    ];
    Some(Event::new(run_id, at, EventKind::RunSealed).with_fields(fields))
}

/// sha256 → lowercase hex (the registry client's idiom, mirrored).
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    hex_lower(&sha2::Sha256::digest(bytes))
}

pub(crate) fn sidecar_path(workflow: &Path) -> PathBuf {
    let mut name = workflow.as_os_str().to_os_string();
    name.push(".minisig");
    PathBuf::from(name)
}

/// The enrolled pub (env · keychain · 0600 file) then retired-ledger lines.
fn enrolled_pubboxes() -> Vec<String> {
    let current = if let Ok(pf) = key_file_env("NIKA_RUN_PUB_FILE") {
        std::fs::read_to_string(pf).ok() // seam-bypass-ok: CLI custody read (L4 surface)
    } else if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER_PUB) {
        entry.get_password().ok()
    } else {
        fallback_key_path().and_then(|p| std::fs::read_to_string(p.with_extension("pub")).ok()) // seam-bypass-ok: same custody read
    };
    let mut out: Vec<String> = current.into_iter().map(|pk| pk.trim().to_owned()).collect();
    let retired = retired_path().and_then(|p| std::fs::read_to_string(p).ok()); // seam-bypass-ok: same custody read
    if let Some(text) = retired {
        let lines = text.lines().map(str::trim);
        out.extend(lines.filter(|l| !l.is_empty()).map(str::to_owned));
    }
    out
}
/// The workflow-signature verdict (each surface maps its own exit codes).
pub(crate) enum WorkflowSig {
    Valid(String),   // verified — the enrolled key's 16-hex TOFU fingerprint
    Invalid(String), // the sidecar exists but does not verify / unknown key
    MissingSidecar,  // no `<file>.minisig` beside the workflow
    NoEnrolledKey,   // nothing enrolled on this machine to verify against
}
/// The injected-key `nika sign` (hermetic tests drive it directly): ONE
/// detached minisign over the EXACT file bytes → `<file>.minisig`.
pub(crate) fn sign_workflow_with(
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
/// `sign --check` / the `--require-signature` gate ([`verify_sidecar`]).
pub(crate) fn check_workflow(path: &Path) -> WorkflowSig {
    let text = std::fs::read_to_string(sidecar_path(path)); // seam-bypass-ok: CLI reads the sidecar beside the named file
    let Ok(text) = text else {
        return WorkflowSig::MissingSidecar;
    };
    let data = std::fs::read(path); // seam-bypass-ok: CLI reads the named workflow file
    let Ok(data) = data else {
        return WorkflowSig::Invalid(format!("cannot read {}", path.display()));
    };
    verify_sidecar(&data, &text, &enrolled_pubboxes())
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
    use nika_event::EventKind;
    use nika_types::id::EventId;
    use nika_types::timestamp::Timestamp;

    use super::*;

    fn keypair() -> (String, minisign::SecretKey) {
        let pair =
            minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair");
        (pair.pk.to_box().expect("pk box").to_string(), pair.sk)
    }

    #[test]
    fn the_fingerprint_is_16_hex_of_the_pubkey_sha256() {
        let (pk, _) = keypair();
        let fp = fingerprint(&pk);
        assert_eq!(fp.len(), 16);
        assert!(fp.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(fingerprint(&pk), fp, "deterministic");
    }

    #[test]
    fn the_seal_event_carries_the_envelope_and_verifies() {
        let (pk, sk) = keypair();
        let ev = seal_event(
            EventId::generate(),
            Timestamp::from_unix_ms(1_700_000_000_000),
            "ab12cd",
            142,
            "wf-hash-7c2a",
            "0.105.0",
            &sk,
            &pk,
        )
        .expect("the seal mints");
        assert!(matches!(ev.kind, EventKind::RunSealed));
        let get = |key: &str| {
            ev.fields
                .iter()
                .find(|f| f.key == key)
                .and_then(|f| match &f.value {
                    nika_types::resource::Value::String(s) => Some(s.clone()),
                    _ => None,
                })
        };
        assert_eq!(get("alg").as_deref(), Some("ed25519"));
        assert_eq!(get("key_id").as_deref(), Some(fingerprint(&pk).as_str()));
        let covers = get("covers").expect("covers present");
        assert!(covers.contains("ab12cd") && covers.contains("wf-hash-7c2a"));

        // The signature verifies against the pubkey box over the SAME
        // preimage (the proof layer's canonicalization).
        let covers_json = serde_json::json!({
            "head": "ab12cd", "events": 142, "workflow": "wf-hash-7c2a", "engine": "0.105.0",
        });
        let preimage =
            nika_runtime::proof::preimage(nika_runtime::proof::HashDomain::Trace, 1, &covers_json);
        let sig_box = minisign::SignatureBox::from_string(&get("sig").expect("sig present"))
            .expect("sig parses");
        let pk_box = minisign::PublicKeyBox::from_string(&pk).expect("pk box parses");
        let pk_key = pk_box.into_public_key().expect("pk decodes");
        minisign::verify(
            &pk_key,
            &sig_box,
            std::io::Cursor::new(preimage.as_bytes()),
            true,
            false,
            false,
        )
        .expect("the seal verifies");
    }

    #[test]
    fn the_file_fallback_round_trips_secret_and_public_boxes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (pk, sk) = keypair();
        let key_path = dir.path().join("run-signing.key");
        let pub_path = dir.path().join("run-signing.pub");
        std::fs::write(&key_path, sk.to_box(None).expect("sk box").to_string()).expect("write key");
        std::fs::write(&pub_path, &pk).expect("write pub");

        let (loaded_key, loaded_pub) =
            load_from_files(&key_path, &pub_path).expect("the pair loads");
        assert_eq!(loaded_pub, pk.trim(), "the public box round-trips");
        // The loaded secret signs the same bytes to the same verifiable
        // signature shape (ed25519 is deterministic).
        let sig = minisign::sign(None, &loaded_key, std::io::Cursor::new(b"x"), None, None)
            .expect("signs");
        let pk_box = minisign::PublicKeyBox::from_string(&pk).expect("pk box");
        let pk_key = pk_box.into_public_key().expect("pk decodes");
        minisign::verify(
            &pk_key,
            &sig,
            std::io::Cursor::new(b"x"),
            true,
            false,
            false,
        )
        .expect("round-trip signature verifies");
    }

    /// `sign → check` round-trip: a workflow signed with an injected key
    /// verifies against that key's enrolled pub, naming its TOFU
    /// fingerprint — and the workflow file itself is untouched.
    #[test]
    fn workflow_sign_then_check_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (pk, sk) = keypair();
        let wf = dir.path().join("flow.nika.yaml");
        let bytes = b"nika: v1\nworkflow:\n  id: signed\n";
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
        std::fs::write(&wf, b"nika: v1\nworkflow:\n  id: honest\n").expect("fixture");
        sign_workflow_with(&wf, &sk, pk.trim()).expect("signs");
        let text = std::fs::read_to_string(sidecar_path(&wf)).expect("sidecar");

        let forged = b"nika: v1\nworkflow:\n  id: FORGED\n";
        let WorkflowSig::Invalid(why) = verify_sidecar(forged, &text, &[pk.trim().to_owned()])
        else {
            unreachable!("a tampered workflow must not verify")
        };
        assert!(why.contains("bad signature"), "{why}");
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
