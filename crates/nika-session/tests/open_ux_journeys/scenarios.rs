// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The journeys, shared by the parent (the world and the author's script) and the child (the
//! lines a person types, in order). The person writes short French with the typos people type;
//! the author's decisions are scripted against the author tool contract (`ask` · `candidate_write`
//! · `propose` · `new_request`), so each scenario proves what the Session does with them —
//! identities, provenance, retention, authority — never an intelligence's judgment.

use std::fmt::Write as _;

use serde_json::{Value, json};

use super::peer::{Script, Step, call, call_saying, hold, say, think};

/// The Session's reading of the sentence that accepts the plan: its recommended option.
fn accepted_pick() -> Vec<String> {
    vec!["recommended".to_owned()]
}

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
/// The counting request in one line: every path stated by the person.
pub(crate) const COUNT_REQUEST: &str =
    "compte les lignes de ./data/ventes.csv et ecris le total dans ./out/total.txt";
/// What the author says once its candidate was held: it repairs, nothing is proposed.
pub(crate) const REPAIRING: &str =
    "Le vérificateur doute de ce workflow : je le corrige avant de te le proposer.";
/// The words a candidate held on a doubt nothing located opens with.
pub(crate) const UNRESOLVED: &str =
    "The verifier doubted the request as a whole but located nothing";

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
    /// The run the Session requested ends as the door observed it: the child writes its trace
    /// frames and the Session observes them, as a door does.
    RunEnds(Ran),
    /// A project file written as a person edits it, between two lines.
    Edit(&'static str, &'static str),
}

/// How a run the child stands in for ended.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Ran {
    /// Every task ran and the workflow completed.
    Succeeded,
    /// The task failed with the engine's detail (« CODE message »).
    Failed(&'static str, &'static str),
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

/// One journey: the project files, whether the Session keeps history and prepares continuously
/// (a host's money draft, as the native door keeps it), the person's acts, and the author's script.
pub(crate) struct Scenario {
    pub(crate) files: Vec<(&'static str, &'static str)>,
    pub(crate) history: bool,
    pub(crate) preparing: bool,
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
        call_saying(
            "Voici le workflow : Hacker News et TechCrunch, résumé dans ./news/digest.md.",
            "propose",
            json!({}),
        ),
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
        "refused_offer" => refused_offer(),
        "deflection_takes_the_recommendation" => deflection_takes_the_recommendation(),
        "delegated_name" => delegated_name(),
        "source_swap" => source_swap(),
        "folder_line" => folder_line(),
        "failed_run_repaired" => failed_run_repaired(),
        "complaint_after_success" => complaint_after_success(),
        "doubted_candidate" => doubted_candidate(),
        "trial_offset" => tried("raw", OFFSET_BROKEN, OFFSET_FIXED),
        "trial_kept_nothing" => tried("feed", RFC_822_BROKEN, RFC_822_FIXED),
        "trial_names_what_it_skips" => trial_names_what_it_skips(),
        "own_filter" => own_filter(),
        "reads_a_project_file" => reads_sales(false),
        "a_read_file_changes" => reads_sales(true),
        "write_carries_its_findings" => write_carries_its_findings(),
        "thought_only" => thought_only(),
        other => panic!("unknown scenario {other}"),
    }
}

/// A candidate its judge rejects as a whole, locating nothing: held, never proposed; the
/// author verifies the same bytes again, then says it repairs.
fn doubted_candidate() -> Scenario {
    let write = json!({"source": COUNT, "resolutions": [],
        "summary": "compte les lignes de ./data/ventes.csv"});
    Scenario {
        files: vec![(
            "data/ventes.csv",
            "date,montant\n2026-10-01,12\n2026-10-02,30\n",
        )],
        history: false,
        preparing: false,
        acts: vec![Act::Turn(COUNT_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", write),
                call("propose", json!({})),
                call("verify", json!({})),
                say(REPAIRING),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: vec!["unfaithful".to_owned(), "unexercised".to_owned()],
        },
    }
}

fn accept_offer() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::AnswerQuestionOf(0, "oui"),
        ],
        script: Script {
            agent: accepted_steps(),
            replies: Vec::new(),
            picks: accepted_pick(),
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
        preparing: false,
        acts: vec![Act::Turn(DELEGATING)],
        script: Script {
            agent: vec![
                call("candidate_write", write),
                call_saying(
                    "J'ai choisi Hacker News, TechCrunch et Le Monde international ; le résumé va dans ./news/digest.md.",
                    "propose",
                    json!({}),
                ),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
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
        call_saying(
            "J'ai ajouté Le Monde international ; le reste ne change pas.",
            "propose",
            json!({}),
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(ADD_GEOPOLITICS),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
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
        preparing: false,
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
                call_saying(
                    "Voici le workflow : Hacker News, résumé dans ./news/digest.md.",
                    "propose",
                    json!({}),
                ),
                call("ask", again),
                call("candidate_write", added),
                call_saying("J'ai ajouté TechCrunch.", "propose", json!({})),
                say("Oui, c'est noté : le résumé va déjà dans ./news/digest.md."),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn reopen() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([
        call("candidate_write", with_le_monde()),
        call_saying(
            "J'ai ajouté Le Monde international ; le reste ne change pas.",
            "propose",
            json!({}),
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: true,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Reopen,
            Act::Turn(ADD_GEOPOLITICS),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}

fn question_about_the_question() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
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
            picks: Vec::new(),
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
        preparing: false,
        acts: vec![Act::Turn(HOOKS), Act::Submit(ONE_HOOK)],
        script: Script {
            agent: vec![
                call("ask", first),
                call_saying("Merci, il me manque l'adresse du support.", "ask", rest),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
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
        preparing: false,
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
            picks: Vec::new(),
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
        call_saying("Nouveau workflow : il compte les lignes de ./data/ventes.csv et écrit le total dans ./out/total.txt.", "propose", json!({})),
    ]);
    Scenario {
        files: vec![(
            "data/ventes.csv",
            "date,montant\n2026-10-01,12\n2026-10-02,30\n",
        )],
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(REPLACE),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}

fn save_and_run_in_words() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([call_saying(
        "C'est enregistré ; je le lance.",
        "propose",
        json!({"acts": ["save", "run"],
                "authorized_by": {"message": "u3", "excerpt": SAVE_AND_RUN_WORDS}}),
    )]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_AND_RUN),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
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
        call_saying(
            "Le fichier de sortie a changé : confirme l'enregistrement.",
            "propose",
            json!({"acts": ["save", "run"],
                "authorized_by": {"message": "u3", "excerpt": SAVE_AND_RUN_WORDS}}),
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_AND_RUN),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
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
        preparing: false,
        acts: vec![Act::While(START_A, HELD, During::Steer(STEER_B))],
        script: Script {
            agent: vec![
                call("models", json!({"role": "run"})),
                hold(HELD),
                call("models", json!({"role": "author"})),
                say(STEERED),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
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
        preparing: false,
        acts: vec![Act::While(START_A, HELD, During::FollowUp(THEN_C))],
        script: Script {
            agent: vec![
                hold(HELD),
                call("models", json!({"role": "chef"})),
                say(FIRST_DONE),
                say(FOLLOWED),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
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
        preparing: false,
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
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The person's plain refusal of the plan.
pub(crate) const REFUSE: &str = "non";
/// E1's request: one source named, one ambiguous, the output left to the author.
pub(crate) const YC_REQUEST: &str = "recupere les news tech de Y Combinator et de TechCrunch, resume les et ecris le resultat en markdown dans un dossier du projet";
/// E1 machine-1 turn 1: a deflection of the question on Y Combinator.
pub(crate) const DEFLECT: &str = "Je t'ai déjà donné les sources.";
pub(crate) const YC_BLOG: &str = "https://www.ycombinator.com/blog/";
/// E1 machine-2 turns 3 and 4: a rename, then the name left to the author.
pub(crate) const RENAME: &str = "Oui, garde le même résultat, mais change le nom.";
pub(crate) const BEST: &str = "Oui, fais au mieux.";
pub(crate) const RENAMED: &str = "./news/actualites-tech.md";
/// E1 TUI turn 5: « today », which names no source.
pub(crate) const TODAY: &str = "En fait je voulais les articles d'aujourd'hui.";
/// A replacement in words that name what they replace.
pub(crate) const SWAP: &str = "change hacker news pour algolia";
pub(crate) const ALGOLIA: &str = "https://hn.algolia.com/api/v1/search_by_date?tags=story";
/// E1 TUI turn 3: the folder, typed at a proposal.
pub(crate) const FOLDER: &str = "Mets ça dans le dossier actualites.";
pub(crate) const FOLDER_DIGEST: &str = "./actualites/digest.md";
/// The engine's detail for E1 machine-2's failed summary.
pub(crate) const TIMED_OUT: &str = "NIKA-INFER-001 provider call failed during `infer`: provider API error (408): HTTP request timed out after 30000ms";
/// The person's repair words, after a failed Run and after an empty one.
pub(crate) const FIX_AND_RUN: &str = "corrige et relance";
pub(crate) const EMPTY: &str = "Il n'y a rien dedans, corrige et relance.";

/// The question E1 asked: which source « Y Combinator » means, Hacker News recommended.
fn yc_question() -> Value {
    json!({"questions": [{"key": "yc_source", "role": "read_source",
        "question": "Pour « Y Combinator », quelle source d'actualités veux-tu ?",
        "options": [
            {"key": "hackernews", "label": "Hacker News", "recommended": true,
             "values": [{"role": "read_source", "value": HACKER_NEWS, "name": "Hacker News"}]},
            {"key": "yc_blog", "label": "Le blog YC",
             "values": [{"role": "read_source", "value": YC_BLOG, "name": "Le blog YC"}]}],
        "free_text": true}]})
}

/// F2: the person refuses the plan; the author's claim that they accepted it is refused.
/// A model that only reasons, twice: asked once more, then the turn says so.
fn thought_only() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(REQUEST)],
        script: Script {
            agent: vec![
                think("Quelles sources en premier ?"),
                think("Toujours rien à dire."),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

fn refused_offer() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(REQUEST), Act::Submit(REFUSE)],
        script: Script {
            agent: vec![
                call("ask", plan_offer(&tech())),
                call("candidate_write", accepted_digest()),
                say("D'accord, je cherche autre chose."),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// Item 4(a), E1 machine-1: a deflection takes the recommended Hacker News, delegated.
fn deflection_takes_the_recommendation() -> Scenario {
    let write = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [
            resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch"),
            resolution(DIGEST, "derived", "output_path", "u1",
                "ecris le resultat en markdown dans un dossier du projet"),
        ],
        "summary": "Hacker News et TechCrunch, résumé dans ./news/digest.md"
    });
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(YC_REQUEST), Act::Submit(DEFLECT)],
        script: Script {
            agent: vec![
                call("ask", yc_question()),
                call("candidate_write", write),
                call_saying(
                    "Je garde mon conseil : Hacker News et TechCrunch.",
                    "propose",
                    json!({}),
                ),
            ],
            replies: Vec::new(),
            picks: vec!["DELEGATE".to_owned()],
            verdicts: Vec::new(),
        },
    }
}

/// Item 4(b), E1 machine-2: a name left to the author is chosen once and never asked again.
fn delegated_name() -> Scenario {
    let name = json!({"questions": [{"key": "new_name", "role": "output_path",
        "question": "Quel nouveau nom pour le fichier ?", "options": [],
        "reopens": {"message": "u3", "excerpt": "change le nom"}}]});
    let renamed = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], RENAMED),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            resolution(RENAMED, "derived", "output_path", "u4", "fais au mieux"),
        ],
        "summary": "le résumé va dans ./news/actualites-tech.md"
    });
    let mut agent = accepted_steps();
    agent.extend([
        call("ask", name.clone()),
        call("ask", name),
        call("candidate_write", renamed),
        call_saying(
            "J'ai choisi ./news/actualites-tech.md ; dis-moi si tu préfères un autre nom.",
            "propose",
            json!({}),
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(RENAME),
            Act::Submit(BEST),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: vec!["recommended".to_owned(), "DELEGATE".to_owned()],
            verdicts: Vec::new(),
        },
    }
}

/// Item 4(e) and lane B's T3, E1 TUI: « today » names no source and removes none; a
/// replacement in words that name the source is said under the proposal.
fn source_swap() -> Scenario {
    let swapped = |message: &str, excerpt: &str| {
        json!({
            "source": digest(&[("hacker_news", ALGOLIA), ("techcrunch", TECHCRUNCH)], DIGEST),
            "resolutions": [
                resolution(ALGOLIA, "named", "read_source", message, "algolia"),
                retained(TECHCRUNCH, "read_source", "u2"),
                retained(DIGEST, "output_path", "u2"),
            ],
            "removed": [{"value": HACKER_NEWS, "message": message, "excerpt": excerpt}],
            "summary": "Hacker News lu par l'API Algolia"
        })
    };
    let mut agent = accepted_steps();
    agent.extend([
        call(
            "candidate_write",
            swapped("u3", "les articles d'aujourd'hui"),
        ),
        say("Je garde Hacker News ; dis-moi si tu veux en changer."),
        call("candidate_write", swapped("u4", "change hacker news")),
        call_saying(
            "Hacker News est désormais lu par l'API Algolia.",
            "propose",
            json!({}),
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(TODAY),
            Act::Submit(SWAP),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}

/// Item 4(f), E1 TUI: the folder typed at a proposal is a revision, never money input.
fn folder_line() -> Scenario {
    let moved = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], FOLDER_DIGEST),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            resolution(FOLDER_DIGEST, "derived", "output_path", "u3", "dans le dossier actualites"),
        ],
        "summary": "le résumé va dans ./actualites/digest.md"
    });
    let mut agent = accepted_steps();
    agent.extend([
        call("candidate_write", moved),
        call_saying(
            "Le résumé va dans ./actualites/digest.md.",
            "propose",
            json!({}),
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![Act::Turn(REQUEST), Act::Submit(ACCEPT), Act::Submit(FOLDER)],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}

/// The accepted digest with a seven-minute deadline on its summary: the repair of a timeout.
fn repaired_digest() -> Value {
    let source = digest(
        &[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)],
        DIGEST,
    )
    .replacen("  summarize:\n", "  summarize:\n    timeout: 7m\n", 1);
    json!({
        "source": source,
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            retained(DIGEST, "output_path", "u2"),
        ],
        "summary": "summarize a désormais 7 minutes"
    })
}

/// The proposal of the current revision with save and run, on the person's line `message`,
/// shown with the author's words `said`.
fn run_on(said: &str, message: &str, excerpt: &str) -> Step {
    call_saying(
        said,
        "propose",
        json!({"acts": ["save", "run"], "authorized_by": {"message": message, "excerpt": excerpt}}),
    )
}

/// Item 5, E1 machine-2: a failed Run goes back to its author at once; the repaired revision
/// never runs on the words that ran the failed one, and runs on the person's new words.
fn failed_run_repaired() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([
        run_on("C'est enregistré ; je le lance.", "u3", SAVE_AND_RUN_WORDS),
        call("candidate_write", repaired_digest()),
        run_on(
            "summarize a dépassé 30 s : je lui ai donné 7 minutes. Dis-moi si je relance.",
            "u3",
            SAVE_AND_RUN_WORDS,
        ),
        run_on("Je relance la version corrigée.", "u4", FIX_AND_RUN),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_AND_RUN),
            Act::RunEnds(Ran::Failed("summarize", TIMED_OUT)),
            Act::Submit(FIX_AND_RUN),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}

/// Item 5, the evaluator's repair cell: a complaint about a successful Run reaches the author
/// with the run's facts, and the repair runs on those words.
fn complaint_after_success() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([
        run_on("C'est enregistré ; je le lance.", "u3", SAVE_AND_RUN_WORDS),
        call("candidate_write", repaired_digest()),
        run_on(
            "Le résumé était vide : je corrige et je relance.",
            "u4",
            "corrige et relance",
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_AND_RUN),
            Act::RunEnds(Ran::Succeeded),
            Act::Submit(EMPTY),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}
/// When every journey's pages were observed: 2026-10-10 08:00 UTC.
pub(crate) const OBSERVED_AT: u64 = 1_791_619_200_000;
/// The feed the trial journeys read.
pub(crate) const STORIES: &str = "https://feed.example/stories";
/// Its stories dated without an offset: the E1 shape `fromdateiso8601` refuses.
const NO_OFFSET: &str = r#"{"hits": [{"title": "A", "created_at": "2026-10-10T08:00:00"}, {"title": "B", "created_at": "2026-10-10T09:30:00"}]}"#;
/// Its stories as an RSS feed, whose `feed` mode gives each item an RFC 3339 `published`.
const STORIES_RSS: &str = "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Stories</title><item><title>A</title><link>https://feed.example/a</link><pubDate>Sat, 10 Oct 2026 08:00:00 GMT</pubDate></item><item><title>B</title><link>https://feed.example/b</link><pubDate>Sat, 10 Oct 2026 09:30:00 GMT</pubDate></item></channel></rss>";
/// The request of the trial journeys: the feed and the file, both named.
pub(crate) const STORIES_REQUEST: &str =
    "recupere https://feed.example/stories et garde les stories du jour dans ./news/today.json";
/// The request that sends a summary of the feed to a hook.
pub(crate) const SEND_REQUEST: &str = "recupere https://feed.example/stories, resume les et envoie le resume a https://hooks.example/team";
/// A filter that refuses a date without an offset, and its repair.
const OFFSET_BROKEN: &str = "fromjson | [.hits[] | select((.created_at | fromdateiso8601) >= 0)]";
/// The repair keeps the civil date the request means, never inventing an offset.
const OFFSET_FIXED: &str =
    "fromjson | [.hits[] | select(.created_at | startswith(\"2026-10-10\"))]";
/// A filter that reads RFC 822 dates out of an RFC 3339 feed (it keeps nothing), and its repair.
const RFC_822_BROKEN: &str = "[.items[] | select(.published | test(\"^[A-Z][a-z]{2}, \"))]";
const RFC_822_FIXED: &str = "[.items[] | select(.published | startswith(\"2026-10-10\"))]";
/// What the author says once its repaired candidate is shown.
pub(crate) const TRIED: &str = "Voici le workflow corrigé : il garde les stories du jour.";
/// The words a proposal's trial opens with.
pub(crate) const TRIED_ON: &str =
    "tried on the pages observed at 08:00 UTC: https://feed.example/stories";

/// The pages a scenario's trials observe, by address: every other page is none of the journey's.
pub(crate) fn feeds(name: &str) -> Vec<(&'static str, &'static str)> {
    match name {
        "trial_offset" => vec![(STORIES, NO_OFFSET)],
        "trial_kept_nothing" | "trial_names_what_it_skips" | "own_filter" => {
            vec![(STORIES, STORIES_RSS)]
        }
        _ => Vec::new(),
    }
}

/// The stories of the day the jq `filter` keeps of the page read in `mode`, written to
/// `./news/today.json`.
fn stories(mode: &str, filter: &str) -> Value {
    let source = format!(
        "nika: today-stories\npermits:\n  tools: [\"nika:fetch\", \"nika:jq\", \"nika:write\"]\n  net:\n    http: [\"feed.example\"]\n  fs:\n    write: [\"./news/today.json\"]\ntasks:\n  stories:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{STORIES}\", mode: {mode} }}\n  today:\n    with: {{ stories: \"${{{{ tasks.stories.output }}}}\" }}\n    invoke:\n      tool: \"nika:jq\"\n      args: {{ input: \"${{{{ with.stories }}}}\", expression: '{filter}' }}\n  keep:\n    with: {{ kept: \"${{{{ tasks.today.output }}}}\" }}\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"./news/today.json\", content: \"${{{{ with.kept }}}}\" }}\n"
    );
    json!({"source": source, "resolutions": [],
        "summary": "les stories du jour dans ./news/today.json"})
}

/// A candidate whose `broken` filter fails its trial, then its `fixed` revision.
fn tried(mode: &str, broken: &str, fixed: &str) -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(STORIES_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", stories(mode, broken)),
                call("propose", json!({})),
                call("candidate_write", stories(mode, fixed)),
                call_saying(TRIED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The author's own jq filter over the named feed, written and proposed with no selection for it.
fn own_filter() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(STORIES_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", stories("feed", RFC_822_FIXED)),
                call_saying(TRIED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The feed summarized by the session's model and sent to a hook: a trial runs neither.
fn trial_names_what_it_skips() -> Scenario {
    let source = format!(
        "nika: stories-digest\nmodel: {SEAT}\npermits:\n  tools: [\"nika:fetch\"]\n  net:\n    http: [\"feed.example\", \"hooks.example\"]\ntasks:\n  stories:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{STORIES}\", mode: feed }} }}\n  summarize:\n    with: {{ stories: \"${{{{ tasks.stories.output }}}}\" }}\n    infer: {{ prompt: \"Résume ces stories : ${{{{ with.stories }}}}\", max_tokens: 400 }}\n  send:\n    with: {{ digest: \"${{{{ tasks.summarize.output }}}}\" }}\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://hooks.example/team\", method: POST, body: \"${{{{ with.digest }}}}\" }} }}\n"
    );
    let write = json!({"source": source, "resolutions": [],
        "summary": "le résumé des stories envoyé à https://hooks.example/team"});
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(SEND_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", write),
                call_saying(
                    "Voici le workflow : il résume les stories et envoie le résumé.",
                    "propose",
                    json!({}),
                ),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
/// The sales the counting journeys read.
const SALES: &str = "date,montant\n2026-10-01,12\n2026-10-02,30\n";

/// The counting workflow, written for the counting request.
fn count_write() -> Value {
    json!({"source": COUNT, "resolutions": [], "summary": "compte les lignes de ./data/ventes.csv"})
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
        preparing: false,
        acts,
        script: Script {
            agent: vec![
                call("candidate_write", count_write()),
                call_saying(COUNTED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The counting workflow written once over an output path no word of the person names, then
/// over the one they named, then proposed: no `check` or `verify` round trip.
fn write_carries_its_findings() -> Scenario {
    let invented = json!({"source": COUNT.replace("./out/total.txt", "./out/other.txt"),
        "resolutions": [], "summary": "compte les lignes de ./data/ventes.csv"});
    Scenario {
        files: vec![("data/ventes.csv", SALES)],
        history: false,
        preparing: false,
        acts: vec![Act::Turn(COUNT_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", invented),
                call("candidate_write", count_write()),
                call_saying(
                    "Voici le workflow : il compte les lignes de ./data/ventes.csv.",
                    "propose",
                    json!({}),
                ),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
