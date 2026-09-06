// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika trace verify --json` (#1405) — the machine projection of the
//! tier ladder: one document per trace, `verify_version` 1, additive.
//! Split from `trace_verify.rs` at the 1500-line file wall.

use nika_dap::anchor::tier;

use super::{ChainHeadline, VerbOutput, VerifyOptions};

/// The machine projection's schema version (additive, like `check --json`).
pub const VERIFY_VERSION: u32 = 1;

/// Wrap a non-ladder verdict (a broken chain · an unchained or empty
/// journal · a refused read) as the one JSON document when `--json` was
/// asked; the prose rides `lines`, the class rides `tier`.
pub(super) fn finish(opts: &VerifyOptions, trace: &str, tier: &str, out: VerbOutput) -> VerbOutput {
    if !opts.json {
        return out;
    }
    let lines: Vec<&str> = out.text.lines().collect();
    let doc = serde_json::json!({
        "verify_version": VERIFY_VERSION,
        "trace": trace,
        "tier": tier,
        "exit": out.code,
        "lines": lines,
    });
    VerbOutput {
        text: format!("{doc}\n"),
        code: out.code,
    }
}

/// The ladder's JSON projection: `tier` is the attained tier (the word
/// the prose ladder prints in capitals), `exit` the class it maps to,
/// one object per leg with the facts the prose carries.
pub(super) fn ladder_doc(
    trace: &str,
    events: usize,
    head: &str,
    headline: ChainHeadline,
    report: &tier::TierReport,
    code: u8,
    lines: &[String],
    liveness: Option<&str>,
) -> String {
    let seal = match &report.seal {
        tier::SealTier::Unsealed => serde_json::json!({"tier": "unsealed"}),
        tier::SealTier::Sealed(v) => {
            serde_json::json!({"tier": "sealed", "key_id": v.key_id, "source": v.source})
        }
        tier::SealTier::Forged(reason) => serde_json::json!({"tier": "forged", "reason": reason}),
        tier::SealTier::Buried { line, trailing } => {
            serde_json::json!({"tier": "buried", "line": line, "trailing": trailing})
        }
        tier::SealTier::Unattributable(reason) => {
            serde_json::json!({"tier": "unattributable", "reason": reason})
        }
    };
    let anchor = match &report.anchor {
        tier::AnchorTier::NotPresent => serde_json::json!({"tier": "not-present"}),
        tier::AnchorTier::Required => serde_json::json!({"tier": "required"}),
        tier::AnchorTier::Anchored(a) => serde_json::json!({
            "tier": "anchored",
            "log_index": a.log_index,
            "tree_size": a.tree_size,
            "gen_time": a.gen_time,
        }),
        tier::AnchorTier::Gap(reason) => serde_json::json!({"tier": "gap", "reason": reason}),
    };
    let replay = match &report.replay {
        tier::ReplayTier::NotAsked => serde_json::json!({"tier": "not-asked"}),
        tier::ReplayTier::Replayed => serde_json::json!({"tier": "replayed"}),
        tier::ReplayTier::Diverged(reason) => {
            serde_json::json!({"tier": "diverged", "reason": reason})
        }
        tier::ReplayTier::NotAttempted(reason) => {
            serde_json::json!({"tier": "not-attempted", "reason": reason})
        }
        _ => serde_json::json!({"tier": "unknown"}),
    };
    let attained = match report.attained {
        tier::AttainedTier::Ok => "ok",
        tier::AttainedTier::Sealed => "sealed",
        tier::AttainedTier::Anchored => "anchored",
        tier::AttainedTier::Replayed => "replayed",
        _ => "unknown",
    };
    let headline = match headline {
        ChainHeadline::Intact => "intact",
        ChainHeadline::Torn => "torn",
        ChainHeadline::Incomplete => "incomplete",
    };
    serde_json::json!({
        "verify_version": VERIFY_VERSION,
        "trace": trace,
        "tier": attained,
        "exit": code,
        "chain": {"events": events, "head": head, "headline": headline, "liveness": liveness},
        "seal": seal,
        "anchor": anchor,
        "replay": replay,
        "lines": lines,
    })
    .to_string()
}
