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
//! - `journal.ndjson` — the exact bytes of the run journal (copied,
//!   never re-serialized — the chain hashes over those bytes).
//! - `pack.json` — the manifest: trace head + chain status · the seal
//!   grade · the workflow's semantic hash · the declared `permits:`
//!   boundary · the trifecta static verdict · the sandbox mode · the
//!   engine version · `exported_at`.
//! - `receipt.json` — the spec-15 receipt
//!   ([`nika_runtime::proof::receipt::build_run_receipt`]) when a
//!   hash-checked workflow was available (its certificate is a
//!   check-time artifact the journal never carried).
//! - `VERIFY.md` — the three commands an auditor runs, plain.
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
/// VERIFY.md body, and the journal's exact bytes for the copy.
#[derive(Debug)]
#[non_exhaustive]
pub struct EvidencePack {
    /// `pack.json`'s content (the `evidence_format: 1` envelope).
    pub manifest: Value,
    /// `receipt.json`'s content, when buildable (see the module doc).
    pub receipt: Option<Value>,
    /// `VERIFY.md`'s content.
    pub verify_md: String,
    /// The journal's exact bytes (write as `journal.ndjson` verbatim —
    /// the chain hashes over these bytes).
    pub journal: String,
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

/// Build the pack over one journal: fold the chain, the started
/// fields, the seal grade and (when `workflow` is given) the
/// hash-checked file arm into the manifest + receipt + VERIFY.md.
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
/// `unavailable`.
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
    let manifest = json!({
        "evidence_format": EVIDENCE_FORMAT,
        "trace": {
            "journal_sha256": sha256_hex(raw.as_bytes()),
            "events": event_count,
            "chain": chain.status,
            "head": chain.head,
            "note": chain.note,
        },
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
        "unavailable": unavailable,
        "exported_at": now_millis(),
        "exported_by": format!("nika {}", env!("CARGO_PKG_VERSION")),
    });
    EvidencePack {
        verify_md: render_verify_md(seal),
        manifest,
        receipt,
        journal: raw.to_owned(),
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

/// The engine identity from the journal header (Q11 attestation).
fn engine_field(started: &StartedFacts, unavailable: &mut BTreeMap<String, String>) -> Value {
    if started.engine_version.is_none() {
        unavailable.insert(
            "engine_version".to_owned(),
            "the journal header carries no engine_version field".to_owned(),
        );
    }
    json!({
        "version": started.engine_version,
        "platform": started.platform,
    })
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
    let judged: Vec<(
        nika_schema::types::AssertProperty,
        nika_schema::types::AssertLevel,
    )> = valid
        .wf
        .assert
        .iter()
        .map(|sp| {
            let property = sp.value.clone();
            let level = property.level(true); // the journal IS the trace
            (property, level)
        })
        .collect();
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
        &judged,
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

/// Write the bundle: the journal's exact bytes (copied, never
/// re-serialized), the manifest, the receipt when present, VERIFY.md.
/// An existing dir refuses — evidence is never clobbered in place.
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

/// VERIFY.md — the three commands an auditor runs, plus what each
/// trust tier means. Plain, short, and honest about the unsigned case.
fn render_verify_md(seal: &SealFacts) -> String {
    let seal_section = if seal.present {
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
    };
    format!(
        r"# VERIFY — this evidence pack

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

## What each tier means

- **unchained** — a pre-0.96 journal: nothing to verify, nothing to
  distrust.
- **chained** — tamper-EVIDENT: edits show. A whole-file rewrite does
  not — only the out-of-band head catches that.
- **sealed** — chained + attributable: forging the journal needs the
  run key, not just write access to the file.
- **anchored** — sealed + the head matches one you saved elsewhere.
"
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use nika_event::{Event, EventKind};
    use nika_types::id::EventId;
    use nika_types::resource::{KeyValue, Value as FieldValue};
    use nika_types::timestamp::Timestamp;

    use super::*;
    use crate::chain::CHAIN_GENESIS;

    /// A workflow with a declared boundary, one exec task and one
    /// statically-decidable assert — check-clean and projectable.
    const WF_YAML: &str = "nika: v1\nworkflow:\n  id: pay\npermits:\n  fs: { read: [\"./in/**\"], write: [\"./out/**\"] }\n  exec: [\"echo\"]\nassert: [\"no_secret_egress\"]\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";

    fn keypair() -> (String, minisign::SecretKey) {
        let pair =
            minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair");
        (pair.pk.to_box().expect("pk box").to_string(), pair.sk)
    }

    fn parsed_wf() -> nika_schema::raw::RawWorkflow {
        nika_schema::parse(
            WF_YAML,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn wf_semantic() -> String {
        semantic_ir_hash(&parsed_wf())
            .expect("projectable")
            .as_hex()
            .to_owned()
    }

    fn wf_permits_json() -> String {
        let permits = parsed_wf().permits.expect("fixture declares a boundary");
        serde_json::to_string(&permits.value).expect("permits serialize")
    }

    /// One journaled event with string fields.
    fn event(kind: EventKind, fields: &[(&str, &str)]) -> Event {
        let mut ev = Event::new(
            EventId::generate(),
            Timestamp::from_unix_ms(1_700_000_000_000),
            kind,
        );
        for (key, value) in fields {
            ev = ev.with_field(KeyValue::new(*key, FieldValue::String((*value).to_owned())));
        }
        ev
    }

    /// Append one event to a raw journal, continuing the chain (mirrors
    /// the sink: the `chain` field is the PREVIOUS line's sha256, the
    /// head advances over the exact written bytes).
    fn append_chained(raw: &mut String, chain: &mut String, ev: &Event) {
        let mut v = serde_json::to_value(ev).expect("event serializes");
        v.as_object_mut()
            .expect("an event is an object")
            .insert("chain".to_owned(), Value::String(chain.clone()));
        let line = serde_json::to_string(&v).expect("line serializes");
        *chain = sha256_hex(line.as_bytes());
        raw.push_str(&line);
        raw.push('\n');
    }

    fn chained(events: &[Event]) -> (String, String) {
        let mut raw = String::new();
        let mut chain = sha256_hex(CHAIN_GENESIS);
        for ev in events {
            append_chained(&mut raw, &mut chain, ev);
        }
        (raw, chain)
    }

    /// The A5+ `workflow_started` — records the boundary, the semantic
    /// hash and the sandbox mode in the journal itself.
    fn started_v2(sem: &str) -> Event {
        event(
            EventKind::WorkflowStarted,
            &[
                ("workflow", "pay"),
                ("permits", "declared boundary · default-deny"),
                ("workflow_sha256", &sha256_hex(WF_YAML.as_bytes())),
                ("engine_version", "0.105.0"),
                ("platform", "macos/aarch64"),
                ("semantic_hash", sem),
                ("sandbox", "seatbelt"),
                ("permits_json", &wf_permits_json()),
            ],
        )
    }

    /// A pre-A5 `workflow_started` — none of the evidence fields.
    fn started_v1() -> Event {
        event(
            EventKind::WorkflowStarted,
            &[
                ("workflow", "pay"),
                ("permits", "declared boundary · default-deny"),
                ("workflow_sha256", &sha256_hex(WF_YAML.as_bytes())),
                ("engine_version", "0.105.0"),
            ],
        )
    }

    fn completed() -> Event {
        event(EventKind::WorkflowCompleted, &[("workflow", "pay")])
    }

    /// A sealed journal over the A5 started event, minted with an
    /// explicit key. Returns (raw, `final_head`, fingerprint,
    /// `pubkey_box`) — the pubkey rides back so tests enroll it.
    fn sealed_journal_with(pk: &str, sk: &minisign::SecretKey) -> (String, String, String, String) {
        let sem = wf_semantic();
        let events = vec![started_v2(&sem), completed()];
        let (mut raw, mut chain) = chained(&events);
        let seal = seal_with(sk, pk, &chain, events.len(), &sem, "0.105.0");
        append_chained(&mut raw, &mut chain, &seal);
        (raw, chain.clone(), fingerprint(pk), pk.to_owned())
    }

    /// The seal-event WRITER with an explicit key (the pack's grade
    /// enrolls the matching pubkey).
    fn seal_with(
        sk: &minisign::SecretKey,
        pk: &str,
        head: &str,
        events: usize,
        workflow_hash: &str,
        engine: &str,
    ) -> Event {
        let covers = serde_json::json!({
            "head": head,
            "events": events,
            "workflow": workflow_hash,
            "engine": engine,
        });
        let preimage = preimage(HashDomain::Trace, 1, &covers);
        let sig_box = minisign::sign(None, sk, Cursor::new(preimage.as_bytes()), None, None)
            .expect("the seal signs");
        Event::new(
            EventId::generate(),
            Timestamp::from_unix_ms(1_700_000_000_100),
            EventKind::RunSealed,
        )
        .with_fields(vec![
            KeyValue::new("seal_format", FieldValue::Int(1)),
            KeyValue::new("covers", FieldValue::String(covers.to_string())),
            KeyValue::new("key_id", FieldValue::String(fingerprint(pk))),
            KeyValue::new("alg", FieldValue::String("ed25519".to_owned())),
            KeyValue::new("sig", FieldValue::String(sig_box.into_string())),
        ])
    }

    /// Unique-per-test staging under the cargo tmp root (plain `cargo
    /// test` shares one process across tests — namespacing per test).
    fn stage(test: &str, name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-evidence-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, body).expect("staged");
        path
    }

    fn manifest_of(dir: &Path) -> Value {
        let text = std::fs::read_to_string(dir.join("pack.json")).expect("pack.json readable");
        serde_json::from_str(&text).expect("pack.json parses")
    }

    /// Build + write + return the manifest (the test pipeline most
    /// cases share).
    fn pack_over(
        test: &str,
        raw: &str,
        workflow: Option<&Path>,
        keys: &[(String, String)],
    ) -> (PathBuf, Value) {
        let trace = stage(test, "run.ndjson", raw);
        let pack = build(&trace, workflow, keys).expect("the pack builds");
        let out = trace.with_extension("out");
        write(&out, &pack).expect("the pack writes");
        let manifest = manifest_of(&out);
        (out, manifest)
    }

    /// The sealed pack: journal bytes verbatim · the seal verifies
    /// against the enrolled key · journal-provenance boundary, sandbox,
    /// engine — and the receipt stays absent (no `--workflow`), said.
    #[test]
    fn sealed_pack_exports_with_journal_provenance() {
        let (pk, sk) = keypair();
        let (raw, head, fp, pk) = sealed_journal_with(&pk, &sk);
        let keys = vec![(fp.clone(), pk)];
        let (out, pack) = pack_over("sealed", &raw, None, &keys);

        // The journal copy is byte-identical — never re-serialized.
        let copied = std::fs::read_to_string(out.join("journal.ndjson")).expect("journal copied");
        assert_eq!(copied, raw, "the bundle's journal is the exact bytes");

        assert_eq!(pack["evidence_format"], json!(1));
        assert_eq!(pack["trace"]["chain"], json!("intact"));
        assert_eq!(pack["trace"]["head"], json!(head));
        assert_eq!(pack["trace"]["events"], json!(3));
        assert_eq!(
            pack["trace"]["journal_sha256"],
            json!(sha256_hex(raw.as_bytes()))
        );
        assert_eq!(pack["seal"]["present"], json!(true));
        assert_eq!(pack["seal"]["verifies"], json!(true), "{}", pack["seal"]);
        assert_eq!(pack["seal"]["key_id"], json!(fp));
        assert_eq!(pack["seal"]["alg"], json!("ed25519"));
        assert_eq!(pack["seal"]["covers_chain"], json!(true));
        assert_eq!(pack["workflow"]["semantic_hash"], json!(wf_semantic()));
        assert_eq!(pack["workflow"]["semantic_hash_source"], json!("seal"));
        assert_eq!(pack["boundary"]["declared"], json!(true));
        assert_eq!(pack["boundary"]["source"], json!("journal"));
        assert_eq!(
            pack["boundary"]["permits"]["fs"]["write"],
            json!(["./out/**"])
        );
        assert_eq!(pack["sandbox"]["mode"], json!("seatbelt"));
        assert_eq!(pack["engine"]["version"], json!("0.105.0"));
        assert_eq!(pack["receipt"]["present"], json!(false));
        assert!(
            pack["trifecta"].is_null() && !pack["unavailable"]["trifecta"].is_null(),
            "check-time verdict is an honest null without --workflow: {pack}"
        );
        assert!(!out.join("receipt.json").exists());

        let verify_md = std::fs::read_to_string(out.join("VERIFY.md")).expect("VERIFY.md");
        assert!(verify_md.contains("nika trace verify journal.ndjson"));
        assert!(verify_md.contains(&fp), "the enrolled key is named");
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// The `--workflow` arm: the file hash-matches the journal, so the
    /// receipt (certificate + asserts + trace verdict), the trifecta
    /// verdict and the file-provenance fields land.
    #[test]
    fn hash_checked_workflow_adds_receipt_and_trifecta() {
        let (pk, sk) = keypair();
        let (raw, _, fp, pk) = sealed_journal_with(&pk, &sk);
        let wf_path = stage("filearm", "wf.nika.yaml", WF_YAML);
        let keys = vec![(fp, pk)];
        let (out, pack) = pack_over("filearm", &raw, Some(&wf_path), &keys);

        assert_eq!(pack["receipt"]["present"], json!(true));
        assert_eq!(pack["receipt"]["proves"], json!(wf_semantic()));
        assert_eq!(pack["trifecta"]["verdict"], json!("clean"));
        assert_eq!(pack["trifecta"]["source"], json!("file"));
        assert_eq!(pack["boundary"]["source"], json!("journal"));

        // The receipt verifies against the workflow's semantic hash and
        // folds the real certificate + the judged assert.
        let receipt_text =
            std::fs::read_to_string(out.join("receipt.json")).expect("receipt.json written");
        let receipt: Value = serde_json::from_str(&receipt_text).expect("receipt parses");
        assert!(
            nika_runtime::proof::receipt::verify(&receipt, &wf_semantic()),
            "the receipt verifies: {receipt}"
        );
        assert_eq!(receipt["receipt_format"], json!(1));
        assert_eq!(receipt["lock_digest"], json!(LOCK_UNRECORDED));
        assert_eq!(
            receipt["assertions"][0]["assert"],
            json!("no_secret_egress")
        );
        assert_eq!(receipt["assertions"][0]["level"], json!("StaticProof"));
        assert_eq!(receipt["trace_verdict"]["outcome"], json!("completed"));
        assert_eq!(receipt["trace_verdict"]["sealed"], json!(true));
        assert!(
            receipt["certificate"].is_object(),
            "the real RunCertificate"
        );
        assert!(
            !nika_runtime::proof::receipt::verify(&receipt, "blake3:someother"),
            "a swapped proof is refused"
        );
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// F-P13 (NEP-0014 law 2) — the input origins project with journal
    /// provenance; absent on an older journal, said (never invented).
    #[test]
    fn the_pack_projects_the_input_origins() {
        let sem = wf_semantic();
        let inputs = serde_json::json!({ "count": "ci-context", "region": "file" }).to_string();
        let events = vec![
            event(
                EventKind::WorkflowStarted,
                &[
                    ("workflow", "pay"),
                    ("engine_version", "0.106.1"),
                    ("semantic_hash", sem.as_str()),
                    ("inputs", inputs.as_str()),
                ],
            ),
            completed(),
        ];
        let (raw, _) = chained(&events);
        let (out, pack) = pack_over("inputs", &raw, None, &[]);
        assert_eq!(pack["inputs"]["origins"]["count"], json!("ci-context"));
        assert_eq!(pack["inputs"]["origins"]["region"], json!("file"));
        assert_eq!(pack["inputs"]["source"], json!("journal"));
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// The honest null: a journal without the `inputs` field (pre-F-P13
    /// · or a no-input workflow) projects `null` and names why.
    #[test]
    fn absent_input_origins_are_said_not_invented() {
        let (raw, _) = chained(&[started_v1(), completed()]);
        let (out, pack) = pack_over("inputs-absent", &raw, None, &[]);
        assert!(pack["inputs"].is_null(), "{pack}");
        assert!(
            pack["unavailable"]["inputs"]
                .as_str()
                .is_some_and(|r| r.contains("predates input-origin journaling")),
            "the reason is named: {pack}"
        );
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// An UNSEALED journal packs too: `seal.present: false`, VERIFY.md
    /// says what the unsigned tier means — never a faked seal.
    #[test]
    fn unsealed_pack_is_honest_about_the_unsigned_tier() {
        let sem = wf_semantic();
        let (raw, _) = chained(&[started_v2(&sem), completed()]);
        let (out, pack) = pack_over("unsealed", &raw, None, &[]);
        assert_eq!(pack["seal"], json!({ "present": false }));
        // Journal-provenance fields still land (A5+ journal, unsealed).
        assert_eq!(pack["workflow"]["semantic_hash_source"], json!("journal"));
        assert_eq!(pack["boundary"]["declared"], json!(true));

        let verify_md = std::fs::read_to_string(out.join("VERIFY.md")).expect("VERIFY.md");
        assert!(
            verify_md.contains("NOT sealed") && verify_md.contains("tamper-EVIDENT only"),
            "the unsigned tier explained: {verify_md}"
        );
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// A pre-A5 journal (no evidence fields recorded) exports with
    /// honest nulls — every one named in `unavailable`, never guessed.
    #[test]
    fn old_journal_exports_with_honest_unavailables() {
        let (raw, _) = chained(&[started_v1(), completed()]);
        let (out, pack) = pack_over("old", &raw, None, &[]);
        assert!(pack["boundary"].is_null(), "{pack}");
        assert!(pack["sandbox"].is_null(), "{pack}");
        assert!(pack["trifecta"].is_null(), "{pack}");
        assert!(pack["workflow"]["semantic_hash"].is_null(), "{pack}");
        assert_eq!(pack["receipt"]["present"], json!(false));
        for field in ["boundary", "sandbox", "trifecta", "semantic_hash"] {
            assert!(
                pack["unavailable"][field].is_string(),
                "unavailable.{field} names the reason: {pack}"
            );
        }
        assert!(
            pack["unavailable"]["boundary"]
                .as_str()
                .expect("a reason")
                .contains("--workflow"),
            "the pointer is actionable: {pack}"
        );
        // The engine version WAS recorded — journal provenance.
        assert_eq!(pack["engine"]["version"], json!("0.105.0"));
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// A `--workflow` that is NOT the run's file (hash mismatch) never
    /// leaks its boundary/verdict into the pack — the journal's own
    /// claims still land, the mismatch is spoken.
    #[test]
    fn hash_mismatch_workflow_never_leaks_into_the_pack() {
        let (pk, sk) = keypair();
        let (raw, _, fp, pk) = sealed_journal_with(&pk, &sk);
        let other = stage(
            "mismatch",
            "other.nika.yaml",
            "nika: v1\nworkflow:\n  id: other\ntasks:\n  b:\n    exec: { command: [\"echo\", \"yo\"] }\n",
        );
        let keys = vec![(fp, pk)];
        let (out, pack) = pack_over("mismatch", &raw, Some(&other), &keys);
        // The JOURNAL-provenance boundary still lands (the journal is
        // the attested record) — the mismatch only blocks the FILE arm.
        assert_eq!(pack["boundary"]["source"], json!("journal"), "{pack}");
        assert!(pack["trifecta"].is_null(), "{pack}");
        assert_eq!(pack["receipt"]["present"], json!(false));
        assert!(
            pack["unavailable"]["workflow_file"]
                .as_str()
                .expect("a reason")
                .contains("does not match"),
            "the mismatch is spoken: {pack}"
        );
        // The seal is untouched — the journal's own evidence still packs.
        assert_eq!(pack["seal"]["verifies"], json!(true));
        assert!(!out.join("receipt.json").exists());
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// On a pre-A5 journal (boundary not journaled), the mismatched
    /// `--workflow` leaves the boundary itself an honest null.
    #[test]
    fn hash_mismatch_on_an_old_journal_nulls_the_boundary() {
        let (raw, _) = chained(&[started_v1(), completed()]);
        let other = stage(
            "mismatch-old",
            "other.nika.yaml",
            "nika: v1\nworkflow:\n  id: other\ntasks:\n  b:\n    exec: { command: [\"echo\", \"yo\"] }\n",
        );
        let (out, pack) = pack_over("mismatch-old", &raw, Some(&other), &[]);
        assert!(pack["boundary"].is_null(), "{pack}");
        assert!(
            pack["unavailable"]["workflow_file"]
                .as_str()
                .expect("a reason")
                .contains("does not match"),
            "the mismatch is spoken: {pack}"
        );
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// Key unavailable → `verifies: null` WITH the enrolment reason —
    /// never silent, never false.
    #[test]
    fn unenrolled_key_grades_null_with_reason() {
        let (pk, sk) = keypair();
        let (raw, _, _, _) = sealed_journal_with(&pk, &sk);
        // Enrolled set carries a DIFFERENT key — the seal's key_id misses.
        let (wrong_pk, _) = keypair();
        let keys = vec![(fingerprint(&wrong_pk), wrong_pk)];
        let (out, pack) = pack_over("nokey", &raw, None, &keys);
        assert_eq!(pack["seal"]["present"], json!(true));
        assert!(pack["seal"]["verifies"].is_null(), "{pack}");
        assert!(
            pack["seal"]["reason"]
                .as_str()
                .expect("a reason")
                .contains("not enrolled"),
            "key-unavailable is null with reason: {pack}"
        );
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// A seal whose signed content was edited grades `false` — the loud
    /// forgery case (the journal itself stays chain-intact: the seal is
    /// the LAST line, the one position the chain cannot self-check).
    #[test]
    fn a_tampered_seal_grades_false() {
        let (pk, sk) = keypair();
        let (raw, _, fp, pk) = sealed_journal_with(&pk, &sk);
        let tampered = raw.replace("\\\"events\\\":2", "\\\"events\\\":99");
        assert_ne!(tampered, raw, "the covers string was edited");
        let keys = vec![(fp, pk)];
        let (out, pack) = pack_over("tampered", &tampered, None, &keys);
        assert_eq!(pack["trace"]["chain"], json!("intact"), "{pack}");
        assert_eq!(pack["seal"]["verifies"], json!(false), "{pack}");
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// A mid-journal edit breaks the chain — the pack still exports and
    /// says BROKEN loudly (the evidence of tampering IS the point).
    #[test]
    fn a_broken_chain_packs_loudly() {
        let sem = wf_semantic();
        let (raw, _) = chained(&[started_v2(&sem), completed()]);
        let broken = raw.replace("\"pay\"", "\"paz\"");
        assert_ne!(broken, raw);
        let (out, pack) = pack_over("broken", &broken, None, &[]);
        assert_eq!(pack["trace"]["chain"], json!("broken"), "{pack}");
        assert!(pack["trace"]["head"].is_null(), "{pack}");
        assert!(
            pack["trace"]["note"]
                .as_str()
                .expect("a note")
                .contains("broken at line"),
            "{pack}"
        );
        let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
    }

    /// The default out dir is `<trace-stem>.evidence/` beside the
    /// journal; an existing dir refuses (evidence is never clobbered).
    #[test]
    fn default_out_dir_and_no_clobber() {
        let sem = wf_semantic();
        let (raw, _) = chained(&[started_v2(&sem), completed()]);
        let trace = stage("layout", "2026-07-20T18-57-07-ab12.ndjson", &raw);
        let expected = trace.with_file_name("2026-07-20T18-57-07-ab12.evidence");
        assert_eq!(default_out(&trace), expected);
        let pack = build(&trace, None, &[]).expect("the pack builds");
        write(&expected, &pack).expect("the pack writes");
        assert!(expected.join("pack.json").exists());
        // Second export over the same dir → the honest refusal.
        let again = write(&expected, &pack);
        let refusal = again.expect_err("an existing dir refuses");
        assert!(matches!(refusal, PackError::Write(_)), "{refusal}");
        assert!(refusal.to_string().contains("already exists"), "{refusal}");
        let _ = std::fs::remove_dir_all(trace.parent().expect("parent"));
    }

    /// The writer/reader round-trip: a seal minted by the mirror
    /// verifies through the pack's grading path — the proof layer's
    /// ONE canonicalization makes the preimage byte-equal. A
    /// placeholder box in the enrolment set grades null-with-reason.
    #[test]
    fn the_crafted_seal_verifies_through_the_pack() {
        let (pk_box, sk) = keypair();
        let (raw, _, fp, pk) = sealed_journal_with(&pk_box, &sk);
        let events = crate::recover::recover_events(&raw, "t")
            .expect("recovers")
            .events;
        let facts = seal_facts(&events, &raw, &[(fp, pk)]);
        assert_eq!(facts.verifies, Some(true));
        assert_eq!(facts.covers_chain, Some(true));

        // A placeholder (non-key) box in the enrolment set cannot
        // verify — null with the enrolment reason, never a panic.
        let sealed = events
            .iter()
            .find(|e| matches!(e.kind, EventKind::RunSealed))
            .expect("the seal event");
        let key_id = str_field(sealed, "key_id").expect("key_id").to_owned();
        let covers: Value =
            serde_json::from_str(str_field(sealed, "covers").expect("covers")).expect("json");
        let sig = str_field(sealed, "sig").expect("sig");
        let (verifies, reason) = grade_seal(
            Some(&covers),
            Some(sig),
            Some(&key_id),
            &[(key_id.clone(), "not-a-key-box".to_owned())],
        );
        assert_eq!(verifies, Some(false), "a malformed box is a non-verify");
        assert!(reason.is_none());
    }
}
