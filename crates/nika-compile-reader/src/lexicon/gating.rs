// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a gate phrase with no named effect gates: the last automatic effect, or the last
//! automatic outbound effect when the phrase names sending (« Demandez-moi confirmation
//! avant tout envoi ; rien ne doit partir sans mon accord »: the write stays automatic, the
//! second phrase restates the gate the send already carries).
use super::super::plan::{Effect, EffectPolicy, EffectVerb};
use super::{ReadState, Reading};

/// Sending words, folded: a gate phrase that names them (« avant tout envoi », « rien ne
/// doit partir », « before sending ») gates the outbound effects, never a write.
const SENDING_WORDS: &[&str] = &[
    "envoi",
    "envoyer",
    "envoyez",
    "envoyé",
    "partir",
    "expédier",
    "send",
    "sending",
    "sent",
    "post",
    "posting",
    "enviar",
    "envío",
    "envio",
    "envíe",
    "envíes",
    "invio",
    "inviare",
    "invii",
    "spedire",
    "senden",
    "versand",
    "versenden",
    "verschicken",
    "schicken",
    "enviei",
];

pub(super) fn names_sending(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|w| SENDING_WORDS.contains(&w))
}

/// A gate phrase with no named effect gates the last automatic effect; one that names
/// sending gates the last automatic outbound effect (« Demandez-moi confirmation avant tout
/// envoi ; rien ne doit partir sans mon accord »: the write stays automatic, and the second
/// phrase restates the gate the send already carries).
pub(super) fn gate_last_automatic(reading: &mut Reading, state: &mut ReadState, outbound: bool) {
    let is_outbound = |e: &Effect| {
        matches!(
            e.verb,
            EffectVerb::Send | EffectVerb::Publish | EffectVerb::Notify
        )
    };
    if let Some(last) = reading
        .plan
        .effects
        .iter_mut()
        .rev()
        .find(|e| e.policy == EffectPolicy::Automatic && (!outbound || is_outbound(e)))
    {
        last.policy = EffectPolicy::HumanFirst;
    } else if outbound
        && reading
            .plan
            .effects
            .iter()
            .any(|e| is_outbound(e) && e.policy == EffectPolicy::HumanFirst)
    {
        // The phrase restates a gate the send already carries.
    } else {
        state.final_gate = true;
    }
}
