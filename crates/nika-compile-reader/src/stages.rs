// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The stages a request states in words beside or instead of a filter, each a closed form
//! over folded words: a count of the rows per column ("count the rows per client", "compte
//! les lignes par client"), an aggregate per column ("the total of the amount column per
//! client"), a sort with its key and direction ("sort the rows by amount descending"), a
//! top-N with its rank measure ("keep the 2 rows with the highest amount", "garde les 2
//! lignes au montant le plus élevé"), a projection of named fields ("keep only the id and
//! title of each ticket"), a removal of duplicates ("remove the duplicate lines") and a join
//! of several sources on one column ("merge them on the id column"). A form that lacks its
//! key, its measure, its direction, its field list or its column is `None`: the human is
//! asked, nothing is guessed. Every jq the shapes lower to was run on the engine first.

use super::aggregate::{self, AggOp, Aggregation, COLUMN_WORDS, ROW_WORDS, Shape};
use super::rule_cues::{COPULAS, NEGATIONS, RELATIVES};
use super::rule_tokens::{fold, hinted, section};
use super::rules::{Clause, Junction, Operand};
use std::sync::LazyLock;

/// The closed word tables of the stages, one `[name]` section each, words in their order
/// (data only: the grammar below reads them; `tests::tables_are_the_frozen_lists` pins them).
const STAGE_WORDS: &str = include_str!("../assets/stage_words.txt");

/// The words of the `[name]` section of `stage_words.txt`, in file order.
fn table(name: &str) -> Vec<&'static str> {
    section(STAGE_WORDS, name)
}

/// One word of a stated stage: as written, folded, and whether a comma followed it.
struct Word {
    original: String,
    folded: String,
    comma_after: bool,
}

/// A pronoun hyphenated to its verb ("fusionne-les", "compte-les"): split off as a word.
static HYPHENATED_PRONOUNS: LazyLock<Vec<&str>> = LazyLock::new(|| table("hyphenated_pronouns"));

fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        let comma_after = raw.ends_with(',');
        let core = raw.trim_end_matches([',', '.', ';', ':']);
        let lower = core.to_lowercase();
        let split_at = HYPHENATED_PRONOUNS
            .iter()
            .find(|s| lower.ends_with(*s) && lower.len() > s.len())
            .map(|s| core.len() - s.len());
        let parts: Vec<&str> =
            match split_at.and_then(|at| Some((core.get(..at)?, core.get(at + 1..)?))) {
                Some((stem, pronoun)) => vec![stem, pronoun],
                None => raw.split('\'').collect(),
            };
        let last = parts.len().saturating_sub(1);
        for (index, part) in parts.into_iter().enumerate() {
            let word = part.trim_matches(|c: char| {
                matches!(
                    c,
                    '.' | ','
                        | ';'
                        | ':'
                        | '('
                        | ')'
                        | '"'
                        | '`'
                        | '«'
                        | '»'
                        | '!'
                        | '?'
                        | '“'
                        | '”'
                        | '-'
                )
            });
            if word.is_empty() {
                continue;
            }
            out.push(Word {
                original: word.to_owned(),
                folded: fold(word),
                comma_after: comma_after && index == last,
            });
        }
    }
    out
}

static DETERMINERS: LazyLock<Vec<&str>> = LazyLock::new(|| table("determiners"));

/// A distributive word a tail may carry ("of each ticket").
static EACH: LazyLock<Vec<&str>> = LazyLock::new(|| table("each"));

/// A pronoun standing for the rows ("count them per client", "compte-les par client").
static PRONOUNS: LazyLock<Vec<&str>> = LazyLock::new(|| table("pronouns"));

static COUNT_VERBS: LazyLock<Vec<&str>> = LazyLock::new(|| table("count_verbs"));

/// One word that opens a grouping ("per client", "by client", "par client").
static GROUP_WORDS: LazyLock<Vec<&str>> = LazyLock::new(|| table("group_words"));
/// Two words that open a grouping ("for each client", "pour chaque client").
const GROUP_PHRASES: &[(&str, &str)] = &[
    ("for", "each"),
    ("for", "every"),
    ("pour", "chaque"),
    ("por", "cada"),
    ("per", "ogni"),
    ("per", "ciascun"),
    ("per", "ciascuna"),
    ("fur", "jede"),
    ("fur", "jeden"),
    ("fur", "jedes"),
];

static SORT_VERBS: LazyLock<Vec<&str>> = LazyLock::new(|| table("sort_verbs"));
/// The word between a sort and its key.
static BY_WORDS: LazyLock<Vec<&str>> = LazyLock::new(|| table("by_words"));
/// Words a sort clause may carry without changing it ("in descending order").
static SORT_FILLERS: LazyLock<Vec<&str>> = LazyLock::new(|| table("sort_fillers"));
static DESCENDING: LazyLock<Vec<&str>> = LazyLock::new(|| table("descending"));
static ASCENDING: LazyLock<Vec<&str>> = LazyLock::new(|| table("ascending"));

/// Words that lead a kept selection ("keep", "garde", "ne … que", "select").
static KEEP_LEADS: LazyLock<Vec<&str>> = LazyLock::new(|| table("keep_leads"));
static ONLY_WORDS: LazyLock<Vec<&str>> = LazyLock::new(|| table("only_words"));
/// Words a top-N clause may carry between its number and its measure.
static TOPN_FILLERS: LazyLock<Vec<&str>> = LazyLock::new(|| table("topn_fillers"));
/// The words an identity tail opens with (« same columns », « mêmes colonnes », « mismas
/// columnas »): the stage is complete, the rest states what it keeps by construction.
static IDENTITY_LEADS: LazyLock<Vec<&str>> = LazyLock::new(|| table("identity_leads"));
/// A rank word: the measure's highest values first (`true`) or lowest first (`false`).
static RANK_HIGH: LazyLock<Vec<&str>> = LazyLock::new(|| table("rank_high"));
static RANK_LOW: LazyLock<Vec<&str>> = LazyLock::new(|| table("rank_low"));

/// A word between two listed fields.
static LIST_WORDS: LazyLock<Vec<&str>> = LazyLock::new(|| table("list_words"));
/// Words that open the scope of a projection ("of each ticket", "de chaque ticket").
static TAIL_OPENERS: LazyLock<Vec<&str>> = LazyLock::new(|| table("tail_openers"));

static REMOVE_VERBS: LazyLock<Vec<&str>> = LazyLock::new(|| table("remove_verbs"));
static ANY_WORDS: LazyLock<Vec<&str>> = LazyLock::new(|| table("any_words"));
static DUPLICATE_WORDS: LazyLock<Vec<&str>> = LazyLock::new(|| table("duplicate_words"));
static UNIQUE_WORDS: LazyLock<Vec<&str>> = LazyLock::new(|| table("unique_words"));

/// The verbs of a rename, six languages ("rename", "renomme", "renombra", "rinomina",
/// "benenne", "renomeia"), unaccented as the folded word.
static RENAME_VERBS: LazyLock<Vec<&str>> = LazyLock::new(|| table("rename_verbs"));
/// The word between the old name and the new one ("rename country to region", "renomme
/// country en region", "renombra country a region", "rinomina country in region",
/// "benenne country in region um", "renomeia country para region").
static RENAME_TO: LazyLock<Vec<&str>> = LazyLock::new(|| table("rename_to"));
/// A particle a rename may end with ("benenne … um").
static RENAME_TAILS: LazyLock<Vec<&str>> = LazyLock::new(|| table("rename_tails"));

static JOIN_VERBS: LazyLock<Vec<&str>> = LazyLock::new(|| table("join_verbs"));
/// What a join may name before its column: the sources, never a third thing.
static JOIN_OBJECTS: LazyLock<Vec<&str>> = LazyLock::new(|| table("join_objects"));
static ON_WORDS: LazyLock<Vec<&str>> = LazyLock::new(|| table("on_words"));

fn folded(words: &[Word], at: usize) -> Option<&str> {
    words.get(at).map(|w| w.folded.as_str())
}

fn is(words: &[Word], at: usize, table: &[&str]) -> bool {
    folded(words, at).is_some_and(|w| table.contains(&w))
}

/// A function word that never names a column.
fn function_word(folded: &str) -> bool {
    DETERMINERS.contains(&folded)
        || EACH.contains(&folded)
        || COLUMN_WORDS.contains(&folded)
        || ROW_WORDS.contains(&folded)
        || PRONOUNS.contains(&folded)
        || LIST_WORDS.contains(&folded)
        || ONLY_WORDS.contains(&folded)
        || TAIL_OPENERS.contains(&folded)
        || GROUP_WORDS.contains(&folded)
        || BY_WORDS.contains(&folded)
        || ON_WORDS.contains(&folded)
        || RELATIVES.contains(&folded)
        || COPULAS.contains(&folded)
        || NEGATIONS.contains(&folded)
        || KEEP_LEADS.contains(&folded)
        || RANK_HIGH.contains(&folded)
        || RANK_LOW.contains(&folded)
        || DESCENDING.contains(&folded)
        || ASCENDING.contains(&folded)
}

/// A word that names a column: a hint column when a hint exists, else a name-shaped word
/// that is no function word of the grammar. The guard proves it exists at run time.
fn column(word: &Word, columns: &[String]) -> Option<String> {
    if !columns.is_empty() {
        return hinted(&word.original, columns);
    }
    let shaped = word
        .original
        .chars()
        .next()
        .is_some_and(char::is_alphabetic)
        && word
            .original
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'));
    (shaped && !function_word(&word.folded)).then(|| word.original.clone())
}

fn skip(words: &[Word], at: &mut usize, table: &[&str]) {
    while is(words, *at, table) {
        *at += 1;
    }
}

/// The grouping at the end of a clause ("… per client", "… pour chaque client"): where it
/// starts and the column it names. The tail after the group word is one column, with
/// determiners and a column word allowed around it.
fn split_group(words: &[Word], columns: &[String]) -> Option<(usize, String)> {
    for start in (0..words.len()).rev() {
        let width = if is(words, start, &GROUP_WORDS) {
            1
        } else if GROUP_PHRASES
            .iter()
            .any(|(a, b)| folded(words, start) == Some(*a) && folded(words, start + 1) == Some(*b))
        {
            2
        } else {
            continue;
        };
        let mut at = start + width;
        skip(words, &mut at, &DETERMINERS);
        skip(words, &mut at, COLUMN_WORDS);
        let name = column(words.get(at)?, columns)?;
        at += 1;
        skip(words, &mut at, COLUMN_WORDS);
        return (at == words.len() && start > 0).then_some((start, name));
    }
    None
}

/// A count of the rows ("count the rows", "compte les lignes") or an aggregate over a column
/// ("the total of the amount column"), optionally per column. The verb form names its
/// output `count`; the noun form keeps the word the request wrote.
fn count_or_aggregate(words: &[Word], columns: &[String]) -> Option<Shape> {
    let (head, group) = match split_group(words, columns) {
        Some((start, name)) => (words.get(..start)?, Some(name)),
        None => (words, None),
    };
    let aggregation = if is(head, 0, &COUNT_VERBS) {
        let mut at = 1;
        skip(head, &mut at, &DETERMINERS);
        if !(is(head, at, ROW_WORDS) || is(head, at, &PRONOUNS)) || at + 1 != head.len() {
            return None;
        }
        Aggregation {
            field: None,
            op: AggOp::Count,
            name: "count".to_owned(),
            round: None,
        }
    } else {
        let text = head
            .iter()
            .map(|w| w.original.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        aggregate::stated(&text, columns)?
    };
    Some(Shape {
        group_by: group,
        aggregations: vec![aggregation],
        ..Shape::default()
    })
}

/// A count or an aggregate the words before a relative clause state over the rows it keeps
/// (R4 F1): « count the rows where … », « the number of rows whose … », « the total of the
/// amount column where … », read whole by the same forms. The noun the rows are named by
/// (« count the orders where … ») stands for them in a count, as a row word does.
pub(crate) fn lead_stage(text: &str, columns: &[String]) -> Option<Shape> {
    let mut words = words(text);
    if let Some(shape) = count_or_aggregate(&words, columns) {
        return Some(shape);
    }
    let noun = words.last_mut()?;
    if function_word(&noun.folded) || column(noun, &[]).is_none() {
        return None;
    }
    "rows".clone_into(&mut noun.original);
    "rows".clone_into(&mut noun.folded);
    count_or_aggregate(&words, columns)
        .filter(|shape| shape.aggregations.iter().all(|a| a.op == AggOp::Count))
}

/// A word that states a stage of its own (a count, a sort, a rank, a removal of duplicates, a
/// join, a rename): never a word the lead of a filter drops (R4 F1).
pub(crate) fn operation_word(folded: &str) -> bool {
    [
        &COUNT_VERBS,
        &SORT_VERBS,
        &RANK_HIGH,
        &RANK_LOW,
        &REMOVE_VERBS,
        &DUPLICATE_WORDS,
        &UNIQUE_WORDS,
        &JOIN_VERBS,
        &RENAME_VERBS,
    ]
    .iter()
    .any(|table| table.contains(&folded))
}

/// A word the lead of a filter carries without stating anything (a keep verb, « only », a
/// determiner): after the clause's own verb, the lead holds only these up to the rows' noun.
pub(crate) fn lead_word(folded: &str) -> bool {
    KEEP_LEADS.contains(&folded) || ONLY_WORDS.contains(&folded) || DETERMINERS.contains(&folded)
}

/// One step a reading builds (R4 F5): a filter, then stages in the fixed per-step lowering
/// order; the first becomes the rule's own filter and shape, every later one a `Then`.
#[derive(Default)]
pub(crate) struct Step {
    pub(crate) clauses: Vec<Clause>,
    pub(crate) junction: Option<Junction>,
    pub(crate) shape: Shape,
}

/// The ranks of a shape's stages in `Shape::lower`'s fixed order.
fn ranks(s: &Shape) -> Vec<u8> {
    [
        (s.join_on.is_some(), 0),
        (!s.distinct_by.is_empty(), 1),
        (s.group_by.is_some() || !s.aggregations.is_empty(), 2),
        (!s.derived.is_empty(), 3),
        (s.sort_by.is_some(), 4),
        (s.limit.is_some(), 5),
        (!s.columns.is_empty(), 6),
        (!s.renames.is_empty(), 7),
        (s.distinct, 8),
    ]
    .into_iter()
    .filter_map(|(present, rank)| present.then_some(rank))
    .collect()
}

/// The source columns a stage reads.
fn stage_reads(s: &Shape) -> Vec<&str> {
    let sort = s.sort_by.iter().map(|(field, _)| field.as_str());
    let fields = s.aggregations.iter().filter_map(|a| a.field.as_deref());
    let keys = s.distinct_by.iter().chain(&s.columns).map(String::as_str);
    s.group_by
        .as_deref()
        .into_iter()
        .chain(sort)
        .chain(fields)
        .chain(keys)
        .collect()
}

/// Whether a later step can read `reads` from the rows `s` wrote: source rows after a filter,
/// a sort, a cut or duplicates removed; the kept columns after a projection; the key alone
/// after a grouping (a produced value is no column the guard can check); nothing after
/// totals, a rename or a derived value (R4 F5).
fn rows_read(s: &Shape, reads: &[&str]) -> bool {
    let closed = s.is_totals() || !s.renames.is_empty() || !s.derived.is_empty();
    let kept = |r: &&str| {
        s.group_by.as_deref().is_none_or(|key| key == *r)
            && (s.columns.is_empty() || s.columns.iter().any(|c| c == r))
    };
    !closed && reads.iter().all(kept)
}

/// Where a segment's clauses go (R4 F5). They join the last step when a row filter reading
/// `reads` keeps the same rows before its stages as after them: after a join (which always
/// runs first), a stable sort with no cut, whole-row duplicates removed, a projection keeping
/// the read columns, or a grouping on the one read key. Otherwise they open a step on the rows
/// the last step wrote, or the text is not read (`None`): never a filter moved before a cut, a
/// total or a grouping it does not commute with.
pub(crate) fn place_clauses(
    steps: &mut Vec<Step>,
    clauses: Vec<Clause>,
    junction: Option<Junction>,
) -> Option<()> {
    let reads: Vec<&str> = clauses
        .iter()
        .flat_map(|c| {
            let other = match &c.value {
                Operand::Column(other) => Some(other.as_str()),
                _ => None,
            };
            std::iter::once(c.field.as_str()).chain(other)
        })
        .filter(|f| *f != ".")
        .collect();
    let last = steps.last_mut()?;
    let s = &last.shape;
    let readable = rows_read(s, &reads);
    let before = s.limit.is_none() && s.distinct_by.is_empty() && s.derived.is_empty();
    if !(before && readable) {
        if !readable {
            return None;
        }
        steps.push(Step {
            clauses,
            junction,
            shape: Shape::default(),
        });
        return Some(());
    }
    let or = |j: Option<Junction>| j == Some(Junction::Or);
    if !last.clauses.is_empty() && (or(junction) || or(last.junction)) {
        return None;
    }
    if last.clauses.is_empty() {
        last.junction = junction;
    }
    for clause in clauses {
        if !last.clauses.contains(&clause) {
            last.clauses.push(clause);
        }
    }
    Some(())
}

/// Where a stage goes (R4 F5). It joins the last step when `Shape::lower` runs it after every
/// stage already there over names still readable, or when a sort or a cut follows a projection
/// keeping its key (a projection is row-wise); otherwise it opens a step on the rows the last
/// step wrote. The same stage twice, or a join after the first step, is not read (`None`).
pub(crate) fn place_stage(steps: &mut Vec<Step>, stage: Shape) -> Option<()> {
    let last = steps.last_mut()?;
    let (before, after) = (ranks(&last.shape), ranks(&stage));
    if before.iter().any(|r| after.contains(r)) {
        return None;
    }
    let reads = stage_reads(&stage);
    let s = &last.shape;
    let produced = s.produced();
    let readable = !s.is_totals()
        && (s.group_by.is_none() || reads.iter().all(|r| produced.contains(r)))
        && (s.columns.is_empty() || reads.iter().all(|r| s.columns.iter().any(|c| c == r)));
    let in_order = before.iter().max() < after.iter().min();
    let row_wise = after.iter().all(|r| matches!(r, 4 | 5)) && before.iter().all(|r| *r == 6);
    if readable
        && (in_order || row_wise)
        && let Some(merged) = s.clone().merge(stage.clone())
    {
        last.shape = merged;
        return Some(());
    }
    if stage.join_on.is_some() || !rows_read(s, &reads) {
        return None;
    }
    steps.push(Step {
        shape: stage,
        ..Step::default()
    });
    Some(())
}

fn direction(folded: &str) -> Option<bool> {
    if DESCENDING.contains(&folded) {
        Some(true)
    } else if ASCENDING.contains(&folded) {
        Some(false)
    } else {
        None
    }
}

/// A sort with its key ("sort the rows by amount descending", "trie les lignes par montant
/// décroissant"): one column after a `by` word, at most one direction; without a direction
/// the sort is ascending, which is what the word means. Without a key, `None`.
fn sort(words: &[Word], columns: &[String]) -> Option<Shape> {
    if !is(words, 0, &SORT_VERBS) {
        return None;
    }
    let mut at = 1;
    skip(words, &mut at, &DETERMINERS);
    if is(words, at, ROW_WORDS) || is(words, at, &PRONOUNS) {
        at += 1;
    }
    let mut by = false;
    let mut field = None;
    let mut descending = None;
    for word in words.get(at..)? {
        let folded = word.folded.as_str();
        if BY_WORDS.contains(&folded) {
            by = true;
            continue;
        }
        if let Some(d) = direction(folded) {
            if descending.is_some() {
                return None;
            }
            descending = Some(d);
            continue;
        }
        if SORT_FILLERS.contains(&folded)
            || DETERMINERS.contains(&folded)
            || COLUMN_WORDS.contains(&folded)
        {
            continue;
        }
        if field.is_some() {
            return None;
        }
        field = Some(column(word, columns)?);
    }
    if !by {
        return None;
    }
    Some(Shape {
        sort_by: Some((field?, descending.unwrap_or(false))),
        ..Shape::default()
    })
}

fn rank(folded: &str) -> Option<bool> {
    if RANK_HIGH.contains(&folded) {
        Some(true)
    } else if RANK_LOW.contains(&folded) {
        Some(false)
    } else {
        None
    }
}

/// A count stated as digits or as a number word (« five », « cinq », « fünf »).
fn count(folded: &str) -> Option<u32> {
    folded
        .parse::<u32>()
        .ok()
        .or_else(|| super::lexicon::number_word(folded))
}

/// The words that lead a measure after an entity noun (« issues by rating », « corredores
/// por tiempo », « Kunden nach Umsatz »).
static MEASURE_LEADS: LazyLock<Vec<&str>> = LazyLock::new(|| table("measure_leads"));

/// The first N rows by a measure ("keep the 2 rows with the highest amount", "the top 3 rows
/// by amount", "garde les 2 lignes au montant le plus élevé"): a number, a row noun, one
/// column and a rank word saying which end comes first. "keep the best rows" has no number
/// and no measure; "keep the 2 rows by amount" has no rank: both `None`.
fn top_n(words: &[Word], columns: &[String]) -> Option<Shape> {
    let mut at = 0;
    let mut descending = None;
    while let Some(word) = words.get(at) {
        let folded = word.folded.as_str();
        if count(folded).is_some() {
            break;
        }
        if folded == "top" {
            descending = Some(true);
        } else if !(KEEP_LEADS.contains(&folded)
            || ONLY_WORDS.contains(&folded)
            || DETERMINERS.contains(&folded))
        {
            return None;
        }
        at += 1;
    }
    let n: u32 = count(folded(words, at)?)?;
    if n == 0 {
        return None;
    }
    at += 1;
    if let Some(d) = folded(words, at).and_then(rank) {
        if descending.is_some_and(|x| x != d) {
            return None;
        }
        descending = Some(d);
        at += 1;
    }
    // A row noun (« rows », « lignes »), or the entity the rows are (« issues », « clients »)
    // when a measure follows it (« by rating », « par montant »).
    let entity = !is(words, at, ROW_WORDS)
        && folded(words, at).is_some_and(|w| w.chars().all(char::is_alphabetic))
        && folded(words, at + 1).is_some_and(|w| MEASURE_LEADS.contains(&w));
    if !(is(words, at, ROW_WORDS) || entity) {
        return None;
    }
    at += 1;
    let mut field = None;
    for word in words.get(at..)? {
        let folded = word.folded.as_str();
        if IDENTITY_LEADS.contains(&folded) && field.is_some() {
            break;
        }
        if let Some(d) = rank(folded) {
            if descending.is_some_and(|x| x != d) {
                return None;
            }
            descending = Some(d);
            continue;
        }
        if TOPN_FILLERS.contains(&folded)
            || DETERMINERS.contains(&folded)
            || COLUMN_WORDS.contains(&folded)
        {
            continue;
        }
        if field.is_some() {
            return None;
        }
        field = Some(column(word, columns)?);
    }
    Some(Shape {
        sort_by: Some((field?, descending?)),
        limit: Some(n),
        ..Shape::default()
    })
}

/// A projection ("keep only the id and title of each ticket", "ne garde que les champs id et
/// titre", "keep only the columns id, title and status"): the listed names are the columns
/// written, in order. One name alone needs a column word before it or a scope after it;
/// "keep only the important fields" lists nothing the grammar can name.
fn projection(words: &[Word], columns: &[String]) -> Option<Shape> {
    let mut at = 0;
    skip(words, &mut at, &KEEP_LEADS);
    if at == 0 {
        return None;
    }
    skip(words, &mut at, &ONLY_WORDS);
    skip(words, &mut at, &DETERMINERS);
    let column_word_before = is(words, at, COLUMN_WORDS);
    if column_word_before {
        at += 1;
    }
    let mut fields = Vec::new();
    loop {
        let word = words.get(at)?;
        fields.push(column(word, columns)?);
        at += 1;
        if is(words, at, &LIST_WORDS) {
            at += 1;
        } else if !word.comma_after {
            break;
        }
    }
    if is(words, at, COLUMN_WORDS) {
        at += 1;
    }
    let mut scoped = false;
    if is(words, at, &TAIL_OPENERS) {
        at += 1;
        skip(words, &mut at, &EACH);
        skip(words, &mut at, &DETERMINERS);
        // The scope noun ("ticket") names the records, never a column: name-shaped is enough.
        column(words.get(at)?, &[])?;
        at += 1;
        scoped = true;
    }
    if at != words.len() || (fields.len() < 2 && !(column_word_before || scoped)) {
        return None;
    }
    Some(Shape {
        columns: fields,
        ..Shape::default()
    })
}

/// A removal of duplicates ("remove the duplicate lines", "supprime les lignes en double",
/// "keep only the unique rows"): the rows kept are the first occurrences, in place.
fn dedup(words: &[Word]) -> Option<Shape> {
    let distinct = || Shape {
        distinct: true,
        ..Shape::default()
    };
    let mut at = 0;
    if is(words, 0, &KEEP_LEADS) {
        skip(words, &mut at, &KEEP_LEADS);
        skip(words, &mut at, &ONLY_WORDS);
        skip(words, &mut at, &DETERMINERS);
        if !is(words, at, &UNIQUE_WORDS) {
            return None;
        }
        at += 1;
        return (is(words, at, ROW_WORDS) && at + 1 == words.len()).then(distinct);
    }
    if !is(words, 0, &REMOVE_VERBS) {
        return None;
    }
    at = 1;
    skip(words, &mut at, &DETERMINERS);
    skip(words, &mut at, &ANY_WORDS);
    skip(words, &mut at, &DETERMINERS);
    if is(words, at, &DUPLICATE_WORDS) {
        at += 1;
        if is(words, at, ROW_WORDS) {
            at += 1;
        }
        return (at == words.len()).then(distinct);
    }
    if !is(words, at, ROW_WORDS) {
        return None;
    }
    at += 1;
    let pair = (folded(words, at), folded(words, at + 1));
    if is(words, at, &DUPLICATE_WORDS) {
        at += 1;
    } else if pair == (Some("en"), Some("double")) || pair == (Some("in"), Some("doppio")) {
        at += 2;
    } else {
        return None;
    }
    (at == words.len()).then(distinct)
}

/// A join of the read sources on one column ("merge them on the id column", "fusionne-les
/// sur la colonne id"): the column is the key; the sources are what the request read.
/// "merge them" names no key: `None`.
fn join(words: &[Word], columns: &[String]) -> Option<Shape> {
    if !is(words, 0, &JOIN_VERBS) {
        return None;
    }
    let mut at = 1;
    while let Some(word) = folded(words, at) {
        if ON_WORDS.contains(&word) {
            break;
        }
        if !(JOIN_OBJECTS.contains(&word) || DETERMINERS.contains(&word)) {
            return None;
        }
        at += 1;
    }
    if !is(words, at, &ON_WORDS) {
        return None;
    }
    at += 1;
    skip(words, &mut at, &DETERMINERS);
    skip(words, &mut at, COLUMN_WORDS);
    let name = column(words.get(at)?, columns)?;
    at += 1;
    skip(words, &mut at, COLUMN_WORDS);
    (at == words.len()).then(|| Shape {
        join_on: Some(name),
        ..Shape::default()
    })
}

/// A join of the read sources that names no column ("merge them", "fusionne les deux
/// fichiers"): the operation is clear, its key is not. The reader keeps such a clause
/// unresolved (the human names the column) instead of an external merge effect.
pub(crate) fn join_without_key(text: &str) -> bool {
    let words = words(text);
    is(&words, 0, &JOIN_VERBS)
        && words.len() >= 2
        && words.get(1..).unwrap_or_default().iter().all(|w| {
            JOIN_OBJECTS.contains(&w.folded.as_str()) || DETERMINERS.contains(&w.folded.as_str())
        })
}

/// A rename of one column ("rename the country column to region", "renomme la colonne
/// country en region", "benenne die Spalte country in region um"): the old name is a column
/// of the hint or a name-shaped word, the new name a name-shaped word the request states;
/// nothing else may follow. Without both names, `None`: the human is asked.
fn rename(words: &[Word], columns: &[String]) -> Option<Shape> {
    if !is(words, 0, &RENAME_VERBS) {
        return None;
    }
    let mut at = 1;
    skip(words, &mut at, &DETERMINERS);
    skip(words, &mut at, COLUMN_WORDS);
    let from = column(words.get(at)?, columns)?;
    at += 1;
    skip(words, &mut at, COLUMN_WORDS);
    if !is(words, at, &RENAME_TO) {
        return None;
    }
    at += 1;
    skip(words, &mut at, &DETERMINERS);
    skip(words, &mut at, COLUMN_WORDS);
    // The new name is what the request writes: never hinted, name-shaped.
    let to = column(words.get(at)?, &[])?;
    at += 1;
    skip(words, &mut at, COLUMN_WORDS);
    skip(words, &mut at, &RENAME_TAILS);
    (at == words.len() && from != to).then(|| Shape {
        renames: vec![(from, to)],
        ..Shape::default()
    })
}

/// The stage one whole segment states, or `None` when no closed form reads it whole.
pub(crate) fn stated(text: &str, columns: &[String]) -> Option<Shape> {
    let words = words(text);
    if words.is_empty() {
        return None;
    }
    count_or_aggregate(&words, columns)
        .or_else(|| sort(&words, columns))
        .or_else(|| top_n(&words, columns))
        .or_else(|| projection(&words, columns))
        .or_else(|| dedup(&words))
        .or_else(|| join(&words, columns))
        .or_else(|| rename(&words, columns))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stage tables before they became data (stages.rs at 4bddf8a14, sha256 669a2916…,
    /// extracted mechanically): the asset holds exactly these words, in this order, one
    /// section each, and every table reads its own section. No word was added or dropped.
    const FROZEN: &[(&str, &str)] = &[
        ("hyphenated_pronouns", "-les -la -le -lo -li -moi -nous"),
        (
            "determiners",
            "the a an le la les l un une el los las il lo i gli die der das den dem its their leur leurs ses sa son sus su all tous toutes tutti tutte todos todas alle",
        ),
        (
            "each",
            "each every chaque chacun chacune ogni ciascun ciascuna cada jede jeden jedes jeder",
        ),
        ("pronouns", "them les las los li le sie"),
        (
            "count_verbs",
            "count counts compte comptez compter cuenta contar conta contare zahle zahl zahlen",
        ),
        ("group_words", "per by par por pro"),
        (
            "sort_verbs",
            "sort sorts order rank trie triez trier ordonne ordonnez ordonner ordena ordenar ordina ordinare sortiere sortieren",
        ),
        ("by_words", "by par por per secondo nach"),
        (
            "sort_fillers",
            "in en order ordre orden ordine reihenfolge of de du des del della von dans value values valeur valeurs",
        ),
        (
            "descending",
            "descending desc decreasing decroissant decroissante decroissants decroissantes decreciente descendente decrescente absteigend",
        ),
        (
            "ascending",
            "ascending asc increasing croissant croissante croissants croissantes creciente ascendente crescente aufsteigend",
        ),
        (
            "keep_leads",
            "keep keeps garde gardez garder conserve conservez conserver retain retains retiens retenez select selects take prends prenez ne n conserva mantieni manten behalte behalten",
        ),
        ("only_words", "only que seulement just solo soltanto nur"),
        (
            "topn_fillers",
            "with having by avec au a aux con por per dal dalla del della mit plus most the value values valeur valeurs first d'abord en premier primero prima zuerst primeiro",
        ),
        (
            "identity_leads",
            "same memes meme mismas mismo misma stesse stesso denselben dieselben gleichen gleiche mesmas mesma identiques unchanged",
        ),
        (
            "rank_high",
            "highest largest biggest greatest top maximum max eleve elevee eleves elevees grand grande grands grandes haut haute hauts hautes gros grosse mayor mayores alto alta altos altas maggiore maggiori hochsten grossten",
        ),
        (
            "rank_low",
            "lowest smallest least minimum min bas basse basses petit petite petits petites faible faibles menor menores bajo baja bajos bajas minore minori basso bassa niedrigsten kleinsten",
        ),
        ("list_words", "and et y e ed und &"),
        (
            "tail_openers",
            "of from for in de des d du di von pour para dans",
        ),
        (
            "remove_verbs",
            "remove removes drop drops delete deletes strip discard supprime supprimez supprimer enleve enlevez enlever elimine eliminez eliminer retire retirez retirer elimina quita rimuovi entferne entfernen losche",
        ),
        ("any_words", "any all tous toutes todos todas tutti tutte"),
        (
            "duplicate_words",
            "duplicate duplicated duplicates doublon doublons duplique dupliquee dupliques dupliquees duplicado duplicada duplicados duplicadas duplicato duplicata duplicati doppelte doppelten duplikate",
        ),
        (
            "unique_words",
            "unique uniques distinct distincts distincte distinctes unico unica unicos unicas univoco univoci distinti distinte eindeutige eindeutigen",
        ),
        (
            "rename_verbs",
            "rename renames renomme renommez renommer renombra renombre renombrar rinomina rinominare benenne umbenennen renomeia renomeie renomear",
        ),
        ("rename_to", "to as en a in para als zu nach como"),
        ("rename_tails", "um"),
        (
            "join_verbs",
            "merge merges join joins combine combines fusionne fusionnez fusionner joindre combinez combiner unisci unire combina fusiona fusionar junta juntar verbinde verbinden",
        ),
        (
            "join_objects",
            "them both two files file rows records tables table sources datasets data csvs deux fichiers fichier lignes enregistrements due righe ambos archivos filas registros beide dateien zeilen together ensemble",
        ),
        (
            "on_words",
            "on sur by par por su per secondo nach uber using via",
        ),
        ("measure_leads", "by par por per nach selon según segun"),
    ];

    #[test]
    fn tables_are_the_frozen_lists() {
        let statics: [(&str, &[&str]); 30] = [
            ("hyphenated_pronouns", &HYPHENATED_PRONOUNS),
            ("determiners", &DETERMINERS),
            ("each", &EACH),
            ("pronouns", &PRONOUNS),
            ("count_verbs", &COUNT_VERBS),
            ("group_words", &GROUP_WORDS),
            ("sort_verbs", &SORT_VERBS),
            ("by_words", &BY_WORDS),
            ("sort_fillers", &SORT_FILLERS),
            ("descending", &DESCENDING),
            ("ascending", &ASCENDING),
            ("keep_leads", &KEEP_LEADS),
            ("only_words", &ONLY_WORDS),
            ("topn_fillers", &TOPN_FILLERS),
            ("identity_leads", &IDENTITY_LEADS),
            ("rank_high", &RANK_HIGH),
            ("rank_low", &RANK_LOW),
            ("list_words", &LIST_WORDS),
            ("tail_openers", &TAIL_OPENERS),
            ("remove_verbs", &REMOVE_VERBS),
            ("any_words", &ANY_WORDS),
            ("duplicate_words", &DUPLICATE_WORDS),
            ("unique_words", &UNIQUE_WORDS),
            ("rename_verbs", &RENAME_VERBS),
            ("rename_to", &RENAME_TO),
            ("rename_tails", &RENAME_TAILS),
            ("join_verbs", &JOIN_VERBS),
            ("join_objects", &JOIN_OBJECTS),
            ("on_words", &ON_WORDS),
            ("measure_leads", &MEASURE_LEADS),
        ];
        for ((name, words), (read, held)) in FROZEN.iter().zip(statics) {
            let frozen: Vec<&str> = words.split(' ').collect();
            assert_eq!(name, &read, "the tables keep their order");
            assert_eq!(super::table(name), frozen, "{name}");
            assert_eq!(held, frozen.as_slice(), "{name}");
        }
        let sections = STAGE_WORDS.lines().filter(|l| l.starts_with('[')).count();
        assert_eq!(
            sections,
            FROZEN.len(),
            "no section beyond the frozen tables"
        );
    }

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn lowered(text: &str) -> Option<String> {
        stated(text, &[]).map(|s| {
            s.lower(
                ".records".to_owned(),
                &crate::rules::numbers::Numbers::new(),
            )
        })
    }

    #[test]
    fn a_count_per_column_is_a_grouping_and_a_bare_count_is_a_total() {
        let grouped = Some(
            ".records | group_by(.client) | map({\"client\": (.[0] | .client), \"count\": length})"
                .to_owned(),
        );
        assert_eq!(lowered("count the rows per client"), grouped);
        assert_eq!(lowered("compte les lignes par client"), grouped);
        assert_eq!(lowered("count them per client"), grouped);
        assert_eq!(lowered("the number of rows per client"), Some(".records | group_by(.client) | map({\"client\": (.[0] | .client), \"number\": length})".to_owned()));
        assert_eq!(
            lowered("count the rows for each client"),
            grouped,
            "a two-word group phrase"
        );
        assert_eq!(
            lowered("the total of the amount column per client"),
            Some(".records | group_by(.client) | map({\"client\": (.[0] | .client), \"total\": (map(.amount | tonumber) | add // 0)})".to_owned())
        );
        // No key: a total over every row, never a grouping.
        assert_eq!(
            lowered("count the rows"),
            Some(".records | {\"count\": length}".to_owned())
        );
        assert_eq!(
            lowered("compte les lignes"),
            Some(".records | {\"count\": length}".to_owned())
        );
        // A columns hint fixes the spelling; a name outside it is no column.
        let hint = cols(&["date", "Amount", "Client"]);
        assert_eq!(
            stated("count the rows per client", &hint).and_then(|s| s.group_by),
            Some("Client".to_owned())
        );
        assert_eq!(stated("count the rows per country", &hint), None);
        for none in [
            "count the tickets per status",
            "count the open rows per client",
            "count per client",
            "count the rows per",
            "count the rows per client and country",
            "the count of those orders per country as JSON",
            "write the count of those orders per country as JSON",
        ] {
            assert_eq!(stated(none, &[]), None, "{none}");
        }
    }

    #[test]
    fn a_rename_names_the_old_column_and_the_new_name_in_six_languages() {
        let renamed = Some(
            ".records | map(with_entries(if .key == \"country\" then .key = \"region\" else . end))"
                .to_owned(),
        );
        for text in [
            "rename the country column to region",
            "rename country to region",
            "renomme la colonne country en region",
            "renombra la columna country a region",
            "rinomina la colonna country in region",
            "benenne die Spalte country in region um",
            "renomeia a coluna country para region",
        ] {
            assert_eq!(lowered(text), renamed, "{text}");
        }
        // A columns hint fixes the spelling of the old name; the new name is the request's.
        let hint = cols(&["date", "Country", "Amount"]);
        assert_eq!(
            stated("rename the country column to region", &hint).map(|s| s.renames),
            Some(vec![("Country".to_owned(), "region".to_owned())])
        );
        assert_eq!(stated("rename the city column to region", &hint), None);
        for none in [
            "rename the country column",
            "rename to region",
            "rename the country column to region and the city column to town",
            "rename the country column to the same country",
            "rename the country column to region please",
        ] {
            assert_eq!(stated(none, &[]), None, "{none}");
        }
    }

    #[test]
    fn a_sort_names_its_key_and_its_direction() {
        let desc = Some(".records | sort_by(.amount | tonumber? // .) | reverse".to_owned());
        assert_eq!(lowered("sort the rows by amount descending"), desc);
        assert_eq!(
            lowered("sort the rows by the amount column in descending order"),
            desc
        );
        assert_eq!(lowered("sort by amount desc"), desc);
        assert_eq!(
            lowered("trie les lignes par montant décroissant"),
            Some(".records | sort_by(.montant | tonumber? // .) | reverse".to_owned())
        );
        assert_eq!(
            lowered("sort the rows by amount"),
            Some(".records | sort_by(.amount | tonumber? // .)".to_owned()),
            "a sort without a direction is ascending, which is what the word means"
        );
        assert_eq!(
            lowered("sort the rows by amount ascending"),
            Some(".records | sort_by(.amount | tonumber? // .)".to_owned())
        );
        for none in [
            "sort the rows",
            "sort the rows descending",
            "sort the rows by amount and client",
            "sort the rows by amount descending then ascending",
            "sort the tickets into bug or feature",
        ] {
            assert_eq!(stated(none, &[]), None, "{none}");
        }
    }

    #[test]
    fn a_top_n_needs_its_number_its_measure_and_its_rank() {
        let top2 =
            Some(".records | sort_by(.amount | tonumber? // .) | reverse | .[:2]".to_owned());
        assert_eq!(lowered("keep the 2 rows with the highest amount"), top2);
        // A number word, an entity noun before the measure, « first » after the rank word,
        // and an identity tail the stage keeps by construction.
        assert_eq!(
            stated(
                "top five issues by rating, highest first, same columns",
                &[]
            )
            .map(|s| s.lower(
                ".records".to_owned(),
                &crate::rules::numbers::Numbers::new()
            )),
            Some(".records | sort_by(.rating | tonumber? // .) | reverse | .[:5]".to_owned())
        );
        assert_eq!(
            lowered("keep the top three rows by amount"),
            Some(".records | sort_by(.amount | tonumber? // .) | reverse | .[:3]".to_owned())
        );
        assert_eq!(
            lowered("keep only the 2 rows with the largest amount"),
            top2
        );
        assert_eq!(lowered("the top 2 rows by amount"), top2);
        assert_eq!(lowered("keep the 2 highest rows by amount"), top2);
        assert_eq!(
            lowered("garde les 2 lignes au montant le plus élevé"),
            Some(".records | sort_by(.montant | tonumber? // .) | reverse | .[:2]".to_owned())
        );
        assert_eq!(
            lowered("keep the 3 rows with the lowest amount"),
            Some(".records | sort_by(.amount | tonumber? // .) | .[:3]".to_owned())
        );
        for none in [
            "keep the best rows",
            "keep the 2 best rows",
            "keep the 2 rows by amount",
            "keep the 2 rows",
            "keep the 0 rows with the highest amount",
            "the top 3 countries",
            "the 3 rows whose amount > 100",
            "keep the 2 rows with the highest amount and the lowest date",
            "keep the top 2 rows with the lowest amount",
        ] {
            assert_eq!(stated(none, &[]), None, "{none}");
        }
    }

    #[test]
    fn a_projection_lists_the_fields_it_keeps() {
        let slim = Some(".records | map({\"id\": .id, \"title\": .title})".to_owned());
        assert_eq!(lowered("keep only the id and title of each ticket"), slim);
        assert_eq!(lowered("keep only the id and title"), slim);
        assert_eq!(lowered("keep only the columns id and title"), slim);
        assert_eq!(lowered("keep only the id and title fields"), slim);
        assert_eq!(
            lowered("ne garde que les champs id et titre"),
            Some(".records | map({\"id\": .id, \"titre\": .titre})".to_owned())
        );
        assert_eq!(
            lowered("keep only the id, title and status of each ticket"),
            Some(
                ".records | map({\"id\": .id, \"title\": .title, \"status\": .status})".to_owned()
            )
        );
        assert_eq!(
            lowered("keep only the id of each ticket"),
            Some(".records | map({\"id\": .id})".to_owned())
        );
        assert_eq!(
            lowered("keep only the column id"),
            Some(".records | map({\"id\": .id})".to_owned())
        );
        let hint = cols(&["id", "Title", "body"]);
        assert_eq!(
            stated("keep only the id and title of each ticket", &hint).map(|s| s.columns),
            Some(cols(&["id", "Title"]))
        );
        assert_eq!(
            stated("keep only the id and owner of each ticket", &hint),
            None
        );
        for none in [
            "keep only the important fields",
            "keep only the id",
            "keep only the rows",
            "keep only the open ones",
            "keep only the tickets whose status is open",
            "keep the tone warm",
            "keep only the id and title of ./tickets.json",
        ] {
            assert_eq!(stated(none, &[]), None, "{none}");
        }
    }

    #[test]
    fn a_removal_of_duplicates_keeps_the_first_occurrences() {
        let distinct = Some(format!(".records | {}", aggregate::DISTINCT));
        assert_eq!(lowered("remove the duplicate lines"), distinct);
        assert_eq!(lowered("remove duplicates"), distinct);
        assert_eq!(lowered("remove any duplicate rows"), distinct);
        assert_eq!(lowered("supprime les lignes en double"), distinct);
        assert_eq!(lowered("supprime les doublons"), distinct);
        assert_eq!(lowered("elimina las filas duplicadas"), distinct);
        assert_eq!(lowered("keep only the unique lines"), distinct);
        for none in [
            "remove the duplicate tickets",
            "remove the lines",
            "remove the file",
            "keep only the unique",
            "remove the duplicate lines and the empty ones",
        ] {
            assert_eq!(stated(none, &[]), None, "{none}");
        }
    }

    #[test]
    fn a_join_names_the_column_it_joins_on() {
        assert_eq!(
            stated("merge them on the id column", &[]).and_then(|s| s.join_on),
            Some("id".to_owned())
        );
        assert_eq!(
            stated("join the two files on id", &[]).and_then(|s| s.join_on),
            Some("id".to_owned())
        );
        assert_eq!(
            stated("fusionne-les sur la colonne id", &[]).and_then(|s| s.join_on),
            Some("id".to_owned())
        );
        assert_eq!(
            stated("merge both by customer_id", &[]).and_then(|s| s.join_on),
            Some("customer_id".to_owned())
        );
        for none in [
            "merge them",
            "merge them on the id and name columns",
            "merge the summaries into one file",
            "merge all four blurbs in the listed order into a single ./out/x.md",
            "merge the pull request",
        ] {
            assert_eq!(stated(none, &[]), None, "{none}");
        }
        // A join that names its sources but no key is the one clause a human must complete.
        for keyless in [
            "merge them",
            "fusionne les deux fichiers",
            "join both files",
        ] {
            assert!(join_without_key(keyless), "{keyless}");
        }
        for effect in [
            "merge the pull request",
            "merge the summaries into one file",
            "merge them on the id column",
            "merge",
        ] {
            assert!(!join_without_key(effect), "{effect}");
        }
    }

    #[test]
    fn two_segments_merge_into_one_shape_or_none() {
        let count = stated("count the rows per client", &[]).expect("a grouping");
        let top = stated("keep the 2 rows with the highest count", &[]).expect("a top-N");
        let both = count.clone().merge(top.clone()).expect("merged");
        assert_eq!(both.group_by.as_deref(), Some("client"));
        assert_eq!(both.limit, Some(2));
        assert_eq!(
            both.lower(
                ".records".to_owned(),
                &crate::rules::numbers::Numbers::new()
            ),
            ".records | group_by(.client) | map({\"client\": (.[0] | .client), \"count\": length}) | sort_by(.count) | reverse | .[:2]",
            "a sort on a produced name compares the number it already is"
        );
        assert_eq!(top.clone().merge(top.clone()), None, "two sorts");
        let distinct = stated("remove the duplicate lines", &[]).expect("distinct");
        assert_eq!(top.merge(distinct), None, "a top-N beside a dedup");
        let record = both.to_json();
        assert_eq!(Shape::from_json(Some(&record)), Some(both));
    }
}
