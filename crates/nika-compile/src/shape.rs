// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The structural shape a plan and its request carry: which instruction is an operation
//! (a numeric rule is code, never prompt guidance), which work is distributive (one draft
//! per item), which lookup selects one record by an identifier. Everything here is
//! deterministic text evidence over the plan's own elements and the request's verbatim
//! words; nothing invents an element, and every promoted element keeps an exact excerpt.

pub(super) use super::objects::without_distributive_tail;
use super::plan::{Op, Plan, Step};
pub(super) use super::rule_tokens::{ATTEMPT_UNITS, SIZE_UNITS, fold};

/// Symbols that compare the number beside them.
const COMPARISON_SYMBOLS: &[&str] = &[">=", "<=", "≥", "≤", ">", "<"];

/// One token of a folded constraint: a word, a number or a comparison symbol, with the
/// surrounding punctuation dropped and a symbol glued to a number split off.
fn tokens(folded: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in folded.split_whitespace() {
        let word = raw.trim_matches(|c: char| {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '(' | ')' | '"' | '\'' | '«' | '»' | '!' | '?'
            )
        });
        if word.is_empty() {
            continue;
        }
        if COMPARISON_SYMBOLS.contains(&word) {
            out.push(word.to_owned());
            continue;
        }
        let symbol = COMPARISON_SYMBOLS
            .iter()
            .find(|s| word.starts_with(*s) && word.len() > s.len());
        if let Some(symbol) = symbol {
            out.push((*symbol).to_owned());
            out.push(word[symbol.len()..].to_owned());
        } else {
            out.push(word.to_owned());
        }
    }
    out
}

fn is_number(token: &str) -> bool {
    let digits = token.trim_start_matches(['-', '+', '€', '$']);
    let digits = digits.trim_end_matches(['%', '€', '$']);
    !digits.is_empty()
        && digits.starts_with(|c: char| c.is_ascii_digit())
        && digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ','))
}

/// A digit beside a comparison cue (the numeric cues of [`super::rules`]), not bounded by
/// a size, attempt or turn unit, and not the concurrency bound the assembler already
/// consumes: a rule that must run as code.
pub(super) fn numeric_rule(text: &str) -> bool {
    if super::bindings::parallel_bound(text).is_some() {
        return false;
    }
    // A prohibition ("Do not copy more than 10 consecutive words") is a rule the prose obeys,
    // never a computation the workflow runs.
    if prohibits(text) {
        return false;
    }
    let folded = fold(text);
    let words = tokens(&folded);
    for (index, token) in words.iter().enumerate() {
        if !is_number(token) {
            continue;
        }
        let after = words.get(index + 1).map(String::as_str).unwrap_or_default();
        let after2 = words.get(index + 2).map(String::as_str).unwrap_or_default();
        let unit = |w: &str| SIZE_UNITS.contains(&w) || ATTEMPT_UNITS.contains(&w);
        if unit(after) || (matches!(after, "a" | "de" | "di" | "of") && unit(after2)) {
            continue;
        }
        let before = words.get(index.wrapping_sub(1)).map(String::as_str);
        let symbol_beside = before.is_some_and(|w| COMPARISON_SYMBOLS.contains(&w))
            || COMPARISON_SYMBOLS.contains(&after);
        let phrase_before = (1..=5).any(|width| {
            index >= width
                && words
                    .get(index - width..index)
                    .is_some_and(|w| super::rules::numeric_cue(&w.join(" ")).is_some())
        });
        if symbol_beside || phrase_before {
            return true;
        }
    }
    false
}

/// A prohibition at the head of a clause ("never keep …", "ne garde pas …", "do not …") is
/// a rule the prose obeys, never a computation: promoting it would run its complement. The
/// reader may keep a whole clause as the constraint ("Read ./x.json, do not keep …"), so
/// every comma- or connector-separated part is judged at its own head. The French
/// restriction "ne … que" ("ne garde que les lignes …") is "only", not "never": a filter
/// the workflow runs.
pub(super) fn prohibits(text: &str) -> bool {
    let joined = text
        .replace(" and ", ", ")
        .replace(" et ", ", ")
        .replace(" then ", ", ")
        .replace(" puis ", ", ")
        .replace(" but ", ", ")
        .replace(" mais ", ", ");
    joined
        .split([',', ';'])
        .map(super::objects::as_clause)
        .any(|part| super::cognition::starts_with_prohibition(part) && !restrictive_ne_que(part))
}

/// "ne garde que …", "n'écris que …", "ne conserve plus que …": the `que` within three
/// words of the `ne`, and no negation word beside it.
fn restrictive_ne_que(text: &str) -> bool {
    let folded = fold(text);
    let words: Vec<&str> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    matches!(words.first(), Some(&"ne" | &"n"))
        && words
            .get(1..4)
            .is_some_and(|window| window.contains(&"que"))
        && !words.iter().any(|w| {
            matches!(
                *w,
                "pas" | "jamais" | "aucun" | "aucune" | "rien" | "personne"
            )
        })
}

/// Every constraint that states a rule becomes a compute step anchored in the request: a
/// digit beside a comparison cue, or a filter the closed grammar reads whole ("keep only
/// the tickets whose status is open", "ne garde que les lignes dont amount dépasse 200").
/// The step is inserted right after the last source step so every later step sees the
/// computed result, and the rule leaves the prompt guidance; a parsed rule is recorded on
/// the plan so admission and binding read the same predicate. A constraint that is not a
/// verbatim excerpt of the request stays a constraint (nothing is invented); a prohibition
/// stays prose; a compute step that already carries the rule is not duplicated. Applying
/// this twice changes nothing.
pub(super) fn promote_stated_rules(plan: &mut Plan, intent: &str) {
    let hint = super::columns::columns_hint(intent);
    let rules: Vec<(String, Option<super::rules::Rule>)> = plan
        .constraints
        .iter()
        .filter(|c| !prohibits(c))
        .filter_map(|c| {
            let parsed = super::rules::synthesize(c, &hint);
            (parsed.is_some() || numeric_rule(c)).then(|| (c.clone(), parsed))
        })
        .collect();
    for (constraint, parsed) in rules {
        let detail = constraint.trim().to_owned();
        let carried = plan
            .steps
            .iter()
            .any(|s| s.op == Op::Compute && s.detail.contains(detail.as_str()));
        if !carried {
            let Some(evidence) = super::cognition::exact_excerpt(intent, &constraint) else {
                continue;
            };
            if let Some(existing) = plan.steps.iter_mut().find(|s| s.op == Op::Compute) {
                if !existing.detail.is_empty() {
                    existing.detail.push_str(" ; ");
                }
                existing.detail.push_str(&detail);
            } else {
                let at = plan
                    .steps
                    .iter()
                    .rposition(|s| matches!(s.op, Op::Read | Op::Fetch | Op::Lookup | Op::Search))
                    .map_or(0, |i| i + 1);
                plan.steps
                    .insert(at, Step::new(Op::Compute, evidence, detail, Vec::new()));
            }
            // The parsed rule is recorded for the step this promotion made or joined. A
            // step that already carried the rule may say more than it (a grouping the
            // grammar does not read): recording the part would let it stand for the whole.
            if let Some(rule) = parsed
                && !plan.rules.iter().any(|r| r.text() == rule.text())
            {
                plan.rules.push(rule);
            }
        }
        plan.constraints.retain(|c| c != &constraint);
    }
}

/// Words that name a heading (EN · FR · ES · IT · DE, folded).
const HEADING_WORDS: &[&str] = &[
    "heading",
    "headings",
    "header",
    "headers",
    "title",
    "titles",
    "titre",
    "titres",
    "intitule",
    "en-tete",
    "entete",
    "titulo",
    "titulos",
    "encabezado",
    "titolo",
    "titoli",
    "intestazione",
    "uberschrift",
    "uberschriften",
    "cabecalho",
    "cabecalhos",
];

/// Distributive words and file-name phrases that, beside a heading word, ask for one
/// heading per item (folded, whole words or phrases).
const DISTRIBUTIVE_CUES: &[&str] = &[
    "each",
    "every",
    "per",
    "chaque",
    "chacun",
    "chacune",
    "cada",
    "ogni",
    "ciascun",
    "ciascuno",
    "jede",
    "jeden",
    "jedes",
    "jeder",
    "par fichier",
    "par document",
    "par note",
    "named after the file",
    "nom du fichier",
    "file name",
    "filename",
    "nombre del archivo",
    "nome del file",
    "dateiname",
    "dateinamen",
    "por ficheiro",
    "por arquivo",
    "nome do ficheiro",
    "nome do arquivo",
];

/// A quantifier that leads a clause and distributes the work over items.
const LEADING_QUANTIFIERS: &[&str] = &[
    "for each",
    "for every",
    "pour chaque",
    "pour chacun",
    "pour chacune",
    "para cada",
    "per ogni",
    "per ciascun",
    "fur jede",
    "fur jeden",
    "fur jedes",
    "para cada um",
    "para cada uma",
];

/// Order and heading phrases the fan-in structure realizes itself, so they leave the
/// prompts once the work is distributed.
const STRUCTURAL_CUES: &[&str] = &[
    "in exactly that order",
    "in that order",
    "in the listed order",
    "in order",
    "in the same order",
    "dans l'ordre",
    "dans cet ordre",
    "dans le meme ordre",
    "en ese orden",
    "en el mismo orden",
    "in quest'ordine",
    "nello stesso ordine",
    "in dieser reihenfolge",
];

/// The folded text with every non-alphanumeric run (apostrophes and hyphens kept) as one
/// space, padded, so a phrase matches as whole words.
fn padded(text: &str) -> String {
    let folded = fold(text);
    let mut out = String::with_capacity(folded.len() + 2);
    out.push(' ');
    let mut space = false;
    for c in folded.chars() {
        if c.is_alphanumeric() || matches!(c, '\'' | '-') {
            out.push(c);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

/// A heading word within one clause of a distributive or file-name cue: one heading per
/// item. The window is sixty characters either side on the padded text.
fn heading_beside_distributive(text: &str) -> bool {
    let padded = padded(text);
    let positions = |table: &[&str]| -> Vec<usize> {
        let mut found = Vec::new();
        for word in table {
            let needle = format!(" {word} ");
            let mut from = 0;
            while let Some(at) = padded.get(from..).and_then(|rest| rest.find(&needle)) {
                found.push(from + at);
                from += at + needle.len();
            }
        }
        found
    };
    let headings = positions(HEADING_WORDS);
    if headings.is_empty() {
        return false;
    }
    let cues = positions(DISTRIBUTIVE_CUES);
    headings
        .iter()
        .any(|h| cues.iter().any(|c| h.abs_diff(*c) <= 60))
}

pub(super) fn led_by_quantifier(text: &str) -> bool {
    let padded = padded(text);
    LEADING_QUANTIFIERS
        .iter()
        .any(|q| padded.starts_with(&format!(" {q} ")))
}

/// The byte span of the sentence a trigger opens: from the trigger's first occurrence in
/// the request (case-insensitive when folding keeps every byte) to the sentence end that
/// follows it (`.`, `!`, `?`, a line break, or the end of the request). What that sentence
/// states is done once per item of the trigger; what a later sentence states is not.
pub(super) fn triggered_span(intent: &str, trigger: &str) -> Option<(usize, usize)> {
    let trigger = trigger.trim();
    if trigger.is_empty() {
        return None;
    }
    let lower = intent.to_lowercase();
    let start = if lower.len() == intent.len() {
        lower.find(&trigger.to_lowercase())
    } else {
        intent.find(trigger)
    }?;
    let after = start + trigger.len();
    let end = intent
        .get(after..)?
        .find(['.', '!', '?', '\n'])
        .map_or(intent.len(), |at| after + at);
    Some((start, end))
}

/// Whether the request distributes its draft over items: a heading word beside a
/// distributive or file-name cue anywhere in the request or the plan's elements, or a
/// draft evidence or plan trigger led by a distributive quantifier. Distributive words
/// inside one draft's object with a single file and a single length cap ("l'essentiel de
/// chaque note … en un seul fichier … max 12 lignes") stay one draft.
pub(super) fn per_item(intent: &str, plan: &Plan) -> bool {
    if heading_beside_distributive(intent) {
        return true;
    }
    let elements = plan
        .constraints
        .iter()
        .map(String::as_str)
        .chain(plan.effects.iter().map(|e| e.target.as_str()))
        .chain(plan.effects.iter().map(|e| e.evidence.as_str()));
    for text in elements {
        if heading_beside_distributive(text) {
            return true;
        }
    }
    plan.steps
        .iter()
        .filter(|s| s.op == Op::Draft)
        .any(|s| led_by_quantifier(&s.evidence))
        || plan.trigger.as_deref().is_some_and(led_by_quantifier)
}

/// Distributive words that lead an object ("each ticket", "chaque ligne", "every row").
const DISTRIBUTIVE_LEADS: &[&str] = &[
    "each", "every", "chaque", "chacun", "chacune", "cada", "ogni", "ciascun", "ciascuna", "jede",
    "jeden", "jedes", "jeder",
];

/// Whether an object distributes its work over items: led by a distributive word or a
/// quantifier ("each ticket as bug or feature", "for each file …"), or scoped by a
/// distributive tail ("… of each one").
pub(super) fn distributive(text: &str) -> bool {
    let padded = padded(text);
    led_by_quantifier(text)
        || DISTRIBUTIVE_LEADS
            .iter()
            .any(|w| padded.starts_with(&format!(" {w} ")))
        || without_distributive_tail(text).len() < text.trim().len()
}

/// Whether the request distributes its extract over the read items: an extract step whose
/// object or clause is scoped to each item ("the supplier, the date and the amount of each
/// one", "pour chaque facture, extrais …"). One record per item is then produced, and the
/// step never sees the folded corpus.
pub(super) fn per_item_extract(plan: &Plan) -> bool {
    plan.steps
        .iter()
        .filter(|s| s.op == Op::Extract)
        .any(|s| distributive(&s.detail) || led_by_quantifier(&s.evidence))
}

/// Whether the request classifies each record of its source ("classify each ticket as bug
/// or feature"): one category per parsed record, so a write naming a category carries the
/// records routed to it.
pub(super) fn per_record_classify(plan: &Plan) -> bool {
    plan.steps
        .iter()
        .filter(|s| s.op == Op::Classify)
        .any(|s| distributive(&s.detail) || led_by_quantifier(&s.evidence))
}

/// Cues that the request supplies its material at invocation ("summarize the supplied
/// text", "le texte ci-dessous", "each incoming request"), folded, whole words or phrases.
const SUPPLIED_CUES: &[&str] = &[
    "supplied",
    "provided",
    "attached",
    "given",
    "pasted",
    "below",
    "following",
    "incoming",
    "this text",
    "the text",
    "this document",
    "the document",
    "this message",
    "the message",
    "the input",
    "each request",
    "every request",
    "fourni",
    "fournie",
    "fournis",
    "fournies",
    "ci-joint",
    "ci-jointe",
    "ci-dessous",
    "suivant",
    "suivante",
    "entrant",
    "entrante",
    "ce texte",
    "le texte",
    "ce document",
    "le document",
    "ce message",
    "le message",
    "chaque demande",
    "adjunto",
    "proporcionado",
    "este texto",
    "el texto",
    "allegato",
    "fornito",
    "questo testo",
    "il testo",
    "beigefugt",
    "dieser text",
    "der text",
];

/// Whether the request names material an invocation supplies, so a plan without a source
/// step still works on something real.
pub(super) fn names_supplied_material(intent: &str) -> bool {
    let padded = padded(intent);
    SUPPLIED_CUES
        .iter()
        .any(|cue| padded.contains(&format!(" {cue} ")))
}

/// A constraint the fan-in structure realizes (order, one heading per item): consumed
/// out of the prompts when the work is distributed.
pub(super) fn structural(constraint: &str) -> bool {
    let padded = padded(constraint);
    STRUCTURAL_CUES
        .iter()
        .any(|cue| padded.contains(&format!(" {cue} ")))
        || HEADING_WORDS
            .iter()
            .any(|word| padded.contains(&format!(" {word} ")))
}

/// The first identifier token of a phrase: digits beside letters (`T-4471`, `SKU_12`,
/// `#88240`) or an email. Never a bare number, a date-like run of digits and hyphens, a
/// path or a URL.
pub(super) fn identifier(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| {
                matches!(
                    c,
                    '.' | ',' | ';' | ':' | '(' | ')' | '"' | '\'' | '«' | '»' | '!' | '?'
                )
            })
        })
        .filter(|w| !w.is_empty())
        .find(|w| {
            let path_or_url =
                w.contains("://") || w.contains('/') || w.starts_with('.') || w.starts_with('~');
            if path_or_url {
                return false;
            }
            if w.contains('@') && w.contains('.') && !w.starts_with('@') {
                return true;
            }
            let digits = w.chars().any(|c| c.is_ascii_digit());
            let letters = w.chars().any(char::is_alphabetic);
            let joiners = w
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '#'));
            digits && joiners && (letters || w.starts_with('#'))
        })
        .map(str::to_owned)
}

/// A lookup that selects one record by a literal identifier in one JSON file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LiteralLookup {
    /// The one JSON file the detail names.
    pub file: String,
    /// The identifier token, verbatim.
    pub id: String,
    /// The slug of the detail without the identifier (`ticket`), for constant names.
    pub slug: String,
}

/// A lookup detail that names exactly one JSON file and an identifier token binds the
/// file without a directory question and selects the one record at run time.
pub(super) fn lookup_by_identifier(detail: &str) -> Option<LiteralLookup> {
    let file = match super::paths::literals(detail).as_slice() {
        [super::paths::PathShape::File(path)]
            if super::paths::extension(path).as_deref() == Some("json") =>
        {
            path.clone()
        }
        _ => return None,
    };
    literal_lookup(detail, file)
}

/// A lookup detail that names an identifier token and no path at all (« ticket 42 »)
/// selects its record in the ONE JSON file the request already reads (« Read
/// ./tickets.json, find ticket 42 »): the read clause locates the material, so no second
/// « directory » is asked. A detail that names a path of its own keeps its own law.
pub(super) fn lookup_by_identifier_over(detail: &str, located: &str) -> Option<LiteralLookup> {
    if !super::paths::literals(detail).is_empty()
        || super::paths::extension(located).as_deref() != Some("json")
    {
        return None;
    }
    literal_lookup(detail, located.to_owned())
}

/// The bare number that names a record in a LOOKUP detail: « ticket 42 », « order 1002 »,
/// « le ticket 42 » — one all-digit token right after a word of letters. Only a lookup
/// detail reads a bare number this way (a lookup selects one record); everywhere else a
/// bare number stays a count or a bound, which [`identifier`] deliberately never returns.
fn numbered_record(detail: &str) -> Option<String> {
    let words: Vec<&str> = detail
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect();
    let numbers: Vec<usize> = words
        .iter()
        .enumerate()
        .filter(|(_, w)| w.chars().all(|c| c.is_ascii_digit()))
        .map(|(i, _)| i)
        .collect();
    match numbers.as_slice() {
        [at] if *at > 0 && words[at - 1].chars().all(char::is_alphabetic) => {
            Some(words[*at].to_owned())
        }
        _ => None,
    }
}

fn literal_lookup(detail: &str, file: String) -> Option<LiteralLookup> {
    let id = identifier(detail).or_else(|| numbered_record(detail))?;
    let rest: Vec<&str> = detail
        .split_whitespace()
        .filter(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && c != '#') != id
        })
        .collect();
    let slug = super::lexicon::slug(&rest.join(" "));
    Some(LiteralLookup { file, id, slug })
}

/// The topology the assembler realized for one plan; recorded in provenance, never
/// authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    /// The corpus is several files read in a bounded fan-out.
    pub fan_out: bool,
    /// The draft runs once per read item and is folded back in item order.
    pub per_item: bool,
    /// Written files plus wired endpoints.
    pub outputs: usize,
    /// A human gate dominates at least one effect.
    pub gated: bool,
}

impl Shape {
    pub(super) const fn word(self) -> &'static str {
        if self.gated {
            "human_gated"
        } else if self.outputs > 1 {
            "multiple_outputs"
        } else if self.fan_out && self.per_item {
            "fan_out_fan_in"
        } else if self.fan_out {
            "fan_out_fold"
        } else {
            "linear"
        }
    }
    pub(super) fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "word": self.word(),
            "fan_out": self.fan_out,
            "per_item": self.per_item,
            "outputs": self.outputs,
            "gated": self.gated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAPTERS: &str = "For each of the four files ./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md, at most 2 at a time, write a two-sentence summary. Then merge the summaries in exactly that order into ./out/digest.md, with one heading per file named after the file.";
    const CATALOG: &str = "For each of these four product slugs - solar-lamp, wind-chime, rain-barrel, compost-bin - read ./catalog/<slug>.md and draft one two-sentence marketing blurb in a warm, down-to-earth tone. Process at most 2 products at a time, then merge all four blurbs in the listed order into a single ./out/catalog-blurbs.md with the product name as a heading above each blurb.";
    const NOTES: &str = "bon alors jai un dossier ./notes avec plein de fichiers .md de reunion de la semaine faut que tu me fasse un resumé de chaque en 3 lignes max et que tu me mette tout ca dans ./out/recap-semaine.md avec le nom du fichier en titre stp pas de blabla juste les decisions et les trucs a faire merci";
    const ONE_FILE: &str = "resume moi tout les notes qui sont dans le dossier ./notes en un seul fichier ./out/resume.md max 12 lignes stp jveux juste lessentiel de chaque note c urgent merci";
    const ORDERS: &str = "Read the CSV at ./data/orders-2026-09.csv and keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON to ./out/shipped-by-country.json, then write a short Markdown note naming the top 3 countries to ./out/summary.md.";

    #[test]
    fn per_item_is_a_heading_beside_a_distributive_cue_or_a_leading_quantifier() {
        let bare = Plan::default();
        assert!(
            per_item(CHAPTERS, &bare),
            "heading per file named after the file"
        );
        assert!(per_item(CATALOG, &bare), "a heading above each blurb");
        assert!(per_item(NOTES, &bare), "le nom du fichier en titre");
        // The French notes case: distributive words inside one draft's object, a single
        // file, a single length cap.
        let mut one = Plan::default();
        one.steps = vec![Step::new(
            Op::Draft,
            "resume moi tout les notes qui sont dans le dossier ./notes",
            "tout les notes … lessentiel de chaque note",
            Vec::new(),
        )];
        one.constraints = vec!["max 12 lignes".to_owned()];
        assert!(!per_item(ONE_FILE, &one));
        assert!(
            !per_item(ORDERS, &bare),
            "per country is a grouping, no heading"
        );
        // A draft evidence or a trigger led by the quantifier distributes the work.
        let mut led = Plan::default();
        led.steps = vec![Step::new(
            Op::Draft,
            "For each of the four files, write a two-sentence summary",
            "a two-sentence summary",
            Vec::new(),
        )];
        assert!(per_item("write a two-sentence summary", &led));
        let mut triggered = Plan::default();
        triggered.trigger = Some("pour chaque fichier".to_owned());
        assert!(per_item("résume", &triggered));
        // A plan element carrying the heading cue counts even when the intent text is bare.
        let mut constrained = Plan::default();
        constrained.constraints = vec!["with one heading per file named after the file".to_owned()];
        assert!(per_item("x", &constrained));
    }

    #[test]
    fn structural_constraints_are_order_and_heading_cues() {
        assert!(structural("in exactly that order"));
        assert!(structural("with one heading per file named after the file"));
        assert!(structural("dans l'ordre des fichiers"));
        assert!(structural("avec le nom du fichier en titre"));
        assert!(!structural("3 lignes max"));
        assert!(!structural("in a warm, down-to-earth tone"));
        assert!(!structural("Process at most 2 products at a time"));
    }

    #[test]
    fn an_identifier_is_digits_beside_letters_never_a_bare_number_a_path_or_a_url() {
        assert_eq!(
            identifier("ticket T-4471 in ./data/tickets.json").as_deref(),
            Some("T-4471")
        );
        assert_eq!(identifier("order #88240").as_deref(), Some("#88240"));
        assert_eq!(identifier("the SKU_12 entry").as_deref(), Some("SKU_12"));
        assert_eq!(
            identifier("the customer omar@example.org").as_deref(),
            Some("omar@example.org")
        );
        assert_eq!(identifier("ticket (T-4471)").as_deref(), Some("T-4471"));
        for none in [
            "order 88240",
            "./data/tickets.json",
            "https://example.invalid/t/T-4471",
            "orders from 2026-09-20",
            "le client",
            "12.5",
        ] {
            assert_eq!(identifier(none), None, "{none}");
        }
        assert_eq!(
            lookup_by_identifier("ticket T-4471 in ./data/tickets.json"),
            Some(LiteralLookup {
                file: "./data/tickets.json".to_owned(),
                id: "T-4471".to_owned(),
                slug: "ticket".to_owned(),
            })
        );
        assert_eq!(
            lookup_by_identifier("le dossier D-12 dans ./data/dossiers.json")
                .map(|l| l.slug)
                .as_deref(),
            Some("dossier")
        );
        // No file, a CSV, or no identifier: the plain lookup with its directory question.
        assert_eq!(lookup_by_identifier("ticket T-4471"), None);
        assert_eq!(
            lookup_by_identifier("ticket T-4471 in ./data/tickets.csv"),
            None
        );
        assert_eq!(
            lookup_by_identifier("the customer in ./customers.json"),
            None
        );
    }

    #[test]
    fn a_shape_has_one_word() {
        let shape = |fan_out, per_item, outputs, gated| Shape {
            fan_out,
            per_item,
            outputs,
            gated,
        };
        assert_eq!(shape(false, false, 1, false).word(), "linear");
        assert_eq!(shape(true, false, 1, false).word(), "fan_out_fold");
        assert_eq!(shape(true, true, 1, false).word(), "fan_out_fan_in");
        assert_eq!(shape(false, false, 2, false).word(), "multiple_outputs");
        assert_eq!(shape(true, true, 1, true).word(), "human_gated");
        assert_eq!(
            shape(true, true, 1, false).to_json(),
            serde_json::json!({"word": "fan_out_fan_in", "fan_out": true, "per_item": true, "outputs": 1, "gated": false})
        );
    }

    fn step(op: Op, detail: &str, evidence: &str) -> Step {
        Step::new(op, evidence, detail, Vec::new())
    }

    #[test]
    fn a_numeric_rule_is_a_digit_beside_a_comparison_cue() {
        for rule in [
            "keep only the rows whose amount is strictly greater than 100",
            "whose total_eur is above 120",
            "products with fewer than 10 units",
            "orders whose total is at least 50 EUR",
            "amount > 100",
            "amount >= 100",
            "montant ≥ 100",
            "quantité inférieure à 10",
            "les commandes dont le montant est plus grand que 200",
            "au moins 3 articles en stock",
            "productos con menos de 10 unidades",
            "cantidad mayor que 5",
            "articoli con quantità minore di 5",
            "Bestellungen mit Betrag größer als 100",
            "mindestens 3 Stück",
        ] {
            assert!(numeric_rule(rule), "{rule}");
        }
        for guidance in [
            "a brief of under 150 words",
            "un résumé de chaque en 3 lignes max",
            "at most 3 attempts",
            "no more than 5 turns",
            "Process at most 2 products at a time",
            "traite 3 fichiers à la fois",
            "the top 3 countries",
            "as 5 bullets",
            "in a warm tone",
            "at most 2 pages",
            "au plus 4 phrases",
            "never retry more than 3 times",
            "at most two of them",
        ] {
            assert!(!numeric_rule(guidance), "{guidance}");
        }
    }

    #[test]
    fn a_numeric_rule_constraint_becomes_a_compute_step_after_the_sources() {
        let intent = "Read ./data/orders.csv, keep only the rows whose amount is strictly greater than 100, and write ./out/summary.md with one line stating the total.";
        let rule = "keep only the rows whose amount is strictly greater than 100";
        let mut plan = Plan::default();
        plan.steps = vec![
            step(Op::Read, "./data/orders.csv", "Read ./data/orders.csv"),
            step(
                Op::Draft,
                "one line stating the total",
                "one line stating the total",
            ),
        ];
        plan.constraints = vec![rule.to_owned(), "in a warm tone".to_owned()];
        promote_stated_rules(&mut plan, intent);
        let ops: Vec<Op> = plan.steps.iter().map(|s| s.op).collect();
        assert_eq!(ops, [Op::Read, Op::Compute, Op::Draft]);
        assert_eq!(plan.steps[1].detail, rule);
        assert_eq!(plan.steps[1].evidence, rule);
        assert_eq!(plan.constraints, ["in a warm tone"]);
        // Idempotent.
        let once = plan.clone();
        promote_stated_rules(&mut plan, intent);
        assert_eq!(plan, once);
        // A rule wrapped by the model is anchored through its folded excerpt.
        let mut wrapped = Plan::default();
        wrapped.steps = vec![step(
            Op::Read,
            "./data/orders.csv",
            "Read ./data/orders.csv",
        )];
        wrapped.constraints =
            vec!["keep only the rows whose  amount is strictly\ngreater than 100".to_owned()];
        promote_stated_rules(&mut wrapped, intent);
        assert_eq!(wrapped.steps[1].evidence, rule);
        assert!(wrapped.constraints.is_empty());
        // A rule the request never spelled stays guidance: nothing is invented.
        let mut foreign = Plan::default();
        foreign.steps = vec![step(
            Op::Read,
            "./data/orders.csv",
            "Read ./data/orders.csv",
        )];
        foreign.constraints = vec!["amount above 500".to_owned()];
        promote_stated_rules(&mut foreign, intent);
        assert_eq!(foreign.steps.len(), 1);
        assert_eq!(foreign.constraints, ["amount above 500"]);
        // No source step: the rule leads the plan.
        let mut sourceless = Plan::default();
        sourceless.steps = vec![step(Op::Draft, "the total", "the total")];
        sourceless.constraints = vec![rule.to_owned()];
        promote_stated_rules(&mut sourceless, intent);
        assert_eq!(sourceless.steps[0].op, Op::Compute);
        // An existing compute step absorbs a second rule instead of a second step.
        let mut two = Plan::default();
        two.steps = vec![
            step(Op::Read, "./data/orders.csv", "Read ./data/orders.csv"),
            step(Op::Compute, "the total", "the total"),
        ];
        two.constraints = vec![rule.to_owned()];
        promote_stated_rules(&mut two, intent);
        assert_eq!(two.steps.len(), 2);
        assert_eq!(two.steps[1].detail, format!("the total ; {rule}"));
    }

    #[test]
    fn a_stated_filter_is_promoted_and_recorded_but_a_prohibition_stays_prose() {
        let intent = "Read ./tickets.json, keep only the tickets whose status is open, and write them to ./open.json";
        let rule = "keep only the tickets whose status is open";
        let mut plan = Plan::default();
        plan.steps = vec![step(Op::Read, "./tickets.json", "Read ./tickets.json")];
        plan.constraints = vec![rule.to_owned()];
        promote_stated_rules(&mut plan, intent);
        let ops: Vec<Op> = plan.steps.iter().map(|s| s.op).collect();
        assert_eq!(ops, [Op::Read, Op::Compute]);
        assert!(plan.constraints.is_empty());
        assert_eq!(plan.rules.len(), 1);
        assert_eq!(plan.rules[0].text(), rule);
        assert_eq!(
            plan.rules[0].jq(),
            "[.records[] | select(.status == \"open\")]"
        );
        // The French restriction "ne … que" is "only", a filter; the negation "ne … pas"
        // is a prohibition, never promoted, never inverted.
        let intent = "Lis ./sales.csv, ne garde que les lignes dont amount dépasse 200 et écris-les dans ./big.csv";
        let mut plan = Plan::default();
        plan.steps = vec![step(Op::Read, "./sales.csv", "Lis ./sales.csv")];
        plan.constraints = vec!["ne garde que les lignes dont amount dépasse 200".to_owned()];
        promote_stated_rules(&mut plan, intent);
        assert_eq!(plan.steps.len(), 2, "{plan:?}");
        assert_eq!(
            plan.rules
                .first()
                .map(super::super::rules::Rule::jq)
                .as_deref(),
            Some("[.records[] | select((.amount | tonumber) > 200)]")
        );
        let intent = "Lis ./sales.csv, ne garde pas les lignes dont amount dépasse 200 et écris-les dans ./big.csv";
        let mut plan = Plan::default();
        plan.steps = vec![step(Op::Read, "./sales.csv", "Lis ./sales.csv")];
        plan.constraints = vec!["ne garde pas les lignes dont amount dépasse 200".to_owned()];
        promote_stated_rules(&mut plan, intent);
        assert_eq!(plan.steps.len(), 1, "{plan:?}");
        assert_eq!(plan.constraints.len(), 1);
        assert!(plan.rules.is_empty());
        for prohibition in [
            "never keep closed tickets",
            "do not keep the tickets whose status is closed",
            "Read ./tickets.json, do not keep the tickets whose status is closed",
            "Read ./tickets.json and never keep the tickets whose status is closed",
            "ne garde pas les lignes dont amount dépasse 200",
            "ne garde jamais les lignes dont amount dépasse 200",
        ] {
            assert!(prohibits(prohibition), "{prohibition}");
        }
        for restriction in [
            "ne garde que les lignes",
            "n'écris que les lignes",
            "keep only the rows",
        ] {
            assert!(!prohibits(restriction), "{restriction}");
        }
        // A constraint an existing computation already carries is consumed without a
        // recorded rule: the step may say more than the constraint (a grouping here), and
        // the part must not stand for the whole.
        let intent = "Read ./o.csv, keep only the rows whose amount is above 100 and count them per country, then write it to ./c.json";
        let detail = "keep only the rows whose amount is above 100 and count them per country";
        let mut plan = Plan::default();
        plan.steps = vec![
            step(Op::Read, "./o.csv", "Read ./o.csv"),
            step(Op::Compute, detail, detail),
        ];
        plan.constraints = vec!["keep only the rows whose amount is above 100".to_owned()];
        promote_stated_rules(&mut plan, intent);
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[1].detail, detail);
        assert!(plan.constraints.is_empty());
        assert!(plan.rules.is_empty(), "{:?}", plan.rules);
    }
}
