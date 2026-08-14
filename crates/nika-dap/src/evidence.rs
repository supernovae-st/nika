// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The evidence pack (the verifiable-run wave · A5) — ONE bundle an
//! auditor takes offline and believes the run. Descended from
//! `nika-cli`'s `verbs::evidence` 2026-07-21 (the crate-size cap · the
//! W0 trace-descent precedent): the pack COMPUTATION lives here in the
//! forensics plane; the CLI keeps the handle resolution and the
//! one-line human summary.
//!
//! The pack law is PROVENANCE: every field in `pack.json` is traceable
//! to bytes inside the bundle — the journal's own `workflow_started`
//! fields first, the seal's signed `covers` second, a `--workflow
//! <file>` re-check ONLY when the file's hash matches the journal's,
//! and an honest `null` (the reason named in `unavailable`) when none
//! of those exist. A claim without provenance is marketing; the pack
//! is an evidence artifact, so it never guesses.
//!
//! ## The bundle (`evidence_format: 1`)
//!
//! - `journal.ndjson` — the run journal, in one of TWO disclosure
//!   classes (T9 · the SLSA/in-toto posture: an attestation carries
//!   the payload's HASH, never the payload):
//!   - **redacted** (the default) — the projection: payload fields
//!     (model outputs · tool results · file contents · failure
//!     details · streamed chunks · shown prompts) ride as sha256
//!     placeholders, structural fields ride verbatim. The pack proves
//!     the run's INTEGRITY, not its CONTENT — two different asks, and
//!     content disclosure is a separate, operator-side gesture
//!     (VERIFY.md teaches both).
//!   - **full** (`--full`) — the exact bytes, copied, never
//!     re-serialized (the chain hashes over those bytes). The manifest
//!     SAYS the class: a silent full pack is the leak wearing a new
//!     hat.
//! - `pack.json` — the manifest: trace head + chain status · the seal
//!   grade · the workflow's semantic hash · the declared `permits:`
//!   boundary · the trifecta static verdict · the sandbox mode · the
//!   engine version · the redaction class · `exported_at`.
//! - `receipt.json` — the spec-15 receipt
//!   ([`nika_runtime::proof::receipt::build_run_receipt`]) when a
//!   hash-checked workflow was available (its certificate is a
//!   check-time artifact the journal never carried).
//! - `VERIFY.md` — the auditor's checks, per class, plain.
//!
//! An UNSEALED journal packs too: `seal.present: false` and VERIFY.md
//! says what that means (tamper-EVIDENT chain only). Never a faked seal.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use nika_event::{Event, EventKind};
use nika_runtime::proof::ir::semantic_ir_hash;
use nika_runtime::proof::receipt::build_run_receipt;
use nika_runtime::proof::{HashDomain, SemanticHash, preimage};
use serde_json::{Value, json};

use crate::chain::{Verdict, walk};
use nika_event::source_id::sha256_hex;

mod redact;

/// The pack envelope version (`pack.json`'s `evidence_format` field) —
/// additive fields bump nothing (the forward-compat posture); a shape
/// break lands as `evidence_format: 2`, never a flag day.
const EVIDENCE_FORMAT: u32 = 1;

/// The receipt's lock-digest placeholder: the journal does not record
/// the `nika.lock` the run resolved under, so the receipt SAYS so —
/// hashing whatever lock file sits nearby TODAY would be a claim
/// without provenance (it may differ from run time). Shared with the
/// teardown seal (F-P2): the run's receipt fold says the same words.
pub(crate) const LOCK_UNRECORDED: &str =
    "unrecorded — the journal does not carry the run's nika.lock digest";

/// The one pointer every workflow-derived null shares.
const WORKFLOW_HINT: &str =
    "pass --workflow <file> — the pack hash-checks it against the journal before trusting it";

/// A pack failure — every variant maps to the environment exit class
/// (spec §4 · 3): an unreadable journal, a named-but-unusable
/// workflow, a bundle write that failed. The CLI maps the `Display`
/// text verbatim.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PackError {
    /// The journal file cannot be read (missing · permissions · not UTF-8).
    #[error("cannot read {0}")]
    UnreadableJournal(String),
    /// The file is not a journal this engine wrote — nothing to attest.
    #[error("{0}")]
    NotAJournal(String),
    /// The `--workflow` file was NAMED but cannot be used (unreadable ·
    /// unparseable) — an invocation error, not a pack finding.
    #[error("{0}")]
    Workflow(String),
    /// A bundle write failed (the directory is never clobbered).
    #[error("{0}")]
    Write(String),
}

/// The assembled pack, ready to write or project: the manifest value,
/// the receipt (when a hash-checked workflow made one possible), the
/// VERIFY.md body, and journal.ndjson's content.
#[derive(Debug)]
#[non_exhaustive]
pub struct EvidencePack {
    /// `pack.json`'s content (the `evidence_format: 1` envelope).
    pub manifest: Value,
    /// `receipt.json`'s content, when buildable (see the module doc).
    pub receipt: Option<Value>,
    /// `VERIFY.md`'s content.
    pub verify_md: String,
    /// `journal.ndjson`'s content: the redacted PROJECTION on the
    /// default class (payload fields as sha256 placeholders — the
    /// original's chain links ride verbatim but do not re-walk here),
    /// the journal's exact bytes on the full class (the chain hashes
    /// over those bytes).
    pub journal: String,
}

/// The pack's disclosure class (T9) — REDACTED by default: the
/// auditor's bundle proves the run's INTEGRITY, not its CONTENT.
/// Crate-internal: the public surface is the two named builders, so
/// no caller picks a class silently.
enum Class {
    /// Payload fields ride as sha256 placeholders ([`redact`]).
    Redacted,
    /// The journal's exact bytes (`--full` — the manifest SAYS it).
    Full,
}

/// The journal's tamper-evidence walk, folded for the manifest.
struct ChainFacts {
    status: &'static str,
    head: Option<String>,
    note: Option<String>,
}

/// The `workflow_started` fields the pack reads (all optional — older
/// journals carry fewer; absence is reported, never invented).
#[derive(Default)]
struct StartedFacts {
    workflow: Option<String>,
    engine_version: Option<String>,
    platform: Option<String>,
    workflow_sha256: Option<String>,
    semantic_hash: Option<String>,
    sandbox: Option<String>,
    permits_json: Option<String>,
    inputs_json: Option<String>,
    resumed_from_engine: Option<String>,
    resume_compat: Option<String>,
}

/// The seal grade — `verifies: None` is the key-unavailable case
/// (NEVER silent: `reason` then says why).
struct SealFacts {
    present: bool,
    key_id: Option<String>,
    alg: Option<String>,
    covers: Option<Value>,
    covers_chain: Option<bool>,
    verifies: Option<bool>,
    reason: Option<String>,
}

/// The hash anchors a `--workflow` re-read must match: the semantic
/// hash (the seal's signed `covers.workflow` first — attributable —
/// then the journaled field) and the source-bytes sha256.
struct Anchors<'a> {
    semantic: Option<String>,
    source_sha256: Option<&'a str>,
}

/// The `--workflow` arm's three outcomes: the file PROVED it is the
/// one the journal records, it was refused at the hash check, or no
/// file was given.
enum FileArm {
    /// Hash-checked: the file IS this run's workflow — its boundary,
    /// trifecta verdict, certificate and asserts are this run's own.
    /// Boxed: the checked workflow dwarfs the other variants.
    Valid(Box<ValidFile>),
    /// A file was given but failed the hash check (or the journal
    /// carries no hash to check against) — the reason, for
    /// `unavailable`. The pack exports without the file's claims.
    Refused(String),
    /// No `--workflow` given.
    None,
}

/// The validated `--workflow` payload.
struct ValidFile {
    wf: nika_schema::raw::RawWorkflow,
    report: nika_check::CheckReport,
    semantic: Option<SemanticHash>,
}

/// The public fingerprint of a public-key box string — the first 16
/// hex of its sha256. Mirror of `nika-cli`'s `seal::fingerprint` (the
/// minting side): the RULE is one line of sha2, and the verifier side
/// of the custody belongs to the forensics plane.
fn fingerprint(pubkey_box: &str) -> String {
    sha256_hex(pubkey_box.as_bytes())[..16].to_owned()
}

/// Add one public-key box to the enrolment set, deduped by
/// fingerprint — a key enrolled in two places is one candidate.
fn push_unique_pubkey(out: &mut Vec<(String, String)>, pk_box: &str) {
    let pk_box = pk_box.trim();
    if pk_box.is_empty() || minisign::PublicKeyBox::from_string(pk_box).is_err() {
        return; // blank line or non-key text in a custody file — never a candidate
    }
    let fp = fingerprint(pk_box);
    if !out.iter().any(|(known, _)| known == &fp) {
        out.push((fp, pk_box.to_owned()));
    }
}

/// The public halves a verifier can try, each paired with its
/// fingerprint — the enrolment set a seal grade draws on: the CI/env
/// override (`NIKA_RUN_PUB_FILE`), the keychain public entry, the 0600
/// fallback file, and the retired-pubkeys ledger (rotation keeps old
/// journals verifiable). Pub-only — the secret half is never touched.
#[must_use]
pub fn candidate_pubkeys() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    // Env reads are CONFIG paths (the run-key custody seam — the same
    // scoped exemption as `seal.rs`'s custody reads).
    #[allow(clippy::disallowed_methods)]
    let env_pub = std::env::var("NIKA_RUN_PUB_FILE").ok();
    if let Some(pf) = env_pub {
        // seam-bypass-ok: run-key custody read — the CI enrolment file
        if let Ok(text) = std::fs::read_to_string(pf) {
            push_unique_pubkey(&mut out, &text);
        }
    }
    if let Ok(entry) = keyring::Entry::new("nika", "run-signing-key.pub")
        && let Ok(text) = entry.get_password()
    {
        push_unique_pubkey(&mut out, &text);
    }
    #[allow(clippy::disallowed_methods)]
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home {
        let keys_dir = PathBuf::from(home).join(".nika").join("keys");
        // seam-bypass-ok: run-key custody read — the 0600 fallback pub file
        if let Ok(text) = std::fs::read_to_string(keys_dir.join("run-signing.pub")) {
            push_unique_pubkey(&mut out, &text);
        }
        // seam-bypass-ok: run-key custody read — the retired-pubs ledger
        if let Ok(text) = std::fs::read_to_string(keys_dir.join("retired.pub")) {
            for line in text.lines() {
                push_unique_pubkey(&mut out, line);
            }
        }
    }
    out
}

/// Build the REDACTED pack (the default class · T9): journal.ndjson
/// is the projection — payload fields ride as sha256 placeholders,
/// structural fields verbatim — while the manifest keeps attesting
/// the ORIGINAL journal (`journal_sha256` · chain · head · seal). The
/// pack proves the run's INTEGRITY, not its CONTENT; disclosure is a
/// separate, operator-side gesture VERIFY.md teaches.
///
/// `keys` is the enrolled-key set the seal grades against —
/// [`candidate_pubkeys`] for the machine's own custody, a crafted set
/// for hermetic tests.
///
/// # Errors
///
/// [`PackError`] — the journal is unreadable or is not one, the named
/// workflow cannot be used, or the manifest cannot be assembled.
pub fn build(
    trace: &Path,
    workflow: Option<&Path>,
    keys: &[(String, String)],
) -> Result<EvidencePack, PackError> {
    build_class(trace, workflow, keys, &Class::Redacted)
}

/// Build the FULL-CONTENT pack (the CLI's `--full`): journal.ndjson
/// is the journal's EXACT bytes — model outputs, tool results and
/// file reads in the clear — and the manifest SAYS so
/// (`redaction.class: "full"`). The opt-in shape for when the auditor
/// must see content; the default is [`build`].
///
/// # Errors
///
/// [`PackError`] — as [`build`].
pub fn build_full(
    trace: &Path,
    workflow: Option<&Path>,
    keys: &[(String, String)],
) -> Result<EvidencePack, PackError> {
    build_class(trace, workflow, keys, &Class::Full)
}

/// Build the pack over one journal: fold the chain, the started
/// fields, the seal grade and (when `workflow` is given) the
/// hash-checked file arm into the manifest + receipt + VERIFY.md —
/// then cut the journal down to the class's disclosure shape. Every
/// fact folds from the ORIGINAL bytes BEFORE the projection, so the
/// seal grade and the chain verdict describe the operator's journal.
fn build_class(
    trace: &Path,
    workflow: Option<&Path>,
    keys: &[(String, String)],
    class: &Class,
) -> Result<EvidencePack, PackError> {
    let label = trace.display().to_string();
    let raw = read_journal(&label)?;
    let chain = chain_facts(&raw, &label)?;
    let events = crate::recover::recover_events(&raw, &label)
        .map(|r| r.events)
        .map_err(|e| PackError::NotAJournal(format!("cannot parse {label}: {e}")))?;
    let started = started_facts(&events);
    let seal = seal_facts(&events, &raw, keys);
    let outcome = outcome_of(&events);
    let anchors = Anchors {
        semantic: seal
            .covers
            .as_ref()
            .and_then(|c| c.get("workflow"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| started.semantic_hash.clone()),
        source_sha256: started.workflow_sha256.as_deref(),
    };
    let file = match workflow {
        Some(path) => load_file_arm(&path.display().to_string(), &anchors)?,
        None => FileArm::None,
    };
    Ok(assemble(
        &raw,
        &chain,
        events.len(),
        &started,
        &seal,
        outcome,
        workflow.is_some(),
        &file,
        class,
    ))
}

/// Read the journal's exact bytes (UTF-8 by construction — anything
/// else is not a journal this engine wrote).
fn read_journal(label: &str) -> Result<String, PackError> {
    // seam-bypass-ok: the evidence pack reads the operator's own journal
    // · NEP-0012 law 1: the whole-file bound rides here too (the read
    // is bounded BEFORE it happens).
    if let Ok(meta) = std::fs::metadata(label)
        && meta.len() > crate::bounded::MAX_JOURNAL_BYTES as u64
    {
        return Err(PackError::UnreadableJournal(format!(
            "{label}: {} bytes — over the journal bound ({} bytes · NEP-0012 law 1)",
            meta.len(),
            crate::bounded::MAX_JOURNAL_BYTES
        )));
    }
    let bytes = std::fs::read(label)
        .map_err(|e| PackError::UnreadableJournal(format!("cannot read {label}: {e}")))?;
    String::from_utf8(bytes).map_err(|_| {
        PackError::UnreadableJournal(format!(
            "{label}: not UTF-8 — not a journal this engine wrote"
        ))
    })
}

/// Fold the chain walk into manifest facts. Empty/garbage files refuse
/// (nothing to attest); every other verdict PACKS — a broken chain is
/// exactly what an auditor needs bundled, said loudly.
fn chain_facts(raw: &str, label: &str) -> Result<ChainFacts, PackError> {
    match walk(raw) {
        Verdict::Intact { head, .. } => Ok(ChainFacts {
            status: "intact",
            head: Some(head),
            note: None,
        }),
        Verdict::Incomplete { head, .. } => Ok(ChainFacts {
            status: "incomplete",
            head: Some(head),
            note: Some(
                "the run never reached a terminal frame (killed or crashed between writes) — the chain covers every complete line"
                    .to_owned(),
            ),
        }),
        Verdict::TornTail { head, .. } => Ok(ChainFacts {
            status: "torn_tail",
            head: Some(head),
            note: Some(
                "the final line is torn (a crash mid-write) — the chain covers every complete line"
                    .to_owned(),
            ),
        }),
        Verdict::Broken { line, .. } => Ok(ChainFacts {
            status: "broken",
            head: None,
            note: Some(format!(
                "chain broken at line {line} — every line from there on is unverified (edited, inserted, dropped or reordered)"
            )),
        }),
        Verdict::Unchained => Ok(ChainFacts {
            status: "unchained",
            head: None,
            note: Some(
                "pre-chain journal (pre-0.96) — nothing to verify, nothing to distrust".to_owned(),
            ),
        }),
        Verdict::Empty => Err(PackError::NotAJournal(format!("{label}: no events"))),
        Verdict::Unreadable { line, .. } => Err(PackError::NotAJournal(format!(
            "{label}:{line}: not a journal — the line is not valid JSON"
        ))),
        // F-P1 · the fortress line bound: beyond the verifier's bounds
        // is nothing an evidence pack attests (refused, never packed).
        Verdict::LineOverLong { line, got } => Err(PackError::NotAJournal(format!(
            "{label}:{line}: line is {got} bytes — beyond the verifier's line bound"
        ))),
    }
}

/// A string field of one event, when present and string-typed.
fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| match &f.value {
            nika_types::resource::Value::String(s) => Some(s.as_str()),
            _ => None,
        })
}

/// The `workflow_started` projection — the first one wins (a journal
/// has exactly one by construction).
fn started_facts(events: &[Event]) -> StartedFacts {
    let Some(started) = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::WorkflowStarted))
    else {
        return StartedFacts::default();
    };
    let get = |key: &str| str_field(started, key).map(str::to_owned);
    StartedFacts {
        workflow: get("workflow"),
        engine_version: get("engine_version"),
        platform: get("platform"),
        workflow_sha256: get("workflow_sha256"),
        semantic_hash: get("semantic_hash"),
        sandbox: get("sandbox"),
        permits_json: get("permits_json"),
        inputs_json: get("inputs"),
        resumed_from_engine: get("resumed_from_engine"),
        resume_compat: get("resume_compat"),
    }
}

/// The chain field recorded on the `run_sealed` LINE itself — the
/// head the seal claims to cover (the seal binds itself at exactly the
/// position it seals). Read from the raw bytes, never re-serialized.
fn seal_line_chain(raw: &str) -> Option<String> {
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .find_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            if v.get("kind").and_then(Value::as_str) == Some("run_sealed") {
                v.get("chain").and_then(Value::as_str).map(str::to_owned)
            } else {
                None
            }
        })
}

/// Extract + grade the seal. Absent stays absent (`present: false` —
/// the unsigned tier, never dressed up); present grades against the
/// enrolled-key set with the key-unavailable case as `null` + reason.
fn seal_facts(events: &[Event], raw: &str, keys: &[(String, String)]) -> SealFacts {
    let Some(sealed) = events
        .iter()
        .rev()
        .find(|e| matches!(e.kind, EventKind::RunSealed))
    else {
        return SealFacts {
            present: false,
            key_id: None,
            alg: None,
            covers: None,
            covers_chain: None,
            verifies: None,
            reason: None,
        };
    };
    let key_id = str_field(sealed, "key_id").map(str::to_owned);
    let alg = str_field(sealed, "alg").map(str::to_owned);
    let covers: Option<Value> =
        str_field(sealed, "covers").and_then(|text| serde_json::from_str(text).ok());
    let sig = str_field(sealed, "sig").map(str::to_owned);
    let covers_chain = covers
        .as_ref()
        .map(|c| c.get("head").and_then(Value::as_str) == seal_line_chain(raw).as_deref());
    let (verifies, reason) = grade_seal(covers.as_ref(), sig.as_deref(), key_id.as_deref(), keys);
    SealFacts {
        present: true,
        key_id,
        alg,
        covers,
        covers_chain,
        verifies,
        reason,
    }
}

/// Grade the seal's signature against the enrolled keys. `None` =
/// ungradeable (malformed envelope, or the key not enrolled — the
/// reason always names which); `Some(false)` = the signature does NOT
/// verify: the pack says so, loudly.
fn grade_seal(
    covers: Option<&Value>,
    sig: Option<&str>,
    key_id: Option<&str>,
    keys: &[(String, String)],
) -> (Option<bool>, Option<String>) {
    let (Some(covers), Some(sig), Some(key_id)) = (covers, sig, key_id) else {
        return (
            None,
            Some(
                "the seal event is malformed (covers · sig · key_id must all be present)"
                    .to_owned(),
            ),
        );
    };
    let Some((_, pk_box)) = keys.iter().find(|(fp, _)| fp == key_id) else {
        return (
            None,
            Some(format!(
                "run key {key_id} is not enrolled on this machine — enroll the public key (VERIFY.md step 2)"
            )),
        );
    };
    (Some(verify_signature(covers, sig, pk_box)), None)
}

/// The one crypto seam: recompute the proof-layer preimage over the
/// seal's parsed `covers` and verify the detached minisign signature
/// against the enrolled public key. A malformed box is a NON-verify
/// (`false`) — it cannot be the valid signature of anything.
fn verify_signature(covers: &Value, sig: &str, pk_box: &str) -> bool {
    let (Ok(sig_box), Ok(pk_box)) = (
        minisign::SignatureBox::from_string(sig),
        minisign::PublicKeyBox::from_string(pk_box),
    ) else {
        return false;
    };
    let Ok(pk) = pk_box.into_public_key() else {
        return false;
    };
    let preimage = preimage(HashDomain::Trace, 1, covers);
    minisign::verify(
        &pk,
        &sig_box,
        Cursor::new(preimage.as_bytes()),
        true,
        false,
        false,
    )
    .is_ok()
}

/// The run's terminal outcome word (for the receipt's trace verdict) —
/// the LAST terminal workflow event wins; a journal with none is
/// `unfinished` (torn · still running · hand-cut).
fn outcome_of(events: &[Event]) -> &'static str {
    for event in events.iter().rev() {
        match event.kind {
            EventKind::WorkflowCompleted => return "completed",
            EventKind::WorkflowFailed => return "failed",
            EventKind::WorkflowCancelled => return "cancelled",
            EventKind::WorkflowPaused => return "paused",
            _ => {}
        }
    }
    "unfinished"
}

/// Read + strict-parse + ladder-check the `--workflow` file — the
/// SAME ladder the CLI's `load_checked_with_source` walks (the
/// composed lane included), kept identical so check≡pack cannot drift.
fn load_workflow(
    path: &str,
) -> Result<
    (
        String,
        nika_schema::raw::RawWorkflow,
        nika_check::CheckReport,
    ),
    PackError,
> {
    // seam-bypass-ok: the evidence pack reads the operator's own workflow
    let source = std::fs::read_to_string(path)
        .map_err(|e| PackError::Workflow(format!("cannot read {path}: {e}")))?;
    let wf = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .map_err(|e| PackError::Workflow(format!("PARSE ✗  [{}] {e}", e.spec_code())))?;
    let report = nika_check::check_composed(&wf, path, &mut |p| {
        // seam-bypass-ok: the composed lane's child-workflow reads (the check seam)
        std::fs::read_to_string(p).map_err(|e| e.to_string())
    });
    Ok((source, wf, report))
}

/// Load + hash-check the `--workflow` file. An unreadable/unparseable
/// file is an invocation error — it was NAMED; a file that parses but
/// fails the hash check is NOT this run's workflow, and the pack says
/// so through the honest-null path (never a wrong boundary).
fn load_file_arm(path: &str, anchors: &Anchors<'_>) -> Result<FileArm, PackError> {
    let (source, wf, report) = load_workflow(path)?;
    let semantic = semantic_ir_hash(&wf);
    let semantic_match = anchors
        .semantic
        .as_deref()
        .is_some_and(|j| semantic.as_ref().is_some_and(|s| s.as_hex() == j));
    let source_match = anchors
        .source_sha256
        .is_some_and(|j| sha256_hex(source.as_bytes()) == j);
    if semantic_match || source_match {
        return Ok(FileArm::Valid(Box::new(ValidFile {
            wf,
            report,
            semantic,
        })));
    }
    if anchors.semantic.is_none() && anchors.source_sha256.is_none() {
        return Ok(FileArm::Refused(
            "the journal records no workflow hash to check --workflow against".to_owned(),
        ));
    }
    Ok(FileArm::Refused(
        "the named workflow does not match the journal's recorded hashes — not this run's file"
            .to_owned(),
    ))
}

/// Assemble the manifest + receipt + VERIFY.md from every folded fact.
/// Each section names its provenance; each null names its reason in
/// `unavailable`. The class decides journal.ndjson's shape: the exact
/// bytes (full) or the redacted projection (default) — the manifest
/// attests the ORIGINAL journal either way, and says which class ships.
#[allow(clippy::too_many_arguments)]
fn assemble(
    raw: &str,
    chain: &ChainFacts,
    event_count: usize,
    started: &StartedFacts,
    seal: &SealFacts,
    outcome: &str,
    workflow_given: bool,
    file: &FileArm,
    class: &Class,
) -> EvidencePack {
    let mut unavailable: BTreeMap<String, String> = BTreeMap::new();
    if let FileArm::Refused(reason) = file {
        unavailable.insert("workflow_file".to_owned(), reason.clone());
    }
    let semantic = semantic_field(started, seal, file, &mut unavailable);
    let boundary = boundary_field(started, file, workflow_given, &mut unavailable);
    let inputs = inputs_field(started, &mut unavailable);
    let trifecta = trifecta_field(file, workflow_given, &mut unavailable);
    let sandbox = sandbox_field(started, &mut unavailable);
    let engine = engine_field(started, &mut unavailable);
    let (receipt_meta, receipt) = receipt_field(file, chain, event_count, outcome, seal.present);
    let disclosure = disclosure_of(raw, class);
    let mut trace = json!({
        "journal_sha256": sha256_hex(raw.as_bytes()),
        "events": event_count,
        "chain": chain.status,
        "head": chain.head,
        "note": chain.note,
    });
    if let Some(projection_sha256) = &disclosure.projection_sha256
        && let Some(map) = trace.as_object_mut()
    {
        map.insert("projection_sha256".to_owned(), json!(projection_sha256));
    }
    let manifest = json!({
        "evidence_format": EVIDENCE_FORMAT,
        "trace": trace,
        "seal": seal_json(seal),
        "workflow": {
            "name": started.workflow,
            "semantic_hash": semantic.0,
            "semantic_hash_source": semantic.1,
        },
        "boundary": boundary,
        "inputs": inputs,
        "trifecta": trifecta,
        "sandbox": sandbox,
        "engine": engine,
        "receipt": receipt_meta,
        "redaction": disclosure.redaction,
        "unavailable": unavailable,
        "exported_at": now_millis(),
        "exported_by": format!("nika {}", env!("CARGO_PKG_VERSION")),
    });
    EvidencePack {
        verify_md: render_verify_md(seal, class),
        manifest,
        receipt,
        journal: disclosure.journal,
    }
}

/// The class's disclosure shape: journal.ndjson's content + the
/// manifest's `redaction` section + the projection's own attestation
/// (redacted only). The corollary rides the manifest where the
/// auditor reads it: a redacted pack proves the run's INTEGRITY, not
/// its CONTENT.
struct Disclosure {
    journal: String,
    redaction: Value,
    projection_sha256: Option<String>,
}

/// The full class ships the exact bytes and SAYS it carries content;
/// the redacted class ships the projection and says WHAT it proves.
fn disclosure_of(raw: &str, class: &Class) -> Disclosure {
    match class {
        Class::Full => Disclosure {
            journal: raw.to_owned(),
            redaction: json!({
                "class": "full",
                "posture": "this pack carries the run's CONTENT verbatim (model outputs · tool results · file reads) — handle the directory as sensitively as the journal itself",
            }),
            projection_sha256: None,
        },
        Class::Redacted => {
            let cut = redact::redact_journal(raw);
            Disclosure {
                projection_sha256: Some(sha256_hex(cut.projection.as_bytes())),
                journal: cut.projection,
                redaction: json!({
                    "class": "redacted",
                    "policy": "payload fields ride as sha256 placeholders over the field's own bytes",
                    "fields": redact::PAYLOAD_KEYS,
                    "placeholders": cut.placeholders,
                    "posture": "this pack proves the run's INTEGRITY (chain · head · seal over the ORIGINAL journal the operator keeps) — not its CONTENT; disclosure is a separate, operator-side gesture (VERIFY.md)",
                }),
            }
        }
    }
}

/// The semantic-hash field: seal `covers.workflow` (signed —
/// attributable) → the journaled field → the hash-checked file → an
/// honest null. Returns (hash, source) — both null when unknown.
fn semantic_field(
    started: &StartedFacts,
    seal: &SealFacts,
    file: &FileArm,
    unavailable: &mut BTreeMap<String, String>,
) -> (Value, Value) {
    if let Some(hash) = seal
        .covers
        .as_ref()
        .and_then(|c| c.get("workflow"))
        .and_then(Value::as_str)
    {
        return (json!(hash), json!("seal"));
    }
    if let Some(hash) = &started.semantic_hash {
        return (json!(hash), json!("journal"));
    }
    if let FileArm::Valid(valid) = file
        && let Some(hash) = &valid.semantic
    {
        return (json!(hash.as_hex()), json!("file"));
    }
    unavailable.insert(
        "semantic_hash".to_owned(),
        format!("the journal records no semantic hash — {WORKFLOW_HINT}"),
    );
    (Value::Null, Value::Null)
}

/// The permits boundary: the journaled `permits_json` first (the
/// attested record), then the hash-checked file, then an honest null.
/// A journal from an engine that journals the boundary (it carries
/// `semantic_hash`) without `permits_json` records a FACT: no boundary
/// was declared.
fn boundary_field(
    started: &StartedFacts,
    file: &FileArm,
    workflow_given: bool,
    unavailable: &mut BTreeMap<String, String>,
) -> Value {
    if let Some(text) = &started.permits_json {
        if let Ok(permits) = serde_json::from_str::<Value>(text) {
            return json!({ "declared": true, "permits": permits, "source": "journal" });
        }
        unavailable.insert(
            "boundary".to_owned(),
            "the journaled permits_json is not valid JSON".to_owned(),
        );
        return Value::Null;
    }
    if started.semantic_hash.is_some() {
        // A5+ journals record the boundary iff one was declared.
        return json!({ "declared": false, "permits": Value::Null, "source": "journal" });
    }
    if let FileArm::Valid(valid) = file {
        let permits = valid
            .wf
            .permits
            .as_ref()
            .and_then(|p| serde_json::to_value(&p.value).ok());
        return json!({ "declared": permits.is_some(), "permits": permits, "source": "file" });
    }
    let reason = if workflow_given {
        "the named workflow could not be hash-checked against this journal".to_owned()
    } else {
        format!("this journal predates boundary journaling — {WORKFLOW_HINT}")
    };
    unavailable.insert("boundary".to_owned(), reason);
    Value::Null
}

/// The input origins (F-P13 · NEP-0014 law 2): the journaled `inputs`
/// map is the ONLY honest source (the origins exist at run start, never
/// derivable after the fact — the `boundary_field` posture). Absent means
/// a pre-F-P13 journal OR a workflow without inputs: both said, never
/// guessed.
fn inputs_field(started: &StartedFacts, unavailable: &mut BTreeMap<String, String>) -> Value {
    if let Some(text) = &started.inputs_json {
        if let Ok(origins) = serde_json::from_str::<Value>(text) {
            return json!({ "origins": origins, "source": "journal" });
        }
        unavailable.insert(
            "inputs".to_owned(),
            "the journaled inputs field is not valid JSON".to_owned(),
        );
        return Value::Null;
    }
    unavailable.insert(
        "inputs".to_owned(),
        "this journal predates input-origin journaling, or the workflow declares no inputs"
            .to_owned(),
    );
    Value::Null
}

/// The trifecta static verdict: computed at check time, so ONLY the
/// hash-checked file arm can speak it (re-derived NOW from the proven
/// file — `source: file` says exactly that).
fn trifecta_field(
    file: &FileArm,
    workflow_given: bool,
    unavailable: &mut BTreeMap<String, String>,
) -> Value {
    let FileArm::Valid(valid) = file else {
        let reason = if workflow_given {
            "the named workflow could not be hash-checked against this journal".to_owned()
        } else {
            format!("the static verdict is computed at check time — {WORKFLOW_HINT}")
        };
        unavailable.insert("trifecta".to_owned(), reason);
        return Value::Null;
    };
    let findings: Vec<Value> = valid
        .report
        .trifecta_findings
        .iter()
        .map(|f| {
            json!({
                "task": f.task,
                "source": f.source,
                "detail": f.detail,
            })
        })
        .collect();
    json!({
        "verdict": if findings.is_empty() { "clean" } else { "violations" },
        "findings": findings,
        "source": "file",
    })
}

/// The sandbox mode: journaled since A5 (`workflow_started.sandbox`),
/// never derivable after the fact — an old journal means an honest
/// null.
fn sandbox_field(started: &StartedFacts, unavailable: &mut BTreeMap<String, String>) -> Value {
    if let Some(mode) = &started.sandbox {
        return json!({ "mode": mode, "source": "journal" });
    }
    unavailable.insert(
        "sandbox".to_owned(),
        "this journal predates sandbox journaling — the run's confinement mode was not recorded"
            .to_owned(),
    );
    Value::Null
}

/// The engine identity from the journal header (Q11 attestation) —
/// plus the F-P21 declared compat when the run crossed versions
/// (NEP-0014 law 4 · absent on an exact resume: no crossing, no claim).
fn engine_field(started: &StartedFacts, unavailable: &mut BTreeMap<String, String>) -> Value {
    if started.engine_version.is_none() {
        unavailable.insert(
            "engine_version".to_owned(),
            "the journal header carries no engine_version field".to_owned(),
        );
    }
    let mut engine = json!({
        "version": started.engine_version,
        "platform": started.platform,
    });
    if started.resume_compat.is_some()
        && let Some(map) = engine.as_object_mut()
    {
        map.insert(
            "resumed_from_engine".to_owned(),
            json!(started.resumed_from_engine),
        );
        map.insert("resume_compat".to_owned(), json!(started.resume_compat));
    }
    engine
}

/// The receipt: foldable ONLY from the hash-checked file arm (the
/// certificate is a check-time artifact — the journal never carried
/// one). The meta rides in the manifest; the body becomes
/// `receipt.json`.
fn receipt_field(
    file: &FileArm,
    chain: &ChainFacts,
    event_count: usize,
    outcome: &str,
    sealed: bool,
) -> (Value, Option<Value>) {
    let FileArm::Valid(valid) = file else {
        return (
            json!({ "present": false, "reason": format!("the receipt's certificate is a check-time artifact — {WORKFLOW_HINT}") }),
            None,
        );
    };
    let Some(proves) = &valid.semantic else {
        return (
            json!({ "present": false, "reason": "the workflow did not project to a semantic hash" }),
            None,
        );
    };
    let trace_verdict = json!({
        "outcome": outcome,
        "chain": chain.status,
        "events": event_count,
        "head": chain.head,
        "sealed": sealed,
    });
    let receipt = build_run_receipt(
        proves,
        &valid.report.certificate,
        trace_verdict,
        LOCK_UNRECORDED,
    );
    (
        json!({ "present": true, "proves": proves.as_hex(), "file": "receipt.json" }),
        Some(receipt),
    )
}

/// The seal section of the manifest — `verifies: null` ALWAYS carries
/// its reason (key unavailable is null, never silent).
fn seal_json(seal: &SealFacts) -> Value {
    if !seal.present {
        return json!({ "present": false });
    }
    let mut obj = json!({
        "present": true,
        "key_id": seal.key_id,
        "alg": seal.alg,
        "covers": seal.covers,
        "covers_chain": seal.covers_chain,
        "verifies": seal.verifies,
    });
    if let Some(reason) = &seal.reason
        && let Some(map) = obj.as_object_mut()
    {
        map.insert("reason".to_owned(), json!(reason));
    }
    obj
}

/// The default output dir: `<trace-stem>.evidence/` beside the journal.
#[must_use]
pub fn default_out(trace: &Path) -> PathBuf {
    let stem = trace
        .file_stem()
        .map_or_else(|| "trace".to_owned(), |s| s.to_string_lossy().into_owned());
    trace
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{stem}.evidence"))
}

/// Write the bundle: journal.ndjson (the redacted projection by
/// default · the exact bytes on the full class — [`EvidencePack::journal`]
/// already holds the class's shape), the manifest, the receipt when
/// present, VERIFY.md. An existing dir refuses — evidence is never
/// clobbered in place.
///
/// # Errors
///
/// [`PackError::Write`] — the directory exists, cannot be created, or
/// a file cannot be written.
pub fn write(dir: &Path, pack: &EvidencePack) -> Result<(), PackError> {
    if dir.exists() {
        return Err(PackError::Write(format!(
            "evidence dir already exists: {} — remove it or pass --out <dir>",
            dir.display()
        )));
    }
    // seam-bypass-ok: the evidence pack writes the operator's own bundle
    std::fs::create_dir_all(dir)
        .map_err(|e| PackError::Write(format!("cannot create {}: {e}", dir.display())))?;
    let write_file = |name: &str, body: &str| {
        // seam-bypass-ok: the evidence pack writes the operator's own bundle
        std::fs::write(dir.join(name), body).map_err(|e| {
            PackError::Write(format!("cannot write {}: {e}", dir.join(name).display()))
        })
    };
    write_file("journal.ndjson", &pack.journal)?;
    write_file("pack.json", &format!("{}\n", pretty(&pack.manifest)))?;
    if let Some(receipt) = &pack.receipt {
        write_file("receipt.json", &format!("{}\n", pretty(receipt)))?;
    }
    write_file("VERIFY.md", &pack.verify_md)?;
    Ok(())
}

/// Pretty JSON for a bundle file / the `--json` projection.
fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_owned())
}

/// Wall-clock milliseconds for `exported_at` (the L4 boundary — the
/// same posture as the journal sink's own stamp).
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// VERIFY.md — the auditor's checks, per disclosure class. The full
/// pack re-walks the exact bytes (the three commands); the redacted
/// pack checks the projection's self-consistency + the manifest
/// hashes, and names the two disclosure paths — the command list
/// changes CLASS because the evidence changes class.
fn render_verify_md(seal: &SealFacts, class: &Class) -> String {
    let section = seal_section(seal);
    match class {
        Class::Full => render_verify_full(&section),
        Class::Redacted => render_verify_redacted(&section),
    }
}

/// The seal section, shared by both classes — the grade speaks the
/// same either way: the seal covers the ORIGINAL journal's head.
/// Plain, short, and honest about the unsigned case.
fn seal_section(seal: &SealFacts) -> String {
    if seal.present {
        format!(
            r"## 2 · the seal

`pack.json → seal.verifies` grades the ed25519 seal against the run keys
enrolled HERE:
- `true` — the signature over {{chain head · event count · workflow
  semantic hash · engine version}} verifies. The journal is attributable
  to run key `{}`.
- `null` — the signing key is NOT enrolled on this machine. That is not
  a failure: on the machine that ran the workflow, `nika key trust`
  prints the public key + fingerprint; put that public key where this
  machine reads run keys (`NIKA_RUN_PUB_FILE`, or `~/.nika/keys/`),
  then re-export — the grade flips to `true`.
- `false` — the signature does NOT verify. Treat the whole pack as
  forged: nothing in it is evidence.
",
            seal.key_id.as_deref().unwrap_or("unknown")
        )
    } else {
        r"## 2 · the seal — ABSENT on this journal

This journal is NOT sealed (`seal.present: false`). The chain is
tamper-EVIDENT only: it catches edits, but nothing attributes the file
to a key — anyone with write access could rewrite the whole journal and
re-chain it. `nika key init` on the machine that runs the workflows
mints the key every future run seals with.
"
        .to_owned()
    }
}

/// The trust tiers, shared by both classes.
const TIERS: &str = r"## What each tier means

- **unchained** — a pre-0.96 journal: nothing to verify, nothing to
  distrust.
- **chained** — tamper-EVIDENT: edits show. A whole-file rewrite does
  not — only the out-of-band head catches that.
- **sealed** — chained + attributable: forging the journal needs the
  run key, not just write access to the file.
- **anchored** — sealed + the head matches one you saved elsewhere.
";

/// The FULL-class text: the three commands over the exact bytes, with
/// the sensitivity warning up top — the pack carries CONTENT, and it
/// must SAY so (a silent full pack is the leak wearing a new hat).
fn render_verify_full(seal_section: &str) -> String {
    format!(
        r"# VERIFY — this evidence pack (FULL CONTENT)

This pack carries the run's CONTENT verbatim — model answers, tool
results and file reads sit in `journal.ndjson` in the clear. Handle
the directory as sensitively as the journal itself. The default class
(`nika trace evidence` without `--full`) is the redacted pack:
integrity without content.

Everything here checks offline, from this directory. Three commands:

## 1 · the journal's tamper-evidence chain

    nika trace verify journal.ndjson

Recomputes the sha256 chain over every line. `OK` = no line edited,
inserted, dropped or reordered since the run wrote it. Compare the
printed head with `pack.json → trace.head` — and, stronger, with the
`chain <head>` the run printed when it finished (CI log · scrollback):
a head you saved out-of-band is the one anchor a whole-file rewrite
cannot reproduce.

{seal_section}
## 3 · read the manifest

    cat pack.json

Every claim names its provenance (`source: journal|seal|file`). A
`null` field is an honest unknown — the matching entry in
`unavailable` says why (usually: pass `--workflow <file>` so the pack
can hash-check the workflow against the journal and re-derive the
boundary, the trifecta verdict and the receipt).

{TIERS}"
    )
}

/// The REDACTED-class text: the corollary first (INTEGRITY ≠
/// CONTENT), then the one offline check the projection supports, the
/// seal, and the two disclosure paths the operator can later walk.
fn render_verify_redacted(seal_section: &str) -> String {
    format!(
        r#"# VERIFY — this evidence pack (REDACTED)

This pack proves the run's INTEGRITY, not its CONTENT — those are two
different asks. The payload fields (model outputs · tool results ·
file contents · failure details · streamed chunks · shown prompts)
ride as sha256 placeholders; the raw journal never left the operator's
machine. What the run DID is attested here; what the run SAID is a
separate, operator-side disclosure (§3).

Everything here checks offline, from this directory.

## 1 · the projection is the bytes you were handed

    shasum -a 256 journal.ndjson

must print `pack.json → trace.projection_sha256`. Inside, every
redacted field reads

    {{"key":"output","value":{{"sha256":"…","unavailable":"…"}}}}

— the sha256 over the field's OWN bytes (the string verbatim · the
compact JSON for a non-string), the `unavailable` naming why.
Structural fields — kind · ids · timestamps · hashes · chain links ·
durations · counts · error classes — are verbatim, and a line with no
payload field is byte-identical to the operator's line.

The `chain` fields and `pack.json → trace.head` describe the ORIGINAL
journal: they do not re-walk over this projection (the placeholders
change the bytes the chain hashes — that is the construction, not a
tamper), so do not run `nika trace verify` here; it reports broken by
design. `pack.json → trace.chain` is the verdict the export computed
over the original bytes.

{seal_section}
## 3 · content disclosure — the operator's separate gesture

Integrity is covered above. CONTENT is a second ask, answered only by
the operator disclosing — and both forms check offline:

- one field — the operator shows the original value of one redacted
  field; then

      printf '%s' '<value>' | shasum -a 256

  must equal that field's `sha256` placeholder.
- the journal — the operator hands over the original journal.ndjson;
  its `shasum -a 256` must equal `pack.json → trace.journal_sha256`,
  and `nika trace verify` on THAT file re-walks intact to
  `trace.head` — the seal (§2) then covers the disclosed bytes.

A pack whose operator cannot produce a matching disclosure proves the
integrity of SOMETHING — not of what you were shown.

## 4 · read the manifest

    cat pack.json

Every claim names its provenance (`source: journal|seal|file`). A
`null` field is an honest unknown — the matching entry in
`unavailable` says why (usually: pass `--workflow <file>` so the pack
can hash-check the workflow against the journal and re-derive the
boundary, the trifecta verdict and the receipt).

{TIERS}"#
    )
}

#[cfg(test)]
mod tests;
