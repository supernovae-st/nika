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
];
/// Connectors that bind an effect to a later approval.
const AFTER: &[&str] = &[
    "after", "once", "upon", "if", "when", "après", "apres", "lorsque", "quand", "si",
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
];
/// Asking verbs: a request for approval addressed to a person.
const ASK: &[&str] = &[
    "ask", "asks", "get", "obtain", "require", "wait", "await", "check", "demande", "demandez",
    "demander", "attends", "attendez", "attendre", "obtiens", "obtenez",
];
/// Connectors that bound a prohibition or an asking verb by an approval: `until`, `before`.
const UNTIL: &[&str] = &[
    "until", "unless", "without", "before", "till", "sans", "avant", "jusqu'à", "jusqu'a", "tant",
];

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
pub(crate) fn final_gate(lower: &str) -> Option<(usize, usize)> {
    let tokens = tokens(lower);
    for k in 0..tokens.len() {
        if !is_approval(&tokens, k) {
            continue;
        }
        let from = k.saturating_sub(4);
        // `after`/`once`/`if` … approval: the effect before the connector is gated.
        if let Some(c) = (from..k).find(|j| AFTER.contains(&tokens[*j].word)) {
            let mut start = tokens[c].start;
            if c > 0 && tokens[c - 1].word == "only" {
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

/// A gate that asks a person: `ask me to confirm before writing …`, `wait for my confirmation`,
/// `demande mon accord avant d'écrire`. Returns the span of the asking phrase; what follows
/// the span names the gated effect when a `before` connector closes the phrase.
pub(crate) fn named_gate(lower: &str) -> Option<(usize, usize)> {
    let tokens = tokens(lower);
    for (a, token) in tokens.iter().enumerate() {
        if !ASK.contains(&token.word) {
            continue;
        }
        let to = (a + 7).min(tokens.len());
        let approval = (a + 1..to).find(|k| is_approval(&tokens, *k));
        let before = (a + 1..to).find(|k| UNTIL.contains(&tokens[*k].word));
        match (approval, before) {
            // `ask me to confirm before writing …` / `ask me before writing …` (a person is asked)
            (_, Some(b))
                if tokens[a + 1..b].iter().any(|t| PERSONS.contains(&t.word))
                    || approval.is_some() =>
            {
                // `avant de` / `avant d'écrire`: the preposition belongs to the connector.
                let end = tokens.get(b + 1).map_or(tokens[b].end, |t| {
                    if t.word == "de" {
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

/// A prohibition bounded by an approval is a gate, not a prohibition: `don't write until i
/// approve`, `never publish without my approval`, `ne publie rien sans ma validation`.
pub(crate) fn approval_bound(lower: &str) -> bool {
    let tokens = tokens(lower);
    (0..tokens.len()).any(|u| {
        UNTIL.contains(&tokens[u].word)
            && (u + 1..(u + 5).min(tokens.len())).any(|k| is_approval(&tokens, k))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, found: Option<(usize, usize)>) -> &str {
        found.map_or("", |(s, e)| &text[s..e])
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
    fn a_prohibition_bounded_by_an_approval_is_a_gate() {
        assert!(approval_bound("write it to ./final.md until i approve"));
        assert!(approval_bound("publish anything without my approval"));
        assert!(approval_bound("publie rien sans ma validation"));
        assert!(!approval_bound("copy more than 10 consecutive words"));
        assert!(!approval_bound("write before noon"));
    }
}
