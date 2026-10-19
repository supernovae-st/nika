// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a waiting human answers, as words: the whole-line consent words, the one grammar a fresh
//! spending decision reads, and the slash commands every waiting state answers from the Session's
//! own facts (moved whole out of nika-session's `runtime/decision.rs`). None of it grants anything
//! beyond the one decision shown.

/// The whole-line tokens a confirm gate accepts — a protocol, not a
/// reading of language (a longer line is routed, never reduced to one).
#[must_use]
pub fn is_gate_token(line: &str) -> bool {
    matches!(
        line.trim().to_lowercase().as_str(),
        "yes" | "y" | "true" | "ok" | "oui" | "approve" | "no" | "n" | "false" | "non" | "deny"
    )
}

/// The refusal line: `no` in the few words a human types for it.
#[must_use]
pub fn is_no(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "no" | "n" | "non" | "discard" | "cancel" | "drop" | "nope" | "stop"
    )
}

/// The consent line, and nothing else: `yes` in the few words a human
/// types for it, or the shown workflow saved in words (« enregistre-le », « save it »).
#[must_use]
pub fn is_yes(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "yes" | "y" | "apply" | "ok" | "oui" | "go" | "do it"
    ) || worded(answer) == Some(Worded::Save)
}

/// A yes with its go-ahead or courtesy, and nothing more, said to a proposal shown: one yes word
/// first, then only words that say go (« Oui, allez-y », « ok, vas-y merci », « yes, go ahead
/// please », « d'accord, c'est bon »). Live gauntlet r01, 2026-10-16: « Oui, allez-y » under a
/// card saying « « oui » écrit exactement ces octets » started fifteen minutes of re-authoring.
/// A line that says anything else (« oui mais … ») is not it.
#[must_use]
pub fn is_plain_yes(answer: &str) -> bool {
    const YES: &[&str] = &[
        "oui", "ouais", "ok", "okay", "yes", "yep", "d'accord", "parfait", "super", "go",
    ];
    const GO: &[&str] = &[
        "allez-y",
        "allez",
        "vas-y",
        "go",
        "ahead",
        "c'est",
        "bon",
        "parti",
        "fais-le",
        "faites-le",
        "do",
        "it",
        "sure",
        "absolument",
        "carrément",
        "tout",
        "à",
        "fait",
    ];
    let line = answer
        .trim()
        .trim_end_matches(['.', '!', ' '])
        .replace('\u{2019}', "'")
        .replace(',', " ")
        .to_lowercase();
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some((first, rest)) = words.split_first() else {
        return false;
    };
    YES.contains(first)
        && !rest.is_empty()
        && (rest.iter()).all(|word| GO.contains(word) || COURTESY.contains(word))
}

/// The combined consent line, and nothing else: save the proposal shown and run what it saves
/// once (`save & run`, the word a host's Save & run sends), the whole line, or the same said of
/// the shown workflow (« enregistre ce workflow et lance-le », « save it and run it »).
#[must_use]
pub fn is_save_and_run(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "save & run" | "save and run" | "enregistre et lance"
    ) || worded(answer) == Some(Worded::SaveAndRun)
}

/// What a whole line says of the workflow shown: save it, or save it then run it.
#[derive(Debug, PartialEq, Eq)]
enum Worded {
    Save,
    SaveAndRun,
}

/// The verbs that save the workflow shown, their infinitive after a request (« je voudrais
/// l'enregistrer ») included.
const SAVES: &[&str] = &[
    "enregistre",
    "enregistrez",
    "enregistrer",
    "sauvegarde",
    "sauvegardez",
    "sauvegarder",
    "save",
];

/// The requests that may open a save line (« je voudrais l'enregistrer », « can you save it »),
/// by their words, longest first.
const REQUESTS: &[&[&str]] = &[
    &["est-ce", "que", "tu", "peux"],
    &["est-ce", "que", "vous", "pouvez"],
    &["i", "would", "like", "to"],
    &["je", "voudrais"],
    &["j'aimerais"],
    &["je", "veux"],
    &["tu", "peux"],
    &["vous", "pouvez"],
    &["peux-tu"],
    &["pouvez-vous"],
    &["i'd", "like", "to"],
    &["i", "want", "to"],
    &["can", "you"],
    &["could", "you"],
    &["please"],
];

/// The plain yes that may open a save line (« oui, enregistre »), nothing else: a line opened
/// by another word (« parfait, enregistre-le puis lance-le ») keeps its route to the author,
/// whose relay the Session checks against the words.
const LEADS: &[&str] = &["oui", "ok", "okay", "yes", "d'accord"];

/// The courtesy that may close it (« … stp », « … s'il vous plaît », « … please »).
const COURTESY: &[&str] = &[
    "merci", "stp", "svp", "please", "thanks", "s'il", "vous", "te", "plaît", "plait",
];

/// The verbs that run it, their infinitive after a request included.
const RUNS: &[&str] = &[
    "lance",
    "lancez",
    "lancer",
    "exécute",
    "exécutez",
    "exécuter",
    "execute",
    "run",
];

/// The ways the workflow shown is named after its verb, longest first, nothing else: the
/// changes a card's own « Enregistrer ces changements » names included (live gauntlet wave 6 r08
/// B4, 2026-10-18: « oui, enregistrez ces changements » under that card saved nothing and reached
/// the author, which proposed something else).
const OBJECTS: &[&[&str]] = &[
    &["ce", "workflow"],
    &["le", "workflow"],
    &["this", "workflow"],
    &["the", "workflow"],
    &["ces", "changements"],
    &["les", "changements"],
    &["ce", "changement"],
    &["le", "changement"],
    &["these", "changes"],
    &["the", "changes"],
    &["this", "change"],
    &["the", "change"],
    &["ça"],
    &["ca"],
    &["cela"],
    &["it"],
];

/// A closed grammar, never a reading of language: one save verb and the workflow shown (a
/// pronoun on the verb, « it », « ce workflow » or nothing), then optionally « et », « and »,
/// « puis » or « then » and one run verb named the same ways, the whole line and nothing more.
/// Live gauntlet, 2026-10-15: « Enregistre ce workflow et lance-le. » at a shown proposal was
/// read as a change and proposed again, instead of saved and run.
fn worded(answer: &str) -> Option<Worded> {
    let line = answer
        .trim()
        .trim_end_matches(['.', '!', '?', ' '])
        .replace('\u{2019}', "'")
        .replace(',', " ")
        .to_lowercase();
    let mut words: Vec<&str> = line.split_whitespace().collect();
    // A yes before the verb and a courtesy after it say nothing more (live gauntlet r08,
    // 2026-10-16: « oui, enregistre » was not read as the save it asked).
    while words.first().is_some_and(|word| LEADS.contains(word)) {
        words.remove(0);
    }
    // So does a request around it (live gauntlet r08 B4, 2026-10-16: « je voudrais
    // l'enregistrer et le lancer » reached the author and failed).
    if let Some(request) = REQUESTS.iter().find(|request| words.starts_with(request)) {
        words.drain(..request.len());
    }
    while words.last().is_some_and(|word| COURTESY.contains(word)) {
        words.pop();
    }
    let rest = act(&words, SAVES)?;
    let Some((connector, rest)) = rest.split_first() else {
        return Some(Worded::Save);
    };
    if !matches!(*connector, "et" | "and" | "puis" | "then") {
        return None;
    }
    act(rest, RUNS)?.is_empty().then_some(Worded::SaveAndRun)
}

/// One verb of `verbs` and the workflow it names at the head of `words`: what follows it. A
/// pronoun rides the verb (« lance-le »); a bare verb names the workflow shown.
fn act<'a>(words: &'a [&'a str], verbs: &[&str]) -> Option<&'a [&'a str]> {
    // A pronoun before an infinitive names the workflow shown (« le lancer », « l'enregistrer »).
    let words = match words {
        ["le" | "la", next, ..] if verbs.contains(next) => &words[1..],
        _ => words,
    };
    let (first, rest) = words.split_first()?;
    if let Some(verb) = first.strip_prefix("l'") {
        return verbs.contains(&verb).then_some(rest);
    }
    let (verb, clitic) = match first.split_once('-') {
        Some((verb, "le" | "la")) => (verb, true),
        Some(_) => return None,
        None => (*first, false),
    };
    if !verbs.contains(&verb) {
        return None;
    }
    if clitic {
        return Some(rest);
    }
    let named = OBJECTS.iter().find(|object| rest.starts_with(object));
    Some(named.map_or(rest, |object| &rest[object.len()..]))
}

/// One answer to a fresh spending decision: the Session's one-time
/// unknown-cost choice and a Run cost decision read the same grammar
/// (English and French). Approval is the whole line and nothing else; a
/// refusal may lead a longer line (« non, finalement pas maintenant »); any
/// other line is asked again: it never approves, never spends and never
/// cancels. New answers may be added: a door that does not know one asks
/// again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecisionAnswer {
    /// `yes` · `oui` · the Session's consent words, the whole line.
    Approve,
    /// `no` · `non` · `cancel` …, the whole line or its first word.
    Decline,
    /// `details` · `/details` · `/why` · `why?`: the same review's evidence.
    Details,
    /// Anything else: the decision is asked again, nothing is sent.
    Unknown,
}

/// Reads one line as the answer to a fresh spending decision.
#[must_use]
pub fn decision_answer(line: &str) -> DecisionAnswer {
    let whole = line.trim().trim_end_matches(['.', '!', ' ']).to_lowercase();
    if matches!(
        whole.as_str(),
        "details" | "/details" | "/why" | "why" | "why?"
    ) {
        return DecisionAnswer::Details;
    }
    // A spending approval stays the strict yes: a go-ahead around it (« yes please ») is asked
    // again, never guessed (`is_plain_yes` reads a proposal's consent, not money).
    if is_yes(&whole) {
        return DecisionAnswer::Approve;
    }
    let first = whole
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'))
        .next()
        .unwrap_or("");
    if is_no(&whole) || is_no(first) {
        return DecisionAnswer::Decline;
    }
    DecisionAnswer::Unknown
}

/// The slash commands every waiting state still answers from the session's
/// own facts: never the model, never an answer to what waits.
#[must_use]
pub fn local_command_of(line: &str) -> Option<&'static str> {
    match line.trim() {
        "/help" => Some("/help"),
        "/status" => Some("/status"),
        "/details" => Some("/details"),
        "/why" => Some("/why"),
        "/meaning" => Some("/meaning"),
        "/proof" => Some("/proof"),
        "/restore" => Some("/restore"),
        "/knowledge" => Some("/knowledge"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_plain_yes, is_save_and_run, is_yes};

    /// The whole line said of the workflow shown, the few ways a person says it; anything more
    /// is open language and keeps its route.
    #[test]
    fn saving_and_running_the_shown_workflow_in_words_is_the_consent_it_names() {
        for line in [
            "Enregistre ce workflow et lance-le.",
            "enregistre-le et lance-le",
            "save it and run it",
            "Sauvegarde ce workflow puis exécute-le !",
            "save and run",
            "enregistre et lance",
            "save this workflow and run it",
            // A yes before, a courtesy after (live gauntlet r08, 2026-10-16).
            "Oui, enregistre ce workflow et lance-le stp",
            "ok, save it and run it please",
            // A request around it (live gauntlet r08 B4, 2026-10-16).
            "je voudrais l'enregistrer et le lancer",
            "Je voudrais l’enregistrer et le lancer s'il vous plaît.",
            "pouvez-vous l'enregistrer puis le lancer ?",
            "I'd like to save it and run it",
            "can you save it and run it",
        ] {
            assert!(is_save_and_run(line), "{line}");
            assert!(!is_yes(line), "{line}");
        }
        for line in [
            "je voudrais l'enregistrer",
            "peux-tu l'enregistrer ?",
            "please save it",
        ] {
            assert!(is_yes(line) && !is_save_and_run(line), "{line}");
        }
        for line in [
            "enregistre-le",
            "save it",
            "Enregistre ce workflow.",
            "save",
            "oui",
            "oui, enregistre",
            "Oui, enregistre-le s'il vous plaît",
            "yes save it",
            // The card's own « Enregistrer ces changements », said back (live gauntlet wave 6 r08
            // B4: it saved nothing and reached the author).
            "oui, enregistrez ces changements",
            "Enregistrer ces changements",
            "save these changes",
            "ok, enregistre ça",
        ] {
            assert!(is_yes(line), "{line}");
            assert!(!is_save_and_run(line), "{line}");
        }
        assert!(
            is_save_and_run("enregistrez ces changements et lancez-le"),
            "the changes saved, then run"
        );
        for line in [
            "Oui, allez-y",
            "oui, allez-y, merci.",
            "ok vas-y",
            "Yes, go ahead please",
            "d'accord, c'est bon",
        ] {
            assert!(is_plain_yes(line), "{line}");
        }
        for line in [
            "oui mais change le titre",
            "oui",
            "ok",
            "allez-y",
            "oui, enregistre",
        ] {
            assert!(!is_plain_yes(line), "{line}");
        }
        for line in [
            "enregistre le résumé dans out/x.md",
            "save the results",
            "lance-le",
            "run it",
            "enregistre ce workflow sous le nom veille",
            "enregistre-le et ajoute une colonne",
            "enregistre ce workflow et lance-le chaque matin",
            "save it and email it",
            // Only a plain yes opens the closed grammar: these reach the author.
            "parfait, enregistre-le puis lance-le",
            "bon, enregistre-le",
            "alors, enregistre-le ?",
        ] {
            assert!(!is_save_and_run(line) && !is_yes(line), "{line}");
        }
    }
}
