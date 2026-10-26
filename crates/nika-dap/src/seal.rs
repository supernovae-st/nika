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
//! An explicit file pair first (`NIKA_RUN_KEY_FILE` · `NIKA_RUN_PUB_FILE`),
//! then the OS keychain (entry `nika/run-signing-key`, the minisign secret
//! box as text), then `~/.nika/keys/run-signing.key` (0600) as the
//! no-backend fallback (CI without a session keyring). The keychain is
//! opened only by a binary whose signature keeps a keychain grant (a
//! Developer ID release, never an ad-hoc build, which the keychain would
//! see as a new identity and ask for the password on every run), from a
//! terminal, and never by a resident (`nika serve` seals from files). A
//! secret key file other users can read is refused, as ssh refuses one.
//! A release and the builds kept from the keychain sign with one key only
//! when the 0600 files hold the keychain's: `nika key align`, from a signed
//! release, makes them one (the replaced file key retired). A release
//! records which key the keychain holds (`~/.nika/keys/keychain.json`, with
//! the item's modification date) so those builds can compare while the item
//! is unchanged. `nika key init|trust|rotate|align` manages the lifecycle;
//! an absent key keeps the journal unsigned and says so — sealing is
//! additive, never a gate. A verifier weighs the public halves in this same
//! order, the keychain aside ([`candidate_pubkeys`]).

use std::fmt::Write as _;
use std::io::{Cursor, Write as IoWrite};
use std::path::{Path, PathBuf};

use nika_event::settlement::{RunCause, RunSettlement};
use nika_event::{Event, EventKind};
use nika_types::id::EventId;
use nika_types::timestamp::Timestamp;

mod key_files;
mod public_box;
#[cfg(test)]
mod served_tests;
mod signing;

pub(crate) use key_files::keyring_entry;
pub(crate) use public_box::parse_many as parse_public_boxes;

/// The public half in its canonical form for `secret` — the pair honesty
/// the custody loaders apply BEFORE a seal mints, and the ONE projection a
/// seal's `key_id` names: a key handed to the seal from elsewhere (a door's
/// injected source) seals attributably only through it. `None` when the
/// box is not this secret's public half.
pub(crate) fn canonical_public_box(secret: &minisign::SecretKey, text: &str) -> Option<String> {
    let public = public_box::canonical(text)?;
    if !public_box::is_own(secret, &public) {
        return None;
    }
    Some(public)
}

/// The keychain entries the run-key lives under. The separate public half lets
/// trust and verification inspect enrollment without opening the secret; the
/// signing loader checks it against the already-open secret's derived public key.
const KEYRING_SERVICE: &str = "nika";
const KEYRING_USER: &str = "run-signing-key";
pub(crate) const KEYRING_USER_PUB: &str = "run-signing-key.pub";

/// The keychain's public run-key item, read at most once per process.
/// Every public-only reader (key trust, the doctor, the memory fold, a seal
/// grade, a workflow signature) reads this item alone, never the secret one.
/// `None` when this process does not open the keychain or the item is absent.
pub(crate) fn keychain_public_item() -> Option<String> {
    static PUBLIC: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    PUBLIC
        .get_or_init(|| keyring_entry(KEYRING_USER_PUB).and_then(|entry| entry.get_password().ok()))
        .clone()
}

/// The keychain's secret run-key item, read at most once per process
/// and only by the two readers that must open it: the signing loader and
/// `nika key align`. Its text then stays in this process's memory, as a 0600
/// file's text would.
fn keychain_secret_item() -> Option<String> {
    static SECRET: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SECRET
        .get_or_init(|| keyring_entry(KEYRING_USER).and_then(|entry| entry.get_password().ok()))
        .clone()
}

/// Whether `nika key init` would clobber a keychain item: either run-key item
/// is there, read from attributes alone (no data, no prompt). A process that
/// writes the keychain (a signed release) reads an unknown answer (a locked
/// keychain) as occupied: it never overwrites what it could not see. A test
/// binary, a resident and the keychain off never ask.
pub(crate) fn keychain_may_be_occupied() -> bool {
    let keychain = keychain_use();
    if !matches!(
        keychain,
        KeychainUse::Opened | KeychainUse::AdHoc | KeychainUse::NoTerminal
    ) {
        return false;
    }
    [KEYRING_USER, KEYRING_USER_PUB].into_iter().any(|user| {
        match signing::keychain_item(KEYRING_SERVICE, user) {
            Some(signing::KeychainItem::Present(_)) => true,
            Some(signing::KeychainItem::Absent) => false,
            None => keychain == KeychainUse::Opened,
        }
    })
}

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
pub(crate) fn keychain_enabled() -> bool {
    keychain_use() == KeychainUse::Opened
}

/// Whether this process opens the OS keychain for its run-key, and when it
/// does not, why: the one decision every custody reader and `nika doctor`
/// share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeychainUse {
    /// It opens it: the keychain is on, stderr is a terminal and this
    /// binary's signature keeps its keychain grant.
    Opened,
    /// `NIKA_KEYCHAIN=off`.
    Off,
    /// A cargo test binary: a new identity on every compile.
    TestBinary,
    /// A resident (`nika serve`): it seals from files and never stops to ask
    /// for a password.
    Resident,
    /// No terminal on stderr: a keychain prompt could never be answered.
    NoTerminal,
    /// This build's signature cannot keep a keychain grant (ad-hoc or
    /// unsigned): the keychain would ask for its password on every run.
    AdHoc,
}

/// Who this process is, for its keychain: a resident never opens it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Process {
    /// A process a person attends (`nika run` · the Session · `nika doctor`).
    Attended,
    /// A resident (`nika serve`), once [`resident_custody`] said so.
    Resident,
}

/// Set once by a resident at its start ([`resident_custody`]).
static RESIDENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// This process is a resident (`nika serve`): from now on it never opens the
/// keychain, whatever its terminal or its signature — a resident
/// cannot stop to ask for a password. Its run-key comes from the explicit
/// pair or the 0600 files, and every keychain reader in it says so.
pub fn resident_custody() {
    RESIDENT.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Whether this process opens the OS keychain, and why not (see
/// [`KeychainUse`]).
// Env reads are CONFIG paths (a flag, never a secret value · the scoped
// `env_flag` precedent).
#[allow(clippy::disallowed_methods)]
#[must_use]
pub fn keychain_use() -> KeychainUse {
    let process = if RESIDENT.load(std::sync::atomic::Ordering::SeqCst) {
        Process::Resident
    } else {
        Process::Attended
    };
    // seam-bypass-ok: build-host flag · no secret crosses here
    keychain_decision(
        keychain_flag(std::env::var("NIKA_KEYCHAIN").ok().as_deref()),
        running_as_a_cargo_test_binary(std::env::current_exe().ok().as_deref()),
        process,
        std::io::IsTerminal::is_terminal(&std::io::stderr()),
        signing::grant_can_stick,
    )
}

/// Whether the OS keychain may be opened for this process. Pure so the
/// pipe/ACL hang and the per-build password prompt are fixtures, not
/// session reproductions.
///
/// A captured stderr (pipe · CI · an agent `Popen`) cannot complete a
/// macOS keychain ACL prompt. Asking anyway is a hang after a green
/// card (`nika try 01-hello` piped · sampled 2026-08-22 on
/// `SecKeychainFindGenericPassword`). An ad-hoc build can answer it, but
/// its « Always Allow » never sticks: the password returns on
/// every run. A resident never stops to ask. File and env custody stay. The
/// signature is asked last, so only a process that would otherwise open the
/// keychain pays for it.
fn keychain_decision(
    flag: bool,
    test_bin: bool,
    process: Process,
    stderr_tty: bool,
    grant_sticks: impl FnOnce() -> bool,
) -> KeychainUse {
    if !flag {
        KeychainUse::Off
    } else if test_bin {
        KeychainUse::TestBinary
    } else if process == Process::Resident {
        KeychainUse::Resident
    } else if !stderr_tty {
        KeychainUse::NoTerminal
    } else if !grant_sticks() {
        KeychainUse::AdHoc
    } else {
        KeychainUse::Opened
    }
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
/// (explicit files, otherwise keychain then the 0600 file fallback). A broken
/// explicit pair refuses instead of selecting another identity. The returned
/// public box contains decoded public-key material, never custody metadata.
#[must_use]
pub fn load_signing_key() -> Option<(minisign::SecretKey, String)> {
    load_signing_key_from(true)
}

/// The signing half a resident seals with (`nika serve`): the explicit file
/// pair, otherwise the 0600 files — never the keychain, whatever its
/// terminal or its signature (a resident cannot stop to ask for a password).
/// The same 0600 rule holds for every file it reads.
#[must_use]
pub fn load_served_signing_key() -> Option<(minisign::SecretKey, String)> {
    load_signing_key_from(false)
}

/// The custody order, with the keychain in it only when `keychain` allows
/// it (and [`keychain_enabled`] still guards its every entry).
fn load_signing_key_from(keychain: bool) -> Option<(minisign::SecretKey, String)> {
    // An explicit key file wins (CI injects keys this way · hermetic tests
    // too — the OS keychain is never the only door).
    if let Some(files) = key_files::configured().ok()? {
        return load_from_files(&files.secret, &files.public);
    }
    // The keychain holds the minisign secret box as text (custody IS the
    // keychain's own encryption).
    if keychain
        && let Some(text) = keychain_secret_item()
        && let Ok(boxed) = minisign::SecretKeyBox::from_string(&text)
        && let Some(sk) = open_secret_box(boxed, &key_password())
        && let Some(pk_box) = keychain_public_item()
    {
        let pk_box = public_box::canonical(&pk_box)?;
        if !public_box::is_own(&sk, &pk_box) {
            return None;
        }
        return Some((sk, pk_box));
    }
    // Fallback: the 0600 files (unencrypted in v1 — the encrypted upgrade
    // lands with the password/keychain-hydration pass).
    let path = fallback_key_path()?;
    load_from_files(&path, &path.with_extension("pub"))
}

/// Load a (secret box, public box) pair from two files — `None` on any
/// miss (absent file · unparseable box · a secret file others can read).
fn load_from_files(key: &Path, pub_: &Path) -> Option<(minisign::SecretKey, String)> {
    let text = read_private(key)?;
    let boxed = minisign::SecretKeyBox::from_string(&text).ok()?;
    let sk = open_secret_box(boxed, &key_password())?;
    let pk_box = std::fs::read_to_string(pub_).ok()?;
    let pk_box = public_box::canonical(&pk_box)?;
    if !public_box::is_own(&sk, &pk_box) {
        return None;
    }
    Some((sk, pk_box))
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

/// The PUBLIC half of the run-key — the same precedence as
/// [`load_signing_key`] (env override, keychain, 0600 fallback), WITHOUT ever
/// decrypting. A file pair is probed parse-only (`secret_box_parses` —
/// envelope shape + the crate's own structural parse; no password, no
/// `into_secret_key`), so an orphaned file `.pub` is never announced. The
/// keychain tier reads its public item alone: proving that
/// pair would open the secret item, a prompt no public question may cost, so
/// a keychain `.pub` whose secret is gone is announced until a seal fails to
/// open it. `key trust`, the doctor, the memory fold and the rotation ledger
/// stay off the secret path.
/// Public boxes are decoded and reconstructed: untrusted comments and trailing
/// bytes cannot reach the trust output or retired ledger. Engine-generated
/// boxes keep their bytes and fingerprint; custom comments are excluded from
/// new identities. Historical custom-comment identities require their original
/// public enrollment record when verifying older seals.
#[must_use]
pub fn load_public_box() -> Option<String> {
    public_box_and_tier().map(|(pk_box, _)| pk_box)
}

/// The custody tier a public half came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier {
    Explicit,
    Keychain,
    Files,
}

/// [`load_public_box`] with the tier it read.
fn public_box_and_tier() -> Option<(String, Tier)> {
    // The explicit file override IS the custody when both vars are set
    // (init writes there, load reads there) — a broken pair under it
    // answers "no usable key", never a silent fall-through to another
    // tier.
    if let Some(files) = key_files::configured().ok()? {
        return public_from_files(&files.secret, &files.public).map(|pk| (pk, Tier::Explicit));
    }
    // The keychain tier reads its public item alone: proving
    // the pair would open the secret item, the very prompt a public question
    // must never cost. The signing loader proves the pair when it signs.
    if let Some(pk_box) = keychain_public_item() {
        return public_box::canonical(&pk_box).map(|pk| (pk, Tier::Keychain));
    }
    let path = fallback_key_path()?;
    public_from_files(&path, &path.with_extension("pub")).map(|pk| (pk, Tier::Files))
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
/// (absent or unparseable secret half · absent or invalid pub), mirroring
/// [`load_from_files`] minus the decrypt.
fn public_from_files(key: &Path, pub_: &Path) -> Option<String> {
    let text = read_private(key)?;
    if !secret_box_parses(&text) {
        return None;
    }
    let pk_box = std::fs::read_to_string(pub_).ok()?;
    public_box::canonical(&pk_box)
}

/// A secret key file's text, read only when no one but its owner can read
/// it (0600 or tighter), as ssh reads a private key: a secret other users
/// can read is no custody, so the run stays unsigned and `nika doctor` says
/// why. The mode is judged on the very handle the text is read from.
fn read_private(path: &Path) -> Option<String> {
    use std::io::Read as _;
    // seam-bypass-ok: run-key custody read, never workflow I/O
    let mut file = std::fs::File::open(path).ok()?;
    if exposed_mode(&file.metadata().ok()?).is_some() {
        return None;
    }
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

/// The permission bits of a key file others can read or write, when they
/// can (unix group or other bits): `None` for a private one.
#[cfg(unix)]
fn exposed_mode(meta: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = meta.permissions().mode() & 0o777;
    // Unix group/other bits; the serve token's own mask.
    #[allow(clippy::verbose_bit_mask)]
    let exposed = mode & 0o077 != 0;
    exposed.then_some(mode)
}

/// Off unix no mode bits say who can read a file: none is judged exposed.
#[cfg(not(unix))]
fn exposed_mode(_meta: &std::fs::Metadata) -> Option<u32> {
    None
}

/// A secret key file, as custody finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyFile {
    /// Nothing at its path.
    Absent,
    /// Present and private (0600 or tighter): custody reads it.
    Private,
    /// Present and readable by others (its permission bits): refused.
    Exposed(u32),
    /// Present but its permissions could not be read: refused.
    Unreadable,
}

/// Where this process takes its run-key from, and which key that is. Decided
/// from configuration, file metadata and public halves: the keychain's data
/// is read only by a process that opens it anyway, and a dev build learns
/// whether the keychain holds a key from the item's attributes alone (a
/// question about custody never costs a password).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Custody {
    /// The explicit pair, when `NIKA_RUN_KEY_FILE` · `NIKA_RUN_PUB_FILE`
    /// name one: its secret file and state, or why the pair is refused.
    pub explicit: Option<Result<(PathBuf, KeyFile), String>>,
    /// Whether this process opens the keychain, and why not.
    pub keychain: KeychainUse,
    /// The 0600 fallback's secret file (`~/.nika/keys/run-signing.key`) and
    /// its state, when a home names one.
    pub fallback: Option<(PathBuf, KeyFile)>,
    /// The fingerprint of the key this process would sign with, from its
    /// public half, in the tier order sealing walks; `None` when no tier
    /// holds a usable pair (runs stay unsigned).
    pub signer: Option<String>,
    /// Whether the keychain holds a run-key, from the item's attributes only:
    /// never asked by a test binary, a resident or with the keychain off, nor
    /// beside an explicit pair (`None`: unknown).
    pub keychain_key: Option<bool>,
    /// The keychain key's fingerprint, when this process read it (a signed
    /// release): then it is also the signer.
    pub keychain_fingerprint: Option<String>,
    /// The keychain key's fingerprint as a signed release last recorded it
    /// (`~/.nika/keys/keychain.json`), only while that record is fresh: the
    /// item's modification date is still the one recorded. What a dev build
    /// knows of the keychain key without reading it.
    pub recorded: Option<String>,
    /// The fingerprint of the pair in the 0600 files, what dev builds and
    /// `nika serve` sign with, when a usable pair is there.
    pub file_fingerprint: Option<String>,
}

impl Custody {
    /// A custody from its three parts, naming no key yet: what [`custody`]
    /// observes, or the fixture of a host that says it.
    #[must_use]
    pub fn new(
        explicit: Option<Result<(PathBuf, KeyFile), String>>,
        keychain: KeychainUse,
        fallback: Option<(PathBuf, KeyFile)>,
    ) -> Self {
        Self {
            explicit,
            keychain,
            fallback,
            signer: None,
            keychain_key: None,
            keychain_fingerprint: None,
            recorded: None,
            file_fingerprint: None,
        }
    }

    /// The same custody naming its keys: the signer's fingerprint, whether
    /// the keychain holds a key, and the keychain and file keys'
    /// fingerprints when known.
    #[must_use]
    pub fn with_keys(
        mut self,
        signer: Option<String>,
        keychain_key: Option<bool>,
        keychain_fingerprint: Option<String>,
        file_fingerprint: Option<String>,
    ) -> Self {
        self.signer = signer;
        self.keychain_key = keychain_key;
        self.keychain_fingerprint = keychain_fingerprint;
        self.file_fingerprint = file_fingerprint;
        self
    }

    /// The same custody with the keychain key a fresh record names.
    #[must_use]
    pub fn with_record(mut self, recorded: Option<String>) -> Self {
        self.recorded = recorded;
        self
    }
}

/// This process's run-key custody (see [`Custody`]).
#[must_use]
pub fn custody() -> Custody {
    let explicit = match key_files::configured() {
        Ok(None) => None,
        Ok(Some(files)) => Some(Ok((files.secret.clone(), key_file(&files.secret)))),
        Err(why) => Some(Err(why)),
    };
    let fallback = fallback_key_path().map(|path| {
        let state = key_file(&path);
        (path, state)
    });
    let keychain = keychain_use();
    // One read of the tiers: the keychain's data only where this process
    // opens it anyway (a signed release reads it to sign as well).
    let signer = public_box_and_tier();
    let keychain_fingerprint = signer
        .as_ref()
        .filter(|(_, tier)| *tier == Tier::Keychain)
        .map(|(pk_box, _)| fingerprint(pk_box));
    // An explicit pair is the whole custody: the keychain is not asked.
    let item = explicit
        .is_none()
        .then(|| keychain_run_key(keychain))
        .flatten();
    let keychain_key = item
        .as_ref()
        .map(|item| *item != signing::KeychainItem::Absent);
    let recorded = item.as_ref().and_then(recorded_keychain_key);
    let file_fingerprint = fallback_key_path()
        .and_then(|path| public_from_files(&path, &path.with_extension("pub")))
        .map(|pk_box| fingerprint(&pk_box));
    Custody::new(explicit, keychain, fallback)
        .with_keys(
            signer.map(|(pk_box, _)| fingerprint(&pk_box)),
            keychain_key,
            keychain_fingerprint,
            file_fingerprint,
        )
        .with_record(recorded)
}

/// The keychain's run-key item, from its attributes alone (never its data,
/// never a prompt): whether it is there and when it last changed. A test
/// binary, a resident and the keychain off never touch the keychain at all
/// and answer `None`.
fn keychain_run_key(keychain: KeychainUse) -> Option<signing::KeychainItem> {
    matches!(
        keychain,
        KeychainUse::Opened | KeychainUse::AdHoc | KeychainUse::NoTerminal
    )
    .then(|| signing::keychain_item(KEYRING_SERVICE, KEYRING_USER))
    .flatten()
}

/// The record a signed release keeps for the builds that never open the
/// keychain: which key the keychain holds, and when its item last changed.
const KEYCHAIN_RECORD: &str = "keychain.json";

/// The record's text: the keychain key's fingerprint and the item's
/// modification date, as the keychain describes it.
fn keychain_record(fingerprint: &str, modified: &str) -> String {
    serde_json::json!({ "fingerprint": fingerprint, "modified": modified }).to_string()
}

/// The fingerprint a record names, only when it is fresh: written for the
/// item as it is now (the same modification date). A stale, absent or
/// malformed record names nothing, so it never claims one identity.
fn fresh_record(text: &str, item: &signing::KeychainItem) -> Option<String> {
    let signing::KeychainItem::Present(Some(modified)) = item else {
        return None;
    };
    let record: serde_json::Value = serde_json::from_str(text).ok()?;
    if record.get("modified")?.as_str()? != modified.as_str() {
        return None;
    }
    let fingerprint = record.get("fingerprint")?.as_str()?;
    let hex = fingerprint
        .bytes()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    (fingerprint.len() == 16 && hex).then(|| fingerprint.to_owned())
}

/// The keychain key's fingerprint from this machine's record, when fresh.
fn recorded_keychain_key(item: &signing::KeychainItem) -> Option<String> {
    let path = keys_path(KEYCHAIN_RECORD)?;
    let text = crate::anchor::read_owned(&path, KEY_FILE_CAP)
        .ok()
        .flatten()?;
    fresh_record(&text, item)
}

/// Record, for the builds that never open the keychain, which key it now
/// holds: written by a process that just wrote or read the keychain pair (a
/// signed release, at init, rotate and align). Best effort: without a fresh
/// record those builds say they do not know.
fn record_keychain_key(pk_box: &str) {
    let Some(signing::KeychainItem::Present(Some(modified))) =
        signing::keychain_item(KEYRING_SERVICE, KEYRING_USER)
    else {
        return;
    };
    let Some(path) = keys_path(KEYCHAIN_RECORD) else {
        return;
    };
    if let Some(parent) = path.parent() {
        // seam-bypass-ok: run-key custody record, never workflow I/O
        let _ = std::fs::create_dir_all(parent);
    }
    let record = keychain_record(&fingerprint(pk_box), &modified);
    let _ = key_files::write(&path, &record, true);
}

/// Whether `nika key init` would mint a second identity: this process does
/// not open the keychain, so it would write the 0600 files beside the
/// keychain key a signed release keeps signing with.
fn init_splits_identity(keychain: KeychainUse, keychain_key: Option<bool>) -> bool {
    matches!(keychain, KeychainUse::AdHoc | KeychainUse::NoTerminal) && keychain_key == Some(true)
}

/// A secret key file's state, from its metadata only (never its bytes).
fn key_file(path: &Path) -> KeyFile {
    // seam-bypass-ok: run-key custody metadata, never workflow I/O
    match std::fs::metadata(path) {
        Ok(meta) => exposed_mode(&meta).map_or(KeyFile::Private, KeyFile::Exposed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => KeyFile::Absent,
        Err(_) => KeyFile::Unreadable,
    }
}

/// `nika key init` — generate + store a run-key (idempotent: refuses to
/// clobber an existing one without `--force`).
///
/// # Errors
///
/// A refusal string for an invalid file override, existing custody material
/// without `--force` (even an orphan or corrupt pair, or a keychain key this
/// process does not open), or when generation/storage fails (keychain
/// unavailable AND the 0600 fallback unwritable).
pub fn key_init(force: bool) -> Result<String, String> {
    let explicit = key_files::configured()?;
    let keychain = keychain_use();
    if !force
        && explicit.is_none()
        && init_splits_identity(
            keychain,
            keychain_run_key(keychain).map(|item| item != signing::KeychainItem::Absent),
        )
    {
        return Err(
            "the keychain holds a run-key, which this process does not open (a signed release on a terminal signs with it): `nika key align` from there gives every build that key; `--force` mints a second key for the builds that do not open the keychain"
                .to_owned(),
        );
    }
    if !force && key_files::occupied(explicit.as_ref()) {
        return Err(
            "run-signing key material already exists — inspect or restore the pair; `--force` explicitly replaces it"
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
    store_key_boxes(&sk_box, &pk_box, explicit.as_ref(), force)?;
    Ok(fingerprint(&pk_box))
}

/// `nika key trust` — the public key + fingerprint to enroll elsewhere.
#[must_use]
pub fn key_trust() -> Option<(String, String)> {
    let pk_box = load_public_box()?;
    Some((pk_box.clone(), fingerprint(&pk_box)))
}

/// `nika key rotate` — retire the current pubkey to the ledger, then
/// generate a fresh key. Engine-generated old journals stay verifiable
/// against the ledger. Imported custom-comment fingerprints require the
/// original enrollment record; retirement cannot copy untrusted metadata.
///
/// # Errors
///
/// A refusal string when no key exists to rotate, or when the ledger or
/// the new key cannot be written.
pub fn key_rotate() -> Result<String, String> {
    let old_pk = load_public_box()
        .ok_or_else(|| "no run-signing key to rotate — `nika key init` first".to_owned())?;
    retire(&old_pk)?;
    key_init(true)
}

/// Append `pk` to the retired-pubkeys ledger, so every journal it sealed
/// stays verifiable after another key takes its place.
fn retire(pk: &str) -> Result<(), String> {
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
        writeln!(file, "{pk}").map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}

/// What `nika key align` did.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Alignment {
    /// The fingerprint every build on this machine now signs with.
    pub fingerprint: String,
    /// The fingerprint of the file key it replaced, now in the retired
    /// ledger, when the files held another key.
    pub retired: Option<String>,
    /// The files already held this very key: nothing was written.
    pub already: bool,
}

/// `nika key align` — one gesture so every build on this machine signs with
/// one key. The keychain pair, the identity a signed release seals
/// with, is written to the 0600 files every other build reads (a dev build, a
/// resident); the file key it replaces is retired to the ledger first, so the
/// journals that key sealed stay verifiable. Only a process that opens the
/// keychain does it (a signed release, on a terminal), and it never mints a
/// key. It records which key the keychain holds (`keychain.json`), so a dev
/// build can say it signs with the same one while the item is unchanged.
/// Afterwards any process running as this user can read the key from the
/// 0600 files, as with every file custody.
///
/// # Errors
///
/// A refusal string when an explicit pair is this process's custody, when
/// this process does not open the keychain, when the keychain holds no usable
/// pair, or when the ledger or the files cannot be written.
pub fn key_align() -> Result<Alignment, String> {
    if key_files::configured()?.is_some() {
        return Err(
            "NIKA_RUN_KEY_FILE names this process's custody: align that pair by hand".to_owned(),
        );
    }
    if keychain_use() != KeychainUse::Opened {
        return Err(
            "this process does not open the keychain: run `nika key align` from a signed release, on a terminal"
                .to_owned(),
        );
    }
    let (Some(sk_box), Some(pk_box)) = (keychain_secret_item(), keychain_public_item()) else {
        return Err("the keychain holds no run-key pair to align with".to_owned());
    };
    // Pair honesty: the secret opens, and the public half is its own.
    let boxed = minisign::SecretKeyBox::from_string(&sk_box)
        .map_err(|_| "the keychain's secret half is not a minisign box".to_owned())?;
    let sk = open_secret_box(boxed, &key_password())
        .ok_or("the keychain's secret half does not open with the custody password")?;
    let not_own = "the keychain's public half is not its secret's own";
    let pk_box = public_box::canonical(&pk_box).ok_or(not_own)?;
    if !public_box::is_own(&sk, &pk_box) {
        return Err(not_own.to_owned());
    }
    let aligned = fingerprint(&pk_box);
    let path = fallback_key_path().ok_or("no HOME for the key files")?;
    let files = key_files::KeyFiles {
        public: path.with_extension("pub"),
        secret: path,
    };
    if load_from_files(&files.secret, &files.public)
        .is_some_and(|(_, pk)| fingerprint(&pk) == aligned)
    {
        record_keychain_key(&pk_box);
        return Ok(Alignment {
            fingerprint: aligned,
            retired: None,
            already: true,
        });
    }
    // The file key it replaces, retired first: the journals it sealed stay
    // verifiable.
    let replaced = std::fs::read_to_string(&files.public)
        .ok()
        .and_then(|pk| public_box::canonical(&pk))
        .filter(|pk| fingerprint(pk) != aligned);
    if let Some(pk) = &replaced {
        retire(pk)?;
    }
    files.store(&sk_box, &pk_box, true)?;
    record_keychain_key(&pk_box);
    Ok(Alignment {
        fingerprint: aligned,
        retired: replaced.map(|pk| fingerprint(&pk)),
        already: false,
    })
}

/// Store both boxes — the env override first (init writes where load
/// reads), then the keychain, then the 0600 files as fallback.
fn store_key_boxes(
    sk_box: &str,
    pk_box: &str,
    explicit: Option<&key_files::KeyFiles>,
    force: bool,
) -> Result<(), String> {
    if let Some(files) = explicit {
        return files.store(sk_box, pk_box, force);
    }
    if let Some(entry) = keyring_entry(KEYRING_USER)
        && entry.set_password(sk_box).is_ok()
        && let Some(pub_entry) = keyring_entry(KEYRING_USER_PUB)
        && pub_entry.set_password(pk_box).is_ok()
    {
        record_keychain_key(pk_box);
        return Ok(());
    }
    let path = fallback_key_path().ok_or("no HOME for the key fallback".to_owned())?;
    key_files::KeyFiles {
        public: path.with_extension("pub"),
        secret: path,
    }
    .store(sk_box, pk_box, force)
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
    /// Local SDK receipt claims known before the seal mints. The signed
    /// `covers.sdk_receipt` binding lets a verifier reject a genuine trace
    /// paired with fabricated execution, trace, or snapshot identity.
    pub sdk_receipt: Option<serde_json::Value>,
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

    /// The teardown a service-bound run attests (spec 17 §the end of the
    /// run · descended from Serve's resident door, C6): the CLI's attended
    /// facts as far as a service boundary carries them. The receipt inputs
    /// (proves · the certificate · the `outcome` word the caller's door
    /// maps), the budgets ρ from the settlement's own spend (ADR-128 ·
    /// `spent_usd` only when metered), the caller's `sdk_receipt` binding,
    /// and the signed-memory fold under `memory_root`. The effects ε need the
    /// per-task records a service boundary redacts: their key stays OUT
    /// (absent is honest, never a fabricated zero). The quarantine fold is
    /// the host's to set from the driver's in-process run outcome
    /// ([`crate::quarantine::attend_at`] under the display root); this
    /// constructor leaves it out.
    #[must_use]
    pub fn served(
        workflow: &nika_schema::raw::RawWorkflow,
        report: &nika_check::CheckReport,
        outcome: &str,
        settlement: Option<&RunSettlement>,
        sdk_receipt: serde_json::Value,
        memory_root: &Path,
    ) -> Self {
        let mut teardown = Self::new();
        teardown.proves = nika_runtime::proof::ir::semantic_ir_hash(workflow)
            .map(|hash| hash.as_hex().to_owned());
        teardown.certificate = serde_json::to_value(&report.certificate).ok();
        teardown.outcome = Some(outcome.to_owned());
        teardown.budgets = settlement.map(|settlement| served_budgets(settlement, report));
        teardown.sdk_receipt = Some(sdk_receipt);
        let memory = crate::memory::attend(Some(memory_root));
        teardown.memory = memory.fold;
        teardown.memory_rejected = memory.rejected;
        teardown
    }
}

/// The workflow hash a run's seal is taken under (the CLI's `seal_hash`): the
/// per-task Merkle root of the admitted workflow, `None` when it has none.
#[must_use]
pub fn workflow_hash(workflow: &nika_schema::raw::RawWorkflow) -> Option<String> {
    nika_runtime::proof::ir::merkle_by_task(workflow)
        .map(|proof| proof.workflow.as_hex().to_owned())
}

/// The budgets ρ a settlement states: `spent_usd` only when something was
/// metered, the priced and unpriced call counts, whether the budget stopped
/// the run, and the certificate's ceiling when it names one.
fn served_budgets(
    settlement: &RunSettlement,
    report: &nika_check::CheckReport,
) -> serde_json::Value {
    let mut budgets = serde_json::Map::new();
    if let Some(spent) = settlement.spend.total_cost_usd {
        budgets.insert("spent_usd".to_owned(), serde_json::json!(spent));
    }
    budgets.insert(
        "priced_calls".to_owned(),
        settlement.spend.priced_calls.into(),
    );
    budgets.insert(
        "unpriced_calls".to_owned(),
        settlement.spend.unpriced_calls.into(),
    );
    let exceeded = settlement.cause == RunCause::Budget;
    budgets.insert("budget_exceeded".to_owned(), exceeded.into());
    if let Some(ceiling) = &report.certificate.usd_micros
        && let Ok(value) = serde_json::to_value(ceiling)
    {
        budgets.insert("ceiling".to_owned(), value);
    }
    serde_json::Value::Object(budgets)
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
    if let Some(binding) = &teardown.sdk_receipt {
        covers["sdk_receipt"] = binding.clone();
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
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
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
/// order: an explicit key file first (the `--key` voice), then the keys
/// this process signs with, in the order the signer takes them (the public
/// half of the explicit `NIKA_RUN_KEY_FILE` · `NIKA_RUN_PUB_FILE` pair,
/// then the custody default), then every line of the retired ledger
/// (rotation keeps old journals verifiable). The keychain is not weighed.
/// Each entry is `(public box, source label)` — the source rides into the
/// tier's report line; the pair's label is its variable's name, so a door
/// serving the report never carries the file's path.
///
/// # Errors
///
/// A reason string when the explicit key file cannot be read, a custody file
/// that exists is refused (never taken for an absent key) or more than
/// `MAX_CANDIDATES` keys are found (never cut): the invocation's own
/// failure, never a forgery signal.
pub fn candidate_pubkeys(key_file: Option<&Path>) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    if let Some(path) = key_file {
        let read = crate::anchor::read_owned(path, KEY_FILE_CAP).map_err(|e| e.to_string());
        let text = (read.and_then(|t| t.ok_or_else(|| "no such file".to_owned())))
            .map_err(|e| format!("cannot read --key {}: {e}", path.display()))?;
        extend_candidate_pubkeys(&mut out, &text, &path.display().to_string());
    }
    if let Some(path) = key_files::configured_public() {
        let text = crate::anchor::read_owned(&path, KEY_FILE_CAP)
            .map_err(|e| format!("cannot read {EXPLICIT_PUBLIC}: {e}"))?;
        if let Some(text) = text {
            extend_candidate_pubkeys(&mut out, &text, EXPLICIT_PUBLIC);
        }
    }
    for name in ["run-signing.pub", "retired.pub"] {
        let Some(path) = keys_path(name) else {
            continue;
        };
        let text = crate::anchor::read_owned(&path, KEY_FILE_CAP)
            .map_err(|e| format!("cannot read ~/.nika/keys/{name}: {e}"))?;
        if let Some(text) = text {
            extend_candidate_pubkeys(&mut out, &text, &format!("~/.nika/keys/{name}"));
        }
    }
    if out.len() > MAX_CANDIDATES {
        return Err(format!(
            "more than {MAX_CANDIDATES} candidate keys: none was weighed"
        ));
    }
    Ok(out)
}

/// The source label of the explicit pair's public half: the variable naming
/// it, never its path.
const EXPLICIT_PUBLIC: &str = "NIKA_RUN_PUB_FILE";
/// A custody file's byte cap (a public box is ~150 bytes; more is refused).
const KEY_FILE_CAP: u64 = 64 * 1024;
/// The most candidate keys one verify weighs (more is refused, never cut).
const MAX_CANDIDATES: usize = 256;

/// Read one or more minisign public-key boxes from a custody file. A public
/// box is two lines (comment + payload); splitting the file line-by-line turns
/// both halves into invalid candidates and makes the default key appear
/// absent. The retired ledger is the same shape repeated with blank separators.
fn extend_candidate_pubkeys(out: &mut Vec<(String, String)>, text: &str, source: &str) {
    for candidate in parse_public_boxes(text) {
        let candidate_fp = fingerprint(&candidate);
        if !out
            .iter()
            .any(|(known, _)| fingerprint(known) == candidate_fp)
        {
            out.push((candidate, source.to_owned()));
        }
    }
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
#[cfg(unix)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod captured_tests;

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
    fn custody_candidates_keep_whole_boxes_and_dedupe_current_from_retired() {
        let (current, _) = keypair();
        let (retired, _) = keypair();
        let mut candidates = Vec::new();

        extend_candidate_pubkeys(&mut candidates, &current, "run-signing.pub");
        extend_candidate_pubkeys(
            &mut candidates,
            &format!("garbage is not a key\n\n{current}\n\n{retired}\n"),
            "retired.pub",
        );

        assert_eq!(candidates.len(), 2, "one current and one retired key");
        assert_eq!(candidates[0].0, current.trim());
        assert_eq!(candidates[0].1, "run-signing.pub");
        assert_eq!(candidates[1].0, retired.trim());
        assert_eq!(candidates[1].1, "retired.pub");
        assert!(
            candidates
                .iter()
                .all(|(key, _)| minisign::PublicKeyBox::from_string(key).is_ok()),
            "every custody candidate remains a complete minisign box"
        );
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
        teardown.sdk_receipt = Some(serde_json::json!({
            "receipt_format": 1,
            "execution_id": "exe-7",
            "trace_id": "trace-7",
            "snapshot_digest": "snapshot-7"
        }));
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
        assert_eq!(covers["sdk_receipt"], teardown.sdk_receipt.unwrap());

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
        private(&key_path);
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
        // Each secret half is private, so these refusals judge its parse, never its mode.
        std::fs::write(&key_path, "not a minisign box").expect("write junk");
        private(&key_path);
        assert!(
            public_from_files(&key_path, &pub_path).is_none(),
            "unparseable secret half → no announcement"
        );
        // Shaped like a box (comment + valid base64) but structurally
        // truncated — the crate-parse arm of the probe rejects it too.
        std::fs::write(&key_path, "untrusted comment: stub\nAAAA\n").expect("write stub");
        private(&key_path);
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
        private(&key_path);
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
        let signed = || true;
        let attended = Process::Attended;
        assert_eq!(
            keychain_decision(true, false, attended, false, signed),
            KeychainUse::NoTerminal,
            "piped stderr · skip custody"
        );
        assert_eq!(
            keychain_decision(true, false, attended, true, signed),
            KeychainUse::Opened,
            "a signed binary on a TTY still consults"
        );
        assert_eq!(
            keychain_decision(true, true, attended, true, signed),
            KeychainUse::TestBinary,
            "a cargo test binary never consults, TTY or not"
        );
        assert_eq!(
            keychain_decision(false, false, attended, true, signed),
            KeychainUse::Off,
            "NIKA_KEYCHAIN=off still wins"
        );
    }

    /// A resident never opens the keychain, even a signed one on
    /// a terminal: it cannot stop to ask for a password.
    #[test]
    fn a_resident_never_opens_the_keychain() {
        assert_eq!(
            keychain_decision(true, false, Process::Resident, true, || true),
            KeychainUse::Resident
        );
    }

    /// An ad-hoc build on a terminal never opens the keychain. Its
    /// « Always Allow » is bound to an identity the next build no longer has,
    /// so opening it was a password prompt on every run (red on 292c7bee0,
    /// whose decision was `flag && !test_bin && stderr_tty`). A binary whose
    /// signature keeps its grant (a Developer ID release) keeps the keychain;
    /// `NIKA_KEYCHAIN=off` still wins over both.
    #[test]
    fn an_ad_hoc_build_on_a_terminal_never_opens_the_keychain() {
        let attended = Process::Attended;
        assert_eq!(
            keychain_decision(true, false, attended, true, || false),
            KeychainUse::AdHoc
        );
        assert_eq!(
            keychain_decision(true, false, attended, true, || true),
            KeychainUse::Opened,
            "a stably signed binary keeps its keychain custody"
        );
        assert_eq!(
            keychain_decision(false, false, attended, true, || true),
            KeychainUse::Off
        );
        // The signature is asked last: only a process that would otherwise
        // open the keychain pays for the code-signing check.
        let asked = std::cell::Cell::new(false);
        let probe = || {
            asked.set(true);
            true
        };
        assert_eq!(
            keychain_decision(true, false, attended, false, probe),
            KeychainUse::NoTerminal
        );
        assert!(!asked.get(), "no terminal: the signature is never asked");
    }

    /// A test binary, a resident and the keychain off never touch
    /// the keychain, not even to learn from its attributes whether it holds a
    /// key; and since this test binary is one, the gesture that copies the
    /// keychain key to the files refuses here before reading anything.
    #[test]
    fn a_process_kept_from_the_keychain_never_asks_whether_it_holds_a_key() {
        for kept in [
            KeychainUse::TestBinary,
            KeychainUse::Resident,
            KeychainUse::Off,
        ] {
            assert_eq!(keychain_run_key(kept), None, "{kept:?}");
        }
        let refused = key_align().expect_err("a test binary never aligns");
        assert!(
            refused.starts_with("this process does not open the keychain")
                || refused.contains("NIKA_RUN_KEY_FILE"),
            "{refused}"
        );
    }

    /// `nika key init` in a process that does not open the keychain,
    /// while the keychain holds a key, would write a second identity to the
    /// 0600 files beside the one a signed release keeps signing with. It
    /// refuses unforced; a release, which opens the keychain, finds its key
    /// itself.
    #[test]
    fn init_never_mints_a_second_identity_beside_the_keychain_key() {
        for closed in [KeychainUse::AdHoc, KeychainUse::NoTerminal] {
            assert!(init_splits_identity(closed, Some(true)), "{closed:?}");
            assert!(!init_splits_identity(closed, Some(false)), "{closed:?}");
            assert!(!init_splits_identity(closed, None), "{closed:?}");
        }
        for other in [
            KeychainUse::Opened,
            KeychainUse::TestBinary,
            KeychainUse::Resident,
            KeychainUse::Off,
        ] {
            assert!(!init_splits_identity(other, Some(true)), "{other:?}");
        }
    }

    /// Every read of a keychain item's data is one of two
    /// process-wide reads, so no item is read twice in one process, and only
    /// the signing loader and `nika key align` read the secret item. A public
    /// question (key trust, the doctor, the memory fold, a seal grade, a
    /// workflow signature) reads the public item alone. A test process reads
    /// neither.
    #[test]
    fn the_secret_item_is_read_by_the_signer_and_align_alone_once_per_process() {
        let reads = ["get_pass", "word("].concat();
        let secret = ["keychain_secret", "_item()"].concat();
        let mut pending = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let (mut read_sites, mut custody_sites) = (Vec::new(), Vec::new());
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).expect("crate source directory") {
                let path = entry.expect("source entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let text = std::fs::read_to_string(&path).expect("source file");
                    for line in text.lines() {
                        if line.contains(&reads) {
                            read_sites.push((path.clone(), line.trim().to_owned()));
                        }
                        if line.contains(&secret) {
                            custody_sites.push(line.trim().to_owned());
                        }
                    }
                }
            }
        }
        assert_eq!(read_sites.len(), 2, "{read_sites:#?}");
        assert!(
            read_sites
                .iter()
                .all(|(path, line)| path.ends_with("src/seal.rs") && line.contains("get_or_init")),
            "every item read is a once-per-process read: {read_sites:#?}"
        );
        // Its definition, the signing loader and `key_align`: nothing else.
        assert_eq!(custody_sites.len(), 3, "{custody_sites:#?}");
        assert!(custody_sites.iter().any(|line| line.starts_with("fn ")));
        assert!(
            custody_sites
                .iter()
                .any(|line| line.contains("let Some(text)"))
        );
        assert!(
            custody_sites
                .iter()
                .any(|line| line.contains("keychain_public_item()"))
        );
        assert_eq!(keychain_public_item(), None, "a test process reads no item");
    }

    /// A dev build learns the keychain key from the record a signed
    /// release wrote, only while the item is still the one recorded (its
    /// modification date unchanged). A stale, absent or malformed record names
    /// no key, so it never claims one identity.
    #[test]
    fn a_keychain_record_names_the_key_only_while_the_item_is_unchanged() {
        use signing::KeychainItem;
        let at = "2026-08-20 10:11:12 +0000";
        let record = keychain_record("0f1e2d3c4b5a6978", at);
        let unchanged = KeychainItem::Present(Some(at.to_owned()));
        assert_eq!(
            fresh_record(&record, &unchanged).as_deref(),
            Some("0f1e2d3c4b5a6978")
        );
        let changed = KeychainItem::Present(Some("2026-10-12 09:00:00 +0000".to_owned()));
        assert_eq!(fresh_record(&record, &changed), None, "a later write");
        assert_eq!(fresh_record(&record, &KeychainItem::Absent), None);
        assert_eq!(fresh_record(&record, &KeychainItem::Present(None)), None);
        assert_eq!(fresh_record("not json", &unchanged), None);
        let forged = keychain_record("0F1E2D3C4B5A6978 ", at);
        assert_eq!(fresh_record(&forged, &unchanged), None, "not a fingerprint");
    }

    /// Make a fixture's secret key file private (0600), as `nika key init` writes one: a file
    /// other users can read is refused, which a test of its own judges.
    fn private(path: &std::path::Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .expect("fixture key file made private");
        }
        #[cfg(not(unix))]
        let _ = path;
    }

    /// A secret key file other users can read is refused, as ssh
    /// refuses a private key: custody never seals with it (the run stays
    /// unsigned), never announces it as this machine's key, and `custody`
    /// names it exposed with its bits. Made private again, it is the key.
    #[cfg(unix)]
    #[test]
    fn a_key_file_others_can_read_is_refused_like_ssh_refuses_one() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("fixture directory");
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("fixture keypair");
        let key = dir.path().join("run-signing.key");
        let public = dir.path().join("run-signing.pub");
        std::fs::write(&key, pair.sk.to_box(None).expect("secret box").to_string())
            .expect("fixture secret");
        std::fs::write(&public, pair.pk.to_box().expect("public box").to_string())
            .expect("fixture public");
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644))
            .expect("loose fixture");
        assert_eq!(key_file(&key), KeyFile::Exposed(0o644));
        assert!(
            load_from_files(&key, &public).is_none(),
            "never sealed with"
        );
        assert!(
            public_from_files(&key, &public).is_none(),
            "never announced"
        );
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600))
            .expect("private fixture");
        assert_eq!(key_file(&key), KeyFile::Private);
        assert!(load_from_files(&key, &public).is_some(), "0600 is the key");
        assert_eq!(key_file(&dir.path().join("absent.key")), KeyFile::Absent);
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
}
