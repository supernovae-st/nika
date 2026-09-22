// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate composer: a finite, deterministically ordered set of private plans
//! between the COLD proposals, the recalled candidates and the assembler, judged by a
//! deterministic feasibility filter before any seat sees them.
//!
//! One [`Candidate`] is one plan with its source, its structural signature and its
//! feasibility verdict. The set is the distinct admissible COLD plans (first-seen
//! order, capped at [`CAP`]) plus the pattern-informed variants the recalled
//! candidates suggest in a dimension the request leaves open — and ONLY when the
//! assembler can express that dimension. Today the assembler emits one linear chain
//! per invocation and the private plan carries no topology, so the fan-out and fan-in
//! dimensions the pattern words name are recorded as suggestions
//! (`expressible: false`) and yield no variant: a variant nobody can assemble is not
//! composed, and a source nothing produces is not declared.
//!
//! Feasibility is a hard filter against the deterministic reading (the floor) and the
//! request text. Every failure is a recorded reason; an infeasible candidate stays in
//! provenance and is never offered to a seat. Nothing here calls a provider, writes
//! source or grants authority: the assembler and Check judge whatever is selected.

use std::cmp::Reverse;

use super::lexicon::Reading;
use super::plan::{Binding, EffectPolicy, EffectVerb, Op, Plan, Step};
use super::retrieve::Hit;
use serde_json::{Value, json};

/// The finite cap on composed candidates.
pub(super) const CAP: usize = 8;

/// Topologies the assembler expresses from a private plan today: one linear chain per
/// invocation. A pattern dimension outside this set is recorded, never composed.
const EXPRESSIBLE: &[&str] = &["sequential"];

/// A whole-word phrase of the request.
type Phrase = &'static [&'static str];
/// One pattern dimension: its word, the recalled pattern words that name it, and the
/// request phrases that already fix it (a variant there would contradict the request).
type DimensionSpec = (&'static str, &'static [&'static str], &'static [Phrase]);

/// The pattern dimensions the recalled candidates may suggest.
const DIMENSIONS: &[DimensionSpec] = &[
    (
        "fanout",
        &["fanout", "batch"],
        &[
            &["each"],
            &["every"],
            &["chaque"],
            &["chacun"],
            &["chacune"],
            &["parallel"],
            &["parallele"],
            &["batch"],
            &["one", "by", "one"],
            &["un", "par", "un"],
        ],
    ),
    (
        "fanin",
        &["fanin", "join"],
        &[
            &["combine"],
            &["merge"],
            &["fusionne"],
            &["regroupe"],
            &["aggregate"],
            &["agrege"],
            &["consolidate"],
            &["consolide"],
            &["join"],
        ],
    ),
];

/// Where one candidate came from. Pattern variants and ambiguity readings would be
/// further sources; neither exists today (see the module doc), so neither is declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CandidateSource {
    /// The admissible plan first proposed by this COLD sample.
    ColdSample(usize),
}

/// One composed candidate.
#[derive(Clone, Debug)]
pub(super) struct Candidate {
    pub plan: Plan,
    pub source: CandidateSource,
    /// Sorted operation words, `effect:verb:policy` words and obligation words.
    pub signature: Vec<String>,
    /// `Ok` when every hard filter holds; otherwise every reason, in filter order.
    pub feasibility: Result<(), Vec<String>>,
}

impl Candidate {
    pub(super) fn feasible(&self) -> bool {
        self.feasibility.is_ok()
    }
    /// The COLD sample this candidate came from.
    pub(super) fn sample(&self) -> usize {
        match self.source {
            CandidateSource::ColdSample(index) => index,
        }
    }
    /// The provenance projection: private, observational, never authority. The plan is
    /// recorded so an oracle can judge every candidate, not only the selected one.
    pub(super) fn to_json(&self, index: usize) -> Value {
        json!({
            "index": index,
            "source": match self.source {
                CandidateSource::ColdSample(sample) => json!({"kind": "cold_sample", "sample": sample}),
            },
            "signature": self.signature,
            "feasible": self.feasible(),
            "reasons": self.feasibility.as_ref().err().cloned().unwrap_or_default(),
            "plan": self.plan.to_json(),
        })
    }
}

/// A topology dimension the recalled candidates suggest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Dimension {
    pub name: &'static str,
    /// Recalled candidates whose pattern words name it.
    pub hits: Vec<String>,
    /// The request already fixes this dimension itself.
    pub fixed_by_request: bool,
    /// The assembler can express a variant in this dimension.
    pub expressible: bool,
}

impl Dimension {
    pub(super) fn to_json(&self) -> Value {
        json!({
            "dimension": self.name,
            "hits": self.hits,
            "fixed_by_request": self.fixed_by_request,
            "expressible": self.expressible,
        })
    }
}

/// What [`compose`] produced: the candidates and the dimension record.
pub(super) struct Composition {
    pub candidates: Vec<Candidate>,
    pub dimensions: Vec<Dimension>,
}

/// Compose the finite candidate set from the admissible COLD plans (`samples`, in
/// sample order), the deterministic reading (the floor) and the recalled candidates.
/// Two samples with the same signature and the same feasibility verdict are one
/// candidate; a feasible twin of an infeasible signature stays visible.
pub(super) fn compose(
    samples: &[(usize, Plan)],
    reading: &Reading,
    hits: &[Hit],
    intent: &str,
) -> Composition {
    let mut candidates: Vec<Candidate> = Vec::new();
    for (index, plan) in samples {
        if candidates.len() >= CAP {
            break;
        }
        let signature = signature(plan);
        let feasibility = feasibility(plan, &reading.plan, intent);
        if candidates
            .iter()
            .any(|c| c.signature == signature && c.feasibility.is_ok() == feasibility.is_ok())
        {
            continue;
        }
        candidates.push(Candidate {
            plan: plan.clone(),
            source: CandidateSource::ColdSample(*index),
            signature,
            feasibility,
        });
    }
    // A variant is composed only in a dimension the request leaves open AND the
    // assembler expresses. That set is empty today, so the dimensions are the record.
    Composition {
        candidates,
        dimensions: dimensions(hits, intent),
    }
}

fn dimensions(hits: &[Hit], intent: &str) -> Vec<Dimension> {
    let text = fold(intent);
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    DIMENSIONS
        .iter()
        .filter_map(|(name, patterns, fixers)| {
            let hits: Vec<String> = hits
                .iter()
                .filter(|hit| {
                    hit.patterns
                        .iter()
                        .any(|pattern| patterns.contains(&pattern.as_str()))
                })
                .map(|hit| hit.id.clone())
                .collect();
            if hits.is_empty() {
                return None;
            }
            Some(Dimension {
                name,
                hits,
                fixed_by_request: fixers
                    .iter()
                    .any(|phrase| words.windows(phrase.len()).any(|window| window == *phrase)),
                expressible: EXPRESSIBLE.contains(name),
            })
        })
        .collect()
}

/// Lowercase with French diacritics folded, for whole-word phrase matching.
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            other => other,
        })
        .collect()
}

/// The family of an effect verb: writing a file, moving money, or reaching out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    File,
    Money,
    Outbound,
}

fn family(verb: EffectVerb) -> Family {
    if verb == EffectVerb::Write {
        Family::File
    } else if verb.moves_money() {
        Family::Money
    } else {
        Family::Outbound
    }
}

/// Two targets naming the same URL, path or address.
fn shares_literal(a: &str, b: &str) -> bool {
    let literals = |t: &str| -> Vec<String> {
        t.split_whitespace()
            .map(|w| w.trim_end_matches(['.', ',', ';', ')', ':']).to_lowercase())
            .filter(|w| {
                w.starts_with("http://")
                    || w.starts_with("https://")
                    || w.starts_with("./")
                    || (w.contains('@') && w.contains('.'))
            })
            .collect()
    };
    let (a, b) = (literals(a), literals(b));
    a.iter().any(|x| b.contains(x))
}

/// The deterministic hard filters, in a fixed order so reasons are stable:
///
/// 1. anchored — every operation, effect and obligation carries a nonempty verbatim
///    excerpt of the request;
/// 2. nonempty — at least one operation or effect;
/// 3. settled — no unresolved requested work;
/// 4. floor operations — every operation the reading recognized is accounted for by a
///    candidate operation or effect whose excerpt overlaps its clause, or by a
///    candidate operation of the same kind (a plan merges same-kind operations and
///    keeps one excerpt): the model may re-read a clause as another operation; it may
///    not drop the operation;
/// 5. floor effects and policy floor — every effect the reading recognized is present
///    with its policy, or with `human_first` where the reading said `automatic`
///    (strengthening only): a human gate is never removed, a prohibition, an
///    indecision or a contradiction is never resolved by a model, and a policy
///    literal the reading found is kept;
/// 6. money gate — (retired 2026-09-22: an automatic money movement is the assembler's
///    closed approval question, `effect.<verb>.approval`, never an infeasibility);
/// 7. floor obligations — every obligation the reading recognized is present;
/// 8. literals carried — every literal the reading bound (URL, path, email, timezone)
///    is carried verbatim by some candidate operation or effect;
/// 9. no invented literal — every URL, path, email or number in a candidate detail or
///    target appears verbatim in the request;
/// 10. produced content — an effect that names content (a written file's content, a reply
///     sent, a report published, a note created…) has a draft, extract or compute step, or
///     a source step under a copy cue;
/// 11. recheckable revision — a revision check has a lookup to reread.
pub(super) fn feasibility(candidate: &Plan, floor: &Plan, intent: &str) -> Result<(), Vec<String>> {
    let mut why: Vec<String> = Vec::new();
    if !candidate.anchored(intent) {
        why.push(
            "an operation, effect or obligation lacks an exact nonempty excerpt of the request"
                .to_owned(),
        );
    }
    if candidate.steps.is_empty() && candidate.effects.is_empty() {
        why.push("names no operation and no effect".to_owned());
    }
    for unknown in &candidate.unknowns {
        why.push(format!("unresolved requested work: {unknown}"));
    }
    floor_operations(candidate, floor, intent, &mut why);
    floor_effects(candidate, floor, &mut why);
    for obligation in &floor.obligations {
        if !candidate
            .obligations
            .iter()
            .any(|o| o.kind == obligation.kind)
        {
            why.push(format!(
                "dropped the obligation `{}` ({})",
                obligation.kind.word(),
                obligation.evidence.trim()
            ));
        }
    }
    literals(candidate, floor, intent, &mut why);
    // Rule 12: a constraint needs an operation that carries it. Reads and writes carry no
    // prompt and no computation: a plan that keeps the constraint and drops every step that
    // could honour it drops the constraint silently.
    // A context sentence or a structure law binds no operation and is judged elsewhere.
    let carried_by_an_operation = candidate
        .constraints
        .iter()
        .filter(|c| !super::structure::binds_no_operation(c))
        .filter(|c| !restated(candidate, floor, c))
        .filter(|c| !carried_by_an_obligation(candidate, c))
        .count();
    if carried_by_an_operation > 0 && !candidate.steps.iter().any(|s| s.op.carries_constraints()) {
        why.push(format!(
            "{carried_by_an_operation} constraint(s) have no operation to carry them"
        ));
    }
    // Rule 14: an effect is asked by its excerpt. An excerpt that carries no word of the
    // effect's family (a file word or a path for a write, an outbound word or an endpoint
    // for a send, a money word for a refund) was read into the request, never out of it;
    // a prohibition is anchored by the ban it states.
    let columns = super::columns::columns_hint(intent);
    for effect in &candidate.effects {
        if effect.policy == EffectPolicy::Forbidden {
            continue;
        }
        let lower = effect.evidence.to_lowercase();
        // Only an excerpt the lexicon can read is judged: one whose effect words all belong
        // to another family, or whose only effect word is a listed column (`name, email e
        // city`). An excerpt in a language the lexicon does not read carries no verdict.
        let read = super::lexicon::effect_words(&lower, &[]);
        let words = super::lexicon::effect_words(&lower, &columns);
        let asked = read.is_empty()
            || words.iter().any(|w| family(*w) == family(effect.verb))
            || (effect.verb == EffectVerb::Write && super::objects::has_literal(&effect.evidence))
            || (family(effect.verb) == Family::Outbound
                && (lower.contains("http://")
                    || lower.contains("https://")
                    || lower.contains('@')));
        if !asked {
            why.push(format!(
                "`{}` is not asked by its excerpt ({})",
                effect.verb.word(),
                effect.evidence.trim()
            ));
        }
    }
    // Rule 13: gate dominance. An automatic effect on the clause or the endpoint of a
    // human-first effect performs the gated action before the gate the request demanded.
    for gated in candidate
        .effects
        .iter()
        .filter(|e| e.policy == EffectPolicy::HumanFirst)
    {
        for other in candidate
            .effects
            .iter()
            .filter(|e| e.policy == EffectPolicy::Automatic)
        {
            if other.evidence == gated.evidence || shares_literal(&other.target, &gated.target) {
                why.push(format!(
                    "`{}` is automatic on the clause or endpoint that gates `{}`",
                    other.verb.word(),
                    gated.verb.word()
                ));
            }
        }
    }
    // Rule 10: an effect that names content needs a step that produces it (the HOT law,
    // applied to every candidate: a proposal that kept the write, the send or the notify
    // and dropped the draft is not feasible).
    super::hot::unproduced_content(candidate, &mut why);
    // Rule 11: a revision check needs a lookup to reread.
    super::hot::unrecheckable_revision(candidate, &mut why);
    why.dedup();
    if why.is_empty() { Ok(()) } else { Err(why) }
}

/// Rule 4: a recognized operation may be re-read, never dropped. The reader is reliable on
/// the operations its cue table names unambiguously; a validation, a computation or an
/// exploration it guessed from an instruction ("fais relire", "compare") is advisory and
/// never vetoes a proposal on its own.
fn floor_operations(candidate: &Plan, floor: &Plan, intent: &str, why: &mut Vec<String>) {
    for step in floor
        .steps
        .iter()
        .filter(|s| !matches!(s.op, Op::Validate | Op::Compute | Op::Explore))
    {
        let accounted = candidate
            .steps
            .iter()
            .any(|s| same_family(s.op, step.op) || overlaps(&s.evidence, &step.evidence))
            || candidate
                .effects
                .iter()
                .any(|e| overlaps(&e.evidence, &step.evidence))
            || written_computation(candidate, step, intent);
        if !accounted {
            why.push(format!(
                "dropped the recognized operation `{}` ({})",
                step.op.word(),
                step.evidence.trim()
            ));
        }
    }
}

/// A `draft` the reader guessed over a write clause (« write just the number, nothing else,
/// to ./out/x.txt ») is the write of a computed value: a candidate that computes and writes
/// in the same sentence of the request accounts for it. Rule 10 still requires the written
/// content to be produced by a step, so a real draft dropped for a bare write stays refused.
fn written_computation(candidate: &Plan, step: &Step, intent: &str) -> bool {
    if step.op != Op::Draft || !candidate.has(Op::Compute) {
        return false;
    }
    let lower = intent.to_lowercase();
    let clause = step.evidence.trim().to_lowercase();
    super::lexicon::split_sentences(&lower)
        .into_iter()
        .filter(|sentence| sentence.contains(&clause))
        .any(|sentence| {
            candidate.effects.iter().any(|e| {
                e.verb == EffectVerb::Write
                    && super::paths::literals(&e.target)
                        .into_iter()
                        .chain(super::paths::literals(&e.evidence))
                        .any(|shape| match shape {
                            super::paths::PathShape::File(path) => {
                                sentence.contains(&path.to_lowercase())
                            }
                            _ => false,
                        })
            })
        })
}

/// The retrieval family: the reader's `search` over « retrouve le dossier dans `MongoDB` »
/// and a seat's `lookup` over the same words retrieve the same records; the reader's
/// word-level choice between them is no floor fact.
fn same_family(a: Op, b: Op) -> bool {
    a == b || matches!((a, b), (Op::Search, Op::Lookup) | (Op::Lookup, Op::Search))
}

/// A constraint that restates a clause the candidate or the reading already carries
/// elsewhere (an operation's or an effect's excerpt, an obligation's words, the trigger)
/// binds nothing new: the seat listed the clause twice, once as what it is and once as a
/// constraint, or restated a clause the reader recognized as an operation.
/// A constraint that restates a safeguard the plan carries as an obligation (« Déduplique
/// les événements entrants par leur identifiant ; pas de seconde action pour le même
/// événement » beside the dedup obligation « dédoublonne le callback par identifiant ») is
/// carried by that obligation's machinery — the admit task, the retry, the recheck — never
/// by a prompt.
fn carried_by_an_obligation(candidate: &Plan, constraint: &str) -> bool {
    let folded = super::shape::fold(constraint);
    candidate.obligations.iter().any(|o| {
        let cues: &[&str] = match o.kind.word() {
            "dedup" => &[
                "dedup",
                "dedoublonn",
                "dedupliq",
                "deduplic",
                "doublon",
                "duplicat",
                "duplicad",
                "doppelt",
                "duplikat",
            ],
            "revision_check" => &["version"],
            "retry_bound" => &[
                "tentative",
                "essai",
                "retry",
                "retries",
                "attempt",
                "versuch",
                "tentativ",
                "intento",
                "reintent",
            ],
            _ => &[],
        };
        cues.iter().any(|cue| folded.contains(cue))
    })
}

fn restated(candidate: &Plan, floor: &Plan, constraint: &str) -> bool {
    let fold = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let wanted = fold(constraint);
    let inside = |text: &str| {
        let text = fold(text);
        !text.is_empty() && (text.contains(&wanted) || wanted.contains(&text))
    };
    [candidate, floor].into_iter().any(|plan| {
        plan.steps.iter().any(|s| inside(&s.evidence))
            || plan
                .effects
                .iter()
                .any(|e| inside(&e.evidence) || inside(&e.target))
            || plan.obligations.iter().any(|o| inside(&o.evidence))
            || plan.trigger.as_deref().is_some_and(inside)
    })
}

/// Rule 5: every recognized effect stays, with its policy floor and its policy literal.
fn floor_effects(candidate: &Plan, floor: &Plan, why: &mut Vec<String>) {
    for effect in &floor.effects {
        let Some(found) = candidate.effects.iter().find(|e| e.verb == effect.verb) else {
            why.push(format!(
                "dropped the recognized effect `{}` ({})",
                effect.verb.word(),
                effect.evidence.trim()
            ));
            continue;
        };
        let strengthened =
            effect.policy == EffectPolicy::Automatic && found.policy == EffectPolicy::HumanFirst;
        if found.policy != effect.policy && !strengthened {
            if effect.policy == EffectPolicy::HumanFirst {
                why.push(format!(
                    "removed the human gate before `{}`",
                    effect.verb.word()
                ));
            } else {
                why.push(format!(
                    "changed the policy of `{}` from {} to {}",
                    effect.verb.word(),
                    effect.policy.word(),
                    found.policy.word()
                ));
            }
        }
        if let Some(literal) = &effect.policy_literal
            && found.policy_literal.as_ref() != Some(literal)
        {
            why.push(format!(
                "dropped the policy literal `{literal}` of `{}`",
                effect.verb.word()
            ));
        }
    }
}

/// Rules 8 and 9: every bound literal carried, no literal invented.
fn literals(candidate: &Plan, floor: &Plan, intent: &str, why: &mut Vec<String>) {
    // A money policy the reader bound is meaningful only beside an effect that moves money;
    // a threshold in a filter ("total above 120") is not a payment rule to carry.
    let money_effect = floor.effects.iter().any(|e| e.verb.moves_money());
    for binding in floor
        .bindings
        .iter()
        .filter(|b| b.role != "money_policy" || money_effect)
    {
        if !carries(candidate, binding) {
            why.push(format!(
                "the {} `{}` is no longer carried by any operation or effect",
                binding.role, binding.literal
            ));
        }
    }
    let intent_runs = digit_runs(intent);
    for (text, language) in candidate
        .steps
        .iter()
        .map(|s| (s.detail.as_str(), !matches!(s.op, Op::Compute)))
        .chain(candidate.effects.iter().map(|e| (e.target.as_str(), false)))
    {
        for token in literal_tokens(text) {
            let present = if token.bytes().all(|b| b.is_ascii_digit()) {
                intent_runs.contains(&token)
                    || stated_range_covers(intent, &token)
                    || number_word_covers(intent, &token)
                    // « the incident with the most minutes », « le plus long »: a superlative
                    // names one row, the `1` of a limit the seat wrote out.
                    || (token == "1" && superlative_covers(intent))
                    // « (cycle de correction 1) », « heading 2 »: an enumeration in a seat's
                    // paraphrase of a language step, never a value the workflow carries.
                    || (language && token.len() == 1)
            } else {
                intent.contains(token.as_str()) || derived_path(&token, intent)
            };
            if !present {
                why.push(format!("the literal `{token}` is not in the request"));
            }
        }
    }
}

fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    !a.is_empty() && !b.is_empty() && (a.contains(b) || b.contains(a))
}

/// A bound literal is carried when a candidate names it, or when the element that consumes
/// it in the assembler is present: a URL by a fetch, a path by a read or a write, an email
/// by an effect that addresses someone, a money policy by the effect that keeps the literal,
/// a timezone by any step. The reading seeds the bindings themselves, so listing a binding
/// is never enough on its own.
fn carries(plan: &Plan, binding: &Binding) -> bool {
    let literal = binding.literal.as_str();
    let named = plan
        .steps
        .iter()
        .any(|s| s.detail.contains(literal) || s.evidence.contains(literal))
        || plan
            .effects
            .iter()
            .any(|e| e.target.contains(literal) || e.evidence.contains(literal));
    let consumed = match binding.role {
        "url" => plan.has(Op::Fetch),
        "path" => plan.has(Op::Read) || plan.effects.iter().any(|e| e.verb == EffectVerb::Write),
        "email" => plan.effects.iter().any(|e| {
            matches!(
                e.verb,
                EffectVerb::Send | EffectVerb::Notify | EffectVerb::Create | EffectVerb::Update
            )
        }),
        "money_policy" => plan
            .effects
            .iter()
            .any(|e| e.policy_literal.as_deref() == Some(literal)),
        _ => !plan.steps.is_empty(),
    };
    named || consumed
}

/// The literal-looking tokens of a detail or target: whole URLs, paths and emails, and
/// each maximal digit run of a token that carries digits.
fn literal_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let token = word.trim_end_matches(['.', ',', ';', ':', ')', ']', '"', '\'']);
        if token.is_empty() {
            continue;
        }
        let path = token.starts_with("./") || (token.starts_with('/') && token.contains('.'));
        let url = token.starts_with("http://") || token.starts_with("https://");
        let email = token.contains('@') && token.contains('.') && !token.starts_with('@');
        if url || path || email {
            out.push(token.to_owned());
        } else if token.bytes().any(|b| b.is_ascii_digit()) {
            out.extend(digit_runs(token));
        }
    }
    out
}

/// A path the request spells with a placeholder (`./catalog/<slug>.md` beside the slugs it
/// lists) is derived, not invented: every component of the path appears verbatim in the
/// request. URLs and emails never qualify.
fn derived_path(token: &str, intent: &str) -> bool {
    let is_path = token.starts_with("./") || (token.starts_with('/') && token.contains('.'));
    if !is_path {
        return false;
    }
    // A glob star is structure, never a literal: `./recettes/*.md` is derived from « les .md
    // de ./recettes » once the folder and the extension both appear.
    let mut components = token
        .split(['/', '.', '-', '_'])
        .filter(|part| !part.is_empty() && !part.bytes().all(|b| b == b'*'))
        .peekable();
    components.peek().is_some()
        && components.all(|part| {
            intent.contains(part)
                || (part.bytes().all(|b| b.is_ascii_digit()) && stated_range_covers(intent, part))
        })
}

/// A digit the request spells as a word (« trois puces », « drei Zeilen », « tre punti ») is
/// derived from the request, not invented. One is never anchored this way: « un », « una »,
/// « um » and « ein » are articles far more often than counts.
fn number_word_covers(intent: &str, number: &str) -> bool {
    let Ok(n) = number.parse::<u32>() else {
        return false;
    };
    if n < 2 {
        return false;
    }
    super::shape::fold(intent)
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| {
            super::cardinality::NUMBER_WORDS
                .iter()
                .any(|(word, value)| *value == n && *word == w)
        })
}

/// Superlatives that name one row of a corpus (EN · FR · IT · ES · DE · PT, accented and
/// folded): the `1` a seat writes as a limit beside « the most », « le plus », « el mayor »
/// is stated by them.
const SUPERLATIVES: &[&str] = &[
    "most",
    "worst",
    "best",
    "highest",
    "lowest",
    "largest",
    "smallest",
    "biggest",
    "longest",
    "shortest",
    "latest",
    "earliest",
    "oldest",
    "newest",
    "least",
    "le plus",
    "la plus",
    "les plus",
    "le moins",
    "la moins",
    "il più",
    "la più",
    "il piu",
    "la piu",
    "il meno",
    "la meno",
    "el más",
    "la más",
    "el mas",
    "la mas",
    "el menos",
    "la menos",
    "mayor",
    "menor",
    "höchste",
    "hochste",
    "niedrigste",
    "größte",
    "grosste",
    "kleinste",
    "längste",
    "langste",
    "kürzeste",
    "kurzeste",
    "meisten",
    "wenigsten",
    "o mais",
    "a mais",
    "o maior",
    "a maior",
    "o menor",
    "a menor",
];

/// Whether the request states a superlative: one row of its corpus, the `1` of a limit.
fn superlative_covers(intent: &str) -> bool {
    let folded = super::shape::fold(intent);
    let padded = format!(" {folded} ");
    SUPERLATIVES
        .iter()
        .any(|w| padded.contains(&format!(" {w} ")))
}

/// Words and dashes that join the two ends of a stated numeric range.
const RANGE_LINKS: &[&str] = &[
    "to", "through", "thru", "à", "a", "au", "jusqu'à", "hasta", "bis", "fino a", "-", "–", "—",
    "…", "...", "..",
];

/// Whether the request states a numeric range that covers `number` ("01 to 04", "1 à 4",
/// "chapters 1-4", "fiche-01 … fiche-04"): two digit runs joined by a range word or dash.
/// A number inside a stated range is derived from the request, not invented.
fn stated_range_covers(intent: &str, number: &str) -> bool {
    let Ok(n) = number.parse::<u64>() else {
        return false;
    };
    let lower = intent.to_lowercase();
    let runs: Vec<(usize, usize)> = {
        let mut out = Vec::new();
        let mut start: Option<usize> = None;
        for (i, ch) in lower.char_indices() {
            match (ch.is_ascii_digit(), start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    out.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            out.push((s, lower.len()));
        }
        out
    };
    runs.windows(2).any(|pair| {
        let (a_start, a_end) = pair[0];
        let (b_start, b_end) = pair[1];
        let Some(between) = lower.get(a_end..b_start) else {
            return false;
        };
        let between = between.trim();
        let link = between
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| c == '`' || c == '"' || c == '\''))
            .collect::<Vec<_>>();
        let linked = between.len() <= 12
            && (RANGE_LINKS.contains(&between) || link.iter().any(|w| RANGE_LINKS.contains(w)));
        if !linked {
            return false;
        }
        match (
            lower
                .get(a_start..a_end)
                .and_then(|s| s.parse::<u64>().ok()),
            lower
                .get(b_start..b_end)
                .and_then(|s| s.parse::<u64>().ok()),
        ) {
            (Some(lo), Some(hi)) => lo <= n && n <= hi && hi.saturating_sub(lo) <= 64,
            _ => false,
        }
    })
}

fn digit_runs(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A plan's structural signature: operation words, effect verb:policy words, obligation words.
pub(super) fn signature(plan: &Plan) -> Vec<String> {
    let mut items: Vec<String> = plan
        .steps
        .iter()
        .map(|s| format!("op:{}", s.op.word()))
        .chain(
            plan.effects
                .iter()
                .map(|e| format!("effect:{}:{}", e.verb.word(), e.policy.word())),
        )
        .chain(
            plan.obligations
                .iter()
                .map(|o| format!("obligation:{}", o.kind.word())),
        )
        .collect();
    items.sort();
    items.dedup();
    items
}

/// Symmetric-difference similarity: shared items minus items only one side carries.
fn similarity(a: &[String], b: &[String]) -> isize {
    let shared = a.iter().filter(|item| b.contains(item)).count();
    let shared = isize::try_from(shared).unwrap_or(isize::MAX / 4);
    let (la, lb) = (
        isize::try_from(a.len()).unwrap_or(isize::MAX / 4),
        isize::try_from(b.len()).unwrap_or(isize::MAX / 4),
    );
    2 * shared - la - lb
}

/// Where distinct signatures differ: operations, effects, obligations.
pub(super) fn classify_disagreement(distinct: &[Vec<String>]) -> Vec<String> {
    let mut kinds = Vec::new();
    for prefix in ["op:", "effect:", "obligation:"] {
        let sets: Vec<Vec<&String>> = distinct
            .iter()
            .map(|sig| sig.iter().filter(|s| s.starts_with(prefix)).collect())
            .collect();
        if sets.windows(2).any(|w| w[0] != w[1]) {
            kinds.push(prefix.trim_end_matches(':').to_owned());
        }
    }
    kinds
}

/// The documented deterministic scorer for several feasible candidates without a seat:
/// the candidate closest to every accepted sample under the symmetric-difference
/// similarity (a medoid weighted by sample support, a candidate's own sample excluded);
/// a tie keeps the earliest candidate. Returns an index into `candidates`.
pub(super) fn rank(
    candidates: &[Candidate],
    feasible: &[usize],
    samples: &[(usize, Plan)],
) -> Option<usize> {
    let signatures: Vec<(usize, Vec<String>)> = samples
        .iter()
        .map(|(index, plan)| (*index, signature(plan)))
        .collect();
    feasible
        .iter()
        .copied()
        .filter_map(|k| {
            let candidate = candidates.get(k)?;
            let score: isize = signatures
                .iter()
                .filter(|(index, _)| *index != candidate.sample())
                .map(|(_, sig)| similarity(&candidate.signature, sig))
                .sum();
            Some((score, Reverse(k)))
        })
        .max()
        .map(|(_, Reverse(k))| k)
}

/// The seat's description of one feasible candidate: its signature and what differs
/// from the other feasible candidates.
pub(super) fn describe(candidates: &[Candidate], feasible: &[usize], k: usize) -> String {
    let Some(candidate) = candidates.get(k) else {
        return String::new();
    };
    let others: Vec<&Candidate> = feasible
        .iter()
        .filter(|index| **index != k)
        .filter_map(|index| candidates.get(*index))
        .collect();
    let mut text = format!("plan with {}", candidate.signature.join(", "));
    if others.is_empty() {
        return text;
    }
    let only_here: Vec<&String> = candidate
        .signature
        .iter()
        .filter(|item| !others.iter().all(|o| o.signature.contains(item)))
        .collect();
    let mut lacking: Vec<&String> = others
        .iter()
        .flat_map(|o| o.signature.iter())
        .filter(|item| !candidate.signature.contains(item))
        .collect();
    lacking.sort();
    lacking.dedup();
    if !only_here.is_empty() {
        text.push_str("; only here: ");
        text.push_str(
            &only_here
                .iter()
                .map(|item| item.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if !lacking.is_empty() {
        text.push_str("; lacking: ");
        text.push_str(
            &lacking
                .iter()
                .map(|item| item.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::super::plan::{Binding, Effect, EffectVerb, Obligation, ObligationKind, Op, Step};
    use super::super::retrieve::HitKind;
    use super::*;

    const INTENT: &str = "Fetch https://example.com/pricing, classify the page, then draft a note. Ask me before any refund of 100 EUR; never retry more than 3 times.";

    fn step(op: Op, detail: &str, evidence: &str) -> Step {
        Step::new(op, evidence, detail, Vec::new())
    }
    fn effect(verb: EffectVerb, target: &str, evidence: &str, policy: EffectPolicy) -> Effect {
        Effect::new(verb, target, evidence, policy)
    }
    /// The deterministic reading's floor: a literal fetch, an explicit classify, a gated
    /// refund with its policy literal, a retry bound and the URL binding.
    fn floor() -> Plan {
        let mut refund = effect(
            EffectVerb::Refund,
            "any refund of 100 EUR",
            "Ask me before any refund of 100 EUR",
            EffectPolicy::HumanFirst,
        );
        refund.policy_literal = Some("100 EUR".to_owned());
        let mut plan = Plan::default();
        plan.steps = vec![
            step(
                Op::Fetch,
                "https://example.com/pricing",
                "Fetch https://example.com/pricing",
            ),
            step(Op::Classify, "the page", "classify the page"),
        ];
        plan.effects = vec![refund];
        plan.obligations = vec![Obligation::new(
            ObligationKind::RetryBound(3),
            "never retry more than 3 times",
        )];
        plan.bindings = vec![Binding::new("url", "https://example.com/pricing")];
        plan
    }
    /// A faithful candidate: the floor plus the draft the model read.
    fn base() -> Plan {
        let mut plan = floor();
        plan.steps
            .push(step(Op::Draft, "a note", "then draft a note"));
        plan
    }
    fn reasons(candidate: &Plan) -> Vec<String> {
        feasibility(candidate, &floor(), INTENT)
            .err()
            .unwrap_or_default()
    }

    #[test]
    fn the_base_and_a_strengthened_reading_are_feasible() {
        assert_eq!(feasibility(&base(), &floor(), INTENT), Ok(()));
        // The model may re-read a recognized clause as another operation, never drop it.
        let mut reread = base();
        reread.steps[1].op = Op::Extract;
        assert_eq!(feasibility(&reread, &floor(), INTENT), Ok(()));
        // An automatic effect may be strengthened to human-first, never the reverse.
        let mut automatic_floor = floor();
        automatic_floor.effects[0].policy = EffectPolicy::Automatic;
        automatic_floor.effects[0].policy_literal = None;
        let mut strengthened = base();
        strengthened.effects[0].policy_literal = None;
        assert_eq!(feasibility(&strengthened, &automatic_floor, INTENT), Ok(()));
    }

    #[test]
    fn removing_a_recognized_operation_is_infeasible() {
        let mut candidate = base();
        candidate.steps.remove(0);
        let why = reasons(&candidate);
        assert!(
            why.iter().any(|r| r
                == "dropped the recognized operation `fetch` (Fetch https://example.com/pricing)"),
            "{why:?}"
        );
        assert!(
            why.iter().any(|r| r
                == "the url `https://example.com/pricing` is no longer carried by any operation or effect"),
            "{why:?}"
        );
    }

    #[test]
    fn adding_an_unanchored_operation_is_infeasible() {
        let mut candidate = base();
        candidate
            .steps
            .push(step(Op::Compute, "a score", "compute a score"));
        let why = reasons(&candidate);
        assert_eq!(
            why,
            ["an operation, effect or obligation lacks an exact nonempty excerpt of the request"]
        );
    }

    #[test]
    fn inventing_an_effect_is_infeasible() {
        let mut candidate = base();
        candidate.effects.push(effect(
            EffectVerb::Send,
            "the note",
            "send the note",
            EffectPolicy::Automatic,
        ));
        let why = reasons(&candidate);
        assert!(
            why.iter()
                .any(|r| r.contains("lacks an exact nonempty excerpt")),
            "{why:?}"
        );
        // An anchored money-moving effect whose gate the reading found is never feasible
        // without it; the money law itself is the assembler's approval question.
        let mut candidate = base();
        candidate.effects[0].policy = EffectPolicy::Automatic;
        let why = reasons(&candidate);
        assert!(
            why.iter()
                .any(|r| r == "removed the human gate before `refund`"),
            "{why:?}"
        );
        assert!(!why.iter().any(|r| r.contains("moves money")), "{why:?}");
    }

    #[test]
    fn dropping_or_resolving_a_recognized_effect_is_infeasible() {
        let mut candidate = base();
        candidate.effects.clear();
        assert!(
            reasons(&candidate).iter().any(|r| r
                == "dropped the recognized effect `refund` (Ask me before any refund of 100 EUR)"),
            "{:?}",
            reasons(&candidate)
        );
        let mut forbidden_floor = floor();
        forbidden_floor.effects[0].policy = EffectPolicy::Forbidden;
        forbidden_floor.effects[0].policy_literal = None;
        for resolved in [
            EffectPolicy::Automatic,
            EffectPolicy::HumanFirst,
            EffectPolicy::Undecided,
        ] {
            let mut candidate = base();
            candidate.effects[0].policy = resolved;
            candidate.effects[0].policy_literal = None;
            let why = feasibility(&candidate, &forbidden_floor, INTENT).unwrap_err();
            assert!(
                why.iter()
                    .any(|r| r.starts_with("changed the policy of `refund` from forbidden to")),
                "{why:?}"
            );
        }
        let mut candidate = base();
        candidate.effects[0].policy_literal = None;
        assert_eq!(
            reasons(&candidate),
            ["dropped the policy literal `100 EUR` of `refund`"]
        );
    }

    #[test]
    fn dropping_an_obligation_is_infeasible() {
        let mut candidate = base();
        candidate.obligations.clear();
        assert_eq!(
            reasons(&candidate),
            ["dropped the obligation `retry_bound` (never retry more than 3 times)"]
        );
        let mut candidate = base();
        candidate.obligations[0].kind = ObligationKind::RetryBound(5);
        assert_eq!(reasons(&candidate).len(), 1);
    }

    #[test]
    fn a_number_inside_a_stated_range_is_derived_not_invented() {
        let intent = "Résume les fiches ./fiches/fiche-01.md à fiche-04.md, chapters 1 to 4, then write ./out/x.md";
        assert!(stated_range_covers(intent, "02"));
        assert!(stated_range_covers(intent, "3"));
        assert!(!stated_range_covers(intent, "07"));
        assert!(derived_path("./fiches/fiche-03.md", intent));
        assert!(!derived_path("./fiches/fiche-09.md", intent));
        assert!(!stated_range_covers("write 150 words to ./out/a.md", "42"));
    }

    #[test]
    fn a_glob_star_and_a_number_word_are_derived_not_invented() {
        let intent =
            "Prends tous les .md de ./recettes et écris trois puces par recette dans ./out/menu.md";
        assert!(derived_path("./recettes/*.md", intent));
        assert!(!derived_path("./plats/*.md", intent));
        assert!(number_word_covers(intent, "3"));
        assert!(!number_word_covers(intent, "4"));
        // One is never anchored by an article.
        assert!(!number_word_covers("write a note, un peu longue", "1"));
        assert!(number_word_covers("schreib drei Zeilen", "3"));
        assert!(number_word_covers("scrivi tre punti", "3"));
        assert!(number_word_covers("escreve três linhas", "3"));
        assert!(number_word_covers("escribe cinco viñetas", "5"));
    }

    #[test]
    fn a_superlative_states_the_one_of_a_limit() {
        assert!(superlative_covers(
            "Under Worst incident, name the incident with the most minutes by its id"
        ));
        assert!(superlative_covers("garde l'incident le plus long"));
        assert!(superlative_covers("la línea con el mayor retraso"));
        assert!(superlative_covers("die Zeile mit den meisten Minuten"));
        assert!(!superlative_covers("write three lines to ./out/a.md"));
        assert!(!superlative_covers("almost every row"));
    }

    #[test]
    fn altering_a_literal_is_infeasible() {
        let mut candidate = base();
        candidate.steps[0].detail = "https://example.com/price".to_owned();
        let why = reasons(&candidate);
        // The evidence still carries the URL: only the rewritten detail is judged.
        assert_eq!(
            why,
            ["the literal `https://example.com/price` is not in the request"]
        );
        let mut candidate = base();
        candidate.steps[0].detail = "https://example.com/pricing/".to_owned();
        assert_eq!(
            reasons(&candidate),
            ["the literal `https://example.com/pricing/` is not in the request"]
        );
        let mut candidate = base();
        candidate.effects[0].target = "any refund of 1000 EUR".to_owned();
        assert_eq!(
            reasons(&candidate),
            ["the literal `1000` is not in the request"]
        );
        let mut candidate = base();
        candidate.steps[2].detail = "a note in 3 lines".to_owned();
        assert_eq!(reasons(&candidate), Vec::<String>::new());
    }

    #[test]
    fn unsettled_and_empty_candidates_are_infeasible() {
        let mut candidate = base();
        candidate.unknowns.push("a bypassed approval".to_owned());
        assert_eq!(
            reasons(&candidate),
            ["unresolved requested work: a bypassed approval"]
        );
        let empty = Plan::default();
        let why = feasibility(&empty, &Plan::default(), INTENT).unwrap_err();
        assert_eq!(why, ["names no operation and no effect"]);
    }

    fn reading(plan: Plan) -> Reading {
        let mut reading = Reading::default();
        reading.plan = plan;
        reading
    }
    fn hit(id: &str, patterns: &[&str]) -> Hit {
        Hit {
            id: id.to_owned(),
            kind: HitKind::Family,
            title: id.to_owned(),
            patterns: patterns.iter().map(|p| (*p).to_owned()).collect(),
            score: 1.0,
            skeleton: None,
            signature: None,
        }
    }

    #[test]
    fn compose_is_finite_deterministic_and_keeps_an_infeasible_twin_visible() {
        let mut dropped = base();
        dropped.steps.remove(0);
        let mut reread = base();
        reread.steps[1].op = Op::Extract;
        let samples: Vec<(usize, Plan)> = vec![
            (0, base()),
            (1, dropped.clone()),
            (2, base()),
            (3, reread.clone()),
            (4, dropped),
        ];
        let composition = compose(&samples, &reading(floor()), &[], INTENT);
        let sources: Vec<usize> = composition
            .candidates
            .iter()
            .map(Candidate::sample)
            .collect();
        assert_eq!(sources, [0, 1, 3]);
        let feasible: Vec<bool> = composition
            .candidates
            .iter()
            .map(Candidate::feasible)
            .collect();
        assert_eq!(feasible, [true, false, true]);
        assert!(composition.dimensions.is_empty());
        // The cap holds even when every sample is distinct.
        let many: Vec<(usize, Plan)> = (0..12)
            .map(|i| {
                let mut plan = base();
                if i > 0 {
                    plan.obligations.push(Obligation::new(
                        ObligationKind::RetryBound(u32::try_from(i).unwrap()),
                        "never retry more than 3 times",
                    ));
                    plan.steps[2].detail = format!("note {i}");
                }
                (i, plan)
            })
            .collect();
        assert!(
            compose(&many, &reading(Plan::default()), &[], INTENT)
                .candidates
                .len()
                <= CAP
        );
        let json = composition.candidates[1].to_json(1);
        assert_eq!(json["index"], 1);
        assert_eq!(json["source"], json!({"kind": "cold_sample", "sample": 1}));
        assert_eq!(json["feasible"], false);
        assert!(!json["reasons"].as_array().unwrap().is_empty());
        assert!(json["plan"]["operations"].is_array());
    }

    #[test]
    fn rank_prefers_the_support_weighted_medoid_and_breaks_ties_first() {
        let mut reread = base();
        reread.steps[1].op = Op::Extract;
        let samples: Vec<(usize, Plan)> = vec![(0, reread), (1, base()), (2, base())];
        let composition = compose(&samples, &reading(floor()), &[], INTENT);
        let feasible: Vec<usize> = (0..composition.candidates.len()).collect();
        assert_eq!(rank(&composition.candidates, &feasible, &samples), Some(1));
        // Equal support: the earliest candidate.
        let composition = compose(&samples[..2], &reading(floor()), &[], INTENT);
        let feasible: Vec<usize> = (0..composition.candidates.len()).collect();
        assert_eq!(
            rank(&composition.candidates, &feasible, &samples[..2]),
            Some(0)
        );
        assert_eq!(rank(&composition.candidates, &[], &samples), None);
        let text = describe(&composition.candidates, &feasible, 0);
        assert!(text.starts_with("plan with "), "{text}");
        assert!(text.contains("only here: op:extract"), "{text}");
        assert!(text.contains("lacking: op:classify"), "{text}");
        assert_eq!(
            classify_disagreement(&[
                composition.candidates[0].signature.clone(),
                composition.candidates[1].signature.clone()
            ]),
            ["op"]
        );
    }

    // ── rule 10 generalized · a consumer of produced content needs a producer ─────
    const SEND_INTENT: &str = "Read ./inbox/a.md and send the summary to ops@example.invalid.";
    const COPY_INTENT: &str =
        "Read ./inbox/a.md and forward the summary as is to ops@example.invalid.";

    fn send_floor(intent: &str, target: &str, evidence: &str) -> Plan {
        let mut plan = Plan::default();
        plan.steps = vec![step(Op::Read, "./inbox/a.md", "Read ./inbox/a.md")];
        plan.effects = vec![effect(
            EffectVerb::Send,
            target,
            evidence,
            EffectPolicy::Automatic,
        )];
        plan.bindings = vec![
            Binding::new("path", "./inbox/a.md"),
            Binding::new("email", "ops@example.invalid"),
        ];
        plan.tap(|plan| assert!(plan.anchored(intent)))
    }
    trait Tap: Sized {
        fn tap(self, f: impl FnOnce(&Self)) -> Self {
            f(&self);
            self
        }
    }
    impl Tap for Plan {}

    #[test]
    fn a_consumer_of_produced_content_needs_a_producer() {
        let floor = send_floor(
            SEND_INTENT,
            "the summary to ops@example.invalid",
            "send the summary to ops@example.invalid",
        );
        let why = feasibility(&floor, &floor, SEND_INTENT).unwrap_err();
        assert_eq!(
            why,
            ["`send` names content no step produces: summary (the summary to ops@example.invalid)"]
        );
        // A draft produces it.
        let mut drafted = floor.clone();
        drafted
            .steps
            .push(step(Op::Draft, "the summary", "the summary"));
        assert_eq!(feasibility(&drafted, &floor, SEND_INTENT), Ok(()));
        // A copy cue beside a source step forwards existing material: nothing to produce.
        let copied = send_floor(
            COPY_INTENT,
            "the summary as is to ops@example.invalid",
            "forward the summary as is to ops@example.invalid",
        );
        assert_eq!(feasibility(&copied, &copied, COPY_INTENT), Ok(()));
        // A copy cue with no source step still has nothing to send.
        let mut sourceless = copied.clone();
        sourceless.steps.clear();
        sourceless.bindings.retain(|b| b.role != "path");
        let why = feasibility(&sourceless, &sourceless, COPY_INTENT).unwrap_err();
        assert!(
            why.iter()
                .any(|r| r.starts_with("`send` names content no step produces: summary")),
            "{why:?}"
        );
        // A prohibited effect is never emitted, so it needs no producer.
        let mut forbidden = floor.clone();
        forbidden.effects[0].policy = EffectPolicy::Forbidden;
        assert_eq!(feasibility(&forbidden, &forbidden, SEND_INTENT), Ok(()));
    }

    // ── rule 11 · a revision check needs a source to recheck ──────────────────────
    const REVISION_INTENT: &str = "Read ./inbox/a.md, look up the customer record, and recheck the current version before the end.";

    #[test]
    fn a_revision_check_without_a_lookup_is_infeasible() {
        let mut floor = Plan::default();
        floor.steps = vec![step(Op::Read, "./inbox/a.md", "Read ./inbox/a.md")];
        floor.obligations = vec![Obligation::new(
            ObligationKind::RevisionCheck,
            "recheck the current version before the end",
        )];
        floor.bindings = vec![Binding::new("path", "./inbox/a.md")];
        let why = feasibility(&floor, &floor, REVISION_INTENT).unwrap_err();
        assert_eq!(
            why,
            ["the obligation `revision_check` has no retrievable source to recheck"]
        );
        floor.steps.push(step(
            Op::Lookup,
            "the customer record",
            "look up the customer record",
        ));
        assert_eq!(feasibility(&floor, &floor, REVISION_INTENT), Ok(()));
    }

    #[test]
    fn pattern_dimensions_are_recorded_never_composed() {
        let hits = [
            hit("A01", &["fetch", "summarize"]),
            hit("D02", &["extract", "fanout", "fanin"]),
            hit("I03", &["batch", "classify"]),
        ];
        let open = compose(&[(0, base())], &reading(floor()), &hits, INTENT);
        assert_eq!(open.candidates.len(), 1);
        assert_eq!(
            open.dimensions,
            [
                Dimension {
                    name: "fanout",
                    hits: vec!["D02".to_owned(), "I03".to_owned()],
                    fixed_by_request: false,
                    expressible: false,
                },
                Dimension {
                    name: "fanin",
                    hits: vec!["D02".to_owned()],
                    fixed_by_request: false,
                    expressible: false,
                },
            ]
        );
        let fixed = compose(
            &[(0, base())],
            &reading(floor()),
            &hits,
            "Pour chaque demande, agrège les résultats.",
        );
        assert!(fixed.dimensions.iter().all(|d| d.fixed_by_request));
        assert_eq!(
            open.dimensions[0].to_json(),
            json!({"dimension": "fanout", "hits": ["D02", "I03"], "fixed_by_request": false, "expressible": false})
        );
    }
}
