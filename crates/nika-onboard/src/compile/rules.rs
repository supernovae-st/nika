// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A rule the request states in words ("keep only the rows whose amount is strictly
//! greater than 100", "cuya cantidad es menor que 10", "whose status is refunded") becomes
//! the jq the workflow runs, deterministically, so the compiler never asks for an
//! expression the request already states.
//!
//! The grammar is closed. A clause is a FIELD that names a column (an identifier token, a
//! word of the columns hint the request states, the noun phrase between a relative pronoun
//! and the comparison, or the word left of a comparison symbol), a COMPARATOR (a symbol or
//! a multilingual cue, with a copula for equality), and a VALUE (a number, a quoted or bare
//! word for equality, or a second column). Clauses join through one conjunction. Anything
//! the grammar does not cover is `None`: the human is asked, nothing is guessed. Every
//! expression shape emitted here was run on the engine's jq before it was written down.

use super::shape::{ATTEMPT_UNITS, SIZE_UNITS, fold};
use serde_json::{Value, json};

pub(super) use super::aggregate::{AggOp, Aggregation, Shape};

/// The six comparisons a rule may state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Comparator {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

impl Comparator {
    /// The comparator that holds exactly when this one does not.
    pub(super) fn negated(self) -> Self {
        match self {
            Self::Gt => Self::Le,
            Self::Ge => Self::Lt,
            Self::Lt => Self::Ge,
            Self::Le => Self::Gt,
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
        }
    }

    /// A comparator named by a word or a symbol (`gt`, `>=`, `eq`, `<>`).
    pub(super) fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            ">" | "gt" | "greater" => Some(Self::Gt),
            ">=" | "≥" | "ge" | "gte" => Some(Self::Ge),
            "<" | "lt" | "less" => Some(Self::Lt),
            "<=" | "≤" | "le" | "lte" => Some(Self::Le),
            "==" | "=" | "eq" | "equals" => Some(Self::Eq),
            "!=" | "<>" | "≠" | "ne" => Some(Self::Ne),
            _ => None,
        }
    }
    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Eq => "==",
            Self::Ne => "!=",
        }
    }
    const fn numeric(self) -> bool {
        !matches!(self, Self::Eq | Self::Ne)
    }
}

/// Comparison cues (EN · FR · ES · IT · PT · DE, diacritics folded, lowercase), one
/// `|`-separated list per comparator. Matched as whole phrases, longest first, up to
/// five words.
const NUMERIC_CUES: &[(Comparator, &str)] = &[
    (
        Comparator::Gt,
        "strictly greater than|strictly more than|strictly higher than|strictly above|\
         greater than|more than|higher than|bigger than|larger than|above|over|exceeds|\
         exceeding|\
         strictement superieur a|strictement superieure a|strictement superieurs a|\
         strictement superieures a|strictement plus grand que|strictement plus grande que|\
         plus grand que|plus grande que|plus grands que|plus grandes que|plus eleve que|\
         plus elevee que|superieur a|superieure a|superieurs a|superieures a|superieur au|\
         superieure au|superieur aux|superieure aux|au-dessus de|au dessus de|plus de|\
         depasse|depassant|\
         estrictamente mayor que|estrictamente mayor a|mayor que|mayores que|mayor a|\
         mayores a|mayor al|superior a|superiores a|superior al|por encima de|mas de|\
         mas que|supera|superan|\
         strettamente maggiore di|maggiore di|maggiori di|maggiore del|maggiore della|\
         superiore a|superiori a|superiore al|superiore alla|superiore allo|al di sopra di|\
         piu grande di|piu alto di|piu di|superano|\
         estritamente maior que|maior do que|maior que|maiores que|superior ao|acima de|\
         mais de|mais que|excede|excedem|\
         strikt grosser als|echt grosser als|grosser als|hoher als|mehr als|oberhalb von|\
         ubersteigt|ubersteigen|uber",
    ),
    (
        Comparator::Ge,
        "greater than or equal to|more than or equal to|not less than|not fewer than|\
         no less than|no fewer than|at least|\
         superieur ou egal a|superieure ou egale a|superieurs ou egaux a|\
         superieures ou egales a|pas moins de|au moins|au minimum|\
         mayor o igual que|mayor o igual a|mayores o iguales que|mayores o iguales a|\
         no menos de|por lo menos|al menos|como minimo|\
         maggiore o uguale a|maggiori o uguali a|non meno di|almeno|al minimo|\
         maior ou igual a|maiores ou iguais a|nao menos de|pelo menos|ao menos|no minimo|\
         grosser oder gleich|grosser gleich|nicht weniger als|mindestens|wenigstens",
    ),
    (
        Comparator::Lt,
        "strictly less than|strictly lower than|strictly fewer than|strictly below|\
         less than|fewer than|lower than|smaller than|below|under|\
         strictement inferieur a|strictement inferieure a|strictement inferieurs a|\
         strictement inferieures a|strictement plus petit que|strictement plus petite que|\
         plus petit que|plus petite que|plus petits que|plus petites que|plus bas que|\
         plus basse que|inferieur a|inferieure a|inferieurs a|inferieures a|inferieur au|\
         inferieure au|inferieur aux|inferieure aux|en dessous de|en-dessous de|\
         au-dessous de|moins de|\
         estrictamente menor que|estrictamente menor a|menor que|menores que|menor a|\
         menores a|menor al|inferior a|inferiores a|inferior al|por debajo de|menos de|\
         menos que|\
         strettamente minore di|minore di|minori di|minore del|minore della|inferiore a|\
         inferiori a|inferiore al|inferiore alla|inferiore allo|al di sotto di|\
         piu piccolo di|piu basso di|meno di|\
         estritamente menor que|menor do que|inferior ao|abaixo de|\
         strikt kleiner als|echt kleiner als|kleiner als|niedriger als|weniger als|\
         unterhalb von|unter",
    ),
    (
        Comparator::Le,
        "less than or equal to|fewer than or equal to|not more than|no more than|at most|\
         inferieur ou egal a|inferieure ou egale a|inferieurs ou egaux a|\
         inferieures ou egales a|pas plus de|au plus|au maximum|\
         menor o igual que|menor o igual a|menores o iguales que|menores o iguales a|\
         no mas de|como maximo|a lo sumo|\
         minore o uguale a|minori o uguali a|non piu di|al massimo|\
         menor ou igual a|menores ou iguais a|nao mais de|no maximo|\
         kleiner oder gleich|kleiner gleich|nicht mehr als|hochstens|maximal",
    ),
];

/// Equality and inequality cues that carry their own verb (a copula alone is equality).
const EQUALITY_CUES: &[(Comparator, &str)] = &[
    (
        Comparator::Eq,
        "equal to|equals to|equals|equal|egal a|egale a|egaux a|egales a|vaut|valent|\
         igual a|iguales a|iguais a|vale|valen|uguale a|uguali a|gleich|entspricht",
    ),
    (
        Comparator::Ne,
        "not equal to|unequal to|differs from|different from|other than|different de|\
         differente de|differents de|differentes de|distinto de|distinta de|distintos de|\
         distintas de|diferente de|diferentes de|diverso da|diversa da|diversi da|\
         diverse da|ungleich|anders als|verschieden von",
    ),
];

/// The longest cue phrase, in words.
const CUE_WIDTH: usize = 5;

/// A copula: the field is the phrase before it, the comparison (or the equality value)
/// follows it.
const COPULAS: &[&str] = &[
    "is", "are", "was", "were", "be", "being", "has", "have", "est", "sont", "n'est", "es", "son",
    "esta", "estan", "e", "sao", "ist", "sind", "ha", "hanno", "tiene", "tienen", "tem", "hat",
    "haben",
];

/// A copula that carries its own negation.
const NEGATED_COPULAS: &[&str] = &["isn't", "aren't", "wasn't", "weren't", "n'est", "n'a"];

/// A negation right before or right after a copula.
const NEGATIONS: &[&str] = &["not", "pas", "no", "non", "nao", "ne", "nicht"];

/// A relative pronoun or preposition that opens the noun phrase naming the field
/// ("whose amount", "dont le montant", "cuya cantidad", "la cui quantita", "deren Betrag").
const RELATIVES: &[&str] = &[
    "whose",
    "where",
    "which",
    "that",
    "with",
    "having",
    "in which",
    "for which",
    "dont",
    "avec",
    "ayant",
    "cuya",
    "cuyo",
    "cuyas",
    "cuyos",
    "donde",
    "con",
    "la cui",
    "il cui",
    "le cui",
    "i cui",
    "cui",
    "cujo",
    "cuja",
    "cujos",
    "cujas",
    "deren",
    "dessen",
    "mit",
    "wo",
    "que",
];

/// Articles and possessives stripped from the head of a noun phrase.
const ARTICLES: &[&str] = &[
    "the", "a", "an", "its", "their", "le", "la", "les", "l'", "l", "un", "une", "des", "du", "de",
    "sa", "son", "ses", "leur", "leurs", "el", "los", "las", "una", "unos", "unas", "su", "sus",
    "il", "lo", "i", "gli", "uno", "suo", "sua", "suoi", "sue", "o", "os", "as", "um", "uma",
    "seu", "seus", "suas", "der", "die", "das", "den", "dem", "ein", "eine", "einer", "einem",
    "einen", "sein", "seine", "seiner", "ihr", "ihre", "ihrer", "ihren",
];

/// Words skipped between a comparator and its value.
const FILLERS: &[&str] = &[
    "than", "que", "als", "di", "de", "da", "a", "of", "to", "the", "le", "la", "les", "el", "los",
    "las", "il", "lo", "der", "die", "das", "den", "dem", "del", "della", "dello", "dei", "degli",
    "delle", "du", "des", "do", "dos", "ao", "au", "aux", "al", "alla", "allo", "ai", "agli",
    "alle", "zu", "zum", "zur",
];

/// A unit or currency word that may trail a numeric value without changing the rule.
const UNIT_WORDS: &[&str] = &[
    "€",
    "$",
    "£",
    "eur",
    "euro",
    "euros",
    "usd",
    "dollar",
    "dollars",
    "gbp",
    "pound",
    "pounds",
    "chf",
    "cent",
    "cents",
    "centimes",
    "unit",
    "units",
    "unite",
    "unites",
    "unidad",
    "unidades",
    "unita",
    "unidade",
    "einheit",
    "einheiten",
    "stuck",
    "stueck",
    "piece",
    "pieces",
    "pieza",
    "piezas",
    "pezzo",
    "pezzi",
    "peca",
    "pecas",
    "item",
    "items",
    "article",
    "articles",
    "articulo",
    "articulos",
    "articolo",
    "articoli",
    "artikel",
    "kg",
    "g",
    "grams",
    "grammes",
    "cm",
    "mm",
    "m",
    "km",
    "l",
    "ml",
    "percent",
    "pourcent",
    "porcento",
    "prozent",
    "day",
    "days",
    "jour",
    "jours",
    "dia",
    "dias",
    "giorno",
    "giorni",
    "tag",
    "tage",
    "hour",
    "hours",
    "heure",
    "heures",
    "hora",
    "horas",
    "ora",
    "ore",
    "stunde",
    "stunden",
    "minute",
    "minutes",
    "minutos",
    "minuti",
    "minuten",
];

/// A two-word unit phrase that may trail a numeric value.
const UNIT_PHRASES: &[&str] = &[
    "in stock",
    "en stock",
    "em estoque",
    "auf lager",
    "en inventario",
    "in magazzino",
    "on hand",
];

/// Words of a trailing count-or-total request ("how many rows were kept and the total of
/// their amounts"): the claims the summary stage computes, so a rule that carries them
/// is still a rule. `|`-separated, folded.
const SUMMARY_WORDS: &str = "how|many|much|rows|row|records|record|lines|line|entries|entry|\
    items|item|results|result|matches|match|were|was|are|is|be|been|kept|retained|\
    remaining|remain|remains|left|selected|matched|matching|filtered|found|count|counted|\
    counting|number|total|totals|totalling|totaling|sum|summed|of|their|the|a|an|its|them|\
    those|these|that|it|value|values|amount|amounts|along|with|together|plus|also|as|well|\
    combien|de|des|du|la|le|les|l|lignes|ligne|enregistrements|gardees|gardes|conservees|\
    conserves|retenues|retenus|restantes|restants|nombre|totaux|somme|montant|montants|\
    valeur|valeurs|leurs|leur|ainsi|que|ont|ete|sont|est|\
    cuantas|cuantos|filas|fila|registros|registro|quedan|quedaron|conservadas|conservados|\
    retenidas|seleccionadas|numero|suma|importe|importes|valor|valores|sus|su|fueron|son|\
    han|sido|el|los|las|un|una|\
    quante|quanti|righe|riga|restano|rimaste|rimasti|tenute|mantenute|selezionate|totale|\
    somma|importo|importi|valore|valori|loro|il|i|gli|sono|state|stati|\
    quantas|quantos|linhas|linha|registos|ficaram|mantidas|mantidos|retidas|soma|seus|\
    suas|o|os|foram|sao|\
    wie|viele|zeilen|zeile|datensatze|datensatz|blieben|bleiben|behalten|ubrig|anzahl|\
    summe|gesamt|gesamtbetrag|gesamtsumme|betrag|betrage|wert|werte|ihrer|ihre|der|die|\
    das|den|sowie|wurden|sind|\
    compute|computed|calculate|calculated|report|reported|state|stating|stated|give|\
    return|output|produce|calcule|calculer|calculez|indique|indiquer|donne|donner|calcula|\
    calcular|indica|indicar|calcola|calcolare|berechne|berechnen|gib|angeben";

/// The words that make such a residual a request for a count or a total, not noise.
const SUMMARY_CORE: &str = "many|count|counted|number|total|totals|sum|combien|nombre|somme|\
    cuantas|cuantos|numero|suma|quante|quanti|totale|somma|quantas|quantos|soma|viele|\
    anzahl|summe|gesamtsumme|gesamtbetrag";

fn listed(table: &str, word: &str) -> bool {
    table.split('|').any(|w| w == word)
}

fn cue_in(table: &[(Comparator, &str)], phrase: &str) -> Option<Comparator> {
    table
        .iter()
        .find(|(_, cues)| cues.split('|').any(|cue| cue == phrase))
        .map(|(comparator, _)| *comparator)
}

/// The comparator a folded phrase states as a numeric comparison, if any.
pub(super) fn numeric_cue(phrase: &str) -> Option<Comparator> {
    cue_in(NUMERIC_CUES, phrase)
}

fn equality_cue(phrase: &str) -> Option<Comparator> {
    cue_in(EQUALITY_CUES, phrase)
}

// ── tokens ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Word,
    /// A number with its canonical text (`100`, `1.5`, `-3`).
    Number(String),
    Symbol(Comparator),
    /// The exact text between a pair of quotes.
    Quoted,
}

#[derive(Clone, Debug)]
struct Token {
    /// The text as written (punctuation trimmed), for keys and values.
    original: String,
    /// Lowercase, diacritics folded, for the tables.
    folded: String,
    kind: Kind,
}

impl Token {
    fn word(&self) -> Option<&str> {
        matches!(self.kind, Kind::Word).then_some(self.folded.as_str())
    }
}

/// Comparison symbols, longest first so `>=` is never read as `>` then `=`.
const SYMBOLS: &[(&str, Comparator)] = &[
    (">=", Comparator::Ge),
    ("<=", Comparator::Le),
    ("==", Comparator::Eq),
    ("!=", Comparator::Ne),
    ("<>", Comparator::Ne),
    ("≥", Comparator::Ge),
    ("≤", Comparator::Le),
    ("≠", Comparator::Ne),
    (">", Comparator::Gt),
    ("<", Comparator::Lt),
    ("=", Comparator::Eq),
];

fn quote_close(open: char) -> Option<char> {
    match open {
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        '«' => Some('»'),
        '“' => Some('”'),
        '‘' => Some('’'),
        _ => None,
    }
}

fn is_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '!' | '?' | '"' | '«' | '»'
    )
}

/// A number's canonical text: currency and percent signs dropped, a single decimal
/// comma read as a point, thousands separators removed; anything else is not a number.
fn number(word: &str) -> Option<String> {
    let trimmed = word
        .trim_start_matches(['€', '$', '£', '+'])
        .trim_end_matches(['€', '$', '£', '%']);
    let (sign, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", trimmed),
    };
    if !digits.starts_with(|c: char| c.is_ascii_digit())
        || !digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ','))
    {
        return None;
    }
    let canonical = if digits.contains(',') {
        let groups: Vec<&str> = digits.split(',').collect();
        let thousands = !digits.contains('.')
            && groups.len() > 1
            && groups.iter().skip(1).all(|g| g.len() == 3)
            && groups.first().is_some_and(|g| (1..=3).contains(&g.len()));
        if thousands {
            digits.replace(',', "")
        } else if groups.len() == 2 && !digits.contains('.') {
            digits.replace(',', ".")
        } else {
            return None;
        }
    } else {
        digits.to_owned()
    };
    let points = canonical.matches('.').count();
    if points > 1 || canonical.ends_with('.') {
        return None;
    }
    Some(format!("{sign}{canonical}"))
}

fn push_word(word: &str, out: &mut Vec<Token>) {
    let word = word.trim_matches(is_punctuation);
    if word.is_empty() {
        return;
    }
    let symbol = SYMBOLS
        .iter()
        .filter_map(|(s, c)| word.find(s).map(|at| (at, *s, *c)))
        .min_by_key(|(at, s, _)| (*at, std::cmp::Reverse(s.len())));
    if let Some((at, symbol, comparator)) = symbol {
        if let Some(before) = word.get(..at) {
            push_word(before, out);
        }
        out.push(Token {
            original: symbol.to_owned(),
            folded: symbol.to_owned(),
            kind: Kind::Symbol(comparator),
        });
        if let Some(after) = word.get(at + symbol.len()..) {
            push_word(after, out);
        }
        return;
    }
    let kind = number(word).map_or(Kind::Word, Kind::Number);
    out.push(Token {
        original: word.to_owned(),
        folded: fold(word),
        kind,
    });
}

/// Words, numbers, symbols and quoted spans. A quote opens only at the start of a token,
/// so an apostrophe inside a word (`l'ordre`, `n'est`) stays in the word.
fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let Some(first) = rest.chars().next() else {
            break;
        };
        if let Some(close) = quote_close(first)
            && let Some(inner_from) = rest.get(first.len_utf8()..)
            && let Some(end) = inner_from.find(close)
            && let Some(inner) = inner_from.get(..end).map(str::trim)
        {
            out.push(Token {
                original: inner.to_owned(),
                folded: fold(inner),
                kind: Kind::Quoted,
            });
            rest = inner_from
                .get(end + close.len_utf8()..)
                .unwrap_or_default()
                .trim_start();
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if let Some(word) = rest.get(..end) {
            push_word(word, &mut out);
        }
        rest = rest.get(end..).unwrap_or_default().trim_start();
    }
    out
}

fn phrase(tokens: &[Token], at: usize, width: usize) -> Option<String> {
    let slice = tokens.get(at..at + width)?;
    if slice.iter().any(|t| t.word().is_none()) {
        return None;
    }
    Some(
        slice
            .iter()
            .map(|t| t.folded.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// The cue starting at `at`, longest first: the comparator and the width consumed.
fn cue_at(tokens: &[Token], at: usize) -> Option<(Comparator, usize)> {
    (1..=CUE_WIDTH).rev().find_map(|width| {
        let phrase = phrase(tokens, at, width)?;
        numeric_cue(&phrase)
            .or_else(|| equality_cue(&phrase))
            .map(|c| (c, width))
    })
}

/// A column-shaped identifier: letters, digits and underscores, and more than a plain
/// lowercase word (`total_eur`, `q1`, `unitPrice`).
pub(super) fn identifier_shaped(word: &str) -> bool {
    let mut chars = word.chars();
    let starts = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_');
    let joined = word.chars().all(|c| c.is_alphanumeric() || c == '_');
    let digit = word.chars().any(|c| c.is_ascii_digit());
    let letter = word.chars().any(char::is_alphabetic);
    let inner_upper = word.chars().skip(1).any(char::is_uppercase);
    let lower = word.chars().any(char::is_lowercase);
    starts && joined && (word.contains('_') || (digit && letter) || (inner_upper && lower))
}

fn normalized(name: &str) -> String {
    fold(name).replace([' ', '-'], "_")
}

/// The hint column a name designates, in the hint's own spelling.
fn hinted(name: &str, columns: &[String]) -> Option<String> {
    let wanted = normalized(name);
    columns.iter().find(|c| normalized(c) == wanted).cloned()
}

/// A token that names a column: a hint column when a hint exists, else an identifier.
fn column_named(token: &Token, columns: &[String]) -> Option<String> {
    token.word()?;
    if columns.is_empty() {
        identifier_shaped(&token.original).then(|| token.original.clone())
    } else {
        hinted(&token.original, columns)
    }
}

// ── the rule ─────────────────────────────────────────────────────────────────────

/// What a field is compared to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Operand {
    /// A number in canonical text, compared after `tonumber`.
    Number(String),
    /// An exact string, compared case-sensitively.
    Text(String),
    /// Another column of the same record.
    Column(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Clause {
    pub field: String,
    pub comparator: Comparator,
    pub value: Operand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Junction {
    And,
    Or,
}

impl Junction {
    const fn word(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

/// A rule synthesized from the request: its clauses, how they join, and the text it
/// came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Rule {
    text: String,
    clauses: Vec<Clause>,
    junction: Junction,
    /// The text also asked how many rows were kept or what they add up to.
    summary: bool,
    /// What happens to the rows after the filter.
    shape: Shape,
}

/// The jq path of one column: a bare identifier as `.name`, anything else bracketed.
pub(super) fn key(field: &str) -> String {
    let bare = field
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if bare {
        format!(".{field}")
    } else {
        format!(".[{}]", json!(field))
    }
}

impl Clause {
    fn jq(&self) -> String {
        let field = key(&self.field);
        match (&self.value, self.comparator.numeric()) {
            (Operand::Number(n), _) => {
                format!("({field} | tonumber) {} {n}", self.comparator.symbol())
            }
            (Operand::Column(other), true) => format!(
                "({field} | tonumber) {} ({} | tonumber)",
                self.comparator.symbol(),
                key(other)
            ),
            (Operand::Column(other), false) => {
                format!("{field} {} {}", self.comparator.symbol(), key(other))
            }
            (Operand::Text(text), _) => {
                format!("{field} {} {}", self.comparator.symbol(), json!(text))
            }
        }
    }
    fn to_json(&self) -> Value {
        let (value, kind) = match &self.value {
            Operand::Number(n) => (n, "number"),
            Operand::Text(t) => (t, "text"),
            Operand::Column(c) => (c, "column"),
        };
        json!({"field": self.field, "comparator": self.comparator.symbol(), "value": value, "value_kind": kind})
    }
    fn from_json(value: &Value) -> Option<Self> {
        let field = value.get("field")?.as_str()?.trim().to_owned();
        let comparator = Comparator::from_word(value.get("comparator")?.as_str()?)?;
        let literal = value.get("value")?.as_str()?.to_owned();
        let operand = match value.get("value_kind").and_then(Value::as_str) {
            Some("number") => Operand::Number(literal),
            Some("column") => Operand::Column(literal),
            _ => Operand::Text(literal),
        };
        if field.is_empty() {
            return None;
        }
        Some(Self {
            field,
            comparator,
            value: operand,
        })
    }
}

impl Rule {
    /// A rule the semantic frontend stated as a typed predicate over the request's own
    /// columns and literals, validated by the compiler; lowered exactly like a parsed one.
    pub(super) fn typed(
        text: &str,
        clauses: Vec<Clause>,
        junction: Junction,
        shape: Shape,
    ) -> Self {
        Self {
            text: text.to_owned(),
            clauses,
            junction,
            summary: false,
            shape,
        }
    }
    /// The columns the computation writes, in order, when it fixes them.
    pub(super) fn output_columns(&self) -> Option<Vec<String>> {
        self.shape.output_columns()
    }
    /// The names of the totals, when the computation is totals over every row.
    pub(super) fn totals_names(&self) -> Vec<String> {
        self.shape.totals_names()
    }
    /// The excerpt the rule was read from.
    pub(super) fn text(&self) -> &str {
        &self.text
    }
    /// The inverse of [`Rule::to_json`], for a recorded plan replayed on an answer round.
    pub(super) fn from_json(value: &Value) -> Option<Self> {
        let text = value.get("text")?.as_str()?.to_owned();
        let clauses = value
            .get("clauses")?
            .as_array()?
            .iter()
            .map(Clause::from_json)
            .collect::<Option<Vec<_>>>()?;
        let shape = Shape::from_json(value.get("shape"))?;
        if clauses.is_empty() && shape == Shape::default() {
            return None;
        }
        let junction = match value.get("junction").and_then(Value::as_str) {
            Some("or") => Junction::Or,
            _ => Junction::And,
        };
        let summary = value
            .get("summary")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Some(Self {
            text,
            clauses,
            junction,
            summary,
            shape,
        })
    }
    /// Whether the text also asked for the count and totals the summary stage computes.
    pub(super) const fn summary(&self) -> bool {
        self.summary
    }
    /// Every source column the rule reads, first use first: the clauses, the group column,
    /// the aggregated columns, a sort or a projection on a source column (a sort or a
    /// projection on a produced name reads nothing from the source).
    pub(super) fn fields(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |name: &str| {
            if !out.iter().any(|f| f == name) {
                out.push(name.to_owned());
            }
        };
        for clause in &self.clauses {
            push(&clause.field);
            if let Operand::Column(other) = &clause.value {
                push(other);
            }
        }
        let shape = &self.shape;
        let produced = shape.produced();
        if let Some(group) = &shape.group_by {
            push(group);
        }
        for aggregation in &shape.aggregations {
            if let Some(field) = &aggregation.field {
                push(field);
            }
        }
        if let Some((field, _)) = &shape.sort_by
            && !produced.contains(&field.as_str())
        {
            push(field);
        }
        if produced.is_empty() {
            for column in &shape.columns {
                push(column);
            }
        }
        out
    }
    fn predicate(&self) -> String {
        self.clauses
            .iter()
            .map(Clause::jq)
            .collect::<Vec<_>>()
            .join(&format!(" {} ", self.junction.word()))
    }
    /// The computation over the parsed records: the filter, then the shape's stages in
    /// their fixed order.
    pub(super) fn jq(&self) -> String {
        let filtered = if self.clauses.is_empty() {
            ".records".to_owned()
        } else {
            format!("[.records[] | select({})]", self.predicate())
        };
        self.shape.lower(filtered)
    }
    /// True when the records are an array whose first record carries every column the
    /// rule reads (an empty array passes): a wrong column fails loudly, never filters
    /// everything in silence.
    pub(super) fn guard(&self) -> String {
        let has = self
            .fields()
            .iter()
            .map(|f| format!(" and has({})", json!(f)))
            .collect::<Vec<_>>()
            .concat();
        format!(
            "(.records | type) == \"array\" and ((.records | length) == 0 or (.records[0] | type == \"object\"{has}))"
        )
    }
    pub(super) fn guard_message(&self) -> String {
        let fields = self
            .fields()
            .iter()
            .map(|f| format!("`{f}`"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "The rule `{}` reads the column(s) {fields}, but the first parsed record has no such field; check the source header.",
            self.text
        )
    }
    /// The provenance record: observational, never authority.
    pub(super) fn to_json(&self) -> Value {
        let mut record = json!({
            "text": self.text,
            "fields": self.fields(),
            "clauses": self.clauses.iter().map(Clause::to_json).collect::<Vec<_>>(),
            "jq": self.jq(),
            "synthesized": true,
            "summary": self.summary,
            "shape": self.shape.to_json(),
        });
        if let [only] = self.clauses.as_slice() {
            let clause = only.to_json();
            record["field"] = clause["field"].clone();
            record["comparator"] = clause["comparator"].clone();
            record["value"] = clause["value"].clone();
        } else {
            record["junction"] = json!(self.junction.word());
        }
        record
    }
}

// ── parsing ──────────────────────────────────────────────────────────────────────

fn junction_of(token: &Token) -> Option<Junction> {
    match token.word()? {
        "and" | "et" | "y" | "e" | "und" => Some(Junction::And),
        "or" | "ou" | "o" | "oder" => Some(Junction::Or),
        _ => None,
    }
}

/// A trailing count-or-total request ("how many rows were kept and the total of their
/// amounts"): every word is a summary word, a junction or a column of the hint (singular
/// or plural), and at least one asks for a count or a total.
fn summary_residual(tokens: &[Token], from: usize, columns: &[String]) -> bool {
    let rest = tokens.get(from..).unwrap_or_default();
    let mut core = false;
    for token in rest {
        let Some(word) = token.word() else {
            return false;
        };
        if listed(SUMMARY_CORE, word) {
            core = true;
            continue;
        }
        let column = columns.iter().any(|c| {
            let name = normalized(c);
            word == name || word == format!("{name}s") || word == format!("{name}es")
        });
        if !(listed(SUMMARY_WORDS, word) || junction_of(token).is_some() || column) {
            return false;
        }
    }
    core
}

/// Where a clause's comparison sits: the field ends before `field_end`, the comparator
/// (when the anchor is a copula, whatever follows it) starts at `value_from`.
struct Anchor {
    comparator: Option<Comparator>,
    negated: bool,
    symbol: bool,
    field_end: usize,
    value_from: usize,
}

fn anchor_at(tokens: &[Token], at: usize) -> Option<Anchor> {
    let token = tokens.get(at)?;
    if let Kind::Symbol(comparator) = token.kind {
        return Some(Anchor {
            comparator: Some(comparator),
            negated: false,
            symbol: true,
            field_end: at,
            value_from: at + 1,
        });
    }
    if let Some((comparator, width)) = cue_at(tokens, at) {
        return Some(Anchor {
            comparator: Some(comparator),
            negated: false,
            symbol: false,
            field_end: at,
            value_from: at + width,
        });
    }
    let word = token.word()?;
    let negated_copula = NEGATED_COPULAS.contains(&word);
    if !negated_copula && !COPULAS.contains(&word) {
        return None;
    }
    let negation = |i: usize| {
        tokens
            .get(i)
            .and_then(Token::word)
            .is_some_and(|w| NEGATIONS.contains(&w))
    };
    let before = at > 0 && negation(at - 1);
    let after = negation(at + 1);
    Some(Anchor {
        comparator: None,
        negated: negated_copula || before || after,
        symbol: false,
        field_end: if before { at - 1 } else { at },
        value_from: if after { at + 2 } else { at + 1 },
    })
}

/// The comparator a copula anchor states: a cue or symbol after it, else equality.
fn comparator_after(tokens: &[Token], anchor: &Anchor) -> Option<(Comparator, usize)> {
    if let Some(comparator) = anchor.comparator {
        return Some((comparator, anchor.value_from));
    }
    if let Some((comparator, width)) = cue_at(tokens, anchor.value_from) {
        return (!anchor.negated).then_some((comparator, anchor.value_from + width));
    }
    if let Some(Kind::Symbol(comparator)) = tokens.get(anchor.value_from).map(|t| t.kind.clone()) {
        return (!anchor.negated).then_some((comparator, anchor.value_from + 1));
    }
    let comparator = if anchor.negated {
        Comparator::Ne
    } else {
        Comparator::Eq
    };
    Some((comparator, anchor.value_from))
}

/// What the words left of the comparison name.
enum Left {
    Column(String),
    /// Nothing names a column there; the noun after the number may.
    Unnamed,
}

/// The last relative marker in a region: its index and width.
fn last_relative(region: &[Token]) -> Option<(usize, usize)> {
    let mut found = None;
    for at in 0..region.len() {
        for width in [2, 1] {
            if phrase(region, at, width).is_some_and(|p| RELATIVES.contains(&p.as_str())) {
                found = Some((at, width));
                break;
            }
        }
    }
    found
}

/// A negation among the words that lead a clause ("do not keep the tickets whose status
/// is closed", "never keep …", "ne garde pas …") inverts the whole clause; the grammar
/// reads no polarity there, so it reads nothing. The French restriction "ne … que" is
/// "only", never a negation.
fn negated_lead(lead: &[Token]) -> bool {
    let words: Vec<&str> = lead.iter().filter_map(Token::word).collect();
    words.iter().enumerate().any(|(at, word)| {
        if matches!(*word, "ne" | "n") {
            return !words
                .get(at + 1..(at + 4).min(words.len()))
                .is_some_and(|window| window.contains(&"que"));
        }
        NEGATIONS.contains(word)
            || matches!(
                *word,
                "never"
                    | "jamais"
                    | "nunca"
                    | "mai"
                    | "niemals"
                    | "nie"
                    | "don't"
                    | "doesn't"
                    | "won't"
                    | "isn't"
                    | "aren't"
            )
    })
}

/// The field named left of the comparison. `None` when the lead carries a number, a
/// symbol, a quote or a negation the grammar did not consume; `Unnamed` when nothing
/// there names a column.
fn left_field(tokens: &[Token], from: usize, anchor: &Anchor, columns: &[String]) -> Option<Left> {
    let region = tokens.get(from..anchor.field_end).unwrap_or_default();
    let (lead, phrase, relative) = match last_relative(region) {
        Some((at, width)) => (
            region.get(..at).unwrap_or_default(),
            region.get(at + width..).unwrap_or_default(),
            true,
        ),
        None => match region.split_last() {
            Some((last, lead)) => (lead, std::slice::from_ref(last), false),
            None => (region, region, false),
        },
    };
    if lead.iter().any(|t| t.word().is_none()) || negated_lead(lead) {
        return None;
    }
    let mut phrase = phrase;
    while let Some((head, rest)) = phrase.split_first()
        && head.word().is_some_and(|w| ARTICLES.contains(&w))
    {
        phrase = rest;
    }
    if phrase.is_empty() {
        return Some(Left::Unnamed);
    }
    if phrase.len() > 3 || phrase.iter().any(|t| t.word().is_none()) {
        return if relative { None } else { Some(Left::Unnamed) };
    }
    let name = phrase
        .iter()
        .map(|t| t.original.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if !columns.is_empty() {
        return match hinted(&name, columns) {
            Some(column) => Some(Left::Column(column)),
            None if relative => None,
            None => Some(Left::Unnamed),
        };
    }
    let column = relative || anchor.symbol || (phrase.len() == 1 && identifier_shaped(&name));
    Some(if column {
        Left::Column(name)
    } else {
        Left::Unnamed
    })
}

/// The value right of the comparator and the index after it.
fn parse_value(
    tokens: &[Token],
    from: usize,
    comparator: Comparator,
    columns: &[String],
) -> Option<(Operand, usize)> {
    let mut at = from;
    while tokens
        .get(at)
        .and_then(Token::word)
        .is_some_and(|w| FILLERS.contains(&w))
    {
        at += 1;
    }
    let token = tokens.get(at)?;
    let operand = match &token.kind {
        Kind::Number(n) => Operand::Number(n.clone()),
        Kind::Quoted if comparator.numeric() => Operand::Number(number(&token.original)?),
        Kind::Quoted => Operand::Text(token.original.clone()),
        Kind::Word if comparator.numeric() => Operand::Column(column_named(token, columns)?),
        Kind::Word => match hinted(&token.original, columns) {
            Some(column) => Operand::Column(column),
            None => Operand::Text(token.original.clone()),
        },
        Kind::Symbol(_) => return None,
    };
    Some((operand, at + 1))
}

/// A number bounded by a size, attempt or turn unit is prose or a loop bound, not data.
fn unit_after(tokens: &[Token], at: usize) -> bool {
    let unit = |i: usize| {
        tokens
            .get(i)
            .and_then(Token::word)
            .is_some_and(|w| SIZE_UNITS.contains(&w) || ATTEMPT_UNITS.contains(&w))
    };
    let bridge = tokens
        .get(at)
        .and_then(Token::word)
        .is_some_and(|w| matches!(w, "a" | "de" | "di" | "of"));
    unit(at) || (bridge && unit(at + 1))
}

/// After the value: unit words, a verb-final copula, then the end or a junction.
fn residual(tokens: &[Token], from: usize) -> Option<usize> {
    let mut at = from;
    loop {
        let Some(token) = tokens.get(at) else {
            return Some(at);
        };
        if junction_of(token).is_some() {
            return Some(at);
        }
        if phrase(tokens, at, 2).is_some_and(|p| UNIT_PHRASES.contains(&p.as_str())) {
            at += 2;
            continue;
        }
        let trailing = token
            .word()
            .is_some_and(|w| UNIT_WORDS.contains(&w) || COPULAS.contains(&w));
        if !trailing {
            return None;
        }
        at += 1;
    }
}

/// One clause from `from`: the clause and the index of the token after it.
fn parse_clause(tokens: &[Token], from: usize, columns: &[String]) -> Option<(Clause, usize)> {
    let anchor = (from..tokens.len()).find_map(|at| anchor_at(tokens, at))?;
    let (comparator, value_from) = comparator_after(tokens, &anchor)?;
    let left = left_field(tokens, from, &anchor, columns)?;
    let (value, mut next) = parse_value(tokens, value_from, comparator, columns)?;
    let numeric_value = matches!(value, Operand::Number(_));
    if numeric_value && unit_after(tokens, next) {
        return None;
    }
    let field = match left {
        Left::Column(field) => field,
        Left::Unnamed => {
            if !numeric_value {
                return None;
            }
            let field = column_named(tokens.get(next)?, columns)?;
            next += 1;
            field
        }
    };
    if let Operand::Column(other) = &value
        && *other == field
    {
        return None;
    }
    let next = residual(tokens, next)?;
    Some((
        Clause {
            field,
            comparator,
            value,
        },
        next,
    ))
}

/// Sentences and `;`-joined rules, each of which must parse whole.
fn segments(text: &str) -> impl Iterator<Item = &str> {
    text.split(';')
        .flat_map(|part| part.split(". "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The rule the text states, or `None` when any part is outside the grammar.
pub(super) fn synthesize(text: &str, columns: &[String]) -> Option<Rule> {
    let text = text.trim();
    let mut clauses = Vec::new();
    let mut junction: Option<Junction> = None;
    let mut summary = false;
    let mut shape = Shape::default();
    for segment in segments(text) {
        let tokens = tokenize(segment);
        if tokens.is_empty() {
            continue;
        }
        if !clauses.is_empty() {
            if junction == Some(Junction::Or) {
                return None;
            }
            junction = Some(Junction::And);
        }
        let mut at = 0;
        loop {
            let Some((clause, next)) = parse_clause(&tokens, at, columns) else {
                // A whole segment stating one aggregate over a column is the shape's work:
                // the filter (if any) runs first, the totals follow.
                if at == 0
                    && let Some(aggregation) = super::aggregate::stated(segment, columns)
                {
                    if !shape.aggregations.contains(&aggregation) {
                        shape.aggregations.push(aggregation);
                    }
                    break;
                }
                // After a clause, a count-or-total request is the summary stage's work.
                let trailing = (at > 0 || !clauses.is_empty()) && junction != Some(Junction::Or);
                if trailing && summary_residual(&tokens, at, columns) {
                    summary = true;
                    break;
                }
                return None;
            };
            // The same clause twice (a promoted constraint beside the seat's paraphrase
            // of it) is one clause.
            if !clauses.contains(&clause) {
                clauses.push(clause);
            }
            let Some(token) = tokens.get(next) else {
                break;
            };
            let joined = junction_of(token)?;
            if junction.is_some_and(|j| j != joined) {
                return None;
            }
            junction = Some(joined);
            at = next + 1;
            if at >= tokens.len() {
                return None;
            }
        }
    }
    if clauses.is_empty() && shape.aggregations.is_empty() {
        return None;
    }
    Some(Rule {
        text: text.to_owned(),
        clauses,
        junction: junction.unwrap_or(Junction::And),
        summary,
        shape,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn jq(text: &str) -> Option<String> {
        synthesize(text, &[]).map(|r| r.jq())
    }

    fn jq_with(text: &str, columns: &[&str]) -> Option<String> {
        synthesize(text, &cols(columns)).map(|r| r.jq())
    }

    #[test]
    fn a_trailing_count_or_total_request_is_the_summary_stage() {
        let hint = cols(&["order_id", "customer", "amount", "status"]);
        let folded = "keep only the rows whose amount is strictly greater than 100 and how many rows were kept and the total of their amounts";
        let rule = synthesize(folded, &hint).expect("a rule");
        assert!(rule.summary());
        assert_eq!(
            rule.jq(),
            "[.records[] | select((.amount | tonumber) > 100)]"
        );
        assert_eq!(rule.to_json()["summary"], true);
        let french = "garde les lignes dont le montant est plus grand que 100 et le nombre de lignes gardées et le total de leurs montants";
        assert!(synthesize(french, &[]).expect("a rule").summary());
        let sentence = "amount > 100. Count how many rows were kept";
        assert!(synthesize(sentence, &[]).expect("a rule").summary());
        // The live seat's paraphrase beside the promoted constraint: one clause, summary.
        let paraphrase = "filter rows with amount strictly greater than 100 and compute count plus total amount ; keep only the rows whose amount is strictly greater than 100";
        let rule = synthesize(paraphrase, &hint).expect("a rule");
        assert!(rule.summary());
        assert_eq!(
            rule.jq(),
            "[.records[] | select((.amount | tonumber) > 100)]"
        );
        assert_eq!(rule.to_json()["clauses"].as_array().map(Vec::len), Some(1));
        assert!(!synthesize("amount > 100", &[]).expect("a rule").summary());
        for none in [
            "how many rows were kept and the total of their amounts",
            "amount > 100 or how many rows were kept",
            "amount > 100 and how many rows were kept and whose status is open",
            "amount > 100 and the total per country",
            "keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON",
        ] {
            assert_eq!(synthesize(none, &hint), None, "{none}");
        }
    }

    #[test]
    fn a_numeric_rule_in_six_languages_becomes_one_select() {
        let gt = Some("[.records[] | select((.amount | tonumber) > 100)]".to_owned());
        assert_eq!(
            jq("keep only the rows whose amount is strictly greater than 100"),
            gt
        );
        assert_eq!(jq("whose amount is above 100"), gt);
        assert_eq!(jq("amount > 100"), gt);
        assert_eq!(jq("rows where amount > 100"), gt);
        assert_eq!(jq("amount>100"), gt);
        assert_eq!(
            jq("dont le montant est plus grand que 200"),
            Some("[.records[] | select((.montant | tonumber) > 200)]".to_owned())
        );
        assert_eq!(
            jq("les lignes dont le montant est strictement supérieur à 200 €"),
            Some("[.records[] | select((.montant | tonumber) > 200)]".to_owned())
        );
        assert_eq!(
            jq("cuya cantidad es menor que 10"),
            Some("[.records[] | select((.cantidad | tonumber) < 10)]".to_owned())
        );
        assert_eq!(
            jq("las filas cuyo total es mayor o igual a 50"),
            Some("[.records[] | select((.total | tonumber) >= 50)]".to_owned())
        );
        assert_eq!(
            jq("le righe la cui quantità è minore di 5"),
            Some("[.records[] | select((.[\"quantità\"] | tonumber) < 5)]".to_owned())
        );
        assert_eq!(
            jq("as linhas cuja quantidade é menor que 10"),
            Some("[.records[] | select((.quantidade | tonumber) < 10)]".to_owned())
        );
        assert_eq!(
            jq("die Zeilen, deren Betrag größer als 100 ist"),
            Some("[.records[] | select((.Betrag | tonumber) > 100)]".to_owned())
        );
        assert_eq!(
            jq("whose total_eur is at least 120 EUR"),
            Some("[.records[] | select((.total_eur | tonumber) >= 120)]".to_owned())
        );
        assert_eq!(
            jq("whose total_eur is at most 12.5"),
            Some("[.records[] | select((.total_eur | tonumber) <= 12.5)]".to_owned())
        );
        assert_eq!(
            jq("montant ≥ 1,5"),
            Some("[.records[] | select((.montant | tonumber) >= 1.5)]".to_owned())
        );
        assert_eq!(
            jq("total_eur >= 1,000"),
            Some("[.records[] | select((.total_eur | tonumber) >= 1000)]".to_owned())
        );
        assert_eq!(
            jq("whose qty is 0"),
            Some("[.records[] | select((.qty | tonumber) == 0)]".to_owned())
        );
        assert_eq!(
            jq("whose qty is not 0"),
            Some("[.records[] | select((.qty | tonumber) != 0)]".to_owned())
        );
    }

    #[test]
    fn a_columns_hint_names_the_field_in_its_own_spelling() {
        let hint = ["order_id", "customer", "Amount", "status", "unit price"];
        assert_eq!(
            jq_with("keep the rows with amount above 100", &hint),
            Some("[.records[] | select((.Amount | tonumber) > 100)]".to_owned())
        );
        assert_eq!(
            jq_with("whose unit price is below 3", &hint),
            Some("[.records[] | select((.[\"unit price\"] | tonumber) < 3)]".to_owned())
        );
        assert_eq!(
            jq_with("whose unit_price is below 3", &hint),
            Some("[.records[] | select((.[\"unit price\"] | tonumber) < 3)]".to_owned())
        );
        assert_eq!(
            jq_with("products with fewer than 10 units", &["sku", "units"]),
            Some("[.records[] | select((.units | tonumber) < 10)]".to_owned())
        );
        // A word outside the hint is not a column: the human is asked.
        assert_eq!(jq_with("whose total is above 100", &hint), None);
        assert_eq!(jq_with("whose amount_eur is above 100", &hint), None);
    }

    #[test]
    fn an_equality_rule_keeps_the_exact_case_of_its_value() {
        assert_eq!(
            jq("whose status is refunded"),
            Some("[.records[] | select(.status == \"refunded\")]".to_owned())
        );
        assert_eq!(
            jq("whose status is \"Refunded\""),
            Some("[.records[] | select(.status == \"Refunded\")]".to_owned())
        );
        assert_eq!(
            jq("dont le statut est « expédié »"),
            Some("[.records[] | select(.statut == \"expédié\")]".to_owned())
        );
        assert_eq!(
            jq("cuyo estado no es reembolsado"),
            Some("[.records[] | select(.estado != \"reembolsado\")]".to_owned())
        );
        assert_eq!(
            jq("whose status is not 'shipped'"),
            Some("[.records[] | select(.status != \"shipped\")]".to_owned())
        );
        assert_eq!(
            jq("whose status equals shipped"),
            Some("[.records[] | select(.status == \"shipped\")]".to_owned())
        );
        assert_eq!(
            jq("status = shipped"),
            Some("[.records[] | select(.status == \"shipped\")]".to_owned())
        );
        assert_eq!(
            jq("whose country is different from FR"),
            Some("[.records[] | select(.country != \"FR\")]".to_owned())
        );
        assert_eq!(
            jq("deren Status ungleich offen"),
            Some("[.records[] | select(.Status != \"offen\")]".to_owned())
        );
    }

    #[test]
    fn two_columns_compare_when_both_name_columns() {
        assert_eq!(
            jq("la cui quantita e inferiore alla soglia_minima"),
            Some(
                "[.records[] | select((.quantita | tonumber) < (.soglia_minima | tonumber))]"
                    .to_owned()
            )
        );
        assert_eq!(
            jq_with(
                "cuja quantidade é menor que minimo",
                &["sku", "nome", "quantidade", "minimo"]
            ),
            Some(
                "[.records[] | select((.quantidade | tonumber) < (.minimo | tonumber))]".to_owned()
            )
        );
        assert_eq!(
            jq("whose stock_qty is below reorder_level"),
            Some(
                "[.records[] | select((.stock_qty | tonumber) < (.reorder_level | tonumber))]"
                    .to_owned()
            )
        );
        // A bare word right of a numeric comparison is not a column.
        assert_eq!(jq("whose quantity is below minimum"), None);
        assert_eq!(jq("whose stock_qty is below stock_qty"), None);
    }

    #[test]
    fn clauses_join_through_one_conjunction() {
        assert_eq!(
            jq("keep only the rows whose status is \"shipped\" and whose total_eur is above 120"),
            Some(
                "[.records[] | select(.status == \"shipped\" and (.total_eur | tonumber) > 120)]"
                    .to_owned()
            )
        );
        assert_eq!(
            jq("amount > 100 or amount < 10"),
            Some(
                "[.records[] | select((.amount | tonumber) > 100 or (.amount | tonumber) < 10)]"
                    .to_owned()
            )
        );
        assert_eq!(
            jq("dont le montant est plus grand que 100 et dont le statut est ouvert"),
            Some(
                "[.records[] | select((.montant | tonumber) > 100 and .statut == \"ouvert\")]"
                    .to_owned()
            )
        );
        assert_eq!(
            jq("cuya cantidad es menor que 10 y cuyo estado es activo"),
            Some(
                "[.records[] | select((.cantidad | tonumber) < 10 and .estado == \"activo\")]"
                    .to_owned()
            )
        );
        // Two promoted rules joined by the plan's separator.
        assert_eq!(
            jq("amount > 100 ; whose status is open"),
            Some(
                "[.records[] | select((.amount | tonumber) > 100 and .status == \"open\")]"
                    .to_owned()
            )
        );
        // Mixed junctions have no fixed precedence in prose: the human is asked.
        assert_eq!(jq("amount > 100 and amount < 10 or qty > 3"), None);
        assert_eq!(jq("amount > 100 or qty > 3 ; status = open"), None);
        assert_eq!(jq("amount > 100 and"), None);
    }

    #[test]
    fn a_stated_aggregate_is_the_shape_after_the_filter() {
        assert_eq!(
            jq("the total of the amount column"),
            Some(".records | {\"total\": (map(.amount | tonumber) | add // 0)}".to_owned())
        );
        assert_eq!(
            jq("the total of the amount column ; whose client is acme"),
            Some(
                "[.records[] | select(.client == \"acme\")] | {\"total\": (map(.amount | tonumber) | add // 0)}"
                    .to_owned()
            )
        );
        let rule = synthesize("la moyenne de la colonne montant", &[]).expect("a rule");
        assert_eq!(rule.totals_names(), ["moyenne"]);
        assert_eq!(rule.fields(), ["montant"]);
        assert!(!rule.summary());
        assert_eq!(jq("the total of the amount column per client"), None);
    }

    #[test]
    fn a_negation_among_the_lead_words_is_read_as_nothing_never_inverted() {
        for text in [
            "do not keep the tickets whose status is closed",
            "never keep the tickets whose status is closed",
            "Read ./tickets.json, do not keep the tickets whose status is closed",
            "ne garde pas les lignes dont amount dépasse 200",
            "ne garde jamais les lignes dont amount dépasse 200",
            "don't keep rows whose amount is above 100",
        ] {
            assert_eq!(synthesize(text, &[]), None, "{text}");
        }
        // The French restriction is "only": a filter, read as stated.
        assert_eq!(
            jq("ne garde que les lignes dont amount dépasse 200"),
            Some("[.records[] | select((.amount | tonumber) > 200)]".to_owned())
        );
        // A negation after the copula is the clause's own polarity, still read.
        assert_eq!(
            jq("whose status is not closed"),
            Some("[.records[] | select(.status != \"closed\")]".to_owned())
        );
    }

    #[test]
    fn what_the_grammar_does_not_cover_is_none() {
        for text in [
            "a brief of under 150 words",
            "au plus 12 lignes",
            "at most 3 attempts",
            "Process at most 2 products at a time",
            "products with fewer than 10 units",
            "quantité inférieure à 10",
            "au moins 3 articles en stock",
            "the top 3 countries",
            "in a warm tone",
            "keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON",
            "whose amount is above 100 per country",
            "whose status is refunded today",
            "the 3 rows whose amount > 100",
            "whose status is shipped or refunded",
            "whose amount is between 10 and 20",
            "whose amount is not above 100",
            "whose amount > \"high\"",
            "",
        ] {
            assert_eq!(synthesize(text, &[]), None, "{text}");
        }
    }

    #[test]
    fn the_guard_names_every_column_and_the_record_is_observational() {
        let rule = synthesize(
            "whose status is \"shipped\" and whose total_eur is above stock_min",
            &[],
        )
        .expect("a rule");
        assert_eq!(rule.fields(), ["status", "total_eur", "stock_min"]);
        assert_eq!(
            rule.guard(),
            "(.records | type) == \"array\" and ((.records | length) == 0 or (.records[0] | type == \"object\" and has(\"status\") and has(\"total_eur\") and has(\"stock_min\")))"
        );
        assert!(
            rule.guard_message()
                .contains("`status`, `total_eur`, `stock_min`")
        );
        let record = rule.to_json();
        assert_eq!(record["synthesized"], true);
        assert_eq!(record["junction"], "and");
        assert_eq!(record["clauses"][1]["value_kind"], "column");
        assert!(record.get("field").is_none());
        let one = synthesize("whose amount is above 100", &[]).expect("a rule");
        let record = one.to_json();
        assert_eq!(record["field"], "amount");
        assert_eq!(record["comparator"], ">");
        assert_eq!(record["value"], "100");
        assert_eq!(record["text"], "whose amount is above 100");
        assert_eq!(key("unit price"), ".[\"unit price\"]");
        assert_eq!(key("_a1"), "._a1");
    }

    #[test]
    fn numbers_fold_currency_and_separators() {
        assert_eq!(number("100").as_deref(), Some("100"));
        assert_eq!(number("€100").as_deref(), Some("100"));
        assert_eq!(number("100€").as_deref(), Some("100"));
        assert_eq!(number("15%").as_deref(), Some("15"));
        assert_eq!(number("-3").as_deref(), Some("-3"));
        assert_eq!(number("1,5").as_deref(), Some("1.5"));
        assert_eq!(number("1,000").as_deref(), Some("1000"));
        assert_eq!(number("1,000,000").as_deref(), Some("1000000"));
        assert_eq!(number("12.50").as_deref(), Some("12.50"));
        for not in ["abc", "1.2.3", "1,2,3", "T-4471", "", "1.000,50"] {
            assert_eq!(number(not), None, "{not}");
        }
    }
}
