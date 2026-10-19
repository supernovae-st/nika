// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The language the person speaks, read from their own lines. The Session's own sentences (the
//! activation questions, the declaration it proposes, what saving and declaring say) follow it,
//! in English or French; a host reads it (`nika_session::SessionRuntime::language`) to say its
//! own words in it. Live gauntlet, 2026-10-16: every French-speaking persona met English
//! questions and cards between the author's French answers.
//!
//! Owned here, below the session, so the unit's members say their own words in the same
//! language; the session keeps it at its `nika_session::Lang` path.

pub mod schedule;

/// A language the Session says its own sentences in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Lang {
    /// English, the default until a line says otherwise.
    #[default]
    En,
    /// French.
    Fr,
}

/// Words only French speakers write often.
const FRENCH: &[&str] = &[
    "le", "la", "les", "des", "du", "de", "un", "une", "et", "est", "sont", "pour", "avec", "dans",
    "sur", "chaque", "je", "tu", "il", "elle", "nous", "vous", "ils", "mon", "ma", "mes", "ton",
    "ta", "ses", "ce", "cette", "ces", "qui", "que", "pas", "plus", "aussi", "mais", "ou", "donc",
    "au", "aux", "très", "tout", "tous", "toutes", "fichier", "dossier", "bonjour", "merci", "stp",
    "svp", "oui", "non", "peux", "veux", "voudrais", "fais", "lis", "écris", "envoie", "relance",
    "lance", "active", "activez", "matin", "soir", "jour", "heure", "mois", "semaine", "quand",
    "comment", "pourquoi", "moi", "toi", "lui", "leur", "même", "après",
];

/// Words only English speakers write often.
const ENGLISH: &[&str] = &[
    "the", "and", "is", "are", "for", "with", "in", "on", "each", "every", "i", "you", "he", "she",
    "we", "they", "my", "your", "his", "her", "this", "that", "these", "those", "which", "what",
    "not", "more", "also", "but", "or", "so", "to", "of", "at", "all", "file", "folder", "hello",
    "thanks", "please", "yes", "no", "can", "want", "would", "do", "make", "read", "write", "send",
    "run", "again", "activate", "morning", "evening", "day", "hour", "month", "week", "when",
    "how", "why", "me", "it", "them", "same", "after",
];

impl Lang {
    /// The language a line is written in, when its words say so: the one of more marker words
    /// (an accented letter counts as French), two at least. `None` for a line too short or too
    /// balanced to say: the language spoken so far stands.
    #[must_use]
    pub fn of(line: &str) -> Option<Self> {
        let lower = line.replace('\u{2019}', "'").to_lowercase();
        let words: Vec<&str> = (lower.split(|c: char| !c.is_alphanumeric() && c != '\''))
            .flat_map(|word| word.split('\''))
            .filter(|word| !word.is_empty())
            .collect();
        let count = |list: &[&str]| words.iter().filter(|word| list.contains(word)).count();
        let accents = (lower.chars())
            .filter(|c| "éèêëàâçùûîïô".contains(*c))
            .count()
            .min(3);
        let (fr, en) = (count(FRENCH) + accents, count(ENGLISH));
        if fr > en && fr >= 2 {
            Some(Self::Fr)
        } else if en > fr && en >= 2 {
            Some(Self::En)
        } else {
            None
        }
    }

    /// The sentence in this language, of the two given.
    #[must_use]
    pub const fn pick<'a>(self, en: &'a str, fr: &'a str) -> &'a str {
        match self {
            Self::En => en,
            Self::Fr => fr,
        }
    }

    /// The lifecycle rail with its fields named in this language (« Brouillon ✓ · Enregistré ○
    /// · … »): a host shows the Session's stages in the words its person reads.
    #[must_use]
    pub fn rail(self, lifecycle: &nika_onboard::lifecycle::Lifecycle) -> String {
        match self {
            Self::En => lifecycle.rail(),
            Self::Fr => {
                lifecycle.rail_named(["Brouillon", "Enregistré", "Vérifié", "Actif", "Run"])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Lang;

    /// The person's own lines say their language; a short or balanced line says none.
    #[test]
    fn a_line_says_the_language_it_is_written_in() {
        for (line, lang) in [
            (
                "Je voudrais un bilan des dépenses le 1er de chaque mois à 8h",
                Some(Lang::Fr),
            ),
            (
                "Maintenant active-le tous les matins à 7h stp",
                Some(Lang::Fr),
            ),
            (
                "Read ./notes/brief.md and write it to ./out/copy.md",
                Some(Lang::En),
            ),
            ("Every Tuesday at 9:15, read the notes", Some(Lang::En)),
            ("Europe/Paris", None),
            ("0.20", None),
            ("ok", None),
        ] {
            assert_eq!(Lang::of(line), lang, "{line}");
        }
    }

    /// The rail says the same stages in the person's language.
    #[test]
    fn the_rail_names_its_stages_in_the_persons_language() {
        let mut facts = nika_onboard::lifecycle::LifecycleFacts::new();
        facts.saved = true;
        facts.check_clean = Some(true);
        let lifecycle = nika_onboard::lifecycle::Lifecycle::from_facts(&facts);
        assert_eq!(Lang::En.rail(&lifecycle), lifecycle.rail());
        assert_eq!(
            Lang::Fr.rail(&lifecycle),
            "Brouillon ✓ · Enregistré ✓ · Vérifié ✓ · Actif ○ · Run ○"
        );
    }
}
