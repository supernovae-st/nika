// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(all(unix, feature = "access-harness"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

//! The Session led by an ACP agent ("Prepare with Claude Code"): the person chose Claude Code
//! over ACP, and its agent leads the conversation with its own loop, reaching Nika's tools over
//! MCP through the real tool server, while the Session keeps the tree, the citations, the
//! identities and the effects. Only the agent's executable is scripted (`agent`): it is found
//! on PATH through the registry's real `claude-code` row and speaks ACP as claude-agent-acp
//! 0.81.1 does; every decision it makes is a scripted tool call, so these suites prove what the
//! Session does with them, never an intelligence's judgment. Each scenario re-runs this binary
//! as a child whose environment holds only the fixture's PATH and HOME.

#[path = "acp_led/agent.rs"]
mod agent;
#[path = "acp_led/child.rs"]
mod child;

use std::fmt::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nika_providers::model_choice::{ModelInventory, ModelRole};
use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
use nika_types::access::AccessClass;
use serde_json::{Value, json};

/// The workflow model the candidate names.
const MODEL: &str = "vllm/oux-author";
const HACKER_NEWS: &str = "https://news.ycombinator.com";
const TECHCRUNCH: &str = "https://techcrunch.com";
const DIGEST: &str = "./news/digest.md";
/// The ordinary request: sources and output left open.
pub(crate) const REQUEST: &str = "fais moi un workflow tres simple qui recupere les news tech recentes, les resume et ecrit le resultat en markdown dans un dossier du projet";
/// The acceptance of the agent's recommendation: the consent word, a pick by protocol (a
/// sentence would be read by the seat's own one-shot, which this fixture does not script).
pub(crate) const ACCEPT: &str = "oui";
/// The person asks which model runs their workflow.
pub(crate) const CHOOSE: &str = "which model should run my workflow?";
/// A model no route on this machine offers.
const INVENTED: &str = "acme/imaginary-1";

/// The digest workflow over two news sites: one GET each, one summary, one write.
fn digest() -> String {
    let sources = [("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)];
    let mut source = format!(
        "nika: news-digest\nmodel: {MODEL}\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net:\n    http: [\"news.ycombinator.com\", \"techcrunch.com\"]\n  fs:\n    write: [\"{DIGEST}\"]\ntasks:\n"
    );
    for (task, url) in sources {
        write!(
            source,
            "  {task}:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{url}\", method: GET }}\n"
        )
        .unwrap();
    }
    source.push_str("  summarize:\n    with:\n");
    for (task, _) in sources {
        writeln!(source, "      {task}: \"${{{{ tasks.{task}.output }}}}\"").unwrap();
    }
    writeln!(
        source,
        "    infer:\n      max_tokens: 1000\n      prompt: \"Résume en Markdown les actualités ci-dessous, sans rien inventer : ${{{{ with.hacker_news }}}} ${{{{ with.techcrunch }}}}\""
    )
    .unwrap();
    writeln!(
        source,
        "  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{DIGEST}\", content: \"${{{{ with.digest }}}}\" }}"
    )
    .unwrap();
    source
}

/// The agent's one recommendation for the ordinary request, asked as a choice.
fn plan_offer() -> Value {
    let values = json!([
        {"role": "read_source", "value": HACKER_NEWS, "name": "Hacker News"},
        {"role": "read_source", "value": TECHCRUNCH, "name": "TechCrunch"},
        {"role": "output_path", "value": DIGEST}
    ]);
    json!({"questions": [{
        "key": "plan",
        "question": format!("Je prends Hacker News, TechCrunch et j'écris le résumé dans {DIGEST} : ça te va ?"),
        "why": "La demande ne nomme ni les sources ni le fichier de sortie.",
        "options": [
            {"key": "recommended", "label": format!("Oui : Hacker News, TechCrunch → {DIGEST}"),
             "recommended": true, "values": values},
            {"key": "other", "label": "D'autres sources ou un autre fichier"}
        ],
        "free_text": true
    }]})
}

/// A selection taken from the recommended option the person accepted with `u2`.
fn offered(value: &str, role: &str) -> Value {
    json!({"value": value, "kind": "offered", "role": role, "message": "u2", "excerpt": ACCEPT,
        "question": "plan", "option": "recommended"})
}

/// The candidate the accepted recommendation builds.
fn accepted_digest() -> Value {
    json!({
        "source": digest(),
        "resolutions": [
            offered(HACKER_NEWS, "read_source"),
            offered(TECHCRUNCH, "read_source"),
            offered(DIGEST, "output_path"),
        ],
        "summary": "Hacker News et TechCrunch, résumé dans ./news/digest.md"
    })
}

/// A metered vendor route whose key is present: the probe's facts only, no call, no network.
pub(crate) fn deepseek_probe() -> ProviderProbe {
    let readiness = ProviderReadiness::new(
        true,
        true,
        None,
        None,
        true,
        ExecutionLocus::Cloud,
        AccessClass::Api,
    );
    ProviderProbe::new(
        "deepseek",
        true,
        true,
        "DEEPSEEK_API_KEY",
        true,
        readiness,
        "https://api.deepseek.com",
    )
}

/// A model the metered route offers for a run, and its facts as the inventory states them.
fn offered_model() -> (String, Value) {
    let inventory = ModelInventory::from_probes(&[deepseek_probe()]);
    let offer = (inventory.offers(ModelRole::Run).iter())
        .find(|offer| offer.model.starts_with("deepseek/"))
        .expect("the route offers the catalogue's models");
    let route = &offer.route;
    let mut facts = json!({"role": "run", "model": offer.model, "via": route.access,
        "class": route.class.as_str(), "configured": route.configured,
        "billing": route.billing.as_str()});
    if let Some(price) = offer.output_usd_per_million {
        facts["output_usd_per_million"] = json!(price);
    }
    (offer.model.clone(), facts)
}

/// The agent asks which model runs the workflow: one the inventory offers, one it does not.
fn model_offer(model: &str) -> Value {
    let option = |key: &str, value: &str, recommended: bool| {
        json!({"key": key, "label": value, "recommended": recommended,
            "values": [{"role": "run_model", "value": value}]})
    };
    json!({"questions": [{
        "key": "model",
        "role": "run_model",
        "question": "Which model runs the workflow?",
        "options": [option("offered", model, true), option("invented", INVENTED, false)],
        "free_text": false
    }]})
}

/// One line queued for the run under way, as a receipt or the Work shows it.
fn queued(id: &str, mode: &str, line: &str, state: &str) -> Value {
    json!({"id": id, "mode": mode, "line": line, "state": state})
}

/// A tool step a host received: its start, then its end with the time it took.
fn tool_step(call: &str, tool: &str) -> [Value; 2] {
    [
        json!([call, tool, "started", false]),
        json!([call, tool, "finished", true]),
    ]
}

fn call(id: &str, tool: &str, args: &Value) -> Value {
    json!({"id": id, "call": tool, "args": args})
}

/// A fixture: the project, the home, the agent on PATH (unless absent) and what it observes.
struct World {
    dir: tempfile::TempDir,
}

impl World {
    fn new(turns: Option<&Value>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        for name in ["bin", "project", "home", "observed"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }
        if let Some(turns) = turns {
            let bin = dir.path().join("bin");
            let script = bin.join("claude-agent-acp");
            std::fs::write(&script, agent::AGENT).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::write(bin.join("scenario.json"), turns.to_string()).unwrap();
        }
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// The JSON lines the agent noted under `name`.
    fn observed(&self, name: &str) -> Vec<Value> {
        let text = std::fs::read_to_string(self.path("observed").join(name)).unwrap_or_default();
        text.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The Session tree kept beside the history, one JSON value per line.
    fn tree(&self) -> Vec<Value> {
        let sessions = self.path("home").join(".nika/sessions");
        let found = std::fs::read_dir(&sessions)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("tree.jsonl"))
            .find(|path| path.exists())
            .expect("the tree is kept beside the history");
        let text = std::fs::read_to_string(found).unwrap();
        text.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// Run the scenario `name` in a child process; its report.
    fn run(&self, name: &str) -> Value {
        let report = self.path("report.json");
        let log = self.path("child.log");
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.path("bin").display()),
            )
            .env("HOME", self.path("home"))
            .env("NIKA_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .env("ACP_LED_CHILD", name)
            .env("ACP_LED_ROOT", self.path("project"))
            .env("ACP_LED_OBSERVED", self.path("observed"))
            .env("ACP_LED_REPORT", &report)
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::fs::File::create(&log).unwrap()))
            .stderr(Stdio::from(
                std::fs::OpenOptions::new().append(true).open(&log).unwrap(),
            ))
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(180);
        let status = loop {
            if let Some(status) = process.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = process.kill();
                panic!("the child scenario `{name}` did not finish within 180 s");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(
            status.success(),
            "child `{name}` failed:\n{}",
            std::fs::read_to_string(&log).unwrap_or_default()
        );
        serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap()
    }
}

/// The child: drive the scenario its parent named, write the report.
#[test]
#[ignore = "run by the parent scenarios in a child process"]
fn child() {
    let Some(name) = std::env::var_os("ACP_LED_CHILD") else {
        return;
    };
    let var = |name: &str| PathBuf::from(std::env::var_os(name).unwrap());
    let (root, home) = (var("ACP_LED_ROOT"), var("HOME"));
    let report = child::drive(
        &name.to_string_lossy(),
        &root,
        &home,
        &var("ACP_LED_OBSERVED"),
    );
    std::fs::write(
        var("ACP_LED_REPORT"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}

fn kind(step: &Value) -> &str {
    step["outcome"]["kind"].as_str().unwrap_or_default()
}

/// The tree's entries of type `kind` (each line's body is the header or one entry).
fn entries<'a>(tree: &'a [Value], kind: &str) -> Vec<&'a Value> {
    (tree.iter())
        .map(|line| &line["body"]["entry"])
        .filter(|entry| entry["kind"]["type"] == kind)
        .collect()
}

/// The project's `.nika` files, relative.
fn workflows(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "nika") {
            let name = entry.file_name().to_string_lossy().into_owned();
            out.push((name, std::fs::read_to_string(&path).unwrap()));
        }
    }
    out
}

/// The agent asked through `ask`: one question, under the Session's own identity.
fn assert_asked(asked: &Value) {
    assert_eq!(kind(asked), "question", "{asked:#}");
    let text = asked["outcome"]["text"].as_str().unwrap();
    assert!(
        text.starts_with("Je te propose deux sources tech.") && text.contains("ça te va ?"),
        "{text}"
    );
    assert_eq!(asked["shown"]["waiting"]["kind"], "question", "{asked:#}");
    let question = &asked["shown"]["work"]["questions"][0];
    assert_eq!(question["key"], "plan");
    // The identity serializes as its whole witness and displays as the witness's first 12 hex.
    let full = question["id"].as_str().unwrap();
    assert_eq!(asked["shown"]["waiting"]["id"], full, "{asked:#}");
    let displayed = asked["shown"]["question_id"].as_str().unwrap();
    assert!(
        full.starts_with(displayed),
        "{full} displays as {displayed}"
    );
}

/// The candidate the agent wrote binds the accepted values with their provenance, and the
/// proposal waits for the person's consent.
fn assert_proposed(proposed: &Value) {
    assert_eq!(kind(proposed), "proposal", "{proposed:#}");
    let text = proposed["outcome"]["text"].as_str().unwrap();
    assert!(text.starts_with("Voici le workflow"), "{text}");
    let bindings = proposed["shown"]["work"]["bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 3, "{bindings:#?}");
    let offered =
        |b: &Value| b["provenance"]["kind"] == "offered" && b["provenance"]["message"] == "u2";
    assert!(bindings.iter().all(offered), "{bindings:#?}");
    assert_eq!(proposed["shown"]["proposal"], proposed["outcome"]["id"]);
}

/// What the agent read: Nika's instructions with the first line, then the answer, cited; the
/// replies of Nika's tools it called, every one allowed; a foreign tool refused.
fn assert_agent_read(world: &World) {
    let prompts = world.observed("prompts");
    assert_eq!(prompts.len(), 2);
    let first = prompts[0].as_str().unwrap();
    assert!(first.starts_with("You lead a Nika Session"), "{first}");
    assert!(
        first.contains("mcp__nika__") && first.contains("end your turn"),
        "{first}"
    );
    assert!(
        first.ends_with(&format!("{REQUEST}\n\n(cited as u1)")),
        "{first}"
    );
    // The answer, then what Nika read it to pick: the recommended offer, bound by Nika.
    let answer = format!("The person answered, cited as u2:\n{ACCEPT}\n\nNika (not the person):\n");
    assert!(
        prompts[1].as_str().unwrap().starts_with(&answer),
        "{}",
        prompts[1]
    );
    assert!(
        prompts[1]
            .as_str()
            .unwrap()
            .contains("the person picked `recommended`"),
        "{}",
        prompts[1]
    );
    let names = nika_session_change::tools::NAMES.len();
    assert_eq!(world.observed("tools")[0].as_array().unwrap().len(), names);
    let replies = world.observed("replies");
    let reply = |tool: &str| -> String {
        let found = replies.iter().find(|r| r["call"] == tool);
        let found = found.unwrap_or_else(|| panic!("no reply to {tool}: {replies:#?}"));
        assert_eq!(found["is_error"], false, "{replies:#?}");
        found["text"].as_str().unwrap().to_owned()
    };
    assert!(reply("ask").contains("End your turn now"));
    reply("candidate_write");
    assert!(reply("propose").contains("shown to the person"));
    assert!(
        world.observed("refused").is_empty(),
        "every Nika tool was allowed"
    );
    let rejected = json!({"outcome": "selected", "optionId": "reject"});
    assert_eq!(world.observed("foreign"), [rejected]);
}

/// The tree keeps who leads, each turn's record (the foreign tool denied with it), the calls
/// that waited for the person and the line that answered the first.
fn assert_tree_kept(world: &World) {
    let tree = world.tree();
    let facts = entries(&tree, "fact");
    let led_by = facts.iter().find(|f| f["kind"]["name"] == "led_by");
    assert_eq!(led_by.unwrap()["kind"]["data"]["seat"], "claude-code");
    let turns = (facts.iter()).filter(|f| f["kind"]["name"] == "led_turn");
    assert_eq!(turns.count(), 2);
    let denied =
        (facts.iter()).any(|f| f["kind"]["data"]["permissions"]["denied"] == json!(["Bash"]));
    assert!(denied, "{facts:#?}");
    // The question, then, since a shown proposal ends the turn, the proposal.
    let parked = entries(&tree, "parked");
    assert_eq!(parked.len(), 2);
    assert_eq!(parked[0]["kind"]["call"], "toolu_ask");
    assert_eq!(parked[1]["kind"]["call"], "toolu_propose");
    let answered = (entries(&tree, "user").into_iter())
        .any(|u| u["kind"]["cite"] == "u2" && u["kind"]["answers"] == "toolu_ask");
    assert!(answered, "{tree:#?}");
}

#[test]
fn an_acp_agent_leads_the_session_with_nikas_tools() {
    let turns = json!([
        [
            {"id": "toolu_bash", "foreign": "Bash"},
            call("toolu_ask", "ask", &plan_offer()),
            {"say": "Je te propose deux sources tech."}
        ],
        [
            call("toolu_write", "candidate_write", &accepted_digest()),
            call("toolu_propose", "propose", &json!({})),
            {"say": "Voici le workflow : Hacker News et TechCrunch, résumé dans ./news/digest.md."}
        ]
    ]);
    let world = World::new(Some(&turns));
    let report = world.run("lead");
    let steps = report["steps"].as_array().unwrap();
    assert_asked(&steps[0]);
    assert_proposed(&steps[1]);
    // Verified before it was shown: the seat's own one-shot judged the whole request.
    let judged = world.observed("oneshots");
    let asked = |text: &Value| text.as_str().is_some_and(|text| text.contains("faithful"));
    assert!(judged.iter().any(asked), "{judged:?}");
    // The person's `yes` saved exactly the candidate the agent wrote.
    assert_ne!(kind(&steps[2]), "refusal", "{:#}", steps[2]);
    let written = workflows(&world.path("project"));
    assert_eq!(written.len(), 1, "{written:?}");
    assert_eq!(written[0].1, digest());
    assert_agent_read(&world);
    assert_tree_kept(&world);
    // Each call of Nika's tools reached the host as a tool step, in order; the foreign tool,
    // refused at its permission, never ran.
    let steps_seen = [
        tool_step("toolu_ask", "ask"),
        tool_step("toolu_write", "candidate_write"),
        tool_step("toolu_propose", "propose"),
    ]
    .concat();
    assert_eq!(report["tools"], json!(steps_seen));
    assert_eq!(world.observed("spawned").len(), 1, "one agent session");
    assert_eq!(
        report["agent_alive_after_close"], false,
        "the agent ends with the Session"
    );
}

#[test]
fn stop_cancels_the_agent_turn_once_and_the_conversation_goes_on() {
    let turns = json!([[{"await_cancel": true}], [{"say": "Still here."}]]);
    let world = World::new(Some(&turns));
    let report = world.run("stop");
    let steps = report["steps"].as_array().unwrap();
    let waiting = queued("l1", "follow_up", "then c", "waiting");
    assert_eq!(report["receipts"], json!([{"queued": waiting}]));
    // A typed stop: how it reached the agent, the queued line returned unsent, no draft.
    let returned = json!([queued("l1", "follow_up", "then c", "returned")]);
    let text = "stopped by you · the agent was asked to stop and ended its turn · the \
        conversation and its draft are kept · not sent: « then c »";
    let stopped = json!({"kind": "stopped", "reach": "agent_cancelled", "unsent": returned,
        "candidate": null, "text": text});
    assert_eq!(steps[0]["outcome"], stopped, "{steps:#?}");
    assert_eq!(steps[0]["shown"]["work"]["queued"], returned);
    assert_eq!(world.observed("cancels"), [json!("session/cancel")]);
    assert!(
        !world.observed("after").contains(&json!("session/cancel")),
        "Stop sends `session/cancel` once"
    );
    assert_eq!(kind(&steps[1]), "reply", "{steps:#?}");
    assert_eq!(steps[1]["outcome"]["text"], "Still here.");
    let prompts = world.observed("prompts");
    assert_eq!(prompts[1], "are you still there?\n\n(cited as u2)");
    assert_eq!(
        world.observed("spawned").len(),
        1,
        "the line after Stop rides the same agent session"
    );
    let next_work = steps[1]["shown"]["work"].as_object().unwrap();
    assert!(
        !next_work.contains_key("queued"),
        "the next run starts with no queued line"
    );
    let tree = world.tree();
    let recorded = entries(&tree, "stopped");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0]["kind"]["queued"], json!(["then c"]));
    assert_eq!(report["after_turn"], json!({"refused": "not_reading"}));
    assert_eq!(
        report["agent_alive_after_close"], false,
        "the agent ends with the Session"
    );
}

/// The person's cited lines in the tree, with how each waited.
fn users(world: &World) -> Vec<(Value, Value)> {
    (entries(&world.tree(), "user").into_iter())
        .map(|u| (u["kind"]["cite"].clone(), u["kind"]["queued"].clone()))
        .collect()
}

#[test]
fn a_steering_line_enters_the_agent_turn_under_way() {
    let turns = json!([[{"await_cancel": true}], [{"say": "Using b."}]]);
    let world = World::new(Some(&turns));
    let report = world.run("steer");
    let waiting = queued("l1", "steer", "use b instead", "waiting");
    assert_eq!(report["receipts"], json!([{"queued": waiting}]));
    let step = &report["steps"][0];
    let reply = json!({"kind": "reply", "text": "Using b."});
    assert_eq!(step["outcome"], reply, "{step:#}");
    let mut entered = queued("l1", "steer", "use b instead", "entered");
    entered["cite"] = json!("u2");
    assert_eq!(step["shown"]["work"]["queued"], json!([entered]));
    // The agent was asked once to stop its turn, and read the line as its next prompt.
    assert_eq!(world.observed("cancels"), [json!("session/cancel")]);
    let prompts = world.observed("prompts");
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[1], "use b instead\n\n(cited as u2)");
    let cited = [(json!("u1"), Value::Null), (json!("u2"), json!("steer"))];
    assert_eq!(users(&world), cited);
    assert_eq!(report["after_turn"], json!({"refused": "not_reading"}));
    assert_eq!(world.observed("spawned").len(), 1, "one agent session");
    assert_eq!(report["agent_alive_after_close"], false);
}

#[test]
fn a_follow_up_line_enters_when_the_agent_ends_its_turn() {
    let turns = json!([
        [{"mark": "turn-open"}, {"await_marker": "queued"}, {"say": "a done"}],
        [{"say": "c done"}]
    ]);
    let world = World::new(Some(&turns));
    let report = world.run("follow");
    let waiting = queued("l1", "follow_up", "and c", "waiting");
    assert_eq!(report["receipts"], json!([{"queued": waiting}]));
    let step = &report["steps"][0];
    let reply = json!({"kind": "reply", "text": "c done"});
    assert_eq!(step["outcome"], reply, "{step:#}");
    let mut entered = queued("l1", "follow_up", "and c", "entered");
    entered["cite"] = json!("u2");
    assert_eq!(step["shown"]["work"]["queued"], json!([entered]));
    assert!(
        world.observed("cancels").is_empty(),
        "a follow-up never stops the agent"
    );
    assert_eq!(world.observed("prompts")[1], "and c\n\n(cited as u2)");
    let tree = world.tree();
    let said: Vec<&Value> = (entries(&tree, "assistant").into_iter())
        .map(|a| &a["kind"]["content"][0]["text"])
        .collect();
    assert_eq!(said, [&json!("a done"), &json!("c done")]);
    let cited = [
        (json!("u1"), Value::Null),
        (json!("u2"), json!("follow_up")),
    ];
    assert_eq!(users(&world), cited);
    assert_eq!(report["after_turn"], json!({"refused": "not_reading"}));
}

#[test]
fn a_model_the_agent_offers_carries_the_inventorys_facts() {
    let (model, facts) = offered_model();
    let turns = json!([[
        call("toolu_model", "ask", &model_offer(&model)),
        {"say": "Which model runs it?"}
    ]]);
    let world = World::new(Some(&turns));
    let report = world.run("choices");
    let step = &report["steps"][0];
    assert_eq!(kind(step), "question", "{step:#}");
    let options = &step["shown"]["work"]["questions"][0]["options"];
    let offered = &options[0]["values"][0];
    assert_eq!(offered["value"], model.as_str());
    assert_eq!(offered["choice"], facts, "{options:#}");
    let typed = (&facts["class"], &facts["billing"], &facts["configured"]);
    assert_eq!(typed, (&json!("api"), &json!("api_metered"), &json!(true)));
    let invented = options[1]["values"][0].as_object().unwrap();
    assert_eq!(invented["value"], INVENTED);
    assert!(
        !invented.contains_key("choice"),
        "a choice is never invented"
    );
    assert_eq!(report["tools"], json!(tool_step("toolu_model", "ask")));
}

#[test]
fn a_seat_that_cannot_lead_refuses_and_nothing_is_sent() {
    let world = World::new(None);
    let report = world.run("unavailable");
    let step = &report["steps"][0];
    assert_eq!(kind(step), "refusal", "{step:#}");
    let text = step["outcome"]["text"].as_str().unwrap();
    assert!(
        text.contains("`claude-code` cannot lead this conversation")
            && text.contains("nothing was sent"),
        "{text}"
    );
    let users = entries(&world.tree(), "user").len();
    assert_eq!(users, 0, "the line was not taken into the conversation");
}
