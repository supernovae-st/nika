// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The closed grammar of an engine question: whole clauses, punctuation and
//! courtesy aside, against a closed set of forms with at most one typed slot
//! — the house grammar of the closed conversation acts (`is_cancel`,
//! `is_what_happened`). A line none of whose clauses is a form here is never
//! read for its words: a job that mentions a template, a model or a run
//! reaches understanding whole. Beside a waiting proposal a question is about
//! the proposal, unless it is a strict catalog question
//! ([`Phase::BesideProposal`]).
//!
//! A form is words separated by spaces: `a|b` is a choice, `_` the empty
//! choice, `<code>` a diagnostic code, `<flow>` a workflow the snapshot holds,
//! `<word>` a word of the language (one or two words, either number), `<tail>`
//! the job a gallery question describes. Forms are separated by `;`.

use crate::snapshot::ProjectSnapshot;

/// Where the line was typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    /// Nothing waits: any closed engine question.
    Idle,
    /// A proposal waits: only the workflows, builtins, providers, a code, the
    /// last run or a definition; any other question reads the proposal's bytes.
    BesideProposal,
}

/// What a closed engine question asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Ask {
    /// A diagnostic code's teaching.
    Explain(String),
    /// The verdict on a workflow the snapshot holds (its relative path).
    Verdict(String),
    /// The workflows the snapshot saw.
    Workflows,
    /// The builtins the catalog ships.
    Builtins,
    /// The providers this binary drives.
    Providers,
    /// The gallery (examples and templates).
    Gallery,
    /// The last run, read from its trace.
    LastRun,
    /// What the language calls the asked words.
    Vocabulary(Vec<&'static str>),
}

const WORKFLOWS: &str = "which|what workflows are|exist|_ \
    here|there|available|saved|clean|valid|_; which|what workflows do|have i|we have|_; \
    how many workflows are|do there|i|we|_ have|_; \
    list|show|see me|us|_ the|my|all|_ workflows; can|could i|we|you see|show|list me|us|_ \
    the|my|all|_ workflows; my|the|_ workflows; liste|montre|montre-moi|affiche les|mes|_ \
    workflows; quels sont|_ les|mes|_ workflows";
const BUILTINS: &str = "which|what builtin|built-in|_ builtins|tools|built-ins are|exist|_ \
    available|there|_; which|what builtins|tools|built-ins can|do i|we|you use|call|have|_; \
    which|what builtins|tools|built-ins does|do the|_ nika|engine|you ship|have|offer; \
    which|what builtins|tools|built-ins ship|come with nika; list|show|see me|us|_ the|all|_ \
    builtins|tools|built-ins; can|could i|you see|show|list me|_ the|all|_ \
    builtins|tools|built-ins; builtins|tools|built-ins; liste|montre|montre-moi les|_ \
    builtins|outils; quels sont|_ les|_ outils|builtins";
const PROVIDERS: &str = "which|what llm|ai|model|local|cloud|_ providers|models are|_ \
    supported|available|there|_; which|what llm|ai|model|local|cloud|_ providers|models \
    do|does|can you|nika|i|we support|use|drive|_; list|show|see me|us|_ the|all|_ \
    providers|models; providers; quels providers|modèles sont|_ supportés|disponibles|_; \
    liste les providers|modèles";
const EXPLAIN: &str = "explain|explique this|the|_ error|code|finding|_ <code> to|_ me|_; \
    can|could you explain <code> to|_ me|_; what is|does|_ <code> mean|means|_; what's <code>; \
    why did|do i get|got|see <code>; <code>";
const VERDICT: &str = "check|validate|verify|audit|vérifie <flow> for|_ errors|problems|_; \
    can|could you check|validate|verify <flow> for|_ errors|problems|_; is|are the|_ <flow> \
    workflow|file|_ still|_ valid|clean|ok; check if|whether the|_ <flow> workflow|file|_ is|_ \
    valid|clean|ok";
const LAST_RUN: &str = "what happened in|with|during|to the last|latest|previous run; how did \
    the|_ last|latest|previous run go; did it run|work; did the last|latest|previous run \
    fail|succeed|pass|work|finish; why did the last|latest|previous run fail; what went wrong \
    in|with|during the last|latest|previous run; what|what's|whats is|was|_ the \
    last|latest|previous run; what was|is the result|outcome|status of the last|latest|previous \
    run; which|what tasks|steps failed|ran|succeeded in|during the last|latest|previous run; \
    can|could|_ you|_ show me|_ the|_ last|latest|previous run; the|_ last|latest|previous \
    run; montre|montre-moi le|_ dernier run";
const GALLERY: &str = "examples|templates|shapes|scaffolds|skeletons|starters; list|show|see \
    me|us|_ the|all|some|your|_ examples|templates|shapes|scaffolds|skeletons|starters; \
    can|could i|we|you see|show|list me|us|_ the|all|some|your|_ examples|templates|shapes; \
    which|what examples|templates|shapes|scaffolds are|exist|do|_ there|available|you|_ have|_; \
    what kind|kinds|sort|type|types of examples|templates|shapes|example|template are|exist|do|_ \
    there|available|you|_ have|_; which|what example|template|shape|scaffold <tail>; is|are \
    there a|an|any|_ example|template|examples|templates for <tail>; do you have a|an|any|_ \
    example|template|examples|templates for <tail>; show me a|an|some|_ \
    example|template|examples|templates for <tail>; what's|whats a|an|the good|best|_ \
    example|template for <tail>; what is a|an|the good|best|_ example|template for <tail>; montre|montre-moi|liste les modèles|exemples|templates; quels \
    modèles|exemples|templates as-tu|existent|_";
/// What the language calls a word: answered in any phase.
const DEFINITIONS: &str = "what is|are a|an|_ <word>; what's|whats a|an|_ <word>; what do \
    you call a|an|_ <word>; what is the nika|_ word|term|equivalent for|of a|an|_ <word>; \
    what's|whats the nika|_ word|term|equivalent for|of a|an|_ <word>; what does a|an|_ \
    <word> mean; meaning of a|an|_ <word>; explain|define a|an|_ <word>";
/// Whether the language has a word, how to write it: beside a proposal this
/// is about the proposal (« is there a timeout? »), so idle only.
const PRESENCE: &str = "is|are there a|an|any|_ <word> concept|notion|_; how do|can i|we|you \
    make|write|add|declare|use|express|define|get|set|do a|an|any|_ <word>; what does a|an|_ \
    <word> do";
/// A vocabulary question's continuation (« … and a secret? »).
const MORE_WORDS: &str = "a|an|any|_ <word>";
/// What may only follow another question (« is alpha valid? check it »).
const CONTINUATIONS: &str = "check it|that; ok; thanks; thank you";

/// What a form set asks, given its slot.
type Asks = fn(Slot) -> Option<Ask>;

/// The forms a phase reads, most specific first, with what each asks.
fn forms(phase: Phase) -> Vec<(&'static str, Asks)> {
    let mut forms: Vec<(&'static str, Asks)> = vec![
        (EXPLAIN, |slot| match slot {
            Slot::Code(code) => Some(Ask::Explain(code)),
            _ => None,
        }),
        (WORKFLOWS, |_| Some(Ask::Workflows)),
        (BUILTINS, |_| Some(Ask::Builtins)),
        (PROVIDERS, |_| Some(Ask::Providers)),
        (LAST_RUN, |_| Some(Ask::LastRun)),
        (DEFINITIONS, WORD),
    ];
    if phase == Phase::Idle {
        forms.push((VERDICT, |slot| match slot {
            Slot::Flow(path) => Some(Ask::Verdict(path)),
            _ => None,
        }));
        forms.push((GALLERY, |_| Some(Ask::Gallery)));
        forms.push((PRESENCE, WORD));
    }
    forms
}

/// A vocabulary form's ask: the word its slot captured.
const WORD: Asks = |slot| match slot {
    Slot::Word(word) => Some(Ask::Vocabulary(vec![word])),
    _ => None,
};

/// What a form's slot captured.
enum Slot {
    None,
    Code(String),
    Flow(String),
    Word(&'static str),
}

/// The closed question a line asks in `phase`, or `None` when any clause of
/// it is not a form here.
pub(super) fn ask(input: &str, snapshot: &ProjectSnapshot, phase: Phase) -> Option<Ask> {
    let input = input.replace(['\u{2018}', '\u{2019}'], "'");
    let mut first: Option<Ask> = None;
    for clause in clauses(&input) {
        let raw = words_of(clause);
        let more = raw
            .first()
            .is_some_and(|w| matches!(w.as_str(), "and" | "or" | "also" | "et"));
        let words = trimmed_courtesy(&raw);
        if words.is_empty() {
            continue;
        }
        let asked = forms(phase)
            .into_iter()
            .find_map(|(set, asks)| read(set, &words, snapshot).and_then(asks));
        match (asked, &mut first) {
            (Some(asked), None) => first = Some(asked),
            (Some(Ask::Vocabulary(word)), Some(Ask::Vocabulary(known))) => known.extend(word),
            (Some(_), Some(_)) => {}
            (None, Some(Ask::Vocabulary(asked))) if more => {
                let Some(Slot::Word(word)) = read(MORE_WORDS, &words, snapshot) else {
                    return None;
                };
                asked.push(word);
            }
            (None, Some(_)) if read(CONTINUATIONS, &words, snapshot).is_some() => {}
            (None, _) => return None,
        }
    }
    first
}

/// The slot a clause captures when it is one of the `;`-separated forms.
fn read(set: &str, words: &[String], snapshot: &ProjectSnapshot) -> Option<Slot> {
    set.split(';').find_map(|form| {
        let form: Vec<&str> = form.split_whitespace().collect();
        fit(&form, words, snapshot)
    })
}

/// Whether `words` are exactly `form`, and what its slot captured.
fn fit(form: &[&str], words: &[String], snapshot: &ProjectSnapshot) -> Option<Slot> {
    let Some((head, rest)) = form.split_first() else {
        return words.is_empty().then_some(Slot::None);
    };
    let next = |n: usize| words.get(n..).and_then(|left| fit(rest, left, snapshot));
    match *head {
        "<tail>" => (rest.is_empty() && is_tail(words)).then_some(Slot::None),
        "<code>" => {
            let code = words.first().filter(|w| is_code(w))?.to_uppercase();
            next(1).map(|_| Slot::Code(code))
        }
        "<flow>" => {
            let path = held(snapshot, words.first()?)?;
            next(1).map(|_| Slot::Flow(path))
        }
        "<word>" => [2, 1].into_iter().find_map(|n| {
            let (known, _) = nika_vocab::glossary::entry(&words.get(..n)?.join(" "))?;
            next(n).map(|_| Slot::Word(known))
        }),
        choices => choices.split('|').find_map(|choice| {
            if choice == "_" {
                fit(rest, words, snapshot)
            } else {
                (words.first().map(String::as_str) == Some(choice))
                    .then(|| next(1))
                    .flatten()
            }
        }),
    }
}

/// A gallery job: words that describe work to find a shape for, never a
/// question about something said (« which template is this »), never a
/// file, path or URL to act on.
fn is_tail(words: &[String]) -> bool {
    const REFERENCE: &str =
        "is are was were does do did will would should this that it these those";
    let opens_on_a_reference = words
        .first()
        .is_some_and(|w| REFERENCE.split(' ').any(|r| r == w));
    !words.is_empty() && !opens_on_a_reference && !words.iter().any(|w| names_a_place(w))
}

/// A diagnostic code (`NIKA-AUTH-006`, `NIKA-1709`), never a version.
fn is_code(word: &str) -> bool {
    word.strip_prefix("nika-").is_some_and(|rest| {
        let parts: Vec<&str> = rest.split('-').collect();
        parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()))
            && parts
                .last()
                .is_some_and(|p| p.chars().all(|c| c.is_ascii_digit()))
    })
}

/// The one workflow a word names (the snapshot's own rules, any case).
fn held(snapshot: &ProjectSnapshot, word: &str) -> Option<String> {
    let needle = word.trim_start_matches("./").to_lowercase();
    let mut hits = snapshot.workflows.iter().filter(|w| {
        let path = w.path.to_lowercase();
        path == needle
            || path.ends_with(&format!("/{needle}"))
            || w.name.as_deref().map(str::to_lowercase).as_deref() == Some(needle.as_str())
            || path.trim_end_matches(".nika") == needle
    });
    let first = hits.next()?;
    hits.next().is_none().then(|| first.path.clone())
}

/// The line's clauses: split at `?`, `!`, `;` and at a period that ends a
/// sentence (never the one inside `alpha.nika` or `0.25`).
fn clauses(input: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (at, c) in input.char_indices() {
        let ends = matches!(c, '?' | '!' | ';')
            || (c == '.'
                && input[at + 1..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace));
        if ends {
            out.push(&input[start..at]);
            start = at + 1;
        }
    }
    out.push(&input[start..]);
    out
}

/// A clause's lowered words, punctuation around them removed; a leading
/// period stays (`./alpha.nika`).
fn words_of(clause: &str) -> Vec<String> {
    let punctuation = |c: char| "`\"',()[]:«»".contains(c);
    clause
        .split_whitespace()
        .map(|w| {
            w.trim_start_matches(punctuation)
                .trim_end_matches(|c: char| punctuation(c) || c == '.')
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// The clause without the courtesy around it (« please », « here »,
/// « in this project »); empty when it was courtesy only (« Hi there »).
fn trimmed_courtesy(raw: &[String]) -> Vec<String> {
    const LEADING: &str = "please hey hi hello ok okay so now also well nika thanks thank merci \
        salut bonjour alright cool great sure and or et";
    const TRAILING: &str = "please;stp;svp;here;now;right now;in nika;in this project;in the \
        project;in my project;in this folder;for me;ici;dans ce projet";
    let start = raw
        .iter()
        .position(|w| !LEADING.split_whitespace().any(|c| c == w))
        .unwrap_or(raw.len());
    let mut words = raw[start..].to_vec();
    while let Some(cut) = TRAILING.split(';').find_map(|tail| {
        let tail: Vec<&str> = tail.split_whitespace().collect();
        let at = words.len().checked_sub(tail.len()).filter(|at| *at > 0)?;
        words[at..]
            .iter()
            .zip(&tail)
            .all(|(w, t)| w == t)
            .then_some(at)
    }) {
        words.truncate(cut);
    }
    if words.iter().all(|w| matches!(w.as_str(), "there" | "you")) {
        Vec::new()
    } else {
        words
    }
}

/// A URL, a path or a file name (`./a`, `../b`, `~/c`, `/d/e`, `name.ext`).
fn names_a_place(token: &str) -> bool {
    if token.contains("://") || token.starts_with("www.") {
        return true;
    }
    if ["./", "../", "~/"].iter().any(|p| token.starts_with(p))
        || (token.starts_with('/') && token.len() > 1)
    {
        return true;
    }
    let path = std::path::Path::new(token);
    let stem = path.file_stem().is_some_and(|s| !s.is_empty());
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            stem && ext.len() >= 2
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && ext.chars().any(|c| c.is_ascii_alphabetic())
        })
}
