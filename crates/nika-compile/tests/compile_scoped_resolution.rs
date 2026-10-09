// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A value the human did not type can still be authorized: a public source the request names
//! (« Hacker News »), public sources the human delegated (« tu les choisis toi-même ») and a
//! routine new output inside the project are concrete selections within the request's own scope.
//! The author states each one with its provenance and the words that authorize it; the compile
//! door judges the complete document as any candidate, records the provenance it admitted, and
//! never files a selection as a human answer. The scope stays narrow: a delegated source is a
//! public address read with GET, never a private address, never a POST; an authorization needs
//! the human's own words; a derived output never replaces a file the project already holds; and
//! a selection stated with no provenance is still an invented literal.
//!
//! SCRIPTED hermetic doubles only: `Author` answers fixed documents in order and records every
//! authoring call; the verifier's whole-request judge is approved (`common::Judged`). A scripted
//! answer is not a model's: these tests prove the door, its laws and its records, never a model's
//! ability to choose sources.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::Judged;
use nika_compile::surface::literal_projection;
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode};
use nika_compile_cognition::{Cognition, compile_with_cognition_composed};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, Role, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::Duration;

const HACKER_NEWS: &str = "https://news.ycombinator.com";
const TECHCRUNCH: &str = "https://techcrunch.com";
const LE_MONDE: &str = "https://www.lemonde.fr/international/";
const DIGEST: &str = "./news/digest.md";

/// Sources the request names, an output it leaves to the author.
const NAMED: &str = "Fais un workflow très simple qui récupère les news tech récentes de Hacker News et de TechCrunch, les résume, et écrit le résumé en Markdown dans un fichier du projet.";
/// The words of `NAMED` that ask for a new Markdown file inside the project.
const NAMED_OUTPUT: &str = "écrit le résumé en Markdown dans un fichier du projet";

/// Sources the human delegates, an output it leaves to the author.
const DELEGATED: &str = "Récupère les news tech et géopolitiques récentes, résume-les et écris le résumé en Markdown dans un dossier du projet. Les sources publiques, tu les choisis toi-même.";
/// The words of `DELEGATED` that delegate the choice of the public sources.
const DELEGATION: &str = "Les sources publiques, tu les choisis toi-même";
/// The words of `DELEGATED` that ask for a new Markdown file inside the project.
const DELEGATED_OUTPUT: &str = "écris le résumé en Markdown dans un dossier du projet";

/// Every literal stated by the request itself: the existing forms, no selection to declare.
const STATED: &str =
    "Résume les news de https://news.ycombinator.com et écris le résumé dans ./news/digest.md.";

/// One authoring call as the scripted author received it.
struct Call {
    last: String,
}

/// A scripted author: the documents it answers, in order, and every authoring call it received.
/// A call past the script fails as a provider would.
struct Author {
    answers: Vec<String>,
    calls: Mutex<Vec<Call>>,
}

impl Author {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// The last message of the authoring call `at`: a repair names what the laws refused.
    fn last(&self, at: usize) -> String {
        self.calls.lock().unwrap()[at].last.clone()
    }
}

fn text(content: &[ContentBlock]) -> String {
    (content.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let last = (request.messages.iter())
            .rev()
            .find(|m| !matches!(m.role, Role::System))
            .map_or_else(String::new, |m| text(&m.content));
        let at = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(Call { last });
            calls.len() - 1
        };
        match self.answers.get(at) {
            Some(answer) => Ok(InferResponse::new(
                vec![ContentBlock::Text {
                    text: answer.clone(),
                }],
                TokenUsage::new(300, 200),
                StopReason::EndTurn,
            )),
            None => Err(ProviderError::Other {
                reason: "the scripted author has no further answer".to_owned(),
            }),
        }
    }
}

/// The document door's answer: the whole source and the selections it states, each with its
/// provenance and the words that authorize it.
fn answer(source: &str, resolutions: &Value) -> String {
    json!({"candidate": source, "candidate_lines": [], "operations": [], "questions": [],
        "gaps": [], "notes": "scripted", "resolutions": resolutions})
    .to_string()
}

/// The same document with no selection stated: the door's answer as it is written today.
fn undeclared(source: &str) -> String {
    json!({"candidate": source, "candidate_lines": [], "operations": [], "questions": [],
        "gaps": [], "notes": "scripted"})
    .to_string()
}

/// One stated selection.
fn resolution(value: &str, kind: &str, role: &str, excerpt: &str) -> Value {
    json!({"value": value, "kind": kind, "role": role, "excerpt": excerpt})
}

/// The host an address reaches.
fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

/// A digest workflow: one `nika:fetch` per source (with `method`), one summary, one write.
fn digest_with(sources: &[(&str, &str)], method: &str, output: &str) -> String {
    let hosts: Vec<String> = sources
        .iter()
        .map(|(_, url)| format!("\"{}\"", host(url)))
        .collect();
    let mut source = format!(
        "nika: news-digest\nmodel: mock/echo\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net:\n    http: [{}]\n  fs:\n    write: [\"{output}\"]\ntasks:\n",
        hosts.join(", ")
    );
    for (task, url) in sources {
        write!(
            source,
            "  {task}:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{url}\", method: {method} }}\n"
        )
        .unwrap();
    }
    source.push_str("  summarize:\n    with:\n");
    for (task, _) in sources {
        writeln!(source, "      {task}: \"${{{{ tasks.{task}.output }}}}\"").unwrap();
    }
    let read: Vec<String> = sources
        .iter()
        .map(|(task, _)| format!("${{{{ with.{task} }}}}"))
        .collect();
    write!(
        source,
        "    infer:\n      max_tokens: 1000\n      prompt: \"Résume en Markdown les actualités ci-dessous, sans rien inventer : {}\"\n",
        read.join(" ")
    )
    .unwrap();
    write!(
        source,
        "  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{output}\", content: \"${{{{ with.digest }}}}\" }}\n"
    )
    .unwrap();
    source
}

fn digest(sources: &[(&str, &str)]) -> String {
    digest_with(sources, "GET", DIGEST)
}

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 8192, Duration::from_secs(5))
        .with_native(NativeMode::Escalate)
}

/// The request as a host sends it: the human's words, the workflow's model already chosen (the
/// model is not what these tests judge), the session's authoring policy.
fn request(intent: &str) -> CompileRequest {
    CompileRequest::create(intent)
        .answer("model", "\"mock/echo\"")
        .with_authoring_policy(policy())
}

async fn create(request: &CompileRequest, author: &Author) -> CompileOutcome {
    let judged = Judged::approving(author);
    let cognition = Cognition {
        provider: Some(&judged),
        seat: None,
    };
    compile_with_cognition_composed(request, cognition, None, None)
        .await
        .unwrap()
}

/// Every finding of the outcome, one per line.
fn findings(out: &CompileOutcome) -> String {
    (out.diagnostics.iter())
        .map(|d| format!("{}: {}", d.target, d.message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The selections the outcome's record admitted, by value: `(kind, role, excerpt)`.
fn recorded(out: &CompileOutcome) -> Vec<(String, String, String, String)> {
    let plan = out.provenance.plan.as_ref().expect("the door's record");
    let rows = plan["resolutions"]
        .as_array()
        .unwrap_or_else(|| panic!("the record states the admitted selections: {plan:#}"));
    rows.iter()
        .map(|row| {
            let field = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
            (
                field("value"),
                field("kind"),
                field("role"),
                field("excerpt"),
            )
        })
        .collect()
}

/// The literal projection of the READY candidate.
fn ready(out: &CompileOutcome) -> Value {
    assert_eq!(
        out.status,
        CompileStatus::Ready,
        "the candidate is READY: {}",
        findings(out)
    );
    let candidate = out.candidate.as_deref().expect("a candidate");
    literal_projection(candidate).expect("a literal document")
}

fn sorted(value: &Value) -> Vec<String> {
    let mut items: Vec<String> = (value.as_array().into_iter().flatten())
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    items.sort();
    items
}

/// Whether some finding of the outcome names every one of `words`.
fn named_by_a_finding(out: &CompileOutcome, words: &[&str]) -> bool {
    out.diagnostics
        .iter()
        .any(|d| words.iter().all(|w| d.message.contains(w)))
}

#[tokio::test]
async fn named_public_sources_and_a_routine_output_resolve_with_their_provenance() {
    let source = digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)]);
    let stated = json!([
        resolution(HACKER_NEWS, "named", "read_source", "Hacker News"),
        resolution(TECHCRUNCH, "named", "read_source", "TechCrunch"),
        resolution(DIGEST, "derived", "output_path", NAMED_OUTPUT),
    ]);
    let author = Author::new(vec![answer(&source, &stated)]);
    let out = create(&request(NAMED), &author).await;
    let doc = ready(&out);
    assert_eq!(author.count(), 1, "no repair round: {}", findings(&out));
    assert!(
        out.questions.is_empty(),
        "no address or file is asked: {:#?}",
        out.questions
    );
    assert!(
        !findings(&out).contains("INVENTED LITERAL"),
        "{}",
        findings(&out)
    );
    // The exact selections are materialized in the candidate and its boundary.
    assert_eq!(
        doc["tasks"]["hacker_news"]["invoke"]["args"]["url"],
        HACKER_NEWS
    );
    assert_eq!(
        doc["tasks"]["techcrunch"]["invoke"]["args"]["url"],
        TECHCRUNCH
    );
    assert_eq!(
        sorted(&doc["permits"]["net"]["http"]),
        ["news.ycombinator.com", "techcrunch.com"]
    );
    assert_eq!(sorted(&doc["permits"]["fs"]["write"]), [DIGEST]);
    // Each selection keeps its real provenance: named or derived, never a human answer.
    let mut admitted = recorded(&out);
    admitted.sort();
    assert_eq!(
        admitted,
        [
            (
                DIGEST.to_owned(),
                "derived".to_owned(),
                "output_path".to_owned(),
                NAMED_OUTPUT.to_owned()
            ),
            (
                HACKER_NEWS.to_owned(),
                "named".to_owned(),
                "read_source".to_owned(),
                "Hacker News".to_owned()
            ),
            (
                TECHCRUNCH.to_owned(),
                "named".to_owned(),
                "read_source".to_owned(),
                "TechCrunch".to_owned()
            ),
        ]
    );
}

#[tokio::test]
async fn delegated_public_sources_resolve_within_the_delegation() {
    let source = digest(&[
        ("hacker_news", HACKER_NEWS),
        ("techcrunch", TECHCRUNCH),
        ("le_monde", LE_MONDE),
    ]);
    let stated = json!([
        resolution(HACKER_NEWS, "delegated", "read_source", DELEGATION),
        resolution(TECHCRUNCH, "delegated", "read_source", DELEGATION),
        resolution(LE_MONDE, "delegated", "read_source", DELEGATION),
        resolution(DIGEST, "derived", "output_path", DELEGATED_OUTPUT),
    ]);
    let author = Author::new(vec![answer(&source, &stated)]);
    let out = create(&request(DELEGATED), &author).await;
    let doc = ready(&out);
    assert!(
        out.questions.is_empty(),
        "a delegated choice is not a questionnaire: {:#?}",
        out.questions
    );
    assert_eq!(doc["tasks"]["le_monde"]["invoke"]["args"]["url"], LE_MONDE);
    assert_eq!(
        sorted(&doc["permits"]["net"]["http"]),
        ["news.ycombinator.com", "techcrunch.com", "www.lemonde.fr"]
    );
    let admitted = recorded(&out);
    let delegated: Vec<&str> = (admitted.iter())
        .filter(|(_, kind, role, excerpt)| {
            kind == "delegated" && role == "read_source" && excerpt == DELEGATION
        })
        .map(|(value, ..)| value.as_str())
        .collect();
    assert_eq!(delegated.len(), 3, "{admitted:?}");
    for source in [HACKER_NEWS, TECHCRUNCH, LE_MONDE] {
        assert!(delegated.contains(&source), "{source}: {admitted:?}");
    }
    assert!(
        admitted.iter().all(|(_, kind, ..)| kind != "answered"),
        "a selection is never filed as a human answer: {admitted:?}"
    );
}

/// The scope of a delegation is public reading: a private address is outside it, whatever the
/// author states.
#[tokio::test]
async fn a_delegation_never_reaches_a_private_address() {
    let private = "http://192.168.1.20/flux.xml";
    let source = digest(&[("hacker_news", HACKER_NEWS), ("intranet", private)]);
    let stated = json!([
        resolution(HACKER_NEWS, "delegated", "read_source", DELEGATION),
        resolution(private, "delegated", "read_source", DELEGATION),
        resolution(DIGEST, "derived", "output_path", DELEGATED_OUTPUT),
    ]);
    let author = Author::new(vec![answer(&source, &stated)]);
    let out = create(&request(DELEGATED), &author).await;
    assert_ne!(out.status, CompileStatus::Ready, "{}", findings(&out));
    assert!(
        author.count() > 1,
        "the laws sent a repair: {}",
        findings(&out)
    );
    let repair = author.last(1);
    assert!(
        repair.contains("192.168.1.20") && repair.contains("public"),
        "the repair refuses the private address for the delegated public scope: {repair}"
    );
    assert!(
        !repair.contains("news.ycombinator.com"),
        "the public source the delegation covers is admitted: {repair}"
    );
}

/// A delegated source is read; a delegation never authorizes sending anything to it.
#[tokio::test]
async fn a_delegated_source_is_read_never_posted_to() {
    let source = digest_with(&[("hacker_news", HACKER_NEWS)], "POST", DIGEST);
    let stated = json!([
        resolution(HACKER_NEWS, "delegated", "read_source", DELEGATION),
        resolution(DIGEST, "derived", "output_path", DELEGATED_OUTPUT),
    ]);
    let author = Author::new(vec![answer(&source, &stated)]);
    let out = create(&request(DELEGATED), &author).await;
    assert_ne!(out.status, CompileStatus::Ready, "{}", findings(&out));
    assert!(
        author.count() > 1 && author.last(1).contains("GET") && author.last(1).contains("POST"),
        "the repair says a delegated source is read with GET, never posted to: {}",
        findings(&out)
    );
}

/// An authorization is the human's own words: a selection citing words the request does not
/// carry is no selection.
#[tokio::test]
async fn a_selection_needs_the_human_words_that_authorize_it() {
    let invented = "tu connais déjà mes sources préférées";
    let source = digest(&[("hacker_news", HACKER_NEWS)]);
    let stated = json!([
        resolution(HACKER_NEWS, "delegated", "read_source", invented),
        resolution(DIGEST, "derived", "output_path", NAMED_OUTPUT),
    ]);
    let author = Author::new(vec![answer(&source, &stated)]);
    let out = create(&request(NAMED), &author).await;
    assert_ne!(out.status, CompileStatus::Ready, "{}", findings(&out));
    assert!(
        author.count() > 1 && author.last(1).contains(invented),
        "the repair names the words the request does not carry: {}",
        findings(&out)
    );
}

/// A routine output name is a new file: one the project already holds is a material decision,
/// asked or refused, never replaced by a derived choice.
#[tokio::test]
async fn a_derived_output_never_replaces_a_file_the_project_holds() {
    let world = json!({"observed": [
        {"path": DIGEST, "state": "observed", "complete": false, "kind": "text"}
    ]});
    let source = digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)]);
    let stated = json!([
        resolution(HACKER_NEWS, "named", "read_source", "Hacker News"),
        resolution(TECHCRUNCH, "named", "read_source", "TechCrunch"),
        resolution(DIGEST, "derived", "output_path", NAMED_OUTPUT),
    ]);
    let author = Author::new(vec![answer(&source, &stated)]);
    let out = create(&request(NAMED).with_knowledge(world), &author).await;
    assert_ne!(out.status, CompileStatus::Ready, "{}", findings(&out));
    let asked = out
        .questions
        .iter()
        .any(|q| q.label.contains(DIGEST) || q.why.contains(DIGEST));
    let said = named_by_a_finding(&out, &[DIGEST, "exist"])
        || (author.count() > 1 && author.last(1).contains(DIGEST));
    assert!(
        asked || said,
        "the existing file is a decision, never silently replaced: {}",
        findings(&out)
    );
}

/// Guard: a selection stated with no provenance stays an invented literal, as today.
#[tokio::test]
async fn a_selection_stated_without_provenance_stays_an_invented_literal() {
    let source = digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)]);
    let author = Author::new(vec![undeclared(&source)]);
    let out = create(&request(NAMED), &author).await;
    assert_ne!(out.status, CompileStatus::Ready, "{}", findings(&out));
    assert!(author.count() > 1, "the laws sent a repair");
    let repair = author.last(1);
    assert!(
        repair.contains("INVENTED LITERAL") && repair.contains("techcrunch.com"),
        "{repair}"
    );
}

/// Guard: the literals a request states keep their existing forms; nothing to declare. A request
/// that states every literal may compile without asking the author at all; when the author is
/// asked, its document is accepted without a repair.
#[tokio::test]
async fn literals_the_request_states_need_no_declared_selection() {
    let source = digest(&[("hacker_news", HACKER_NEWS)]);
    let author = Author::new(vec![undeclared(&source)]);
    let out = create(&request(STATED), &author).await;
    let doc = ready(&out);
    assert!(author.count() <= 1, "{}", findings(&out));
    // Whoever wrote it (the deterministic core names its own tasks), the document carries both
    // stated literals as the person wrote them.
    let text = doc.to_string();
    assert!(
        text.contains(&format!("\"{HACKER_NEWS}\"")) && text.contains(&format!("\"{DIGEST}\"")),
        "{text}"
    );
}
