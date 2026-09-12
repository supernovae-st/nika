// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Render the admitted model-scope hints without auditing or selecting
//! models again. The caller owns task scoping and its output-mode policy.

/// The human boot notice for an envelope override with retained model
/// selections. Machine and quiet callers pass `human: false`.
#[must_use]
pub fn notice(
    report: &nika_check::CheckReport,
    model_override: Option<&str>,
    human: bool,
) -> Option<String> {
    let model = model_override.filter(|_| human)?;
    let rows: Vec<_> = report
        .hints
        .iter()
        .filter(|hint| hint.kind == "envelope-model")
        .map(|hint| format!("  {}", hint.advice))
        .collect();
    if rows.is_empty() {
        return None;
    }
    Some(format!(
        "model override: --model `{model}` replaces this workflow's default only\n{}",
        rows.join("\n")
    ))
}
