// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The unknowns a seat lists that the compiler binds itself. The seat is told never to list
//! a connection, an endpoint or a schedule's hour as unknown work, and gpt-5-mini lists them
//! anyway (« Destino y canal para 'envíame' », « Genauer Zeitpunkt von 'Montagmorgen' »,
//! « Horário exato que define 'manhã' e fuso horário »): every such unknown ended the
//! request in a catch-all clarification while the compiler already asks the endpoint of an
//! outbound effect as `const.send_endpoint` and binds a schedule's time and zone beside the
//! candidate. An unknown that only names what the compiler asks is not unresolved work.
//! Six languages, diacritics folded, matched on word prefixes.

use super::plan::{EffectPolicy, EffectVerb, Plan};

/// Words that name where an outbound effect goes: a recipient, an address, a channel, a
/// messaging system, a webhook. Prefixes of folded lowercase words.
const DESTINATION_CUES: &[&str] = &[
    "destin",
    "recipient",
    "recipien",
    "adresse",
    "address",
    "direccion",
    "endereco",
    "indirizzo",
    "canal",
    "channel",
    "kanal",
    "email",
    "e-mail",
    "mail",
    "correo",
    "courriel",
    "webhook",
    "slack",
    "teams",
    "telegram",
    "whatsapp",
    "discord",
    "sms",
    "wohin",
    "empfanger",
    "empfaenger",
];

/// Words that name the hour or the zone a schedule fires in. Prefixes of folded words, plus
/// two-word phrases matched whole.
const TIME_CUES: &[&str] = &[
    "heure",
    "horaire",
    "hora",
    "horario",
    "orario",
    "uhrzeit",
    "zeitpunkt",
    "zeitzone",
    "timezone",
    "fuseau",
    "fuso",
    "what time",
    "exact time",
    "time of day",
    "zona horaria",
];

/// The element of the plan that binds an unknown the seat listed, if any: an outbound effect
/// binds its destination (the compiler asks the endpoint as its own question), a stated
/// trigger binds its time and zone (asked beside the candidate). `None` when the unknown
/// names something else, or when nothing in the plan carries it.
#[must_use]
pub fn bound_by_the_compiler(unknown: &str, plan: &Plan) -> Option<&'static str> {
    let folded = super::hot::fold(unknown);
    let padded = format!(
        " {} ",
        folded.replace(
            |c: char| !c.is_alphanumeric() && c != '-' && c != '\'' && c != '\u{2019}',
            " "
        )
    );
    let names = |cues: &[&str]| {
        cues.iter().any(|cue| {
            if cue.contains(' ') {
                padded.contains(&format!(" {cue} "))
            } else {
                padded.split_whitespace().any(|word| word.starts_with(cue))
            }
        })
    };
    let outbound = plan.effects.iter().any(|e| {
        matches!(
            e.verb,
            EffectVerb::Send | EffectVerb::Notify | EffectVerb::Publish
        ) && !matches!(e.policy, EffectPolicy::Forbidden | EffectPolicy::Conflict)
    });
    if outbound && names(DESTINATION_CUES) {
        return Some("outbound effect, which asks where to send as its own question");
    }
    if plan.trigger.is_some() && names(TIME_CUES) {
        return Some("stated trigger, whose time and zone are bound beside the candidate");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Effect;

    fn plan_with(send: bool, trigger: bool) -> Plan {
        let mut plan = Plan::default();
        if send {
            plan.effects.push(Effect::new(
                EffectVerb::Send,
                "envoie-moi un récapitulatif",
                "envoie-moi un récapitulatif des tickets ouverts",
                EffectPolicy::Automatic,
            ));
        }
        if trigger {
            plan.trigger = Some("Chaque lundi matin".to_owned());
        }
        plan
    }

    #[test]
    fn a_destination_or_an_hour_the_compiler_binds_is_not_unresolved_work() {
        let plan = plan_with(true, true);
        for unknown in [
            "Destino y canal para 'envíame' (dirección de correo, usuario de chat, webhook, etc.)",
            "Wohin genau soll die Zusammenfassung geschickt werden (z. B. E‑Mail-Adresse, Slack‑Kanal, andere Zieladresse)?",
            "Canal de envio desejado (por exemplo: email, Slack, Microsoft Teams, outro) — não especificado no pedido",
            "Destinataire et canal d'envoi pour « envoie-moi » (adresse e-mail, webhook…)",
            "me",
        ] {
            let bound = bound_by_the_compiler(unknown, &plan);
            if unknown == "me" {
                assert_eq!(bound, None, "{unknown}");
            } else {
                assert!(
                    bound.is_some_and(|b| b.starts_with("outbound effect")),
                    "{unknown}: {bound:?}"
                );
            }
        }
        for unknown in [
            "Genauer Zeitpunkt von 'Montagmorgen' (z. B. 08:00) ist nicht spezifiziert.",
            "Horário exato que define 'manhã' (hora concreta) e fuso horário para executar o envio",
            "Heure exacte du lundi matin",
            "What time on Monday morning and in which timezone",
        ] {
            assert!(
                bound_by_the_compiler(unknown, &plan)
                    .is_some_and(|b| b.starts_with("stated trigger")),
                "{unknown}"
            );
        }
    }

    #[test]
    fn an_unknown_nothing_in_the_plan_carries_stays_unresolved() {
        // A field name is asked by no element here; a destination with no outbound effect and
        // an hour with no trigger are not bound either.
        let plan = plan_with(true, true);
        assert_eq!(
            bound_by_the_compiler(
                "Welches Feld in ./tickets.json kennzeichnet den offenen Status (z. B. \"status\" == \"open\")?",
                &plan
            ),
            None
        );
        assert_eq!(
            bound_by_the_compiler("Destino y canal para 'envíame'", &plan_with(false, true)),
            None
        );
        assert_eq!(
            bound_by_the_compiler("Heure exacte du lundi matin", &plan_with(true, false)),
            None
        );
        // A prohibited send binds no destination: nothing is sent.
        let mut forbidden = plan_with(true, false);
        forbidden.effects[0].policy = EffectPolicy::Forbidden;
        assert_eq!(
            bound_by_the_compiler("Destino y canal para 'envíame'", &forbidden),
            None
        );
    }
}
