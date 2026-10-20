// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The question document and its identity come from one Session work snapshot.
//! Unsupported shapes keep the ordinary prompt; painting grants no answer or effect.

use nika_session::{ProposalId, QuestionId, work};
use std::sync::atomic::{AtomicU64, Ordering};

// One process-local scope per native runtime incarnation. This is a painted
// token namespace only; Session still owns and validates the strong QuestionId.
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

pub(super) fn epoch() -> u64 {
    NEXT_EPOCH.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn witness(id: &QuestionId, epoch: u64) -> String {
    format!("{epoch}:{}", id.as_str())
}

pub(super) fn proposal_witness(proposal: &ProposalId, id: &QuestionId, epoch: u64) -> String {
    format!("{epoch}:proposal:{}:{}", proposal.as_str(), id.as_str())
}

use crate::model::{Asked, Committed, Kind, Offer, Retained, Shape, Waiting};

/// What the Session keeps of `request`, read beside the question `asked`:
/// its goal in the person's own words (the original request, and the latest
/// correction when one was said, never the frame the Session joined them with
/// for its author) and the questions still open other than this one (the
/// Session keeps the question it asks as open, and the surface shows it
/// already); nothing when neither remains. A question the request does not
/// keep open answers something else (an activation, a later change, a
/// conversation whose author keeps its own questions): its card names no
/// request rather than one it does not answer.
fn retained(request: &work::Request, asked: &str) -> Option<Retained> {
    if !(request.unresolved.iter()).any(|open| open.trim() == asked.trim()) {
        return None;
    }
    let open: Vec<String> = (request.unresolved.iter())
        .filter(|open| open.trim() != asked.trim())
        .cloned()
        .collect();
    let goal = request
        .said()
        .map(|(original, corrections)| match corrections.last() {
            Some(latest) => format!("{original} (correction: {latest})"),
            None => original.to_owned(),
        });
    let kept = goal.is_some() || !open.is_empty();
    kept.then(|| Retained::new(goal, open))
}

pub(super) fn capture(snapshot: &work::Work, epoch: u64) -> Waiting {
    match &snapshot.waiting {
        work::Waiting::RunReview { .. } => plain("run_cost"),
        work::Waiting::CostChoice => plain("unknown_cost"),
        work::Waiting::IntelligenceChoice => intelligence_choice(snapshot, epoch),
        work::Waiting::Consent { proposal } => {
            let Some(question) = snapshot
                .open_choices
                .iter()
                .find(|q| q.state == work::AskedState::Open)
            else {
                return Waiting::Proposal;
            };
            let mut asked = cooperative(
                question,
                epoch,
                proposal_witness(proposal, &question.id, epoch),
            );
            asked.retained = retained(&snapshot.request, &question.question);
            Waiting::proposal_choice(proposal.as_str(), &question.key, asked)
        }
        work::Waiting::Gate { .. } => Waiting::Gate,
        work::Waiting::Question { key, id } => {
            // The cooperative Session owns multiplicity and free-form permission.
            // Match the strong identity as well as the semantic key; another
            // question's options never lend a painted answer authority.
            let current = snapshot
                .questions
                .iter()
                .find(|q| q.id == *id && q.key == *key && q.state == work::AskedState::Open);
            if let Some(q) = current {
                let mut asked = cooperative(q, epoch, witness(id, epoch));
                asked.retained = retained(&snapshot.request, &q.question);
                return Waiting::asked(key.clone(), asked);
            }
            let asked = snapshot
                .question
                .as_ref()
                .filter(|q| q.key == *key)
                .and_then(|q| {
                    let shape = match q.answer_type {
                        "choice" if !q.options.is_empty() => Shape::Choice(
                            q.options
                                .iter()
                                .map(|offer| Offer::new(&offer.key, &offer.label))
                                .collect(),
                        ),
                        "text" => Shape::Text,
                        "literal" => Shape::Literal,
                        _ => return None,
                    };
                    let mut asked = Asked::new(
                        &q.label,
                        &q.why,
                        q.mandatory,
                        shape,
                        witness(id, epoch),
                        epoch,
                    );
                    asked.retained = retained(&snapshot.request, &q.label);
                    Some(asked)
                });
            asked.map_or_else(|| plain(key), |asked| Waiting::asked(key.clone(), asked))
        }
        work::Waiting::Questions { ids } => {
            let Some(question) = grouped_question(snapshot, ids) else {
                return Waiting::Free;
            };
            let mut asked = cooperative(question, epoch, witness(&question.id, epoch));
            asked.retained = retained(&snapshot.request, &question.question);
            Waiting::asked(&question.key, asked)
        }
        work::Waiting::KnowledgeChoice { line } => {
            knowledge(line, snapshot.knowledge.as_ref(), epoch)
        }
        work::Waiting::Activation { key } => plain(key),
        work::Waiting::RunCeiling { .. } => plain("run.ceiling"),
        work::Waiting::Input { .. } => plain(""),
        _ => Waiting::Free,
    }
}

fn intelligence_choice(snapshot: &work::Work, epoch: u64) -> Waiting {
    let Some(intelligence) = snapshot
        .intelligence
        .as_ref()
        .filter(|i| !i.options.is_empty())
    else {
        return Waiting::Choosing;
    };
    let mut offers = intelligence
        .options
        .iter()
        .map(|offer| Offer::new(offer.key.clone(), offer.label.clone()))
        .collect::<Vec<_>>();
    let usable = intelligence
        .selected
        .as_ref()
        .is_some_and(|selected| selected.ready && selected.kind != "none");
    if usable {
        offers.push(Offer::new("cancel", "Keep the current selection"));
    }
    // The list names no request: the Session's goal is the conversation's first one, never
    // necessarily a message this choice holds (the guide says when one waits).
    let asked = Asked::new(
        "Choose the intelligence for this conversation",
        "",
        !usable,
        Shape::Choice(offers),
        format!("{epoch}:intelligence"),
        epoch,
    );
    Waiting::selection(asked)
}

/// A grouped ask lends one open question at a time, in the owner's order.
/// Its offered labels are answers even when no workflow value is attached.
pub(super) fn grouped_question<'a>(
    snapshot: &'a work::Work,
    ids: &[QuestionId],
) -> Option<&'a work::AskedQuestion> {
    ids.iter().find_map(|id| {
        snapshot
            .questions
            .iter()
            .find(|q| q.id == *id && q.state == work::AskedState::Open)
    })
}

fn cooperative(question: &work::AskedQuestion, epoch: u64, witness: String) -> Asked {
    // The author's recommendation rides along: the surface preselects it and offers to leave
    // the choice to Nika; the answer is still the human's own `Enter`.
    let offers = question
        .options
        .iter()
        .map(|offer| {
            let shown = Offer::new(offer.key.clone(), offer.label.clone());
            if offer.recommended {
                shown.recommended()
            } else {
                shown
            }
        })
        .collect::<Vec<_>>();
    let shape = if offers.is_empty() {
        Shape::Text
    } else if question.multi_select {
        Shape::Multiple(offers)
    } else {
        Shape::Choice(offers)
    };
    let mut asked = Asked::new(
        &question.question,
        question.why.clone().unwrap_or_default(),
        false,
        shape,
        witness,
        epoch,
    );
    asked.free_text = question.free_text;
    asked
}

/// The knowledge decision a held line waits for, from the same snapshot: the
/// refusal in the Session's words, the line held exactly as typed (the
/// request kept), and the two acts as the exact lines the Session reads.
/// It names no identity; the epoch only scopes a selection.
fn knowledge(line: &str, state: Option<&work::Knowledge>, epoch: u64) -> Waiting {
    let label = match state {
        Some(work::Knowledge::Refused {
            source,
            by,
            code,
            cause,
            ..
        }) => format!("The knowledge {source} the {by} names was not admitted ({code}): {cause}"),
        _ => "The knowledge the configuration names was not admitted.".to_owned(),
    };
    let offers = vec![
        Offer::new(
            "/knowledge embedded",
            "use the knowledge built into Nika for this conversation · your message resumes once",
        ),
        Offer::new("cancel", "drop your message · it was sent nowhere"),
    ];
    let witness = format!("{epoch}:knowledge");
    let asked = Asked::new(label, "", false, Shape::Choice(offers), witness, epoch)
        .retaining(Retained::new(Some(line.to_owned()), Vec::new()));
    Waiting::knowledge(asked)
}

fn plain(key: &str) -> Waiting {
    Waiting::Question {
        key: key.to_owned(),
    }
}

/// The words and their painted witness come from the same supported question snapshot.
/// Legacy prompts retain the complete words without lending a typed answer identity.
pub(super) fn question(snapshot: &work::Work, key: &str, text: String, epoch: u64) -> Committed {
    if !matches!(key, "run_cost" | "unknown_cost")
        && let Waiting::QuestionDocument {
            key: pending,
            asked,
        } = capture(snapshot, epoch)
        && pending == key
    {
        return Committed::question(asked.witness, text);
    }
    Committed::new(Kind::Question, text)
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    /// The request beside a question reads as the person said it: the
    /// original words and their latest correction, never the frame the
    /// Session restated them with for its author; a plain goal reads whole,
    /// and open questions other than the one asked stay listed.
    #[test]
    fn the_request_beside_a_question_reads_as_said() {
        let original = "Fais-moi une veille quotidienne sur la météo";
        let once = work::Request::restated(original, "I leave the choice to Nika.");
        let twice = work::Request::restated(&once, "en markdown");
        let open = vec!["Which hour?".to_owned(), "Which city?".to_owned()];
        let request = work::Request::new(Some(twice), Vec::new(), open);
        let Some(kept) = retained(&request, "Which hour?") else {
            panic!("the request is kept")
        };
        assert_eq!(
            kept.goal.as_deref(),
            Some("Fais-moi une veille quotidienne sur la météo (correction: en markdown)")
        );
        assert_eq!(kept.open, ["Which city?"]);
        let shown = kept.goal.unwrap_or_default();
        for frame in ["Original request", "takes precedence"] {
            assert!(!shown.contains(frame), "{frame}: {shown}");
        }
        let hour = vec!["Which hour?".to_owned()];
        let plain = work::Request::new(Some(original.to_owned()), Vec::new(), hour);
        let Some(kept) = retained(&plain, "Which hour?") else {
            panic!("a plain goal is kept")
        };
        assert_eq!(kept.goal.as_deref(), Some(original));
        assert!(kept.open.is_empty());
        assert_eq!(retained(&work::Request::default(), "Which hour?"), None);
    }

    /// A question the request does not keep open answers something else (an
    /// activation's zone, a change asked after the save, an author's own
    /// question): its card names no request, never the first one said in the
    /// session; the request's own question keeps it.
    #[test]
    fn a_question_the_request_does_not_keep_open_names_no_request() {
        let first = "qu'est-ce que tu sais faire ?";
        let open = vec!["Which month?".to_owned()];
        let request = work::Request::new(Some(first.to_owned()), Vec::new(), open);
        for other in [
            "Which time zone should « chaque mois à 8h » follow?",
            "Which days should it run?",
        ] {
            assert_eq!(retained(&request, other), None, "{other}");
        }
        let Some(kept) = retained(&request, "  Which month? ") else {
            panic!("the request's own question names it")
        };
        assert_eq!(kept.goal.as_deref(), Some(first));
        let none_open = work::Request::new(Some(first.to_owned()), Vec::new(), Vec::new());
        assert_eq!(retained(&none_open, "Which month?"), None);
    }

    /// A held line waits as a typed knowledge decision: the refusal in the
    /// Session's words, the line kept exactly as typed, the two exact acts,
    /// nothing required.
    #[test]
    fn a_held_line_is_a_knowledge_decision_with_its_two_exact_acts() {
        let refused = work::Knowledge::Refused {
            source: "snapshot",
            by: "environment",
            code: "ADMISSION_UNTRUSTED".to_owned(),
            cause: "the release is not signed".to_owned(),
        };
        let Waiting::Knowledge { asked } = knowledge("  sum the amounts ", Some(&refused), 7)
        else {
            panic!("a knowledge decision");
        };
        assert_eq!(
            asked.label,
            "The knowledge snapshot the environment names was not admitted \
             (ADMISSION_UNTRUSTED): the release is not signed"
        );
        let Shape::Choice(offers) = &asked.shape else {
            panic!("its acts are offers");
        };
        let keys: Vec<&str> = offers.iter().map(|offer| offer.key.as_str()).collect();
        assert_eq!(keys, ["/knowledge embedded", "cancel"]);
        let kept = Retained::new(Some("  sum the amounts ".to_owned()), Vec::new());
        assert_eq!(asked.retained, Some(kept));
        assert!(!asked.mandatory);
        assert_eq!(asked.epoch, 7);
        let Waiting::Knowledge { asked } = knowledge("x", None, 7) else {
            panic!("a knowledge decision without the refusal's facts");
        };
        assert_eq!(
            asked.label,
            "The knowledge the configuration names was not admitted."
        );
    }
}

#[cfg(test)]
mod choice_tests;
