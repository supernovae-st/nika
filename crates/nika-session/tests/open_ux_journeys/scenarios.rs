// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The journeys, shared by the parent (the world and the author's script) and the child (the
//! lines a person types, in order). The person writes short French with the typos people type;
//! the author's decisions are scripted against the author tool contract (`ask` · `candidate_write`
//! · `propose` · `new_request`), so each scenario proves what the Session does with them —
//! identities, provenance, retention, authority — never an intelligence's judgment.

use std::fmt::Write as _;

use serde_json::{Value, json};

use super::peer::{Script, Step, call, call_saying, hold, say};

/// The workflow model the human chose for the session (a local engine's route).
pub(crate) const SEAT: &str = "vllm/oux-author";
pub(crate) const HACKER_NEWS: &str = "https://news.ycombinator.com";
pub(crate) const TECHCRUNCH: &str = "https://techcrunch.com";
pub(crate) const LE_MONDE: &str = "https://www.lemonde.fr/international/";
pub(crate) const DIGEST: &str = "./news/digest.md";

/// The ordinary request: sources and output left open.
pub(crate) const REQUEST: &str = "fais moi un workflow tres simple qui recupere les news tech recentes, les resume et ecrit le resultat en markdown dans un dossier du projet";
/// The acceptance of the current recommendation.
pub(crate) const ACCEPT: &str = "oui tout me va, je suis tes recos";
/// A request that delegates the public sources and leaves the output name to the author.
pub(crate) const DELEGATING: &str = "recupere les news tech et geopolitiques recentes, resume les et ecris le resume en markdown dans un dossier du projet. les sources publiques tu les choisis toi meme";
/// The delegating words of `DELEGATING`.
pub(crate) const DELEGATION: &str = "les sources publiques tu les choisis toi meme";
/// The words of `DELEGATING` that ask for a Markdown file inside the project.
pub(crate) const OUTPUT_WORDS: &str = "ecris le resume en markdown dans un dossier du projet";
/// A partial correction: one more source, everything else unchanged.
pub(crate) const ADD_GEOPOLITICS: &str =
    "ajoute aussi les news geopolitiques, le monde international par exemple";
/// The source that correction names.
pub(crate) const LE_MONDE_WORDS: &str = "le monde international";
/// Values typed as a reply, a path without its `./`.
pub(crate) const TYPED: &str = "utilise https://news.ycombinator.com et ecris dans news/digest.md";
/// One more named source.
pub(crate) const ADD_TECHCRUNCH: &str = "ajoute aussi techcrunch";
/// Frustration about a value already given.
pub(crate) const ALREADY: &str = "je te l'ai deja dit c'est news/digest.md !";
/// A question about the question.
pub(crate) const WHY: &str = "pourquoi tu as besoin de ca ?";
/// A correction typed while the first offer waits.
pub(crate) const ALSO_LE_MONDE: &str = "en fait ajoute le monde international aussi";
/// Two private addresses only the human holds, and a question that depends on one of them.
pub(crate) const HOOKS: &str = "chaque matin resume les news tech de hacker news et envoie le resume au webhook de mon equipe et a celui du support";
/// A reply that answers one of two independent questions.
pub(crate) const ONE_HOOK: &str =
    "equipe : https://hooks.example.org/team, pour le support je sais pas encore";
pub(crate) const TEAM_HOOK: &str = "https://hooks.example.org/team";
/// A complete replacement of the request.
pub(crate) const REPLACE: &str = "non laisse tomber tout ca, je veux plutot compter les lignes de ./data/ventes.csv et ecrire le total dans ./out/total.txt";
/// An explicit Save and Run, said in words.
pub(crate) const SAVE_AND_RUN: &str = "parfait, enregistre-le puis lance-le";
/// The words of `SAVE_AND_RUN` that ask for both acts.
pub(crate) const SAVE_AND_RUN_WORDS: &str = "enregistre-le puis lance-le";
/// A request the author starts on.
pub(crate) const START_A: &str = "resume moi les news tech de hacker news";
/// A line steered into the run under way.
pub(crate) const STEER_B: &str = "non prend plutot techcrunch";
/// A line queued for after the run.
pub(crate) const THEN_C: &str = "et apres dis moi combien ca coute";
/// A line queued, then the run stopped.
pub(crate) const THEN_STOP: &str = "et ecris le dans news/digest.md";
/// A line after a stopped run.
pub(crate) const STILL_THERE: &str = "t'es toujours la ?";
/// What the author says once it read the steered line.
pub(crate) const STEERED: &str = "D'accord, je prends TechCrunch.";
/// What the author says at the end of its first run, then once it read the follow-up.
pub(crate) const FIRST_DONE: &str = "Voici le resume de Hacker News.";
pub(crate) const FOLLOWED: &str = "Ca ne coute rien de plus ici.";
/// What the author says after a stopped run.
pub(crate) const STILL_HERE: &str = "Oui, je suis la.";
/// The marker of the author's held request.
pub(crate) const HELD: &str = "held";

/// One act of the person, in order.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Act {
    /// A line typed while nothing is shown: `turn`.
    Turn(&'static str),
    /// A line typed against what the Session shows now: `submit(line, waiting())`.
    Submit(&'static str),
    /// A line typed against what the Session showed after act `n` (a stale screen).
    SubmitAsShownAfter(usize, &'static str),
    /// The question that waited after act `n`, answered by its identity.
    AnswerQuestionOf(usize, &'static str),
    /// The question that waited after act `n`, answered by its identity in another Session of
    /// the same project.
    AnswerElsewhere(usize, &'static str),
    /// The Session closes and a new one opens on the same project and home.
    Reopen,
    /// A line typed while nothing is shown, in a turn the host can stop (the host begins it).
    Stoppable(&'static str),
    /// `Stoppable`, and while the author's request is held under the marker, the person acts
    /// from the host's thread.
    While(&'static str, &'static str, During),
    /// A project file written as a person edits it, between two lines.
    Edit(&'static str, &'static str),
}

/// What the person does from the host's thread while the author's request is under way.
#[derive(Clone, Copy, Debug)]
pub(crate) enum During {
    /// A line that enters after the calls under way.
    Steer(&'static str),
    /// A line that enters when the run would end.
    FollowUp(&'static str),
    /// A line queued for after the run, then Stop.
    FollowUpThenStop(&'static str),
}

/// One journey: the project files, whether the Session keeps history, the person's acts, and
/// the author's script.
pub(crate) struct Scenario {
    pub(crate) files: Vec<(&'static str, &'static str)>,
    pub(crate) history: bool,
    pub(crate) acts: Vec<Act>,
    pub(crate) script: Script,
}

/// A digest workflow on the session's model: one GET per source, one summary, one write.
pub(crate) fn digest(sources: &[(&str, &str)], output: &str) -> String {
    let host = |url: &str| -> String {
        let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
        rest.split(['/', '?', '#'])
            .next()
            .unwrap_or(rest)
            .to_owned()
    };
    let hosts: Vec<String> = sources
        .iter()
        .map(|(_, url)| format!("\"{}\"", host(url)))
        .collect();
    let mut source = format!(
        "nika: news-digest\nmodel: {SEAT}\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net:\n    http: [{}]\n  fs:\n    write: [\"{output}\"]\ntasks:\n",
        hosts.join(", ")
    );
    for (task, url) in sources {
        write!(
            source,
            "  {task}:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{url}\", method: GET }}\n"
        )
        .expect("a String takes the write");
    }
    source.push_str("  summarize:\n    with:\n");
    for (task, _) in sources {
        writeln!(source, "      {task}: \"${{{{ tasks.{task}.output }}}}\"")
            .expect("a String takes the write");
    }
    let read: Vec<String> = sources
        .iter()
        .map(|(task, _)| format!("${{{{ with.{task} }}}}"))
        .collect();
    writeln!(
        source,
        "    infer:\n      max_tokens: 1000\n      prompt: \"Résume en Markdown les actualités ci-dessous, sans rien inventer : {}\"",
        read.join(" ")
    )
    .expect("a String takes the write");
    writeln!(
        source,
        "  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{output}\", content: \"${{{{ with.digest }}}}\" }}"
    )
    .expect("a String takes the write");
    source
}

/// The replacement request's workflow: every path stated by the person.
pub(crate) const COUNT: &str = "nika: count-lines\npermits:\n  tools: [\"nika:read\", \"nika:jq\", \"nika:write\"]\n  fs:\n    read: [\"./data/ventes.csv\"]\n    write: [\"./out/total.txt\"]\ntasks:\n  read_sales:\n    invoke:\n      tool: \"nika:read\"\n      args: { path: \"./data/ventes.csv\" }\n  count:\n    with: { text: \"${{ tasks.read_sales.output }}\" }\n    invoke:\n      tool: \"nika:jq\"\n      args: { input: \"${{ with.text }}\", expression: \"split(\\\"\\\\n\\\") | map(select(length > 0)) | length | tostring\" }\n  write_total:\n    with: { total: \"${{ tasks.count.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/total.txt\", content: \"${{ with.total }}\" }\n";

/// One selection the author states: its value, provenance, role, and the person's message and
/// words that authorize it.
pub(crate) fn resolution(
    value: &str,
    kind: &str,
    role: &str,
    message: &str,
    excerpt: &str,
) -> Value {
    json!({"value": value, "kind": kind, "role": role, "message": message, "excerpt": excerpt})
}

/// A selection taken from the recommended option of the question `plan`.
fn offered(value: &str, role: &str, message: &str) -> Value {
    let mut row = resolution(value, "offered", role, message, ACCEPT);
    row["question"] = json!("plan");
    row["option"] = json!("recommended");
    row
}

/// A selection kept from an earlier message, unchanged.
fn retained(value: &str, role: &str, message: &str) -> Value {
    json!({"value": value, "kind": "retained", "role": role, "message": message})
}

/// The author's one recommendation for the ordinary request, asked as a choice.
pub(crate) fn plan_offer(sources: &[(&str, &str)]) -> Value {
    let values: Vec<Value> = sources
        .iter()
        .map(|(name, url)| json!({"role": "read_source", "value": url, "name": name}))
        .chain(std::iter::once(
            json!({"role": "output_path", "value": DIGEST}),
        ))
        .collect();
    let names: Vec<&str> = sources.iter().map(|(name, _)| *name).collect();
    json!({"questions": [{
        "key": "plan",
        "question": format!("Je prends {} et j'écris le résumé dans {DIGEST} : ça te va ?", names.join(", ")),
        "why": "La demande ne nomme ni les sources ni le fichier de sortie.",
        "options": [
            {"key": "recommended", "label": format!("Oui : {} → {DIGEST}", names.join(", ")),
             "recommended": true, "values": values},
            {"key": "other", "label": "D'autres sources ou un autre fichier"}
        ],
        "free_text": true
    }]})
}

fn tech() -> [(&'static str, &'static str); 2] {
    [("Hacker News", HACKER_NEWS), ("TechCrunch", TECHCRUNCH)]
}

/// The tech digest the accepted recommendation builds (message `u2` accepted it).
fn accepted_digest() -> Value {
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [
            offered(HACKER_NEWS, "read_source", "u2"),
            offered(TECHCRUNCH, "read_source", "u2"),
            offered(DIGEST, "output_path", "u2"),
        ],
        "summary": "Hacker News et TechCrunch, résumé dans ./news/digest.md"
    })
}

/// The first two turns every accepting journey shares: the offer, then its acceptance.
fn accepted_steps() -> Vec<Step> {
    vec![
        call("ask", plan_offer(&tech())),
        call("candidate_write", accepted_digest()),
        call("propose", json!({})),
        say("Voici le workflow : Hacker News et TechCrunch, résumé dans ./news/digest.md."),
    ]
}

/// The tech digest with Le Monde added by message `u3`, everything else kept.
fn with_le_monde() -> Value {
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH),
            ("le_monde", LE_MONDE)], DIGEST),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            retained(DIGEST, "output_path", "u2"),
            resolution(LE_MONDE, "named", "read_source", "u3", LE_MONDE_WORDS),
        ],
        "summary": "Le Monde international ajouté"
    })
}

pub(crate) fn scenario(name: &str) -> Scenario {
    match name {
        "accept_offer" => accept_offer(),
        "delegated_sources" => delegated_sources(),
        "partial_correction" => partial_correction(),
        "already_given" => already_given(),
        "reopen" => reopen(),
        "question_about_the_question" => question_about_the_question(),
        "grouped_questions" => grouped_questions(),
        "stale_answers" => stale_answers(),
        "complete_replacement" => complete_replacement(),
        "save_and_run_in_words" => save_and_run_in_words(),
        "save_and_run_scope_changed" => save_and_run_scope_changed(),
        "steer_mid_run" => steer_mid_run(),
        "follow_up_after_run" => follow_up_after_run(),
        "stop_mid_request" => stop_mid_request(),
        "doubted_candidate" => doubted_candidate(),
        "reads_a_project_file" => reads_sales(false),
        "a_read_file_changes" => reads_sales(true),
        other => panic!("unknown scenario {other}"),
    }
}

fn accept_offer() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::AnswerQuestionOf(0, "oui"),
        ],
        script: Script {
            agent: accepted_steps(),
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn delegated_sources() -> Scenario {
    let write = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH),
            ("le_monde", LE_MONDE)], DIGEST),
        "resolutions": [
            resolution(HACKER_NEWS, "delegated", "read_source", "u1", DELEGATION),
            resolution(TECHCRUNCH, "delegated", "read_source", "u1", DELEGATION),
            resolution(LE_MONDE, "delegated", "read_source", "u1", DELEGATION),
            resolution(DIGEST, "derived", "output_path", "u1", OUTPUT_WORDS),
        ],
        "summary": "Hacker News, TechCrunch et Le Monde international, résumé dans ./news/digest.md"
    });
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![Act::Turn(DELEGATING)],
        script: Script {
            agent: vec![
                call("candidate_write", write),
                call("propose", json!({})),
                say(
                    "J'ai choisi Hacker News, TechCrunch et Le Monde international ; le résumé va dans ./news/digest.md.",
                ),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn partial_correction() -> Scenario {
    // The author first drops TechCrunch without being asked to: the Session refuses to lose a
    // retained selection silently, and the author writes the correction again, keeping it.
    let dropped = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("le_monde", LE_MONDE)], DIGEST),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(DIGEST, "output_path", "u2"),
            resolution(LE_MONDE, "named", "read_source", "u3", LE_MONDE_WORDS),
        ],
        "summary": "Le Monde international ajouté"
    });
    let mut agent = accepted_steps();
    agent.extend([
        call("candidate_write", dropped),
        call("propose", json!({})),
        call("candidate_write", with_le_monde()),
        call("propose", json!({})),
        say("J'ai ajouté Le Monde international ; le reste ne change pas."),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(ADD_GEOPOLITICS),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn already_given() -> Scenario {
    let typed = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS)], DIGEST),
        "resolutions": [
            resolution(HACKER_NEWS, "answered", "read_source", "u2", HACKER_NEWS),
            resolution(DIGEST, "answered", "output_path", "u2", "news/digest.md"),
        ],
        "summary": "Hacker News, résumé dans ./news/digest.md"
    });
    let added = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(DIGEST, "output_path", "u2"),
            resolution(TECHCRUNCH, "named", "read_source", "u3", "techcrunch"),
        ],
        "summary": "TechCrunch ajouté"
    });
    // A careless author asks again for the output it already holds: the Session answers the
    // ask itself with the settled value, and the person never sees the question.
    let again = json!({"questions": [{
        "key": "output", "role": "output_path",
        "question": "Dans quel fichier écrire le résumé ?",
        "why": "Il faut un fichier de sortie.", "free_text": true, "options": []
    }]});
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(TYPED),
            Act::Submit(ADD_TECHCRUNCH),
            Act::Submit(ALREADY),
        ],
        script: Script {
            agent: vec![
                call("ask", plan_offer(&tech())),
                call("candidate_write", typed),
                call("propose", json!({})),
                say("Voici le workflow : Hacker News, résumé dans ./news/digest.md."),
                call("ask", again),
                call("candidate_write", added),
                call("propose", json!({})),
                say("J'ai ajouté TechCrunch."),
                say("Oui, c'est noté : le résumé va déjà dans ./news/digest.md."),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn reopen() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([
        call("candidate_write", with_le_monde()),
        call("propose", json!({})),
        say("J'ai ajouté Le Monde international ; le reste ne change pas."),
    ]);
    Scenario {
        files: Vec::new(),
        history: true,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Reopen,
            Act::Turn(ADD_GEOPOLITICS),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn question_about_the_question() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(WHY),
            Act::AnswerQuestionOf(0, "oui"),
        ],
        script: Script {
            agent: vec![
                call("ask", plan_offer(&tech())),
                call_saying(
                    "La demande ne nomme ni les sources ni le fichier de sortie : voici ce que je propose.",
                    "ask",
                    plan_offer(&tech()),
                ),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn grouped_questions() -> Scenario {
    let first = json!({"questions": [
        {"key": "team_webhook", "role": "value", "question": "Quelle est l'adresse HTTPS du webhook de ton équipe ?",
         "why": "Une adresse privée que seule ton équipe connaît.", "free_text": true, "options": []},
        {"key": "support_webhook", "role": "value", "question": "Quelle est l'adresse HTTPS du webhook du support ?",
         "why": "Une adresse privée que seul le support connaît.", "free_text": true, "options": []},
        {"key": "support_token", "role": "value", "question": "Le webhook du support demande-t-il un jeton ?",
         "why": "Un jeton se garde en secret, jamais dans le workflow.", "free_text": false,
         "options": [{"key": "yes", "label": "Oui"}, {"key": "no", "label": "Non"}],
         "after": ["support_webhook"]}
    ]});
    let rest = json!({
        "answered": [{"key": "team_webhook", "value": TEAM_HOOK, "message": "u2", "excerpt": TEAM_HOOK}],
        "questions": [
            {"key": "support_webhook", "role": "value", "question": "Quelle est l'adresse HTTPS du webhook du support ?",
             "why": "Il me manque encore celle-ci.", "free_text": true, "options": []},
            {"key": "support_token", "role": "value", "question": "Le webhook du support demande-t-il un jeton ?",
             "why": "Un jeton se garde en secret, jamais dans le workflow.", "free_text": false,
             "options": [{"key": "yes", "label": "Oui"}, {"key": "no", "label": "Non"}],
             "after": ["support_webhook"]}
        ]
    });
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![Act::Turn(HOOKS), Act::Submit(ONE_HOOK)],
        script: Script {
            agent: vec![
                call("ask", first),
                call_saying("Merci, il me manque l'adresse du support.", "ask", rest),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn stale_answers() -> Scenario {
    let le_monde = [
        ("Hacker News", HACKER_NEWS),
        ("TechCrunch", TECHCRUNCH),
        ("Le Monde international", LE_MONDE),
    ];
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::SubmitAsShownAfter(0, ALSO_LE_MONDE),
            Act::SubmitAsShownAfter(0, "oui"),
            Act::AnswerElsewhere(1, "oui"),
        ],
        script: Script {
            agent: vec![
                call("ask", plan_offer(&tech())),
                call("ask", plan_offer(&le_monde)),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn complete_replacement() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([
        call(
            "new_request",
            json!({"message": "u3", "excerpt": "laisse tomber tout ca"}),
        ),
        call(
            "candidate_write",
            json!({"source": COUNT, "resolutions": [], "summary": "compte les lignes de ./data/ventes.csv"}),
        ),
        call("propose", json!({})),
        say("Nouveau workflow : il compte les lignes de ./data/ventes.csv et écrit le total dans ./out/total.txt."),
    ]);
    Scenario {
        files: vec![(
            "data/ventes.csv",
            "date,montant\n2026-10-01,12\n2026-10-02,30\n",
        )],
        history: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(REPLACE),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn save_and_run_in_words() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([
        call(
            "propose",
            json!({"acts": ["save", "run"],
                "authorized_by": {"message": "u3", "excerpt": SAVE_AND_RUN_WORDS}}),
        ),
        say("C'est enregistré ; je le lance."),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_AND_RUN),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn save_and_run_scope_changed() -> Scenario {
    // The author changes what the workflow writes, then claims the earlier words cover it.
    let elsewhere = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], "./news/autre.md"),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            resolution("./news/autre.md", "derived", "output_path", "u1", "ecrit le resultat en markdown dans un dossier du projet"),
        ],
        "summary": "le résumé va dans ./news/autre.md"
    });
    let mut agent = accepted_steps();
    agent.extend([
        call("candidate_write", elsewhere),
        call(
            "propose",
            json!({"acts": ["save", "run"],
                "authorized_by": {"message": "u3", "excerpt": SAVE_AND_RUN_WORDS}}),
        ),
        say("Le fichier de sortie a changé : confirme l'enregistrement."),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_AND_RUN),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// A line steered while the author's second request is under way: the call of its first step
/// ran; the call it answers with once the line arrived is not run, and it reads the line, cited,
/// before it answers.
fn steer_mid_run() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![Act::While(START_A, HELD, During::Steer(STEER_B))],
        script: Script {
            agent: vec![
                call("models", json!({"role": "run"})),
                hold(HELD),
                call("models", json!({"role": "author"})),
                say(STEERED),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// A line queued for after the run: the author's failed call does not end the run, its answer
/// does, and the line enters then.
fn follow_up_after_run() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![Act::While(START_A, HELD, During::FollowUp(THEN_C))],
        script: Script {
            agent: vec![
                hold(HELD),
                call("models", json!({"role": "chef"})),
                say(FIRST_DONE),
                say(FOLLOWED),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// Stop while the author's request is under way, a line queued: the request is dropped, the
/// line comes back unsent, and the next line goes on.
fn stop_mid_request() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        acts: vec![
            Act::While(START_A, HELD, During::FollowUpThenStop(THEN_STOP)),
            Act::Stoppable(STILL_THERE),
        ],
        script: Script {
            agent: vec![
                hold(HELD),
                say("(the dropped request's answer: never read)"),
                say(STILL_HERE),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
/// The counting request in one line: every path stated by the person.
pub(crate) const COUNT_REQUEST: &str =
    "compte les lignes de ./data/ventes.csv et ecris le total dans ./out/total.txt";
/// The sales the counting journeys read.
const SALES: &str = "date,montant\n2026-10-01,12\n2026-10-02,30\n";
/// What the author says once its candidate was held: it repairs, nothing is proposed.
pub(crate) const REPAIRING: &str =
    "Le vérificateur doute de ce workflow : je le corrige avant de te le proposer.";
/// The words a candidate held on a doubt nothing located opens with.
pub(crate) const UNRESOLVED: &str =
    "The verifier doubted the request as a whole but located nothing";

/// The counting workflow, written for the counting request.
fn count_write() -> Value {
    json!({"source": COUNT, "resolutions": [], "summary": "compte les lignes de ./data/ventes.csv"})
}

/// A candidate its judge rejects as a whole, locating nothing: held, never proposed; the
/// author verifies the same bytes again, then says it repairs.
fn doubted_candidate() -> Scenario {
    Scenario {
        files: vec![("data/ventes.csv", SALES)],
        history: false,
        acts: vec![Act::Turn(COUNT_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", count_write()),
                call("propose", json!({})),
                call("verify", json!({})),
                say(REPAIRING),
            ],
            replies: Vec::new(),
            verdicts: vec!["unfaithful".to_owned()],
        },
    }
}
/// The same sales, changed after the proposal.
pub(crate) const SALES_CHANGED: &str = "date,montant\n2026-10-01,12\n2026-10-02,30\n2026-10-03,7\n";
/// What the author says once the counting workflow is shown.
const COUNTED: &str = "Voici le workflow : il compte les lignes de ./data/ventes.csv.";

/// The counting workflow proposed, then the person's yes; `changed` edits the sales between.
fn reads_sales(changed: bool) -> Scenario {
    let mut acts = vec![Act::Turn(COUNT_REQUEST)];
    if changed {
        acts.push(Act::Edit("data/ventes.csv", SALES_CHANGED));
    }
    acts.push(Act::Submit("oui"));
    Scenario {
        files: vec![("data/ventes.csv", SALES)],
        history: false,
        acts,
        script: Script {
            agent: vec![
                call("candidate_write", count_write()),
                call("propose", json!({})),
                say(COUNTED),
            ],
            replies: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
