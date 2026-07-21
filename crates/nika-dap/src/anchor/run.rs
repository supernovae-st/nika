// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The anchor verb's orchestration — descended from `nika-cli`'s
//! `verbs::trace_anchor` 2026-07-21 (the 15k wall: compute descends,
//! render stays). Everything but the effect composition lives here:
//! read the journal, classify the head, load the custody key, submit
//! through the injected `HttpPostDyn` seam, persist the sidecar. The
//! CLI keeps the `ReqwestHttp` composer, the `VerbOutput` envelope,
//! and the exit code for each failure class — the failure REASONS
//! travel as data (what the refusal IS), never as pre-rendered prose
//! the caller cannot re-voice.

use std::path::PathBuf;

use nika_kernel::http::HttpPostDyn;

use super::{
    AnchorSidecar, HeadRefusal, head_of, run_key_material, sidecar_path, submit, write_sidecar,
};

/// What a successful anchor produced (the CLI renders the receipt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchoredReport {
    /// The persisted sidecar.
    pub sidecar: AnchorSidecar,
    /// The notarized head (lowercase hex).
    pub head: String,
    /// Verified event-line count the head covers.
    pub events: usize,
    /// Where the sidecar landed (`<trace>.anchor.json`).
    pub path: PathBuf,
    /// The checkpoint's origin is NOT the pinned Sigstore shard (a
    /// private deployment — the receipt says so, and the ANCHORED
    /// verify tier stays out of reach there).
    pub custom_shard: bool,
}

/// Why an anchor attempt failed. The refusal is data — the CLI maps
/// each class to its exit taxonomy (`is_file` below) and prints
/// [`AnchorFailure::describe`] verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnchorFailure {
    /// The journal file cannot be read (ENV).
    Read(String),
    /// The journal itself refuses (broken and torn are the CLI's FILE
    /// class; the pre-chain/garbage classes are ENV).
    Journal(HeadRefusal),
    /// No run-signing key in custody (ENV).
    NoKey,
    /// The custody key cannot sign (ENV).
    KeyUnusable(String),
    /// Submit or the log/TSA answer's verification failed (ENV).
    Submit(String),
    /// The sidecar write failed (ENV).
    Write(String),
}

impl AnchorFailure {
    /// The house class: is this refusal the FILE's own state (broken
    /// or torn journal — the forgery-adjacent classes) or the
    /// environment's (everything else)?
    #[must_use]
    pub fn is_file(&self) -> bool {
        matches!(
            self,
            Self::Journal(HeadRefusal::Broken { .. } | HeadRefusal::TornTail { .. })
        )
    }

    /// The refusal's exact text — the same lines the verb has always
    /// printed (byte-stable for every consumer downstream of them).
    #[must_use]
    pub fn describe(&self, trace: &str) -> String {
        match self {
            Self::Read(msg) | Self::Submit(msg) | Self::Write(msg) => msg.clone(),
            Self::Journal(HeadRefusal::Broken { line }) => format!(
                "BROKEN at line {line} — refusing to anchor a journal whose chain does not verify (fix the journal, then anchor)"
            ),
            Self::Journal(HeadRefusal::TornTail { events }) => format!(
                "the final line is TORN (a crash mid-write) — refusing to anchor: the chain covers {events} events but the journal is not cleanly final"
            ),
            Self::Journal(HeadRefusal::Unchained) => format!(
                "unchained — {trace} predates the chain (pre-0.96 journal): there is no head to anchor"
            ),
            Self::Journal(HeadRefusal::Empty) => format!("{trace}: no events"),
            Self::Journal(HeadRefusal::Unreadable { line }) => format!(
                "{trace}:{line}: not a journal — the line is not valid JSON"
            ),
            Self::Journal(_) => format!(
                "{trace}: unknown verdict class — the forensics library is newer than this CLI"
            ),
            Self::NoKey => "no run-signing key on this machine — the Rekor entry is signed with it; `nika key init` mints one"
                .to_owned(),
            Self::KeyUnusable(e) => format!("the run key cannot sign: {e}"),
        }
    }
}

/// The whole anchor attempt: journal → head → custody key → submit →
/// persist. The `HttpPostDyn` seam is injected so tests drive it with
/// `MockHttp` (no network in tests; the CLI hands `ReqwestHttp`).
///
/// # Errors
///
/// The [`AnchorFailure`] class for the first refusal — anchoring fails
/// closed at every step (no partial sidecar on a failed submission).
pub async fn anchor_journal(
    http: &impl HttpPostDyn,
    trace: &str,
    rekor_url: &str,
    tsa_url: &str,
) -> Result<AnchoredReport, AnchorFailure> {
    let raw = std::fs::read_to_string(trace) // seam-bypass-ok: L4 verb reading the journal it anchors (the trace_verify idiom)
        .map_err(|e| AnchorFailure::Read(format!("cannot read {trace}: {e}")))?;
    let (head, head32, events) = head_of(&raw).map_err(AnchorFailure::Journal)?;
    let Some((sk, pk_box)) = crate::seal::load_signing_key() else {
        return Err(AnchorFailure::NoKey);
    };
    let material = run_key_material(&sk, &pk_box).map_err(AnchorFailure::KeyUnusable)?;
    let sidecar = submit(http, rekor_url, tsa_url, &head32, &material)
        .await
        .map_err(AnchorFailure::Submit)?;
    let path = sidecar_path(trace);
    write_sidecar(&path, &sidecar).map_err(AnchorFailure::Write)?;
    let custom_shard = super::rekor::parse_checkpoint_body(&sidecar.rekor.checkpoint)
        .map(|(origin, _, _)| origin != super::rekor::REKOR_ORIGIN)
        .unwrap_or(true);
    Ok(AnchoredReport {
        sidecar,
        head,
        events,
        path,
        custom_shard,
    })
}
