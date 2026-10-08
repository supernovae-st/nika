// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Explicit indecision about an effect: a clause that says an effect is not decided yet
//! (« je n'ai pas encore décidé si … », « not decided whether … »).
use super::super::plan::{Effect, EffectPolicy, EffectVerb};
use super::cues::UNDECIDED_MARKERS;
use super::effects::{effect_words, push_effect};
use super::{ReadState, Reading, earliest, prefix_before, read_prefix};

/// Whether `text` declares an effect undecided: whatever precedes the marker is read first,
/// then the effect the marker names is recorded as undecided. `false` when it declares none.
pub(super) fn declared(
    text: &str,
    clause: &str,
    reading: &mut Reading,
    state: &mut ReadState,
) -> bool {
    let Some((pos, marker)) = earliest(text, UNDECIDED_MARKERS) else {
        return false;
    };
    read_prefix(prefix_before(text, pos), clause, reading, state);
    let target = text
        .get(pos + marker.len()..)
        .unwrap_or_default()
        .split([';', '.'])
        .next()
        .unwrap_or_default()
        .trim();
    let verb = effect_words(target, &reading.columns)
        .first()
        .copied()
        .unwrap_or(EffectVerb::Other);
    push_effect(
        &mut reading.plan,
        Effect::new(verb, target, clause, EffectPolicy::Undecided),
    );
    true
}
