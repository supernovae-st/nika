// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The cue and marker tables of the deterministic reader, across its languages
//! (EN · FR · IT · ES): what settles an ambiguous retrieval, what a clause may be
//! led by, which words are articles, how a policy sentence announces itself.
//! Every table is closed; a cue absent here is a cue the reader does not know.

/// Cues that settle an ambiguous retrieval head deterministically.
pub(super) const LOOKUP_CUES: &[&str] = &[
    "mongodb",
    "base ",
    "database",
    "annuaire",
    "directory",
    "registre",
    "registry",
    "catalogue",
    "catalog",
    "historique",
    "history",
    "calendrier",
    "calendar",
    "crm",
    "shopify",
    "runbook",
    "checklist",
    "knowledge base",
    "disponibilit",
    "availabilit",
    "agenda",
    "base de connaissances",
    "record",
    "customer",
    "client",
    "entreprise",
    "compte",
];
pub(super) const SEARCH_CUES: &[&str] = &[
    "pdf",
    "dossier",
    "fichiers",
    "files",
    "folder",
    "pages",
    "documents",
    "guide",
    "passages",
    "corpus",
];
pub(super) const READ_CUES: &[&str] = &[
    "fourni",
    "fournie",
    "fournis",
    "fournies",
    "supplied",
    "provided",
    "attached",
    "ci-joint",
    "formulaire",
    "form ",
    "transcript",
];
pub(super) const LEADING_FILLER: &[&str] = &[
    "ensuite ",
    "puis ",
    "then ",
    "also ",
    "aussi ",
    "please ",
    "veuillez ",
    "s'il te plaît ",
    "s'il vous plaît ",
    "automatically ",
    "automatiquement ",
    "seulement ",
    "only ",
    "always ",
    "toujours ",
];
pub(super) const ARTICLES: &[&str] = &[
    "le", "la", "les", "l", "l'", "d", "qu", "n", "s", "c", "j", "un", "une", "des", "du", "de",
    "d'", "the", "a", "an", "my", "mon", "ma", "mes", "notre", "nos", "our", "son", "sa", "ses",
    "its", "their", "leur", "leurs", "ce", "cet", "cette", "ces", "chaque", "each", "every",
    "tout", "toute", "tous", "toutes", "any", "all", "en", "ensuite",
];
pub(super) const TRIGGER_PREFIXES: &[&str] = &[
    "pour la ",
    "pour le ",
    "pour les ",
    "pour chaque ",
    "for the ",
    "for every ",
    "quand ",
    "lorsque ",
    "dès que ",
    "tous les ",
    "toutes les ",
    "chaque fois ",
    "après ",
    "for each ",
    "when ",
    "whenever ",
    "every ",
    "after ",
    "once ",
    "à partir de ",
    "from the ",
    "starting from ",
];

pub(super) const NUMBER_WORDS: &[(&str, u32)] = &[
    ("un", 1),
    ("une", 1),
    ("one", 1),
    ("deux", 2),
    ("two", 2),
    ("trois", 3),
    ("three", 3),
    ("quatre", 4),
    ("four", 4),
    ("cinq", 5),
    ("five", 5),
    ("six", 6),
    ("sept", 7),
    ("seven", 7),
    ("huit", 8),
    ("eight", 8),
    ("neuf", 9),
    ("nine", 9),
    ("dix", 10),
    ("ten", 10),
];
pub(super) const ATTEMPT_NOUNS: &[&str] = &[
    "essai",
    "tentative",
    "itération",
    "iteration",
    "cycle",
    "attempt",
    "retr",
    "tries",
    "try",
    "round",
];
pub(super) const BOUND_WORDS: &[&str] = &[
    "limite ",
    "limit ",
    "au maximum",
    "maximum",
    "at most",
    "up to",
    "no more than",
    "at max",
    "max ",
];
pub(super) const UNDECIDED_MARKERS: &[&str] = &[
    "je n'ai pas encore décidé si le workflow doit ",
    "je n'ai pas encore décidé si le workflow devait ",
    "je n'ai pas encore décidé si ",
    "i have not decided whether the workflow should ",
    "i have not decided whether to ",
    "i have not decided whether ",
    "i haven't decided whether to ",
    "i haven't decided whether ",
];
pub(super) const REVISION_MARKERS: &[&str] = &[
    "vérifie de nouveau la version",
    "vérifie à nouveau la version",
    "re-check the current",
    "recheck the current",
    "check the current version again",
    "re-verify the current",
];
pub(super) const FINAL_GATE_MARKERS: &[&str] = &[
    "mais cette action finale exige la validation humaine",
    "cette action finale exige la validation humaine",
    "this final action requires human validation",
    "this final action requires human approval",
    "only after my approval",
    "only after i approve",
    "only once i approve",
    "once i approve",
    "after my approval",
    "seulement après mon accord",
    "après mon accord",
    "après ma validation",
    "après validation humaine",
    "but ask me before",
    "but get my approval before",
    "get my approval before",
    "with my approval before",
];
pub(super) const NAMED_GATE_MARKERS: &[&str] = &[
    "demande mon accord avant ",
    "demandez mon accord avant ",
    "demander mon accord avant ",
    "demande un accord humain avant ",
    "demande ma validation avant ",
    "require my approval before ",
    "ask me before ",
    "ask for my approval before ",
    "obtain my approval before ",
    "hold every ",
    "attends ma validation avant ",
    "wait for my approval before ",
];
pub(super) const FORBIDDEN_MARKERS: &[&str] = &[
    "il est aussi absolument interdit de ",
    "il est aussi absolument interdit d'",
    "il est absolument interdit de ",
    "il est absolument interdit d'",
    "il est aussi interdit de ",
    "il est aussi interdit d'",
    "il est interdit de ",
    "il est interdit d'",
    "it is absolutely forbidden to ",
    "it is also absolutely forbidden to ",
    "it is forbidden to ",
    "never ",
    "do not ",
    "don't ",
    "nothing should be ",
    "ne jamais ",
];
pub(super) const STOP_MARKERS: &[&str] = &[
    "arrête-toi après",
    "aucune autre action n'est demandée",
    "no other step",
    "nothing else",
    "no further action",
];

/// Connectors that open a new clause whatever follows (a sequencing word must never
/// swallow an unknown verb as the previous object).
pub(super) const STRONG_CONNECTORS: &[&str] = &[
    ", puis ", " puis ", ", then ", " then ", ", mais ", " mais ", ", but ", " but ",
];

/// Connectors that open a new clause only before a known head.
pub(super) const WEAK_CONNECTORS: &[&str] = &[", et ", " et ", ", and ", " and ", ", "];

/// One filler word after the first word of a clause (`passe ensuite la commande`).
pub(super) const SECOND_WORD_FILLERS: &[&str] = &[
    "ensuite",
    "alors",
    "then",
    "also",
    "aussi",
    "puis",
    "immédiatement",
    "immediately",
];

/// Coordinating connectors inside an object: a coordinated object is never explicit.
pub(super) const OBJECT_CONNECTORS: &[&str] = &[
    ", ",
    " and ",
    " et ",
    " or ",
    " ou ",
    " puis ",
    " then ",
    ";",
    " sans ",
    " without ",
    " mais ",
    " but ",
];

/// Category markers of a classify object (`classe en A, B ou C`).
pub(super) const CATEGORY_MARKERS: &[&str] = &[" en ", " into ", " as "];

/// Clause openers that state a negation (a prohibition or a restriction).
pub(super) const NEGATION_OPENERS: &[&str] =
    &["ne ", "n'", "do not ", "don't ", "never ", "no ", "aucun"];

/// Clause openers that state a condition or a preservation, kept as a constraint.
pub(super) const CONSTRAINT_OPENERS: &[&str] = &[
    "si ",
    "if ",
    "lorsque ",
    "unless ",
    "laisse ",
    "laissez ",
    "leave ",
    "keep ",
    "conserve ",
    "garde ",
    "ignore ",
];
