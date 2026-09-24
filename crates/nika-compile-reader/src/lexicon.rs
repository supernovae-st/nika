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

mod cadence;
mod convert;
mod copy;
mod cues;
mod effects;
mod es;
mod gating;
mod heads;
mod it;
mod lines;
mod literals;
mod slugs;

use super::paths::{self, Structured};
use super::plan::{
    Binding, Effect, EffectPolicy, EffectVerb, Obligation, ObligationKind, Op, Plan, Step,
};
use super::{gates, hot, objects};
pub(crate) use cadence::quoted;
use cadence::{cut_head, cut_tail, record_recurrence, settle_tails};
pub use cues::settle_retrieval;
pub(crate) use cues::{ARTICLES, OBJECT_CONNECTORS};
use cues::{
    CONSTRAINT_OPENERS, DEDUP_MARKERS, FINAL_GATE_MARKERS, FORBIDDEN_MARKERS, LEADING_FILLER,
    NAMED_GATE_MARKERS, NEGATION_OPENERS, REVISION_MARKERS, SECOND_WORD_FILLERS, STOP_MARKERS,
    STRONG_CONNECTORS, UNDECIDED_MARKERS, WEAK_CONNECTORS,
};
pub use effects::effect_words;
pub(crate) use effects::kindred;
use effects::{push_effect, push_obligation};
pub(crate) use heads::Head;
pub use slugs::slug;

/// The number a word spells in six languages (« five », « cinq », « fünf »), for the stages
/// that read a count.
pub(crate) fn number_word(folded: &str) -> Option<u32> {
    cues::NUMBER_WORDS
        .iter()
        .find(|(word, _)| *word == folded)
        .map(|(_, n)| *n)
}

/// One clause the lexicon could not settle alone: a small feasible set, never a guess.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ambiguity {
    pub clause: String,
    pub detail: String,
    pub options: Vec<Op>,
}

/// The deterministic reading of one intent.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Reading {
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
    /// The columns the request lists beside its source: a word among them names a column,
    /// never an effect, and the closed rule grammar reads them.
    pub columns: Vec<String>,
}

/// A clause the closed rule grammar read whole ("count the rows per client", "merge them on
/// the id column") is a compute step carrying its rule: the words are its literals.
fn push_rule(original: &str, rule: super::rules::Rule, reading: &mut Reading) {
    // The detail is the text the grammar read (the whole clause, or the object after a
    // computation head such as « compute »): the admission and the binding find the rule by
    // that very text.
    reading.plan.push_step(Step {
        op: Op::Compute,
        evidence: original.to_owned(),
        detail: rule.text().trim().to_owned(),
        categories: Vec::new(),
    });
    if !reading.plan.rules.iter().any(|r| r.text() == rule.text()) {
        reading.plan.rules.push(rule);
    }
}

impl Reading {
    /// Why this reading may NOT be admitted as HOT under the strict contract: every clause
    /// consumed is not evidence of understanding. A step is explicit when its object is a
    /// typed literal or a short noun phrase without coordinated residue; an effect when its
    /// target is short or literal; and nothing ambiguous, unresolved or unknown remains.
    #[must_use]
    pub fn hot_rejections(&self) -> Vec<String> {
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
            // A rule the closed grammar parsed is a typed literal, explicit by construction;
            // the plan joins a promoted rule to an existing computation with ` ; `, so each
            // part is judged on its own.
            let ruled = step.op == Op::Compute
                && step.detail.split(" ; ").all(|part| {
                    objects::explicit_object(part)
                        || self.plan.rules.iter().any(|r| r.text() == part.trim())
                });
            // An extract's object is the list of the fields to pull out: a list of short
            // noun phrases is explicit, whatever its length.
            let listed = step.op == Op::Extract && objects::explicit_field_list(&step.detail);
            if !categorical && !ruled && !listed && !objects::explicit_object(&step.detail) {
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
            ) && !objects::explicit_object(&effect.target)
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
                || self.plan.rules.iter().any(|r| within(r.text()))
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
    #[must_use]
    pub fn complete(&self) -> bool {
        self.unresolved.is_empty()
            && self.ambiguous.is_empty()
            && (!self.plan.steps.is_empty() || !self.plan.effects.is_empty())
    }
}

/// Typographic apostrophes fold to `'` so byte offsets stay aligned between the
/// lowercase matching copy and the evidence copy. Callers anchor against this form.
#[must_use]
pub fn fold_apostrophes(intent: &str) -> String {
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
    // A fold of pieces produced earlier ("the combined brief" after a draft of each one)
    // refers back to those pieces; with nothing produced, it names new content.
    let produced = reading
        .plan
        .steps
        .iter()
        .any(|s| matches!(s.op, Op::Draft | Op::Extract | Op::Compute | Op::Classify));
    // "the category" after a classify step is that classification.
    let classified = reading.plan.has(Op::Classify) && objects::names_classification(object_lower);
    // "the page title" after a fetch is a facet of the fetched page: the fetch's own
    // extract mode, carried as it is, never a draft of it.
    let fetched = reading.plan.has(Op::Fetch) && objects::page_facet(object_lower).is_some();
    if refers_back || classified || fetched || (produced && objects::folds(object_lower)) {
        return;
    }
    // « write the lines that start with # to ./out/titles.txt »: the kept lines.
    if lines::written_lines(object, original, reading) {
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

pub(super) fn normalize(text: &str) -> String {
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

/// The unknown a deterministic reading raises when it saw a final human gate and no effect
/// for it to guard; the cold merge lifts it once the proposal supplies the effect.
pub const GATE_WITHOUT_EFFECT: &str =
    "a final action requires human validation, but no final effect was recognized";

/// The unknown work a request is when it states two triggers one workflow cannot both start
/// on (an event or a head and a sentence-final cadence that differs from it, or two different
/// sentence-final cadences): its first words, followed by both triggers.
pub const TWO_TRIGGERS: &str = "the request states two triggers";

/// Sentences of a request: `.`, `!`, `?` end one only before whitespace or the end (a dot
/// inside `./out/sent.md` or `127.0.0.1` does not); `;` and a newline always do.
pub fn split_sentences(intent: &str) -> Vec<&str> {
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

pub(crate) fn head_of_exact(lower: &str) -> Option<(&'static str, &'static Head)> {
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

/// Whether an object opens with a noun naming produced content (`un digest des notes`,
/// `a short report`, `un riassunto in 3 punti`): after its determiners, within the head
/// noun phrase (before an `of`/`de`/`di`), one of the closed produced-content nouns.
fn opens_with_produced_noun(object_lower: &str) -> bool {
    const OF: &[&str] = &[
        "of", "de", "du", "des", "d'", "from", "about", "sur", "di", "del", "della", "dei",
        "delle", "degli", "sobre", "dans", "in", "en", "nel", "nella",
    ];
    hot::fold(object_lower)
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'')
        .flat_map(|t| t.split('\''))
        .filter(|t| !t.is_empty())
        .skip_while(|t| ARTICLES.contains(t))
        .take_while(|t| !OF.contains(t))
        .take(3)
        .any(|t| hot::PRODUCED_NOUNS.lines().any(|n| n == t))
}

/// Whether the local path an object names is its material rather than a destination: a
/// folder or a glob is never written to (`les notes dans ./notes` is a source whatever
/// its connector); a file is material only when no destination connector precedes it.
fn source_path(detail: &str, detail_lower: &str, path: &str) -> bool {
    match paths::token(path) {
        Some(paths::PathShape::Directory(_) | paths::PathShape::Glob(_)) => true,
        _ => detail
            .find(path)
            .is_some_and(|at| objects::destination_at(detail_lower, at).is_none()),
    }
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

/// Whether a connector opens a stage the closed grammar reads whole (« …, sort them by
/// priority and … », « …, garde seulement les lignes dont le statut est ouvert, … »): a
/// clause boundary the reader cuts whatever verb the stage opens with, so a computation
/// stated after a comma never rides as the residue of the clause before it.
fn stage_ahead(rest: &str) -> bool {
    let end = STRONG_CONNECTORS
        .iter()
        .chain(WEAK_CONNECTORS.iter())
        .filter_map(|connector| rest.find(connector))
        .min()
        .unwrap_or(rest.len());
    let segment = rest.get(..end).unwrap_or_default().trim();
    // A stated stage (a sort, a count, a projection, a rename…) or a keep-family filter the
    // grammar reads whole; never a bare comparator (`token=…?` inside a question) nor a
    // question: those stay in the clause that carries them.
    if segment.is_empty() || segment.ends_with('?') {
        return false;
    }
    super::stages::stated(segment, &[]).is_some()
        || (KEEP_OPENERS.iter().any(|lead| segment.starts_with(lead))
            && super::rules::synthesize(segment, &[]).is_some())
}

/// A connector inside a projection list (« keep only the name and email of each person »,
/// « ne garde que le nom et l'email de chaque personne ») is no clause boundary, whatever
/// verb the next item spells (« email » is also an effect head): the list the grammar reads
/// whole continues across it. The keep lead is looked for before the connector, and the
/// grammar judges the whole span up to the next connector.
fn projection_continues(before: &str, connector: &str, rest: &str) -> bool {
    let end = STRONG_CONNECTORS
        .iter()
        .chain(WEAK_CONNECTORS.iter())
        .filter_map(|c| rest.find(c))
        .min()
        .unwrap_or(rest.len());
    let tail = rest.get(..end).unwrap_or_default().trim();
    if tail.is_empty() {
        return false;
    }
    KEEP_OPENERS.iter().any(|lead| {
        before.rfind(lead).is_some_and(|at| {
            let span = format!("{}{connector}{tail}", before.get(at..).unwrap_or_default());
            super::stages::stated(&span, &[]).is_some_and(|shape| !shape.columns.is_empty())
        })
    })
}

/// The constraint openers of the keep family: « keep only the rows whose status is open »
/// states a computation the closed grammar reads whole, « keep the tone formal » a
/// constraint. The grammar judges first; only what it cannot read is a constraint.
const KEEP_OPENERS: &[&str] = &[
    "keep ",
    "conserve ",
    "garde ",
    "mantieni ",
    "conserva ",
    "mantén ",
    "manten ",
];

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
                let before = lower.get(..at).unwrap_or_default();
                if (always || head_of(strip_filler(rest)).is_some() || stage_ahead(rest))
                    && !projection_continues(before, connector, rest)
                {
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

struct ReadState {
    conflict_marker: bool,
    final_gate: bool,
    money_sentences: Vec<String>,
    /// The sentence-final cadences cut off during the walk, settled once it ends.
    tails: Vec<String>,
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
        .trim_end_matches(" e")
        .trim_end_matches(" ed")
        .trim_end_matches(" poi")
        .trim_end_matches(" y")
        .trim_end_matches(" luego")
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
    if let Some(n) = literals::retry_bound(text) {
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
        "deduplica",
        "elimina i duplicati",
        "rimuovi i duplicati",
        "evita i duplicati",
        "elimina los duplicados",
        "quita los duplicados",
        "evita los duplicados",
    ]
    .iter()
    .any(|m| text.starts_with(m));
    if !dedup_head && let Some((pos, _)) = earliest(text, DEDUP_MARKERS) {
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
    // « Non serve chiedermi conferma », « no need to ask me »: a waiver is no gate. It is a
    // policy clause; beside a contrary prohibition, the compiler refuses the bypass. A negated
    // waiver (« mais pas sans me demander ») is the gate it denies waiving.
    match gates::waiver_polarity(text) {
        Some(true) => {
            reading.policy_clauses.push(clause.to_owned());
            return;
        }
        Some(false) => {
            reading.policy_clauses.push(clause.to_owned());
            gating::gate_last_automatic(reading, state, gating::names_sending(text));
            return;
        }
        None => {}
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
            gating::gate_last_automatic(reading, state, gating::names_sending(text));
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
            gating::gate_last_automatic(reading, state, gating::names_sending(text));
        } else {
            for verb in verbs {
                let literal = (verb.moves_money() && literals::money_literal(clause))
                    .then(|| clause.to_owned());
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
            if gates::approval_bound(target) {
                // The clause is the policy of the effect it names, which may be stated
                // elsewhere ("post it to <url>. Never send anything without my approval").
                reading.policy_clauses.push(clause.to_owned());
            }
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
#[must_use]
pub fn read(intent: &str) -> Reading {
    let mut reading = Reading {
        columns: super::columns::columns_hint(intent),
        ..Reading::default()
    };
    let mut state = ReadState {
        conflict_marker: false,
        final_gate: false,
        money_sentences: Vec::new(),
        tails: Vec::new(),
    };
    for sentence in split_sentences(intent) {
        let lower = normalize(sentence);
        let text = strip_filler(&lower);
        if literals::money_literal(sentence) {
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
        let (body, body_lower) = cut_head(sentence, text, &mut reading);
        if body_lower.is_empty() {
            continue;
        }
        let negated_sentence = FORBIDDEN_MARKERS.iter().any(|m| body_lower.starts_with(m))
            || body_lower.starts_with("ne ")
            || body_lower.starts_with("n'")
            || body_lower.starts_with("non ")
            || body_lower.starts_with("nunca ");
        // A sentence-final cadence (« … chaque lundi », « … every Monday ») is cut off as a
        // head is, then settled against the heads once every sentence is read.
        let body = if negated_sentence {
            body
        } else {
            cut_tail(body, &body_lower, &mut state.tails)
        };
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
    for effect in &mut reading.plan.effects {
        if effect.verb.moves_money() && effect.policy_literal.is_none() {
            effect.policy_literal = state.money_sentences.first().cloned();
        }
    }
    settle_tails(&state.tails, &mut reading);
    record_recurrence(intent, &mut reading);
    super::hot::destination_floor(intent, &mut reading.plan);
    // A deferred final approval holds the last automatic effect once the floors have added the
    // writes the request asks for (« … dans un fichier. Seulement après ma validation. »).
    // Settled before them it missed those writes and, beside a prohibited effect, vanished
    // while the floor's write stayed automatic.
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
            reading.plan.unknowns.push(GATE_WITHOUT_EFFECT.to_owned());
        }
    }
    literals::collect_bindings(intent, &mut reading.plan);
    reading
}

/// Read one clause; returns whether it produced an operation, effect or obligation.
#[allow(clippy::too_many_lines)] // one clause walk: negation, head, medium, then the head's arm
fn read_clause(lower: &str, original: &str, reading: &mut Reading, money: &mut [String]) -> bool {
    let text = strip_filler(lower);
    if text.is_empty() {
        return false;
    }
    // « ./rando/guide.md : extrais … »: the clause is led by its source.
    if lines::leading_source(original, reading, money) {
        return true;
    }
    // « as they are, in order »: what a lines rule already does by construction.
    if reading.plan.rules.iter().any(super::rules::Rule::lines)
        && super::rules::by_construction_tail(text)
    {
        return true;
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
    // « copy ./a.txt as is to ./out/b.txt »: a read and a write of what was read.
    if copy::read(text, original, reading) {
        return true;
    }
    // « convert ./a.csv into ./out/a.json »: the parsed records, written in the other format.
    if convert::read(text, original, reading) {
        return true;
    }
    if CONSTRAINT_OPENERS.iter().any(|m| text.starts_with(m)) {
        if KEEP_OPENERS.iter().any(|m| text.starts_with(m))
            && !cues::question(original)
            && let Some(rule) = super::rules::synthesize(original, &reading.columns)
        {
            push_rule(original, rule, reading);
            return true;
        }
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
        // A clause with no head that the closed rule grammar reads whole ("count the rows
        // per client", "sort the rows by amount descending", "remove the duplicate lines")
        // is a stated computation: its words are the literals, the jq is the compiler's.
        // A question ("Which ending is gentler?") is a conversation, never a rule.
        if !cues::question(original)
            && let Some(rule) = super::rules::synthesize(original, &reading.columns)
        {
            push_rule(original, rule, reading);
            return true;
        }
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
    // A make head (`fais-moi`, `fammi`, `hazme`) is a draft only of produced content: a
    // digest, un résumé, un riassunto. `fais-moi un café` is a request the reader does not
    // know, never a draft of a coffee.
    if heads::is_make(phrase) && !opens_with_produced_noun(detail_lower) {
        reading.unresolved.push(original.to_owned());
        return false;
    }
    // A named local path settles the medium: writing TO a path is a file effect,
    // reading a path is the supplied-document read.
    if let Some(path) = &path {
        let writes = heads::writes_to_path(phrase);
        // « rename the country column to region and write ./sales-region.csv »: a bare path
        // after the verb is the destination of the rows a computation produced. With no
        // computation before it, a bare path stays a write with no content.
        let bare = detail_lower.trim() == path.to_lowercase()
            && reading.plan.steps.iter().any(|s| s.op == Op::Compute);
        let saves = bare
            || detail_lower.contains(" to ")
            || detail_lower.contains(" dans ")
            || detail_lower.contains(" into ")
            || detail_lower.contains(" sous ")
            || objects::destination_at(detail_lower, detail.find(path.as_str()).unwrap_or(0))
                .is_some();
        if writes && saves {
            // Several destinations in one clause ("write the bugs to ./bugs.json and the
            // features to ./features.json"): one write per destination, each with its own
            // object and its own verbatim excerpt.
            let segments = objects::write_segments(&detail);
            if segments.len() >= 2 {
                for (_, target, segment) in &segments {
                    reading.plan.bindings.push(Binding {
                        role: "path",
                        literal: target.clone(),
                    });
                    push_effect(
                        &mut reading.plan,
                        Effect {
                            verb: EffectVerb::Write,
                            target: target.clone(),
                            evidence: segment.clone(),
                            policy: EffectPolicy::Automatic,
                            policy_literal: None,
                        },
                    );
                    written_object(segment, &segment.to_lowercase(), target, segment, reading);
                }
                return true;
            }
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
        // `write ./sorted.csv`: a write head whose whole object is the path writes the
        // latest result there. With nothing produced before it, the admission law names
        // the missing content; the path is never read as something to draft.
        if writes
            && matches!(head, Head::Op(Op::Draft))
            && detail.trim().trim_end_matches(['.', ',', ';']) == path.as_str()
        {
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
            return true;
        }
        if matches!(head, Head::Choice(options) if options.contains(&Op::Read)) {
            // "Read ./a.csv and ./b.csv": a list of files is read as stated, every file a
            // path literal in order; any other residue re-enters as a clause of its own.
            let listed = objects::path_list(&detail);
            reading.plan.push_step(Step {
                op: Op::Read,
                evidence: original.to_owned(),
                detail: listed
                    .as_ref()
                    .map_or_else(|| paths::material(path), |files| files.join(" ; ")),
                categories: Vec::new(),
            });
            if listed.is_none() {
                defer_residue(&detail, path, reading);
            }
            return true;
        }
        // A source path inside the object of an operation (`traduis ./notes/brief.md en
        // anglais`, `un digest des notes dans ./notes`) is the material the operation
        // consumes: the read is that path (a folder is every file directly under it) and
        // the operation keeps the object the clause states, verbatim.
        // An extraction of whole lines by a stated pattern is a line filter, never language work.
        if matches!(head, Head::Op(Op::Extract))
            && lines::read_extract(&detail, detail_lower, original, reading)
        {
            return true;
        }
        if let Head::Op(op @ (Op::Draft | Op::Extract | Op::Classify | Op::Validate | Op::Compute)) =
            head
            && source_path(&detail, detail_lower, path)
        {
            reading.plan.bindings.push(Binding {
                role: "path",
                literal: path.clone(),
            });
            reading.plan.push_step(Step {
                op: Op::Read,
                evidence: original.to_owned(),
                detail: paths::material(path),
                categories: Vec::new(),
            });
            let categories = if *op == Op::Classify {
                literals::categories_of(detail_lower)
            } else {
                Vec::new()
            };
            reading.plan.push_step(Step {
                op: *op,
                evidence: original.to_owned(),
                detail,
                categories,
            });
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
                        policy_literal: literals::money_literal(original)
                            .then(|| original.to_owned()),
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
            // "trie les lignes par montant décroissant": a head the reader knows as a
            // classify whose whole clause the closed grammar reads (a sort by a stated
            // column) is that computation; "classe les tickets en bugs et features" stays
            // a classification, the grammar reads no shape in it. A computation head
            // (« compute the total of the amount column per client », « conserva solo las
            // filas cuyo importe supera 200 ») is the computation its clause or its object
            // states: the grammar reads it whole, and its words are the literals.
            if matches!(op, Op::Classify | Op::Compute)
                && let Some(rule) =
                    super::rules::synthesize(original, &reading.columns).or_else(|| {
                        (*op == Op::Compute)
                            .then(|| super::rules::synthesize(&detail, &reading.columns))
                            .flatten()
                    })
            {
                push_rule(original, rule, reading);
                return true;
            }
            let categories = if *op == Op::Classify {
                literals::categories_of(detail_lower)
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
            } else {
                settle_retrieval(detail_lower, options)
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
            // "merge them on the id column": a join of the read sources on a stated column
            // is a computation the compiler writes, never an external merge effect.
            if *verb == EffectVerb::Merge
                && let Some(rule) = super::rules::synthesize(original, &reading.columns)
                && rule.joins()
            {
                push_rule(original, rule, reading);
                return true;
            }
            // "merge them": the sources are named, the key is not. The human completes the
            // clause; no endpoint is asked for a merge of the files the request read.
            if *verb == EffectVerb::Merge && super::stages::join_without_key(original) {
                reading.unresolved.push(original.to_owned());
                return false;
            }
            let literal = literals::money_literal(original).then(|| original.to_owned());
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
