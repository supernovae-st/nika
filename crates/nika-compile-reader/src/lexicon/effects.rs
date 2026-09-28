// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effect words of a clause: the verbs a text names, the endpoint family a gate
//! phrase may name by its verb alone, and how a named obligation or effect joins the
//! plan (one effect per verb, two literal files are two writes, a gate naming the
//! action merges with the kindred effect that has the destination).

use super::super::objects;
use super::super::plan::{Effect, EffectPolicy, EffectVerb, Obligation, Plan};
use super::{Head, head_of};

/// The effect verbs a text names. A word the request lists as a column (`name, email e
/// city`) is a column, never the verb it spells.
#[must_use]
pub fn effect_words(lower: &str, columns: &[String]) -> Vec<EffectVerb> {
    let masked = objects::mask_columns(lower, columns);
    let lower = masked.as_str();
    let mut verbs = Vec::new();
    for word in lower.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != ' ') {
        let mut seen = false;
        for candidate in word.split_whitespace() {
            if let Some((_, Head::Effect(verb))) = head_of(candidate) {
                if !verbs.contains(verb) {
                    verbs.push(*verb);
                }
                seen = true;
            }
        }
        if !seen && lower.contains("remboursement") && !verbs.contains(&EffectVerb::Refund) {
            verbs.push(EffectVerb::Refund);
        }
    }
    if (lower.contains("write")
        || lower.contains("écri")
        || lower.contains("enregistre")
        || lower.contains("save")
        || lower.contains("scriv")
        || lower.contains("salva")
        || lower.contains("escrib")
        || lower.contains("guarda"))
        && (lower.contains("disk")
            || lower.contains("disque")
            || lower.contains("disco")
            || lower.contains("file")
            || lower.contains("fichier")
            || lower.contains("archivo")
            || lower.contains("fichero")
            || lower.contains("./"))
        && !verbs.contains(&EffectVerb::Write)
    {
        verbs.push(EffectVerb::Write);
    }
    for (needle, verb) in [
        ("remboursement", EffectVerb::Refund),
        ("refund", EffectVerb::Refund),
        ("envoi", EffectVerb::Send),
        ("sending", EffectVerb::Send),
        ("writing", EffectVerb::Write),
        ("written", EffectVerb::Write),
        ("saving", EffectVerb::Write),
        ("publishing", EffectVerb::Publish),
        ("publication", EffectVerb::Publish),
        ("paiement", EffectVerb::Pay),
        ("payment", EffectVerb::Pay),
        ("commande", EffectVerb::Order),
        ("crédit", EffectVerb::Pay),
        ("credits", EffectVerb::Pay),
        ("rimborso", EffectVerb::Refund),
        ("reembolso", EffectVerb::Refund),
        ("invio", EffectVerb::Send),
        ("envío", EffectVerb::Send),
        ("pubblicazione", EffectVerb::Publish),
        ("publicación", EffectVerb::Publish),
        ("pagamento", EffectVerb::Pay),
        ("pago", EffectVerb::Pay),
        ("pedido", EffectVerb::Order),
    ] {
        if lower.contains(needle) && !verbs.contains(&verb) {
            verbs.push(verb);
        }
    }
    verbs
}

/// The word position of the first effect word of a text (words split on whitespace and
/// apostrophes, columns masked), and the text after that word.
pub(super) fn first_effect_word(lower: &str, columns: &[String]) -> Option<(usize, String)> {
    let masked = objects::mask_columns(lower, columns);
    let mut rest = masked.as_str();
    let mut at = 0;
    while let Some(start) = rest.find(|c: char| !c.is_whitespace() && c != '\'') {
        let from = &rest[start..];
        let end = from
            .find(|c: char| c.is_whitespace() || c == '\'')
            .unwrap_or(from.len());
        let word = from[..end].trim_matches(|c: char| !c.is_alphanumeric());
        if matches!(head_of(word), Some((_, Head::Effect(_)))) {
            return Some((at, from[end..].to_owned()));
        }
        at += 1;
        rest = &from[end..];
    }
    None
}

/// Whether a negation reaches the effect its words name: `after` is the text that follows
/// the negation (a leading « ne » or « n' » skipped). At most one particle may stand between
/// them (« never post », « ne pas envoyer », « do not ever send »); a word that heads its own
/// predicate first (« don't forget to email », « n'hésite pas à envoyer ») takes the negation.
/// No effect word the reader places — a substring cue only — keeps the ban, as before.
pub(super) fn negation_reaches(after: &str, columns: &[String]) -> bool {
    let after = after
        .strip_prefix("ne ")
        .or_else(|| after.strip_prefix("n'"))
        .unwrap_or(after);
    first_effect_word(after, columns).is_none_or(|(at, _)| at <= 1)
}

/// The objects that name every write at once. A negated write head with one of them, and at
/// most a terminal qualifier after it, bans the write effect itself: « do not write anything »,
/// « n'écris rien », « ne rien écrire ». An object that restricts them (« anything about
/// salaries », « anything else ») is a content or a shape instruction, never a ban of the write.
const EVERY_WRITE: &[&str] = &[
    "anything",
    "rien",
    "any file",
    "any files",
    "aucun fichier",
    "quoi que ce soit",
];

/// The only words a ban of every write may end with.
const BAN_QUALIFIERS: &[&str] = &[
    "at all",
    "to disk",
    "anywhere",
    "to any file",
    "du tout",
    "nulle part",
    "sur le disque",
];

/// The particles a negation may carry before its verb (« never ever write », « ne jamais rien
/// écrire »).
const BAN_PARTICLES: &[&str] = &["ever", "jamais", "plus", "pas"];

/// Whether the words after a negation (`negated`) ban every write, or (not `negated`) whether a
/// clause says « write nothing »: a write head and one object of [`EVERY_WRITE`] (« nothing »
/// in the positive form), in either order (« ne rien écrire »), and nothing after them but a
/// terminal qualifier. A quoted object (« write 'nothing' ») is content, never a ban.
pub(super) fn universal_write(after: &str, negated: bool) -> bool {
    if after.contains(['"', '`', '«', '“']) || after.contains(" '") || after.starts_with('\'') {
        return false;
    }
    let words: Vec<&str> = after
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut rest = words.as_slice();
    while let Some((first, tail)) = rest.split_first()
        && BAN_PARTICLES.contains(first)
    {
        rest = tail;
    }
    let every: &[&str] = if negated { EVERY_WRITE } else { &["nothing"] };
    // The object of every write at the head of `words`: how many words it spans.
    let object = |words: &[&str]| {
        every.iter().find_map(|phrase| {
            let wanted: Vec<&str> = phrase.split(' ').collect();
            (words.len() >= wanted.len() && words[..wanted.len()] == wanted[..])
                .then_some(wanted.len())
        })
    };
    let after_both = match rest {
        [head, tail @ ..] if super::heads::writes_to_path(head) => object(tail).map(|n| &tail[n..]),
        _ if negated => object(rest).and_then(|n| match &rest[n..] {
            [head, tail @ ..] if super::heads::writes_to_path(head) => Some(tail),
            _ => None,
        }),
        _ => None,
    };
    after_both
        .is_some_and(|tail| tail.is_empty() || BAN_QUALIFIERS.contains(&tail.join(" ").as_str()))
}

/// Whether a directly negated clause bans an object of its own rather than the requested
/// effect of the same verb: it names an object after its effect word, shares no literal (URL,
/// address, path) with the request, and does not refer back to it (« it », a repeated noun).
/// « post the digest to `<url>`; never post the raw CSV » is a request and a targeted ban.
fn bans_another_object(requested: &Effect, ban: &Effect) -> bool {
    if requested.evidence == ban.evidence {
        return false;
    }
    let shared = literal_tokens(&ban.target).iter().any(|literal| {
        requested.target.contains(literal.as_str()) || requested.evidence.contains(literal.as_str())
    });
    if shared {
        return false;
    }
    let Some((_, object)) = first_effect_word(&ban.target.to_lowercase(), &[]) else {
        return false;
    };
    let object = object.trim();
    !object.is_empty()
        && !objects::refers_back(object, std::iter::once(requested.evidence.as_str()))
}

/// The literal tokens of a text: URLs, addresses and paths, trimmed of punctuation.
fn literal_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| {
            token.trim_matches(|c: char| {
                matches!(
                    c,
                    ',' | ';' | '.' | ':' | '«' | '»' | '"' | '\'' | '(' | ')'
                )
            })
        })
        .filter(|token| {
            token.contains("://")
                || (token.contains('@') && token.contains('.'))
                || token.starts_with("./")
                || token.starts_with("../")
                || token.starts_with('/')
                || token.starts_with('~')
        })
        .map(str::to_owned)
        .collect()
}

pub(super) fn push_obligation(plan: &mut Plan, obligation: Obligation) {
    if !plan
        .obligations
        .iter()
        .any(|o| o.kind.word() == obligation.kind.word())
    {
        plan.obligations.push(obligation);
    }
}

/// The endpoint family: send, publish and notify all reach a stated destination. A gate
/// phrase that names the action by a verb word alone ("ask me before sending", "don't
/// publish until I approve") gates the stated effect of the family ("post it to `<url>`"),
/// never a phantom effect of its own verb with no destination.
const ENDPOINT_FAMILY: [EffectVerb; 3] =
    [EffectVerb::Send, EffectVerb::Publish, EffectVerb::Notify];

pub(crate) fn kindred(a: EffectVerb, b: EffectVerb) -> bool {
    a == b || (ENDPOINT_FAMILY.contains(&a) && ENDPOINT_FAMILY.contains(&b))
}

/// An effect a gate phrase named by its verb word alone: gated, with no destination.
fn names_only_the_action(effect: &Effect) -> bool {
    effect.policy == EffectPolicy::HumanFirst && !objects::has_literal(&effect.target)
}

pub(super) fn push_effect(plan: &mut Plan, effect: Effect) {
    // Two writes to two literal files are two effects; anything else merges by verb, and a
    // gate naming the action by a verb word merges with the kindred effect that has the
    // destination.
    let other_file = |e: &Effect| {
        effect.verb == EffectVerb::Write
            && objects::has_literal(&effect.target)
            && objects::has_literal(&e.target)
            && e.target != effect.target
    };
    let gate_of_kin = |e: &Effect| {
        kindred(e.verb, effect.verb) && (names_only_the_action(e) || names_only_the_action(&effect))
    };
    // A request and a ban of another object are two effects, never a verb-merged conflict.
    let requested =
        |p: EffectPolicy| matches!(p, EffectPolicy::Automatic | EffectPolicy::HumanFirst);
    let targeted_ban = |e: &Effect| {
        (requested(e.policy)
            && effect.policy == EffectPolicy::Forbidden
            && bans_another_object(e, &effect))
            || (e.policy == EffectPolicy::Forbidden
                && requested(effect.policy)
                && bans_another_object(&effect, e))
    };
    if let Some(existing) = plan
        .effects
        .iter_mut()
        .find(|e| ((e.verb == effect.verb && !other_file(e)) || gate_of_kin(e)) && !targeted_ban(e))
    {
        match (existing.policy, effect.policy) {
            (EffectPolicy::Automatic | EffectPolicy::HumanFirst, EffectPolicy::Forbidden)
            | (EffectPolicy::Forbidden, EffectPolicy::Automatic | EffectPolicy::HumanFirst) => {
                existing.policy = EffectPolicy::Conflict;
                existing.evidence = format!("{} / {}", existing.evidence, effect.evidence);
            }
            (EffectPolicy::Automatic, EffectPolicy::HumanFirst) => {
                existing.policy = EffectPolicy::HumanFirst;
            }
            (_, EffectPolicy::Undecided) | (EffectPolicy::Undecided, _) => {
                existing.policy = EffectPolicy::Undecided;
            }
            _ => {}
        }
        if existing.policy_literal.is_none() {
            existing.policy_literal = effect.policy_literal;
        }
        if !objects::has_literal(&existing.target) && objects::has_literal(&effect.target) {
            existing.target = effect.target;
            // The stated effect owns its verb and its clause; the gate phrase that came
            // first only set its policy.
            if existing.verb != effect.verb {
                existing.verb = effect.verb;
                existing.evidence = effect.evidence;
            }
        }
    } else {
        plan.effects.push(effect);
    }
}
