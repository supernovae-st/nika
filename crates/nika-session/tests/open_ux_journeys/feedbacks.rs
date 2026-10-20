// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The guided feedback journeys (NIK-174/175), as the person and the author act them: the digest
//! the person asked to save and run writes its report, then the optional offer at the free prompt
//! is answered, ignored, closed or outlived. `feedback_journeys` judges what the Session did.

use serde_json::{Value, json};

use super::{
    ACCEPT, Act, COUNT, DIGEST, HACKER_NEWS, REQUEST, Ran, SAVE_AND_RUN, SAVE_AND_RUN_WORDS,
    Scenario, TECHCRUNCH, accepted_pick, accepted_steps, consented, digest, retained, run_on,
};
use crate::peer::{Script, Step, call, call_saying, say};

/// The report the first run writes, then the amended run's.
pub(crate) const FIRST_REPORT: &str = "# Tech\n- Rust 1.97 is out\n";
pub(crate) const SECOND_REPORT: &str = "# Tech · AI and security\n- An AI model audit\n";
/// The precision the author asks once the person picked the topics to adjust.
pub(crate) const PRECISION: &str = "Quels sujets dois-je privilégier ?";
/// The person's answer to it.
pub(crate) const TOPICS: &str = "l'IA et la sécurité";
/// Words typed at the prompt instead of answering the offer: one that opens as a list of keys
/// does, and one that reads as an offered key.
pub(crate) const BRACKETED: &str = "[urgent] fais plus court la prochaine fois";
pub(crate) const LATER_IN_WORDS: &str = "later";
/// A new, unrelated request typed once an adjustment was picked (the replacement the person
/// states with both its paths), and its words that replace.
pub(crate) const UNRELATED: &str = super::REPLACE;
const REPLACING: &str = "laisse tomber tout ca";
/// The total the counting run writes.
pub(crate) const TOTAL: &str = "2\n";
/// The sales the counting workflow reads.
const SALES: &str = "date,montant\n2026-10-01,12\n2026-10-02,30\n";

/// The digest amended on the picked dimensions: deeper, on the person's topics.
fn amended_digest() -> Value {
    let source = digest(
        &[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)],
        DIGEST,
    )
    .replacen("max_tokens: 1000", "max_tokens: 2000", 1)
    .replacen(
        "Résume en Markdown les actualités ci-dessous",
        "Résume en détail, en privilégiant l'IA et la sécurité, les actualités ci-dessous",
        1,
    );
    json!({
        "source": source,
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            retained(DIGEST, "output_path", "u2"),
        ],
        "summary": "plus détaillé, consacré à l'IA et à la sécurité"
    })
}

/// The accepted digest saved and run once, its report written; then the author's `agent` steps
/// and the person's `acts`, the Session's `picks` readings after the accepting ones.
fn ran_once(mut agent: Vec<Step>, mut acts: Vec<Act>, picks: Vec<String>) -> Scenario {
    let mut steps = accepted_steps();
    steps.push(run_on(
        "C'est enregistré ; je le lance.",
        "u3",
        SAVE_AND_RUN_WORDS,
    ));
    steps.append(&mut agent);
    let mut all = vec![
        Act::Turn(REQUEST),
        Act::Submit(ACCEPT),
        Act::Submit(SAVE_AND_RUN),
        Act::RunEnds(Ran::Wrote(DIGEST, FIRST_REPORT)),
    ];
    all.append(&mut acts);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: all,
        script: Script {
            agent: steps,
            replies: Vec::new(),
            picks: [accepted_pick(), consented(1), picks].concat(),
            verdicts: Vec::new(),
        },
    }
}

/// Two dimensions, one precision, the amended run, the opinion.
fn guided_correction() -> Scenario {
    ran_once(
        vec![
            call(
                "ask",
                json!({"questions": [
                    {"key": "topics", "question": PRECISION, "free_text": true}]}),
            ),
            call("candidate_write", amended_digest()),
            call_saying(
                "Voici la révision : plus détaillée, consacrée à l'IA et à la sécurité.",
                "propose",
                json!({}),
            ),
            run_on(
                "Je l'enregistre et je la relance.",
                "u5",
                SAVE_AND_RUN_WORDS,
            ),
        ],
        vec![
            Act::AnswerFeedback(r#"["topics","depth"]"#),
            Act::AnswerQuestionOf(4, TOPICS),
            Act::Submit(SAVE_AND_RUN),
            Act::RunEnds(Ran::Wrote(DIGEST, SECOND_REPORT)),
            Act::AnswerFeedback("suitable"),
        ],
        // The precision is free words: read as no offer; then the second consent.
        vec!["NONE".to_owned(), "save_and_run".to_owned()],
    )
}

/// An adjustment picked, then a new request replaces the work: its result is offered the
/// ordinary adjustment, never the opinion of a « corrected » result.
fn new_request_never_inherits() -> Scenario {
    let mut scenario = ran_once(
        vec![
            call(
                "ask",
                json!({"questions": [
                        {"key": "topics", "question": PRECISION, "free_text": true}]}),
            ),
            call(
                "new_request",
                json!({"message": "u4", "excerpt": REPLACING}),
            ),
            call(
                "candidate_write",
                json!({"source": COUNT, "resolutions": [],
                            "summary": "compte les lignes de ./data/ventes.csv"}),
            ),
            call_saying(
                "Nouveau workflow : il compte les lignes de ./data/ventes.csv.",
                "propose",
                json!({}),
            ),
            run_on("Je l'enregistre et je le lance.", "u5", SAVE_AND_RUN_WORDS),
        ],
        vec![
            Act::AnswerFeedback(r#"["topics"]"#),
            Act::Turn(UNRELATED),
            Act::Submit(SAVE_AND_RUN),
            Act::RunEnds(Ran::Wrote("./out/total.txt", TOTAL)),
        ],
        vec!["NONE".to_owned(), "save_and_run".to_owned()],
    );
    scenario.files = vec![("data/ventes.csv", SALES)];
    scenario
}

pub(crate) fn scenario(name: &str) -> Scenario {
    match name {
        "feedback_guided_correction" => guided_correction(),
        // Words at the prompt are the conversation's, whatever their bytes read as: the offer
        // closes unanswered, and its identity answers nothing afterwards.
        "feedback_words_are_lines" => ran_once(
            vec![
                say("D'accord : la prochaine version sera plus courte."),
                say("Entendu."),
            ],
            vec![
                Act::Turn(BRACKETED),
                Act::Turn(LATER_IN_WORDS),
                Act::AnswerFeedbackOf(3, r#"["nothing"]"#),
            ],
            Vec::new(),
        ),
        // « Nothing to change » stands alone; alone, it is recorded and changes nothing.
        "feedback_mixed_refused" => ran_once(
            Vec::new(),
            vec![
                Act::AnswerFeedback(r#"["nothing","depth"]"#),
                Act::AnswerFeedback(r#"["nothing"]"#),
            ],
            Vec::new(),
        ),
        // A prompt that owns the next line keeps it: the offer waits, unanswered, and is
        // answered once nothing else waits.
        "feedback_waits_behind_a_prompt" => ran_once(
            Vec::new(),
            vec![
                Act::Submit("/intelligence"),
                Act::AnswerFeedback(r#"["depth"]"#),
            ],
            Vec::new(),
        ),
        // A run's review closes the offer: its identity answers nothing afterwards.
        "feedback_closed_by_a_run_review" => ran_once(
            Vec::new(),
            vec![Act::ReviewRun, Act::AnswerFeedbackOf(3, r#"["depth"]"#)],
            Vec::new(),
        ),
        // Bytes offered and never answered are not offered again after a reopen.
        "feedback_reopened_offers_nothing_again" => {
            let mut scenario = ran_once(
                vec![say("D'accord.")],
                vec![
                    Act::Turn(LATER_IN_WORDS),
                    Act::Reopen,
                    Act::Turn("run it"),
                    Act::RunEnds(Ran::Wrote(DIGEST, FIRST_REPORT)),
                ],
                Vec::new(),
            );
            scenario.history = true;
            scenario
        }
        "feedback_new_request_never_inherits" => new_request_never_inherits(),
        // « Later », then the same saved bytes run again: nothing is offered again.
        "feedback_rerun_offers_nothing" => ran_once(
            Vec::new(),
            vec![
                Act::AnswerFeedback("later"),
                Act::Turn("run it"),
                Act::RunEnds(Ran::Wrote(DIGEST, FIRST_REPORT)),
            ],
            Vec::new(),
        ),
        _ => panic!("unknown feedback scenario {name}"),
    }
}
