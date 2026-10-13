// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a request proves read alone, independent of any candidate: only what a small, finite
//! language proves, every span of it consumed and owned. The request is read exactly as the
//! strict HOT door reads it (apostrophes folded, the approval backstop applied, the stated rules
//! promoted) and must be admitted by that door; then the caller's own text, never the folded copy
//! the reader reads, must be one whole sentence of
//!
//! ```text
//! REQUEST := "read " SOURCE SEP RULE SEP WRITE ["."] | COPY
//! COPY    := "copy " SOURCE " as is to " TARGET ["."]                (a copy)
//! SEP     := ", " | ", and " | " and "
//! RULE    := "count the rows where " FIELD " is " VALUE          (a count)
//!          | "keep the rows where " FIELD " is " VALUE           (a filter)
//!          | "compute the " AGG " of the " FIELD " column" [" per " GROUP]   (a total)
//!          | "sort the rows by " FIELD [" ascending" | " descending"]       (a sort)
//!          | "keep the top " N " rows by " FIELD                 (a cut)
//! AGG     := "total" | "sum" | "average" | "minimum" | "maximum"
//! WRITE   := "write the count to " TARGET | "write it to " TARGET
//!          | "write the count as " LABEL " to " TARGET            (a count)
//!          | "write them to " TARGET                              (a filter, a sort, a cut)
//!          | "write the " AGG " to " TARGET | "write it to " TARGET    (a total)
//! ```
//!
//! Grammar words compare ASCII case-insensitively; SOURCE, TARGET, FIELD, VALUE, GROUP, LABEL
//! and AGG are identities kept byte for byte (AGG is also the total's name, as the reader names
//! it), N is ASCII digits naming a positive count, tokens are separated by exactly one space, and
//! any other form is outside the language, never transformed. One terminal period is the
//! sentence's punctuation, never part of TARGET: a name ending in its own period is not proven
//! by this syntax. The plan must agree byte for byte: its only read path is SOURCE, its only
//! rule's text is the RULE span (a total's may be the object of its « compute » head, as the
//! reader reads it) and its relation is exactly the clause's: a count or a filter tests exactly
//! `FIELD == "VALUE"` (and counts, for a count); a total keeps every row and aggregates FIELD
//! under AGG (per GROUP when stated); a sort orders every row by FIELD (ascending unless stated,
//! ties free); a cut keeps the N rows of FIELD's highest values (ties at the cut free). Its only
//! write targets TARGET; it holds nothing else. An identity the reader's apostrophe folding
//! alters therefore disagrees with the plan and proves nothing. The write production owns its
//! proof: the write of TARGET is unconditional (no condition slot exists), and a count's or a
//! total's name is free (no name slot) or the stated LABEL or AGG. A copy's plan holds exactly
//! one read of SOURCE and one automatic write of TARGET, two distinct files of a text suffix (a
//! bound on the production, never a proof of their encoding: only the host's whole receipt
//! proves a text), its path bindings and nothing else: its write is unconditional and holds
//! exactly the text of SOURCE ([`Requirement::CopyText`]). No list of words is consulted: a
//! request outside the language proves nothing, and [`contract_of`] keeps its `Unproven`,
//! `Unsupported` and `Naming::Unknown`.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::AggOp;
use nika_compile_reader::plan::{EffectPolicy, EffectVerb, Op, Plan};
use nika_compile_reader::rules::{Comparator, Junction, Rule};
use nika_compile_reader::{gates, hot, lexicon, paths, shape};

use super::pipeline::{Naming, Operand, Pipeline, Sort};
use super::requested::{contract_of, pipeline_of};
use super::{Contract, Format, Presence, Requirement, same_path};

/// The separators between the clauses, the longest first: a shorter one is never a choice where
/// a longer one stands.
const SEPARATORS: [&str; 3] = [", and ", " and ", ", "];

/// What the write clause of an admitted sentence writes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Written {
    /// The rows a filter, a sort or a cut keeps: `write them to`.
    Rows,
    /// A count, with no name slot: `write the count to`, `write it to`.
    Count,
    /// A count or a total under the name the request states, byte for byte: `write the count as
    /// LABEL to`, `write the AGG to`.
    Labelled(String),
    /// The source's own text, as is: `copy SOURCE as is to TARGET`.
    Copy,
    /// A total, with no name slot: `write it to`.
    Total,
}

/// One sentence of the admitted language, its identity spans owned.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Production {
    /// The path the read clause names.
    pub source: String,
    /// The rule clause, verbatim; empty for a copy.
    pub rule: String,
    /// The path the write clause names.
    pub target: String,
    /// What the write clause writes.
    pub written: Written,
}

/// What reading a request alone proves, independent of any candidate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Provenance {
    /// Why the strict HOT admission refuses the reading: empty when it admits it.
    pub rejections: Vec<String>,
    /// The sentence of the admitted language the whole request is, when it is one.
    pub production: Option<Production>,
}

impl Provenance {
    /// Whether the strict HOT admission takes the reading whole.
    #[must_use]
    pub fn admitted(&self) -> bool {
        self.rejections.is_empty()
    }
}

/// The plan the strict HOT door reads from `intent`, and what that reading proves. The reader
/// reads the apostrophe-folded copy; the sentence is matched over `intent`'s own bytes, so the
/// identities compared with the plan are the caller's.
#[must_use]
pub fn read_request(intent: &str) -> (Plan, Provenance) {
    let folded = lexicon::fold_apostrophes(intent);
    let mut reading = lexicon::read(&folded);
    gates::backstop(&folded, &mut reading.plan);
    shape::promote_stated_rules(&mut reading.plan, &folded);
    let mut rejections = reading.hot_rejections();
    rejections.extend(hot::rejections(&folded, &reading));
    let production = rejections
        .is_empty()
        .then(|| production(intent, &reading.plan))
        .flatten();
    let provenance = Provenance {
        rejections,
        production,
    };
    (reading.plan, provenance)
}

/// The contract of `intent` read alone, as the strict HOT door reads it, with what an admitted
/// sentence of the language proves for the output its write clause owns. `intent` reaches the
/// production unfolded, as in [`read_request`]. Pure and deterministic; nothing is read from a
/// candidate.
#[must_use]
pub fn contract_of_request(intent: &str, answers: &BTreeMap<String, String>) -> Contract {
    let (plan, provenance) = read_request(intent);
    let folded = lexicon::fold_apostrophes(intent);
    proven(contract_of(&plan, &folded, answers), &provenance)
}

/// The same contract with what `provenance` proves: nothing unless the reading is admitted and
/// is a sentence of the language, and then only for the output its write clause owns.
pub(super) fn proven(contract: Contract, provenance: &Provenance) -> Contract {
    let Some(production) = provenance
        .production
        .as_ref()
        .filter(|_| provenance.admitted())
    else {
        return contract;
    };
    let Contract {
        obligations,
        sources,
    } = contract;
    let obligations = obligations
        .into_iter()
        .map(|mut obligation| {
            let owned = obligation
                .target
                .as_ref()
                .is_some_and(|target| same_path(&target.path, &production.target));
            if let Requirement::Computed {
                source, pipeline, ..
            } = &mut obligation.requirement
                && owned
                && same_path(source, &production.source)
                && obligation.presence == Presence::Unproven
            {
                obligation.presence = Presence::Required;
                named(pipeline, &production.written);
            }
            if production.written == Written::Copy
                && owned
                && obligation.presence == Presence::Unproven
                && matches!(obligation.requirement, Requirement::Unsupported(_))
            {
                obligation.presence = Presence::Required;
                obligation.requirement = Requirement::CopyText {
                    source: production.source.clone(),
                };
            }
            obligation
        })
        .collect();
    Contract::new(obligations).with_sources(sources)
}

/// A count's or a total's name as the write clause gives it: free without a name slot, the
/// stated name with one.
fn named(pipeline: &mut Pipeline, written: &Written) {
    let [step] = pipeline.steps.as_mut_slice() else {
        return;
    };
    let [aggregate] = step.stages.aggregates.as_mut_slice() else {
        return;
    };
    if aggregate.naming != Naming::Unknown {
        return;
    }
    match written {
        Written::Count | Written::Total => aggregate.naming = Naming::Free,
        Written::Labelled(label) => {
            aggregate.name.clone_from(label);
            aggregate.naming = Naming::Stated;
        }
        Written::Rows | Written::Copy => {}
    }
}

/// What a rule clause states, its identities byte for byte.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind<'a> {
    /// The rows where FIELD is VALUE, kept.
    Filter { field: &'a str, value: &'a str },
    /// The rows where FIELD is VALUE, counted.
    Count { field: &'a str, value: &'a str },
    /// AGG of FIELD over every row, or per GROUP.
    Total {
        agg: &'a str,
        field: &'a str,
        group: Option<&'a str>,
    },
    /// Every row, ordered by FIELD.
    Sort { field: &'a str, descending: bool },
    /// The N rows of FIELD's highest values.
    Top { n: u32, field: &'a str },
}

/// The aggregate words of a total, each as the reader names its output.
const AGGREGATES: [&str; 5] = ["total", "sum", "average", "minimum", "maximum"];

/// One parsed sentence of the language: its identity spans, byte for byte.
struct Sentence<'a> {
    source: &'a str,
    kind: Kind<'a>,
    rule: &'a str,
    written: Written,
    target: &'a str,
}

/// `text` without its leading grammar word `word`, compared ASCII case-insensitively.
fn after<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let head = text.get(..word.len())?;
    if head.eq_ignore_ascii_case(word) {
        text.get(word.len()..)
    } else {
        None
    }
}

/// The leading run of `text` whose characters `accept` takes, and the rest.
fn run_of(text: &str, accept: fn(char) -> bool) -> (&str, &str) {
    let end = text.find(|c: char| !accept(c)).unwrap_or(text.len());
    text.split_at(end)
}

/// A leading identity word: an ASCII letter (or an underscore, when `underscore_first`), then
/// ASCII letters, digits or underscores; and the rest.
fn word(text: &str, underscore_first: bool) -> Option<(&str, &str)> {
    let (taken, rest) = run_of(text, |c| c.is_ascii_alphanumeric() || c == '_');
    let first = taken.chars().next()?;
    (first.is_ascii_alphabetic() || (underscore_first && first == '_')).then_some((taken, rest))
}

/// A leading path token: no whitespace and no comma; and the rest.
fn path(text: &str) -> Option<(&str, &str)> {
    let (taken, rest) = run_of(text, |c| !c.is_whitespace() && c != ',');
    (!taken.is_empty()).then_some((taken, rest))
}

/// `text` after one separator.
fn separated(text: &str) -> Option<&str> {
    SEPARATORS
        .iter()
        .find_map(|separator| after(text, separator))
}

/// The write clause's object and target, for a rule of `kind`.
fn write_clause<'a>(text: &'a str, kind: Kind<'_>) -> Option<(Written, &'a str)> {
    let rest = after(text, "write ")?;
    match kind {
        Kind::Filter { .. } | Kind::Sort { .. } | Kind::Top { .. } => {
            Some((Written::Rows, after(rest, "them to ")?))
        }
        Kind::Count { .. } => {
            if let Some(target) = after(rest, "the count to ").or_else(|| after(rest, "it to ")) {
                return Some((Written::Count, target));
            }
            let (label, rest) = word(after(rest, "the count as ")?, true)?;
            Some((Written::Labelled(label.to_owned()), after(rest, " to ")?))
        }
        Kind::Total { agg, .. } => {
            if let Some(target) = after(rest, "it to ") {
                return Some((Written::Total, target));
            }
            // The name the write states is the total's own: AGG again, byte for byte.
            let (name, rest) = word(after(rest, "the ")?, false)?;
            let target = after(rest, " to ").filter(|_| name == agg)?;
            Some((Written::Labelled(name.to_owned()), target))
        }
    }
}

/// `FIELD " is " VALUE` at the head of `text`, and the rest.
fn tested(text: &str) -> Option<(&str, &str, &str)> {
    let (field, rest) = word(text, true)?;
    let (value, rest) = word(after(rest, " is ")?, false)?;
    Some((field, value, rest))
}

/// The rule clause at the head of `clause`, its identities byte for byte, and the rest.
fn rule_clause(clause: &str) -> Option<(Kind<'_>, &str)> {
    if let Some(rest) = after(clause, "count the rows where ") {
        let (field, value, rest) = tested(rest)?;
        return Some((Kind::Count { field, value }, rest));
    }
    if let Some(rest) = after(clause, "keep the rows where ") {
        let (field, value, rest) = tested(rest)?;
        return Some((Kind::Filter { field, value }, rest));
    }
    if let Some(rest) = after(clause, "compute the ") {
        let (agg, rest) = word(rest, false).filter(|(agg, _)| AGGREGATES.contains(agg))?;
        let (field, rest) = word(after(rest, " of the ")?, true)?;
        let rest = after(rest, " column")?;
        let (group, rest) = after(rest, " per ").map_or(Some((None, rest)), |per| {
            word(per, true).map(|(group, rest)| (Some(group), rest))
        })?;
        return Some((Kind::Total { agg, field, group }, rest));
    }
    if let Some(rest) = after(clause, "sort the rows by ") {
        let (field, rest) = word(rest, true)?;
        let (descending, rest) = after(rest, " descending").map_or_else(
            || (false, after(rest, " ascending").unwrap_or(rest)),
            |rest| (true, rest),
        );
        return Some((Kind::Sort { field, descending }, rest));
    }
    let (digits, rest) = run_of(after(clause, "keep the top ")?, |c| c.is_ascii_digit());
    let n = digits.parse::<u32>().ok().filter(|n| *n > 0)?;
    let (field, rest) = word(after(rest, " rows by ")?, true)?;
    Some((Kind::Top { n, field }, rest))
}

/// The sentence of the language `intent` is, its identities kept byte for byte. One terminal
/// period is the sentence's own punctuation, never part of TARGET.
fn sentence(intent: &str) -> Option<Sentence<'_>> {
    let text = intent.strip_suffix('.').unwrap_or(intent);
    let (source, rest) = path(after(text, "read ")?)?;
    let clause = separated(rest)?;
    let (kind, rest) = rule_clause(clause)?;
    let rule = clause.get(..clause.len() - rest.len())?;
    let (written, rest) = write_clause(separated(rest)?, kind)?;
    let (target, rest) = path(rest)?;
    rest.is_empty().then_some(Sentence {
        source,
        kind,
        rule,
        written,
        target,
    })
}

/// Whether `rule`'s typed relation is exactly the sentence's clause: one step, no policy and no
/// projection, rename or deduplication, its identities byte for byte. A count or a filter tests
/// exactly `FIELD == "VALUE"` (no other spelling) and counts or not; a total, a sort and a cut
/// keep every row, a total aggregates FIELD under AGG (per GROUP when stated) with no rounding,
/// a sort orders by FIELD in the stated direction and a cut keeps FIELD's N highest, ties free.
fn typed(rule: &Rule, parsed: &Sentence<'_>) -> bool {
    let Ok(relation) = pipeline_of(rule, &BTreeMap::new()) else {
        return false;
    };
    let [step] = relation.steps.as_slice() else {
        return false;
    };
    let stages = &step.stages;
    let bare = relation.policies.is_empty()
        && stages.distinct_by.is_empty()
        && stages.columns.is_empty()
        && stages.number_columns.is_empty()
        && stages.renames.is_empty()
        && !stages.distinct;
    let tests = |field: &str, value: &str| match step.filter.tests.as_slice() {
        [test] => {
            step.filter.junction == Junction::And
                && test.field == field
                && test.comparator == Comparator::Eq
                && test.operand == Operand::Text(value.to_owned())
                && test.spellings.is_empty()
        }
        _ => false,
    };
    let every_row = step.filter.tests.is_empty();
    let aggregates = stages.aggregates.as_slice();
    let (group, sort, limit) = (
        stages.group_by.as_deref(),
        stages.sort.as_ref(),
        stages.limit,
    );
    let unshaped = group.is_none() && sort.is_none() && limit.is_none();
    let shaped = match parsed.kind {
        Kind::Filter { field, value } => tests(field, value) && aggregates.is_empty() && unshaped,
        Kind::Count { field, value } => {
            let counted =
                matches!(aggregates, [count] if count.op == AggOp::Count && count.field.is_none());
            tests(field, value) && counted && unshaped
        }
        Kind::Total {
            agg,
            field,
            group: per,
        } => {
            let totalled = matches!(aggregates, [total] if Some(total.op) == AggOp::from_word(agg)
                && total.field.as_deref() == Some(field)
                && total.name == agg
                && total.round.is_none());
            every_row && totalled && group == per && sort.is_none() && limit.is_none()
        }
        Kind::Sort { field, descending } => {
            let order = Sort::new(field, descending, false);
            every_row
                && aggregates.is_empty()
                && group.is_none()
                && sort == Some(&order)
                && limit.is_none()
        }
        Kind::Top { n, field } => {
            let order = Sort::new(field, true, false);
            every_row
                && aggregates.is_empty()
                && group.is_none()
                && sort == Some(&order)
                && limit == Some(n)
        }
    };
    bare && shaped
}

/// Whether the plan holds no unknown, constraint, obligation, slot or trigger.
fn quiet(plan: &Plan) -> bool {
    plan.unknowns.is_empty()
        && plan.constraints.is_empty()
        && plan.obligations.is_empty()
        && plan.slots.is_empty()
        && plan.trigger.is_none()
}

/// The copy sentence `intent` is: `copy SOURCE as is to TARGET`, its identities byte for byte.
fn copy_sentence(intent: &str) -> Option<(&str, &str)> {
    let text = intent.strip_suffix('.').unwrap_or(intent);
    let (source, rest) = path(after(text, "copy ")?)?;
    let (target, rest) = path(after(rest, " as is to ")?)?;
    rest.is_empty().then_some((source, target))
}

/// The copy of `source` to `target` over `plan`, when the plan holds exactly one read of the
/// source and one automatic write of the target, two distinct files of a text suffix, and
/// nothing else.
fn copy_production(source: &str, target: &str, plan: &Plan) -> Option<Production> {
    let [read] = plan.steps.as_slice() else {
        return None;
    };
    let [write] = plan.effects.as_slice() else {
        return None;
    };
    let text = |file: &str| Format::of_path(file) == Some(Format::Text);
    let bound = plan.bindings.iter().all(|binding| {
        binding.role == "path" && (binding.literal == source || binding.literal == target)
    });
    let shaped = quiet(plan)
        && plan.rules.is_empty()
        && bound
        && text(source)
        && text(target)
        && !same_path(source, target)
        && read.op == Op::Read
        && read.detail == source
        && write.verb == EffectVerb::Write
        && write.policy == EffectPolicy::Automatic
        && write.policy_literal.is_none()
        && !write.alone
        && paths::single_file(&write.target).as_deref() == Some(target);
    shaped.then(|| Production {
        source: source.to_owned(),
        rule: String::new(),
        target: target.to_owned(),
        written: Written::Copy,
    })
}

/// The sentence of the language `intent` is over `plan`, when the plan holds exactly its one
/// read, its one typed rule and its one automatic write, or exactly its copy, byte for byte.
pub(super) fn production(intent: &str, plan: &Plan) -> Option<Production> {
    if let Some((source, target)) = copy_sentence(intent) {
        return copy_production(source, target, plan);
    }
    let parsed = sentence(intent)?;
    let [read, compute] = plan.steps.as_slice() else {
        return None;
    };
    let [rule] = plan.rules.as_slice() else {
        return None;
    };
    let [write] = plan.effects.as_slice() else {
        return None;
    };
    // The reader keeps a total's clause as its rule, or the object of its « compute » head.
    let text = rule.text().trim();
    let total = matches!(parsed.kind, Kind::Total { .. });
    let stated = text == parsed.rule || (total && after(parsed.rule, "compute ") == Some(text));
    let shaped = quiet(plan)
        && read.op == Op::Read
        && read.detail == parsed.source
        && compute.op == Op::Compute
        && rule.verified_program().is_none()
        && !rule.lines()
        && !rule.summary()
        && stated
        && write.verb == EffectVerb::Write
        && write.policy == EffectPolicy::Automatic
        && write.policy_literal.is_none()
        && !write.alone
        && paths::single_file(&write.target).as_deref() == Some(parsed.target);
    (shaped && typed(rule, &parsed)).then(|| Production {
        source: parsed.source.to_owned(),
        rule: parsed.rule.to_owned(),
        target: parsed.target.to_owned(),
        written: parsed.written.clone(),
    })
}
