// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a read's words say after its path: work some step must realize, or only a
//! description of the material the path names.

/// The prepositions that attach a description to the source a path names
/// (« ./notes avec plein de fichiers .md », « ./data with the monthly csv files »).
const WITH: &[&str] = &["with ", "avec ", "con ", "mit ", "com "];

/// Words that open another clause or a relative one inside a residue: past one of them
/// the words may demand work, so the residue is not a description alone.
const BOUNDARIES: &[&str] = &[
    " and ",
    " et ",
    " then ",
    " puis ",
    " ensuite ",
    " but ",
    " mais ",
    " y ",
    " e ",
    " und ",
    " to ",
    " pour ",
    " para ",
    " per ",
    " um ",
    " zu ",
    " so ",
    " which ",
    " that ",
    " where ",
    " qui ",
    " que ",
    " dont ",
    " ou ",
    " où ",
    " donde ",
    " dove ",
    " che ",
    " wo ",
    " onde ",
];

/// Whether a read's residue only describes the material its path names. It must be one
/// with-phrase (no clause boundary, relative clause or quotation) that the reader's own
/// context contract reads as a description of material once attached to the source by
/// the declarative joint that contract already grants « with the columns », and in which
/// the deterministic reader finds no step, effect, obligation, rule or trigger. Anything
/// else is a residue some step must realize.
pub(super) fn describes_material(residue: &str) -> bool {
    let lower = residue.trim().to_lowercase();
    let Some(rest) = WITH.iter().find_map(|with| lower.strip_prefix(with)) else {
        return false;
    };
    let padded = format!(" {rest} ");
    if rest.contains([',', ';', ':', '"', '«', '»', '“', '”', '`'])
        || BOUNDARIES.iter().any(|b| padded.contains(b))
    {
        return false;
    }
    if !crate::structure::context_statement(&format!("contains {rest}")) {
        return false;
    }
    let plan = crate::lexicon::read(residue).plan;
    plan.steps.is_empty()
        && plan.effects.is_empty()
        && plan.obligations.is_empty()
        && plan.rules.is_empty()
        && plan.trigger.is_none()
}

#[cfg(test)]
mod tests {
    use super::describes_material;

    #[test]
    fn a_description_of_the_source_material_is_not_absorbed_work() {
        for residue in [
            "avec plein de fichiers .md de reunion de la semaine",
            "avec plein de fichiers .md",
            "with lots of .md meeting files",
            "con muchos archivos .md de las reuniones",
        ] {
            assert!(describes_material(residue), "{residue}");
        }
    }

    #[test]
    fn work_hidden_after_or_inside_a_description_stays_a_residue() {
        for residue in [
            "find ticket 42",
            "keep only the rows whose amount is strictly greater than 100",
            "avec plein de fichiers .md, trouve le ticket 42",
            "avec plein de fichiers .md et trouve le ticket 42",
            "with the monthly csv files and sum their amounts",
            "with the monthly csv files; keep only rows over 100",
            "avec plein de fichiers .md dont il faut extraire les decisions",
            "with the csv files that need a total",
            "avec le fichier \"trouve le ticket 42\"",
            "avec « trouve le ticket 42 »",
            "avec zorblax",
            "",
        ] {
            assert!(!describes_material(residue), "{residue}");
        }
    }
}
