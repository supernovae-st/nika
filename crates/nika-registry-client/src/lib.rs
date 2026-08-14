// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-registry-client` — resolve · verify · cache for
//! `registry:owner/name[@version]` refs (issue #452 · the consumption
//! half of the ADR-106 registry-client lane). L2 service over the ONE
//! injected effect (`HttpGetDyn`): the L4 composer (nika-cli
//! `registry.rs`) constructs the production http client + blocking
//! executor around [`RegistryClient`]. Size-cap descent from nika-cli
//! (D-2026-07-09-N1 · same architectural unit, two members).
//!
//! A registry ref never executes anything at pull time: it RESOLVES to a
//! verified local file under `~/.nika/registry/`, and `nika check` /
//! `nika run` then proceed exactly as if given that path — the audit-
//! before-run pipeline is untouched (`load_checked` reads the cached
//! file like any other). The trust chain, per the registry-v0.1 contract
//! (nika-spec `registry/registry-v0.1.md`):
//!
//! 1. **Resolve** — fetch the public index (`index.json` · contract §4) ·
//!    match `owner/name[@version]` · newest `SemVer` wins a bare ref. A
//!    name that resolves nowhere fails loud (`NIKA-REG-001` — the
//!    slopsquatting guard: an LLM-suggested name must exist).
//! 2. **Refuse on advisory** — a withdrawn version refuses BEFORE any
//!    bytes move (`NIKA-REG-002` · contract §3 MUST-refuse).
//! 3. **Re-verify against the ENTRY** — the index is a convenience
//!    projection; the digest of record comes from the entry TOML itself
//!    (contract §4: "never against the index alone"). Index/entry
//!    disagreement refuses (`NIKA-REG-005`).
//! 4. **Fetch + hash** — raw https fetch of `source.repo@rev:path`
//!    (1 MiB cap) · `sha256(bytes)` MUST equal the pinned digest, else
//!    hard refuse and NOTHING is written (`NIKA-REG-003`).
//! 5. **Cache** — the verified bytes land under ONE canonical dir
//!    (`~/.nika/registry/<owner>/<name>/<version>.nika.yaml`) beside a
//!    digest record; a cache hit re-verifies and runs OFFLINE. A bare
//!    ref writes a pin record so later bare refs never float
//!    (ADR-106 "pin by default").
//! 6. **Provenance tier + admission floor** (NEP-0016) — every
//!    resolution carries a closed-ladder tier (`unprovenanced <
//!    provenanced < stage-clear < verified`) admitted by the EVIDENCE
//!    the fetch observed, never by a claim; the operator's floor
//!    (`policy.toml` beside the cache) refuses anything below it
//!    (`NIKA-REG-008`, before the store — nothing is written), and the
//!    tier is recorded beside the digest so a cache hit re-tells the
//!    same truth (no grandfathering: a tightened floor refuses a hit
//!    too).
//!
//! The fetch happens at CLI-level resolution, BEFORE the workflow is
//! even parsed — a workflow's `permits:` govern the run's effects, not
//! this fetch (said in the help text too). SSRF enforcement stays on;
//! every URL is CONSTRUCTED from charset-validated components, never
//! taken from fetched data (closed-set law: a field the client cannot
//! vet is refused, never interpolated).

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use nika_kernel::http::HttpGetDyn;
use nika_kernel::{HttpError, HttpRequest};

/// The ref scheme — an argument starting with this is a registry ref.
const SCHEME: &str = "registry:";
/// The public index this engine speaks to in v1 (org/private indexes
/// arrive with the `nika add` verb — ADR-106 `--index`).
const INDEX_BASE: &str = "https://raw.githubusercontent.com/supernovae-st/nika-registry/main";
/// Raw content host for pinned artifact bytes.
const RAW_BASE: &str = "https://raw.githubusercontent.com";
/// Artifact size cap (the registry-v0.1 reference cap — workflows are
/// kilobytes of text).
const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
/// Index size cap (22 artifacts ≈ 30 KiB today; generous headroom).
pub const MAX_INDEX_BYTES: usize = 4 * 1024 * 1024;
/// The closed set of entry top-level keys (contract §1) — an unknown
/// field is a smuggling channel and refuses the whole entry.
const ENTRY_KEYS: [&str; 12] = [
    "schema",
    "type",
    "name",
    "publisher",
    "version",
    "description",
    "license",
    "spec",
    "source",
    "integrity",
    "cert",
    "signature",
];
/// The closed set of `[source]` keys (contract §1).
const SOURCE_KEYS: [&str; 3] = ["repo", "rev", "path"];

/// Is this CLI file argument a registry ref (`registry:…`)?
#[must_use]
pub fn is_registry_ref(arg: &str) -> bool {
    arg.starts_with(SCHEME)
}

/// A parsed `registry:owner/name[@version]` ref. Owner is REQUIRED in
/// v1 (no bare names): without a lockfile the qualified form is the
/// only shape immune to dependency confusion.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryRef {
    owner: String,
    name: String,
    version: Option<String>,
}

impl RegistryRef {
    fn coordinate(&self, version: &str) -> String {
        format!("{}/{}@{}", self.owner, self.name, version)
    }
}

mod sign;
mod tier;

pub use tier::ProvenanceTier;

// ---------------------------------------------------------------------
// Errors — one opaque type, teaching Display, greppable NIKA-REG codes.
// ---------------------------------------------------------------------

/// A registry resolution refusal. Every message teaches its fix; the
/// contract-allocated refusals carry a greppable `[NIKA-REG-00x]` code
/// (also via [`RegistryError::code`]).
#[derive(Debug)]
pub struct RegistryError {
    kind: ErrKind,
}

#[derive(Debug)]
enum ErrKind {
    /// The ref itself does not parse — teach the form.
    BadRef { arg: String, why: String },
    /// Nothing in the registry matches (NIKA-REG-001 · slopsquat guard).
    NotFound { what: String, hint: String },
    /// A matching advisory withdraws it (NIKA-REG-002 · MUST-refuse).
    Advisory {
        coordinate: String,
        ids: Vec<String>,
    },
    /// Fetched bytes do not hash to the pinned digest (NIKA-REG-003).
    HashMismatch {
        coordinate: String,
        expected: String,
        actual: String,
    },
    /// A cached copy no longer matches its recorded digest (NIKA-REG-004:
    /// a local record pins bytes; a mismatch fails, never floats).
    CacheTampered {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    /// The registry answered in a shape this engine cannot vet
    /// (NIKA-REG-005 · unknown schema / unknown field / index-entry drift).
    IndexShape { why: String },
    /// The artifact's minisign does not verify (NIKA-REG-006 · the bytes
    /// are not who they claim to be).
    SignatureInvalid { coordinate: String, why: String },
    /// The publisher's key differs from this machine's TOFU record
    /// (NIKA-REG-007 · a rewritten index cannot re-key a publisher we
    /// already trust).
    KeyChanged { publisher: String },
    /// The admitted tier is below the operator's admission floor
    /// (NIKA-REG-008 · NEP-0016 law 4: the artifact VERIFIED — the
    /// policy refuses it, after verification, before the store; a cache
    /// hit under a tightened floor refuses identically).
    BelowFloor {
        coordinate: String,
        tier: ProvenanceTier,
        floor: ProvenanceTier,
        cache_hit: bool,
    },
    /// The cache record names a tier this engine cannot admit — an
    /// unknown tier string, a reserved tier no v1 evidence can prove,
    /// or a `signed`/`tier` disagreement (NEP-0016 laws 1+6: the
    /// NIKA-REG-004 tampered class — a record that over-claims is
    /// treated as tampered, never trusted).
    CacheTierInvalid { path: PathBuf, why: String },
    /// The ref names an artifact that is not a workflow.
    NotAWorkflow { coordinate: String, kind: String },
    /// Cache miss and the network did not answer — the honest offline story.
    Offline { what: String, reason: String },
    /// The registry (or the pinned source) answered a non-200.
    FetchFailed {
        what: String,
        status: u16,
        hint: String,
    },
    /// A body over its cap.
    TooLarge {
        what: String,
        len: usize,
        cap: usize,
    },
    /// Local environment failure (cache dir · runtime · TLS init).
    Env { why: String },
}

impl RegistryError {
    fn new(kind: ErrKind) -> Self {
        Self { kind }
    }

    /// An environment-class refusal — public for the L4 composer (the
    /// executor/http constructors it wraps around this client fail in
    /// the same vocabulary).
    pub fn env(why: impl Into<String>) -> Self {
        Self::new(ErrKind::Env { why: why.into() })
    }

    /// The contract-allocated refusal code, when this refusal has one
    /// (`NIKA-REG-001..008` · ADR-106 + NEP-0016). Parse/transport/
    /// environment failures carry none.
    #[must_use]
    pub fn code(&self) -> Option<&'static str> {
        match &self.kind {
            ErrKind::NotFound { .. } => Some("NIKA-REG-001"),
            ErrKind::Advisory { .. } => Some("NIKA-REG-002"),
            ErrKind::HashMismatch { .. } => Some("NIKA-REG-003"),
            ErrKind::CacheTampered { .. } | ErrKind::CacheTierInvalid { .. } => {
                Some("NIKA-REG-004")
            }
            ErrKind::IndexShape { .. } => Some("NIKA-REG-005"),
            ErrKind::SignatureInvalid { .. } => Some("NIKA-REG-006"),
            ErrKind::KeyChanged { .. } => Some("NIKA-REG-007"),
            ErrKind::BelowFloor { .. } => Some("NIKA-REG-008"),
            _ => None,
        }
    }
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn short(hex: &str) -> &str {
            hex.get(..16).unwrap_or(hex)
        }
        match &self.kind {
            ErrKind::BadRef { arg, why } => write!(
                f,
                "cannot read `{arg}` as a registry ref: {why}\n  form: registry:owner/name  or  registry:owner/name@1.2.0"
            ),
            ErrKind::NotFound { what, hint } => write!(
                f,
                "[NIKA-REG-001] nothing in the registry matches {what}\n  fix: {hint}"
            ),
            ErrKind::Advisory { coordinate, ids } => write!(
                f,
                "[NIKA-REG-002] {coordinate} is withdrawn by advisory {} — refusing before any bytes move\n  see advisories/ in the registry for what happened and what to do",
                ids.join(", ")
            ),
            ErrKind::HashMismatch {
                coordinate,
                expected,
                actual,
            } => write!(
                f,
                "[NIKA-REG-003] {coordinate}: fetched bytes do not match the pinned digest\n  entry pins sha256 {}… · fetched {}…\n  nothing was written. The source moved or the entry lies — report it to the registry.",
                short(expected),
                short(actual)
            ),
            ErrKind::CacheTampered {
                path,
                expected,
                actual,
            } => write!(
                f,
                "[NIKA-REG-004] the cached copy no longer matches its recorded digest\n  file: {}\n  recorded sha256 {}… · found {}…\n  fix: delete that file and re-run — it will re-fetch and re-verify",
                path.display(),
                short(expected),
                short(actual)
            ),
            ErrKind::IndexShape { why } => write!(
                f,
                "[NIKA-REG-005] the registry answered in a shape this engine cannot vet: {why}\n  a schema or field the client does not understand is refused, never skipped"
            ),
            ErrKind::SignatureInvalid { coordinate, why } => write!(
                f,
                "[NIKA-REG-006] {coordinate}: the artifact's signature does not verify ({why})\n  nothing was written — the bytes are not who they claim to be. Report it to the registry."
            ),
            ErrKind::KeyChanged { publisher } => write!(
                f,
                "[NIKA-REG-007] the publisher key for {publisher} differs from this machine's TOFU record (~/.nika/registry/keys/{publisher}.pub)\n  a key rotation is an OPERATOR decision — if it was deliberate, delete that record and re-run to re-anchor; otherwise the registry may be compromised"
            ),
            ErrKind::BelowFloor {
                coordinate,
                tier,
                floor,
                cache_hit,
            } => {
                let line2 = if *cache_hit {
                    "the policy changed under the cache — the cache does not grandfather\n  fix: delete the cached record and re-run; it re-fetches and re-proves against today's registry (a registry-side upgrade rewrites nothing on its own)"
                } else {
                    "the artifact VERIFIED — the policy refuses it, and nothing was written"
                };
                write!(
                    f,
                    "[NIKA-REG-008] {coordinate} resolves at tier `{tier}`, below this machine's admission floor `{floor}`\n  {line2}\n  the floor is operator data: ~/.nika/registry/policy.toml — lower it (or delete the file for the `unprovenanced` default) only if you accept what the lower tiers do not prove"
                )
            }
            ErrKind::CacheTierInvalid { path, why } => write!(
                f,
                "[NIKA-REG-004] the cache record {} claims a provenance tier this engine cannot admit: {why}\n  a record that over-claims is treated as tampered, never trusted\n  fix: delete that file and re-run — it will re-fetch and re-verify",
                path.display()
            ),
            ErrKind::NotAWorkflow { coordinate, kind } => write!(
                f,
                "{coordinate} is a {kind}, not a workflow — check and run consume workflows"
            ),
            ErrKind::Offline { what, reason } => write!(
                f,
                "{what}: not in the local cache and the network did not answer ({reason})\n  an already-fetched artifact runs offline from ~/.nika/registry/ — this one has not been fetched yet"
            ),
            ErrKind::FetchFailed { what, status, hint } => {
                write!(f, "{what} answered HTTP {status}\n  {hint}")
            }
            ErrKind::TooLarge { what, len, cap } => write!(
                f,
                "{what} is {len} bytes — over the {cap}-byte registry cap (workflows are kilobytes of text)"
            ),
            ErrKind::Env { why } => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for RegistryError {}

// ---------------------------------------------------------------------
// Resolution result
// ---------------------------------------------------------------------

/// A resolved registry ref: the verified local file check/run consume.
#[derive(Debug)]
pub struct Resolved {
    /// The cached artifact path — feed it to check/run like any file.
    pub path: PathBuf,
    /// `owner/name@version` as resolved.
    pub coordinate: String,
    /// The verified content digest (64-hex sha256).
    pub sha256: String,
    /// `true` when bytes moved this call; `false` on a cache hit.
    pub fetched: bool,
    /// `true` when a bare ref was answered by the local pin record
    /// (no network involved in choosing the version).
    pub pinned: bool,
    /// `true` when the artifact's minisign verified (registry-v0.2) —
    /// recorded at fetch, so a cache-hit receipt tells the same truth.
    /// `false` on an unsigned entry (the v0.1 digest floor). Kept beside
    /// `tier` for the pre-NEP-0016 readers: it is exactly
    /// `tier >= provenanced`.
    pub signed: bool,
    /// The provenance tier the fetch's EVIDENCE admitted (NEP-0016) —
    /// recorded beside the digest, so a cache hit re-tells the truth of
    /// the day it was fetched (evidence is not re-sought on a hit). v1
    /// admits `unprovenanced` (digest floor) and `provenanced`
    /// (minisign + TOFU) only.
    pub tier: ProvenanceTier,
}

impl Resolved {
    /// The one/two-line stderr note the CLI prints — where the artifact
    /// lives, its verified digest, and its provenance tier (stdout
    /// stays machine-pure).
    #[must_use]
    pub fn describe(&self) -> String {
        let short = self.sha256.get(..16).unwrap_or(&self.sha256);
        let evidence = match self.tier {
            ProvenanceTier::Unprovenanced => "unsigned entry (v0.1 digest floor)",
            ProvenanceTier::Provenanced => "signed (minisign + TOFU)",
            // v1 admits the reserved tiers nowhere — a Resolved cannot
            // carry one (the floor gate + the tampered class see to it).
            ProvenanceTier::StageClear | ProvenanceTier::Verified => {
                "reserved evidence (never admitted in v1)"
            }
        };
        if self.fetched {
            format!(
                "→ registry {} · fetched + digest verified · tier {} · {evidence} (sha256 {short}…)\n  cached: {} — later runs use this copy, offline included",
                self.coordinate,
                self.tier,
                self.path.display()
            )
        } else {
            format!(
                "→ registry {} · cache · digest re-verified (sha256 {short}…) · offline · tier {} · {evidence} (recorded at fetch)",
                self.coordinate, self.tier,
            )
        }
    }
}

// ---------------------------------------------------------------------
// Index / entry / cache-record shapes
// ---------------------------------------------------------------------

/// `index.json` (contract §4) — tolerant read: the index is a derived
/// projection and may grow fields; every load-bearing claim is
/// re-verified against the entry + the bytes.
#[derive(serde::Deserialize)]
struct Index {
    index_schema: u64,
    #[serde(default)]
    artifacts: Vec<IndexArtifact>,
}

#[derive(serde::Deserialize, Clone)]
struct IndexArtifact {
    name: String,
    publisher: String,
    version: String,
    #[serde(rename = "type")]
    kind: String,
    sha256: String,
    source: SourcePin,
    #[serde(default)]
    advisories: Vec<String>,
}

/// The full-commit content pin (contract §1 R2).
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
struct SourcePin {
    repo: String,
    rev: String,
    path: String,
}

/// The cache digest record (`<version>.meta.json`) — what a cache hit
/// re-verifies against.
#[derive(serde::Serialize, serde::Deserialize)]
struct Meta {
    sha256: String,
    coordinate: String,
    source: SourcePin,
    /// Whether the artifact's minisign verified at fetch (v0.2 records —
    /// absent on older records = unsigned floor, `false`). Still written
    /// beside `tier` so a pre-NEP-0016 engine reads the record as the
    /// boolean it speaks (a downgrade is the safe direction).
    #[serde(default)]
    signed: bool,
    /// The tier the fetch's evidence admitted (NEP-0016 records —
    /// absent on pre-NEP records, which read as the tier `signed`
    /// denotes). Validated at hit time: an unknown string, a reserved
    /// tier, or a `signed`/`tier` disagreement is the tampered class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tier: Option<String>,
}

/// The tier a cache record tells (NEP-0016 laws 1+6): a `tier` string
/// must be known AND admissible by v1 evidence AND agree with the
/// legacy boolean — anything else is the tampered class (a record that
/// over-claims never floats). A pre-NEP record (no `tier`) reads as
/// the tier its `signed` denotes.
fn meta_tier(meta: &Meta, meta_path: &Path) -> Result<ProvenanceTier, RegistryError> {
    let invalid = |why: String| {
        RegistryError::new(ErrKind::CacheTierInvalid {
            path: meta_path.to_path_buf(),
            why,
        })
    };
    match &meta.tier {
        None => Ok(if meta.signed {
            ProvenanceTier::Provenanced
        } else {
            ProvenanceTier::Unprovenanced
        }),
        Some(raw) => {
            let tier = ProvenanceTier::parse(raw).ok_or_else(|| {
                invalid(format!(
                    "`{raw}` is not a known tier (the closed ladder is unprovenanced < provenanced < stage-clear < verified)"
                ))
            })?;
            if !tier.admissible_by_v1_evidence() {
                return Err(invalid(format!(
                    "`{}` is reserved — no evidence v1 can observe admits it, so no honest v1 fetch recorded it",
                    tier.as_str()
                )));
            }
            if meta.signed != tier.denotes_signed() {
                return Err(invalid(format!(
                    "`signed: {}` and `tier: \"{}\"` disagree — a record speaks one truth",
                    meta.signed,
                    tier.as_str()
                )));
            }
            Ok(tier)
        }
    }
}

/// What the network lane proved: the verified bytes plus the tier
/// THEIR evidence admits (NEP-0016 law 2 — observed, never claimed).
struct Fetched {
    bytes: Vec<u8>,
    tier: ProvenanceTier,
    /// A first-sight TOFU key (`publisher`, `pubkey`) that verified but
    /// is not anchored yet — the write waits for the floor gate
    /// (NIKA-REG-008 writes NOTHING, the anchor included).
    pending_key: Option<(String, String)>,
}

// ---------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------

/// The registry client: resolve a ref to a verified, cached local file.
/// Generic over the kernel HTTP seam — production injects `ReqwestHttp`,
/// tests a mock (Invariant #27).
pub struct RegistryClient<H> {
    http: H,
    cache_root: PathBuf,
}

impl<H: HttpGetDyn> RegistryClient<H> {
    pub fn new(http: H, cache_root: PathBuf) -> Self {
        Self { http, cache_root }
    }

    /// Resolve `registry:owner/name[@version]` → verified cache path.
    ///
    /// Order of authority: explicit version, else the local pin record
    /// (a bare ref never floats — ADR-106), else the network's newest
    /// `SemVer`. The cache answers before the network; a hit re-verifies
    /// its digest record. The operator's admission floor (NEP-0016)
    /// gates BOTH lanes: a hit whose recorded tier is below the floor
    /// refuses, and a fetch whose evidence admits less than the floor
    /// refuses after verification, before the store — nothing written,
    /// the TOFU anchor included.
    pub async fn resolve(&self, arg: &str) -> Result<Resolved, RegistryError> {
        let r = parse_ref(arg)?;
        let policy = tier::Policy::load(&self.cache_root)?.with_project_floor()?;
        let (version, pinned) = match &r.version {
            Some(v) => (Some(v.clone()), false),
            None => (self.read_pin(&r)?, true),
        };
        if let Some(v) = &version
            && let Some(hit) = self.cached(&r, v, pinned, policy.floor)?
        {
            return Ok(hit);
        }
        let index = self.fetch_index().await?;
        let art = select_artifact(&index, &r, version.as_deref())?;
        let parsed = self.entry_digest(&art).await?;
        let fetched = self.fetch_artifact(&art, &parsed).await?;
        if fetched.tier < policy.floor {
            return Err(RegistryError::new(ErrKind::BelowFloor {
                coordinate: r.coordinate(&art.version),
                tier: fetched.tier,
                floor: policy.floor,
                cache_hit: false,
            }));
        }
        // The floor passed — only now may anything be written: the
        // first-sight TOFU key anchors (it verified), then the store.
        if let Some((publisher, pubkey)) = &fetched.pending_key {
            sign::tofu_record(&self.keys_dir(), publisher, pubkey)?;
        }
        self.store(&r, &art, &parsed.digest, &fetched)
    }

    // -- cache lane ----------------------------------------------------

    fn dir_of(&self, r: &RegistryRef) -> PathBuf {
        self.cache_root.join(&r.owner).join(&r.name)
    }

    /// The TOFU key store root (`~/.nika/registry/keys/`) — first key
    /// seen anchors; a later different key is a hard refusal.
    fn keys_dir(&self) -> PathBuf {
        self.cache_root.join("keys")
    }

    /// The pin record a bare ref wrote at its first resolve — `None`
    /// when this name was never bare-resolved on this machine.
    fn read_pin(&self, r: &RegistryRef) -> Result<Option<String>, RegistryError> {
        let path = self.dir_of(r).join("pin");
        let read = std::fs::read_to_string(&path); // seam-bypass-ok: local cache · #512 follow-up
        let raw = match read {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(RegistryError::env(format!(
                    "cannot read the pin record {}: {e}",
                    path.display()
                )));
            }
        };
        let version = raw.trim().to_owned();
        if version_key(&version).is_none() {
            // A pin that is not a version is a path-injection vector —
            // refuse with the heal, never interpolate it.
            return Err(RegistryError::env(format!(
                "the pin record {} does not hold a version — delete it and re-run",
                path.display()
            )));
        }
        Ok(Some(version))
    }

    /// The cache probe: artifact + digest record present → re-hash the
    /// bytes against the record (a local record pins bytes; a mismatch
    /// fails, never floats — NIKA-REG-004) and re-tell its recorded tier
    /// (evidence is not re-sought on a hit — the record IS the evidence
    /// of what that fetch proved; a tier the record cannot admit is the
    /// tampered class). A hit whose tier is below the operator's floor
    /// refuses (NIKA-REG-008 — the policy changed under the cache; the
    /// cache does not grandfather). Anything missing → `None` (the
    /// network lane heals it).
    fn cached(
        &self,
        r: &RegistryRef,
        version: &str,
        pinned: bool,
        floor: ProvenanceTier,
    ) -> Result<Option<Resolved>, RegistryError> {
        let dir = self.dir_of(r);
        let artifact = dir.join(format!("{version}.nika.yaml"));
        let meta_path = dir.join(format!("{version}.meta.json"));
        let arte = std::fs::read(&artifact); // seam-bypass-ok: local cache · #512 follow-up
        let meta = std::fs::read(&meta_path); // seam-bypass-ok: local cache · #512 follow-up
        let (Ok(bytes), Ok(meta_raw)) = (arte, meta) else {
            return Ok(None);
        };
        let Ok(meta) = serde_json::from_slice::<Meta>(&meta_raw) else {
            return Ok(None); // an unreadable record is refetched, not trusted
        };
        let actual = sha256_hex(&bytes);
        if actual != meta.sha256 {
            return Err(RegistryError::new(ErrKind::CacheTampered {
                path: artifact,
                expected: meta.sha256,
                actual,
            }));
        }
        let tier = meta_tier(&meta, &meta_path)?;
        if tier < floor {
            return Err(RegistryError::new(ErrKind::BelowFloor {
                coordinate: r.coordinate(version),
                tier,
                floor,
                cache_hit: true,
            }));
        }
        Ok(Some(Resolved {
            path: artifact,
            coordinate: r.coordinate(version),
            sha256: actual,
            fetched: false,
            pinned,
            signed: meta.signed,
            tier,
        }))
    }

    // -- network lane ----------------------------------------------------

    async fn fetch_index(&self) -> Result<Index, RegistryError> {
        let body = self
            .get_bytes(
                &format!("{INDEX_BASE}/index.json"),
                "the registry index",
                MAX_INDEX_BYTES,
                "the registry may be unreachable or moved — see https://github.com/supernovae-st/nika-registry",
            )
            .await?;
        let index: Index = serde_json::from_slice(&body).map_err(|e| {
            RegistryError::new(ErrKind::IndexShape {
                why: format!("index.json does not parse: {e}"),
            })
        })?;
        if index.index_schema != 1 {
            return Err(RegistryError::new(ErrKind::IndexShape {
                why: format!(
                    "index_schema {} — this engine speaks schema 1",
                    index.index_schema
                ),
            }));
        }
        Ok(index)
    }

    /// The digest of record comes from the ENTRY, never the index alone
    /// (contract §4). The entry is fetched at its CONSTRUCTED path and
    /// cross-checked field-by-field against the index's claims — drift
    /// between a projection and its source is treated as tampered.
    async fn entry_digest(&self, art: &IndexArtifact) -> Result<ParsedEntry, RegistryError> {
        let url = format!(
            "{INDEX_BASE}/registry/workflows/{}/{}/{}.toml",
            art.publisher, art.name, art.version
        );
        let body = self
            .get_bytes(
                &url,
                "the registry entry",
                MAX_ARTIFACT_BYTES,
                "the index lists an entry the registry does not serve — the registry is inconsistent; report it",
            )
            .await?;
        let text = String::from_utf8(body).map_err(|_| {
            RegistryError::new(ErrKind::IndexShape {
                why: "the entry is not UTF-8 text".to_owned(),
            })
        })?;
        parse_entry(&text, art)
    }

    async fn fetch_artifact(
        &self,
        art: &IndexArtifact,
        parsed: &ParsedEntry,
    ) -> Result<Fetched, RegistryError> {
        let coordinate = format!("{}/{}@{}", art.publisher, art.name, art.version);
        let url = format!(
            "{RAW_BASE}/{}/{}/{}",
            art.source.repo, art.source.rev, art.source.path
        );
        let bytes = self
            .get_bytes(
                &url,
                &coordinate,
                MAX_ARTIFACT_BYTES,
                "the pinned source is gone (the author's repo moved or rewrote history) — report it to the registry",
            )
            .await?;
        let actual = sha256_hex(&bytes);
        if actual != parsed.digest {
            return Err(RegistryError::new(ErrKind::HashMismatch {
                coordinate,
                expected: parsed.digest.clone(),
                actual,
            }));
        }
        // v0.2 — the authenticity half: a signed entry must VERIFY (the
        // digest already proved consistency; the minisign proves origin).
        // The tier is what the OBSERVED evidence admits (NEP-0016 law 2 —
        // never a claim), and a first-sight TOFU key comes back PENDING:
        // the anchor write waits for the floor gate (a refused fetch
        // writes nothing, the key record included).
        let (tier, pending_key) = match &parsed.signature {
            Some(block) => {
                sign::verify_detached(&coordinate, block, &bytes)?;
                let pending = sign::tofu_check(&self.keys_dir(), &art.publisher, &block.pubkey)?;
                (ProvenanceTier::Provenanced, pending)
            }
            None => (ProvenanceTier::Unprovenanced, None),
        };
        Ok(Fetched {
            bytes,
            tier,
            pending_key,
        })
    }

    /// One capped GET with the honest failure taxonomy: transport error
    /// → the offline story · non-200 → what answered + the hint · over
    /// cap → the size law.
    async fn get_bytes(
        &self,
        url: &str,
        what: &str,
        cap: usize,
        non_200_hint: &str,
    ) -> Result<Vec<u8>, RegistryError> {
        let resp = self
            .http
            .get(HttpRequest::get(url))
            .await
            .map_err(|e: HttpError| {
                RegistryError::new(ErrKind::Offline {
                    what: what.to_owned(),
                    reason: e.to_string(),
                })
            })?;
        if resp.status != 200 {
            return Err(RegistryError::new(ErrKind::FetchFailed {
                what: what.to_owned(),
                status: resp.status,
                hint: non_200_hint.to_owned(),
            }));
        }
        if resp.body.len() > cap {
            return Err(RegistryError::new(ErrKind::TooLarge {
                what: what.to_owned(),
                len: resp.body.len(),
                cap,
            }));
        }
        Ok(resp.body.to_vec())
    }

    // -- store ----------------------------------------------------------

    /// Write the VERIFIED bytes + digest record (atomic: temp sibling +
    /// rename), and the pin when the ref was bare. Nothing lands here
    /// unless the hash already matched AND the tier passed the floor.
    fn store(
        &self,
        r: &RegistryRef,
        art: &IndexArtifact,
        digest: &str,
        fetched: &Fetched,
    ) -> Result<Resolved, RegistryError> {
        let dir = self.dir_of(r);
        let made = std::fs::create_dir_all(&dir); // seam-bypass-ok: local cache · #512 follow-up
        made.map_err(|e| {
            RegistryError::env(format!(
                "cannot create the cache dir {}: {e}",
                dir.display()
            ))
        })?;
        let coordinate = r.coordinate(&art.version);
        let meta = Meta {
            sha256: digest.to_owned(),
            coordinate: coordinate.clone(),
            source: art.source.clone(),
            signed: fetched.tier.denotes_signed(),
            tier: Some(fetched.tier.as_str().to_owned()),
        };
        let meta_json = serde_json::to_string_pretty(&meta)
            .map_err(|e| RegistryError::env(format!("cannot encode the digest record: {e}")))?;
        let artifact = dir.join(format!("{}.nika.yaml", art.version));
        write_atomic(&artifact, &fetched.bytes)?;
        write_atomic(
            &dir.join(format!("{}.meta.json", art.version)),
            meta_json.as_bytes(),
        )?;
        if r.version.is_none() {
            write_atomic(&dir.join("pin"), format!("{}\n", art.version).as_bytes())?;
        }
        Ok(Resolved {
            path: artifact,
            coordinate,
            sha256: digest.to_owned(),
            fetched: true,
            pinned: false,
            signed: fetched.tier.denotes_signed(),
            tier: fetched.tier,
        })
    }
}

/// Pick the artifact a ref names, out of the index: exact version when
/// asked, newest `SemVer` otherwise — then the refusal ladder (advisory ·
/// type · pin shape) BEFORE any bytes move.
fn select_artifact(
    index: &Index,
    r: &RegistryRef,
    version: Option<&str>,
) -> Result<IndexArtifact, RegistryError> {
    let named: Vec<&IndexArtifact> = index
        .artifacts
        .iter()
        .filter(|a| a.publisher == r.owner && a.name == r.name)
        .collect();
    if named.is_empty() {
        return Err(RegistryError::new(ErrKind::NotFound {
            what: format!("`{}/{}`", r.owner, r.name),
            hint: "check the spelling against https://github.com/supernovae-st/nika-registry — a name an agent suggests must actually exist (this refusal is the guard)".to_owned(),
        }));
    }
    let workflows: Vec<&IndexArtifact> = named
        .iter()
        .copied()
        .filter(|a| a.kind == "workflow")
        .collect();
    if workflows.is_empty() {
        // Named, but nothing runnable: teach what it IS instead.
        let kind = named[0].kind.clone();
        return Err(RegistryError::new(ErrKind::NotAWorkflow {
            coordinate: format!("{}/{}", r.owner, r.name),
            kind,
        }));
    }
    let chosen: &IndexArtifact = match version {
        Some(v) => workflows
            .iter()
            .copied()
            .find(|a| a.version == v)
            .ok_or_else(|| {
                let mut published: Vec<&str> =
                    workflows.iter().map(|a| a.version.as_str()).collect();
                published.sort_unstable();
                RegistryError::new(ErrKind::NotFound {
                    what: format!("`{}/{}@{v}`", r.owner, r.name),
                    hint: format!("published versions: {}", published.join(", ")),
                })
            })?,
        None => workflows
            .iter()
            .copied()
            .filter(|a| version_key(&a.version).is_some())
            .max_by_key(|a| version_key(&a.version))
            .ok_or_else(|| {
                RegistryError::new(ErrKind::IndexShape {
                    why: format!("no version of {}/{} parses as SemVer", r.owner, r.name),
                })
            })?,
    };
    if !chosen.advisories.is_empty() {
        return Err(RegistryError::new(ErrKind::Advisory {
            coordinate: format!("{}/{}@{}", r.owner, r.name, chosen.version),
            ids: chosen.advisories.clone(),
        }));
    }
    vet_pin(chosen)?;
    Ok(chosen.clone())
}

/// The closed-set vet of everything that becomes a URL or a path — a
/// field the client cannot vet is refused, never interpolated.
fn vet_pin(a: &IndexArtifact) -> Result<(), RegistryError> {
    let shape = |why: String| RegistryError::new(ErrKind::IndexShape { why });
    if a.sha256.len() != 64 || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(shape(format!("`{}` is not a full 64-hex sha256", a.sha256)));
    }
    if a.source.rev.len() != 40 || !a.source.rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(shape(format!(
            "`{}` is not a full 40-hex commit (tags and branches are forbidden pins)",
            a.source.rev
        )));
    }
    let repo_ok = a
        .source
        .repo
        .split_once('/')
        .is_some_and(|(owner, repo)| valid_owner(owner) && valid_repo_name(repo));
    if !repo_ok {
        return Err(shape(format!(
            "`{}` is not an owner/name repo",
            a.source.repo
        )));
    }
    if !valid_source_path(&a.source.path) {
        return Err(shape(format!(
            "`{}` is not a plain repo-relative path",
            a.source.path
        )));
    }
    if version_key(&a.version).is_none() {
        return Err(shape(format!("`{}` is not a SemVer version", a.version)));
    }
    Ok(())
}

/// A vetted entry (contract §1): the digest of record + the optional
/// v0.2 signature block.
#[derive(Debug)]
struct ParsedEntry {
    /// `integrity.sha256`, cross-checked against the index's claim.
    digest: String,
    /// The `[signature]` block when the entry is signed (v0.2).
    signature: Option<sign::SignatureBlock>,
}

/// Parse + vet the entry TOML (contract §1): closed key set, then the
/// fields cross-checked against the index's claims. Returns the digest
/// of record.
fn parse_entry(text: &str, art: &IndexArtifact) -> Result<ParsedEntry, RegistryError> {
    let shape = |why: String| RegistryError::new(ErrKind::IndexShape { why });
    let doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| shape(format!("the entry does not parse as TOML: {e}")))?;
    for (key, _) in doc.iter() {
        if !ENTRY_KEYS.contains(&key) {
            return Err(shape(format!("unknown entry field `{key}`")));
        }
    }
    match doc.get("schema").and_then(toml_edit::Item::as_integer) {
        Some(1) => {}
        Some(n) => {
            return Err(shape(format!(
                "entry schema {n} — this engine speaks schema 1"
            )));
        }
        None => return Err(shape("the entry is missing `schema`".to_owned())),
    }
    let str_at = |table: Option<&str>, key: &str| -> Option<String> {
        let item = match table {
            Some(t) => doc.get(t)?.get(key)?,
            None => doc.get(key)?,
        };
        item.as_str().map(str::to_owned)
    };
    for (table, allowed) in [("source", &SOURCE_KEYS[..]), ("integrity", &["sha256"][..])] {
        if let Some(entries) = doc.get(table).and_then(toml_edit::Item::as_table) {
            for (key, _) in entries {
                if !allowed.contains(&key) {
                    return Err(shape(format!("unknown [{table}] field `{key}`")));
                }
            }
        }
    }
    let claims = [
        ("type", str_at(None, "type"), &art.kind),
        ("name", str_at(None, "name"), &art.name),
        ("publisher", str_at(None, "publisher"), &art.publisher),
        ("version", str_at(None, "version"), &art.version),
        (
            "source.repo",
            str_at(Some("source"), "repo"),
            &art.source.repo,
        ),
        ("source.rev", str_at(Some("source"), "rev"), &art.source.rev),
        (
            "source.path",
            str_at(Some("source"), "path"),
            &art.source.path,
        ),
        (
            "integrity.sha256",
            str_at(Some("integrity"), "sha256"),
            &art.sha256,
        ),
    ];
    for (field, entry_value, index_value) in claims {
        match entry_value {
            Some(v) if &v == index_value => {}
            Some(_) => {
                return Err(shape(format!(
                    "the index and the entry disagree on {field} — a projection that cannot be re-derived is treated as tampered"
                )));
            }
            None => return Err(shape(format!("the entry is missing {field}"))),
        }
    }
    // Cross-checked equal — the entry's digest IS art.sha256 now.
    let signature = sign::parse_signature_block(doc.get("signature"))?;
    Ok(ParsedEntry {
        digest: art.sha256.clone(),
        signature,
    })
}

/// Atomic write: temp sibling + rename, so a torn write can never look
/// like a verified artifact.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), RegistryError> {
    let io_err =
        |e: std::io::Error| RegistryError::env(format!("cannot write {}: {e}", path.display()));
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(io_err)?; // seam-bypass-ok: local artifact cache (digest-pinned reads/writes) · FsDyn descent tracked as the #512 follow-up
    std::fs::rename(&tmp, path).map_err(io_err) // seam-bypass-ok: local artifact cache (digest-pinned reads/writes) · FsDyn descent tracked as the #512 follow-up
}

// ---------------------------------------------------------------------
// Ref parsing + version ordering
// ---------------------------------------------------------------------

fn parse_ref(arg: &str) -> Result<RegistryRef, RegistryError> {
    let bad = |why: &str| {
        RegistryError::new(ErrKind::BadRef {
            arg: arg.to_owned(),
            why: why.to_owned(),
        })
    };
    let rest = arg
        .strip_prefix(SCHEME)
        .ok_or_else(|| bad("missing the `registry:` scheme"))?;
    // ⭐ ONE grammar, two readers — it lives at L0 in `nika_vocab`
    // (`nika-check` is L0 and cannot depend on this L2 crate, so the
    // shared home cannot be here). This crate keeps what is genuinely
    // resolution's: version ORDERING, and the pin ladder that reads an
    // UNPINNED ref legitimately where a workflow may not.
    let parsed = nika_vocab::registry_ref::parse(rest).map_err(|d| bad(d.teaching()))?;
    Ok(RegistryRef {
        owner: parsed.owner.to_owned(),
        name: parsed.name.to_owned(),
        version: parsed.version.map(str::to_owned),
    })
}

fn valid_owner(s: &str) -> bool {
    // ONE definition, at L0 — the owner charset is grammar, not
    // resolution, and a second spelling of it would drift.
    nika_vocab::registry_ref::valid_owner(s)
}

/// GitHub repo names also allow `.` and `_` — path-safe (no separators).
fn valid_repo_name(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

#[allow(
    dead_code,
    reason = "the grammar moved to L0; kept as the named seam for the parity pin"
)]
fn valid_name(s: &str) -> bool {
    nika_vocab::registry_ref::valid_name(s)
}

/// Repo-relative, no traversal, plain charset — the R2 path law.
fn valid_source_path(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('/')
        && s.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
}

/// `SemVer`-precedence sort key (`SemVer` §11, the get.py `version_key`
/// port): numeric core compared numerically; a STABLE release outranks
/// any pre-release of the same core (`0.2.0` > `0.2.0-rc1`); numeric
/// pre-release ids compare numerically and rank below alphanumeric ones.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
struct VersionKey {
    core: Vec<u64>,
    stable: bool,
    pre: Vec<PreId>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
enum PreId {
    Num(u64),
    Alpha(String),
}

fn version_key(v: &str) -> Option<VersionKey> {
    if v.contains('+') {
        return None; // pin without build metadata — a pin must name ONE thing
    }
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let nums: Vec<u64> = core
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if nums.len() != 3 {
        return None;
    }
    let pre_ids = match pre {
        None => Vec::new(),
        Some("") => return None,
        Some(pre) => pre
            .split('.')
            .map(|id| {
                if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return None;
                }
                Some(match id.parse::<u64>() {
                    Ok(n) => PreId::Num(n),
                    Err(_) => PreId::Alpha(id.to_owned()),
                })
            })
            .collect::<Option<Vec<_>>>()?,
    };
    Some(VersionKey {
        core: nums,
        stable: pre_ids.is_empty(),
        pre: pre_ids,
    })
}

// ---------------------------------------------------------------------
// Production wiring (the ONE seam main.rs calls)
// ---------------------------------------------------------------------

/// sha256 as lowercase hex — the digest the index pins. Local on purpose:
/// an L2 service crate reaches no L4 utility (layer DAG); the four lines
/// cost less than the illegal edge.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

/// Lowercase-hex encode (no hex crate — two lines, zero deps).
fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `~/.nika/registry` — the ONE canonical cache dir (HOME/USERPROFILE,
/// the same resolution `nika wire` uses for editor configs).
// Env read is config-path state, not a secret — the same scoped
// exemption as `wire.rs::home_path`.
#[allow(clippy::disallowed_methods)]
pub fn default_cache_root() -> Result<PathBuf, RegistryError> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".nika").join("registry"))
        .ok_or_else(|| RegistryError::env("cannot find HOME/USERPROFILE for the registry cache"))
}

#[cfg(test)]
mod tests;
