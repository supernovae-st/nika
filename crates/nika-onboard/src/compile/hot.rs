// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Strict HOT admission beyond the reader's own rejections: positive evidence that every
//! operation cue of the request produced an element, that no draft is a bare path, that
//! every effect has a producer for the content it names, and that a revision check has a
//! source to reread. These are laws of the reader's own vocabulary (its cue table and its
//! plan), never rules about a corpus: a cue the reader knows but did not turn into a step
//! is exactly the case where "consumed" was not "understood", and the honest answer is to
//! escalate. The composer applies the same producer laws to every proposal.

use super::lexicon::{self, Head, Reading};
use super::plan::{EffectPolicy, EffectVerb, ObligationKind, Op, Plan};

/// Why a reading may not be admitted as HOT, in addition to [`Reading::hot_rejections`].
pub(super) fn rejections(intent: &str, reading: &Reading) -> Vec<String> {
    let mut why = Vec::new();
    let lower = lexicon::fold_apostrophes(intent).to_lowercase();
    cue_coverage(&lower, reading, &mut why);
    path_as_draft(reading, &mut why);
    unproduced_content(&reading.plan, &mut why);
    unrecheckable_revision(&reading.plan, &mut why);
    why
}

/// Every cue of the reader's table that occurs in the request must correspond to an element
/// of the reading (a step of that operation, an effect of that verb, an obligation, a recorded
/// ambiguity) or lie inside a clause the reader already reports as unresolved.
fn cue_coverage(lower: &str, reading: &Reading, why: &mut Vec<String>) {
    let reported: Vec<String> = reading
        .unresolved
        .iter()
        .chain(reading.ambiguous.iter().map(|a| &a.clause))
        .chain(reading.soft_constraints.iter())
        .map(|clause| clause.to_lowercase())
        .collect();
    let mut seen: Vec<&'static str> = Vec::new();
    let mut prev: Option<char> = None;
    for (index, ch) in lower.char_indices() {
        let boundary = prev
            .is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '"' | '\'' | ',' | ';' | ':'));
        prev = Some(ch);
        if !boundary || !ch.is_alphabetic() {
            continue;
        }
        let Some((phrase, head)) = lexicon::head_of_exact(&lower[index..]) else {
            continue;
        };
        if seen.contains(&phrase) {
            continue;
        }
        let inside_reported = reported.iter().any(|clause| {
            lower
                .find(clause.as_str())
                .is_some_and(|start| start <= index && index < start + clause.len())
        });
        if inside_reported || satisfied(head, reading) {
            continue;
        }
        seen.push(phrase);
        why.push(format!("cue `{phrase}` produced no element"));
    }
}

fn satisfied(head: &Head, reading: &Reading) -> bool {
    let plan = &reading.plan;
    match head {
        Head::Op(op) => {
            covers(reading, *op) || reading.ambiguous.iter().any(|a| a.options.contains(op))
        }
        Head::Choice(options) => {
            options.iter().any(|op| covers(reading, *op))
                || reading
                    .ambiguous
                    .iter()
                    .any(|a| a.options.iter().any(|op| options.contains(op)))
        }
        Head::Effect(verb) => plan.effects.iter().any(|e| e.verb == *verb),
        Head::Dedup => plan
            .obligations
            .iter()
            .any(|o| matches!(o.kind, ObligationKind::Dedup)),
    }
}

/// The element a cue may legitimately have become: its own operation, the fetch a read or
/// lookup turns into on a URL, or the write effect a draft cue turns into on a path.
fn covers(reading: &Reading, op: Op) -> bool {
    let plan = &reading.plan;
    plan.has(op)
        || (matches!(op, Op::Read | Op::Lookup) && plan.has(Op::Fetch))
        || (op == Op::Draft && plan.effects.iter().any(|e| e.verb == EffectVerb::Write))
}

/// A draft whose object is a bare path is a write with no content, not a composition.
fn path_as_draft(reading: &Reading, why: &mut Vec<String>) {
    for step in &reading.plan.steps {
        if step.op == Op::Draft && looks_like_path(step.detail.trim()) {
            why.push(format!(
                "`draft` object is a path, a write with no content: {}",
                step.detail.trim()
            ));
        }
    }
}

fn looks_like_path(token: &str) -> bool {
    let token = token.trim_end_matches(['.', ',', ';']);
    !token.is_empty()
        && !token.contains(char::is_whitespace)
        && (token.starts_with("./")
            || token.starts_with("~/")
            || (token.starts_with('/') && token.contains('.'))
            || token.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty() && ext.len() <= 5 && ext.chars().all(char::is_alphanumeric)
            }))
}

const WRITE_HEADS: &[&str] = &[
    "write",
    "writes",
    "écris",
    "ecris",
    "écrire",
    "ecrire",
    "enregistre",
    "sauvegarde",
    "save",
    "store",
    "persist",
    "escreve",
    "escrever",
    "escribe",
    "escribir",
    "guarda",
    "guardar",
    "scrivi",
    "scrivere",
    "schreibe",
    "schreib",
    "speichere",
    "speichern",
];
/// Words that only link a write to its target; never content.
const LINK_WORDS: &[&str] = &[
    "to",
    "into",
    "in",
    "at",
    "dans",
    "vers",
    "sous",
    "em",
    "en",
    "nel",
    "nella",
    "auf",
    "nach",
    "a",
    "the",
    "le",
    "la",
    "les",
    "o",
    "os",
    "as",
    "il",
    "el",
    "der",
    "die",
    "das",
    "it",
    "them",
    "result",
    "résultat",
    "resultado",
    "output",
    "file",
    "fichier",
    "ficheiro",
    "archivo",
];

/// Nouns that name content a step must produce before an effect can carry it (EN · FR ·
/// ES · IT · PT · DE), in their diacritic-folded lowercase form.
const PRODUCED_NOUNS: &[&str] = &[
    "reply",
    "replies",
    "report",
    "reports",
    "summary",
    "summaries",
    "digest",
    "digests",
    "brief",
    "briefs",
    "note",
    "notes",
    "message",
    "messages",
    "blurb",
    "blurbs",
    "draft",
    "drafts",
    "translation",
    "translations",
    "recap",
    "recaps",
    "memo",
    "memos",
    "answer",
    "response",
    "reponse",
    "reponses",
    "rapport",
    "rapports",
    "resume",
    "resumes",
    "synthese",
    "syntheses",
    "brouillon",
    "brouillons",
    "traduction",
    "traductions",
    "recapitulatif",
    "respuesta",
    "respuestas",
    "informe",
    "informes",
    "resumen",
    "resumenes",
    "mensaje",
    "mensajes",
    "borrador",
    "borradores",
    "traduccion",
    "traducciones",
    "nota",
    "notas",
    "risposta",
    "risposte",
    "rapporto",
    "rapporti",
    "riassunto",
    "riassunti",
    "messaggio",
    "messaggi",
    "bozza",
    "bozze",
    "traduzione",
    "traduzioni",
    "resposta",
    "respostas",
    "relatorio",
    "relatorios",
    "resumo",
    "resumos",
    "mensagem",
    "mensagens",
    "rascunho",
    "rascunhos",
    "traducao",
    "traducoes",
    "antwort",
    "antworten",
    "bericht",
    "berichte",
    "zusammenfassung",
    "zusammenfassungen",
    "nachricht",
    "nachrichten",
    "entwurf",
    "entwurfe",
    "ubersetzung",
    "ubersetzungen",
    "notiz",
    "notizen",
];

/// Cues that an effect carries existing material unchanged: a source step is then its
/// producer. Matched as whole words or whole phrases on the folded text.
const COPY_CUES: &[&str] = &[
    "copy",
    "copies",
    "forward",
    "forwards",
    "verbatim",
    "tel quel",
    "telle quelle",
    "as is",
    "as-is",
    "unchanged",
    "attach",
    "attached",
    "attachment",
    "piece jointe",
    "ci-joint",
    "ci-jointe",
    "transmets",
    "transmet",
    "transmettre",
    "transfere",
    "transferer",
    "sans modification",
    "reenvia",
    "reenviar",
    "inoltra",
    "inoltrare",
    "weiterleiten",
    "raw",
];

/// Lowercase with French, Spanish, Portuguese and German diacritics folded, so the
/// noun and cue tables match one spelling.
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ñ' => 'n',
            'ß' => 's',
            other => other,
        })
        .collect()
}

/// The first produced-content noun of a phrase, as the phrase spells it.
fn produced_noun(text: &str) -> Option<String> {
    let folded = fold(text);
    let words = text
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|w| !w.is_empty());
    let folded_words = folded
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|w| !w.is_empty());
    words
        .zip(folded_words)
        .find(|(_, folded)| PRODUCED_NOUNS.contains(folded))
        .map(|(word, _)| word.to_owned())
}

fn copy_cue(text: &str) -> bool {
    let padded = format!(
        " {} ",
        fold(text).replace(|c: char| !c.is_alphanumeric() && c != '-', " ")
    );
    COPY_CUES
        .iter()
        .any(|cue| padded.contains(&format!(" {cue} ")))
}

/// Every effect that names content needs a producer: a write with named content (below),
/// and a send, publish, notify, create, update or other effect whose target or evidence
/// names a produced-content noun (a reply, a report, a summary…) needs a draft, an extract
/// or a compute step, or, under a copy cue (forward, verbatim, tel quel, attach…), a source
/// step whose material it carries unchanged. A prohibited or contradictory effect is never
/// emitted, so it needs nothing; a target naming a local file is a write and follows the
/// write law.
pub(super) fn unproduced_content(plan: &Plan, why: &mut Vec<String>) {
    write_without_producer(plan, why);
    let produces = plan.has(Op::Draft) || plan.has(Op::Extract) || plan.has(Op::Compute);
    if produces {
        return;
    }
    let sourced = plan
        .steps
        .iter()
        .any(|s| matches!(s.op, Op::Read | Op::Fetch | Op::Lookup | Op::Search));
    for effect in plan.effects.iter().filter(|e| {
        matches!(
            e.verb,
            EffectVerb::Send
                | EffectVerb::Publish
                | EffectVerb::Notify
                | EffectVerb::Create
                | EffectVerb::Update
                | EffectVerb::Other
        ) && !matches!(e.policy, EffectPolicy::Forbidden | EffectPolicy::Conflict)
            && super::paths::single_file(&e.target).is_none()
    }) {
        let text = format!("{} {}", effect.target, effect.evidence);
        let Some(noun) = produced_noun(&text) else {
            continue;
        };
        if copy_cue(&text) && sourced {
            continue;
        }
        why.push(format!(
            "`{}` names content no step produces: {noun} ({})",
            effect.verb.word(),
            effect.target.trim()
        ));
    }
}

/// A revision check rereads the record it looked up; without a lookup there is nothing
/// retrievable to recheck.
pub(super) fn unrecheckable_revision(plan: &Plan, why: &mut Vec<String>) {
    if plan
        .obligations
        .iter()
        .any(|o| matches!(o.kind, ObligationKind::RevisionCheck))
        && !plan.has(Op::Lookup)
    {
        why.push("the obligation `revision_check` has no retrievable source to recheck".to_owned());
    }
}

/// A write effect that names content ("write a brief of under 150 words … to ./out/x.md",
/// "escreve em ./out/x.md a lista dos produtos …") needs a step that produces it; with
/// nothing drafted, extracted or computed, the content would be invented by the assembler.
/// The target path and the words that only link the write to it never count as content.
fn write_without_producer(plan: &Plan, why: &mut Vec<String>) {
    let produces = plan.has(Op::Draft) || plan.has(Op::Extract) || plan.has(Op::Compute);
    if produces {
        return;
    }
    for effect in plan.effects.iter().filter(|e| e.verb == EffectVerb::Write) {
        let evidence = effect.evidence.to_lowercase();
        let Some(after_head) = WRITE_HEADS
            .iter()
            .filter_map(|head| evidence.find(head).map(|at| &evidence[at + head.len()..]))
            .min_by_key(|rest| usize::MAX - rest.len())
        else {
            continue;
        };
        let target = effect.target.to_lowercase();
        let content = if target.trim().is_empty() {
            after_head.to_owned()
        } else {
            after_head.replace(target.trim(), " ")
        };
        let words = content
            .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'')
            .filter(|w| !w.is_empty() && !LINK_WORDS.contains(w))
            .count();
        if words >= 3 {
            why.push(format!(
                "`write` names content no step produces: {}",
                content.split_whitespace().collect::<Vec<_>>().join(" ")
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_recognized_and_prose_is_not() {
        assert!(looks_like_path("./out/summary.md"));
        assert!(looks_like_path("/tmp/x.json"));
        assert!(looks_like_path("report.md."));
        assert!(!looks_like_path("a short note to ./out/summary.md"));
        assert!(!looks_like_path("the summary"));
        assert!(!looks_like_path(""));
    }

    #[test]
    fn a_bare_path_draft_and_an_unproduced_write_are_rejected() {
        let reading = lexicon::read("Read ./a.txt and write ./b.txt");
        let why = rejections("Read ./a.txt and write ./b.txt", &reading);
        assert!(
            why.iter().any(|w| w.contains("a write with no content")),
            "{why:?}"
        );
        let intent = "Fetch https://example.com/rfc.txt and then write a plain brief of under 150 words explaining the protocol, as 5 bullets, to ./out/brief.md.";
        let reading = lexicon::read(intent);
        let why = rejections(intent, &reading);
        assert!(
            why.iter()
                .any(|w| w.contains("names content no step produces")),
            "{why:?}"
        );
        // Portuguese word order: the content follows the target.
        let intent = "Lê ./data/estoque.csv e escreve em ./out/reposicao.md a lista dos produtos cuja quantidade está abaixo do mínimo.";
        let reading = lexicon::read(intent);
        let mut why = Vec::new();
        let mut plan = reading.plan.clone();
        plan.steps.retain(|s| s.op == Op::Read);
        if !plan.effects.iter().any(|e| e.verb == EffectVerb::Write) {
            plan.effects.push(super::super::plan::Effect {
                verb: EffectVerb::Write,
                target: "./out/reposicao.md".to_owned(),
                evidence: "escreve em ./out/reposicao.md a lista dos produtos cuja quantidade está abaixo do mínimo".to_owned(),
                policy: super::super::plan::EffectPolicy::Automatic,
                policy_literal: None,
            });
        }
        write_without_producer(&plan, &mut why);
        assert!(
            why.iter()
                .any(|w| w.contains("names content no step produces")),
            "{why:?}"
        );
    }

    #[test]
    fn a_send_of_an_unproduced_reply_is_rejected_and_a_drafted_one_admitted() {
        let intent = "Read ./inbox/a.md and send a reply to ops@example.invalid";
        let reading = lexicon::read(intent);
        assert!(
            reading
                .plan
                .effects
                .iter()
                .any(|e| e.verb == EffectVerb::Send),
            "{:?}",
            reading.plan
        );
        let why = rejections(intent, &reading);
        assert!(
            why.iter().any(|w| w
                == "`send` names content no step produces: reply (a reply to ops@example.invalid)"),
            "{why:?}"
        );
        let intent = "Read ./inbox/a.md, draft a reply, and send the reply to ops@example.invalid";
        let reading = lexicon::read(intent);
        let why = rejections(intent, &reading);
        assert!(
            !why.iter().any(|w| w.contains("no step produces")),
            "{why:?}"
        );
        // Multilingual nouns: a French réponse, a Spanish informe.
        let mut plan = Plan::default();
        plan.effects.push(super::super::plan::Effect {
            verb: EffectVerb::Notify,
            target: "l'équipe avec la réponse".to_owned(),
            evidence: "notifie l'équipe avec la réponse".to_owned(),
            policy: super::super::plan::EffectPolicy::Automatic,
            policy_literal: None,
        });
        let mut why = Vec::new();
        unproduced_content(&plan, &mut why);
        assert_eq!(
            why,
            ["`notify` names content no step produces: réponse (l'équipe avec la réponse)"]
        );
        plan.effects[0].target = "el informe al cliente".to_owned();
        plan.effects[0].evidence = "publica el informe al cliente".to_owned();
        plan.effects[0].verb = EffectVerb::Publish;
        let mut why = Vec::new();
        unproduced_content(&plan, &mut why);
        assert_eq!(
            why,
            ["`publish` names content no step produces: informe (el informe al cliente)"]
        );
        // A revision check needs a lookup to reread.
        let mut plan = Plan::default();
        plan.obligations.push(super::super::plan::Obligation {
            kind: ObligationKind::RevisionCheck,
            evidence: "recheck".to_owned(),
        });
        let mut why = Vec::new();
        unrecheckable_revision(&plan, &mut why);
        assert_eq!(
            why,
            ["the obligation `revision_check` has no retrievable source to recheck"]
        );
    }

    #[test]
    fn a_url_read_then_summarize_is_admitted() {
        let intent = "Lis https://example.invalid/a puis résume le contenu";
        let reading = lexicon::read(intent);
        let why = rejections(intent, &reading);
        assert!(
            why.is_empty(),
            "{why:?} steps={:?}",
            reading
                .plan
                .steps
                .iter()
                .map(|s| (s.op.word(), s.detail.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_fully_produced_reading_has_no_extra_rejection() {
        let intent = "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md";
        let reading = lexicon::read(intent);
        assert!(
            rejections(intent, &reading).is_empty(),
            "{:?}",
            rejections(intent, &reading)
        );
    }
}
