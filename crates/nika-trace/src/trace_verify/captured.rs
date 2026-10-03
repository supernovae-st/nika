// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tiered verify over journal bytes a host already captured: the same
//! walk → tier ladder → report, and the same `--json` document, as
//! [`super::verify_with`], judged over those bytes; the journal file is never
//! reopened. The walk's verdict match lives here, moved whole from
//! `trace_verify.rs` at its 1500-line wall, so both doors run one judge.
//!
//! `trace` names the ORIGINAL journal: the name the report prints and the
//! context its owners resolve from it (the key custody, the
//! `<trace>.anchor.json` sidecar, the writer lease), never a substitute copy.

use super::{ChainHeadline, VerbOutput, Verdict, VerifyOptions, finish, short, tiered, walk};

/// The whole-journal bound (NEP-0012 law 1): a file over it is refused
/// before it is read, and a capture over it is refused the same way.
pub const JOURNAL_BOUND: usize = nika_dap::bounded::MAX_JOURNAL_BYTES;

/// Judge `raw`, the journal bytes a host captured from `trace` within
/// [`JOURNAL_BOUND`]. Keys resolve as in [`super::verify_with`]. A capture
/// over the bound is refused with the file's own words; `opts.replay` is
/// refused, because the reproduce leg reads the journal by its path.
#[must_use]
pub fn verify_captured(trace: &str, raw: &str, opts: &VerifyOptions) -> VerbOutput {
    if opts.replay.is_some() {
        return finish(
            opts,
            trace,
            "refused",
            VerbOutput::env(format!(
                "{trace}: a captured journal is judged without --replay (the reproduce leg reads the file by its path)"
            )),
        );
    }
    let candidates = match crate::seal::candidate_pubkeys(opts.key.as_deref()) {
        Ok(candidates) => candidates,
        Err(e) => return finish(opts, trace, "refused", VerbOutput::env(e)),
    };
    if raw.len() > JOURNAL_BOUND {
        return over_bound(opts, trace, raw.len() as u64);
    }
    judge(trace, raw, opts, &candidates)
}

/// The refusal of a journal over [`JOURNAL_BOUND`], read or captured.
pub(super) fn over_bound(opts: &VerifyOptions, trace: &str, bytes: u64) -> VerbOutput {
    finish(
        opts,
        trace,
        "refused",
        VerbOutput::env(format!(
            "{trace}: {bytes} bytes — over the journal bound ({JOURNAL_BOUND} bytes · NEP-0012 law 1 · a file beyond it is not a run this engine produced)"
        )),
    )
}

/// The walk's verdict over `raw`, then the tier ladder above an intact chain.
pub(super) fn judge(
    trace: &str,
    raw: &str,
    opts: &VerifyOptions,
    candidates: &[(String, String)],
) -> VerbOutput {
    match walk(raw) {
        Verdict::Intact { events, head, .. } => tiered(
            trace,
            raw,
            events,
            &head,
            ChainHeadline::Intact,
            opts,
            candidates,
        ),
        Verdict::Incomplete { events, head, .. } => tiered(
            trace,
            raw,
            events,
            &head,
            ChainHeadline::Incomplete,
            opts,
            candidates,
        ),
        Verdict::TornTail { events, head, .. } => tiered(
            trace,
            raw,
            events,
            &head,
            ChainHeadline::Torn,
            opts,
            candidates,
        ),
        Verdict::Broken {
            line,
            recorded,
            computed,
            ..
        } => finish(
            opts,
            trace,
            "broken",
            VerbOutput::file(format!(
                "BROKEN at line {line} — recorded chain {} · computed {}\n  every line from here on is unverified (edited, inserted, dropped or reordered)",
                short(&recorded),
                short(&computed),
            )),
        ),
        // F-P1 · the fortress line bound: beyond the verifier's bounds
        // is a FILE refusal (a 100 MB line is a DoS vector, never a
        // journal line — recognized, never partially read).
        Verdict::LineOverLong { line, got, .. } => finish(
            opts,
            trace,
            "line-over-long",
            VerbOutput::file(format!(
                "line {line} is {got} bytes — beyond the verifier's line bound ({} bytes)\n  a journal line is small (the seal's covers included); an oversized line is\n  the DoS class, refused before any parse (F-P1)",
                nika_dap::chain::MAX_LINE_BYTES,
            )),
        ),
        Verdict::Unchained => finish(
            opts,
            trace,
            "unchained",
            VerbOutput::env(format!(
                "unchained — {trace} predates the chain (pre-0.96 journal): nothing to verify, nothing to distrust"
            )),
        ),
        Verdict::Empty => finish(
            opts,
            trace,
            "empty",
            VerbOutput::env(format!("{trace}: no events")),
        ),
        Verdict::Unreadable { line, .. } => finish(
            opts,
            trace,
            "unreadable",
            VerbOutput::env(format!(
                "{trace}:{line}: not a journal — the line is not valid JSON"
            )),
        ),
        // The verdict is #[non_exhaustive]: a NEWER forensics crate may
        // learn classes this CLI cannot render — refuse honestly,
        // never mis-render one.
        _ => finish(
            opts,
            trace,
            "unknown",
            VerbOutput::env(format!(
                "{trace}: unknown verdict class — the forensics library is newer than this CLI"
            )),
        ),
    }
}
