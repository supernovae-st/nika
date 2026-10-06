//! Statements of a request that bind no operation. A context sentence describes the
//! material (« Le fichier ./x.csv contient les colonnes a,b,c », « which has the columns … »,
//! « both requirements are checked on the produced file »); a structure law bounds the shape
//! of the workflow (« nothing else », « no other file », « no language model », « a single
//! HTTP request, not one per fine »). Neither needs an operation to carry it: context is
//! realized by the material it describes, a structure law by the shape of the emitted
//! workflow, and a law the shape breaks stays unresolved so that nothing is READY against it.
//! Moved from nika-compile to the reader at the 15k prod-LOC wall (2026-09-22), unchanged.

use super::rule_tokens::fold;

/// A bound on the shape of the workflow, read from its form in six languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Law {
    /// No step or action beyond the stated ones.
    NothingElse,
    /// No destination beyond the stated ones.
    NoOtherFile,
    /// No language model anywhere in the workflow.
    NoModel,
    /// One outbound request in the whole run, never one per item.
    SingleRequest,
}

/// Closures: the request ends here, nothing beyond the stated steps; « stop after these
/// steps » and « stop there » are the same closure, stated as a halt.
const NOTHING_ELSE: &str = include_str!("../assets/law_nothing_else.txt");

/// No destination beyond the stated ones.
const NO_OTHER_FILE: &str = include_str!("../assets/law_no_other_file.txt");

/// No language model in the workflow.
const NO_MODEL: &str = include_str!("../assets/law_no_model.txt");

/// One outbound request for the whole run.
const SINGLE_REQUEST: &str = include_str!("../assets/law_single_request.txt");

/// Nouns of the material a context sentence describes.
const MATERIAL: &str = include_str!("../assets/material_nouns.txt");

/// Verbs and joints of a description, never of a demand.
const DECLARATIVE: &str = include_str!("../assets/declarative_words.txt");

/// The folded text with every run of punctuation collapsed to one space and a space at each
/// end, so a table phrase matches on word boundaries. Apostrophes and hyphens are kept: they
/// are part of « nient'altro », « c'est tout », « en-tete ».
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
    if !space {
        out.push(' ');
    }
    out
}

/// The same hit over an embedded table, one word or phrase per line.
fn hit_lines(padded: &str, table: &str) -> bool {
    table.lines().any(|m| padded.contains(&format!(" {m} ")))
}

/// Every structure law a clause states, strictest first. A closure beside a measurable bound
/// (« nada más que 3 líneas ») is the bound's phrasing, not a closure.
#[must_use]
pub fn laws(text: &str) -> Vec<Law> {
    let padded = padded(text);
    let mut out = Vec::new();
    if hit_lines(&padded, NO_MODEL) {
        out.push(Law::NoModel);
    }
    if hit_lines(&padded, SINGLE_REQUEST) {
        out.push(Law::SingleRequest);
    }
    if hit_lines(&padded, NO_OTHER_FILE) {
        out.push(Law::NoOtherFile);
    }
    if hit_lines(&padded, NOTHING_ELSE) && super::cardinality::bound(text).is_none() {
        out.push(Law::NothingElse);
    }
    out
}

/// A sentence that describes the material rather than demanding work: a declarative joint
/// beside a noun of the material, with no effect word, no measurable bound, no prohibition
/// and no gate in it, and not a selection of the material's rows. « The file has the columns
/// a,b,c », « both requirements are checked on the produced file », « Le fichier ./x.csv
/// contient les colonnes … ».
#[must_use]
pub fn context_statement(text: &str) -> bool {
    let padded = padded(text);
    if !hit_lines(&padded, MATERIAL) || !hit_lines(&padded, DECLARATIVE) || selection_demand(text) {
        return false;
    }
    let lower = text.to_lowercase();
    let columns = super::columns::columns_hint(text);
    super::lexicon::effect_words(&lower, &columns).is_empty()
        && super::cardinality::bound(text).is_none()
        && !super::gates::starts_with_prohibition(&lower)
        && super::gates::final_gate(&lower).is_none()
        && super::gates::named_gate(&lower).is_none()
}

/// A clause that selects rows of the material (R4 A10): led by a keep or an exclusion lead of
/// the rule grammar (« keep the rows whose … », « garde les lignes dont … », « ignore the rows
/// where … ») over a noun of the material. It demands an operation even where the grammar
/// cannot read its predicate (« … whose status is a »): named work, never a description the
/// material realizes nor a prompt's guidance.
#[must_use]
pub fn selection_demand(text: &str) -> bool {
    let padded = padded(text);
    let lead = padded.split_whitespace().next().unwrap_or_default();
    (super::stages::keep_lead(lead) || super::rules::exclusion_lead(lead))
        && hit_lines(&padded, MATERIAL)
}

/// Whether a clause restricts the material or conditions an operation (R4 A11): a keep or an
/// exclusion lead, a negation (its pronouns and determiners too), « only », an exception, a
/// condition, or a structure law (« nothing else », « no other file », a single request). It
/// never asks for nothing. A path or a URL is a literal, never a word of the clause (the `out`
/// of `./out/result.json` is no exclusion).
#[must_use]
pub fn restricts(text: &str) -> bool {
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|token| !token.contains(['/', '\\']))
        .collect();
    !laws(text).is_empty()
        || super::rule_tokens::fold(&words.join(" "))
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| {
                super::stages::restriction_word(w)
                    || super::stages::keep_lead(w)
                    || super::rules::exclusion_lead(w)
            })
}

/// Whether a text's words are function words around one computation head the reader reads
/// (« then compute », « calcule »): the head of a clause whose object the grammar read (R4 A11).
#[must_use]
pub fn only_a_compute_head(text: &str) -> bool {
    let content: Vec<&str> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|word| !word.is_empty() && !super::paths::function_word(word))
        .collect();
    !content.is_empty() && super::lexicon::compute_head(&content.join(" "))
}

/// Whether every word of a text is a function word of the reader's closed table (an article,
/// a preposition, a connective such as « then », « et », « und »): words that state no operation
/// of their own. An empty text has none.
#[must_use]
pub fn only_function_words(text: &str) -> bool {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|word| !word.is_empty())
        .all(super::paths::function_word)
}

/// Whether a constraint needs no operation to carry it: a context statement or a structure
/// law. The composer's carrier rule and the deterministic door's admission skip it.
#[must_use]
pub fn binds_no_operation(text: &str) -> bool {
    context_statement(text) || !laws(text).is_empty()
}

/// The negations of the restriction table: the words a prohibition is stated with, in the six
/// languages the table reads (« mai », Italian « never », is left out: it is also a month;
/// Portuguese « no » is « in the », never a negation, in a Portuguese clause).
const NEGATIONS: &[&str] = &[
    "not", "no", "never", "without", "nothing", "none", "nobody", "neither", "nor", "pas",
    "jamais", "sans", "rien", "aucun", "aucune", "ni", "nunca", "sin", "nada", "ninguno",
    "ninguna", "ningun", "nadie", "non", "senza", "niente", "nulla", "nessuno", "nessuna", "nicht",
    "nie", "ohne", "nichts", "kein", "keine", "keinen", "keinem", "keiner", "keines", "weder",
    "nao", "sem", "nenhum", "nenhuma", "ninguem", "nem",
];

/// The negations that stand where an object does (« send no email », « delete nothing », « make
/// sure nobody … », « lösche nichts »): they forbid the verb just before them.
const NEGATING_OBJECTS: &[&str] = &[
    "no", "nothing", "none", "nobody", "rien", "aucun", "aucune", "nada", "ninguno", "ninguna",
    "ningun", "nadie", "niente", "nulla", "nessuno", "nessuna", "nichts", "kein", "keine",
    "keinen", "keinem", "keiner", "keines", "nenhum", "nenhuma", "ninguem",
];

/// The German negations that close the phrase they forbid (« lösche die Datei nicht »).
const CLOSING_NEGATIONS: &[&str] = &["nicht", "nie", "niemals"];

/// The French particle that opens a negation (« ne … pas », « n'… jamais »), the words that
/// close it, and the subjects that negate it from before (« personne ne … », « rien ne … »);
/// « ne … que » restricts to what follows instead.
const NE: &[&str] = &["ne", "n"];
const NE_CLOSERS: &[&str] = &[
    "pas", "jamais", "rien", "aucun", "aucune", "plus", "personne", "guere",
];
const NE_SUBJECTS: &[&str] = &["personne", "rien", "aucun", "aucune", "nul", "nulle"];

/// Words only a Portuguese clause holds: in one, « no » reads « in the ».
const PORTUGUESE: &[&str] = &[
    "nao",
    "em",
    "com",
    "uma",
    "arquivo",
    "ficheiro",
    "escreva",
    "salve",
    "grave",
    "grava",
    "exporte",
    "relatorio",
    "dados",
    "voce",
    "isso",
    "entao",
    "tambem",
    "depois",
    "apague",
    "mande",
    "leia",
    "calcule",
    "linhas",
    "colunas",
];

/// The words that join two phrases of one clause, each stating its own operation (« do not
/// email the customer and write the refusal »); « or » joins what one negation forbids.
const JOINS: &[&str] = &[
    "and", "then", "also", "but", "instead", "et", "puis", "ensuite", "mais", "y", "luego",
    "despues", "pero", "e", "poi", "ma", "und", "dann", "aber", "sondern", "depois", "mas",
    "entao",
];

/// Verbs a negation turns into a demand (« don't forget to write … », « n'oublie pas … »).
const FORGET: &[&str] = &[
    "forget",
    "fail",
    "miss",
    "neglect",
    "omit",
    "oublie",
    "oublier",
    "oubliez",
    "oublies",
    "olvides",
    "olvidar",
    "olvide",
    "olvideis",
    "vergiss",
    "vergessen",
    "vergesst",
    "dimenticare",
    "dimenticate",
    "dimenticarti",
    "scordare",
    "scordarti",
    "scordate",
    "esqueca",
    "esquecer",
    "esquecas",
];

/// The words of a clause as the prohibition reader sees them, each as written (lowercase) and
/// folded, with the phrase it stands in: a path or URL dropped (its phrase names a target), a
/// hyphenated word kept whole (« non-empty » negates nothing), a French elision split
/// (« n'envoie » reads « n », « envoie »), English negative contractions read as « not »; a
/// comma, a semicolon, a colon or a joining word (« and », « et », « und ») opens a phrase.
struct Clause {
    words: Vec<(String, String)>,
    /// The phrase of each word, and each phrase's words (`start..end`) and whether it names a
    /// path.
    phrase: Vec<usize>,
    spans: Vec<std::ops::Range<usize>>,
    targets: Vec<bool>,
}

impl Clause {
    fn read(text: &str) -> Self {
        let read = text
            .to_lowercase()
            .replace("n't", " not")
            .replace("n’t", " not")
            .replace('’', "'");
        let mut clause = Self {
            words: Vec::new(),
            phrase: Vec::new(),
            spans: Vec::new(),
            targets: vec![false],
        };
        for token in read.split_whitespace() {
            let at = clause.targets.len() - 1;
            let closes = token.ends_with([',', ';', ':']);
            if token.contains(['/', '\\']) {
                clause.targets[at] = true;
            } else {
                for word in token.split('\'') {
                    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                    if word.is_empty() {
                        continue;
                    }
                    let folded = super::rule_tokens::fold(word);
                    if JOINS.contains(&folded.as_str()) {
                        clause.open();
                        continue;
                    }
                    let at = clause.targets.len() - 1;
                    clause.words.push((word.to_owned(), folded));
                    clause.phrase.push(at);
                }
            }
            if closes {
                clause.open();
            }
        }
        clause.spans = vec![0..0; clause.targets.len()];
        for (at, phrase) in clause.phrase.iter().enumerate() {
            let span = &mut clause.spans[*phrase];
            if span.start == span.end {
                *span = at..at + 1;
            } else {
                span.end = at + 1;
            }
        }
        clause
    }

    /// A new phrase, unless the current one is still empty.
    fn open(&mut self) {
        let at = self.targets.len() - 1;
        if self.phrase.last() == Some(&at) || self.targets[at] {
            self.targets.push(false);
        }
    }

    /// Which words negate: a negation of the table (« no » outside a Portuguese clause), or the
    /// French « ne » closed later by « pas », « jamais », « aucun »… before any « que » (« ne …
    /// que » restricts), or negated from before by its subject (« personne ne … »), read in one
    /// pass from the end.
    fn negations(&self) -> Vec<bool> {
        let words = &self.words;
        let portuguese = (words.iter())
            .any(|(w, f)| w.contains(['ã', 'õ', 'ç']) || PORTUGUESE.contains(&f.as_str()));
        let mut negating = vec![false; words.len()];
        // The nearest closer or « que » after the word: whether a « ne » here is closed.
        let mut closed = false;
        for (at, (_, word)) in words.iter().enumerate().rev() {
            let word = word.as_str();
            let subject = at > 0 && NE_SUBJECTS.contains(&words[at - 1].1.as_str());
            negating[at] = (NEGATIONS.contains(&word) && !(portuguese && word == "no"))
                || (NE.contains(&word) && (closed || subject));
            if NE_CLOSERS.contains(&word) {
                closed = true;
            } else if word == "que" || word == "qu" {
                closed = false;
            }
        }
        negating
    }

    /// The words of the phrase the word at `at` stands in.
    fn phrase_of(&self, at: usize) -> std::ops::Range<usize> {
        self.spans[self.phrase[at]].clone()
    }

    /// The first negation of each phrase.
    fn first_negations(&self, negating: &[bool]) -> Vec<Option<usize>> {
        let mut first = vec![None; self.targets.len()];
        for (at, phrase) in self.phrase.iter().enumerate() {
            if negating[at] && first[*phrase].is_none() {
                first[*phrase] = Some(at);
            }
        }
        first
    }

    /// Whether the phrase of the word at `at` ends with a German closing negation after it.
    fn closed_after(&self, at: usize) -> bool {
        let phrase = self.phrase_of(at);
        let last = phrase.end - 1;
        last > at && CLOSING_NEGATIONS.contains(&self.words[last].1.as_str())
    }

    /// Whether a forget verb turns the negation at `at` into a demand: within two words after
    /// it (« don't forget », « n'oublie pas »), or before it in its phrase (« vergiss nicht »,
    /// « vergiss die Kopfzeile nicht »).
    fn forgets(&self, at: usize) -> bool {
        let phrase = self.phrase_of(at);
        let before = &self.words[phrase.start..at];
        let after = self.words[at + 1..phrase.end].iter().take(2);
        (before.iter().chain(after)).any(|(_, w)| FORGET.contains(&w.as_str()))
    }

    /// Whether the word at `at` is forbidden by a negation of its phrase: one before it
    /// (« never email », « n'envoie pas »), one standing for its object within two words after
    /// it (« send no email », « make sure nobody … », « lösche nichts »), or a German negation
    /// closing the phrase (« lösche die Datei nicht »).
    fn negated(&self, at: usize, negating: &[bool], first: &[Option<usize>]) -> bool {
        let phrase = self.phrase_of(at);
        let before = first[self.phrase[at]].is_some_and(|n| n < at);
        let object = (at + 1..phrase.end.min(at + 3))
            .any(|n| negating[n] && NEGATING_OBJECTS.contains(&self.words[n].1.as_str()));
        before || object || self.closed_after(at)
    }
}

/// Whether a clause demands by a negation of forgetting (« don't forget to write … », « n'oublie
/// pas … ») and restricts nothing else: no restriction, keep or exclusion word but that
/// negation, no structure law. Such a clause asks what follows; it forbids nothing.
#[must_use]
pub fn negated_demand(text: &str) -> bool {
    let clause = Clause::read(text);
    let negating = clause.negations();
    let Some(first) = negating.iter().position(|n| *n) else {
        return false;
    };
    let read = text.replace("n't", " not").replace("n’t", " not");
    clause.forgets(first)
        && laws(&read).is_empty()
        && !clause.words.iter().enumerate().any(|(at, (_, word))| {
            let negation = negating[at] || NE_CLOSERS.contains(&word.as_str());
            (super::stages::restriction_word(word) && !negation)
                || super::stages::keep_lead(word) && !NE.contains(&word.as_str())
                || super::rules::exclusion_lead(word)
        })
}

/// Verbs that create something of their own, which the reader's heads do not read: a clause
/// stating one before its negation asks that creation too.
const CREATES: &[&str] = &[
    "produce",
    "produces",
    "create",
    "creates",
    "generate",
    "generates",
    "build",
    "builds",
    "make",
    "makes",
    "prepare",
    "prepares",
    "compose",
    "draft",
    "drafts",
    "produis",
    "produisez",
    "produire",
    "cree",
    "creez",
    "creer",
    "genere",
    "generez",
    "generer",
    "preparez",
    "redige",
    "redigez",
    "rediger",
    "crea",
    "genera",
    "prepara",
    "redacta",
    "erstelle",
    "erzeuge",
    "generiere",
    "bereite",
    "verfasse",
    "produci",
    "redigi",
    "produza",
    "gere",
    "redija",
];

/// Verbs of an effect or an operation of the workflow the reader's heads may not read, in the
/// six languages, folded: a write, a save, a send, a delete, a read, a list, a computation.
const OPERATIONS: &[&str] = &[
    "write",
    "save",
    "store",
    "send",
    "email",
    "mail",
    "post",
    "publish",
    "delete",
    "remove",
    "erase",
    "export",
    "list",
    "print",
    "copy",
    "move",
    "fetch",
    "download",
    "upload",
    "read",
    "load",
    "compute",
    "calculate",
    "sum",
    "count",
    "sort",
    "filter",
    "merge",
    "append",
    "summarize",
    "summarise",
    "notify",
    "call",
    "run",
    "insert",
    "update",
    "overwrite",
    "convert",
    "extract",
    "translate",
    "ecris",
    "ecrire",
    "ecrivez",
    "enregistre",
    "enregistrer",
    "enregistrez",
    "sauvegarde",
    "sauvegarder",
    "envoie",
    "envoyer",
    "envoyez",
    "supprime",
    "supprimer",
    "supprimez",
    "efface",
    "effacer",
    "exporte",
    "exporter",
    "imprime",
    "copier",
    "deplace",
    "recupere",
    "recuperer",
    "telecharge",
    "lis",
    "lire",
    "lisez",
    "calculer",
    "calculez",
    "compter",
    "trie",
    "trier",
    "filtre",
    "filtrer",
    "fusionne",
    "ajoute",
    "ajouter",
    "notifie",
    "convertis",
    "extrais",
    "traduis",
    "escribe",
    "escribir",
    "escriba",
    "guarda",
    "guardar",
    "guarde",
    "envia",
    "enviar",
    "envie",
    "borra",
    "borrar",
    "borres",
    "elimina",
    "eliminar",
    "exporta",
    "exportar",
    "listar",
    "copiar",
    "mueve",
    "descarga",
    "lee",
    "leer",
    "calcula",
    "calcular",
    "contar",
    "ordena",
    "ordenar",
    "filtra",
    "crear",
    "generar",
    "resumir",
    "notifica",
    "ejecuta",
    "convierte",
    "extrae",
    "traduce",
    "scrivi",
    "scrivere",
    "salva",
    "salvare",
    "invia",
    "inviare",
    "manda",
    "mandare",
    "cancella",
    "cancellare",
    "eliminare",
    "esporta",
    "esportare",
    "elenca",
    "stampa",
    "copiare",
    "sposta",
    "scarica",
    "leggi",
    "leggere",
    "carica",
    "calcola",
    "calcolare",
    "contare",
    "ordinare",
    "filtrare",
    "creare",
    "generare",
    "riassumi",
    "esegui",
    "converti",
    "estrai",
    "traduci",
    "schreibe",
    "schreib",
    "schreiben",
    "speichere",
    "speichern",
    "sende",
    "senden",
    "schicke",
    "schicken",
    "losche",
    "loesche",
    "loschen",
    "loeschen",
    "entferne",
    "entfernen",
    "exportiere",
    "exportieren",
    "drucke",
    "kopiere",
    "kopieren",
    "verschiebe",
    "lade",
    "lies",
    "lesen",
    "berechne",
    "berechnen",
    "zahle",
    "zaehle",
    "sortiere",
    "filtere",
    "filtern",
    "fasse",
    "benachrichtige",
    "konvertiere",
    "extrahiere",
    "ubersetze",
    "escreva",
    "escreve",
    "escrever",
    "salve",
    "salvar",
    "grave",
    "grava",
    "gravar",
    "guardar",
    "envie",
    "enviar",
    "mande",
    "apague",
    "apaga",
    "apagar",
    "exclua",
    "exclui",
    "excluir",
    "remova",
    "exporte",
    "imprima",
    "copie",
    "mova",
    "baixe",
    "leia",
    "ler",
    "carregue",
    "calcule",
    "conte",
    "ordene",
    "filtre",
    "crie",
    "criar",
    "gerar",
    "resuma",
    "notifique",
    "execute",
    "converta",
    "extraia",
    "traduza",
];

/// Whether a word states an operation, a selection or an exclusion of its own: a stage verb,
/// a keep or exclusion lead, a head the reader reads (« write », « send »), an effect verb of
/// the six languages, or a creation.
fn asks((written, folded): &(String, String)) -> bool {
    // A negation asks nothing of its own (« n' » reads no head).
    let negation = NEGATIONS.contains(&folded.as_str()) || NE.contains(&folded.as_str());
    !negation && asks_of_its_own(written, folded)
}

/// Whether a word that is no negation states an operation, a selection or an exclusion.
fn asks_of_its_own(written: &str, folded: &str) -> bool {
    CREATES.contains(&folded)
        || OPERATIONS.contains(&folded)
        || super::stages::operation_word(folded)
        || super::stages::keep_lead(folded)
        || super::rules::exclusion_lead(folded)
        || super::lexicon::head_of_exact(written).is_some()
        || super::lexicon::head_of_exact(folded).is_some()
}

/// Whether a clause states an operation of its own (a write, a send, a computation, a
/// selection, a creation) or names a path, beside whatever law or negation it also states:
/// « write the total to ./out/t.txt and nothing else » asks the write; « nothing else » asks
/// nothing.
#[must_use]
pub fn states_operation(text: &str) -> bool {
    let clause = Clause::read(text);
    clause.targets.iter().any(|t| *t) || clause.words.iter().any(asks)
}

/// Whether a clause is a pure prohibition (R4 A11): it forbids by negation alone (« never
/// email the customer », « need no time-zone conversion », « n'envoie pas d'email », « never
/// delete ./raw.csv », « send no email », « lösche die Datei nicht »). Every operation it
/// states is one a negation of its phrase forbids, and every phrase naming a path holds a
/// negation: « produce the report without sending an email » also asks a report, « do not
/// email the customer and write the refusal to ./r.md » asks the write, « keep the rows that
/// are not cancelled » selects. No « only », exception or condition, no structure law, and no
/// « don't forget to … » (a demand). A prohibition is carried by no task doing what it
/// forbids; it asks no operation of its own.
#[must_use]
pub fn pure_prohibition(text: &str) -> bool {
    let read = text.replace("n't", " not").replace("n’t", " not");
    if !laws(&read).is_empty() {
        return false;
    }
    let clause = Clause::read(text);
    let negating = clause.negations();
    let Some(first) = negating.iter().position(|n| *n) else {
        return false;
    };
    // « don't forget to … », « n'oublie pas … », « vergiss nicht … »: a demand, wherever it
    // stands in the clause.
    let words = &clause.words;
    if (first..words.len()).any(|at| negating[at] && clause.forgets(at)) {
        return false;
    }
    // An operation, a selection or an exclusion no negation forbids: the clause asks it too.
    let firsts = clause.first_negations(&negating);
    let asked =
        (0..words.len()).any(|at| asks(&words[at]) && !clause.negated(at, &negating, &firsts));
    // A phrase naming a path with no negation of its own asks a target (« … , just the file
    // ./out/a.md »).
    let targeted =
        (clause.targets.iter().zip(&firsts)).any(|(target, first)| *target && first.is_none());
    if asked || targeted {
        return false;
    }
    // Any other restriction (« only », an exception, a condition) asks a selection; « mai »
    // after a negation is Italian « never » (« non inviare mai »).
    !words.iter().enumerate().any(|(at, (_, word))| {
        let never = word == "mai" && negating[..at].iter().any(|n| *n);
        super::stages::restriction_word(word)
            && !negating[at]
            && !NE_CLOSERS.contains(&word.as_str())
            && !never
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clause that restricts is never one a judge may call asking for nothing (R4 A11).
    #[test]
    fn a_restriction_is_read_in_its_words_and_its_structure_laws() {
        for text in [
            "Once the brief is read, nothing else runs",
            "keep only the rows whose status is a",
            "ignore the cancelled rows",
            "sans les lignes annulées",
            "ninguna fila cancelada",
            "keine stornierten Zeilen",
            "write it to ./out.md and nothing else",
        ] {
            assert!(restricts(text), "{text}");
        }
        for text in [
            "write the sum to ./out/result.json",
            "read ./data/input.csv",
            "thanks",
        ] {
            assert!(!restricts(text), "{text}");
        }
    }

    /// A prohibition forbids by negation alone; a negation inside a selection, an exception, a
    /// condition, a demand or beside an operation of its own is no prohibition (R4 A11).
    #[test]
    fn a_pure_prohibition_is_a_negation_with_nothing_else_asked() {
        for text in [
            "Times are local and need no time-zone conversion",
            "never email the customer",
            "don't round the totals",
            "Do not copy more than 10 consecutive words",
            "sans conversion de fuseau",
            "nunca envíes el correo",
            "keine Umrechnung der Zeitzone",
            "Les heures sont locales et ne nécessitent aucune conversion de fuseau horaire",
            "N'envoie pas d'email au client",
            "Ne jamais envoyer d'email au client",
            "n'arrondis pas les totaux",
            "Never delete ./data/raw.csv",
            "Do not drop any rows",
            "Send no email to the customer",
            "Make no changes to ./data/raw.csv",
            "Delete nothing",
            "Make sure nobody is emailed",
            "Lösche nichts",
            "Lösche die Datei ./data/raw.csv nicht",
            "Sende die E-Mail nicht",
            "Personne ne doit recevoir d'email",
            "Non inviare mai email",
            "Ne supprime pas les lignes qu'on a importées",
            "Não envie emails ao cliente",
            "No envíes más correos",
            "Never delete or overwrite ./data/raw.csv",
        ] {
            assert!(pure_prohibition(text), "{text}");
        }
        for text in [
            "keep rows that are not cancelled",
            "keep only the open tickets",
            "ignore the rows without an email",
            "if a row has no email, skip it",
            "except the rows with no total",
            "write it to ./out.md and nothing else",
            "write the sum to ./out/no.json",
            "read ./data/input.csv",
            "produce the report without sending an email",
            "write the summary and do not send any email",
            "produis le rapport sans envoyer d'email",
            "Don't forget to write the summary to ./out/summary.md",
            "No olvides escribir el resumen",
            "n'oublie pas d'écrire le résumé",
            "write the non-empty rows to ./out/b.csv",
            "envoie le rapport de mai",
            "ne garde que les lignes ouvertes",
            // Portuguese « no » is « in the ».
            "Escreva o total no arquivo ./out/t.txt",
            "Salve os dados no arquivo ./out/d.csv",
            "Grave o resumo no ficheiro ./out/r.md",
            "Exporte o relatório no formato CSV",
            // An operation after the negated phrase, or before a negation that is no object.
            "Do not email the customer and write the refusal to ./out/refusal.md",
            "Ne supprime pas le fichier et écris le résumé dans ./out/r.md",
            "No borres el archivo y escribe el resumen en ./out/r.md",
            "Non cancellare il file e scrivi il riassunto in ./out/r.md",
            "Não apague o arquivo e escreva o resumo em ./out/r.md",
            "Schreibe den Bericht ohne eine E-Mail zu senden",
            "Escreva o relatório sem enviar email",
            "Speichere die nicht leeren Zeilen in ./out/b.csv",
            "Do not send any email and write the report to ./out/r.md",
            "N'envoie pas d'email et écris le rapport dans ./out/r.md",
            "Never round the totals and write them to ./out/t.csv",
            "List the customers with no orders in ./out/c.csv",
            "I never want emails, just the file ./out/a.md",
            "No, write it to ./out/b.csv instead",
            "Never delete ./a.csv, and don't forget to write ./out/b.csv",
            // A demand by a negation of forgetting, the verb anywhere before the negation.
            "Vergiss die Kopfzeile nicht",
            "Non scordare di scrivere il riassunto",
        ] {
            assert!(!pure_prohibition(text), "{text}");
        }
        // Every negation is a word of the restriction table.
        for word in NEGATIONS {
            assert!(super::super::stages::restriction_word(word), "{word}");
        }
    }

    /// A negation of forgetting demands what follows; a negation beside a selection does not.
    #[test]
    fn a_negated_demand_asks_and_forbids_nothing() {
        for text in [
            "Don't forget to write the summary to ./out/summary.md",
            "No olvides escribir el resumen",
            "n'oublie pas d'écrire le résumé",
            "Vergiss nicht, die Datei zu schreiben",
            "Vergiss die Kopfzeile nicht",
            "Non scordare di scrivere il riassunto",
        ] {
            assert!(negated_demand(text), "{text}");
            assert!(!pure_prohibition(text), "{text}");
        }
        for text in [
            "never email the customer",
            "don't forget to keep only the open rows",
            "write the summary to ./out/summary.md",
            "ne garde que les lignes ouvertes",
        ] {
            assert!(!negated_demand(text), "{text}");
        }
    }

    /// An operation stated beside a structure law is asked; a law alone asks none.
    #[test]
    fn an_operation_beside_a_law_is_stated_and_a_law_alone_states_none() {
        for text in [
            "Write the total to ./out/t.txt and nothing else",
            "Fetch the rates with a single HTTP request and write them to ./out/r.json",
            "Write the report to ./out/r.md and no other file",
            "Summarize ./notes.md without any language model and write ./out/s.md",
            "Écris le total dans ./out/t.txt, rien d'autre",
        ] {
            assert!(states_operation(text), "{text}");
        }
        for text in [
            "Nothing else.",
            "No other file.",
            "No language model.",
            "A single HTTP request, not one per fine.",
            "Nessun altro file.",
            "une seule requête HTTP",
        ] {
            assert!(!states_operation(text), "{text}");
        }
    }

    #[test]
    fn structure_laws_are_read_in_six_languages() {
        for (text, law) in [
            ("write it to ./out.md and nothing else", Law::NothingElse),
            ("Écris-le dans ./out.md, rien d'autre.", Law::NothingElse),
            ("Escribe eso en ./out.md, nada más.", Law::NothingElse),
            ("Scrivilo in ./out.md, nient'altro.", Law::NothingElse),
            ("Schreib es nach ./out.md, sonst nix.", Law::NothingElse),
            (
                "Escreve isso em ./out.md, nada mais além disso vlw",
                Law::NothingElse,
            ),
            ("Nessun altro file.", Law::NoOtherFile),
            ("aucun autre fichier", Law::NoOtherFile),
            ("No language model.", Law::NoModel),
            ("sans modèle de langage", Law::NoModel),
            ("Sem modelo de linguagem.", Law::NoModel),
            ("Nessun modello di linguaggio", Law::NoModel),
            ("kein Sprachmodell", Law::NoModel),
            ("sin modelo de lenguaje", Law::NoModel),
            (
                "A single HTTP request, not one per fine.",
                Law::SingleRequest,
            ),
            (
                "Um único pedido HTTP, não um por multa.",
                Law::SingleRequest,
            ),
            ("une seule requête HTTP", Law::SingleRequest),
            ("una sola richiesta", Law::SingleRequest),
            ("eine einzige Anfrage", Law::SingleRequest),
        ] {
            assert!(laws(text).contains(&law), "{text}: {:?}", laws(text));
        }
        assert_eq!(
            laws("Nessun modello di linguaggio, nessun altro file."),
            [Law::NoModel, Law::NoOtherFile]
        );
        // A closure phrase that carries a bound is the bound's wording.
        assert!(laws("nada más que 3 líneas").is_empty());
        assert!(laws("in a warm tone").is_empty());
        assert!(laws("Read ./a.md and write it to ./b.md").is_empty());
    }

    #[test]
    fn a_context_sentence_describes_the_material_and_a_demand_does_not() {
        for text in [
            "Le fichier ./cave/recolte-2026.csv contient les colonnes parcelle,cepage,kg,degre",
            "which has the columns loan_id,member,title,due_date,returned",
            "Both requirements are mandatory and are checked on the produced file",
            "The file ./people.json is a JSON array of records",
            "La tabla tiene las columnas id,nombre,total",
            "Die Datei hat die Spalten artikel,stueck",
            "O ficheiro tem as colunas paciente,data,medico",
            "Il file ha le colonne codice,prezzo",
        ] {
            assert!(context_statement(text), "{text}");
        }
        for text in [
            "in a warm, down-to-earth tone",
            "The file is written in French",
            "3 lignes max",
            "Do not copy more than 10 consecutive words",
            "write it to ./out.md only after my approval",
            "the tone is formal",
            "Nothing else.",
        ] {
            assert!(!context_statement(text), "{text}");
        }
    }

    /// A selection of the material's rows is a demand in every language the grammar leads
    /// with, whatever literal its predicate compares (R4 A10): « whose status is a » defeats
    /// the rule grammar, never the category. The context sentences stay context; a keep of
    /// something else than rows (« keep the tone formal ») stays a constraint of the prose.
    #[test]
    fn a_selection_of_the_rows_is_a_demand_never_context() {
        for text in [
            "keep the rows whose status is a",
            "keep only the records where the grade is a",
            "ignore the rows whose status is a",
            "garde les lignes dont le statut est a",
            "conserva le righe il cui codice contiene a",
            "mantén las filas cuyo estado es a",
            "behalte die Zeilen, deren Status a ist",
        ] {
            assert!(selection_demand(text), "{text}");
            assert!(!context_statement(text), "{text}");
            assert!(!binds_no_operation(text), "{text}");
        }
        for text in [
            "Le fichier ./cave/recolte-2026.csv contient les colonnes parcelle,cepage,kg,degre",
            "which has the columns loan_id,member,title,due_date,returned",
            "Both requirements are mandatory and are checked on the produced file",
            "The file ./people.json is a JSON array of records",
            "La tabla tiene las columnas id,nombre,total",
            "Die Datei hat die Spalten artikel,stueck",
            "O ficheiro tem as colunas paciente,data,medico",
            "Il file ha le colonne codice,prezzo",
            "keep the tone formal",
        ] {
            assert!(!selection_demand(text), "{text}");
        }
        assert!(binds_no_operation("Nothing else."));
        assert!(binds_no_operation("which has the columns a,b,c"));
        assert!(!binds_no_operation("in exactly that order"));
    }
}
