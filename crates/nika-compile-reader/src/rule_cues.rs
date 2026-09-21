// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The cue tables of the closed rule grammar: comparison and equality cues in six
//! languages, copulas and negations, relatives, articles, fillers, units, and the words of
//! a trailing count-or-total request. Data only; the grammar lives in `rules`.

use super::rules::Comparator;

/// Comparison cues (EN · FR · ES · IT · PT · DE, diacritics folded, lowercase), one
/// `|`-separated list per comparator. Matched as whole phrases, longest first, up to
/// five words.
pub(crate) const NUMERIC_CUES: &[(Comparator, &str)] = &[
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
pub(crate) const EQUALITY_CUES: &[(Comparator, &str)] = &[
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
pub(crate) const CUE_WIDTH: usize = 5;

/// A copula: the field is the phrase before it, the comparison (or the equality value)
/// follows it.
pub(crate) const COPULAS: &[&str] = &[
    "is", "are", "was", "were", "be", "being", "has", "have", "est", "sont", "n'est", "es", "son",
    "esta", "estan", "e", "sao", "ist", "sind", "ha", "hanno", "tiene", "tienen", "tem", "hat",
    "haben",
];

/// A copula that carries its own negation.
pub(crate) const NEGATED_COPULAS: &[&str] =
    &["isn't", "aren't", "wasn't", "weren't", "n'est", "n'a"];

/// A negation right before or right after a copula.
pub(crate) const NEGATIONS: &[&str] = &["not", "pas", "no", "non", "nao", "ne", "nicht"];

/// A relative pronoun or preposition that opens the noun phrase naming the field
/// ("whose amount", "dont le montant", "cuya cantidad", "la cui quantita", "deren Betrag").
pub(crate) const RELATIVES: &[&str] = &[
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
pub(crate) const ARTICLES: &[&str] = &[
    "the", "a", "an", "its", "their", "le", "la", "les", "l'", "l", "un", "une", "des", "du", "de",
    "sa", "son", "ses", "leur", "leurs", "el", "los", "las", "una", "unos", "unas", "su", "sus",
    "il", "lo", "i", "gli", "uno", "suo", "sua", "suoi", "sue", "o", "os", "as", "um", "uma",
    "seu", "seus", "suas", "der", "die", "das", "den", "dem", "ein", "eine", "einer", "einem",
    "einen", "sein", "seine", "seiner", "ihr", "ihre", "ihrer", "ihren",
];

/// Words skipped between a comparator and its value.
pub(crate) const FILLERS: &[&str] = &[
    "than", "que", "als", "di", "de", "da", "a", "of", "to", "the", "le", "la", "les", "el", "los",
    "las", "il", "lo", "der", "die", "das", "den", "dem", "del", "della", "dello", "dei", "degli",
    "delle", "du", "des", "do", "dos", "ao", "au", "aux", "al", "alla", "allo", "ai", "agli",
    "alle", "zu", "zum", "zur",
];

/// A unit or currency word that may trail a numeric value without changing the rule.
pub(crate) const UNIT_WORDS: &[&str] = &[
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
pub(crate) const UNIT_PHRASES: &[&str] = &[
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
pub(crate) const SUMMARY_WORDS: &str = "how|many|much|rows|row|records|record|lines|line|entries|entry|\
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
pub(crate) const SUMMARY_CORE: &str = "many|count|counted|number|total|totals|sum|combien|nombre|somme|\
    cuantas|cuantos|numero|suma|quante|quanti|totale|somma|quantas|quantos|soma|viele|\
    anzahl|summe|gesamtsumme|gesamtbetrag";
