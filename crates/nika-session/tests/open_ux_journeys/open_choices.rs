// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The open-choice journeys: a proposal shown at once with the choices it leaves open and
//! their defaults, never a question a default can stand for.

use serde_json::json;

use super::{
    Act, DIGEST, HACKER_NEWS, REQUEST, SEAT, Scenario, TECHCRUNCH, YC_BLOG, defaults_digest,
    digest, plan_choice, resolution, tech,
};
use crate::peer::{Script, Step, call, call_saying, say};

/// A French news request: two sites named, a Markdown summary in a folder left open.
pub(crate) const NEWS_REQUEST: &str = "Récupère les dernières actualités tech de Y Combinator et TechCrunch, fais-moi un résumé et sauvegarde-le en Markdown dans un dossier.";
/// What the author says with its first proposal.
pub(crate) const SHOWN_WITH_DEFAULTS: &str = "Voici le workflow : Hacker News pour Y Combinator, TechCrunch, le résumé dans ./news/digest.md. Dis oui, ou choisis une autre option.";
/// The other folder the author offers.
pub(crate) const NOTES_DIGEST: &str = "./notes/digest.md";
/// What the author says when the person picked the YC blog, without revising.
pub(crate) const BLOG_NOTED: &str = "D'accord, le blog de Y Combinator.";

/// The request's candidate and the choices its proposal shows with their defaults: the Y Combinator
/// site, the folder, the run model.
pub(super) fn news_proposed() -> Vec<Step> {
    let write = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch")],
        "summary": "Hacker News et TechCrunch, résumé dans ./news/digest.md",
    });
    let open = json!({"open": [
        {"key": "yc", "question": "Quel site de Y Combinator ?", "role": "read_source",
         "options": [
            {"key": "hn", "label": "Hacker News", "recommended": true,
             "values": [{"role": "read_source", "value": HACKER_NEWS, "name": "Hacker News"}]},
            {"key": "blog", "label": "Le blog YC",
             "values": [{"role": "read_source", "value": YC_BLOG}]}]},
        {"key": "folder", "question": "Dans quel dossier ?", "role": "output_path",
         "options": [
            {"key": "news", "label": "news/", "recommended": true,
             "values": [{"role": "output_path", "value": DIGEST}]},
            {"key": "notes", "label": "notes/",
             "values": [{"role": "output_path", "value": NOTES_DIGEST}]}]},
        {"key": "model", "question": "Quel modèle pour le résumé ?", "role": "run_model",
         "options": [
            {"key": "seat", "label": "Le modèle de cette Session", "recommended": true,
             "values": [{"role": "run_model", "value": SEAT}]}]}]});
    vec![
        call("candidate_write", write),
        call_saying(SHOWN_WITH_DEFAULTS, "propose", open),
    ]
}

/// The news request proposed at once with its open choices; « oui » keeps the defaults.
pub(super) fn open_defaults() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(NEWS_REQUEST), Act::Submit("oui")],
        script: Script {
            agent: news_proposed(),
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The YC blog picked at the proposal; the author does not revise, so « oui » waits.
pub(super) fn open_pick_other() -> Scenario {
    let mut agent = news_proposed();
    agent.push(say(BLOG_NOTED));
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(NEWS_REQUEST),
            Act::Submit("blog"),
            Act::Submit("oui"),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            // The Session reads the line against the folder and the model: it picks neither.
            picks: vec!["NONE".to_owned(), "NONE".to_owned()],
            verdicts: Vec::new(),
        },
    }
}

/// The author asks for the plan it can recommend: the ask door refuses it with its reason, and the
/// author proposes the plan open at once instead.
pub(super) fn default_not_asked() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(REQUEST)],
        script: Script {
            agent: vec![
                call("ask", json!({"questions": [plan_choice(&tech())]})),
                call("candidate_write", defaults_digest()),
                call_saying(
                    "Je propose Hacker News et TechCrunch, résumé dans ./news/digest.md.",
                    "propose",
                    json!({"open": [plan_choice(&tech())]}),
                ),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
