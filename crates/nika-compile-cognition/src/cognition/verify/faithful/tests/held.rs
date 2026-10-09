// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A component the judged bytes hold as admitted, told to every question that judges them (A5,
//! R6): each part asked alone, each part asked again over a whole trial run and the whole request
//! over that run carry what these bytes hold of the lent catalogue (`construction.held`: the
//! component, its witness and its bindings, bound to these bytes), beside the construction
//! context a localization reads. A judge that follows that context can settle the clause that
//! conditionally asks to use an admitted component; every other part stays judged on what the
//! program does. A localization may name the held component (`held-<k>`): that takes the judge's
//! own `missing` back and leaves the part contested, never settled by itself. The SCRIPTED seat
//! decides from what each question shows: these tests establish what the questions carry and how
//! the verdict weighs the answers, never a model's behaviour.

use super::*;

/// The tickets request: its conditional reuse clause is one of its parts.
const TICKETS: &str = "Create a new workflow that reads ./in/tickets.json. Each row has an id and a numeric age_hours. Keep records whose age_hours is strictly greater than 48, preserve their input order, and write a JSON object containing count and ids to ./out/report.json. Expose the selected records as the named output stale. Use an applicable admitted Foundry component when the catalogue provides one, with these exact paths and threshold bound; otherwise construct the same requested work. Missing catalogue coverage must not remove the report requirement. The request authorizes only the stated file read/write and the read, jq and write tools.";

/// Words of the conditional clause, of the threshold and of the report's write: how the scripted
/// judge recognizes the part it is asked about (test scaffolding, never an engine rule).
const REUSE: &str = "admitted Foundry component";
const THRESHOLD: &str = "greater than 48";
const DESTINATION: &str = "./out/report.json";

/// The admitted component the judge decides fits the request (its contextual decision, scripted).
const FILTER: &str = "block:filter-records";

/// How the scripted judge knows a question told it what holding a component means: the opening of
/// that context.
const HOLDING_SAID: &str = "`construction.held` lists each offered component these exact bytes";

/// What a localization's alternatives say, never told a question that only judges.
const ALTERNATIVES_SAID: &str = "component-<k>:";
const HELD_SAID: &str = "held-<k>:";

/// The candidate's envelope: its name, constants and boundary.
const ENVELOPE: &str = r#"nika: stale-tickets-report
const:
  output_path: ./out/report.json
  source_path: "./in/tickets.json"
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs: { read: ["./in/tickets.json"], write: ["./out/report.json"] }
tasks:
"#;

/// The author's own tasks beside the component: the report built from the kept rows, its write.
const OWN: &str = r#"  build_report:
    with: { stale: "${{ tasks.filter_records.output }}" }
    invoke:
      tool: "nika:jq"
      args:
        input: "${{ with.stale }}"
        expression: "{count: length, ids: map(.id)}"
  write_report:
    with: { report: "${{ tasks.build_report.output }}" }
    invoke:
      tool: "nika:write"
      args:
        path: "${{ const.output_path }}"
        content: "${{ with.report }}"
        overwrite: true
        create_dirs: true
"#;

/// The filter component's nodes as expanded: its read, parse and filter, then its outputs.
const COMPONENT: &str = r#"  read_records:
    invoke: { tool: "nika:read", args: { path: "${{ const.source_path }}" } }
  parse_records:
    with: { raw: "${{ tasks.read_records.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.raw }}", expression: "fromjson" } }
  filter_records:
    with: { records: "${{ tasks.parse_records.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.records }}", expression: "[.[] | select(.age_hours > 48)]" } }
outputs:
  stale: ${{ tasks.filter_records.output }}
  selected: ${{ tasks.filter_records.output }}
"#;

/// A write the candidate performs, as the scripted judge looks for one.
const WRITES: &str = "tool: \"nika:write\"";

/// The tickets the trial run read, and the report it wrote: the rows strictly older than 48 hours.
const READ: &str = r#"[{"id":"fresh-10","age_hours":10},{"id":"stale-60","age_hours":60},{"id":"boundary-48","age_hours":48},{"id":"stale-90","age_hours":90}]"#;
const WRITTEN: &str = r#"{"count":2,"ids":["stale-60","stale-90"]}"#;

/// Why a doubt stayed open when a part stayed open after the run.
const OPEN_AFTER_RUN: &str =
    "the trial run did not decide every part: a part its inputs never exercise stays open";

/// The composed candidate keeping the rows older than `hours`, with or without the author's own
/// report and write.
fn composed(hours: u32, report: bool) -> String {
    let own = if report { OWN } else { "" };
    format!("{ENVELOPE}{own}{COMPONENT}").replace("> 48", &format!("> {hours}"))
}

/// A whole trial run of `candidate`: the tickets read whole, the report it wrote read whole.
fn run(candidate: &str) -> Value {
    json!({
        "candidate_sha256": sha256(candidate),
        "inputs": [{"path": "./in/tickets.json", "text": READ, "read_whole": true}],
        "outputs": [{"path": DESTINATION, "text": WRITTEN, "written": true, "read_whole": true}],
    })
}

/// The release the door was lent.
fn lent() -> Value {
    json!({"version": "r2", "snapshot_sha256": "22", "profile": "profile/r2"})
}

/// The engine facts the door recorded on the candidate (`judge::lent`): the lent release, its
/// three admitted components as offered (the filter one second) with what these bytes hold of
/// each (`witness` for the filter one, null when they hold none of them), and the receipt
/// witnessed on these bytes with what it binds.
fn facts(witness: Option<&str>) -> Value {
    let row = |id: &str, title: &str, callables: &[&str], held: Value| {
        json!({"component": {"id": id, "version": "r2"}, "title": title, "purpose": title,
            "holes": [{"name": "const.source_path", "owner": "human"}], "effects": ["fs.read"],
            "construction": {"held": held, "callables": callables}})
    };
    let offered = [
        row(
            "block:csv-records",
            "CSV rows as JSON records",
            &["nika:read", "nika:convert"],
            Value::Null,
        ),
        row(
            FILTER,
            "JSON records filtered by a field",
            &["nika:read", "nika:jq"],
            json!(witness),
        ),
        row(
            "block:post-summary",
            "A summary posted to an endpoint",
            &["nika:fetch"],
            Value::Null,
        ),
    ];
    let composed: Vec<Value> = (witness.into_iter())
        .map(|verdict| {
            json!({"component": FILTER, "release": lent(), "verdict": verdict,
                "bindings": [{"path": "const.source_path", "bound": "./in/tickets.json"}]})
        })
        .collect();
    json!({"catalogue": lent(), "offered": {"total": 3, "components": offered},
        "composed": composed})
}

/// What a question over `candidate` is shown these bytes hold when they hold the filter component
/// `witness`: its place in the offer, identity, title, witness and bindings, bound to these bytes
/// and the lent release.
fn holding(candidate: &str, witness: &str) -> Value {
    let held = json!({"offer": 1, "component": {"id": FILTER, "version": "r2"},
        "title": "JSON records filtered by a field", "witness": witness,
        "bindings": [{"path": "const.source_path", "bound": "./in/tickets.json"}]});
    json!({"candidate_sha256": sha256(candidate), "catalogue": lent(), "held": [held]})
}

/// How the scripted judge reads the conditional reuse clause.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// It follows what each question shows: the clause is carried when the question tells what
    /// holding means and shows a fitting component these bytes hold, and missing otherwise.
    Follows,
    /// It judges the clause missing whatever it is shown, then names what carries it.
    Missing,
}

/// A SCRIPTED seat deciding each question from what it shows, never from its order: it doubts the
/// whole request, judges each part on the candidate's bytes and on what it is told of how they
/// are built, says why a part is missing, and keeps every question it was asked.
struct Reader {
    reading: Reading,
    asked: Mutex<Vec<ChoiceQuestion>>,
}

impl Reader {
    fn new(reading: Reading) -> Self {
        Self {
            reading,
            asked: Mutex::new(Vec::new()),
        }
    }

    /// The question `id` as the seat received it.
    fn question(&self, id: &str) -> ChoiceQuestion {
        let asked = self.asked.lock().unwrap();
        (asked.iter().find(|question| question.id == id).cloned())
            .unwrap_or_else(|| panic!("no question {id}"))
    }

    /// Whether `question` tells what holding a component means and shows a component these bytes
    /// hold as admitted that fits the request.
    fn held_fits(question: &ChoiceQuestion) -> bool {
        let told = question.instructions.contains(HOLDING_SAID);
        let held = (question.state["construction"]["held"]
            .as_array()
            .into_iter()
            .flatten())
        .any(|held| {
            held["component"]["id"] == FILTER
                && (held["witness"] == "expanded" || held["witness"] == "invoked")
        });
        told && held
    }

    /// Whether the candidate `question` shows carries `clause`.
    fn carries(&self, clause: &str, question: &ChoiceQuestion) -> bool {
        let candidate = question.state["candidate_nika"]
            .as_str()
            .unwrap_or_default();
        if clause.contains(REUSE) {
            self.reading == Reading::Follows && Self::held_fits(question)
        } else if clause.contains(THRESHOLD) {
            candidate.contains("> 48")
        } else if clause.contains(DESTINATION) {
            candidate.contains(WRITES)
        } else {
            true
        }
    }

    /// Why `clause`, judged missing, is missing: the fitting component the bytes hold, else the one
    /// they lack, else none fitting; the task that filters; the write no task performs.
    fn why(clause: &str, question: &ChoiceQuestion) -> String {
        let keys = question.keys();
        let offered = &question.state["authoring"]["offered"]["components"];
        let fitting = |prefix: &str| {
            (keys.iter())
                .find(|key| {
                    let at = key
                        .strip_prefix(prefix)
                        .and_then(|k| k.parse::<usize>().ok());
                    at.is_some_and(|k| offered[k]["component"]["id"] == FILTER)
                })
                .cloned()
        };
        if clause.contains(REUSE) {
            let named = fitting("held-").or_else(|| fitting("component-"));
            return named.unwrap_or_else(|| "no_fit".to_owned());
        }
        let answer = if clause.contains(THRESHOLD) {
            "task-filter_records"
        } else if clause.contains(DESTINATION) && keys.iter().any(|key| key == "omitted") {
            "omitted"
        } else {
            "none"
        };
        answer.to_owned()
    }

    fn decide(&self, question: &ChoiceQuestion) -> String {
        let keys = question.keys();
        let offers = |key: &str| keys.iter().any(|offered| offered == key);
        let clause = question.state["clause"]["text"]
            .as_str()
            .unwrap_or_default();
        if offers("unfaithful") {
            "unfaithful".to_owned()
        } else if offers("only_requested") {
            "only_requested".to_owned()
        } else if offers("no_task") {
            Self::why(clause, question)
        } else if offers("consistent") {
            // The whole request over the run: the first part it shows not carried, if any.
            let missed = (question.options.iter()).find(|option| {
                option.key.starts_with("part-") && !self.carries(&option.description, question)
            });
            missed.map_or_else(|| "consistent".to_owned(), |option| option.key.clone())
        } else if self.carries(clause, question) {
            "carried".to_owned()
        } else {
            "missing".to_owned()
        }
    }
}

impl DecisionSeat for Reader {
    fn name(&self) -> &str {
        SEAT
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            Ok(ChoiceAnswer::new(self.decide(question), SEAT))
        })
    }
}

/// The whole verdict of the tickets request over `candidate`, its state showing `facts`, asked of
/// `reader` beside a whole trial run of these bytes; and its binding.
async fn judged_over(candidate: &str, facts: Value, reader: &Reader) -> (Verdict, Binding) {
    let request = CompileRequest::create(TICKETS);
    let mut base = state(TICKETS, &request, candidate);
    base["authoring"] = facts;
    let binding = Binding::of(TICKETS, &request, &Plan::default(), candidate);
    let judge = Judge::<Scripted>::Seat(reader);
    let (mut verdict, mut out) = (Verdict::default(), crate::initial());
    let observation = run(candidate);
    let asked = (&base, "fixture");
    whole(
        TICKETS,
        asked,
        &judge,
        &binding,
        Some(&observation),
        &mut verdict,
        &mut out,
    )
    .await;
    (verdict, binding)
}

/// The place of the request's part whose words hold `words`, and that part.
fn part(words: &str) -> (usize, String) {
    let split = parts(TICKETS);
    let at = (split.iter().position(|part| part.contains(words)))
        .unwrap_or_else(|| panic!("no part says {words}: {split:?}"));
    (at, split[at].clone())
}

/// Every part question and the whole request over the run carry what the composed bytes hold:
/// the filter component, `expanded` with its source bound, bound to these bytes and this release
/// (`construction`), beside the construction context and what holding means (`expanded` and
/// `invoked` each said apart), never a localization's alternatives. The whole-request and extra
/// questions read the bytes as they did. A judge that follows the context carries the clause.
#[tokio::test]
async fn every_question_judging_the_bytes_is_told_what_they_hold() {
    let candidate = composed(48, true);
    let reader = Reader::new(Reading::Follows);
    let (verdict, _) = judged_over(&candidate, facts(Some("expanded")), &reader).await;
    let count = parts(TICKETS).len();
    let mut asked = vec!["verify-request".to_owned()];
    asked.extend((0..count).map(|k| format!("verify-part-{k}")));
    asked.extend(["verify-extra".to_owned(), "verify-observed".to_owned()]);
    assert_eq!(ids(&verdict), asked);
    let shown = holding(&candidate, "expanded");
    let context = "A clause may concern how the document is built rather than what a task does.";
    let expanded = "`expanded`: its admitted nodes are in these bytes, digest for digest";
    let invoked = "`invoked`: a task of these bytes calls it as a child workflow";
    let mut judging: Vec<String> = (0..count).map(|k| format!("verify-part-{k}")).collect();
    judging.push("verify-observed".to_owned());
    for id in &judging {
        let question = reader.question(id);
        assert_eq!(question.state["construction"], shown, "{id}");
        for said in [context, HOLDING_SAID, expanded, invoked] {
            assert!(question.instructions.contains(said), "{id}: {said}");
        }
        for unsaid in [ALTERNATIVES_SAID, HELD_SAID] {
            assert!(!question.instructions.contains(unsaid), "{id}: {unsaid}");
        }
        assert_eq!(question.state["authoring"], facts(Some("expanded")), "{id}");
        assert_eq!(question.state["candidate_nika"], candidate.as_str(), "{id}");
    }
    let (reuse, clause) = part(REUSE);
    let asked = reader.question(&format!("verify-part-{reuse}"));
    assert_eq!(asked.state["clause"], json!({"text": clause}));
    assert_eq!(
        record(&verdict, &format!("verify-part-{reuse}"))["choice"],
        "carried"
    );
    let over_run = reader.question("verify-observed");
    assert_eq!(over_run.state["observation"], run(&candidate));
    assert_eq!(over_run.state.get("history"), None, "no finding to recall");
    for id in ["verify-request", "verify-extra"] {
        let question = reader.question(id);
        assert_eq!(question.state.get("construction"), None, "{id}");
        assert!(!question.instructions.contains(HOLDING_SAID), "{id}");
    }
}

/// A judge that follows what each question shows settles the clause asking to use an admitted
/// component when one applies on the component these bytes hold, `expanded` or `invoked` alike,
/// and, every other part carried and nothing extra, finds the whole run consistent: the request is
/// carried by the question over that run, as for any candidate.
#[tokio::test]
async fn a_held_component_settles_the_conditional_clause_and_the_run_carries_the_request() {
    let candidate = composed(48, true);
    for witness in ["expanded", "invoked"] {
        let reader = Reader::new(Reading::Follows);
        let (verdict, binding) = judged_over(&candidate, facts(Some(witness)), &reader).await;
        let judgment = carried(TICKETS, "verify-observed", SEAT, &binding);
        assert_eq!(verdict.judgments, [judgment], "{witness}");
        assert_eq!(verdict.settled_by, Some("verify-observed"), "{witness}");
        let doubted = found(&[], &[], &[], &["unfaithful"], &[]);
        assert_eq!(lists(&verdict), doubted, "{witness}");
        assert!(verdict.settled(), "{witness}");
        let (reuse, _) = part(REUSE);
        let asked = reader.question(&format!("verify-part-{reuse}"));
        assert_eq!(
            asked.state["construction"]["held"][0]["witness"], witness,
            "what the judge is told keeps the witness apart"
        );
        let over_run = reader.question("verify-observed");
        assert_eq!(over_run.state["construction"], holding(&candidate, witness));
    }
}

/// Holding the component settles that clause and nothing else: the same bytes without the
/// author's report and write, or keeping another threshold than 48, still hold the component as
/// admitted (the reuse clause carried) and stay held, each on the part they miss: the write no task
/// performs, the task that filters differently (a restriction judged broken is judged again over
/// the run, which confirms it). Nothing is carried.
#[tokio::test]
async fn without_the_report_or_with_another_threshold_the_held_component_settles_nothing_else() {
    let confirmed = |part: &str, note: &str| {
        let over_run = if restricts(part) {
            "; the trial run confirms it"
        } else {
            ""
        };
        format!("{note}{over_run}")
    };
    let (_, write) = part(DESTINATION);
    let (_, threshold) = part(THRESHOLD);
    let omitted = confirmed(&write, OMITTED);
    let filters = confirmed(&threshold, &points("filter_records"));
    let unwritten = [(write.as_str(), omitted.as_str())];
    let unfiltered = [(threshold.as_str(), filters.as_str())];
    let cases = [
        (
            composed(48, false),
            found(&unwritten, &[], &[], &["unfaithful"], &[]),
        ),
        (
            composed(24, true),
            found(&unfiltered, &[], &[], &["unfaithful"], &[]),
        ),
    ];
    for (candidate, expected) in cases {
        let reader = Reader::new(Reading::Follows);
        let (verdict, _) = judged_over(&candidate, facts(Some("expanded")), &reader).await;
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(lists(&verdict), expected);
        assert!(!verdict.settled() && verdict.doubted());
        let (reuse, _) = part(REUSE);
        let id = format!("verify-part-{reuse}");
        assert_eq!(record(&verdict, &id)["choice"], "carried", "{candidate}");
    }
}

/// A component the catalogue merely offers, which these bytes do not hold, tells no question it
/// is held and settles nothing: the clause is judged missing and located on that offer, a defect
/// the repair composes from.
#[tokio::test]
async fn a_component_merely_offered_settles_nothing() {
    let candidate = composed(48, true);
    let reader = Reader::new(Reading::Follows);
    let (verdict, _) = judged_over(&candidate, facts(None), &reader).await;
    let (reuse, clause) = part(REUSE);
    let asked = reader.question(&format!("verify-part-{reuse}"));
    assert_eq!(asked.state.get("construction"), None);
    assert!(!asked.instructions.contains(HOLDING_SAID));
    let point = record(&verdict, &format!("verify-point-{reuse}"));
    assert_eq!(point["choice"], "component-1", "{point:#}");
    let keys = point["options"].as_array().cloned().unwrap_or_default();
    assert!(!keys.iter().any(|key| key == "held-1"), "{point:#}");
    assert_eq!(verdict.defects, std::slice::from_ref(&clause));
    let note = (verdict.notes.iter().find(|(defect, _)| *defect == clause))
        .map(|(_, note)| note.as_str())
        .unwrap_or_default();
    let named = "the judge points to the admitted component `block:filter-records` of release r2";
    assert!(note.starts_with(named), "{note}");
    assert!(!verdict.settled() && verdict.judgments.is_empty());
}

/// A judge that names the clause missing although these bytes hold the component can say so at
/// its localization: `held-<k>`, beside the offers the bytes lack and `no_fit`, describes the held
/// component with its witness and bindings and is recorded with that basis. It takes the judge's
/// `missing` back without a defect: the part stays contested over the bytes and over the run, an
/// honest open result, and the candidate is held.
#[tokio::test]
async fn a_held_component_named_at_the_localization_leaves_the_part_contested() {
    let candidate = composed(48, true);
    let reader = Reader::new(Reading::Missing);
    let (verdict, _) = judged_over(&candidate, facts(Some("expanded")), &reader).await;
    let (reuse, clause) = part(REUSE);
    let shown = holding(&candidate, "expanded");
    let ids_asked = ids(&verdict);
    let over = format!("verify-observed-part-{reuse}");
    for id in [
        format!("verify-point-{reuse}"),
        over.clone(),
        format!("{over}-point"),
    ] {
        assert!(ids_asked.contains(&id.as_str()), "{id}: {ids_asked:?}");
        let question = reader.question(&id);
        assert_eq!(question.state["construction"], shown, "{id}");
        assert!(question.instructions.contains(HOLDING_SAID), "{id}");
    }
    let point = reader.question(&format!("verify-point-{reuse}"));
    for said in [ALTERNATIVES_SAID, HELD_SAID] {
        assert!(point.instructions.contains(said), "{said}");
    }
    let keys = point.keys();
    for key in ["component-0", "component-2", "no_fit", "held-1", "no_task"] {
        assert!(keys.iter().any(|offered| offered == key), "{key}: {keys:?}");
    }
    assert!(!keys.iter().any(|key| key == "component-1"), "{keys:?}");
    let held = (point.options.iter().find(|option| option.key == "held-1"))
        .map(|option| option.description.clone())
        .unwrap_or_default();
    let described = "`block:filter-records` (release r2) · JSON records filtered by a field: these bytes hold it as admitted (expanded; bound: const.source_path = \"./in/tickets.json\")";
    assert_eq!(held, described);
    let recorded = record(&verdict, &format!("verify-point-{reuse}"));
    assert_eq!(recorded["choice"], "held-1");
    let basis = json!({"component": {"id": FILTER, "version": "r2"},
        "construction": {"held": "expanded", "callables": ["nika:read", "nika:jq"]}});
    assert_eq!(recorded["construction"], json!({"held": basis}));
    let open = found(
        &[],
        &[],
        &[clause.as_str(), TICKETS],
        &["unfaithful"],
        &[OPEN_AFTER_RUN],
    );
    assert_eq!(lists(&verdict), open);
    assert_eq!(verdict.judgments, NO_JUDGMENT);
    assert!(!verdict.settled() && verdict.doubted());
}
