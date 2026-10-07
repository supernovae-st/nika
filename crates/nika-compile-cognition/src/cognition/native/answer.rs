// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The answer a door settles: the candidate the compiler emitted from the seat's sketch and
//! fills, with the business questions and the gaps the seat stated. No model writes whole
//! source: the retired source door's wire representation is gone, and a schema asking for it is
//! never sent.

/// What the sketch door settles into the native conclusion: the emitted candidate, its
/// questions and its gaps.
pub(in crate::cognition) struct Answer {
    pub(in crate::cognition) candidate: String,
    pub(in crate::cognition) questions: Vec<Question>,
    pub(in crate::cognition) gaps: Vec<String>,
}

#[derive(serde::Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(in crate::cognition) struct Question {
    pub(in crate::cognition) key: String,
    pub(in crate::cognition) label: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    pub(in crate::cognition) answer_type: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    pub(in crate::cognition) why: String,
}
