//! The structure of a human gate. An approval requirement is not a phrase but a shape: a
//! connector that binds an effect to an approval (`after`, `once`, `if`, `until`, `before`),
//! a requirement word (`needs`, `must`, `requires`), or an asking verb, a few tokens naming
//! who approves, and a word that means approval. Reading the shape lets `only after my
//! explicit approval`, `a human must approve the write first` or `ask me to confirm before
//! writing` land as the same runtime gate without one phrase per wording.

/// Words that mean approval when they close a gate phrase.
const APPROVAL_NOUNS: &[&str] = &[
    "approval",
    "approvals",
    "confirmation",
    "validation",
    "consent",
    "sign-off",
    "signoff",
    "go-ahead",
    "accord",
    "approbation",
    "aval",
    "authorization",
    "authorisation",
    "autorisation",
    "permission",
    // ES · IT · DE · PT (accented and folded)
    "aprobación",
    "aprobacion",
    "confirmación",
    "confirmacion",
    "validación",
    "validacion",
    "autorización",
    "autorizacion",
    "permiso",
    "consentimiento",
    "approvazione",
    "conferma",
    "validazione",
    "autorizzazione",
    "permesso",
    "consenso",
    "genehmigung",
    "bestätigung",
    "bestatigung",
    "freigabe",
    "zustimmung",
    "erlaubnis",
    "aprovação",
    "aprovacao",
    "confirmação",
    "confirmacao",
    "validação",
    "validacao",
    "autorização",
    "autorizacao",
    "permissão",
    "permissao",
    "consentimento",
];
/// Approval verbs and answers: a gate only when a person performs them.
const APPROVAL_VERBS: &[&str] = &[
    "approve",
    "approves",
    "approved",
    "confirm",
    "confirms",
    "confirmed",
    "validate",
    "validates",
    "validated",
    "valide",
    "valides",
    "validé",
    "validée",
    "confirme",
    "confirmes",
    "confirmé",
    "approuve",
    "approuves",
    "approuvé",
    "yes",
    "oui",
    "ok",
    "okay",
    // ES · IT · DE · PT
    "apruebe",
    "apruebo",
    "aprueba",
    "aprobado",
    "confirme",
    "confirmo",
    "confirma",
    "confirmado",
    "valido",
    "validado",
    "sí",
    "approvi",
    "approvo",
    "approva",
    "approvato",
    "confermi",
    "confermo",
    "conferma",
    "confermato",
    "validi",
    "validato",
    "genehmige",
    "genehmigt",
    "bestätige",
    "bestatige",
    "bestätigt",
    "bestatigt",
    "freigebe",
    "freigegeben",
    "ja",
    "aprove",
    "aprovo",
    "aprova",
    "aprovado",
    "confirmado",
    "validado",
    "sim",
];
const PERSONS: &[&str] = &[
    "i",
    "me",
    "my",
    "you",
    "your",
    "we",
    "us",
    "human",
    "humans",
    "operator",
    "manager",
    "someone",
    "je",
    "j'",
    "moi",
    "mon",
    "ma",
    "tu",
    "vous",
    "nous",
    "on",
    "humain",
    "humaine",
    "quelqu'un",
    // ES · IT · DE · PT
    "yo",
    "mi",
    "mí",
    "mío",
    "mía",
    "tú",
    "usted",
    "humano",
    "humana",
    "alguien",
    "io",
    "mio",
    "mia",
    "umano",
    "umana",
    "qualcuno",
    "ich",
    "mich",
    "mir",
    "mein",
    "meine",
    "meiner",
    "du",
    "dich",
    "jemand",
    "mensch",
    "eu",
    "meu",
    "minha",
    "você",
    "voce",
    "alguém",
    "alguem",
];
/// Words that narrow a connector to the approval alone (`only after`, `seulement après`).
const ONLY: &[&str] = &[
    "only",
    "seulement",
    "uniquement",
    "solo",
    "sólo",
    "solamente",
    "soltanto",
    "nur",
    "só",
    "apenas",
];
/// Connectors that bind an effect to a later approval.
const AFTER: &[&str] = &[
    "after", "once", "upon", "if", "when", "après", "apres", "lorsque", "quand", "si", "después",
    "despues", "tras", "cuando", "dopo", "quando", "se", "nach", "sobald", "wenn", "falls",
    "depois", "após", "apos",
];
/// Requirement words: the subject before them is what needs the approval.
const REQUIRE: &[&str] = &[
    "requires",
    "require",
    "required",
    "needs",
    "need",
    "needed",
    "must",
    "exige",
    "exigent",
    "nécessite",
    "necessite",
    "doit",
    "doivent",
    "faut",
    "requis",
    "requise",
    "nécessaire",
    "necessaire",
    // ES · IT · DE · PT
    "requiere",
    "necesita",
    "debe",
    "richiede",
    "necessita",
    "deve",
    "serve",
    "erfordert",
    "braucht",
    "muss",
    "benötigt",
    "benotigt",
    "requer",
    "precisa",
    "exige",
];
/// Asking verbs: a request for approval addressed to a person (a clitic person such as
/// `demande-moi`, `pídeme` or `pergunte-me` counts as the verb and the person).
const ASK: &[&str] = &[
    "ask",
    "asks",
    "get",
    "obtain",
    "require",
    "wait",
    "await",
    "check",
    "demande",
    "demandez",
    "demander",
    "attends",
    "attendez",
    "attendre",
    "obtiens",
    "obtenez",
    "préviens",
    "previens",
    "prévenez",
    "prevenez",
    "avertis",
    "pregunta",
    "pregúntame",
    "preguntame",
    "pide",
    "pídeme",
    "pideme",
    "espera",
    "consulta",
    "chiedi",
    "chiedimi",
    "domanda",
    "aspetta",
    "attendi",
    "frag",
    "frage",
    "fragen",
    "warte",
    "hol",
    "hole",
    "pergunte",
    "pergunta",
    "peça",
    "peca",
    "pede",
    "espere",
    "aguarde",
];
/// Connectors that bound a prohibition or an asking verb by an approval: `until`, `before`.
const UNTIL: &[&str] = &[
    "until", "unless", "without", "before", "till", "sans", "avant", "jusqu'à", "jusqu'a", "tant",
    "hasta", "sin", "antes", "finché", "finche", "senza", "prima", "bis", "ohne", "bevor",
    "vorher", "até", "ate", "sem",
];

/// A clitic person glued to a verb (`demande-moi`, `pergunte-me`, `chiedimi`): the verb and
/// the person it addresses.
fn clitic(word: &str) -> Option<(&str, &str)> {
    if let Some((verb, person)) = word.rsplit_once('-')
        && PERSONS.contains(&person)
    {
        return Some((verb, person));
    }
    // Italian and Spanish glue the person without a hyphen: `chiedimi`, `pídeme`.
    for suffix in ["mi", "me"] {
        if let Some(verb) = word.strip_suffix(suffix)
            && verb.len() >= 4
            && ASK.contains(&verb)
        {
            return Some((verb, suffix));
        }
    }
    None
}

/// The asking verb a token carries, its clitic person set aside.
fn asking(word: &str) -> bool {
    ASK.contains(&word) || clitic(word).is_some_and(|(verb, _)| ASK.contains(&verb))
}

struct Token<'a> {
    word: &'a str,
    start: usize,
    end: usize,
}

fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        let inside = c.is_alphanumeric() || c == '\'' || c == '-';
        match (inside, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push(Token {
                    word: text.get(s..i).unwrap_or_default(),
                    start: s,
                    end: i,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(Token {
            word: text.get(s..).unwrap_or_default(),
            start: s,
            end: text.len(),
        });
    }
    out
}

fn is_approval(tokens: &[Token<'_>], k: usize) -> bool {
    let word = tokens[k].word.trim_matches('-');
    if APPROVAL_NOUNS.contains(&word) {
        return true;
    }
    if APPROVAL_VERBS.contains(&word) {
        // `once i approve`, `a human must approve`, `if you say yes`: a person performs it.
        let from = k.saturating_sub(3);
        return tokens[from..k].iter().any(|t| PERSONS.contains(&t.word));
    }
    false
}

/// A gate the effect it follows (or the subject before the requirement) must wait for:
/// `only after my explicit approval`, `once i approve`, `the write needs my approval first`,
/// `human approval required`. Returns the byte span of the gate phrase; the span starts at
/// the connector (or at the subject of a requirement) and ends after the approval word.
#[must_use]
pub fn final_gate(lower: &str) -> Option<(usize, usize)> {
    let tokens = tokens(lower);
    for k in 0..tokens.len() {
        if !is_approval(&tokens, k) {
            continue;
        }
        let from = k.saturating_sub(4);
        // `after`/`once`/`if` … approval: the effect before the connector is gated.
        if let Some(c) = (from..k).find(|j| AFTER.contains(&tokens[*j].word)) {
            let mut start = tokens[c].start;
            if c > 0 && ONLY.contains(&tokens[c - 1].word) {
                start = tokens[c - 1].start;
            }
            return Some((start, tokens[k].end));
        }
        // subject … needs/must/requires … approval: the subject is what is gated.
        if let Some(r) = (from..k).find(|j| REQUIRE.contains(&tokens[*j].word)) {
            let subject_from = r.saturating_sub(3);
            return Some((tokens[subject_from].start, tokens[k].end));
        }
        // approval … required/needed/first: the requirement follows the noun.
        let to = (k + 3).min(tokens.len());
        if let Some(r) =
            (k + 1..to).find(|j| REQUIRE.contains(&tokens[*j].word) || tokens[*j].word == "first")
        {
            let subject_from = k.saturating_sub(2);
            return Some((tokens[subject_from].start, tokens[r].end));
        }
    }
    None
}

/// Waiver openers, folded: a request that says not to ask (« non serve chiedermi conferma »,
/// « no need to ask me », « sans me demander ») states that no gate is wanted. A waiver is
/// never a gate; beside a contrary prohibition it is a bypass, and that refusal is the
/// compiler's judgement, not the reader's.
const WAIVERS: &[&str] = &[
    "no need to",
    "no need for",
    "needn't",
    "don't",
    "do not",
    "without asking",
    "without checking",
    "pas besoin de",
    "pas la peine de",
    "inutile de",
    "sans me",
    "sans demander",
    "ne me demande pas",
    "ne me demandez pas",
    "no hace falta",
    "no necesitas",
    "no es necesario",
    "sin preguntarme",
    "sin pedirme",
    "no me preguntes",
    "no me pidas",
    "non serve",
    "non c'è bisogno di",
    "non c'e bisogno di",
    "non occorre",
    "senza chiedermi",
    "senza chiedere",
    "non chiedermi",
    "nicht nötig",
    "nicht notwendig",
    "musst mich nicht",
    "ohne mich zu fragen",
    "ohne nachzufragen",
    "frag mich nicht",
    "não precisa",
    "nao precisa",
    "não é preciso",
    "nao e preciso",
    "sem me perguntar",
    "sem perguntar",
    "não me pergunte",
    "nao me pergunte",
];

/// The asking a waiver waives, in the forms a waiver takes (an infinitive, a clitic form):
/// « me demander », « preguntarme », « chiedermi », « mich fragen », « me perguntar ».
const WAIVED_ASKING: &[&str] = &[
    "ask",
    "asking",
    "check",
    "confirm",
    "demander",
    "prévenir",
    "prevenir",
    "confirmer",
    "preguntar",
    "preguntarme",
    "pedir",
    "pedirme",
    "consultar",
    "consultarme",
    "chiedere",
    "chiedermi",
    "chiedimi",
    "confermare",
    "fragen",
    "nachfragen",
    "rückfragen",
    "perguntar",
    "perguntar-me",
    "confirmar",
];

/// Negations that flip a waiver into the gate it denies waiving (« mais pas sans me
/// demander », « but not without asking me »), folded.
const WAIVER_NEGATIONS: &[&str] = &[
    "not", "never", "pas", "jamais", "no", "non", "nunca", "mai", "nicht", "nie", "niemals", "não",
    "nao",
];

/// A waiver clause and its polarity: `Some(true)` waives the asking (« non serve chiedermi
/// conferma », « no need to ask me »), `Some(false)` is a negated waiver — the gate it denies
/// waiving (« mais pas sans me demander ») — and `None` is neither.
#[must_use]
pub fn waiver_polarity(lower: &str) -> Option<bool> {
    let tokens = tokens(lower);
    for opener in WAIVERS {
        for (at, _) in lower.match_indices(opener) {
            let end = at + opener.len();
            let bounded = !lower[..at].ends_with(|c: char| c.is_alphanumeric())
                && !lower[end..].starts_with(|c: char| c.is_alphanumeric());
            if !bounded {
                continue;
            }
            // « without asking », « sin preguntarme »: the opener may carry the asking itself.
            let opener_asks = opener
                .split_whitespace()
                .any(|w| asking(w) || WAIVED_ASKING.contains(&w));
            let asks = tokens
                .iter()
                .enumerate()
                .filter(|(_, t)| t.start >= end)
                .take(6)
                .any(|(k, t)| {
                    asking(t.word) || WAIVED_ASKING.contains(&t.word) || is_approval(&tokens, k)
                });
            if !(opener_asks || asks) {
                continue;
            }
            let negated = tokens
                .iter()
                .filter(|t| t.end <= at)
                .rev()
                .take(2)
                .any(|t| WAIVER_NEGATIONS.contains(&t.word.trim_matches(',')));
            return Some(!negated);
        }
    }
    None
}

/// A clause that waives the asking, not negated.
#[must_use]
pub fn waiver(lower: &str) -> bool {
    waiver_polarity(lower) == Some(true)
}

/// A gate that asks a person: `ask me to confirm before writing …`, `wait for my confirmation`,
/// `demande mon accord avant d'écrire`. Returns the span of the asking phrase; what follows
/// the span names the gated effect when a `before` connector closes the phrase.
#[must_use]
pub fn named_gate(lower: &str) -> Option<(usize, usize)> {
    let tokens = tokens(lower);
    for (a, token) in tokens.iter().enumerate() {
        if !asking(token.word) {
            continue;
        }
        let to = (a + 7).min(tokens.len());
        let approval = (a + 1..to).find(|k| is_approval(&tokens, *k));
        let before = (a + 1..to).find(|k| UNTIL.contains(&tokens[*k].word));
        let addressed = clitic(token.word).is_some();
        match (approval, before) {
            // `ask me to confirm before writing …` / `ask me before writing …` /
            // `demande-moi avant d'écrire` (a person is asked)
            (_, Some(b))
                if addressed
                    || tokens[a + 1..b].iter().any(|t| PERSONS.contains(&t.word))
                    || approval.is_some() =>
            {
                // `avant de` / `avant d'écrire`: the preposition belongs to the connector.
                let end = tokens.get(b + 1).map_or(tokens[b].end, |t| {
                    if matches!(t.word, "de" | "di") {
                        t.end
                    } else if t.word.starts_with("d'") {
                        t.start + 2
                    } else {
                        tokens[b].end
                    }
                });
                return Some((token.start, end));
            }
            // `wait for my confirmation`, `ask me to approve it`: gate what follows or the last effect
            (Some(k), None) => return Some((token.start, tokens[k].end)),
            _ => {}
        }
    }
    None
}

/// How many approval phrases a text states, left to right without overlap: one phrase is one
/// gate however many effects it covers; two phrases in two sentences are two gates.
#[must_use]
pub fn gate_phrases(lower: &str) -> usize {
    let mut count = 0;
    let mut from = 0;
    while from < lower.len() {
        let Some(rest) = lower.get(from..) else {
            break;
        };
        let next = [final_gate(rest), named_gate(rest)]
            .into_iter()
            .flatten()
            .min_by_key(|(start, _)| *start);
        match next {
            Some((start, end)) => {
                count += 1;
                from += end.max(start + 1);
            }
            None => break,
        }
    }
    count
}

/// A prohibition bounded by an approval is a gate, not a prohibition: `don't write until i
/// approve`, `never publish without my approval`, `ne publie rien sans ma validation`.
pub(crate) fn approval_bound(lower: &str) -> bool {
    let tokens = tokens(lower);
    (0..tokens.len()).any(|u| {
        UNTIL.contains(&tokens[u].word)
            && (u + 1..(u + 5).min(tokens.len())).any(|k| is_approval(&tokens, k))
    })
}

/// Words that open a prohibition in the languages the compiler meets.
const PROHIBITION_CUES: &[&str] = &[
    "do not ", "don't ", "never ", "ne ", "n'", "no ", "non ", "nicht ", "keine ", "sans ",
    "jamais ", "nunca ", "mai ", "niemals ",
];

/// A prohibition ("Do not copy …", "Ne cite pas …", "No copies …") at the head of an excerpt.
#[must_use]
pub fn starts_with_prohibition(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    PROHIBITION_CUES.iter().any(|cue| lower.starts_with(cue))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, found: Option<(usize, usize)>) -> &str {
        found.map_or("", |(s, e)| &text[s..e])
    }

    #[test]
    fn a_waiver_says_not_to_ask_and_is_no_gate() {
        for text in [
            "non serve chiedermi conferma.",
            "no need to ask me first",
            "envoie-le automatiquement, sans me demander",
            "no hace falta pedirme confirmación",
            "musst mich nicht um erlaubnis fragen",
            "não precisa me perguntar antes",
        ] {
            assert!(waiver(text), "{text}");
        }
        for text in [
            "demandez-moi confirmation avant tout envoi",
            "il ne faut jamais rien envoyer sans mon accord explicite",
            "ask me before sending",
            "sans me laisser le temps de lire",
        ] {
            assert!(!waiver(text), "{text}");
        }
        // A negated waiver is the gate it denies waiving.
        for text in [
            "mais pas sans me demander",
            "but not without asking me",
            "pero no sin preguntarme",
        ] {
            assert_eq!(waiver_polarity(text), Some(false), "{text}");
            assert!(!waiver(text), "{text}");
        }
        assert_eq!(waiver_polarity("non serve chiedermi conferma."), Some(true));
        assert_eq!(waiver_polarity("écris-le dans ./final.md"), None);
    }

    #[test]
    fn a_final_gate_is_read_from_its_shape_not_its_wording() {
        let t = "publish it to ./announce.md only after my explicit approval";
        assert_eq!(span(t, final_gate(t)), "only after my explicit approval");
        let t = "write it to ./final.md only after i approve";
        assert_eq!(span(t, final_gate(t)), "only after i approve");
        let t = "a human must approve the write first";
        assert_eq!(span(t, final_gate(t)), "a human must approve");
        let t = "the write needs my approval first";
        assert_eq!(span(t, final_gate(t)), "the write needs my approval");
        let t = "writing requires a human approval";
        assert_eq!(span(t, final_gate(t)), "writing requires a human approval");
        let t = "human approval required before the write";
        assert_eq!(span(t, final_gate(t)), "human approval required");
        let t = "publish it to ./final.md only if i explicitly say yes";
        assert_eq!(span(t, final_gate(t)), "only if i explicitly say yes");
        let t = "après validation humaine";
        assert_eq!(span(t, final_gate(t)), "après validation");
    }

    #[test]
    fn an_agents_own_verb_is_not_a_human_gate() {
        assert_eq!(
            final_gate("classify the request and approve the refund"),
            None
        );
        assert_eq!(final_gate("validate the records against the schema"), None);
        assert_eq!(final_gate("write it to ./final.md automatically"), None);
        assert_eq!(
            final_gate("read ./notes/brief.md and write a 3-bullet summary to ./out/summary.md"),
            None
        );
    }

    #[test]
    fn a_named_gate_asks_a_person_and_may_name_its_effect() {
        let t = "ask me to confirm before writing it to ./final.md";
        assert_eq!(span(t, named_gate(t)), "ask me to confirm before");
        let t = "ask me before writing";
        assert_eq!(span(t, named_gate(t)), "ask me before");
        let t = "wait for my confirmation";
        assert_eq!(span(t, named_gate(t)), "wait for my confirmation");
        let t = "ask me to approve it";
        assert_eq!(span(t, named_gate(t)), "ask me to approve");
        let t = "attends ma validation avant d'écrire dans ./final.md";
        assert_eq!(span(t, named_gate(t)), "attends ma validation avant d'");
        assert_eq!(named_gate("ask the api for the current rate"), None);
        assert_eq!(named_gate("wait 30 seconds then retry"), None);
    }

    #[test]
    fn a_gate_is_read_in_six_languages_including_a_clitic_person() {
        let t = "mais demande-moi confirmation avant d'écrire";
        assert_eq!(span(t, named_gate(t)), "demande-moi confirmation avant d'");
        let t = "mais demande-moi avant d'envoyer";
        assert_eq!(span(t, named_gate(t)), "demande-moi avant d'");
        let t = "préviens-moi avant d'envoyer le mail";
        assert_eq!(span(t, named_gate(t)), "préviens-moi avant d'");
        let t = "pídeme confirmación antes de escribir";
        assert_eq!(span(t, named_gate(t)), "pídeme confirmación antes de");
        let t = "chiedimi conferma prima di scrivere";
        assert_eq!(span(t, named_gate(t)), "chiedimi conferma prima di");
        let t = "frag mich bevor du schreibst";
        assert_eq!(span(t, named_gate(t)), "frag mich bevor");
        let t = "pergunte-me antes de enviar";
        assert_eq!(span(t, named_gate(t)), "pergunte-me antes de");
        let t = "escríbelo solo después de mi aprobación";
        assert_eq!(span(t, final_gate(t)), "solo después de mi aprobación");
        let t = "scrivilo solo dopo la mia approvazione";
        assert_eq!(span(t, final_gate(t)), "solo dopo la mia approvazione");
        let t = "schreib es nur nach meiner genehmigung";
        assert_eq!(span(t, final_gate(t)), "nur nach meiner genehmigung");
        let t = "escreva só depois da minha aprovação";
        assert_eq!(span(t, final_gate(t)), "só depois da minha aprovação");
        assert!(approval_bound("no envíes nada sin mi aprobación"));
        assert!(approval_bound("nichts senden ohne meine freigabe"));
        // A verb that merely resembles a clitic is not one.
        assert_eq!(named_gate("resume-le avant midi"), None);
    }

    #[test]
    fn gate_phrases_are_counted_left_to_right_without_overlap() {
        assert_eq!(
            gate_phrases("read ./draft.md and write it to ./final.md"),
            0
        );
        assert_eq!(
            gate_phrases(
                "ask me to confirm before you post it. only after i say yes: do the post, then write it."
            ),
            2
        );
        assert_eq!(
            gate_phrases("ask me before writing it to ./a.md. ask me again before sending it."),
            2
        );
        assert_eq!(
            gate_phrases("write it to ./final.md only after my approval"),
            1
        );
    }

    #[test]
    fn a_prohibition_bounded_by_an_approval_is_a_gate() {
        assert!(approval_bound("write it to ./final.md until i approve"));
        assert!(approval_bound("publish anything without my approval"));
        assert!(approval_bound("publie rien sans ma validation"));
        assert!(!approval_bound("copy more than 10 consecutive words"));
        assert!(!approval_bound("write before noon"));
    }
}
