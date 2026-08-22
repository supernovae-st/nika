// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run seal (the verifiable-run wave · S2) — ONE ed25519 signature,
//! DESCENDED from `nika-cli` to the trace-forensics plane 2026-07-21
//! (the 15k wall — compute descends, render stays; re-exported at the
//! old path there · the workflow author-binding half lives in
//! [`crate::sign`]).
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
//!   journal ↔ workflow bind into one certificate. F-P2 folds the run's
//!   TEARDOWN in additively ([`SealTeardown`]): `receipt_digest` (the
//!   receipt the run folded at teardown), `budgets` (ρ consumed vs the
//!   certificate's ceiling), `effects` (ε exercised vs declared) — the
//!   format stays `1` (the verify tier reads `covers` tolerantly:
//!   unknown keys are ignored, the signature covers the whole object);
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
pub(crate) const KEYRING_SERVICE: &str = "nika";
const KEYRING_USER: &str = "run-signing-key";
pub(crate) const KEYRING_USER_PUB: &str = "run-signing-key.pub";

/// Whether the OS keychain may be consulted at all · `NIKA_KEYCHAIN=off` skips it.
///
/// WHY THIS OPT-OUT EXISTS · a macOS keychain ACL is bound to the requesting
/// BINARY. A cargo test binary is `target/debug/deps/<crate>-<hash>` and that
/// hash changes on every recompile, so approving one with "Always Allow" grants
/// a binary that will never exist again · the prompt returns on the next build,
/// and the next, forever, on any machine where these items exist. There is no
/// operator-side gesture that makes it stop.
///
/// It skips ONLY the keychain. The env and file custody paths are untouched, so
/// a real run on a developer machine loses nothing it had; a machine that keeps
/// its run-key in the keychain simply must not set this. Accepted values ·
/// `off` · `0` · `false` · `no` (case-insensitive). Anything else, including
/// absent, keeps the keychain in play.
// Env reads are CONFIG paths (a flag, never a secret value · the scoped
// `env_flag` precedent).
#[allow(clippy::disallowed_methods)]
pub(crate) fn keychain_enabled() -> bool {
    // seam-bypass-ok: build-host flag · no secret crosses here
    keychain_consults(
        keychain_flag(std::env::var("NIKA_KEYCHAIN").ok().as_deref()),
        running_as_a_cargo_test_binary(std::env::current_exe().ok().as_deref()),
        std::io::IsTerminal::is_terminal(&std::io::stderr()),
    )
}

/// Whether the OS keychain may be opened for this process. Pure so the
/// pipe/ACL hang is a fixture, not a session reproduction.
///
/// A captured stderr (pipe · CI · an agent `Popen`) cannot complete a
/// macOS keychain ACL prompt. Asking anyway is a hang after a green
/// card (`nika try 01-hello` piped · sampled 2026-08-22 on
/// `SecKeychainFindGenericPassword`). File and env custody stay.
fn keychain_consults(flag: bool, test_bin: bool, stderr_tty: bool) -> bool {
    flag && !test_bin && stderr_tty
}

/// Is this process a cargo test binary? Cargo builds them at
/// `<target>/<profile>/deps/<name>-<16 hex>`, and it is that per-compile
/// hash which makes a keychain ACL unable to stick. An installed `nika`
/// (or `cargo run`, which lands beside `deps/`, not inside it) is
/// unaffected and keeps its custody exactly as before.
///
/// Pure over the path so it is provable without spawning anything.
fn running_as_a_cargo_test_binary(exe: Option<&Path>) -> bool {
    let Some(exe) = exe else { return false };
    if exe.parent().and_then(Path::file_name) != Some(std::ffi::OsStr::new("deps")) {
        return false;
    }
    let Some(stem) = exe.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    // `<crate>-<hash>` · the hash half is what changes on every compile.
    match stem.rsplit_once('-') {
        Some((name, hash)) => {
            !name.is_empty() && hash.len() >= 8 && hash.chars().all(|c| c.is_ascii_hexdigit())
        }
        None => false,
    }
}

/// The pure half · the testable unit. Env-setting tests race under the default
/// parallel harness, so the parsing is proven here and the read stays a
/// one-liner above.
fn keychain_flag(raw: Option<&str>) -> bool {
    match raw {
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "off" | "0" | "false" | "no"
        ),
        None => true,
    }
}

/// The local custody fallback (no session keyring on this host).
// Env reads are CONFIG paths (HOME only — no secret value crosses here ·
// the scoped `env_flag`/`link_host` precedent).
#[allow(clippy::disallowed_methods)]
pub(crate) fn fallback_key_path() -> Option<PathBuf> {
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
pub(crate) fn retired_path() -> Option<PathBuf> {
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
pub(crate) fn key_file_env(name: &str) -> Result<String, std::env::VarError> {
    std::env::var(name)
}

/// The public fingerprint of a public-key box string — the first 16 hex
/// of its sha256 (what `nika key trust` prints and the seal's `key_id`
/// records).
#[must_use]
pub fn fingerprint(pubkey_box: &str) -> String {
    let digest = sha256_hex(pubkey_box.as_bytes());
    digest[..16].to_owned()
}

/// The signing half of the run-key, when one exists on this machine
/// (keychain first · the 0600 fallback file second).
#[must_use]
pub fn load_signing_key() -> Option<(minisign::SecretKey, String)> {
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
    if keychain_enabled()
        && let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        && let Ok(text) = entry.get_password()
        && let Ok(boxed) = minisign::SecretKeyBox::from_string(&text)
        && let Some(sk) = open_secret_box(boxed, &key_password())
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
    let sk = open_secret_box(boxed, &key_password())?;
    let pk_box = std::fs::read_to_string(pub_).ok()?;
    Some((sk, pk_box.trim().to_owned()))
}

/// Open a secret-key box under the custody password — plus the pre-0.9
/// rs-minisign legacy: 0.7 wrote boxes MARKED encrypted (kdf alg `Sc`)
/// whose material stayed PLAINTEXT when the password was empty (the
/// crate skipped `encrypt()` but kept the marker and a valid checksum).
/// 0.9 always runs the KDF, so `into_secret_key` with an empty password
/// XOR-corrupts those boxes into a « wrong password » miss. The fallback
/// [`plaintext_probe`] lets the crate's own checksum judge: a legacy
/// box's checksum covers its plaintext material, so
/// [`minisign::SecretKey::is_encrypted`] is false — while a genuinely
/// encrypted box (a real password exists somewhere) stays refused.
pub(crate) fn open_secret_box(
    boxed: minisign::SecretKeyBox,
    password: &str,
) -> Option<minisign::SecretKey> {
    // The box text is borrowed out BEFORE the box is consumed — the probe
    // only matters when the configured password is empty (a non-empty
    // password either opens the box or doesn't).
    let probe = if password.is_empty() {
        Some(boxed.to_string())
    } else {
        None
    };
    if let Ok(sk) = boxed.into_secret_key(Some(password.to_owned())) {
        return Some(sk);
    }
    plaintext_probe(&probe?)
}

/// Parse a box's payload and accept it when the crate's own checksum
/// proves the material is plaintext — the two unencrypted shapes: the
/// standard `KDF_NONE` box (0.9 `generate_unencrypted_keypair`, real
/// minisign with an empty password) and the 0.7 legacy marker.
fn plaintext_probe(text: &str) -> Option<minisign::SecretKey> {
    let mut lines = text.lines().map(str::to_owned);
    let payload = lines.next().and_then(|_| lines.next())?;
    let bytes = base64_decode(&payload)?;
    let sk = minisign::SecretKey::from_bytes(&bytes).ok()?;
    (!sk.is_encrypted()).then_some(sk)
}

/// The fixture open (tests only) — the frozen vector's custody is empty
/// BY CONSTRUCTION, so no password literal ever rides a call site
/// (codeql `rust/hard-coded-cryptographic-value`).
#[cfg(test)]
pub(crate) fn open_fixture_box(boxed: &minisign::SecretKeyBox) -> Option<minisign::SecretKey> {
    plaintext_probe(&boxed.to_string())
}

/// The PUBLIC half of the run-key, when a usable PAIR exists on this
/// machine — the same precedence as [`load_signing_key`] (env override,
/// keychain, 0600 fallback) and the same pair honesty, WITHOUT ever
/// decrypting: the secret half is probed parse-only
/// (`secret_box_parses` — envelope shape + the crate's own structural
/// parse; no password, no prompt, no `into_secret_key`). `key trust`,
/// the init clobber check and the rotation ledger stay off the secret
/// path, and an orphaned `.pub` (secret half gone or corrupt) is never
/// announced as a key this machine can seal with.
#[must_use]
pub fn load_public_box() -> Option<String> {
    // The explicit file override IS the custody when both vars are set
    // (init writes there, load reads there) — a broken pair under it
    // answers "no usable key", never a silent fall-through to another
    // tier.
    if let (Ok(kf), Ok(pf)) = (
        key_file_env("NIKA_RUN_KEY_FILE"),
        key_file_env("NIKA_RUN_PUB_FILE"),
    ) {
        return public_from_files(Path::new(&kf), Path::new(&pf));
    }
    if keychain_enabled()
        && let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        && let Ok(text) = entry.get_password()
        && secret_box_parses(&text)
        && let Ok(pub_entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER_PUB)
        && let Ok(pk_box) = pub_entry.get_password()
    {
        return Some(pk_box.trim().to_owned());
    }
    let path = fallback_key_path()?;
    public_from_files(&path, &path.with_extension("pub"))
}

/// Parse-only pair probe: the text has a minisign secret box's shape —
/// the comment line + a base64 payload the crate's own
/// [`minisign::SecretKey::from_bytes`] accepts structurally (the same
/// two-line split `SecretKey::from_box` performs BEFORE any password
/// work; `SecretKeyBox::from_string` validates nothing — it wraps).
/// Never decrypts (no password, no kdf, no checksum): the sealing path
/// ([`load_signing_key`]) is the only reader that pays that cost, and
/// the only place secret material may flow.
fn secret_box_parses(text: &str) -> bool {
    let mut lines = text.lines();
    let (Some(_comment), Some(payload)) = (lines.next(), lines.next()) else {
        return false;
    };
    base64_decode(payload).is_some_and(|bytes| minisign::SecretKey::from_bytes(&bytes).is_ok())
}

/// The public box of a (key, pub) file pair — `None` on any miss
/// (absent or unparseable secret half · absent pub), mirroring
/// [`load_from_files`] minus the decrypt.
fn public_from_files(key: &Path, pub_: &Path) -> Option<String> {
    let text = std::fs::read_to_string(key).ok()?;
    if !secret_box_parses(&text) {
        return None;
    }
    let pk_box = std::fs::read_to_string(pub_).ok()?;
    Some(pk_box.trim().to_owned())
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
    if load_public_box().is_some() && !force {
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
    let pk_box = load_public_box()?;
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
    let old_pk = load_public_box()
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
    if keychain_enabled()
        && let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
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
/// The teardown-less path: `covers` carries the classic four fields,
/// byte-unchanged ([`seal_event_with`] folds the F-P2 teardown in).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn seal_event(
    run_id: EventId,
    at: Timestamp,
    head: &str,
    events: usize,
    workflow_hash: &str,
    engine: &str,
    sk: &minisign::SecretKey,
    pk_box: &str,
) -> Option<Event> {
    seal_event_with(
        run_id,
        at,
        head,
        events,
        workflow_hash,
        engine,
        None,
        sk,
        pk_box,
    )
}

/// The run's teardown facts (F-P2 · LOT-1) — what the seal's `covers`
/// attests BEYOND the chain: the receipt the run folded at teardown
/// (its digest rides; the body stays the evidence pack's surface), the
/// budgets ρ consumed against the certificate's ceiling, the
/// effects ε exercised against the declared bound, the signed-memory
/// fold (F-P8 · the admitted set's digest + the rejection count), and the
/// failed run's quarantine fold (F-P14 · la dette du run). Every field is
/// ADDITIVE — `seal_format` stays 1: the verify tier reads `covers`
/// tolerantly (unknown keys are ignored, verified by the tier tests),
/// and the signature's canonicalization covers the whole object either
/// way. A `None`/empty fact keeps its key OUT of the covers (absent is
/// honest — never a fabricated zero).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SealTeardown {
    /// The run's semantic hash hex (the receipt's `proves` — the same
    /// Merkle root the boot manifest's `semantic_hash` carries).
    pub proves: Option<String>,
    /// The check certificate as JSON (the receipt's `certificate`).
    pub certificate: Option<serde_json::Value>,
    /// The judged assertions as receipt entries (`{assert, level}`).
    pub assertions: Vec<serde_json::Value>,
    /// The terminal outcome word (`completed` · `failed` · `paused`).
    pub outcome: Option<String>,
    /// The budgets ρ fold (consumed vs ceiling), pre-shaped by the caller.
    pub budgets: Option<serde_json::Value>,
    /// The effects ε fold (exercised vs declared), pre-shaped by the caller.
    pub effects: Option<serde_json::Value>,
    /// The memory fold (F-P8 · SMSR signed memory): `{"v": 1, "stores":
    /// [{store, set_digest, admitted_count, rejected} | {store,
    /// error}]}` — the admitted set NAMED by ONE constant-size digest
    /// (blake3 over the sorted digests — the set's digest IS its name,
    /// so the seal line never crosses the chain walk's 1 MiB line bound,
    /// H13), the rejections counted (unsigned · bad signature · relabel
    /// = rejected, never filtered) and named — the first
    /// `MAX_JOURNALED_REJECTIONS` as `memory_entry_rejected` journal
    /// events, a flood as ONE `memory_rejections_summary` — BEFORE this
    /// seal (the chain covers them), a failed store walk NAMED in place.
    /// Pre-shaped by the caller from `nika_store::seal_fold`. Rides ONLY
    /// when the run's CWD holds a `.nika/memory/` store — `None` keeps
    /// the key OUT (absent is honest).
    pub memory: Option<serde_json::Value>,
    /// The named per-entry rejections the `memory` fold counts (the
    /// law's third leg): [`crate::journal::seal_journal_with`] journals
    /// one `memory_entry_rejected` event per entry BEFORE the seal event
    /// mints, so the chain the seal signs covers the names the fold
    /// counts. Empty without a store — nothing to name.
    pub memory_rejected: Vec<crate::memory::RejectedEntry>,
    /// The quarantine fold (F-P14 · NEP-0017 · « obligation de fin — la
    /// dette du run »): where the failed run's semi-written outputs
    /// moved (`{dir, outputs: [{path, quarantined_to} | {path, error,
    /// action}]}`), pre-shaped by the caller. Rides the FAILURE lane
    /// only — a clean or paused run attests nothing (the key stays
    /// OUT); the saga/compensation palier is declared P2.
    pub quarantine: Option<serde_json::Value>,
}

impl SealTeardown {
    /// An empty teardown (INV-019): the seal carries its classic four
    /// `covers` fields and nothing more.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The seal event with the run's teardown facts folded into `covers`
/// (F-P2): the receipt's digest (folded HERE — the chain head and count
/// it binds are this seal's own), the budgets ρ, the effects ε, and the
/// failed run's quarantine fold (F-P14).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn seal_event_with(
    run_id: EventId,
    at: Timestamp,
    head: &str,
    events: usize,
    workflow_hash: &str,
    engine: &str,
    teardown: Option<&SealTeardown>,
    sk: &minisign::SecretKey,
    pk_box: &str,
) -> Option<Event> {
    let mut covers = serde_json::json!({
        "head": head,
        "events": events,
        "workflow": workflow_hash,
        "engine": engine,
    });
    if let Some(teardown) = teardown {
        extend_covers(&mut covers, head, events, teardown);
    }
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

/// Fold the teardown facts into the seal's `covers` (F-P2 · ADDITIVE).
fn extend_covers(
    covers: &mut serde_json::Value,
    head: &str,
    events: usize,
    teardown: &SealTeardown,
) {
    // The receipt digest: the receipt is folded AT TEARDOWN with the
    // chain facts only this seal knows (the pre-seal head and count —
    // the seal cannot cover its own bytes). The receipt body stays the
    // evidence pack's surface; the digest binds it here. The verdict's
    // `sealed` is written `true`: it becomes true when this line lands
    // — the seal attests WHAT HAPPENED, it never promises the future.
    if let (Some(proves), Some(certificate), Some(outcome)) = (
        teardown.proves.as_deref(),
        teardown.certificate.clone(),
        teardown.outcome.as_deref(),
    ) {
        let trace_verdict = serde_json::json!({
            "outcome": outcome,
            "chain": "intact",
            "events": events,
            "head": head,
            "sealed": true,
        });
        let receipt = nika_runtime::proof::receipt::build_receipt(
            proves,
            certificate,
            trace_verdict,
            teardown.assertions.clone(),
            crate::evidence::LOCK_UNRECORDED,
        );
        if let Some(digest) = receipt.get("digest").and_then(serde_json::Value::as_str) {
            covers["receipt_digest"] = serde_json::Value::String(digest.to_owned());
        }
    }
    if let Some(budgets) = &teardown.budgets {
        covers["budgets"] = budgets.clone();
    }
    if let Some(effects) = &teardown.effects {
        covers["effects"] = effects.clone();
    }
    // F-P8 · the signed-memory fold rides verbatim: the seal pins the
    // verified SET (its ONE set digest + the admitted count) and the
    // rejection count beside it; `None` keeps the key OUT (absent is
    // honest).
    if let Some(memory) = &teardown.memory {
        covers["memory"] = memory.clone();
    }
    // F-P14 · la dette du run: the failed run's quarantine fold rides
    // verbatim (the moves happened BEFORE this seal — the end attested
    // here includes them); a clean/paused run folded `None` and the key
    // stays OUT (absent is honest).
    if let Some(quarantine) = &teardown.quarantine {
        covers["quarantine"] = quarantine.clone();
    }
}

/// Strict standard-alphabet base64 → bytes (`None` on any
/// non-canonical shape). Hand-rolled beside [`hex_lower`] for the same
/// reason: one wire idiom does not buy a codec dependency.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn sextet(b: u8) -> Option<u32> {
        match b {
            b'A'..=b'Z' => Some(u32::from(b - b'A')),
            b'a'..=b'z' => Some(26 + u32::from(b - b'a')),
            b'0'..=b'9' => Some(52 + u32::from(b - b'0')),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let raw = s.as_bytes();
    if raw.is_empty() || !raw.len().is_multiple_of(4) {
        return None;
    }
    let pad = raw.iter().rev().take_while(|&&b| b == b'=').count();
    if pad > 2 {
        return None;
    }
    let body = raw.get(..raw.len() - pad)?;
    let mut out = Vec::with_capacity(raw.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for &b in body {
        acc = (acc << 6) | sextet(b)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).ok()?);
            acc &= (1_u32 << bits) - 1;
        }
    }
    Some(out)
}

/// sha256 → lowercase hex (the registry client's idiom, mirrored).
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    hex_lower(&sha2::Sha256::digest(bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// The candidate public keys for the SEALED verify tier, in pick
/// order: an explicit key file first (the `--key` voice), then the
/// custody default, then every line of the retired ledger (rotation
/// keeps old journals verifiable). Each entry is `(public box, source
/// label)` — the source rides into the tier's report line.
///
/// # Errors
///
/// A reason string when the explicit key file cannot be read (the
/// invocation's own failure — never a forgery signal).
pub fn candidate_pubkeys(key_file: Option<&Path>) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    if let Some(path) = key_file {
        // seam-bypass-ok: reading the operator-named key file (the custody idiom above)
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read --key {}: {e}", path.display()))?;
        out.push((text.trim().to_owned(), path.display().to_string()));
    }
    for name in ["run-signing.pub", "retired.pub"] {
        let Some(path) = keys_path(name) else {
            continue;
        };
        // seam-bypass-ok: reading the operator's own key custody (the idiom above)
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() {
                out.push((line.to_owned(), format!("~/.nika/keys/{name}")));
            }
        }
    }
    Ok(out)
}

/// `~/.nika/keys/<name>` — a config path, never a secret read (the
/// scoped exemption the custody helpers above already carry).
#[allow(clippy::disallowed_methods)]
fn keys_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".nika").join("keys").join(name))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    /// The keychain opt-out · both directions, and the default that must stay ON.
    ///
    /// Absent means ENABLED · a flag that fails closed here would silently
    /// narrow a real run's trust set, which is not what an ergonomics escape
    /// hatch is allowed to do.
    #[test]
    fn keychain_flag_reads_off_and_defaults_on() {
        for off in ["off", "OFF", "0", "false", "False", "no", " off "] {
            assert!(!super::keychain_flag(Some(off)), "{off} must disable");
        }
        for on in ["on", "1", "true", "yes", "", "anything"] {
            assert!(super::keychain_flag(Some(on)), "{on} must keep it enabled");
        }
        assert!(super::keychain_flag(None), "absent means enabled");
    }

    use nika_event::EventKind;
    use nika_types::id::EventId;
    use nika_types::timestamp::Timestamp;

    use super::*;

    fn keypair() -> (String, minisign::SecretKey) {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
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

        // (e) the teardown-less regression guard: `covers` is EXACTLY
        // the classic four keys (the pre-F-P2 wire, byte-unchanged).
        let parsed: serde_json::Value = serde_json::from_str(&covers).expect("covers parses");
        let keys: Vec<&String> = parsed.as_object().expect("an object").keys().collect();
        assert_eq!(
            keys,
            ["engine", "events", "head", "workflow"],
            "the classic covers is the classic four, nothing more"
        );

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

    /// F-P2 (b) · the extended seal: the teardown facts ride `covers`
    /// additively — the receipt digest RECOMPUTES from the same inputs,
    /// and the signature verifies over the extended object (the proof
    /// layer's ONE canonicalization covers whatever `covers` carries).
    #[test]
    fn the_teardown_seal_carries_the_run_end_attestation_and_verifies() {
        let (pk, sk) = keypair();
        let mut teardown = SealTeardown::new();
        teardown.proves = Some("wf-semantic-7c2a".to_owned());
        teardown.certificate = Some(serde_json::json!({
            "task_attempts": { "constant": 2, "terms": [] }
        }));
        teardown.assertions = vec![serde_json::json!({
            "assert": "no_secret_egress", "level": "TraceVerified"
        })];
        teardown.outcome = Some("completed".to_owned());
        teardown.budgets = Some(serde_json::json!({
            "spent_usd": 0.012, "priced_calls": 3, "unpriced_calls": 0, "budget_exceeded": false
        }));
        teardown.effects = Some(serde_json::json!({ "exercised": 2, "escapes": 0 }));
        let ev = seal_event_with(
            EventId::generate(),
            Timestamp::from_unix_ms(1_700_000_000_000),
            "ab12cd",
            142,
            "wf-semantic-7c2a",
            "0.105.0",
            Some(&teardown),
            &sk,
            &pk,
        )
        .expect("the seal mints");
        let get = |key: &str| {
            ev.fields
                .iter()
                .find(|f| f.key == key)
                .and_then(|f| match &f.value {
                    nika_types::resource::Value::String(s) => Some(s.clone()),
                    _ => None,
                })
        };
        let covers: serde_json::Value =
            serde_json::from_str(&get("covers").expect("covers present")).expect("covers parses");

        // The classic four ride unchanged; the teardown keys join them.
        assert_eq!(covers["head"], serde_json::json!("ab12cd"));
        assert_eq!(covers["events"], serde_json::json!(142));
        assert_eq!(covers["workflow"], serde_json::json!("wf-semantic-7c2a"));
        assert_eq!(covers["engine"], serde_json::json!("0.105.0"));
        assert_eq!(covers["budgets"]["priced_calls"], serde_json::json!(3));
        assert_eq!(covers["effects"]["exercised"], serde_json::json!(2));

        // The receipt digest recomputes from the same inputs (the
        // seal's own pre-seal chain facts + the run's certificate).
        let verdict = serde_json::json!({
            "outcome": "completed", "chain": "intact",
            "events": 142, "head": "ab12cd", "sealed": true,
        });
        let receipt = nika_runtime::proof::receipt::build_receipt(
            "wf-semantic-7c2a",
            serde_json::json!({ "task_attempts": { "constant": 2, "terms": [] } }),
            verdict,
            vec![serde_json::json!({
                "assert": "no_secret_egress", "level": "TraceVerified"
            })],
            crate::evidence::LOCK_UNRECORDED,
        );
        assert_eq!(
            covers["receipt_digest"]
                .as_str()
                .expect("the receipt digest rides"),
            receipt["digest"].as_str().expect("the recomputed digest"),
            "the seal's receipt digest IS the folded receipt's own digest"
        );
        assert!(
            nika_runtime::proof::receipt::verify(&receipt, "wf-semantic-7c2a"),
            "the folded receipt verifies its self-digest"
        );

        // The signature verifies over the EXTENDED covers.
        let preimage =
            nika_runtime::proof::preimage(nika_runtime::proof::HashDomain::Trace, 1, &covers);
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
        .expect("the extended seal verifies");
    }

    /// F-P14 (NEP-0017 · la dette du run) · the failed run's quarantine
    /// fold rides `covers` additively and the signature verifies over
    /// the extended object; a `None` keeps the key OUT (a clean run
    /// attests nothing — absent is honest).
    #[test]
    fn the_quarantine_fold_rides_the_covers_and_verifies() {
        let (pk, sk) = keypair();
        let mut teardown = SealTeardown::new();
        teardown.quarantine = Some(serde_json::json!({
            "dir": ".nika/quarantine/2026-07-29T13-40-01Z-a3f2",
            "outputs": [
                { "path": "out.txt", "quarantined_to": ".nika/quarantine/2026-07-29T13-40-01Z-a3f2/out.txt" },
                { "path": "gone.txt", "error": "No such file or directory (os error 2)", "action": "left_in_place" }
            ]
        }));
        let ev = seal_event_with(
            EventId::generate(),
            Timestamp::from_unix_ms(1_700_000_000_000),
            "ab12cd",
            142,
            "wf-hash-7c2a",
            "0.105.0",
            Some(&teardown),
            &sk,
            &pk,
        )
        .expect("the seal mints");
        let get = |key: &str| {
            ev.fields
                .iter()
                .find(|f| f.key == key)
                .and_then(|f| match &f.value {
                    nika_types::resource::Value::String(s) => Some(s.clone()),
                    _ => None,
                })
        };
        let covers: serde_json::Value =
            serde_json::from_str(&get("covers").expect("covers present")).expect("covers parses");

        // The classic four ride unchanged; the quarantine fold joins them
        // VERBATIM (moved entries AND the stated miss).
        assert_eq!(covers["head"], serde_json::json!("ab12cd"));
        assert_eq!(
            covers["quarantine"]["dir"],
            serde_json::json!(".nika/quarantine/2026-07-29T13-40-01Z-a3f2")
        );
        assert_eq!(
            covers["quarantine"]["outputs"][0]["quarantined_to"],
            serde_json::json!(".nika/quarantine/2026-07-29T13-40-01Z-a3f2/out.txt")
        );
        assert_eq!(
            covers["quarantine"]["outputs"][1]["action"],
            serde_json::json!("left_in_place"),
            "the stated miss rides too — never silent"
        );

        // The signature verifies over the EXTENDED covers.
        let preimage =
            nika_runtime::proof::preimage(nika_runtime::proof::HashDomain::Trace, 1, &covers);
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
        .expect("the quarantine seal verifies");

        // The no-fake-zero posture: a teardown WITHOUT the fold seals a
        // covers with no `quarantine` key at all.
        let clean = seal_event_with(
            EventId::generate(),
            Timestamp::from_unix_ms(1_700_000_000_001),
            "ab12cd",
            142,
            "wf-hash-7c2a",
            "0.105.0",
            Some(&SealTeardown::new()),
            &sk,
            &pk,
        )
        .expect("the seal mints");
        let covers: serde_json::Value = serde_json::from_str(
            &clean
                .fields
                .iter()
                .find(|f| f.key == "covers")
                .and_then(|f| match &f.value {
                    nika_types::resource::Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .expect("covers present"),
        )
        .expect("covers parses");
        assert!(
            covers.get("quarantine").is_none(),
            "absent is honest — a clean run attests nothing: {covers}"
        );
    }

    /// Mint a seal with the given teardown and return its parsed
    /// `covers` — the shared spine of the fold-riding tests.
    #[cfg(test)]
    fn mint_and_covers(
        teardown: &SealTeardown,
        sk: &minisign::SecretKey,
        pk: &str,
    ) -> serde_json::Value {
        let ev = seal_event_with(
            EventId::generate(),
            Timestamp::from_unix_ms(1_700_000_000_000),
            "ab12cd",
            142,
            "wf-hash-7c2a",
            "0.105.0",
            Some(teardown),
            sk,
            pk,
        )
        .expect("the seal mints");
        let raw = ev
            .fields
            .iter()
            .find(|f| f.key == "covers")
            .and_then(|f| match &f.value {
                nika_types::resource::Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .expect("covers present");
        serde_json::from_str(&raw).expect("covers parses")
    }

    /// F-P8 (SMSR signed memory) · the memory fold rides `covers`
    /// additively — the admitted set's ONE digest + the counts — and the
    /// signature verifies over the extended object; a `None` keeps the
    /// key OUT (no `.nika/memory/` in the run's CWD ⇒ nothing to attest).
    #[test]
    fn the_memory_fold_rides_the_covers_and_verifies() {
        let (pk, sk) = keypair();
        let mut teardown = SealTeardown::new();
        teardown.memory = Some(serde_json::json!({
            "v": 1,
            "stores": [
                {
                    "store": "default",
                    "set_digest": "b2f1a0c94e6d41f8a0d3c5b7e9f1a2c4b6d8e0f2a4c6b8d0e2f4a6c8b0d2e4f6",
                    "admitted_count": 1,
                    "rejected": 1,
                },
            ],
        }));
        let ev = seal_event_with(
            EventId::generate(),
            Timestamp::from_unix_ms(1_700_000_000_000),
            "ab12cd",
            142,
            "wf-hash-7c2a",
            "0.105.0",
            Some(&teardown),
            &sk,
            &pk,
        )
        .expect("the seal mints");
        let covers: serde_json::Value = mint_and_covers(&teardown, &sk, &pk);

        // The classic four ride unchanged; the memory fold joins them
        // VERBATIM (the set digest AND the counts).
        assert_eq!(covers["head"], serde_json::json!("ab12cd"));
        assert_eq!(
            covers["memory"]["stores"][0]["store"],
            serde_json::json!("default")
        );
        assert_eq!(
            covers["memory"]["stores"][0]["set_digest"]
                .as_str()
                .expect("the set digest rides")
                .len(),
            64,
            "the admitted set is NAMED by ONE constant-size digest (H13)"
        );
        assert_eq!(
            covers["memory"]["stores"][0]["admitted_count"],
            serde_json::json!(1),
            "the set's size rides beside its name"
        );
        assert_eq!(
            covers["memory"]["stores"][0]["rejected"],
            serde_json::json!(1),
            "the rejection count rides beside the admitted set"
        );

        // The signature verifies over the EXTENDED covers.
        let get = |key: &str| {
            ev.fields
                .iter()
                .find(|f| f.key == key)
                .and_then(|f| match &f.value {
                    nika_types::resource::Value::String(s) => Some(s.clone()),
                    _ => None,
                })
        };
        let preimage =
            nika_runtime::proof::preimage(nika_runtime::proof::HashDomain::Trace, 1, &covers);
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
        .expect("the memory seal verifies");

        // The no-fake-zero posture: a teardown WITHOUT the fold seals a
        // covers with no `memory` key at all.
        let covers: serde_json::Value = mint_and_covers(&SealTeardown::new(), &sk, &pk);
        assert!(
            covers.get("memory").is_none(),
            "absent is honest — a run with no memory store attests nothing: {covers}"
        );
    }

    /// (a) The pair probe hands back the public box + a 16-hex
    /// fingerprint WITHOUT decrypting: the secret box is
    /// password-locked and no password reaches this path —
    /// `into_secret_key` would refuse it, the parse-only probe does
    /// not care. Reintroduce a decrypt on the trust path and this test
    /// goes red.
    #[test]
    fn the_public_probe_reads_the_pair_without_decrypting() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pair = minisign::KeyPair::generate_encrypted_keypair(Some("locked-away".to_owned()))
            .expect("keypair");
        let key_path = dir.path().join("run-signing.key");
        let pub_path = dir.path().join("run-signing.pub");
        std::fs::write(&key_path, pair.sk.to_box(None).expect("sk box").to_string())
            .expect("write key");
        let pk = pair.pk.to_box().expect("pk box").to_string();
        std::fs::write(&pub_path, &pk).expect("write pub");

        let pub_box = public_from_files(&key_path, &pub_path).expect("the pair answers");
        assert_eq!(pub_box, pk.trim(), "the public box comes back verbatim");
        let fp = fingerprint(&pub_box);
        assert_eq!(fp.len(), 16);
        assert!(fp.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    /// (b) An orphaned pub is NOT a key: secret half absent → `None` ·
    /// secret half unparseable → `None` — `nika key trust` keeps
    /// saying « no run-signing key » instead of announcing a pub
    /// nothing can seal with (the honesty `load_signing_key` always
    /// had).
    #[test]
    fn an_orphaned_pub_is_not_a_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (pk, _) = keypair();
        let key_path = dir.path().join("run-signing.key");
        let pub_path = dir.path().join("run-signing.pub");
        std::fs::write(&pub_path, &pk).expect("write pub");

        assert!(
            public_from_files(&key_path, &pub_path).is_none(),
            "absent secret half → no announcement"
        );
        std::fs::write(&key_path, "not a minisign box").expect("write junk");
        assert!(
            public_from_files(&key_path, &pub_path).is_none(),
            "unparseable secret half → no announcement"
        );
        // Shaped like a box (comment + valid base64) but structurally
        // truncated — the crate-parse arm of the probe rejects it too.
        std::fs::write(&key_path, "untrusted comment: stub\nAAAA\n").expect("write stub");
        assert!(
            public_from_files(&key_path, &pub_path).is_none(),
            "truncated secret half → no announcement"
        );
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

    /// A cargo test binary lives at `<target>/<profile>/deps/<name>-<hash>`
    /// and that hash is what makes a keychain ACL unable to stick. The
    /// predicate is pure over the path, so this is provable without
    /// spawning anything — and without raising a single modal.
    #[test]
    fn a_cargo_test_binary_is_recognised_by_its_deps_hash_path() {
        let yes = PathBuf::from("/w/target/debug/deps/nika_cli-defa282c811870d8");
        assert!(
            running_as_a_cargo_test_binary(Some(&yes)),
            "the deps/<name>-<hash> shape IS a test binary"
        );
    }

    /// The installed binary and `cargo run` must keep their custody: this
    /// fix may not quietly take the keychain away from a real run.
    #[test]
    fn an_installed_or_cargo_run_binary_keeps_its_custody() {
        let installed = PathBuf::from("/Users/x/.local/bin/nika");
        let cargo_run = PathBuf::from("/w/target/debug/nika-cli");
        let no_hash = PathBuf::from("/w/target/debug/deps/nika");
        let not_hex = PathBuf::from("/w/target/debug/deps/nika_cli-zzzzzzzzzzzzzzzz");
        assert!(
            !running_as_a_cargo_test_binary(Some(&installed)),
            "installed"
        );
        assert!(
            !running_as_a_cargo_test_binary(Some(&cargo_run)),
            "cargo run"
        );
        assert!(
            !running_as_a_cargo_test_binary(Some(&no_hash)),
            "no hash half"
        );
        assert!(!running_as_a_cargo_test_binary(Some(&not_hex)), "not hex");
        assert!(
            !running_as_a_cargo_test_binary(None),
            "unknown exe · keep custody"
        );
    }

    /// A pipe cannot complete a keychain ACL. Asking is the hang after
    /// the card (sampled `nika try 01-hello` 2026-08-22).
    #[test]
    fn a_pipe_does_not_consult_the_keychain() {
        assert!(
            !keychain_consults(true, false, false),
            "piped stderr · skip custody"
        );
        assert!(keychain_consults(true, false, true), "a TTY still consults");
        assert!(
            !keychain_consults(true, true, true),
            "a cargo test binary never consults, TTY or not"
        );
        assert!(
            !keychain_consults(false, false, true),
            "NIKA_KEYCHAIN=off still wins"
        );
    }

    /// The wiring, proven by the only witness that cannot be faked: THIS
    /// process. A hand-written path fixture proves the predicate; it does
    /// not prove the predicate is plugged in. Replacing the default with
    /// `true` passes every fixture test and still spams the operator.
    #[test]
    fn this_very_test_binary_is_recognised_and_cannot_open_the_custody() {
        let exe = std::env::current_exe().expect("current exe");
        assert!(
            running_as_a_cargo_test_binary(Some(&exe)),
            "the REAL running path must be recognised, not just fixtures: {exe:?}"
        );
        assert!(
            !keychain_enabled(),
            "a cargo test binary must never reach the OS custody · {exe:?}"
        );
    }

    /// The RATCHET. Guarding seven of eight sites is what a careful pass
    /// produces, and the eighth is the one that keeps prompting. This
    /// walks the crate's own source so a NINTH site cannot be added
    /// without its guard — the invariant stops depending on review.
    #[test]
    fn every_keyring_call_site_sits_behind_the_custody_flag() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut sites = 0usize;
        let mut naked: Vec<String> = Vec::new();
        let entries = std::fs::read_dir(&dir).expect("the crate's own src/");
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("readable source");
            let all: Vec<&str> = text.lines().collect();
            // Production sites only. A test module MENTIONS the call in a
            // string literal (this very sweep does), and counting a
            // mention as a site is how an instrument reports on itself.
            let end = all
                .iter()
                .position(|l| l.trim_start().starts_with("#[cfg(test)]"))
                .unwrap_or(all.len());
            let lines: Vec<&str> = all[..end].to_vec();
            for (i, line) in lines.iter().enumerate() {
                if !line.contains("keyring::Entry::new") {
                    continue;
                }
                sites += 1;
                let from = i.saturating_sub(10);
                if !lines[from..=i]
                    .iter()
                    .any(|l| l.contains("keychain_enabled"))
                {
                    naked.push(format!("{}:{}", path.display(), i + 1));
                }
            }
        }
        assert!(
            sites > 0,
            "the sweep found no call site — it looked at nothing"
        );
        assert!(
            naked.is_empty(),
            "{} of {sites} keyring call sites are not behind keychain_enabled(): {naked:?}",
            naked.len()
        );
    }
}
