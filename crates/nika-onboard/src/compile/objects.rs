//! Object laws of the deterministic reader. A path literal settles an operation but not the
//! rest of its clause: what the clause still says after the path re-enters the reader as its
//! own clause, and what a write names before its destination is either a reference to
//! something the request already produced or new content the write demands. These are
//! structural laws (a path is a reliable clause boundary; a definite reference recurs; an
//! indefinite object is new), not cue lists.

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
        "and ", "et ", "then ", "puis ", "but ", "mais ", "or ", "ou ", "ensuite ",
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
/// path follows it. A path before the connector is a source, not a destination.
pub(super) fn destination_at(detail_lower: &str, path_at: usize) -> Option<usize> {
    [" to ", " into ", " dans ", " sous ", " vers "]
        .iter()
        .filter_map(|c| detail_lower.find(c))
        .filter(|pos| *pos < path_at)
        .min()
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

/// Whether a target carries a literal (a path, a URL or an address).
pub(super) fn has_literal(target: &str) -> bool {
    target.split_whitespace().any(|w| {
        w.starts_with("./")
            || w.starts_with("http://")
            || w.starts_with("https://")
            || (w.contains('@') && w.contains('.'))
    })
}

/// Does the object of a write refer back to something already in the request, or does it
/// name new content the write demands? A pronoun or a generic result word refers back; so
/// does a head noun that recurs in an earlier clause (`the count` after `count the tickets`).
/// Anything else (`a 3-bullet summary`, `the summary` with nothing summarized before) is new.
pub(super) fn refers_back<'a>(object_lower: &str, earlier: impl Iterator<Item = &'a str>) -> bool {
    const DETERMINERS: &[&str] = &[
        "a", "an", "the", "un", "une", "le", "la", "les", "l'", "des", "du", "de", "ce", "cet",
        "cette", "ces", "this", "that", "these", "those", "my", "mon", "ma", "mes", "its", "their",
        "son", "sa", "ses", "all", "tout", "tous", "toutes", "only",
    ];
    const BACK_REFERENCES: &[&str] = &[
        "it",
        "them",
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
    ];
    const OF: &[&str] = &["of", "de", "du", "des", "d'", "from", "about", "sur"];
    let tokens: Vec<&str> = object_lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .map(|t| t.trim_matches('-'))
        .filter(|t| !t.is_empty())
        .collect();
    let content: Vec<&str> = tokens
        .iter()
        .skip_while(|t| DETERMINERS.contains(t))
        .copied()
        .collect();
    let Some(first) = content.first() else {
        return true;
    };
    if BACK_REFERENCES.contains(first) {
        return true;
    }
    let head = content
        .iter()
        .take_while(|t| !OF.contains(t))
        .last()
        .copied()
        .unwrap_or(first);
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
