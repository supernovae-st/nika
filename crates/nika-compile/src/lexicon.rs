// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
// LOC-EXEMPT: lookup-table [compile · the deterministic reader is FROZEN 2026-09-21 (HOT is a safety floor, every new law lives in the typed plan) · its head, cue and marker tables are the bulk · split scheduled with the season-2 debt wave]

//! Deterministic reading of a free intent (the HOT frontend and the policy backstop).
//!
//! The reader consumes whole clauses through a small EN/FR head-verb lexicon and
//! recognizes explicit policy sentences (prohibition, indecision, human gate,
//! deduplication, revision recheck, attempt bounds). It is not fuzzy retrieval:
//! a clause whose head the lexicon does not know stays UNRESOLVED and blocks the
//! zero-call path; a clause whose head names a small finite set of operations is
//! AMBIGUOUS and may be settled by a bounded decision seat. Nothing here invents
//! an operation, an effect or a policy; every element keeps its verbatim clause.

mod cues;
mod heads;
mod slugs;

use super::paths::Structured;
use super::plan::{
    Binding, Effect, EffectPolicy, EffectVerb, Obligation, ObligationKind, Op, Plan, Step,
};
use super::{gates, objects};
use cues::{
    ARTICLES, ATTEMPT_NOUNS, BOUND_WORDS, CATEGORY_MARKERS, CONSTRAINT_OPENERS, FINAL_GATE_MARKERS,
    FORBIDDEN_MARKERS, LEADING_FILLER, LOOKUP_CUES, NAMED_GATE_MARKERS, NEGATION_OPENERS,
    NUMBER_WORDS, OBJECT_CONNECTORS, READ_CUES, REVISION_MARKERS, SEARCH_CUES, SECOND_WORD_FILLERS,
    STOP_MARKERS, STRONG_CONNECTORS, TRIGGER_PREFIXES, UNDECIDED_MARKERS, WEAK_CONNECTORS,
};
pub(crate) use heads::Head;
pub(super) use slugs::slug;

/// One clause the lexicon could not settle alone: a small feasible set, never a guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Ambiguity {
    pub clause: String,
    pub detail: String,
    pub options: Vec<Op>,
}

/// The deterministic reading of one intent.
#[derive(Clone, Debug, Default)]
pub(super) struct Reading {
    pub plan: Plan,
    pub ambiguous: Vec<Ambiguity>,
    pub unresolved: Vec<String>,
    /// Number of clauses the reader saw (for provenance).
    pub clauses: usize,
    /// Every clause the reader saw, verbatim; HOT must account for each of them.
    pub seen: Vec<String>,
    /// Clauses kept as constraints only by the declarative heuristic: prose the reader could
    /// not parse. They never make a request HOT.
    pub soft_constraints: Vec<String>,
    /// What a settled path deferred: the rest of its clause, to be read as a clause of its own.
    pub pending: Vec<String>,
    /// Clauses whose whole meaning is a policy on an effect read elsewhere (`a human must
    /// approve the write first`): accounted for by the policy they set.
    pub policy_clauses: Vec<String>,
    /// The columns the request lists: a word among them names a column, never an effect.
    pub columns: Vec<String>,
}

impl Reading {
    /// Why this reading may NOT be admitted as HOT under the strict contract: every clause
    /// consumed is not evidence of understanding. A step is explicit when its object is a
    /// typed literal or a short noun phrase without coordinated residue; an effect when its
    /// target is short or literal; and nothing ambiguous, unresolved or unknown remains.
    pub(super) fn hot_rejections(&self) -> Vec<String> {
        let mut why = Vec::new();
        if !self.unresolved.is_empty() {
            why.push(format!("{} unresolved clause(s)", self.unresolved.len()));
        }
        if !self.ambiguous.is_empty() {
            why.push(format!("{} ambiguous clause(s)", self.ambiguous.len()));
        }
        if !self.plan.unknowns.is_empty() {
            why.push("unknown requested work".to_owned());
        }
        if self.plan.steps.is_empty() && self.plan.effects.is_empty() {
            why.push("nothing recognized".to_owned());
        }
        for step in &self.plan.steps {
            let categorical = step.op == Op::Classify && !step.categories.is_empty();
            if !categorical && !explicit_object(&step.detail) {
                why.push(format!(
                    "`{}` object is not explicit: {}",
                    step.op.word(),
                    step.detail.trim()
                ));
            }
        }
        for effect in &self.plan.effects {
            if matches!(
                effect.policy,
                EffectPolicy::Automatic | EffectPolicy::HumanFirst
            ) && !explicit_object(&effect.target)
            {
                why.push(format!(
                    "`{}` target is not explicit: {}",
                    effect.verb.word(),
                    effect.target.trim()
                ));
            }
        }
        if !self.soft_constraints.is_empty() {
            why.push(format!(
                "{} prose clause(s) the reader cannot parse",
                self.soft_constraints.len()
            ));
        }
        // A constraint needs an operation to carry it; reads and writes carry nothing.
        if !self.plan.constraints.is_empty()
            && !self.plan.steps.iter().any(|s| s.op.carries_constraints())
        {
            why.push(format!(
                "{} constraint(s) with no operation to carry them",
                self.plan.constraints.len()
            ));
        }
        // Accounting: a clause the reader saw must be the evidence of something it produced.
        for clause in &self.seen {
            // An element read from the prefix of a clause accounts for the clause: the
            // rest of the clause was its policy (`publish it to ./x.md only after my approval`).
            let within = |evidence: &str| {
                !evidence.trim().is_empty()
                    && (evidence.contains(clause.as_str()) || clause.contains(evidence))
            };
            let accounted = self.plan.steps.iter().any(|s| within(&s.evidence))
                || self.plan.effects.iter().any(|e| within(&e.evidence))
                || self.plan.obligations.iter().any(|o| within(&o.evidence))
                || self.policy_clauses.iter().any(|c| within(c))
                || self
                    .plan
                    .constraints
                    .iter()
                    .any(|c| c == clause || c.contains(clause.as_str()))
                || self.unresolved.contains(clause)
                || self.ambiguous.iter().any(|a| a.clause == *clause)
                || self
                    .plan
                    .trigger
                    .as_deref()
                    .is_some_and(|t| clause.to_lowercase().starts_with(t));
            if !accounted {
                why.push(format!("unaccounted clause: {clause}"));
            }
        }
        why
    }

    /// HOT is possible only when every clause was consumed and something was asked.
    pub(super) fn complete(&self) -> bool {
        self.unresolved.is_empty()
            && self.ambiguous.is_empty()
            && (!self.plan.steps.is_empty() || !self.plan.effects.is_empty())
    }
}

/// Typographic apostrophes fold to `'` so byte offsets stay aligned between the
/// lowercase matching copy and the evidence copy. Callers anchor against this form.
pub(super) fn fold_apostrophes(intent: &str) -> String {
    intent.replace(['’', '‘'], "'")
}

/// What a clause still says after its settling path re-enters the reader as a clause of its
/// own (`… and keep the rows that matter`): it never vanishes with the path.
fn defer_residue(detail: &str, path: &str, reading: &mut Reading) {
    let residue = objects::residue_after_path(detail, path);
    let clause = objects::as_clause(&residue);
    if !clause.is_empty() {
        reading.pending.push(clause.to_owned());
    }
}

/// The object a write names before its destination (`write a 3-bullet summary to ./out/x.md`)
/// either refers back to produced content or names new content the write demands. New
/// prose content is a draft of that object; new data content is a computation the reader
/// has no operation for, so the clause stays unresolved rather than becoming a copy.
fn written_object(
    detail: &str,
    detail_lower: &str,
    path: &str,
    original: &str,
    reading: &mut Reading,
) {
    if detail.len() != detail_lower.len() {
        return;
    }
    let Some(path_at) = detail.find(path) else {
        return;
    };
    let Some(pos) = objects::destination_at(detail_lower, path_at) else {
        return;
    };
    let (Some(object), Some(object_lower)) = (detail.get(..pos), detail_lower.get(..pos)) else {
        return;
    };
    let refers_back = {
        let earlier = reading
            .seen
            .iter()
            .rev()
            .skip(1)
            .map(String::as_str)
            .chain(reading.plan.steps.iter().map(|s| s.detail.as_str()));
        objects::refers_back(object_lower, earlier)
    };
    if refers_back {
        return;
    }
    if Structured::of(path).is_some() {
        reading.unresolved.push(original.to_owned());
    } else {
        reading.plan.push_step(Step {
            op: Op::Draft,
            evidence: original.to_owned(),
            detail: object.trim().to_owned(),
            categories: Vec::new(),
        });
    }
}

/// The earlier of a listed marker and a structural one, as a byte span.
fn nearest(
    listed: Option<(usize, usize)>,
    shaped: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    match (listed, shaped) {
        (Some(a), Some(b)) => Some(if b.0 < a.0 { b } else { a }),
        (a, b) => a.or(b),
    }
}

/// A step object the reader may trust without a model: a typed literal (URL, path, email,
/// timezone, number) or at most four content tokens with no coordinating connector.
fn explicit_object(detail: &str) -> bool {
    let lower = normalize(detail);
    let literal = lower.split_whitespace().any(|w| {
        let w = w.trim_end_matches(['.', ',', ';', ')', ':']);
        w.starts_with("http://")
            || w.starts_with("https://")
            || w.starts_with("./")
            || (w.starts_with('/') && w.contains('.'))
            || (w.contains('@') && w.contains('.'))
            || w.starts_with("europe/")
            || w.starts_with("america/")
            || w.starts_with("asia/")
            || w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty()
    });
    let coordinated = OBJECT_CONNECTORS.iter().any(|c| lower.contains(c));
    let content = lower
        .split(|c: char| {
            !c.is_alphanumeric()
                && c != '\''
                && c != '/'
                && c != '.'
                && c != ':'
                && c != '-'
                && c != '_'
        })
        .filter(|t| !t.is_empty() && !ARTICLES.contains(t))
        .count();
    // A literal names the object only when nothing is coordinated beside it: "./orders.csv
    // and keep the rows that matter" carries a second request the literal does not cover.
    if literal {
        return !coordinated && content <= 6;
    }
    !coordinated && content <= 4
}

fn normalize(text: &str) -> String {
    fold_apostrophes(text).to_lowercase()
}

/// The original casing of the remainder after `consumed` lowercase bytes, when safe.
fn remainder<'a>(original: &'a str, lower: &'a str, consumed: usize) -> &'a str {
    if original.len() == lower.len() && original.is_char_boundary(consumed) {
        original.get(consumed..).unwrap_or_default().trim()
    } else {
        lower.get(consumed..).unwrap_or_default().trim()
    }
}

fn split_sentences(intent: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = intent.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        let terminal = match byte {
            b'.' | b'!' | b'?' => bytes.get(index + 1).is_none_or(u8::is_ascii_whitespace),
            b';' | b'\n' => true,
            _ => false,
        };
        if terminal {
            if let Some(sentence) = intent.get(start..index) {
                let sentence = sentence.trim();
                if !sentence.is_empty() {
                    out.push(sentence);
                }
            }
            start = index + 1;
        }
    }
    if let Some(tail) = intent.get(start..) {
        let tail = tail.trim().trim_end_matches(['.', '!', '?']);
        if !tail.is_empty() {
            out.push(tail);
        }
    }
    out
}

fn head_of(lower: &str) -> Option<(&'static str, &'static Head)> {
    if let Some(found) = head_of_exact(lower) {
        return Some(found);
    }
    // "passe ensuite la commande" / "crée alors le compte": one filler after the first word.
    let mut words = lower.splitn(3, ' ');
    let (first, second, rest) = (words.next(), words.next(), words.next());
    if let (Some(first), Some(second), Some(rest)) = (first, second, rest)
        && SECOND_WORD_FILLERS.contains(&second)
    {
        return head_of_exact(&format!("{first} {rest}"));
    }
    None
}

pub(super) fn head_of_exact(lower: &str) -> Option<(&'static str, &'static Head)> {
    let mut best: Option<(&'static str, &'static Head)> = None;
    for (phrase, head) in heads::TABLES.iter().flat_map(|table| table.iter()) {
        if lower.starts_with(phrase) {
            let boundary = lower.get(phrase.len()..).is_none_or(|rest| {
                rest.is_empty() || rest.starts_with(|c: char| !c.is_alphanumeric())
            });
            if boundary && best.is_none_or(|(b, _)| phrase.len() > b.len()) {
                best = Some((phrase, head));
            }
        }
    }
    best
}

fn strip_filler(lower: &str) -> &str {
    let mut text = lower.trim();
    loop {
        let mut changed = false;
        for filler in LEADING_FILLER {
            if let Some(rest) = text.strip_prefix(filler) {
                text = rest.trim_start();
                changed = true;
            }
        }
        if !changed {
            return text;
        }
    }
}

/// Split one sentence body into clauses at connectors followed by a known head.
fn split_clauses(sentence: &str) -> Vec<&str> {
    // A sequencing connector always opens a new clause (an unknown verb after
    // `puis` must stay visible, never be swallowed as the previous object);
    // a coordinating comma or `et`/`and` opens one only before a known head.
    let lower = normalize(sentence);
    if lower.len() != sentence.len() {
        return vec![sentence];
    }
    let mut cuts = Vec::new();
    for (connectors, always) in [(STRONG_CONNECTORS, true), (WEAK_CONNECTORS, false)] {
        for connector in connectors {
            let mut from = 0;
            while let Some(pos) = lower.get(from..).and_then(|s| s.find(connector)) {
                let at = from + pos;
                let after = at + connector.len();
                let rest = lower.get(after..).unwrap_or_default();
                if always || head_of(strip_filler(rest)).is_some() {
                    cuts.push((at, after));
                }
                from = after;
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup_by_key(|c| c.0);
    let mut out = Vec::new();
    let mut start = 0;
    for (at, after) in cuts {
        if at > start
            && let Some(piece) = sentence.get(start..at)
        {
            out.push(piece.trim());
        }
        start = after;
    }
    if let Some(piece) = sentence.get(start..) {
        out.push(piece.trim());
    }
    out.into_iter().filter(|p| !p.is_empty()).collect()
}

fn retry_bound(lower: &str) -> Option<u32> {
    if !BOUND_WORDS.iter().any(|w| lower.contains(w)) {
        return None;
    }
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect();
    for (index, word) in words.iter().enumerate() {
        let number = word.parse::<u32>().ok().or_else(|| {
            NUMBER_WORDS
                .iter()
                .find(|(w, _)| w == word)
                .map(|(_, n)| *n)
        });
        let Some(number) = number else { continue };
        let next = words.get(index + 1).copied().unwrap_or_default();
        let next2 = words.get(index + 2).copied().unwrap_or_default();
        let prev = index
            .checked_sub(2)
            .and_then(|i| words.get(i))
            .copied()
            .unwrap_or_default();
        let prev2 = index
            .checked_sub(3)
            .and_then(|i| words.get(i))
            .copied()
            .unwrap_or_default();
        if ATTEMPT_NOUNS.iter().any(|n| {
            next.starts_with(n)
                || next2.starts_with(n)
                || prev.starts_with(n)
                || prev2.starts_with(n)
        }) {
            return Some(number);
        }
    }
    None
}

fn money_literal(sentence: &str) -> bool {
    let lower = normalize(sentence);
    let has_amount = lower
        .split(|c: char| !c.is_alphanumeric() && c != '€' && c != '$')
        .any(|w| w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty())
        && ["€", "eur", "euro", "usd", "$", "dollar"]
            .iter()
            .any(|c| lower.contains(c));
    has_amount
        && [
            "éligible",
            "eligible",
            "limite",
            "maximum",
            "cap",
            "only",
            "uniquement",
            "seuls",
            "up to",
            "per ",
        ]
        .iter()
        .any(|c| lower.contains(c))
}

fn categories_of(detail_lower: &str) -> Vec<String> {
    for marker in CATEGORY_MARKERS {
        if let Some(pos) = detail_lower.find(marker) {
            let tail = detail_lower.get(pos + marker.len()..).unwrap_or_default();
            let tail = tail.split([';', '.']).next().unwrap_or_default();
            let parts: Vec<String> = tail
                .replace(" ou ", ",")
                .replace(" or ", ",")
                .split(',')
                .map(|p| p.trim().trim_end_matches(['.', ';']).to_owned())
                .filter(|p| !p.is_empty() && p.split_whitespace().count() <= 3)
                .collect();
            if parts.len() >= 2 {
                return parts;
            }
        }
    }
    Vec::new()
}

/// The effect verbs a text names. A word the request lists as a column (`name, email e
/// city`) is a column, never the verb it spells.
pub(super) fn effect_words(lower: &str, columns: &[String]) -> Vec<EffectVerb> {
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
        || lower.contains("save"))
        && (lower.contains("disk")
            || lower.contains("disque")
            || lower.contains("file")
            || lower.contains("fichier")
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
        ("publishing", EffectVerb::Publish),
        ("publication", EffectVerb::Publish),
        ("paiement", EffectVerb::Pay),
        ("payment", EffectVerb::Pay),
        ("commande", EffectVerb::Order),
        ("crédit", EffectVerb::Pay),
        ("credits", EffectVerb::Pay),
    ] {
        if lower.contains(needle) && !verbs.contains(&verb) {
            verbs.push(verb);
        }
    }
    verbs
}

fn push_obligation(plan: &mut Plan, obligation: Obligation) {
    if !plan
        .obligations
        .iter()
        .any(|o| o.kind.word() == obligation.kind.word())
    {
        plan.obligations.push(obligation);
    }
}

fn push_effect(plan: &mut Plan, effect: Effect) {
    if let Some(existing) = plan.effects.iter_mut().find(|e| e.verb == effect.verb) {
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
        }
    } else {
        plan.effects.push(effect);
    }
}

struct ReadState {
    conflict_marker: bool,
    final_gate: bool,
    money_sentences: Vec<String>,
}

fn earliest<'a>(text: &str, markers: &'a [&'a str]) -> Option<(usize, &'a str)> {
    markers
        .iter()
        .filter_map(|m| text.find(m).map(|p| (p, *m)))
        .min_by_key(|(p, m)| (*p, std::cmp::Reverse(m.len())))
}

/// The lowercase text before a marker, without the connector that introduced it.
fn prefix_before(text: &str, pos: usize) -> &str {
    let before = text.get(..pos).unwrap_or_default().trim();
    before
        .trim_end_matches(',')
        .trim_end_matches(';')
        .trim()
        .trim_end_matches(" and")
        .trim_end_matches(" et")
        .trim_end_matches(" but")
        .trim_end_matches(" mais")
        .trim_end_matches(" puis")
        .trim_end_matches(" then")
        .trim()
}

fn read_prefix(prefix: &str, original: &str, reading: &mut Reading, state: &mut ReadState) {
    let prefix = strip_filler(prefix);
    if prefix.is_empty() {
        return;
    }
    // Re-project the lowercase prefix onto the original clause when lengths align.
    let original_prefix = original
        .get(..prefix.len().min(original.len()))
        .filter(|_| normalize(original).len() == original.len())
        .unwrap_or(prefix);
    let owned = original_prefix.to_owned();
    for clause in split_clauses(&owned) {
        read_policy_or_clause(clause, reading, state);
    }
}

fn gate_last_automatic(reading: &mut Reading, state: &mut ReadState) {
    if let Some(last) = reading
        .plan
        .effects
        .iter_mut()
        .rev()
        .find(|e| e.policy == EffectPolicy::Automatic)
    {
        last.policy = EffectPolicy::HumanFirst;
    } else {
        state.final_gate = true;
    }
}

/// One clause and everything a settled path deferred from it: a residue re-enters as a
/// clause of its own, seen and accounted for like any other.
fn read_policy_or_clause(clause: &str, reading: &mut Reading, state: &mut ReadState) {
    read_one(clause, reading, state);
    while let Some(next) = reading.pending.pop() {
        reading.clauses += 1;
        reading.seen.push(next.clone());
        read_one(&next, reading, state);
    }
}

/// One clause: a policy pattern, an obligation, or an operation. A marker found
/// mid-clause never swallows the request before it.
#[allow(clippy::too_many_lines)] // one clause walk; each policy family is one visible arm
fn read_one(clause: &str, reading: &mut Reading, state: &mut ReadState) {
    let lower = normalize(clause);
    let text = strip_filler(&lower);
    if text.is_empty() {
        return;
    }
    if (text.contains("pose-moi la question") || text.contains("ask me the question"))
        && !text.contains("pas encore décidé")
    {
        // The companion of an explicit indecision: context, accounted for, never an operation.
        reading.plan.constraints.push(clause.to_owned());
        return;
    }
    if let Some((pos, _)) = earliest(text, STOP_MARKERS) {
        read_prefix(prefix_before(text, pos), clause, reading, state);
        reading.plan.constraints.push(clause.to_owned());
        return;
    }
    // Explicit indecision about an effect.
    if let Some((pos, marker)) = earliest(text, UNDECIDED_MARKERS) {
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
            Effect {
                verb,
                target: target.to_owned(),
                evidence: clause.to_owned(),
                policy: EffectPolicy::Undecided,
                policy_literal: None,
            },
        );
        return;
    }
    if (text.contains("pas encore décidé")
        || text.contains("not decided")
        || text.starts_with("maybe "))
        && let Some(verb) = effect_words(text, &reading.columns).first().copied()
    {
        push_effect(
            &mut reading.plan,
            Effect {
                verb,
                target: text.to_owned(),
                evidence: clause.to_owned(),
                policy: EffectPolicy::Undecided,
                policy_literal: None,
            },
        );
        return;
    }
    // A numeric attempt bound rides the clause; a clause that is only the bound is consumed.
    if let Some(n) = retry_bound(text) {
        push_obligation(
            &mut reading.plan,
            Obligation {
                kind: ObligationKind::RetryBound(n),
                evidence: clause.to_owned(),
            },
        );
        let only_bound = text.starts_with("chaque agent a")
            || text.starts_with("arrête la recherche")
            || text.starts_with("stop the research")
            || text.starts_with("limite ")
            || text.starts_with("limit ")
            || text.starts_with("bounded to ")
            || text.starts_with("avec au maximum")
            || text.starts_with("with at most")
            || text.starts_with("at most")
            || text.starts_with("au maximum")
            || text.contains("cycles de correction");
        if only_bound {
            return;
        }
    }
    // Recheck the current version before the final action.
    if let Some((pos, _)) = earliest(text, REVISION_MARKERS) {
        read_prefix(prefix_before(text, pos), clause, reading, state);
        push_obligation(
            &mut reading.plan,
            Obligation {
                kind: ObligationKind::RevisionCheck,
                evidence: clause.to_owned(),
            },
        );
        return;
    }
    // Deduplication by identifier.
    let dedup_head = [
        "déduplique",
        "dédoublonne",
        "dédoublonnez",
        "dédupliquez",
        "deduplicate",
        "dedupe",
        "remove duplicates",
        "prevent duplicates",
        "de-duplicate",
    ]
    .iter()
    .any(|m| text.starts_with(m));
    let dedup_markers = [
        "no second action for the same",
        "pas de seconde action",
        "évite les doublons",
        "évitez les doublons",
        "avoid duplicates",
        "déduplique",
        "dédoublonne",
        "deduplicate",
        "de-duplicate",
        "dedupe",
        "remove duplicates",
        "prevent duplicates",
    ];
    if !dedup_head && let Some((pos, _)) = earliest(text, &dedup_markers) {
        read_prefix(prefix_before(text, pos), clause, reading, state);
        push_obligation(
            &mut reading.plan,
            Obligation {
                kind: ObligationKind::Dedup,
                evidence: clause.to_owned(),
            },
        );
        return;
    }
    if dedup_head {
        push_obligation(
            &mut reading.plan,
            Obligation {
                kind: ObligationKind::Dedup,
                evidence: clause.to_owned(),
            },
        );
        return;
    }
    // The final action requires a fresh human validation: a listed wording or the shape
    // (`only after my explicit approval`, `the write needs my approval first`).
    let listed = earliest(text, FINAL_GATE_MARKERS).map(|(p, m)| (p, p + m.len()));
    if let Some((pos, end)) = nearest(listed, gates::final_gate(text)) {
        read_prefix(prefix_before(text, pos), clause, reading, state);
        reading.policy_clauses.push(clause.to_owned());
        let after = text.get(end..).unwrap_or_default();
        let mut verbs = effect_words(after, &reading.columns);
        if verbs.is_empty() {
            // `the write needs my approval`: the subject of the requirement is gated.
            verbs = effect_words(text.get(pos..end).unwrap_or_default(), &reading.columns);
        }
        if verbs.is_empty() {
            gate_last_automatic(reading, state);
        } else {
            for verb in verbs {
                push_effect(
                    &mut reading.plan,
                    Effect {
                        verb,
                        target: objects::destination_target(after).to_owned(),
                        evidence: clause.to_owned(),
                        policy: EffectPolicy::HumanFirst,
                        policy_literal: None,
                    },
                );
            }
        }
        return;
    }
    // A gate naming its effect: "ask me before any refund" (a listed wording or the shape).
    let listed = earliest(text, NAMED_GATE_MARKERS).map(|(p, m)| (p, p + m.len()));
    if let Some((pos, end)) = nearest(listed, gates::named_gate(text)) {
        read_prefix(prefix_before(text, pos), clause, reading, state);
        reading.policy_clauses.push(clause.to_owned());
        let after = text.get(end..).unwrap_or_default();
        let target = objects::destination_target(after);
        let verbs = effect_words(after, &reading.columns);
        if verbs.is_empty() {
            gate_last_automatic(reading, state);
        } else {
            for verb in verbs {
                let literal =
                    (verb.moves_money() && money_literal(clause)).then(|| clause.to_owned());
                push_effect(
                    &mut reading.plan,
                    Effect {
                        verb,
                        target: target.trim().to_owned(),
                        evidence: clause.to_owned(),
                        policy: EffectPolicy::HumanFirst,
                        policy_literal: literal,
                    },
                );
            }
        }
        return;
    }
    // Explicit prohibition, at the start or after the request it restricts.
    let forbidden = earliest(text, FORBIDDEN_MARKERS)
        .map(|(pos, marker)| (pos, text.get(pos + marker.len()..).unwrap_or_default()))
        .or_else(|| {
            let negated = (text.starts_with("ne ") || text.starts_with("n'"))
                && (text.contains(" jamais") || text.contains(" pas ") || text.contains(" aucun"));
            negated.then_some((0, text))
        });
    if let Some((pos, target)) = forbidden {
        read_prefix(prefix_before(text, pos), clause, reading, state);
        let verbs = effect_words(target, &reading.columns);
        if verbs.is_empty() {
            reading.plan.constraints.push(clause.to_owned());
        } else {
            for verb in verbs {
                let policy = if gates::approval_bound(target) {
                    // `don't write until i approve`: bounded by an approval, a prohibition
                    // is the gate it describes, not a ban.
                    EffectPolicy::HumanFirst
                } else if state.conflict_marker {
                    EffectPolicy::Conflict
                } else {
                    EffectPolicy::Forbidden
                };
                push_effect(
                    &mut reading.plan,
                    Effect {
                        verb,
                        target: objects::destination_target(target).to_owned(),
                        evidence: clause.to_owned(),
                        policy,
                        policy_literal: None,
                    },
                );
            }
        }
        return;
    }
    read_clause(&lower, clause, reading, &mut state.money_sentences);
}

/// Deterministically read one intent (already folded by [`fold_apostrophes`]).
#[allow(clippy::too_many_lines)] // one sentence walk; each policy family is one visible arm
pub(super) fn read(intent: &str) -> Reading {
    let mut reading = Reading {
        columns: super::columns::columns_hint(intent),
        ..Reading::default()
    };
    let mut state = ReadState {
        conflict_marker: false,
        final_gate: false,
        money_sentences: Vec::new(),
    };
    for sentence in split_sentences(intent) {
        let lower = normalize(sentence);
        let text = strip_filler(&lower);
        if money_literal(sentence) {
            state.money_sentences.push(sentence.to_owned());
            reading.plan.bindings.push(Binding {
                role: "money_policy",
                literal: sentence.to_owned(),
            });
        }
        if text.contains("contradiction")
            || text.contains("contradictory")
            || text.contains("these two instructions")
            || text.contains("ces deux consignes")
        {
            state.conflict_marker = true;
            reading.plan.constraints.push(sentence.to_owned());
            continue;
        }
        // Trigger / cadence prefix, or a supplied document.
        let mut body = sentence;
        let mut body_lower = text.to_owned();
        if let Some(prefix) = TRIGGER_PREFIXES.iter().find(|p| body_lower.starts_with(*p))
            && let Some(comma) = body_lower.find(',')
        {
            let head = body_lower.get(..comma).unwrap_or_default().to_owned();
            if prefix.starts_with("à partir de")
                || prefix.starts_with("from the")
                || prefix.starts_with("starting from")
            {
                let detail = head
                    .get(prefix.len()..)
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                reading.plan.push_step(Step {
                    op: Op::Read,
                    evidence: sentence.to_owned(),
                    detail,
                    categories: Vec::new(),
                });
            } else if reading.plan.trigger.is_none() {
                reading.plan.trigger = Some(head.clone());
            }
            let rest_lower = body_lower
                .get(comma + 1..)
                .unwrap_or_default()
                .trim()
                .to_owned();
            if let Some(pos) = normalize(sentence).find(&rest_lower)
                && let Some(rest) = sentence.get(pos..)
            {
                body = rest.trim();
            }
            body_lower = rest_lower;
        }
        if body_lower.is_empty() {
            continue;
        }
        let negated_sentence = FORBIDDEN_MARKERS.iter().any(|m| body_lower.starts_with(m))
            || body_lower.starts_with("ne ")
            || body_lower.starts_with("n'");
        let clauses = if negated_sentence {
            vec![body]
        } else {
            split_clauses(body)
        };
        reading.clauses += clauses.len();
        for clause in clauses {
            reading.seen.push(clause.to_owned());
            read_policy_or_clause(clause, &mut reading, &mut state);
        }
    }
    if state.final_gate {
        if let Some(last) = reading
            .plan
            .effects
            .iter_mut()
            .rev()
            .find(|e| e.policy == EffectPolicy::Automatic)
        {
            last.policy = EffectPolicy::HumanFirst;
        } else if reading.plan.effects.is_empty() {
            reading.plan.unknowns.push(
                "a final action requires human validation, but no final effect was recognized"
                    .to_owned(),
            );
        }
    }
    for effect in &mut reading.plan.effects {
        if effect.verb.moves_money() && effect.policy_literal.is_none() {
            effect.policy_literal = state.money_sentences.first().cloned();
        }
    }
    collect_bindings(intent, &mut reading.plan);
    reading
}

/// Read one clause; returns whether it produced an operation, effect or obligation.
#[allow(clippy::too_many_lines)] // one clause walk: negation, head, medium, then the head's arm
fn read_clause(lower: &str, original: &str, reading: &mut Reading, _money: &mut [String]) -> bool {
    let text = strip_filler(lower);
    if text.is_empty() {
        return false;
    }
    let negated = NEGATION_OPENERS.iter().any(|m| text.starts_with(m));
    if negated {
        let verbs = effect_words(text, &reading.columns);
        if verbs.is_empty() {
            // A negated clause that still carries an operation head ("n'extraire que …",
            // "ne corrige pas …") restricts work the reader cannot read: cognition, not a constraint.
            let carries_head = text
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .any(|w| !w.is_empty() && head_of_exact(w).is_some());
            if carries_head {
                reading.unresolved.push(original.to_owned());
            } else {
                reading.plan.constraints.push(original.to_owned());
            }
        } else {
            for verb in verbs {
                push_effect(
                    &mut reading.plan,
                    Effect {
                        verb,
                        target: original.to_owned(),
                        evidence: original.to_owned(),
                        policy: EffectPolicy::Forbidden,
                        policy_literal: None,
                    },
                );
            }
        }
        return true;
    }
    if CONSTRAINT_OPENERS.iter().any(|m| text.starts_with(m)) {
        reading.plan.constraints.push(original.to_owned());
        return true;
    }
    // A listed column at the head of a clause (`credit_cents, the sum of …`) is a column,
    // never the verb it spells.
    let first_token = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or_default();
    let found = head_of(text).filter(|(phrase, head)| {
        !(matches!(head, Head::Effect(_))
            && reading
                .columns
                .iter()
                .any(|c| c.eq_ignore_ascii_case(phrase) || c.eq_ignore_ascii_case(first_token)))
    });
    let Some((phrase, head)) = found else {
        let declarative = [
            " est ",
            " sont ",
            " reste ",
            " contient ",
            " contiennent ",
            " arrive",
            " annule ",
            " want ",
            " veux ",
            " annulent ",
            " suffisent",
            " peuvent ",
            " peut ",
            " doit ",
            " doivent ",
            " is ",
            " are ",
            " remains ",
            " contains ",
            " has ",
            " have ",
        ]
        .iter()
        .any(|m| text.contains(m));
        if declarative {
            reading.plan.constraints.push(original.to_owned());
            reading.soft_constraints.push(original.to_owned());
        } else {
            reading.unresolved.push(original.to_owned());
        }
        return false;
    };
    let mut consumed_head = phrase.len();
    if !text.starts_with(phrase) {
        // the head matched with one filler word skipped: consume "<first> <filler>" + the rest of the phrase
        if let Some((first, rest)) = text.split_once(' ')
            && let Some((filler, _)) = rest.split_once(' ')
        {
            consumed_head =
                first.len() + 1 + filler.len() + 1 + phrase.len().saturating_sub(first.len() + 1);
        }
    }
    let detail_lower = strip_filler(text.get(consumed_head..).unwrap_or_default());
    let consumed = lower.len() - detail_lower.len();
    let mut detail = remainder(original, lower, consumed).to_owned();
    let path = detail
        .split_whitespace()
        .map(|w| w.trim_end_matches(['.', ',', ';', ')', ':']))
        .find(|w| (w.starts_with("./") || (w.starts_with('/') && w.contains('.'))) && w.len() > 2)
        .map(str::to_owned);
    let mut detail_lower = detail_lower;
    let lowered_path: String;
    if let Some(path) = &path
        && matches!(head, Head::Effect(_))
        && detail.trim_start().starts_with(path.as_str())
        && objects::destination_at(detail_lower, detail.find(path.as_str()).unwrap_or(0)).is_none()
    {
        // `write ./total.md; the write needs my approval first`: the path is the whole
        // object; what follows it is read on its own, never swallowed as the target.
        defer_residue(&detail, path, reading);
        detail.clone_from(path);
        lowered_path = path.to_lowercase();
        detail_lower = &lowered_path;
    }
    // A named local path settles the medium: writing TO a path is a file effect,
    // reading a path is the supplied-document read.
    if let Some(path) = &path {
        let writes = heads::writes_to_path(phrase);
        let saves = detail_lower.contains(" to ")
            || detail_lower.contains(" dans ")
            || detail_lower.contains(" into ")
            || detail_lower.contains(" sous ");
        if writes && saves {
            reading.plan.bindings.push(Binding {
                role: "path",
                literal: path.clone(),
            });
            push_effect(
                &mut reading.plan,
                Effect {
                    verb: EffectVerb::Write,
                    target: path.clone(),
                    evidence: original.to_owned(),
                    policy: EffectPolicy::Automatic,
                    policy_literal: None,
                },
            );
            written_object(&detail, detail_lower, path, original, reading);
            defer_residue(&detail, path, reading);
            return true;
        }
        if matches!(head, Head::Choice(options) if options.contains(&Op::Read)) {
            reading.plan.push_step(Step {
                op: Op::Read,
                evidence: original.to_owned(),
                detail: path.clone(),
                categories: Vec::new(),
            });
            defer_residue(&detail, path, reading);
            return true;
        }
        if let Head::Effect(verb) = head {
            let path_at = detail.find(path.as_str()).unwrap_or(0);
            if objects::destination_at(detail_lower, path_at).is_some() {
                // The path is the destination: the effect targets it and the rest of the
                // clause is read on its own, never swallowed as the target.
                push_effect(
                    &mut reading.plan,
                    Effect {
                        verb: *verb,
                        target: path.clone(),
                        evidence: original.to_owned(),
                        policy: EffectPolicy::Automatic,
                        policy_literal: money_literal(original).then(|| original.to_owned()),
                    },
                );
                if matches!(verb, EffectVerb::Write | EffectVerb::Publish) {
                    written_object(&detail, detail_lower, path, original, reading);
                }
                defer_residue(&detail, path, reading);
                return true;
            }
        }
    }
    let url = detail
        .split_whitespace()
        .map(|w| w.trim_end_matches(['.', ',', ';', ')', ':']))
        .find(|w| w.starts_with("http://") || w.starts_with("https://"))
        .map(str::to_owned);
    if let Some(url) = &url
        && matches!(
            head,
            Head::Op(Op::Draft | Op::Extract | Op::Classify | Op::Validate | Op::Compute)
        )
    {
        reading.plan.bindings.push(Binding {
            role: "url",
            literal: url.clone(),
        });
        reading.plan.push_step(Step {
            op: Op::Fetch,
            evidence: original.to_owned(),
            detail: url.clone(),
            categories: Vec::new(),
        });
    }
    if let Some(url) = &url
        && matches!(
            head,
            Head::Choice(_) | Head::Op(Op::Lookup | Op::Search | Op::Fetch)
        )
    {
        reading.plan.bindings.push(Binding {
            role: "url",
            literal: url.clone(),
        });
        reading.plan.push_step(Step {
            op: Op::Fetch,
            evidence: original.to_owned(),
            detail: url.clone(),
            categories: Vec::new(),
        });
        return true;
    }
    match head {
        Head::Op(op) => {
            let categories = if *op == Op::Classify {
                categories_of(detail_lower)
            } else {
                Vec::new()
            };
            reading.plan.push_step(Step {
                op: *op,
                evidence: original.to_owned(),
                detail,
                categories,
            });
        }
        Head::Choice(options) => {
            let prose = [
                "source",
                "texte",
                "argument",
                "contradict",
                "opinion",
                "version",
                "réponse",
                "text",
                "answer",
            ];
            let numeric = [
                "prix",
                "montant",
                "total",
                "quantit",
                "nombre",
                "chiffre",
                "valeur",
                "seuil",
                "price",
                "amount",
                "quantity",
                "number",
                "threshold",
                "count",
            ];
            let compare = *options == [Op::Compute, Op::Draft];
            let settled = if compare
                && prose.iter().any(|c| detail_lower.contains(c))
                && !numeric.iter().any(|c| detail_lower.contains(c))
            {
                Some(Op::Draft)
            } else if compare && numeric.iter().any(|c| detail_lower.contains(c)) {
                Some(Op::Compute)
            } else if options.contains(&Op::Read)
                && READ_CUES.iter().any(|c| detail_lower.contains(c))
            {
                Some(Op::Read)
            } else if options.contains(&Op::Lookup)
                && LOOKUP_CUES.iter().any(|c| detail_lower.contains(c))
                && !SEARCH_CUES.iter().any(|c| detail_lower.contains(c))
            {
                Some(Op::Lookup)
            } else if options.contains(&Op::Search)
                && SEARCH_CUES.iter().any(|c| detail_lower.contains(c))
            {
                Some(Op::Search)
            } else if options.contains(&Op::Lookup)
                && LOOKUP_CUES.iter().any(|c| detail_lower.contains(c))
            {
                Some(Op::Lookup)
            } else {
                None
            };
            match settled {
                Some(op) => reading.plan.push_step(Step {
                    op,
                    evidence: original.to_owned(),
                    detail,
                    categories: Vec::new(),
                }),
                None => reading.ambiguous.push(Ambiguity {
                    clause: original.to_owned(),
                    detail,
                    options: options.to_vec(),
                }),
            }
        }
        Head::Effect(verb) => {
            let literal = money_literal(original).then(|| original.to_owned());
            let money = [
                "money", "argent", "payment", "paiement", "€", "euro", "dollar", "usd", "eur ",
                "credits", "crédit",
            ]
            .iter()
            .any(|w| detail_lower.contains(w));
            let verb = if *verb == EffectVerb::Send && money {
                EffectVerb::Pay
            } else {
                *verb
            };
            push_effect(
                &mut reading.plan,
                Effect {
                    verb,
                    target: detail.clone(),
                    evidence: original.to_owned(),
                    policy: EffectPolicy::Automatic,
                    policy_literal: literal,
                },
            );
        }
        Head::Dedup => {
            push_obligation(
                &mut reading.plan,
                Obligation {
                    kind: ObligationKind::Dedup,
                    evidence: original.to_owned(),
                },
            );
        }
    }
    true
}

fn collect_bindings(intent: &str, plan: &mut Plan) {
    for word in intent.split_whitespace() {
        let token = word.trim_end_matches(['.', ',', ';', ')', ']', ':']);
        if token.starts_with("http://") || token.starts_with("https://") {
            plan.bindings.push(Binding {
                role: "url",
                literal: token.to_owned(),
            });
        } else if token.contains('@') && token.contains('.') && !token.starts_with('@') {
            plan.bindings.push(Binding {
                role: "email",
                literal: token.to_owned(),
            });
        } else if (token.starts_with("./") || token.starts_with('/')) && token.len() > 2 {
            plan.bindings.push(Binding {
                role: "path",
                literal: token.to_owned(),
            });
        } else if token.starts_with("Europe/")
            || token.starts_with("America/")
            || token.starts_with("Asia/")
            || token.starts_with("Africa/")
        {
            plan.bindings.push(Binding {
                role: "timezone",
                literal: token.to_owned(),
            });
        }
    }
}
