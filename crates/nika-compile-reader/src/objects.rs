//! Object laws of the deterministic reader. A path literal settles an operation but not the
//! rest of its clause: what the clause still says after the path re-enters the reader as its
//! own clause, and what a write names before its destination is either a reference to
//! something the request already produced or new content the write demands. These are
//! structural laws (a path is a reliable clause boundary; a definite reference recurs; an
//! indefinite object is new), not cue lists.

use super::lexicon::{ARTICLES, OBJECT_CONNECTORS, fold_apostrophes};
use super::plan::Plan;

fn normalize(text: &str) -> String {
    fold_apostrophes(text).to_lowercase()
}

/// What an object still says after its path literal, beyond one parenthetical hint attached
/// to the path and trailing punctuation. A non-empty residue is a demand the path did not
/// settle; the reader must not let it vanish with the path.
pub(crate) fn residue_after_path(detail: &str, path: &str) -> String {
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
#[must_use]
pub fn as_clause(residue: &str) -> &str {
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
#[must_use]
pub fn destination_at(detail_lower: &str, path_at: usize) -> Option<usize> {
    const ANYWHERE: &[&str] = &[
        " to ", " into ", " dans ", " sous ", " vers ", " → ", " -> ",
    ];
    const ADJACENT: &[&str] = &[
        " in ", " en ", " nel ", " nella ", " su ", " sul ", " sulla ",
    ];
    let padded = format!(" {detail_lower}");
    let path_at = path_at + 1;
    // A connector right before the path is the destination's own (« dans l'ordre, une par
    // ligne → ./out/titres.txt »: the arrow, never the locative « dans » before it).
    let right_before = ANYWHERE
        .iter()
        .chain(ADJACENT)
        .filter(|c| c.len() <= path_at)
        .find(|c| padded[..path_at].ends_with(**c))
        .map(|c| path_at - c.len());
    if let Some(pos) = right_before {
        return Some(pos.saturating_sub(1));
    }
    ANYWHERE
        .iter()
        .filter_map(|c| padded.find(c))
        .filter(|pos| *pos < path_at)
        .min()
        .map(|pos| pos.saturating_sub(1))
}

/// The target an effect phrase names: its destination path when a connector introduces one
/// (`writing it to ./final.md` → `./final.md`), else the phrase itself.
pub(crate) fn destination_target(text: &str) -> &str {
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
pub(crate) fn write_segments(detail: &str) -> Vec<(String, String, String)> {
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
pub(crate) fn path_list(text: &str) -> Option<Vec<String>> {
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
#[must_use]
pub fn has_literal(target: &str) -> bool {
    target.split_whitespace().any(|w| {
        w.starts_with("./")
            || w.starts_with("http://")
            || w.starts_with("https://")
            || (w.contains('@') && w.contains('.'))
    })
}

/// The text with every listed column blanked out, underscores kept, so a column that spells
/// a verb (`credit_cents`, `email`) is never read as one.
pub(crate) fn mask_columns(lower: &str, columns: &[String]) -> String {
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
pub(crate) fn folds(object_lower: &str) -> bool {
    super::rule_tokens::fold(object_lower)
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
pub(crate) fn names_classification(object_lower: &str) -> bool {
    super::rule_tokens::fold(object_lower)
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
/// Object clitics the reader keeps attached to the verb it stripped (FR « -le », « -la »,
/// « -les »; ES « -lo », « -la », « -los », « -las »; PT « -o », « -a », « -os », « -as »),
/// and the identity cues an object opens with when the pronoun was glued to the verb
/// itself (« escríbelo tal cual en … », « scrivilo così com'è in … »): what is written as it
/// is is what was read. A bare article (« la réponse ») is a determiner, never a pronoun.
const IDENTITY: &[&str] = &[
    "as is",
    "as-is",
    "verbatim",
    "byte for byte",
    "tel quel",
    "telle quelle",
    "tels quels",
    "octet pour octet",
    "tal cual",
    "tal como está",
    "così com'è",
    "cosi com'e",
    "così come",
    "unverändert",
    "unverandert",
    "wie es ist",
    "tal e qual",
    "à l'identique",
];

/// An entire span stating only identity, rather than merely opening with an identity
/// cue. A copy cannot discard arbitrary work after `as is` or `tel quel`.
pub(crate) fn identity_only(text: &str) -> bool {
    text.split(',')
        .map(str::trim)
        .all(|part| part.is_empty() || IDENTITY.contains(&part))
}

fn object_clitic_or_identity(object_lower: &str) -> bool {
    const CLITICS: &[&str] = &[
        "-le", "-la", "-les", "-lo", "-los", "-las", "-o", "-a", "-os", "-as", "it",
    ];
    let object = object_lower.trim_start();
    let first = object
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()
        .unwrap_or_default();
    CLITICS.contains(&first) || IDENTITY.iter().any(|cue| object.starts_with(cue))
}

pub(crate) fn refers_back<'a>(object_lower: &str, earlier: impl Iterator<Item = &'a str>) -> bool {
    // « écris-le tel quel dans … », « escríbelo en … », « escreve-o em … »: the object is the
    // clitic pronoun glued to the verb, and it stands for the material read before.
    if object_clitic_or_identity(object_lower) {
        return true;
    }
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
pub(crate) fn explicit_object(detail: &str) -> bool {
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
pub(crate) fn explicit_field_list(detail: &str) -> bool {
    let lower = normalize(without_distributive_tail(detail));
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

/// Phrases that open a distributive scope at the end of an object ("the supplier, the date
/// and the amount of each one", "les champs de chaque facture"): ASCII, lowercase.
const DISTRIBUTIVE_OPENERS: &[&str] = &[
    " of each",
    " of every",
    " for each",
    " for every",
    " from each",
    " in each",
    " de chaque",
    " de chacun",
    " pour chaque",
    " dans chaque",
    " di ciascun",
    " di ogni",
    " de cada",
];

/// The object without its trailing distributive scope: "the supplier, the date and the
/// amount of each one" → "the supplier, the date and the amount". The scope is the opener
/// and at most two words after it, at the very end; anything else is not a scope.
#[must_use]
pub fn without_distributive_tail(text: &str) -> &str {
    let text = text.trim().trim_end_matches(['.', ',', ';', ':']);
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return text;
    }
    let cut = DISTRIBUTIVE_OPENERS
        .iter()
        .filter_map(|opener| lower.rfind(opener).map(|at| (at, opener.len())))
        .max_by_key(|(at, _)| *at);
    match cut {
        Some((at, len))
            if lower
                .get(at + len..)
                .is_some_and(|tail| tail.split_whitespace().count() <= 2) =>
        {
            text.get(..at).unwrap_or(text).trim()
        }
        _ => text,
    }
}

/// A facet of a fetched page a write carries as it is: the extract mode of `nika:fetch`
/// that yields it and, for one field of the metadata object, that field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Facet {
    pub mode: &'static str,
    pub field: Option<&'static str>,
}

impl Facet {
    #[must_use]
    pub const fn mode(mode: &'static str) -> Self {
        Self { mode, field: None }
    }
    #[must_use]
    pub const fn field(field: &'static str) -> Self {
        Self {
            mode: "metadata",
            field: Some(field),
        }
    }
}

/// Words that name the page itself beside a facet ("the page title", "il titolo della
/// pagina"), folded.
const PAGE_WORDS: &[&str] = &[
    "page", "pages", "pagina", "webpage", "web", "site", "website", "url", "seite", "webseite",
];

/// Words that name the article of a page: its readable body, `mode: article`.
const ARTICLE_WORDS: &[&str] = &["article", "articles", "articolo", "articulo", "artikel"];

/// `of` and its kin, and the connectors that join a facet to its destination or its form
/// ("the page as markdown"): never content, folded.
const LINK_WORDS: &[&str] = &[
    "of", "de", "du", "des", "d", "della", "del", "dell", "di", "da", "von", "der", "from", "to",
    "into", "in", "dans", "vers", "sous", "en", "nel", "nella", "su", "sul", "sulla", "as",
    "comme", "come", "como", "als",
];

/// The facet one head noun names, folded (EN · FR · IT · ES · DE).
fn facet_head(word: &str) -> Option<Facet> {
    const TITLE: &[&str] = &["title", "titre", "titolo", "titulo", "titel"];
    const DESCRIPTION: &[&str] = &["description", "descrizione", "descripcion", "beschreibung"];
    const TEXT: &[&str] = &["text", "texte", "testo", "texto"];
    const CONTENT: &[&str] = &[
        "content",
        "contents",
        "contenu",
        "contenido",
        "contenuto",
        "body",
        "corps",
        "inhalt",
    ];
    const HTML: &[&str] = &["html", "raw"];
    const LINKS: &[&str] = &[
        "links",
        "link",
        "liens",
        "lien",
        "enlaces",
        "enlace",
        "collegamenti",
        "collegamento",
    ];
    const METADATA: &[&str] = &[
        "metadata",
        "metadonnees",
        "metadati",
        "metadatos",
        "metadaten",
    ];
    if TITLE.contains(&word) {
        Some(Facet::field("title"))
    } else if DESCRIPTION.contains(&word) {
        Some(Facet::field("description"))
    } else if TEXT.contains(&word) {
        Some(Facet::mode("text"))
    } else if CONTENT.contains(&word) || word == "markdown" {
        Some(Facet::mode("markdown"))
    } else if HTML.contains(&word) {
        Some(Facet::mode("raw"))
    } else if LINKS.contains(&word) {
        Some(Facet::mode("links"))
    } else if METADATA.contains(&word) {
        Some(Facet::mode("metadata"))
    } else {
        None
    }
}

/// The facet of the fetched page an object names, when it names nothing else: "the page
/// title", "le titre de la page", "the article text", "the links", "the page". Any word
/// outside the page, facet, article and link tables ("a summary of the page", "the top 3
/// links") is content the fetch does not yield as it is, so no facet is read.
#[must_use]
pub fn page_facet(object: &str) -> Option<Facet> {
    let folded = super::hot::fold(&fold_apostrophes(object)).replace("'s", "");
    let mut head: Option<Facet> = None;
    let mut article = false;
    let mut page = false;
    for token in folded
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|t| !t.is_empty())
    {
        if ARTICLES.contains(&token) || LINK_WORDS.contains(&token) {
            continue;
        }
        if PAGE_WORDS.contains(&token) {
            page = true;
            continue;
        }
        if ARTICLE_WORDS.contains(&token) {
            article = true;
            continue;
        }
        head = Some(facet_head(token)?);
    }
    match (head, article, page) {
        // "the article text", "le contenu de l'article": the readable body.
        (Some(facet), true, _) if matches!(facet.mode, "text" | "markdown") => {
            Some(Facet::mode("article"))
        }
        (Some(facet), _, _) => Some(facet),
        (None, true, _) => Some(Facet::mode("article")),
        (None, false, true) => Some(Facet::mode("markdown")),
        (None, false, false) => None,
    }
}

/// Possessive determiners in six languages, folded: an object led by one names the
/// requester's or a party's own records (« mes disponibilités », « our tickets »).
const POSSESSIVES: &[&str] = &[
    "my", "our", "your", "his", "her", "their", "mon", "ma", "mes", "notre", "nos", "votre", "vos",
    "leur", "leurs", "mi", "mis", "nuestro", "nuestra", "nuestros", "nuestras", "tu", "tus",
    "vuestro", "vuestra", "vuestros", "vuestras", "mio", "mia", "miei", "mie", "nostro", "nostra",
    "nostri", "nostre", "tuo", "tua", "tuoi", "tue", "loro", "mein", "meine", "meinen", "meiner",
    "meines", "meinem", "unser", "unsere", "unseren", "unserer", "unseres", "unserem", "dein",
    "deine", "deinen", "deiner", "meu", "meus", "minha", "minhas", "nosso", "nossa", "nossos",
    "nossas", "teu", "teus", "teua", "teuas",
];

/// Whether an object (folded) is led by a possessive determiner, after an optional article
/// (« le mie disponibilità »): the requester's or a party's own records, kept somewhere,
/// never the material an invocation supplies.
#[must_use]
pub fn possessive_object(object_lower: &str) -> bool {
    let mut words = object_lower
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()));
    let Some(first) = words.next() else {
        return false;
    };
    if POSSESSIVES.contains(&first) {
        return true;
    }
    matches!(
        first,
        "the"
            | "le"
            | "la"
            | "les"
            | "el"
            | "los"
            | "las"
            | "il"
            | "lo"
            | "i"
            | "gli"
            | "o"
            | "os"
            | "as"
    ) && words
        .next()
        .is_some_and(|second| POSSESSIVES.contains(&second))
}

/// Connectors that join an effect's object to its destination ("it to `<url>`", "le
/// rapport à ops@x"), folded.
const DESTINATION_CONNECTORS: &[&str] = &[
    "to", "into", "at", "on", "onto", "vers", "a", "sur", "dans", "en", "su", "al", "an", "nach",
];

/// The object of an effect phrase before its destination literal and the connector that
/// joins them: `it to https://x/notify` → `it`; `the report to ops@x` → `the report`.
fn object_before_destination(target: &str) -> String {
    let lower = super::hot::fold(&fold_apostrophes(target));
    let cut = lower
        .split_whitespace()
        .find(|w| {
            w.starts_with("http://")
                || w.starts_with("https://")
                || (w.contains('@') && w.contains('.'))
        })
        .and_then(|w| lower.find(w))
        .unwrap_or(lower.len());
    let words: Vec<&str> = lower
        .get(..cut)
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let end = words.len().saturating_sub(usize::from(
        words
            .last()
            .is_some_and(|w| DESTINATION_CONNECTORS.contains(w)),
    ));
    words.get(..end).unwrap_or_default().join(" ")
}

/// Whether an effect phrase carries material the plan already holds, unchanged: its object
/// before the destination is a back-reference ("it", "the file") or names a source or a
/// produced result by its own head ("the report" after `./report.md`, "the reply" after a
/// drafted reply). Anything else ("a summary") is content a step must produce first.
#[must_use]
pub fn carried(target: &str, plan: &Plan) -> bool {
    let object = object_before_destination(target);
    refers_back(&object, plan.steps.iter().map(|s| s.detail.as_str()))
}

#[cfg(test)]
mod tests {
    use super::super::plan::{Op, Plan, Step};
    use super::*;

    #[test]
    fn a_possessive_object_names_owned_records() {
        for object in [
            "mes disponibilités et celles des participants",
            "my calendar and the participants' availability",
            "nuestros tickets abiertos",
            "le mie disponibilità",
            "meine termine",
            "os meus horários",
        ] {
            assert!(possessive_object(object), "{object}");
        }
        for object in [
            "la transcription fournie",
            "the supplied text",
            "./notes/brief.md",
            "",
            "sur le fil slack",
        ] {
            assert!(!possessive_object(object), "{object}");
        }
    }

    fn plan_with(details: &[(Op, &str)]) -> Plan {
        let mut plan = Plan::default();
        for (op, detail) in details {
            plan.steps.push(Step {
                op: *op,
                evidence: (*detail).to_owned(),
                detail: (*detail).to_owned(),
                categories: Vec::new(),
            });
        }
        plan
    }

    #[test]
    fn a_carried_object_is_a_back_reference_or_a_head_that_recurs_in_a_source() {
        let read = plan_with(&[(Op::Read, "./report.md")]);
        assert_eq!(object_before_destination("it to https://x/notify"), "it");
        assert_eq!(
            object_before_destination("the report to ops@example.invalid"),
            "the report"
        );
        assert_eq!(
            object_before_destination("le rapport à ops@x.fr"),
            "le rapport"
        );
        assert_eq!(object_before_destination("sending"), "sending");
        for target in [
            "it to https://x/notify",
            "the report to https://x/notify",
            "the file to https://x/notify",
            "https://x/notify",
        ] {
            assert!(carried(target, &read), "{target}");
        }
        for target in ["a summary to https://x/notify", "a reply to ops@x"] {
            assert!(!carried(target, &read), "{target}");
        }
        let drafted = plan_with(&[(Op::Read, "./inbox/a.md"), (Op::Draft, "a reply")]);
        assert!(carried("the reply to ops@example.invalid", &drafted));
    }

    #[test]
    fn a_facet_is_read_from_the_page_words_alone_in_five_languages() {
        for (object, facet) in [
            ("the page title", Facet::field("title")),
            ("the title of the page", Facet::field("title")),
            ("the page's title", Facet::field("title")),
            ("le titre de la page", Facet::field("title")),
            ("il titolo della pagina", Facet::field("title")),
            ("el título de la página", Facet::field("title")),
            ("der Titel der Seite", Facet::field("title")),
            ("the page description", Facet::field("description")),
            ("the article text", Facet::mode("article")),
            ("the article", Facet::mode("article")),
            ("le texte de l'article", Facet::mode("article")),
            ("the page text", Facet::mode("text")),
            ("the text", Facet::mode("text")),
            ("the page", Facet::mode("markdown")),
            ("the page content", Facet::mode("markdown")),
            ("the page as markdown", Facet::mode("markdown")),
            ("the raw html", Facet::mode("raw")),
            ("the links", Facet::mode("links")),
            ("les liens de la page", Facet::mode("links")),
            ("the page metadata", Facet::mode("metadata")),
        ] {
            assert_eq!(page_facet(object), Some(facet), "{object}");
        }
    }

    #[test]
    fn content_the_fetch_does_not_yield_as_it_is_reads_no_facet() {
        for object in [
            "a summary of the page",
            "the summary",
            "the top 3 links",
            "a short note about the page",
            "the page title and the description",
            "it",
            "-le",
            "",
            "the main points",
        ] {
            assert_eq!(page_facet(object), None, "{object}");
        }
    }

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
