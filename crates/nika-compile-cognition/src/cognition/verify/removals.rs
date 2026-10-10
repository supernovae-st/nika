// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The removal claims of a conversation's revision, put to its judge before the whole request
//! is: a value a proposal the person saw bound stays theirs unless the judge reads the person's
//! cited words as removing it ([`Removal::question`]). Every question and answer is recorded
//! (`decision.removals`) as the verdict's own are; a provider's calls are journaled.

use nika_compile_seats::judge::Removal;
use nika_kernel::ai::provider::ProviderInferDyn;
use serde_json::json;

use super::{Judge, Verdict, ask};
use crate::CompileOutcome;

/// Each claim asked once; returns why each one the judge did not confirm (`kept`, NONE or no
/// answer) keeps the revision from being proposed.
pub(in crate::cognition) async fn unconfirmed<P: ProviderInferDyn>(
    judge: &Judge<'_, P>,
    claims: &[Removal],
    stated: &str,
    out: &mut CompileOutcome,
) -> Vec<String> {
    let mut verdict = Verdict::default();
    let mut kept = Vec::new();
    for (k, claim) in claims.iter().enumerate() {
        let question = claim.question(k, stated);
        let answer = ask(judge, &question, "judge_removal", &mut verdict, out).await;
        if answer.as_deref() != Some("removed") {
            let (value, words) = (&claim.value, &claim.words);
            kept.push(format!(
                "`{value}` was bound by a proposal the person saw, and the judge does not read « {words} » as removing it: keep it, or ask the person."
            ));
        }
    }
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["removals"] = json!(verdict.records);
    out.provenance.decision = Some(decision);
    kept
}
