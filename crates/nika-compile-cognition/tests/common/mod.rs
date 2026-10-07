// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The verifier's closed choices as the judge doubles of these suites answer them (R4 A11, R6):
//! which closed choices are the verifier's, and the option that approves or refuses each. The
//! doubles script what a judge answers; they never decide what the verifier makes of it.
#![allow(dead_code)]

/// The option each verifier question offers when it approves: the whole request (`faithful`), a
/// clause or a part asked alone (`carried`), an observed run (`consistent`), the
/// extra-operation question (`only_requested`) and the task question of a part judged missing
/// (`no_task`). A closed choice offering none of them is no verifier question.
pub(crate) const APPROVALS: [&str; 5] = [
    "faithful",
    "carried",
    "consistent",
    "only_requested",
    "no_task",
];

/// Whether a closed choice offering `keys` is a verifier question.
pub(crate) fn verifier(keys: &[String]) -> bool {
    keys.iter().any(|key| APPROVALS.contains(&key.as_str()))
}

/// The approving option among `keys`.
pub(crate) fn approval(keys: &[String]) -> String {
    let approval = APPROVALS.into_iter().find(|a| keys.iter().any(|k| k == a));
    approval.unwrap_or("none").to_owned()
}

/// The refusing option among `keys`: the request `unfaithful`, a clause or a part `missing`, the
/// first task (`task-<id>`) a task question, the extra-operation question or an observed run
/// offers, else a task question's `omitted` (a candidate naming no task), else the first part
/// (`part-<k>`) an observed run offers.
pub(crate) fn refusal(keys: &[String]) -> String {
    let offered = |key: &str| keys.iter().find(|k| *k == key);
    let prefixed = |prefix: &str| keys.iter().find(|k| k.starts_with(prefix));
    let refused = (offered("unfaithful").or_else(|| offered("missing")))
        .or_else(|| prefixed("task-"))
        .or_else(|| offered("omitted"))
        .or_else(|| prefixed("part-"));
    refused.map_or_else(|| "none".to_owned(), String::clone)
}
