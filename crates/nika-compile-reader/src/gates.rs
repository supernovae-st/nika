//! The structure of a human gate. An approval requirement is not a phrase but a shape: a
//! connector that binds an effect to an approval (`after`, `once`, `if`, `until`, `before`),
//! a requirement word (`needs`, `must`, `requires`), or an asking verb, a few tokens naming
//! who approves, and a word that means approval. Reading the shape lets `only after my
//! explicit approval`, `a human must approve the write first` or `ask me to confirm before
//! writing` land as the same runtime gate without one phrase per wording.

/// Words that mean approval when they close a gate phrase.
use super::paths::{self, PathShape};
use super::plan::{EffectVerb, Plan};

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
const ASK: &str = include_str!("../assets/gate_ask_verbs.txt");
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
            && ASK.lines().any(|a| a == verb)
        {
            return Some((verb, suffix));
        }
    }
    None
}

/// The asking verb a token carries, its clitic person set aside.
fn asking(word: &str) -> bool {
    ASK.lines().any(|a| a == word)
        || clitic(word).is_some_and(|(verb, _)| ASK.lines().any(|a| a == verb))
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
const WAIVERS: &str = include_str!("../assets/gate_waivers.txt");

/// The asking a waiver waives, in the forms a waiver takes (an infinitive, a clitic form):
/// « me demander », « preguntarme », « chiedermi », « mich fragen », « me perguntar ».
const WAIVED_ASKING: &str = include_str!("../assets/gate_waived_asking.txt");

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
    // A waiver inside quotes (« 'no need to ask me' ») is content: it waives nothing.
    let unquoted = crate::lexicon::unquoted(lower);
    let lower = unquoted.as_str();
    let tokens = tokens(lower);
    for opener in WAIVERS.lines() {
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
                .any(|w| asking(w) || WAIVED_ASKING.lines().any(|a| a == w));
            let asks = tokens
                .iter()
                .enumerate()
                .filter(|(_, t)| t.start >= end)
                .take(6)
                .any(|(k, t)| {
                    asking(t.word)
                        || WAIVED_ASKING.lines().any(|a| a == t.word)
                        || is_approval(&tokens, k)
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
    let unquoted = crate::lexicon::unquoted(lower);
    let tokens = tokens(&unquoted);
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

/// Recognized EN/FR approval-bypass phrases, matched as whole-word sequences.
const APPROVAL_BYPASS: &[&[&str]] = &[
    &["without", "approval"],
    &["without", "asking"],
    &["do", "not", "ask"],
    &["sans", "mon", "accord"],
    &["sans", "accord"],
    &["ne", "pas", "demander"],
    &["approved", "yesterday"],
    &["approval", "from", "yesterday"],
    &["yesterday", "s", "approval"],
    &["validé", "hier"],
    &["validée", "hier"],
    &["approuvé", "hier"],
    &["approuvée", "hier"],
    &["accord", "d", "hier"],
    &["accord", "hier"],
    &["prior", "approval"],
    &["previous", "approval"],
];

/// Negations and prohibitions in six languages: before a bypass phrase in the same
/// sentence, they turn it into a gate.
const NEGATIONS: &str = include_str!("../assets/gate_negations.txt");

/// Whether a recognized bypass phrase is stated as a bypass. The same words inside a
/// prohibition state a gate: « rien ne doit partir sans mon accord », « never send without
/// asking » forbid the effect until the approval, they do not skip it. The negation must
/// precede the phrase in its own sentence; « envoie-le sans mon accord, ne me demande rien »
/// stays a bypass.
#[must_use]
pub fn bypass_stated(lower: &str) -> bool {
    let unquoted = crate::lexicon::unquoted(lower);
    crate::lexicon::split_sentences(&unquoted)
        .into_iter()
        .any(|sentence| {
            let words: Vec<&str> = sentence
                .split(|c: char| !c.is_alphabetic())
                .filter(|w| !w.is_empty())
                .collect();
            APPROVAL_BYPASS.iter().any(|phrase| {
                words.windows(phrase.len()).enumerate().any(|(at, window)| {
                    window == *phrase
                        && !words[..at]
                            .iter()
                            .any(|w| NEGATIONS.lines().any(|n| n == *w))
                })
            })
        })
}

/// Whether the request's own words name a refund, outside the data a typed element of the
/// plan already owns. A bound `./` file or glob is a local name
/// (`./out/refund-tickets.json` asks nothing), and the literal a typed rule compares
/// is a value (`refunded` in « whose status is refunded »), where the rule's excerpt states it
/// once. Absolute paths (possibly endpoints), directories, unowned or repeated literals and
/// excerpts absent from the request remain guarded.
fn names_refund(text: &str, plan: &Plan) -> bool {
    let mut text = text.to_owned();
    for rule in &plan.rules {
        let excerpt = rule.text().to_lowercase();
        if excerpt.trim().is_empty() {
            continue;
        }
        let Some(from) = text.find(excerpt.as_str()) else {
            continue;
        };
        for (_, literal) in rule.text_equalities() {
            let literal = literal.to_lowercase();
            if let [at] = whole_word(&excerpt, &literal).as_slice() {
                let start = from + at;
                text.replace_range(start..start + literal.len(), &" ".repeat(literal.len()));
            }
        }
    }
    for binding in plan
        .bindings
        .iter()
        .filter(|b| b.role == "path" && b.literal.starts_with("./"))
    {
        if matches!(
            paths::token(&binding.literal),
            Some(PathShape::File(_) | PathShape::Glob(_))
        ) {
            text = text.replace(&binding.literal.to_lowercase(), " ");
        }
    }
    text.contains("refund") || text.contains("rembours")
}

/// Where `word` stands whole in `text`: no letter or digit runs into it on either side.
fn whole_word(text: &str, word: &str) -> Vec<usize> {
    if word.is_empty() {
        return Vec::new();
    }
    text.match_indices(word)
        .filter(|(at, _)| {
            let before = text[..*at].chars().next_back();
            let after = text[at + word.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
        .map(|(at, _)| at)
        .collect()
}

/// A conservative EN/FR authority backstop applied to EVERY strategy. It cannot prove
/// arbitrary-language intent preservation (the proposal's own bypass field covers other
/// languages); it refuses the recognized bypasses and keeps recognized money movement from
/// being assembled without a human gate. A refund word only inside data the plan owns (a
/// bound `./` file path, a typed rule's compared value) names no refund (`names_refund`).
pub fn backstop(intent: &str, plan: &mut Plan) {
    let text = crate::lexicon::unquoted(&intent.to_lowercase());
    if bypass_stated(&text) {
        plan.unknowns.push(
            "The request reuses, skips or presupposes an approval (recognized approval-bypass wording); the compiler never grants that authority."
                .to_owned(),
        );
    }
    if names_refund(&text, plan) && !plan.effects.iter().any(|e| e.verb == EffectVerb::Refund) {
        plan.unknowns.push(
            "The request mentions a refund that no recognized effect carries; a refund is never dropped silently."
                .to_owned(),
        );
    }
    // An automatic money movement is not unknown work: the assembler asks its approval
    // as one closed choice (`effect.<verb>.approval`).
    plan.unknowns.dedup();
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

    /// A tag filter whose output is named after the tag: `refund` is a ticket tag and a word of
    /// the output's name, never a money movement.
    const TAG_FILTER: &str = "Look in ./tickets.json and keep only the tickets labelled `refund` that were opened in September 2026. Save ./out/refund-tickets.json as an object with `count` (how many tickets) and `ids` (their ids, sorted).";

    const REFUND_GUARD: &str = "The request mentions a refund";

    fn refund_guarded(plan: &Plan) -> bool {
        plan.unknowns.iter().any(|u| u.starts_with(REFUND_GUARD))
    }

    /// A plan owning only the given path bindings and, when given, the typed rule of `rule`.
    fn owned(paths: &[&str], rule: Option<&str>) -> Plan {
        let mut plan = Plan::default();
        plan.bindings = (paths.iter())
            .map(|p| crate::plan::Binding::new("path", *p))
            .collect();
        plan.rules = (rule.into_iter())
            .map(|r| crate::rules::synthesize(r, &[]).expect("a typed rule"))
            .collect();
        plan
    }

    fn guarded(intent: &str, mut plan: Plan) -> bool {
        backstop(intent, &mut plan);
        refund_guarded(&plan)
    }

    #[test]
    fn a_refund_named_only_as_data_owes_no_refund() {
        // The reader's own plan of the request: no money effect, no refund unknown.
        let mut plan = crate::lexicon::read(TAG_FILTER).plan;
        backstop(TAG_FILTER, &mut plan);
        assert!(
            !plan.effects.iter().any(|e| e.verb.moves_money()),
            "{plan:?}"
        );
        assert!(!refund_guarded(&plan), "{:?}", plan.unknowns);
        // The masking alone: a bound file and a quoted tag are data.
        let paths = ["./tickets.json", "./out/refund-tickets.json"];
        assert!(!guarded(TAG_FILTER, owned(&paths, None)));
        let quoted = "Read ./tickets.json and keep only the tickets tagged \"refund\"";
        assert!(!guarded(quoted, owned(&["./tickets.json"], None)));
        // A typed rule's compared value is data: `refunded` is a status, not a request.
        let intent = "Read ./orders.json, keep only the orders whose status is refunded, and write them to ./out/refunded-orders.json";
        let rule = "keep only the orders whose status is refunded";
        let plan = owned(&["./orders.json", "./out/refunded-orders.json"], Some(rule));
        assert!(!guarded(intent, plan));
    }

    #[test]
    fn a_requested_refund_is_carried_or_named_whatever_data_surrounds_it() {
        // Through the reader: either a refund effect carries the request or the guard names it.
        for intent in [
            "Read ./refunds.csv and refund the customer's last order, then save the receipt to ./out/refund-receipt.md",
            "Rembourse la dernière commande du client et écris le reçu dans ./out/remboursement.md",
            "Read ./orders.json, keep only the orders whose status is refunded, then refund the remaining customers",
        ] {
            let mut plan = crate::lexicon::read(intent).plan;
            backstop(intent, &mut plan);
            let carried = plan.effects.iter().any(|e| e.verb == EffectVerb::Refund);
            assert!(carried || refund_guarded(&plan), "{intent}: {plan:?}");
        }
        // With no refund effect, the data the plan owns never hides the asked refund.
        let rule = "keep only the orders whose status is refunded";
        for (intent, plan) in [
            (
                "refund the customer's last order and save the receipt to ./out/refund-receipt.md",
                owned(&["./out/refund-receipt.md"], None),
            ),
            (
                "Read ./refunds.csv and refund each customer it lists",
                owned(&["./refunds.csv"], None),
            ),
            (
                "Read ./orders.json, keep only the orders whose status is refunded, then refund them",
                owned(&["./orders.json"], Some(rule)),
            ),
        ] {
            assert!(guarded(intent, plan), "{intent}");
        }
    }

    #[test]
    fn a_refund_word_no_typed_element_owns_keeps_the_guard() {
        // Fail closed: an unquoted tag no typed rule reads, a directory and an unbound file
        // name stay words; quoting the value is the stated way to make it data.
        for (intent, plan) in [
            (
                "Read ./tickets.json and keep only the tickets tagged refund",
                owned(&["./tickets.json"], None),
            ),
            (
                "Summarize every file under ./refunds/ into ./out/summary.md",
                owned(&["./refunds/", "./out/summary.md"], None),
            ),
            (
                "Save the tickets as ./out/refund-tickets.json",
                owned(&[], None),
            ),
        ] {
            assert!(guarded(intent, plan), "{intent}");
        }
    }

    #[test]
    fn a_path_binding_cannot_hide_a_refund_endpoint_or_a_later_refund() {
        // A lexical path binding does not prove that an absolute path is a local file.
        for (intent, path) in [
            ("POST /api/refund.json for order 123", "/api/refund.json"),
            (
                "Appelle /api/remboursement.json pour la commande 123",
                "/api/remboursement.json",
            ),
            (
                "Save ./out/refund-tickets.json, then refund the customer",
                "./out/refund-tickets.json",
            ),
            (
                "Écris ./out/remboursement.json, puis rembourse le client",
                "./out/remboursement.json",
            ),
        ] {
            assert!(guarded(intent, owned(&[path], None)), "{intent}");
        }
    }
}
