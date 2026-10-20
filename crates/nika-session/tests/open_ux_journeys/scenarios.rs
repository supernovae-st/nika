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

#[path = "trials.rs"]
mod trials;
pub(crate) use self::trials::{OBSERVED_AT, TRIED_ON, feeds};
use self::trials::{
    OFFSET_BROKEN, OFFSET_FIXED, RFC_822_BROKEN, RFC_822_FIXED, SALES, own_filter, reads_sales,
    trial_assert, trial_names_what_it_skips, tried, two_edits, write_carries_its_findings,
};

#[path = "open_choices.rs"]
mod open_choices;
use self::open_choices::{default_not_asked, open_defaults, open_pick_other};

#[path = "two_outputs.rs"]
mod two_outputs;
pub(crate) use self::two_outputs::{PAYLOAD_NAME, PAYLOAD_PATH, RELIRE, REPORT};

#[path = "schedules.rs"]
mod schedules;
pub(crate) use self::schedules::{COMMENTED, DAILY, WHEN, ZONE};
use self::schedules::{
    copy_request_claimed_as_consent, schedule_activated, schedule_cadence_picked,
    schedule_declared_then_asked, schedule_saved, schedule_saved_then_revised,
    schedule_without_hour, schedule_zone_picked, schedule_zone_revised, schedule_zone_unseen,
};

#[path = "requested_files.rs"]
mod requested_files;
pub(crate) use self::requested_files::BASE;

#[path = "feedbacks.rs"]
mod feedbacks;
pub(crate) use self::feedbacks::{
    BRACKETED, FIRST_REPORT, LATER_IN_WORDS, SECOND_REPORT, TOTAL, UNRELATED,
};

#[path = "byte_changes.rs"]
mod byte_changes;
pub(crate) use self::byte_changes::CLAIMED;

#[path = "dropped_questions.rs"]
mod dropped_questions;
pub(crate) use self::dropped_questions::{DROP, SIMPLE};

#[path = "failing_checks.rs"]
mod failing_checks;
pub(crate) use self::failing_checks::{CLAIMED_RUN, CLAUDE, PLAIN_ANSWER, TEMPERATURE};

#[path = "kept_defaults.rs"]
mod kept_defaults;
pub(crate) use self::kept_defaults::PARIS;

#[path = "session_facts.rs"]
mod session_facts;
pub(crate) use self::session_facts::{DETAILS, HOW_MUCH};

#[path = "run_ceilings.rs"]
mod run_ceilings;
pub(crate) use self::run_ceilings::{
    CEILING_ALONE, LANCE, PICKED, RELANCE, RUN_COST, SAVE_AND_RUN_ONCE,
};

/// The Session's reading of the sentence that accepts the plan: its recommended option.
/// The Session's reading of each cited consent the journey's author claims: save and run.
fn consented(reads: usize) -> Vec<String> {
    vec!["save_and_run".to_owned(); reads]
}

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
/// A genuine line after the proposal that asks for an explanation and consents to nothing.
pub(crate) const EXPLAIN: &str = "explique-moi comment il choisit les articles";
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
    /// A consent typed at the proposal's own door, as a host's consent control sends it, whatever
    /// else waits beside it: `consent(line)`.
    Consent(&'static str),
    /// The question that waited after act `n`, answered by its identity.
    AnswerQuestionOf(usize, &'static str),
    /// One choice from the proposal shown after act `n`, answered under both identities.
    AnswerProposalChoiceOf(usize, usize, &'static str),
    /// Those proposal and question identities presented to another Session.
    AnswerProposalChoiceElsewhere(usize, usize, &'static str),
    /// One choice from the proposal shown after act `n` left to Nika by the host's own
    /// control (« Let Nika choose »), under both identities.
    DelegateProposalChoiceOf(usize, usize),
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
    /// The optional offer about the last result waiting now, answered by its identity with
    /// the keys a host's native control sends.
    AnswerFeedback(&'static str),
    /// The offer shown after act `n`, answered by that identity (closed or not since).
    AnswerFeedbackOf(usize, &'static str),
    /// A host asks the review of a run, as its Run control does (`run_review_asked`).
    ReviewRun,
}

/// How a run the child stands in for ended.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Ran {
    /// Every task ran and the workflow completed.
    Succeeded,
    /// The task failed with the engine's detail (« CODE message »).
    Failed(&'static str, &'static str),
    /// The workflow the Session requested completed writing this content at this path.
    Wrote(&'static str, &'static str),
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

/// The author's one recommendation for the ordinary request, as a choice a proposal shows with
/// its default: the sources and the file the candidate already carries.
pub(crate) fn plan_choice(sources: &[(&str, &str)]) -> Value {
    let values: Vec<Value> = sources
        .iter()
        .map(|(name, url)| json!({"role": "read_source", "value": url, "name": name}))
        .chain(std::iter::once(
            json!({"role": "output_path", "value": DIGEST}),
        ))
        .collect();
    let names: Vec<&str> = sources.iter().map(|(name, _)| *name).collect();
    json!({
        "key": "plan",
        "question": format!("Je prends {} et j'écris le résumé dans {DIGEST} : ça te va ?", names.join(", ")),
        "why": "La demande ne nomme ni les sources ni le fichier de sortie.",
        "options": [
            {"key": "recommended", "label": format!("Oui : {} → {DIGEST}", names.join(", ")),
             "recommended": true, "values": values},
            {"key": "other", "label": "D'autres sources ou un autre fichier"}
        ],
        "free_text": true
    })
}

/// The sources asked as a question that recommends none: the journeys that prove how the
/// Session handles a question and its answers ask it.
pub(crate) fn sources_question(sources: &[(&str, &str)]) -> Value {
    let values: Vec<Value> = sources
        .iter()
        .map(|(name, url)| json!({"role": "read_source", "value": url, "name": name}))
        .collect();
    let names: Vec<&str> = sources.iter().map(|(name, _)| *name).collect();
    json!({"questions": [{
        "key": "plan",
        "question": "Quelles sources d'actualités tech veux-tu ?",
        "why": "La demande ne nomme ni les sources ni le fichier de sortie.",
        "options": [
            {"key": "sources", "label": names.join(", "), "values": values},
            {"key": "other", "label": "D'autres sources"}
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

/// The tech digest with the plan's defaults written and no selection stated: the proposal shows
/// them as its open choice.
fn defaults_digest() -> Value {
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [],
        "summary": "Hacker News et TechCrunch, résumé dans ./news/digest.md"
    })
}

/// The first two turns every accepting journey shares: the proposal with the plan open at once,
/// then, once the person's line kept its default, the candidate with the values it bound.
fn accepted_steps() -> Vec<Step> {
    vec![
        call("candidate_write", defaults_digest()),
        call_saying(
            "Je propose Hacker News et TechCrunch, résumé dans ./news/digest.md.",
            "propose",
            json!({"open": [plan_choice(&tech())]}),
        ),
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
        name if name.starts_with("proposal_choice_") => super::proposal_choices::scenario(name),
        name if name.starts_with("feedback_") => feedbacks::scenario(name),
        "accept_offer" => accept_offer(),
        "meaning_of_a_proposal" => meaning_of_a_proposal(),
        "discard_then_fix" => discard_then_fix(),
        "saved_bytes_again" => saved_bytes_again(),
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
        "explanation_cited_as_consent" => explanation_cited_as_consent(),
        "steer_mid_run" => steer_mid_run(),
        "steer_before_proposal" => steer_before_proposal(),
        "written_not_proposed" => written_not_proposed(),
        "oversized_read" => trials::oversized_read(),
        "edit_after_refused_write" => trials::edit_after_refused_write(),
        "taught_assert" => trials::taught_assert(),
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
        "trial_assert" => trial_assert(),
        "trial_names_what_it_skips" => trial_names_what_it_skips(),
        "own_filter" => own_filter(),
        "two_edits" => two_edits(),
        "reads_a_project_file" => reads_sales(false),
        name if name.starts_with("requested_file_") || name.starts_with("component_read") => {
            requested_files::scenario(name)
        }
        "a_read_file_changes" => reads_sales(true),
        "write_carries_its_findings" => write_carries_its_findings(),
        "two_outputs_answered" => two_outputs::two_outputs_answered(),
        "two_outputs_typed" => two_outputs::two_outputs_typed(),
        "two_outputs_typed_elsewhere" => two_outputs::two_outputs_typed_elsewhere(),
        "two_outputs_typed_rebound" => two_outputs::two_outputs_typed_rebound(),
        "two_outputs_typed_restated" => two_outputs::two_outputs_typed_restated(),
        "thought_only" => thought_only(),
        "open_defaults" => open_defaults(),
        "open_pick_other" => open_pick_other(),
        "consent_bytes_changed" => consent_bytes_changed(),
        "change_then_run" => change_then_run(),
        "default_not_asked" => default_not_asked(),
        "answered_literal" => answered_literal(),
        "schedule_saved" => schedule_saved(),
        "copy_request_claimed_as_consent" => copy_request_claimed_as_consent(),
        "schedule_activated" => schedule_activated(),
        "schedule_declared_then_asked" => schedule_declared_then_asked(),
        "schedule_zone_unseen" => schedule_zone_unseen(),
        "schedule_without_hour" => schedule_without_hour(),
        "schedule_cadence_picked" => schedule_cadence_picked(),
        "schedule_zone_picked" => schedule_zone_picked(),
        "schedule_zone_revised" => schedule_zone_revised(),
        "schedule_saved_then_revised" => schedule_saved_then_revised(),
        "ceiling_restored" => run_ceilings::ceiling_restored(),
        "ceiling_reopened" => run_ceilings::ceiling_reopened(),
        "ceiling_kept_by_revision" => run_ceilings::ceiling_kept_by_revision(),
        "ceiling_kept_by_a_revision" => run_ceilings::ceiling_kept_by_a_revision(),
        "ceiling_kept_by_a_revision_after_a_run" => {
            run_ceilings::ceiling_kept_by_a_revision_after_a_run()
        }
        "ceiling_in_request" => run_ceilings::ceiling_in_request(),
        "ceiling_for_the_pending" => run_ceilings::ceiling_for_the_pending(),
        "same_bytes_again" => byte_changes::same_bytes_again(),
        "question_dropped" => dropped_questions::question_dropped(),
        "cancel_offered" => dropped_questions::cancel_offered(),
        "details_after_a_run" => session_facts::details_after_a_run(),
        "spend_asked" => session_facts::spend_asked(),
        "failing_check" => failing_checks::failing_check(),
        "claimed_after_refusal" => failing_checks::claimed_after_refusal(),
        "claimed_beside_a_pending_proposal" => failing_checks::claimed_beside_a_pending_proposal(),
        "kept_defaults" => kept_defaults::kept_defaults(),
        "kept_defaults_other_zone" => kept_defaults::kept_defaults_other_zone(),
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
            Act::SubmitAsShownAfter(0, "oui"),
        ],
        script: Script {
            agent: accepted_steps(),
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}

/// `/meaning` typed beside the proposal the accepted offer built.
fn meaning_of_a_proposal() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit("/meaning"),
        ],
        script: Script {
            agent: accepted_steps(),
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}

/// The words that save the proposal, read by the author.
pub(crate) const SAVE_IT: &str = "parfait, enregistre-le";
/// A later fix asked after a revision was discarded.
pub(crate) const FIX_AFTER_DISCARD: &str = "corrige juste la consigne du resume, sans rien ajouter";

/// The accepted digest saved on the person's words, a revision adding Le Monde discarded with
/// « non », then a fix: the author reads the candidate, which is the saved workflow again.
fn discard_then_fix() -> Scenario {
    let le_monde = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH),
            ("le_monde", LE_MONDE)], DIGEST),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            retained(DIGEST, "output_path", "u2"),
            resolution(LE_MONDE, "named", "read_source", "u4", LE_MONDE_WORDS),
        ],
        "summary": "Le Monde international ajouté"
    });
    let mut agent = accepted_steps();
    agent.extend([
        call_saying(
            "C'est enregistré.",
            "propose",
            json!({"acts": ["save"], "authorized_by": {"message": "u3", "excerpt": "enregistre-le"}}),
        ),
        call("candidate_write", le_monde),
        call_saying("J'ai ajouté Le Monde international.", "propose", json!({})),
        call("candidate_read", json!({})),
        say("Je repars du workflow enregistré."),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_IT),
            Act::Turn(ALSO_LE_MONDE),
            Act::Submit("non"),
            Act::Turn(FIX_AFTER_DISCARD),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: [accepted_pick(), vec!["save".to_owned()]].concat(),
            verdicts: Vec::new(),
        },
    }
}

/// A line after the save the author answers with the saved workflow's very bytes.
pub(crate) const AFTER_SAVE: &str = "relis-le et dis-moi si tout est bon";

/// The accepted digest saved on the person's words, then proposed again byte for byte.
fn saved_bytes_again() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([
        call_saying(
            "C'est enregistré.",
            "propose",
            json!({"acts": ["save"], "authorized_by": {"message": "u3", "excerpt": "enregistre-le"}}),
        ),
        call("candidate_write", accepted_digest()),
        call_saying("Tout est bon, voici le workflow.", "propose", json!({})),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_IT),
            Act::Turn(AFTER_SAVE),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: [accepted_pick(), vec!["save".to_owned()]].concat(),
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
                call("ask", sources_question(&tech())),
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
                call("ask", sources_question(&tech())),
                call_saying(
                    "La demande ne nomme ni les sources ni le fichier de sortie : voici ce que je propose.",
                    "ask",
                    sources_question(&tech()),
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
                call("ask", sources_question(&tech())),
                call("ask", sources_question(&le_monde)),
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
            picks: [accepted_pick(), consented(1)].concat(),
            verdicts: Vec::new(),
        },
    }
}

/// The person asks how the workflow picks its articles; the author cites that real, later line as
/// if it consented to saving and running the proposal.
fn explanation_cited_as_consent() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([call_saying(
        "Je l'enregistre et je le lance.",
        "propose",
        json!({"acts": ["save", "run"],
                "authorized_by": {"message": "u3", "excerpt": "explique-moi comment"}}),
    )]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(EXPLAIN),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: [accepted_pick(), vec!["NONE".to_owned()]].concat(),
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
/// A request with one source named, one ambiguous, and the output left to the author.
pub(crate) const YC_REQUEST: &str = "recupere les news tech de Y Combinator et de TechCrunch, resume les et ecris le resultat en markdown dans un dossier du projet";
/// A deflection of the question on Y Combinator.
pub(crate) const DEFLECT: &str = "Je t'ai déjà donné les sources.";
pub(crate) const YC_BLOG: &str = "https://www.ycombinator.com/blog/";
/// A rename, then the name left to the author.
pub(crate) const RENAME: &str = "Oui, garde le même résultat, mais change le nom.";
pub(crate) const BEST: &str = "Oui, fais au mieux.";
pub(crate) const RENAMED: &str = "./news/actualites-tech.md";
/// « Today », which names no source.
pub(crate) const TODAY: &str = "En fait je voulais les articles d'aujourd'hui.";
/// A replacement in words that name what they replace.
pub(crate) const SWAP: &str = "change hacker news pour algolia";
pub(crate) const ALGOLIA: &str = "https://hn.algolia.com/api/v1/search_by_date?tags=story";
/// The folder, typed at a proposal.
pub(crate) const FOLDER: &str = "Mets ça dans le dossier actualites.";
pub(crate) const FOLDER_DIGEST: &str = "./actualites/digest.md";
/// The engine's detail for a summary whose provider call timed out.
pub(crate) const TIMED_OUT: &str = "NIKA-INFER-001 provider call failed during `infer`: provider API error (408): HTTP request timed out after 30000ms";
/// The person's repair words, after a failed Run and after an empty one.
pub(crate) const FIX_AND_RUN: &str = "corrige et relance";
pub(crate) const EMPTY: &str = "Il n'y a rien dedans, corrige et relance.";

/// The choice of which source « Y Combinator » means, shown by the proposal with
/// Hacker News its default.
fn yc_choice() -> Value {
    json!({"key": "yc_source", "role": "read_source",
        "question": "Pour « Y Combinator », quelle source d'actualités veux-tu ?",
        "options": [
            {"key": "hackernews", "label": "Hacker News", "recommended": true,
             "values": [{"role": "read_source", "value": HACKER_NEWS, "name": "Hacker News"}]},
            {"key": "yc_blog", "label": "Le blog YC",
             "values": [{"role": "read_source", "value": YC_BLOG, "name": "Le blog YC"}]}],
        "free_text": true})
}

/// The person refuses the plan; the author's claim that they accepted it is refused.
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
                call("ask", sources_question(&tech())),
                call("candidate_write", accepted_digest()),
                say("D'accord, je cherche autre chose."),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// A deflection takes the recommended Hacker News, delegated.
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
                call("candidate_write", write.clone()),
                call_saying(
                    "Je prends Hacker News pour Y Combinator, et TechCrunch.",
                    "propose",
                    json!({"open": [yc_choice()]}),
                ),
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

/// A name left to the author is chosen once and never asked again.
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

/// « Today » names no source and removes none; a
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

/// The folder typed at a proposal is a revision, never money input.
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

/// A failed Run goes back to its author at once; the repaired revision
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
            picks: [accepted_pick(), consented(2)].concat(),
            verdicts: Vec::new(),
        },
    }
}

/// A complaint about a successful Run reaches the author
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
            // « corrige et relance » asks for a change, then the run: the repair answers it.
            picks: [
                accepted_pick(),
                consented(1),
                vec![CHANGE_THEN_RUN_PICK.to_owned()],
            ]
            .concat(),
            verdicts: Vec::new(),
        },
    }
}

/// A count over the lines that carry the person's customer code, a fact only they have.
pub(crate) const CODE_REQUEST: &str = "compte les lignes de ./data/ventes.csv qui portent mon code client et ecris le total dans ./out/total.txt";
/// The person's customer code, their answer.
pub(crate) const CODE: &str = "C-1042";
/// The sales, two lines with the person's code.
const CODED_SALES: &str =
    "date,client,montant\n2026-10-01,C-1042,12\n2026-10-02,C-7,30\n2026-10-03,C-1042,8\n";

/// The person's customer code, asked: no default can stand for it.
fn code_question() -> Value {
    json!({"questions": [{"key": "client_code", "role": "value",
        "question": "Quel est ton code client ?", "free_text": true}]})
}

/// The count of the lines that carry the answered code.
fn coded_count() -> Value {
    let filter = format!("map(select(contains(\\\"{CODE}\\\")))");
    json!({"source": COUNT.replace("map(select(length > 0))", &filter), "resolutions": [],
        "summary": "compte les lignes qui portent C-1042"})
}

/// The code asked, answered, bound by its question and written into the filter; the judge
/// doubts the request unless it reads that answer.
fn answered_literal() -> Scenario {
    let bind = json!({"answered": [{"key": "client_code", "value": CODE, "message": "u2",
        "excerpt": CODE}]});
    Scenario {
        files: vec![("data/ventes.csv", CODED_SALES)],
        history: false,
        preparing: false,
        acts: vec![Act::Turn(CODE_REQUEST), Act::Submit(CODE)],
        script: Script {
            agent: vec![
                call("ask", code_question()),
                call("ask", bind),
                call("candidate_write", coded_count()),
                call_saying(
                    "Voici le workflow : il compte les lignes qui portent C-1042.",
                    "propose",
                    json!({}),
                ),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: vec![
                "unless-answered:client_code".to_owned(),
                "unexercised".to_owned(),
            ],
        },
    }
}

/// A correction steered while the author prepares its proposal: its own filter, no path.
pub(crate) const STEER_EMPTY: &str = "non, compte aussi les lignes vides";

/// The count written, then, while the author's next request is held, the person steers its
/// filter: the `propose` that answers is held back, and the author proposes the revision.
fn steer_before_proposal() -> Scenario {
    let count = json!({"source": COUNT, "resolutions": [],
        "summary": "compte les lignes de ./data/ventes.csv"});
    let moved = json!({"source": COUNT.replace("map(select(length > 0)) | ", ""),
        "resolutions": [], "summary": "compte aussi les lignes vides"});
    Scenario {
        files: vec![("data/ventes.csv", SALES)],
        history: false,
        preparing: false,
        acts: vec![Act::While(COUNT_REQUEST, HELD, During::Steer(STEER_EMPTY))],
        script: Script {
            agent: vec![
                call("candidate_write", count),
                hold(HELD),
                call("propose", json!({})),
                call("candidate_write", moved),
                call_saying(
                    "Voici le workflow : il compte aussi les lignes vides.",
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

/// The author changes what the summary says, not what the workflow may do, then claims the
/// person's consent to the revision they saw covers the new bytes.
fn consent_bytes_changed() -> Scenario {
    let source = digest(
        &[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)],
        DIGEST,
    );
    let english = json!({
        "source": source.replace("Résume en Markdown", "Résume en anglais et en Markdown"),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            retained(DIGEST, "output_path", "u2"),
        ],
        "summary": "le résumé est en anglais"
    });
    let mut agent = accepted_steps();
    agent.extend([
        call("candidate_write", english),
        call_saying(
            "Je l'enregistre et je le lance.",
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
            picks: [accepted_pick(), consented(1)].concat(),
            verdicts: Vec::new(),
        },
    }
}

/// The reading of words that ask for a change, then the save and the run.
const CHANGE_THEN_RUN_PICK: &str = "change_then_save_and_run";
/// A change asked with its save and run.
pub(crate) const CHANGE_THEN_RUN: &str = "mets le resume en anglais puis enregistre-le et lance-le";

/// The person asks for a change, then the save and the run: the author writes the revision in
/// answer to that line and proposes it with the acts, citing the same line.
fn change_then_run() -> Scenario {
    let source = digest(
        &[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)],
        DIGEST,
    );
    let english = json!({
        "source": source.replace("Résume en Markdown", "Résume en anglais et en Markdown"),
        "resolutions": [
            retained(HACKER_NEWS, "read_source", "u2"),
            retained(TECHCRUNCH, "read_source", "u2"),
            retained(DIGEST, "output_path", "u2"),
        ],
        "summary": "le résumé est en anglais"
    });
    let mut agent = accepted_steps();
    agent.extend([
        call("candidate_write", english),
        call_saying(
            "Je l'ai mis en anglais ; je l'enregistre et je le lance.",
            "propose",
            json!({"acts": ["save", "run"],
                "authorized_by": {"message": "u3", "excerpt": "puis enregistre-le et lance-le"}}),
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(CHANGE_THEN_RUN),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: [accepted_pick(), vec![CHANGE_THEN_RUN_PICK.to_owned()]].concat(),
            verdicts: Vec::new(),
        },
    }
}

/// What the author says when it ends a turn over a candidate it wrote and did not propose.
const UNPROPOSED_WORDS: &str = "J'ai écrit le workflow qui compte les lignes.";

/// The count written, then the author's turn ends in words: Nika asks it once to end the turn
/// with a proposal, a question or a refusal, and it proposes.
fn written_not_proposed() -> Scenario {
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
                say(UNPROPOSED_WORDS),
                call_saying(
                    "Voici le workflow : il compte les lignes.",
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
