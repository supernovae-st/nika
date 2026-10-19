// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The person's answers to a Session's questions as the laws read them, each within its scope. A
//! Session states an answer to a question it asked (`bound_values`, `bindings`, `choice_context`):
//! the question, by key, words and role, the line that answers it, the options picked with their
//! values, and the bindings the answer made. A reading of an open name counts only when the whole
//! scope holds: the question's words quote that open name of this request (its occurrence), and a
//! binding fills that question, rests on that line, carries the role the question asked (a value
//! or an output), and either comes from an option the person picked that carries its role and
//! value, or, with no option picked, is the person's own words for that question (a free entry).
//! A binding of another question or another name (foreign), from another line or of a name the
//! request no longer states (stale), a pick no binding stands behind (withdrawn), a binding of an
//! option the person did not pick, or of another role, settles nothing. Such an answer is
//! registered once: its bound values never join the typed values (`allowed`) that read any open
//! name, and Law 2 counts them as the person's literals. A typed value with no question context,
//! as the direct compile API takes it, keeps the historical value reading (`allowed`). The line
//! the person replied with is the answer's own: where the Session located it by its citation
//! (`human_lines`) and it reads as that reply (`human_reply`), Law 1's word settlement leaves that
//! occurrence out, while every other line and every other law keep every word.

use std::collections::BTreeMap;

use serde_json::Value;

use super::Diagnostic;
use crate::lexicon;
use crate::plan::{EffectPolicy, Plan};
use nika_compile_reader::paths::PathShape;

/// The fidelity laws with the person's answers as a host states them (`answers`, by question
/// key): a question's answer the Session describes is read within its scope only (an open name
/// it settles, a literal it bound), any other answer as the typed value it is
/// ([`super::allowed_values`], the historical reading of [`super::laws_resolved`]). `covered` is
/// as there; on these doors no question binds a name and no source name was typed.
pub fn laws_answered(
    intent: &str,
    plan: &Plan,
    doc: &Value,
    (answers, covered): (&BTreeMap<String, String>, &[String]),
    world: Option<&Value>,
    out: &mut Vec<Diagnostic>,
) {
    // A question's answer is registered once, within its scope; any other is read as typed.
    let (described, typed): (BTreeMap<String, String>, BTreeMap<String, String>) = (answers.iter())
        .map(|(key, raw)| (key.clone(), raw.clone()))
        .partition(|(_, raw)| is_described(raw));
    let allowed = super::allowed_values(&typed);
    let (native, given) = scoped_answers(intent, &described);
    // The person's reply to a question is that answer's: its located line settles no other name.
    let settle = without_answer_replies(intent, &described);
    let gating = with_picked_approvals(plan, &described);
    let withdrawn = picked_withdrawals(intent, &described);
    laws_in(
        intent,
        (plan, &gating, doc),
        (&allowed, covered),
        (&native, &given, &settle),
        (&withdrawn, &[]),
        world,
        out,
    );
}

/// The laws in their order, over what the person said: the values they typed (`allowed`), the
/// literals the admitted selections cover (`covered`), what their questions' answers settle
/// (`native`: open name, reading) and bind (`given`: literals only), the words whose folders settle
/// a name (`settle`: the request without its answers' reply lines), the names a question binds
/// (`waived`) and the source names they typed (`clarified`). The approval laws read `gating`: the
/// plan with the approvals the person picked ([`with_picked_approvals`]).
pub(super) fn laws_in(
    intent: &str,
    (plan, gating, doc): (&Plan, &Plan, &Value),
    (allowed, covered): (&[String], &[String]),
    (native, given, settle): (&[(String, String)], &[String], &str),
    (waived, clarified): (&[String], &[String]),
    world: Option<&Value>,
    out: &mut Vec<Diagnostic>,
) {
    let mut literals = Vec::new();
    super::strings(doc, &mut literals);
    let origins = super::role::origins(plan);
    let (said, answered) = ((waived, clarified), (allowed, native, settle));
    super::stated_paths_in(intent, doc, said, answered, world, &origins, out);
    super::approvals(gating, doc, out);
    super::invented_gates(gating, doc, out);
    super::dropped_effects(plan, doc, out);
    super::unnamed_writes(plan, doc, out);
    super::prohibitions(plan, doc, out);
    super::raw_text_as_records(doc, out);
    super::record_scope::record_laws(doc, world, out);
    // A literal a question's answer bound is the person's, never an invention.
    let exempt: Vec<String> = allowed.iter().chain(given).cloned().collect();
    super::invented_except(intent, &literals, (&exempt, covered), out);
}

/// `intent` without the person's replies to a Session's questions (`answers`), each located by its
/// citation and read as typed ([`without_replies`]): the words Law 1's settlement and the workflow's
/// file both read, so a question's reply never names a file or settles another name.
pub(super) fn without_answer_replies(intent: &str, answers: &BTreeMap<String, String>) -> String {
    without_replies(intent, &replies(answers))
}

/// The plan the approval laws read (Laws 3 and 21): the request's, with the approvals the person
/// picked. An offer they picked themselves (`offered`, never a choice left to Nika) whose label
/// states a human approval is that approval in the words they chose, read as their own words are:
/// a gate it asks for is theirs, never invented, and one it binds to no effect holds the final
/// effects. A label that states none adds nothing. Live gauntlet, 2026-10-15: a gate the person
/// picked from five questions was refused five times as invented.
fn with_picked_approvals(plan: &Plan, described: &BTreeMap<String, String>) -> Plan {
    let mut gating = plan.clone();
    for (key, raw) in described {
        let Ok(answer) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        let context = &answer["choice_context"];
        if context["question_key"] != key.as_str() {
            continue;
        }
        let picked = (context["selected_options"].as_array().into_iter().flatten())
            .filter(|offer| offer["provenance"]["kind"] == "offered")
            .filter_map(|offer| offer["label"].as_str());
        // A free yes to a question that asked for the person's approval (« Veux-tu valider chaque
        // envoi avant qu'il parte ? » · « Oui ») states the approval the question words, as a
        // picked offer's label does: live gauntlet q02, the same consent was asked six times,
        // its gate read as invented.
        let none_picked = (context["selected_options"].as_array()).is_none_or(Vec::is_empty);
        let approved_question = (context["human_reply"].as_str())
            .filter(|reply| none_picked && affirms(reply))
            .and(context["question"].as_str());
        for label in picked.chain(approved_question) {
            let read = lexicon::read(label).plan;
            let approved = read.effects.into_iter();
            (gating.effects).extend(approved.filter(|e| e.policy == EffectPolicy::HumanFirst));
            let unbound = (read.unknowns.iter()).any(|u| u == lexicon::GATE_WITHOUT_EFFECT);
            if unbound && !(gating.unknowns.iter()).any(|u| u == lexicon::GATE_WITHOUT_EFFECT) {
                gating
                    .unknowns
                    .push(lexicon::GATE_WITHOUT_EFFECT.to_owned());
            }
        }
    }
    gating
}

/// Whether a free `reply` states the approval its `question` asks for: the question names a
/// person's approval before an effect (« Veux-tu valider chaque envoi avant qu'il parte ? ») and
/// the reply says yes to it (`affirms`). A host describes such a reply for the judges even when
/// it bound nothing; any other free reply stays its located line alone.
#[must_use]
pub fn approved_by_reply(question: &str, reply: &str) -> bool {
    if !affirms(reply) {
        return false;
    }
    // A gate the question names, bound to its effect or not (« valider chaque envoi avant qu'il
    // parte » names the approval and leaves its effect to the workflow).
    let read = lexicon::read(question).plan;
    (read.effects.iter()).any(|effect| effect.policy == EffectPolicy::HumanFirst)
        || (read.unknowns.iter()).any(|u| u == lexicon::GATE_WITHOUT_EFFECT)
}

/// Whether a free reply says yes to the question it answers: it opens with a yes and carries no
/// negation (« oui, sans relecture » is no approval).
fn affirms(reply: &str) -> bool {
    const YES: &[&str] = &[
        "oui",
        "ouais",
        "yes",
        "yeah",
        "yep",
        "ok",
        "okay",
        "d'accord",
        "daccord",
        "dac",
        "absolument",
        "absolutely",
        "exactement",
        "exactly",
        "volontiers",
        "carrément",
        "carrement",
        "sure",
        "certainement",
        "certainly",
        "parfait",
        "perfect",
    ];
    const NOT: &[&str] = &[
        "non", "no", "not", "pas", "jamais", "never", "sans", "without", "nope",
    ];
    let lower = reply.to_lowercase().replace('\u{2019}', "'");
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .collect();
    words.first().is_some_and(|first| YES.contains(first))
        && !words.iter().any(|word| NOT.contains(word))
}

/// The files an offer the person picked themselves takes back (« Copier ton profil dans le
/// workflow … profil.md n'est plus lu »): its label stands where their answer stands among their
/// words, so a later line of theirs that names the file again asks for it again
/// ([`nika_compile_reader::withdrawal`]). Live gauntlet, 2026-10-15: the person picked that
/// offer, and every workflow that no longer read the file was refused as unrealizing it.
fn picked_withdrawals(intent: &str, described: &BTreeMap<String, String>) -> Vec<String> {
    let lines: Vec<&str> = intent.split('\n').collect();
    let mut withdrawn: Vec<String> = Vec::new();
    for (key, raw) in described {
        let Ok(answer) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        let context = &answer["choice_context"];
        if context["question_key"] != key.as_str() {
            continue;
        }
        // Where the answer stands: after its located line, else after every line.
        let end = (context["human_lines"].get(1).and_then(Value::as_u64))
            .and_then(|end| usize::try_from(end).ok())
            .filter(|end| *end <= lines.len())
            .unwrap_or(lines.len());
        let picked = (context["selected_options"].as_array().into_iter().flatten())
            .filter(|offer| offer["provenance"]["kind"] == "offered")
            .filter_map(|offer| offer["label"].as_str());
        for label in picked {
            let mut said: Vec<&str> = lines[..end].to_vec();
            said.push(label);
            said.extend_from_slice(&lines[end..]);
            let said = said.join("\n");
            for path in nika_compile_reader::paths::literals(label)
                .into_iter()
                .filter_map(named)
            {
                if nika_compile_reader::withdrawal::withdrawn(&said, &path)
                    && !withdrawn.contains(&path)
                {
                    withdrawn.push(path);
                }
            }
        }
    }
    withdrawn
}

/// The text of the file, folder or pattern a path shape names.
fn named(shape: PathShape) -> Option<String> {
    match shape {
        PathShape::File(text)
        | PathShape::Directory(text)
        | PathShape::Glob(text)
        | PathShape::Placeholder(text) => Some(text),
        _ => None,
    }
}

/// Whether `raw` is a question's answer as a Session describes it (`choice_context`).
fn is_described(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw).is_ok_and(|answer| answer["choice_context"].is_object())
}

/// The person's lines that answer a Session's questions, each where the Session located it by
/// its citation (`human_lines`: its first line and the one after its last, in the request's
/// words) and as they typed it (`human_reply`).
fn replies(answers: &BTreeMap<String, String>) -> Vec<(usize, usize, String)> {
    (answers.values())
        .filter_map(|raw| serde_json::from_str::<Value>(raw).ok())
        .filter_map(|answer| {
            let context = &answer["choice_context"];
            let lines = context["human_lines"].as_array()?;
            let start = usize::try_from(lines.first()?.as_u64()?).ok()?;
            let end = usize::try_from(lines.get(1)?.as_u64()?).ok()?;
            Some((start, end, context["human_reply"].as_str()?.to_owned()))
        })
        .collect()
}

/// `intent` without the occurrences that answer a question (`replies`): each located range is left
/// out only past the request's first line, and only when its lines are that reply as typed
/// (apostrophes folded as the laws read them). Every other line keeps its words, an identical one
/// included.
fn without_replies(intent: &str, replies: &[(usize, usize, String)]) -> String {
    let fold = |line: &str| nika_compile_reader::lexicon::fold_apostrophes(line.trim());
    let lines: Vec<&str> = intent.split('\n').collect();
    let mut kept = vec![true; lines.len()];
    for (start, end, reply) in replies {
        let typed: Vec<String> = reply.split('\n').map(fold).collect();
        let located = lines.get(*start..*end).filter(|located| {
            *start >= 1
                && located.len() == typed.len()
                && located
                    .iter()
                    .zip(&typed)
                    .all(|(line, typed)| fold(line) == *typed)
        });
        if located.is_some() {
            kept[*start..*end].fill(false);
        }
    }
    (lines.into_iter().zip(kept))
        .filter_map(|(line, keep)| keep.then_some(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the person's answers to a Session's questions settle and bind, each within its scope: the
/// readings of open names of `intent` (`(open name, reading)`) and the literals they bound.
pub(super) fn scoped_answers(
    intent: &str,
    answers: &BTreeMap<String, String>,
) -> (Vec<(String, String)>, Vec<String>) {
    let (mut readings, mut given) = (Vec::new(), Vec::new());
    for (key, raw) in answers {
        let Ok(answer) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        let context = &answer["choice_context"];
        let Some(line) = context["human_citation"].as_str() else {
            continue;
        };
        if context["question_key"] != key.as_str() {
            continue;
        }
        let picked = context["selected_options"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let scope = (key.as_str(), line, context["role"].as_str());
        let bound: Vec<(&str, &str)> = (answer["bindings"].as_array().into_iter().flatten())
            .filter_map(|binding| held(binding, scope, picked))
            .collect();
        given.extend(bound.iter().map(|(_, value)| (*value).to_owned()));
        // The occurrence: the one open name of this request the question's words quote.
        let quoted =
            (context["question"].as_str()).and_then(|words| super::quoted_open_name(intent, words));
        let Some(open) = quoted else {
            continue;
        };
        for (role, value) in bound {
            let file = matches!(role, "value" | "output_path");
            if file && open.readings.iter().any(|named| named == value) {
                readings.push((open.name.clone(), value.to_owned()));
            }
        }
    }
    (readings, given)
}

/// The role and value a binding holds within its question's scope: it fills the question `key`,
/// rests on the answering `line`, carries the role the question asked (`asked`, when it says one),
/// and comes from an option the person picked that carries that role and value, or, with no
/// option picked, is the person's own words for this very question (a free entry).
fn held<'a>(
    binding: &'a Value,
    (key, line, asked): (&str, &str, Option<&str>),
    picked: &[Value],
) -> Option<(&'a str, &'a str)> {
    let provenance = &binding["provenance"];
    let (role, value) = (binding["role"].as_str()?, binding["value"].as_str()?);
    let scoped = binding["key"] == key
        && provenance["message"] == line
        && asked.is_none_or(|asked| asked == role);
    let chosen = match provenance["option"].as_str() {
        Some(option) => {
            provenance["question"] == key
                && matches!(provenance["kind"].as_str(), Some("offered" | "delegated"))
                && picked
                    .iter()
                    .any(|offer| carries(offer, option, (role, value)))
        }
        None => {
            picked.is_empty()
                && provenance["kind"] == "answered"
                && (provenance["question"].is_null() || provenance["question"] == key)
        }
    };
    (scoped && chosen).then_some((role, value))
}

/// Whether the picked option `offer` is `option` and carries `role` with `value`.
fn carries(offer: &Value, option: &str, (role, value): (&str, &str)) -> bool {
    offer["key"] == option
        && (offer["values"].as_array().into_iter().flatten()).any(|carried| {
            carried["role"] == role
                && (carried["value"].as_str())
                    .is_some_and(|carried| super::resolution::same_literal(carried, value))
        })
}
