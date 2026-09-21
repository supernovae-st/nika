//! Object laws of the deterministic reader. A path literal settles an operation but not the
//! rest of its clause: what the clause still says after the path re-enters the reader as its
//! own clause, and what a write names before its destination is either a reference to
//! something the request already produced or new content the write demands. These are
//! structural laws (a path is a reliable clause boundary; a definite reference recurs; an
//! indefinite object is new), not cue lists.

use super::lexicon::{ARTICLES, OBJECT_CONNECTORS, fold_apostrophes};

fn normalize(text: &str) -> String {
    fold_apostrophes(text).to_lowercase()
}

/// What an object still says after its path literal, beyond one parenthetical hint attached
/// to the path and trailing punctuation. A non-empty residue is a demand the path did not
/// settle; the reader must not let it vanish with the path.
pub(super) fn residue_after_path(detail: &str, path: &str) -> String {
    let Some(at) = detail.find(path) else {
        return String::new();
    };
    let mut rest = detail
        .get(at + path.len()..)
        .unwrap_or_default()
        .trim_start_matches(['.', ',', ';', ':'])
        .trim();
    if rest.starts_with('(')
        && let Some(close) = rest.find(')')
    {
        rest = rest.get(close + 1..).unwrap_or_default().trim();
    }
    rest.trim_matches(|c: char| !c.is_alphanumeric()).to_owned()
}

/// A residue re-enters the reader as a clause once the connector that joined it to the path
/// is gone: `, and keep the rows that matter` → `keep the rows that matter`.
pub(super) fn as_clause(residue: &str) -> &str {
    const CONNECTORS: &[&str] = &[
        "and ", "et ", "then ", "puis ", "but ", "mais ", "or ", "ou ", "ensuite ", "e ", "ed ",
        "poi ", "quindi ", "y ", "luego ",
    ];
    let mut text = residue.trim().trim_start_matches([',', ';']).trim();
    loop {
        let cut = CONNECTORS
            .iter()
            .find(|c| {
                text.get(..c.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(c))
            })
            .map(|c| c.len());
        match cut {
            Some(n) => text = text.get(n..).unwrap_or_default().trim_start(),
            None => return text,
        }
    }
}

/// The byte position of the destination connector of a write (`… to ./out/x.md`), when the
/// path follows it. A path before the connector is a source, not a destination. A
/// locative (`in ./x.md`, `en ./x.md`, `nel ./x.md`) is a destination only when the path
/// follows it immediately: `in 3 bullets to ./x.md` keeps its `to`. The connector may open
/// the object (`salvalo in ./x.md` reads as `in ./x.md`): the position is then 0.
pub(super) fn destination_at(detail_lower: &str, path_at: usize) -> Option<usize> {
    const ANYWHERE: &[&str] = &[" to ", " into ", " dans ", " sous ", " vers "];
    const ADJACENT: &[&str] = &[
        " in ", " en ", " nel ", " nella ", " su ", " sul ", " sulla ",
    ];
    let padded = format!(" {detail_lower}");
    let path_at = path_at + 1;
    let anywhere = ANYWHERE
        .iter()
        .filter_map(|c| padded.find(c))
        .filter(|pos| *pos < path_at)
        .min();
    let adjacent = ADJACENT
        .iter()
        .filter_map(|c| padded.find(c).map(|pos| (pos, pos + c.len())))
        .find(|(_, end)| *end == path_at)
        .map(|(pos, _)| pos);
    anywhere
        .into_iter()
        .chain(adjacent)
        .min()
        .map(|pos| pos.saturating_sub(1))
}

/// The target an effect phrase names: its destination path when a connector introduces one
/// (`writing it to ./final.md` → `./final.md`), else the phrase itself.
pub(super) fn destination_target(text: &str) -> &str {
    let path = text
        .split_whitespace()
        .map(|w| w.trim_end_matches(['.', ',', ';', ')', ':']))
        .find(|w| (w.starts_with("./") || (w.starts_with('/') && w.contains('.'))) && w.len() > 2);
    match path.and_then(|p| text.find(p).map(|at| (p, at))) {
        Some((p, at)) if destination_at(text, at).is_some() || text.trim_start().starts_with(p) => {
            p
        }
        _ => text.trim(),
    }
}

/// The destinations of one write clause, in order, when it names several: "the bugs to
/// ./bugs.json and the features to ./features.json" → `("the bugs", "./bugs.json", "the
/// bugs to ./bugs.json")` then `("the features", "./features.json", "the features to
/// ./features.json")`. Every segment is a verbatim span of the detail: an object, a
/// destination connector, one path. A later path with nothing of its own before it ("them
/// to ./bugs.json and ./features.json") shares the object of the earlier one, and its
/// excerpt is the whole span from that object. One destination, or a first destination
/// with no object before it, is not a split.
pub(super) fn write_segments(detail: &str) -> Vec<(String, String, String)> {
    let lower = detail.to_lowercase();
    if lower.len() != detail.len() {
        return Vec::new();
    }
    let mut paths: Vec<(usize, usize)> = Vec::new();
    let mut cursor = 0;
    for word in detail.split_whitespace() {
        let Some(found) = detail.get(cursor..).and_then(|rest| rest.find(word)) else {
            break;
        };
        let start = cursor + found;
        cursor = start + word.len();
        let core = word.trim_end_matches(['.', ',', ';', ')', ':']);
        let is_path = (core.starts_with("./") || (core.starts_with('/') && core.contains('.')))
            && core.len() > 2;
        if is_path {
            paths.push((start, start + core.len()));
        }
    }
    if paths.len() < 2 {
        return Vec::new();
    }
    let mut segments: Vec<(String, String, String)> = Vec::new();
    let mut from = 0;
    for (start, end) in paths {
        let Some(span) = detail.get(from..end) else {
            return Vec::new();
        };
        let segment = as_clause(span);
        let Some(segment_lower) = lower.get(end - segment.len()..end) else {
            return Vec::new();
        };
        let path = detail.get(start..end).unwrap_or_default().to_owned();
        let path_at = segment.len() - (end - start);
        let object = destination_at(segment_lower, path_at)
            .map(|pos| segment.get(..pos).unwrap_or_default().trim())
            .filter(|object| !object.is_empty());
        match (object, segments.last()) {
            (Some(object), _) => segments.push((object.to_owned(), path, segment.to_owned())),
            (None, Some((shared, _, _))) if segment == path => {
                let excerpt = as_clause(detail.get(..end).unwrap_or_default());
                segments.push((shared.clone(), path, excerpt.to_owned()));
            }
            (None, _) => return Vec::new(),
        }
        from = end;
    }
    segments
}

/// A list of files joined by list connectors ("./a.csv and ./b.csv", "./a.md ; ./b.md",
/// "./a.csv, ./b.csv"): every word a file literal or a connector, at least two files. Such
/// an object is explicit by construction: the files are copied, in order, never guessed.
pub(super) fn path_list(text: &str) -> Option<Vec<String>> {
    const LIST_WORDS: &[&str] = &["and", "et", "y", "e", "ed", "und", "&", ";", ","];
    let mut files = Vec::new();
    for word in text.split_whitespace() {
        let bare = word.trim_matches([',', ';']);
        if bare.is_empty() || LIST_WORDS.contains(&bare.to_lowercase().as_str()) {
            continue;
        }
        match super::paths::token(bare) {
            Some(super::paths::PathShape::File(file)) => files.push(file),
            _ => return None,
        }
    }
    (files.len() >= 2).then_some(files)
}

/// Whether a target carries a literal (a path, a URL or an address).
pub(super) fn has_literal(target: &str) -> bool {
    target.split_whitespace().any(|w| {
        w.starts_with("./")
            || w.starts_with("http://")
            || w.starts_with("https://")
            || (w.contains('@') && w.contains('.'))
    })
}

/// The text with every listed column blanked out, underscores kept, so a column that spells
/// a verb (`credit_cents`, `email`) is never read as one.
pub(super) fn mask_columns(lower: &str, columns: &[String]) -> String {
    if columns.is_empty() {
        return lower.to_owned();
    }
    let mut out = String::with_capacity(lower.len());
    for token in lower.split_inclusive(|c: char| !(c.is_alphanumeric() || c == '_')) {
        let (word, separator) = match token.char_indices().last() {
            Some((i, c)) if !(c.is_alphanumeric() || c == '_') => (
                token.get(..i).unwrap_or_default(),
                token.get(i..).unwrap_or_default(),
            ),
            _ => (token, ""),
        };
        if columns.iter().any(|c| c.eq_ignore_ascii_case(word)) {
            out.extend(std::iter::repeat_n(' ', word.chars().count()));
        } else {
            out.push_str(word);
        }
        out.push_str(separator);
    }
    out
}

/// Words that name a fold of pieces produced earlier ("the combined brief", "le résumé
/// fusionné", "il riassunto unito"), diacritics folded: such an object refers back to the
/// pieces a step produced, never to new content.
const FOLD_WORDS: &[&str] = &[
    "combined",
    "merged",
    "consolidated",
    "assembled",
    "concatenated",
    "collated",
    "aggregated",
    "combine",
    "combinee",
    "combines",
    "combinees",
    "fusionne",
    "fusionnee",
    "fusionnes",
    "fusionnees",
    "regroupe",
    "regroupee",
    "regroupes",
    "regroupees",
    "consolide",
    "consolidee",
    "consolides",
    "consolidees",
    "combinado",
    "combinada",
    "combinados",
    "combinadas",
    "fusionado",
    "fusionada",
    "combinato",
    "combinata",
    "combinati",
    "combinate",
    "unito",
    "unita",
    "uniti",
    "kombiniert",
    "kombinierte",
    "kombinierten",
    "zusammengefuhrt",
    "zusammengefuhrte",
    "zusammengefasst",
];

/// Whether an object names a fold of pieces ("the combined brief").
pub(super) fn folds(object_lower: &str) -> bool {
    super::shape::fold(object_lower)
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .any(|w| FOLD_WORDS.contains(&w))
}

/// Words that name the result of a classification ("the category", "la catégorie", "the
/// label"), diacritics folded: after a classify step, such an object is that result.
const CLASSIFICATION_WORDS: &[&str] = &[
    "category",
    "categories",
    "categorie",
    "categoria",
    "categorias",
    "categorie",
    "kategorie",
    "kategorien",
    "classification",
    "classifications",
    "classificazione",
    "clasificacion",
    "klassifizierung",
    "label",
    "labels",
    "etiquette",
    "etiquettes",
    "etiqueta",
    "etiquetas",
    "etichetta",
    "etichette",
];

/// Whether an object names the result of a classification ("the category").
pub(super) fn names_classification(object_lower: &str) -> bool {
    super::shape::fold(object_lower)
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .any(|w| CLASSIFICATION_WORDS.contains(&w))
}

/// A plural head refers back to its singular ("the bugs" after "bug or feature").
fn singular(head: &str) -> &str {
    head.strip_suffix('s')
        .filter(|_| head.len() > 3 && !head.ends_with("ss"))
        .unwrap_or(head)
}

/// Determiners skipped at the head of a written object.
const OBJECT_DETERMINERS: &[&str] = &[
    "a", "an", "the", "un", "une", "le", "la", "les", "l'", "des", "du", "de", "ce", "cet",
    "cette", "ces", "this", "that", "these", "those", "my", "mon", "ma", "mes", "its", "their",
    "son", "sa", "ses", "all", "tout", "tous", "toutes", "only", "il", "lo", "gli", "i", "uno",
    "una", "questo", "questa", "questi", "queste", "mio", "mia", "miei", "mie", "el", "los", "las",
    "este", "esta", "estos", "estas", "mi", "mis", "su", "sus", "tutti", "tutte", "todo", "todos",
    "todas", "solo",
];

/// Pronouns and generic result words: an object led or headed by one refers back.
const BACK_REFERENCES: &[&str] = &[
    "it",
    "them",
    "ones",
    "this",
    "that",
    "these",
    "those",
    "le",
    "la",
    "les",
    "ça",
    "cela",
    "ceci",
    "everything",
    "tout",
    "result",
    "results",
    "résultat",
    "résultats",
    "output",
    "sortie",
    "outcome",
    "content",
    "contenu",
    "file",
    "fichier",
    "files",
    "fichiers",
    "text",
    "texte",
    "document",
    "documents",
    "lo",
    "li",
    "ciò",
    "questo",
    "risultato",
    "risultati",
    "contenuto",
    "testo",
    "documento",
    "documenti",
    "esto",
    "eso",
    "resultado",
    "resultados",
    "contenido",
    "archivo",
    "archivos",
    "fichero",
    "ficheros",
    "texto",
    "documentos",
];

/// The preposition after which an object's head noun ends (`the count of the tickets`).
const OF_WORDS: &[&str] = &[
    "of", "de", "du", "des", "d'", "from", "about", "sur", "di", "del", "della", "dei", "delle",
    "degli", "sobre",
];

/// Does the object of a write refer back to something already in the request, or does it
/// name new content the write demands? A pronoun or a generic result word refers back; so
/// does a head noun that recurs in an earlier clause (`the count` after `count the tickets`).
/// Anything else (`a 3-bullet summary`, `the summary` with nothing summarized before) is new.
pub(super) fn refers_back<'a>(object_lower: &str, earlier: impl Iterator<Item = &'a str>) -> bool {
    let tokens: Vec<&str> = object_lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .map(|t| t.trim_matches('-'))
        .filter(|t| !t.is_empty())
        .collect();
    let content: Vec<&str> = tokens
        .iter()
        .skip_while(|t| OBJECT_DETERMINERS.contains(t))
        .copied()
        .collect();
    let Some(first) = content.first() else {
        return true;
    };
    let head = content
        .iter()
        .take_while(|t| !OF_WORDS.contains(t))
        .last()
        .copied()
        .unwrap_or(first);
    // A pro-form as the head ("the unique ones", "the open ones") stands for the rows an
    // earlier step produced, whatever adjective leads it.
    if BACK_REFERENCES.contains(first) || BACK_REFERENCES.contains(&head) {
        return true;
    }
    let head = singular(head);
    let stem: String = head.chars().take(4).collect();
    if stem.chars().count() < 3 {
        return false;
    }
    let mut earlier = earlier;
    earlier.any(|clause| {
        clause
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| !w.is_empty() && w.starts_with(stem.as_str()))
    })
}

/// A step object the reader may trust without a model: a typed literal (URL, path, email,
/// timezone, number) or at most four content tokens with no coordinating connector.
pub(super) fn explicit_object(detail: &str) -> bool {
    if path_list(detail).is_some() {
        return true;
    }
    let lower = normalize(detail);
    let literal = lower.split_whitespace().any(|w| {
        let w = w.trim_end_matches(['.', ',', ';', ')', ':']);
        w.starts_with("http://")
            || w.starts_with("https://")
            || w.starts_with("./")
            || (w.starts_with('/') && w.contains('.'))
            || (w.contains('@') && w.contains('.'))
            || w.starts_with("europe/")
            || w.starts_with("america/")
            || w.starts_with("asia/")
            || w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty()
    });
    let coordinated = OBJECT_CONNECTORS.iter().any(|c| lower.contains(c));
    let content = lower
        .split(|c: char| {
            !c.is_alphanumeric()
                && c != '\''
                && c != '/'
                && c != '.'
                && c != ':'
                && c != '-'
                && c != '_'
        })
        .filter(|t| !t.is_empty() && !ARTICLES.contains(t))
        .count();
    // A literal names the object only when nothing is coordinated beside it: "./orders.csv
    // and keep the rows that matter" carries a second request the literal does not cover.
    if literal {
        return !coordinated && content <= 6;
    }
    !coordinated && content <= 4
}

/// A list of the fields an extract pulls out ("the supplier, the date and the amount of each
/// one", "le fournisseur, la date et le montant"): at least two items separated by commas or
/// a conjunction, each a short noun phrase (one to three content tokens), no path, an
/// optional distributive scope at the end. Coordination here enumerates, it does not
/// compose.
pub(super) fn explicit_field_list(detail: &str) -> bool {
    let lower = normalize(super::shape::without_distributive_tail(detail));
    let listed = lower
        .replace(" and ", ", ")
        .replace(" et ", ", ")
        .replace(" y ", ", ")
        .replace(" e ", ", ")
        .replace(" und ", ", ")
        .replace(" & ", ", ");
    let items: Vec<&str> = listed
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect();
    items.len() >= 2
        && items.iter().all(|item| {
            let content = item
                .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-' && c != '_')
                .filter(|t| !t.is_empty() && !ARTICLES.contains(t))
                .count();
            (1..=3).contains(&content) && !item.contains("./") && !item.contains("://")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_residue_is_what_the_path_did_not_settle() {
        assert_eq!(
            residue_after_path(
                "./data/orders.csv (columns a,b) and keep the rows that matter",
                "./data/orders.csv"
            ),
            "and keep the rows that matter"
        );
        assert_eq!(residue_after_path("./out/kept.csv.", "./out/kept.csv"), "");
        assert_eq!(
            as_clause(", and keep the rows that matter"),
            "keep the rows that matter"
        );
        assert_eq!(
            as_clause("ask me to confirm before writing"),
            "ask me to confirm before writing"
        );
    }

    #[test]
    fn a_destination_connector_precedes_its_path() {
        let detail = "a 3-bullet summary to ./out/summary.md";
        assert_eq!(destination_at(detail, detail.find("./").unwrap()), Some(18));
        let source = "./report.pdf to bob@example.invalid";
        assert_eq!(destination_at(source, 0), None);
    }

    #[test]
    fn a_later_destination_with_no_object_of_its_own_shares_the_earlier_one() {
        let two = |s: &[&str]| s.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            write_segments("the bugs to ./bugs.json and the features to ./features.json"),
            vec![
                (two(&["the bugs", "./bugs.json", "the bugs to ./bugs.json"])),
                (two(&[
                    "the features",
                    "./features.json",
                    "the features to ./features.json"
                ])),
            ]
            .into_iter()
            .map(|v| (v[0].clone(), v[1].clone(), v[2].clone()))
            .collect::<Vec<_>>()
        );
        assert_eq!(
            write_segments("them to ./bugs.json and ./features.json"),
            vec![
                (
                    "them".to_owned(),
                    "./bugs.json".to_owned(),
                    "them to ./bugs.json".to_owned()
                ),
                (
                    "them".to_owned(),
                    "./features.json".to_owned(),
                    "them to ./bugs.json and ./features.json".to_owned()
                ),
            ]
        );
        // A first destination with no object, or one destination, is not a split.
        assert!(write_segments("./bugs.json and ./features.json").is_empty());
        assert!(write_segments("them to ./bugs.json").is_empty());
        assert!(write_segments("them to ./bugs.json and the rest").is_empty());
    }

    #[test]
    fn a_list_of_files_is_explicit_and_nothing_else_in_it_is() {
        let files = |s: &[&str]| Some(s.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>());
        assert_eq!(
            path_list("./a.csv and ./b.csv"),
            files(&["./a.csv", "./b.csv"])
        );
        assert_eq!(
            path_list("./a.csv ; ./b.csv"),
            files(&["./a.csv", "./b.csv"])
        );
        assert_eq!(
            path_list("./a.csv, ./b.csv and ./c.csv"),
            files(&["./a.csv", "./b.csv", "./c.csv"])
        );
        for none in [
            "./a.csv",
            "./a.csv and the rest",
            "./notes and ./b.csv",
            "./a.csv and ./b/*.csv",
            "the files ./a.csv and ./b.csv",
        ] {
            assert_eq!(path_list(none), None, "{none}");
        }
        assert!(explicit_object("./a.csv ; ./b.csv"));
        assert!(!explicit_object(
            "./orders.csv and keep the rows that matter"
        ));
        // A pro-form head refers back whatever adjective leads it.
        let none: [&str; 0] = [];
        assert!(refers_back("the unique ones", none.iter().copied()));
        assert!(!refers_back("the unique rows", none.iter().copied()));
    }

    #[test]
    fn a_pronoun_or_a_recurring_head_refers_back_and_an_indefinite_object_is_new() {
        let none: [&str; 0] = [];
        assert!(refers_back("it", none.iter().copied()));
        assert!(refers_back("them combined", none.iter().copied()));
        assert!(refers_back("-le", none.iter().copied()));
        assert!(refers_back("the result", none.iter().copied()));
        assert!(refers_back(
            "the count",
            ["count the tickets whose status is open"].iter().copied()
        ));
        assert!(refers_back(
            "the kept rows",
            ["keep the rows that matter"].iter().copied()
        ));
        assert!(!refers_back(
            "a 3-bullet summary",
            ["Read ./notes/brief.md"].iter().copied()
        ));
        assert!(!refers_back(
            "the summary",
            ["Read ./notes/brief.md"].iter().copied()
        ));
        assert!(!refers_back(
            "un résumé en 3 puces",
            ["Lis ./notes/brief.md"].iter().copied()
        ));
        assert!(!refers_back("a French translation", none.iter().copied()));
    }
}
