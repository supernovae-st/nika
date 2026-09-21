//! Statements of a request that bind no operation. A context sentence describes the
//! material (« Le fichier ./x.csv contient les colonnes a,b,c », « which has the columns … »,
//! « both requirements are checked on the produced file »); a structure law bounds the shape
//! of the workflow (« nothing else », « no other file », « no language model », « a single
//! HTTP request, not one per fine »). Neither needs an operation to carry it: context is
//! realized by the material it describes, a structure law by the shape of the emitted
//! workflow, and a law the shape breaks stays unresolved so that nothing is READY against it.

use super::shape::fold;

/// A bound on the shape of the workflow, read from its form in six languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Law {
    /// No step or action beyond the stated ones.
    NothingElse,
    /// No destination beyond the stated ones.
    NoOtherFile,
    /// No language model anywhere in the workflow.
    NoModel,
    /// One outbound request in the whole run, never one per item.
    SingleRequest,
}

/// Closures: the request ends here, nothing beyond the stated steps.
const NOTHING_ELSE: &[&str] = &[
    "nothing else",
    "nothing more",
    "no other step",
    "no other action",
    "no further action",
    "that's all",
    "that is all",
    "rien d'autre",
    "rien de plus",
    "c'est tout",
    "aucune autre action",
    "nada mas",
    "eso es todo",
    "ninguna otra accion",
    "nada mais",
    "sem mais nada",
    "mais nada",
    "nient'altro",
    "niente altro",
    "nient altro",
    "nessun'altra azione",
    "sonst nichts",
    "sonst nix",
    "nichts weiter",
    "nichts anderes",
    "keine weitere aktion",
];

/// No destination beyond the stated ones.
const NO_OTHER_FILE: &[&str] = &[
    "no other file",
    "no other files",
    "aucun autre fichier",
    "pas d'autre fichier",
    "ningun otro archivo",
    "ningun otro fichero",
    "nessun altro file",
    "nenhum outro ficheiro",
    "nenhum outro arquivo",
    "keine andere datei",
    "keine weitere datei",
];

/// No language model in the workflow.
const NO_MODEL: &[&str] = &[
    "no language model",
    "without a language model",
    "without any language model",
    "no llm",
    "without an llm",
    "no model call",
    "zero model call",
    "zero model calls",
    "sans modele de langage",
    "sans aucun modele de langage",
    "aucun modele de langage",
    "sans llm",
    "sin modelo de lenguaje",
    "sin ningun modelo de lenguaje",
    "ningun modelo de lenguaje",
    "sin llm",
    "nessun modello di linguaggio",
    "senza modello di linguaggio",
    "senza alcun modello di linguaggio",
    "senza llm",
    "kein sprachmodell",
    "ohne sprachmodell",
    "ohne llm",
    "sem modelo de linguagem",
    "sem nenhum modelo de linguagem",
    "nenhum modelo de linguagem",
    "sem llm",
];

/// One outbound request for the whole run.
const SINGLE_REQUEST: &[&str] = &[
    "a single http request",
    "a single request",
    "a single post",
    "one single request",
    "one single post",
    "one http request",
    "only one request",
    "only one post",
    "une seule requete",
    "une seule requete http",
    "un seul post",
    "un seul appel http",
    "un seul envoi",
    "una sola peticion",
    "una unica peticion",
    "una sola solicitud",
    "un solo post",
    "un unico post",
    "una sola richiesta",
    "un'unica richiesta",
    "una unica richiesta",
    "un solo post http",
    "eine einzige anfrage",
    "ein einziger post",
    "nur eine anfrage",
    "nur ein post",
    "um unico pedido",
    "um so pedido",
    "um unico post",
    "uma unica requisicao",
    "uma so requisicao",
];

/// Nouns of the material a context sentence describes.
const MATERIAL: &[&str] = &[
    "column",
    "columns",
    "colonne",
    "colonnes",
    "columna",
    "columnas",
    "colonna",
    "spalte",
    "spalten",
    "coluna",
    "colunas",
    "file",
    "files",
    "fichier",
    "fichiers",
    "archivo",
    "archivos",
    "fichero",
    "ficheros",
    "arquivo",
    "arquivos",
    "ficheiro",
    "ficheiros",
    "datei",
    "dateien",
    "row",
    "rows",
    "ligne",
    "lignes",
    "fila",
    "filas",
    "riga",
    "righe",
    "zeile",
    "zeilen",
    "linha",
    "linhas",
    "record",
    "records",
    "enregistrement",
    "enregistrements",
    "registro",
    "registros",
    "datensatz",
    "datensatze",
    "field",
    "fields",
    "champ",
    "champs",
    "campo",
    "campos",
    "feld",
    "felder",
    "header",
    "headers",
    "en-tete",
    "entete",
    "cabecera",
    "intestazione",
    "kopfzeile",
    "cabecalho",
    "requirement",
    "requirements",
    "exigence",
    "exigences",
    "requisito",
    "requisitos",
    "anforderung",
    "anforderungen",
    "csv",
    "json",
    "yaml",
    "table",
    "tableau",
    "tabla",
    "tabella",
    "tabelle",
    "tabela",
    "dataset",
    "donnees",
    "datos",
    "dati",
    "daten",
    "dados",
    "data",
    "source",
    "corpus",
];

/// Verbs and joints of a description, never of a demand.
const DECLARATIVE: &[&str] = &[
    "contains",
    "contain",
    "containing",
    "holds",
    "lists",
    "has",
    "have",
    "having",
    "is",
    "are",
    "looks like",
    "comes with",
    "with the columns",
    "with columns",
    "whose columns",
    "which has",
    "that has",
    "contient",
    "contiennent",
    "contenant",
    "possede",
    "possedent",
    "comporte",
    "comportent",
    "a les colonnes",
    "a la colonne",
    "a pour colonnes",
    "avec les colonnes",
    "avec pour colonnes",
    "est",
    "sont",
    "dont les colonnes",
    "qui contient",
    "tiene",
    "tienen",
    "contiene",
    "contienen",
    "es",
    "son",
    "con las columnas",
    "cuyas columnas",
    "que tiene",
    "que contiene",
    "ha le colonne",
    "ha la colonna",
    "contengono",
    "sono",
    "con le colonne",
    "le cui colonne",
    "che ha",
    "che contiene",
    "hat",
    "haben",
    "enthalt",
    "enthalten",
    "ist",
    "sind",
    "mit den spalten",
    "deren spalten",
    "die die spalten",
    "tem",
    "tem as colunas",
    "contem",
    "sao",
    "com as colunas",
    "cujas colunas",
    "que tem",
    "que contem",
];

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

fn hit(padded: &str, table: &[&str]) -> bool {
    table.iter().any(|m| padded.contains(&format!(" {m} ")))
}

/// Every structure law a clause states, strictest first. A closure beside a measurable bound
/// (« nada más que 3 líneas ») is the bound's phrasing, not a closure.
pub(super) fn laws(text: &str) -> Vec<Law> {
    let padded = padded(text);
    let mut out = Vec::new();
    if hit(&padded, NO_MODEL) {
        out.push(Law::NoModel);
    }
    if hit(&padded, SINGLE_REQUEST) {
        out.push(Law::SingleRequest);
    }
    if hit(&padded, NO_OTHER_FILE) {
        out.push(Law::NoOtherFile);
    }
    if hit(&padded, NOTHING_ELSE) && super::cardinality::bound(text).is_none() {
        out.push(Law::NothingElse);
    }
    out
}

/// A sentence that describes the material rather than demanding work: a declarative joint
/// beside a noun of the material, with no effect word, no measurable bound, no prohibition
/// and no gate in it. « The file has the columns a,b,c », « both requirements are checked on
/// the produced file », « Le fichier ./x.csv contient les colonnes … ».
pub(super) fn context_statement(text: &str) -> bool {
    let padded = padded(text);
    if !hit(&padded, MATERIAL) || !hit(&padded, DECLARATIVE) {
        return false;
    }
    let lower = text.to_lowercase();
    let columns = super::columns::columns_hint(text);
    super::lexicon::effect_words(&lower, &columns).is_empty()
        && super::cardinality::bound(text).is_none()
        && !super::cognition::starts_with_prohibition(&lower)
        && super::gates::final_gate(&lower).is_none()
        && super::gates::named_gate(&lower).is_none()
}

/// Whether a constraint needs no operation to carry it: a context statement or a structure
/// law. The composer's carrier rule and the deterministic door's admission skip it.
pub(super) fn binds_no_operation(text: &str) -> bool {
    context_statement(text) || !laws(text).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(binds_no_operation("Nothing else."));
        assert!(binds_no_operation("which has the columns a,b,c"));
        assert!(!binds_no_operation("in exactly that order"));
    }
}
