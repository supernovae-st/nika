// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_session_change::outcome::Incarnation;
use serde_json::json;

use super::*;

const HN: &str = "https://news.ycombinator.com";
const TC: &str = "https://techcrunch.com";
const DIGEST: &str = "./news/digest.md";

fn citations(lines: &[(&str, &str)]) -> Citations {
    let mut out = Citations::default();
    for (k, (cite, text)) in lines.iter().enumerate() {
        let at = u64::try_from(k).unwrap() * 3 + 1;
        out.record(at, Some(((*cite).into(), (*text).into())));
    }
    out
}

/// The plan a proposal shows as a choice with its default: HN and TC into DIGEST.
fn plan() -> Value {
    json!({"key": "plan", "question": "HN et TC → ./news/digest.md ?",
        "options": [{"key": "recommended", "label": "Oui", "recommended": true, "values": [
            {"role": "read_source", "value": HN, "name": "Hacker News"},
            {"role": "read_source", "value": TC, "name": "TechCrunch"},
            {"role": "output_path", "value": DIGEST}]},
            {"key": "other", "label": "Autre"}],
        "free_text": true})
}

fn offered(value: &str, role: &str, message: &str) -> Value {
    json!({"value": value, "kind": "offered", "role": role, "message": message,
        "question": "plan", "option": "recommended"})
}

fn retained(value: &str, role: &str) -> Value {
    json!({"value": value, "kind": "retained", "role": role, "message": "u2"})
}

/// A conversation whose first proposal showed the plan open with its default, and whose u2 kept
/// that default.
fn accepted(asker: &Arc<Incarnation>) -> (Conversation, Citations) {
    let mut conversation = Conversation::default();
    let request = citations(&[("u1", "un digest des news tech")]);
    let reply = write(
        &mut conversation,
        &request,
        &format!("{HN} {TC} {DIGEST}"),
        json!([]),
    );
    assert!(!reply.is_error, "{reply:?}");
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), asker);
    let open = conversation.open_choices(
        &request,
        &json!({"open": [plan()]}),
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    let shown = (
        "S0".to_owned(),
        open.expect("the plan is shown with its default"),
    );
    assert_eq!(conversation.propose(&request, &json!({}), shown), Ok(None));
    let lines = citations(&[("u1", "un digest des news tech"), ("u2", "oui tout me va")]);
    // A sentence picks by the Session's reading: here, the default the proposal carries.
    let read = BTreeMap::from([("plan".to_owned(), Pick::Offer("recommended".to_owned()))]);
    let said = conversation.answered_by("u2", "oui tout me va", &read);
    assert!(said[0].contains("picked `recommended`"), "{said:?}");
    assert!(conversation.asked_ids().is_empty() && conversation.open().is_empty());
    (conversation, lines)
}

fn write_removing(
    conversation: &mut Conversation,
    lines: &Citations,
    source: &str,
    (rows, removed): (Value, Value),
) -> ToolReply {
    let rows = rows.as_array().cloned().unwrap_or_default();
    let removed = removed.as_array().cloned().unwrap_or_default();
    let texts = (source.to_owned(), "s".to_owned());
    conversation.write(lines, texts, (&rows, &removed), &mut |_| Ok(()))
}

fn write(
    conversation: &mut Conversation,
    lines: &Citations,
    source: &str,
    rows: Value,
) -> ToolReply {
    write_removing(conversation, lines, source, (rows, json!([])))
}

fn kinds(conversation: &Conversation) -> Vec<(String, ProvenanceKind, String)> {
    (conversation.bindings().iter())
        .map(|b| {
            (
                b.value.clone(),
                b.provenance.kind,
                b.provenance.message.clone(),
            )
        })
        .collect()
}

/// The accepted digest, proposed: HN, TC and DIGEST offered by u2.
fn proposed(asker: &Arc<Incarnation>) -> (Conversation, Citations) {
    let (mut conversation, lines) = accepted(asker);
    let rows = json!([
        offered(HN, "read_source", "u2"),
        offered(TC, "read_source", "u2"),
        offered(DIGEST, "output_path", "u2")
    ]);
    let reply = write(
        &mut conversation,
        &lines,
        &format!("{HN} {TC} {DIGEST}"),
        rows,
    );
    assert!(!reply.is_error, "{reply:?}");
    assert!(conversation.dropped().is_empty());
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S1".into(), Vec::new())),
        Ok(None)
    );
    (conversation, lines)
}

#[test]
fn an_accepted_offer_binds_its_values_as_offered_by_the_accepting_line() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = accepted(&asker);
    let rows = json!([
        offered(HN, "read_source", "u2"),
        offered(TC, "read_source", "u2"),
        offered(DIGEST, "output_path", "u2")
    ]);
    let reply = write(
        &mut conversation,
        &lines,
        &format!("{HN} {TC} {DIGEST}"),
        rows,
    );
    assert!(!reply.is_error, "{reply:?}");
    let kinds = kinds(&conversation);
    assert_eq!(kinds.len(), 3);
    assert!(
        kinds
            .iter()
            .all(|(_, kind, message)| *kind == ProvenanceKind::Offered && message == "u2")
    );
    // An offer stated as accepted by another line, or a value no option carries, is refused.
    for wrong in [
        offered(HN, "read_source", "u1"),
        offered("https://example.org", "read_source", "u2"),
    ] {
        let reply = write(&mut conversation, &lines, HN, json!([wrong]));
        assert!(
            reply.is_error && reply.text.contains("offered"),
            "{reply:?}"
        );
    }
}

#[test]
fn a_dropped_value_waits_for_the_persons_words_and_a_replaced_output_does_not() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, mut lines) = proposed(&asker);
    // TechCrunch dropped from the document and from the rows: the revision is written, never
    // proposed, and the value is named.
    let dropped = json!([retained(HN, "read_source"), retained(DIGEST, "output_path")]);
    let reply = write(
        &mut conversation,
        &lines,
        &format!("{HN} {DIGEST}"),
        dropped,
    );
    assert!(!reply.is_error, "{reply:?}");
    let why = conversation.dropped();
    assert!(
        why.len() == 1 && why[0].contains("techcrunch.com"),
        "{why:?}"
    );
    // Still in the document though no row names it: kept, with its own provenance.
    let silent = json!([retained(HN, "read_source"), retained(DIGEST, "output_path")]);
    write(
        &mut conversation,
        &lines,
        &format!("{HN} {TC} {DIGEST}"),
        silent,
    );
    assert!(conversation.dropped().is_empty());
    let tc = kinds(&conversation)
        .into_iter()
        .find(|(v, ..)| v == TC)
        .unwrap();
    assert_eq!((tc.1, tc.2.as_str()), (ProvenanceKind::Offered, "u2"));
    // Another authorized output replaces the accepted one: nothing is dropped.
    let elsewhere = json!([retained(HN, "read_source"), retained(TC, "read_source"),
        {"value": "./news/autre.md", "kind": "derived", "role": "output_path", "message": "u1",
         "excerpt": "un digest"}]);
    write(
        &mut conversation,
        &lines,
        &format!("{HN} {TC} ./news/autre.md"),
        elsewhere,
    );
    assert!(
        conversation.dropped().is_empty(),
        "{:?}",
        conversation.dropped()
    );
    // The person's own words remove a value: it is no longer theirs to keep.
    lines.record(20, Some(("u3".into(), "enleve techcrunch".into())));
    let rows = json!([retained(HN, "read_source"), retained(DIGEST, "output_path")]);
    let forged = json!([{"value": TC, "message": "u3", "excerpt": "garde tout"}]);
    let reply = write_removing(
        &mut conversation,
        &lines,
        &format!("{HN} {DIGEST}"),
        (rows.clone(), forged),
    );
    assert!(reply.is_error, "{reply:?}");
    let removed = json!([{"value": TC, "message": "u3", "excerpt": "enleve techcrunch"}]);
    let reply = write_removing(
        &mut conversation,
        &lines,
        &format!("{HN} {DIGEST}"),
        (rows, removed),
    );
    assert!(!reply.is_error, "{reply:?}");
    assert!(
        conversation.dropped().is_empty(),
        "{:?}",
        conversation.dropped()
    );
}

/// A request names two outputs and leaves where one's name starts open; the other output is
/// bound from the person's line. The choice of where the open name starts asks that name alone:
/// the bound output neither keeps it from being shown nor is unbound by its answer, and a revision
/// that drops the bound output is reported, never taken as replaced by the answer.
#[test]
fn an_open_names_choice_stands_beside_another_output() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let request = "Prépare un workflow qui lit articles.csv. Il faudra générer out/stock.json avec \
                   le total, puis un court résumé dans notes/stock.md.";
    let cited = citations(&[("u1", request)]);
    let summary = json!({"value": "notes/stock.md", "kind": "named", "role": "output_path",
        "message": "u1", "excerpt": "dans notes/stock.md"});
    let both = "nika: stock\n# writes out/stock.json and notes/stock.md\n";
    let reply = write(&mut conversation, &cited, both, json!([summary]));
    assert!(!reply.is_error, "{reply:?}");
    let readings = [
        "faudra générer out/stock.json",
        "générer out/stock.json",
        "out/stock.json",
    ];
    let options: Vec<Value> = (readings.iter().enumerate())
        .map(|(k, path)| {
            json!({"key": format!("r{k}"), "label": path, "recommended": k == 2,
                "values": [{"role": "output_path", "value": path}]})
        })
        .collect();
    let choice = json!({"key": "const.stock_path", "role": "output_path",
        "question": "Quel fichier désigne « faudra générer out/stock.json » ?",
        "options": options});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let open = conversation.open_choices(
        &cited,
        &json!({"open": [choice]}),
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    let open = open.expect("the open name's choice is shown beside the bound output");
    let shown = ("S0".to_owned(), open);
    assert_eq!(conversation.propose(&cited, &json!({}), shown), Ok(None));
    let lines = citations(&[("u1", request), ("u2", "le chemin seul")]);
    let read = BTreeMap::from([("const.stock_path".to_owned(), Pick::Offer("r2".to_owned()))]);
    conversation.answered_by("u2", "le chemin seul", &read);
    let outputs: Vec<&str> = (conversation.bindings().iter())
        .filter(|binding| binding.role == ValueRole::OutputPath)
        .map(|binding| binding.value.as_str())
        .collect();
    assert!(
        outputs.contains(&"notes/stock.md") && outputs.contains(&"out/stock.json"),
        "{outputs:?}"
    );
    let json_only = "nika: stock\n# writes out/stock.json\n";
    let reply = write(&mut conversation, &lines, json_only, json!([]));
    assert!(!reply.is_error, "{reply:?}");
    let dropped = conversation.dropped();
    assert!(
        dropped.iter().any(|why| why.contains("notes/stock.md")),
        "{dropped:?}"
    );
}

/// Two outputs and a third: where the event's name starts is open (`payload out/event.json`),
/// the report and the summary are other outputs.
const EVENTS: &str = "Lis events.csv, écris payload out/event.json pour chaque ligne, puis un \
                      rapport out/report.md et un résumé out/summary.md.";

/// The candidate: it carries every default the choices below show.
const EVENTS_SOURCE: &str =
    "nika: events\n# writes out/event.json, out/report.md and out/summary.md\n";

/// A choice of one output among `paths` (offers `o0`, `o1`, …), the last one its default.
fn output_choice(key: &str, paths: &[&str]) -> Value {
    let options: Vec<Value> = (paths.iter().enumerate())
        .map(|(k, path)| {
            json!({"key": format!("o{k}"), "label": path, "recommended": k + 1 == paths.len(),
                "values": [{"role": "output_path", "value": path}]})
        })
        .collect();
    json!({"key": key, "role": "output_path", "question": format!("{key} ?"), "options": options})
}

/// Where the open name `payload out/event.json` starts: a choice of its readings.
fn event_choice() -> Value {
    output_choice(
        "const.event_path",
        &["payload out/event.json", "out/event.json"],
    )
}

/// Another output's ordinary choice: the report's format.
fn report_choice() -> Value {
    output_choice("const.report_path", &["out/report.json", "out/report.md"])
}

/// The candidate written for the request, nothing asked yet.
fn events() -> (Conversation, Citations) {
    let mut conversation = Conversation::default();
    let lines = citations(&[("u1", EVENTS)]);
    let reply = write(&mut conversation, &lines, EVENTS_SOURCE, json!([]));
    assert!(!reply.is_error, "{reply:?}");
    (conversation, lines)
}

/// The candidate proposed with `choices` shown, after the person's `lines`; else why the Session
/// does not show them.
fn show(
    conversation: &mut Conversation,
    lines: &Citations,
    choices: &[Value],
) -> Result<(), String> {
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let call = format!("c{}", lines.last());
    let args = json!({"open": choices});
    let open = conversation.open_choices(
        lines,
        &args,
        Some(call.as_str()),
        (&mut mint, &mut |_: &str| None),
    )?;
    let proposed = conversation.propose(lines, &json!({}), ("S".to_owned(), open));
    assert_eq!(proposed, Ok(None));
    Ok(())
}

/// Asks `choice` again with no default, as the person's words `(message, excerpt)` reopen it;
/// the keys asked.
fn reopen(
    conversation: &mut Conversation,
    lines: &Citations,
    mut choice: Value,
    (message, excerpt): (&str, &str),
) -> Vec<String> {
    for option in choice["options"].as_array_mut().into_iter().flatten() {
        option["recommended"] = json!(false);
    }
    choice["reopens"] = json!({"message": message, "excerpt": excerpt});
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let args = json!({"questions": [choice]});
    let reply = conversation.ask(
        lines,
        &args,
        Some(message),
        (&mut mint, &mut |_: &str| None),
    );
    let reply: Value = serde_json::from_str(&reply.text).unwrap();
    assert_eq!(reply["refused"], json!([]), "{reply}");
    (reply["asked"].as_array().into_iter().flatten())
        .filter_map(|question| question["key"].as_str().map(str::to_owned))
        .collect()
}

/// The person's line `cite` picks, as the Session reads it, the offer of each choice by key.
fn pick(conversation: &mut Conversation, cite: &str, picked: &[(&str, &str)]) {
    let read: BTreeMap<String, Pick> = (picked.iter())
        .map(|(key, offer)| ((*key).to_owned(), Pick::Offer((*offer).to_owned())))
        .collect();
    conversation.answered_by(cite, "ces fichiers-là", &read);
}

/// One output bound by the offer `offer` of the question `key`, as [`outputs`] says it.
fn output(value: &str, key: &str, offer: &str, line: &str) -> String {
    format!("{value} · {key} · {key}/{offer} · Offered · {line}")
}

/// The outputs bound, sorted: each with its question, the question and offer of its provenance,
/// its kind and the person's line.
fn outputs(conversation: &Conversation) -> Vec<String> {
    let mut outputs: Vec<String> = (conversation.bindings().iter())
        .filter(|binding| binding.role == ValueRole::OutputPath)
        .map(|binding| {
            let provenance = &binding.provenance;
            let key = binding.key.as_deref().unwrap_or("-");
            let question = provenance.question.as_deref().unwrap_or("-");
            let offer = provenance.option.as_deref().unwrap_or("-");
            let (value, kind) = (&binding.value, provenance.kind);
            format!(
                "{value} · {key} · {question}/{offer} · {kind:?} · {}",
                provenance.message
            )
        })
        .collect();
    outputs.sort();
    outputs
}

/// Shown together, an open name's choice and another output's choice keep both outputs,
/// whichever the person's line answers first: picked, or kept as defaults by their consent.
#[test]
fn an_open_name_and_another_output_answered_together_both_stay_bound() {
    let picked = [
        output("out/event.json", "const.event_path", "o1", "u2"),
        output("out/report.json", "const.report_path", "o0", "u2"),
    ];
    let kept = [
        output("out/event.json", "const.event_path", "o1", "u2"),
        output("out/report.md", "const.report_path", "o1", "u2"),
    ];
    for choices in [
        [event_choice(), report_choice()],
        [report_choice(), event_choice()],
    ] {
        let first = choices[0]["key"].clone();
        let (mut conversation, lines) = events();
        show(&mut conversation, &lines, &choices).expect("both choices are shown");
        let both = [("const.event_path", "o1"), ("const.report_path", "o0")];
        pick(&mut conversation, "u2", &both);
        assert_eq!(outputs(&conversation), picked, "{first} answered first");
        let (mut conversation, lines) = events();
        show(&mut conversation, &lines, &choices).expect("both choices are shown");
        assert_eq!(conversation.consented("u2").len(), 2);
        assert_eq!(outputs(&conversation), kept, "{first} kept first");
    }
}

/// Once the person settled where an open name starts, another output's choice is still shown,
/// for that answer is not the other output's, and its answer keeps the open name's output; the
/// other way round as well.
#[test]
fn another_outputs_choice_is_shown_and_bound_beside_a_settled_open_name() {
    let event = |line: &str| output("out/event.json", "const.event_path", "o1", line);
    let report = |line: &str| output("out/report.json", "const.report_path", "o0", line);
    let orders = [
        (event_choice(), report_choice(), [event("u2"), report("u3")]),
        (report_choice(), event_choice(), [event("u3"), report("u2")]),
    ];
    let both = [("const.event_path", "o1"), ("const.report_path", "o0")];
    for (first, then, bound) in orders {
        let (mut conversation, lines) = events();
        show(&mut conversation, &lines, &[first]).expect("the first choice is shown");
        pick(&mut conversation, "u2", &both);
        let lines = citations(&[("u1", EVENTS), ("u2", "ces fichiers-là")]);
        let key = then["key"].clone();
        let shown = show(&mut conversation, &lines, &[then]);
        assert_eq!(shown, Ok(()), "{key} is shown beside the first answer");
        pick(&mut conversation, "u3", &both);
        assert_eq!(outputs(&conversation), bound, "{key} answered second");
    }
}

/// What is not another output is still one output: a choice that offers the settled open name
/// again is settled by it, a second ordinary output's choice is settled by the first, and two
/// ordinary choices answered together keep the later answer only.
#[test]
fn a_choice_of_the_same_output_is_still_settled_or_replaced() {
    let summary = output_choice("const.summary_path", &["out/summary.txt", "out/summary.md"]);
    let again = output_choice("const.event_file", &["out/event.ndjson", "out/event.json"]);
    let defaults = [("const.event_path", "o1"), ("const.report_path", "o1")];
    for (first, then) in [(event_choice(), again), (report_choice(), summary.clone())] {
        let (mut conversation, lines) = events();
        show(&mut conversation, &lines, &[first]).expect("the first choice is shown");
        pick(&mut conversation, "u2", &defaults);
        let lines = citations(&[("u1", EVENTS), ("u2", "ces fichiers-là")]);
        let refused = show(&mut conversation, &lines, &[then]).expect_err("already settled");
        assert!(
            refused.contains("already settled by the person"),
            "{refused}"
        );
    }
    let (mut conversation, lines) = events();
    show(&mut conversation, &lines, &[report_choice(), summary]).expect("both choices are shown");
    pick(
        &mut conversation,
        "u2",
        &[("const.report_path", "o1"), ("const.summary_path", "o1")],
    );
    let later = output("out/summary.md", "const.summary_path", "o1", "u2");
    assert_eq!(outputs(&conversation), [later]);
}

/// The person's later words reopen one output's choice: asking it again unbinds that choice's
/// answer only. Reopening the report keeps where the open name starts; reopening the open name
/// replaces its own answer and keeps the report.
#[test]
fn reopening_one_output_keeps_the_other() {
    let (mut conversation, lines) = events();
    let choices = [report_choice(), event_choice()];
    show(&mut conversation, &lines, &choices).expect("both choices are shown");
    pick(
        &mut conversation,
        "u2",
        &[("const.event_path", "o1"), ("const.report_path", "o1")],
    );
    let mut lines = citations(&[
        ("u1", EVENTS),
        ("u2", "ces fichiers-là"),
        ("u3", "finalement le rapport en JSON"),
    ]);
    let asked = reopen(
        &mut conversation,
        &lines,
        report_choice(),
        ("u3", "le rapport en JSON"),
    );
    assert_eq!(asked, ["const.report_path"]);
    let event = output("out/event.json", "const.event_path", "o1", "u2");
    assert_eq!(outputs(&conversation), std::slice::from_ref(&event));
    pick(&mut conversation, "u4", &[("const.report_path", "o0")]);
    let report = output("out/report.json", "const.report_path", "o0", "u4");
    assert_eq!(outputs(&conversation), [event, report.clone()]);
    let words = "plutôt payload out/event.json en entier";
    lines.record(13, Some(("u5".into(), words.into())));
    let excerpt = ("u5", "payload out/event.json en entier");
    let asked = reopen(&mut conversation, &lines, event_choice(), excerpt);
    assert_eq!(asked, ["const.event_path"]);
    assert_eq!(outputs(&conversation), std::slice::from_ref(&report));
    pick(&mut conversation, "u6", &[("const.event_path", "o0")]);
    let payload = output("payload out/event.json", "const.event_path", "o0", "u6");
    assert_eq!(outputs(&conversation), [report, payload]);
}

/// A choice that also offers one of an open name's readings asks for that name's output too: the
/// offers each question showed decide, never the value picked alone. Answered together with the
/// open name's choice, in either order, the later pick replaces the earlier output; a multiple
/// choice does as well.
#[test]
fn a_choice_offering_an_open_names_reading_replaces_its_output_in_either_order() {
    let format = output_choice(
        "const.event_format",
        &["out/event.ndjson", "out/event.json"],
    );
    let picks = [("const.event_path", "o1"), ("const.event_format", "o0")];
    let orders = [
        (
            [event_choice(), format.clone()],
            output("out/event.ndjson", "const.event_format", "o0", "u2"),
        ),
        (
            [format.clone(), event_choice()],
            output("out/event.json", "const.event_path", "o1", "u2"),
        ),
    ];
    for (choices, later) in orders {
        let first = choices[0]["key"].clone();
        let (mut conversation, lines) = events();
        show(&mut conversation, &lines, &choices).expect("both choices are shown");
        pick(&mut conversation, "u2", &picks);
        assert_eq!(outputs(&conversation), [later], "{first} answered first");
    }
    let mut several = format;
    several["multi_select"] = json!(true);
    let (mut conversation, lines) = events();
    show(&mut conversation, &lines, &[event_choice(), several]).expect("both choices are shown");
    let read = BTreeMap::from([
        ("const.event_path".to_owned(), Pick::Offer("o1".to_owned())),
        (
            "const.event_format".to_owned(),
            Pick::Offers(vec!["o0".to_owned()]),
        ),
    ]);
    conversation.answered_by("u2", "ces fichiers-là", &read);
    let later = output("out/event.ndjson", "const.event_format", "o0", "u2");
    assert_eq!(outputs(&conversation), [later]);
}

/// A line the person wrote in reply to a question Nika asked is located as that reply once the host
/// records it so, whether or not a value was bound from it, and every such line is kept: a second
/// reply to the same question keeps the first. A line that replied to no question is not one.
#[test]
fn every_reply_to_an_asked_question_is_located_bound_or_not() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let mut lines = citations(&[("u1", "écris un court message dans ./out/message.txt")]);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let question = json!({"questions": [{"key": "message_text", "question": "Quel texte ?",
        "free_text": true, "options": []}]});
    for (cite, at, reply) in [("u2", 4, "bonjour.nika"), ("u3", 7, "salut.nika")] {
        let posed = conversation.ask(
            &lines,
            &question,
            Some(cite),
            (&mut mint, &mut |_: &str| None),
        );
        assert!(posed.ends_turn, "{posed:?}");
        lines.record(at, Some((cite.into(), reply.into())));
        conversation.replied_by(cite);
        conversation.answered_by(cite, reply, &BTreeMap::new());
    }
    let words = "enregistre-le dans bonjour.nika";
    lines.record(10, Some(("u4".into(), words.into())));
    assert!(conversation.answers_in(&lines).is_empty());
    let replies: Vec<Value> = (conversation.replies_in(&lines).values())
        .map(|raw| serde_json::from_str::<Value>(raw).unwrap()["choice_context"].clone())
        .collect();
    let located = |cite: &str, at: u64, reply: &str| {
        let lines = [at, at + 1];
        json!({"human_citation": cite, "human_lines": lines, "human_reply": reply})
    };
    let both = [
        located("u2", 1, "bonjour.nika"),
        located("u3", 2, "salut.nika"),
    ];
    assert_eq!(replies, both);
}

#[test]
fn a_question_the_person_settled_is_answered_by_the_session() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = accepted(&asker);
    let rows = json!([
        offered(HN, "read_source", "u2"),
        offered(DIGEST, "output_path", "u2")
    ]);
    write(&mut conversation, &lines, &format!("{HN} {DIGEST}"), rows);
    let again = json!({"questions": [{"key": "output", "role": "output_path",
        "question": "Dans quel fichier ?", "options": []}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let reply = conversation.ask(&lines, &again, Some("c9"), (&mut mint, &mut |_: &str| None));
    assert!(!reply.ends_turn && !reply.is_error, "{reply:?}");
    assert!(reply.text.contains("news/digest.md"), "{reply:?}");
    assert!(conversation.questions().is_empty());
}

#[test]
fn a_settled_value_is_asked_again_only_on_the_persons_later_words() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, _) = accepted(&asker);
    let lines = citations(&[
        ("u1", "un digest des news tech"),
        ("u2", "oui tout me va"),
        ("u3", "garde le même résultat, mais change le nom"),
    ]);
    let rows = json!([
        offered(HN, "read_source", "u2"),
        offered(DIGEST, "output_path", "u2")
    ]);
    write(&mut conversation, &lines, &format!("{HN} {DIGEST}"), rows);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let name = |reopens: Value| {
        json!({"questions": [{"key": "new_name", "role": "output_path",
            "question": "Quel nouveau nom ?", "options": [], "reopens": reopens}]})
    };
    // Words from before the value was settled, or not in the cited line, reopen nothing.
    let unsettling = [
        json!({"message": "u1", "excerpt": "un digest"}),
        json!({"message": "u3", "excerpt": "change la source"}),
    ];
    for reopens in unsettling {
        let args = name(reopens);
        let reply = conversation.ask(&lines, &args, Some("c8"), (&mut mint, &mut |_: &str| None));
        assert!(!reply.ends_turn, "a settled value stays settled");
        assert!(reply.text.contains("already settled"), "given back");
    }
    let args = name(json!({"message": "u3", "excerpt": "change le nom"}));
    let reply = conversation.ask(&lines, &args, Some("c9"), (&mut mint, &mut |_: &str| None));
    assert!(reply.ends_turn, "the person's later words reopen the value");
    assert_eq!(conversation.questions().len(), 1);
    let released = !(conversation.bindings().iter()).any(|b| b.value == DIGEST);
    assert!(released, "the old name is the question's now");
}

#[test]
fn an_answer_the_author_reads_is_bound_by_its_question_and_the_rest_asked_again() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let first = json!({"questions": [
        {"key": "team_webhook", "role": "value", "question": "Équipe ?", "options": []},
        {"key": "support_webhook", "role": "value", "question": "Support ?", "options": []}]});
    conversation.ask(
        &Citations::default(),
        &first,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    let before = conversation.asked_ids();
    let line = "equipe : https://hooks.example.org/team, support je sais pas";
    conversation.answered_by("u2", line, &BTreeMap::new());
    let lines = citations(&[
        ("u1", "envoie aux webhooks"),
        (
            "u2",
            "equipe : https://hooks.example.org/team, support je sais pas",
        ),
    ]);
    let rest = json!({"answered": [{"key": "team_webhook", "value": "https://hooks.example.org/team",
            "message": "u2", "excerpt": "https://hooks.example.org/team"}],
        "questions": [{"key": "support_webhook", "role": "value", "question": "Support ?",
            "options": []}]});
    let reply = conversation.ask(&lines, &rest, Some("c2"), (&mut mint, &mut |_: &str| None));
    assert!(reply.ends_turn, "{reply:?}");
    let team = &conversation.bindings()[0];
    assert_eq!(team.key.as_deref(), Some("team_webhook"));
    assert_eq!(team.provenance.kind, ProvenanceKind::Answered);
    let now = &conversation.questions()[0];
    assert_eq!(now.key, "support_webhook");
    assert!(!before.contains(&now.id), "a new identity");
    // A value the cited line does not hold binds nothing.
    let forged = json!({"answered": [{"key": "support_webhook", "value": "https://evil.example",
        "message": "u2", "excerpt": "support"}]});
    let reply = conversation.ask(
        &lines,
        &forged,
        Some("c3"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(reply.text.contains("not in the person's line"), "{reply:?}");
    assert_eq!(conversation.bindings().len(), 1);
}

#[test]
fn words_cover_only_the_revision_whose_proposal_they_answered() {
    let asker = Arc::new(Incarnation);
    let said = json!({"acts": ["save", "run"],
        "authorized_by": {"message": "u3", "excerpt": "enregistre-le puis lance-le"}});
    let u3 = || Some(("u3".into(), "parfait, enregistre-le puis lance-le".into()));
    // Another scope: nothing authorized, the new proposal waits.
    let (mut conversation, mut lines) = proposed(&asker);
    lines.record(20, u3());
    let refused = conversation.propose(&lines, &said, ("S2".into(), Vec::new()));
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        conversation.shown.as_ref().map(|s| s.scope.as_str()),
        Some("S2")
    );
    // The same scope the words answered: save and run.
    let (mut conversation, mut lines) = proposed(&asker);
    lines.record(20, u3());
    let acts = conversation.propose(&lines, &said, ("S1".into(), Vec::new()));
    assert_eq!(
        acts,
        Ok(Some(Acts {
            save: true,
            run: true,
            revised: false
        }))
    );
    // Those words ran that revision: a repair proposed on them, whatever it changed, is not theirs.
    let again = conversation.propose(&lines, &said, ("S2".into(), Vec::new()));
    let stale = again.is_err_and(|why| why.contains("`u3` does not authorize it"));
    assert!(stale, "stale words authorize nothing");
    // Words written before the proposal was shown authorize nothing.
    let (mut conversation, lines) = proposed(&asker);
    let early =
        json!({"acts": ["save"], "authorized_by": {"message": "u1", "excerpt": "un digest"}});
    assert!(
        conversation
            .propose(&lines, &early, ("S1".into(), Vec::new()))
            .is_err()
    );
}

#[test]
fn a_replacement_keeps_nothing_of_the_former_request() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, mut lines) = proposed(&asker);
    lines.record(20, Some(("u3".into(), "non laisse tomber tout ca".into())));
    let forged = conversation.replace(&lines, &json!({"message": "u3", "excerpt": "tout effacer"}));
    assert!(forged.is_error, "{forged:?}");
    let reply = conversation.replace(
        &lines,
        &json!({"message": "u3", "excerpt": "laisse tomber tout ca"}),
    );
    assert!(!reply.is_error, "{reply:?}");
    assert!(conversation.bindings().is_empty() && conversation.candidate().is_none());
    assert!(conversation.delegations().is_empty() && conversation.questions().is_empty());
    assert!(conversation.dropped().is_empty());
    assert_eq!(conversation.since(), 20);
    assert_eq!(
        lines.stated(conversation.since()),
        "non laisse tomber tout ca"
    );
}

#[test]
fn a_reopen_keeps_the_evidence_and_no_authority() {
    let asker = Arc::new(Incarnation);
    let (conversation, _) = proposed(&asker);
    let kept = conversation.kept();
    let restored = Conversation::restored(&kept).unwrap();
    assert_eq!(kinds(&restored), kinds(&conversation));
    let number = conversation.candidate().map(|c| c.number);
    assert_eq!(restored.candidate().map(|c| c.number), number);
    assert!(restored.shown.is_none() && restored.questions().is_empty());
    // A kept value is still the person's to keep after the reopen.
    let lines = citations(&[("u1", "un digest des news tech"), ("u2", "oui tout me va")]);
    let mut restored = restored;
    let rows = json!([
        retained(HN, "read_source"),
        retained(TC, "read_source"),
        retained(DIGEST, "output_path")
    ]);
    let reply = write(&mut restored, &lines, &format!("{HN} {TC} {DIGEST}"), rows);
    assert!(!reply.is_error, "{reply:?}");
    assert!(Conversation::restored(&json!({"version": 9})).is_none());
}

const YC: &str = "https://www.ycombinator.com/blog/";

/// « Which source for Y Combinator? », Hacker News its default when a proposal shows it.
fn source_choice() -> Value {
    json!({"key": "yc_source", "role": "read_source",
        "question": "Pour « Y Combinator », quelle source ?", "options": [
            {"key": "hackernews", "label": "Hacker News", "recommended": true,
             "values": [{"role": "read_source", "value": HN, "name": "Hacker News"}]},
            {"key": "yc_blog", "label": "Le blog YC",
             "values": [{"role": "read_source", "value": YC, "name": "Le blog YC"}]}]})
}

/// The same question asked after the request u1, with no recommended option.
fn ask_source(conversation: &mut Conversation, lines: &Citations, asker: &Arc<Incarnation>) {
    let mut question = source_choice();
    question["options"][0]["recommended"] = json!(false);
    let source = json!({"questions": [question]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), asker);
    let reply = conversation.ask(lines, &source, Some("c1"), (&mut mint, &mut |_: &str| None));
    assert!(reply.ends_turn, "{reply:?}");
}

/// The same choice shown by a proposal with its default, after the request u1.
fn show_source(conversation: &mut Conversation, asker: &Arc<Incarnation>) {
    let request = citations(&[("u1", "les news de Y Combinator")]);
    let reply = write(conversation, &request, HN, json!([]));
    assert!(!reply.is_error, "{reply:?}");
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), asker);
    let open = conversation.open_choices(
        &request,
        &json!({"open": [source_choice()]}),
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    let shown = (
        "S0".to_owned(),
        open.expect("the source is shown with its default"),
    );
    assert_eq!(conversation.propose(&request, &json!({}), shown), Ok(None));
}

fn source_offered(value: &str, option: &str, message: &str) -> Value {
    json!({"value": value, "kind": "offered", "role": "read_source", "message": message,
        "question": "yc_source", "option": option})
}

/// A refusal picks no offer, so no value of any offer is admitted as offered from it.
#[test]
fn a_refusal_picks_no_offer_and_admits_none() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = citations(&[("u1", "les news de Y Combinator"), ("u2", "non")]);
    ask_source(&mut conversation, &lines, &asker);
    let said = conversation.answered_by("u2", "non", &BTreeMap::new());
    assert!(said[0].contains("declined"), "{said:?}");
    assert!(conversation.bindings().is_empty());
    for (value, option) in [(HN, "hackernews"), (YC, "yc_blog")] {
        let reply = write(
            &mut conversation,
            &lines,
            value,
            json!([source_offered(value, option, "u2")]),
        );
        assert!(
            reply.is_error && reply.text.contains("declined the offers"),
            "{reply:?}"
        );
    }
    // A sentence the reading finds no offer in admits none either.
    let mut conversation = Conversation::default();
    let lines = citations(&[
        ("u1", "les news de Y Combinator"),
        ("u2", "non merci, autre chose"),
    ]);
    ask_source(&mut conversation, &lines, &asker);
    let read = BTreeMap::from([("yc_source".to_owned(), Pick::Nothing)]);
    conversation.answered_by("u2", "non merci, autre chose", &read);
    let reply = write(
        &mut conversation,
        &lines,
        YC,
        json!([source_offered(YC, "yc_blog", "u2")]),
    );
    assert!(
        reply.is_error && reply.text.contains("picked none"),
        "{reply:?}"
    );
}

/// The Session binds the offer the person picked, and only that offer is admitted.
#[test]
fn only_the_offer_the_person_picked_is_admitted() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = citations(&[("u1", "les news de Y Combinator"), ("u2", "le blog yc stp")]);
    ask_source(&mut conversation, &lines, &asker);
    let read = BTreeMap::from([("yc_source".to_owned(), Pick::Offer("yc_blog".to_owned()))]);
    conversation.answered_by("u2", "le blog yc stp", &read);
    let bound = kinds(&conversation);
    assert_eq!(
        bound,
        [(YC.to_owned(), ProvenanceKind::Offered, "u2".to_owned())]
    );
    let other = json!([source_offered(HN, "hackernews", "u2")]);
    let reply = write(&mut conversation, &lines, HN, other);
    assert!(
        reply.is_error && reply.text.contains("picked `yc_blog`"),
        "{reply:?}"
    );
    let picked = json!([source_offered(YC, "yc_blog", "u2")]);
    let reply = write(&mut conversation, &lines, YC, picked);
    assert!(!reply.is_error, "{reply:?}");
}

/// A reply that deflects takes the default the proposal shows, visibly delegated.
#[test]
fn a_deflection_takes_the_recommendation_as_delegated() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let deflects = "Je t'ai déjà donné les sources.";
    let lines = citations(&[("u1", "les news de Y Combinator"), ("u2", deflects)]);
    show_source(&mut conversation, &asker);
    let read = BTreeMap::from([("yc_source".to_owned(), Pick::Delegated)]);
    let said = conversation.answered_by("u2", deflects, &read);
    assert!(said[0].contains("recommended `hackernews`"), "{said:?}");
    let binding = &conversation.bindings()[0];
    assert_eq!(
        (
            binding.value.as_str(),
            binding.provenance.kind,
            binding.key.as_deref()
        ),
        (HN, ProvenanceKind::Delegated, Some("yc_source"))
    );
    assert_eq!(binding.provenance.excerpt.as_deref(), Some(deflects));
    assert_eq!(binding.provenance.option.as_deref(), Some("hackernews"));
    let delegation = &conversation.delegations()[0];
    assert_eq!(
        (delegation.message.as_str(), delegation.scope),
        ("u2", ValueRole::ReadSource)
    );
    // The non-recommended offer is not the person's: refused as offered.
    let reply = write(
        &mut conversation,
        &lines,
        YC,
        json!([source_offered(YC, "yc_blog", "u2")]),
    );
    assert!(
        reply.is_error && reply.text.contains("left the choice to Nika"),
        "{reply:?}"
    );
}

/// A value left to the author is chosen once, bound delegated, never asked again.
#[test]
fn a_value_left_to_the_author_is_never_asked_again() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = citations(&[
        ("u1", "un digest des news tech"),
        ("u2", "change le nom"),
        ("u3", "Oui, fais au mieux."),
    ]);
    let name = json!({"questions": [{"key": "new_name", "role": "output_path",
        "question": "Quel nouveau nom ?", "options": [],
        "reopens": {"message": "u2", "excerpt": "change le nom"}}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    conversation.ask(&lines, &name, Some("c1"), (&mut mint, &mut |_: &str| None));
    let read = BTreeMap::from([("new_name".to_owned(), Pick::Delegated)]);
    let said = conversation.answered_by("u3", "Oui, fais au mieux.", &read);
    assert!(said[0].contains("never ask it again"), "{said:?}");
    let reply = conversation.ask(&lines, &name, Some("c2"), (&mut mint, &mut |_: &str| None));
    assert!(
        !reply.ends_turn && reply.text.contains("left it to you"),
        "{reply:?}"
    );
    let chosen = json!([{"value": "./news/actualites-tech.md", "kind": "delegated",
        "role": "output_path", "message": "u3", "excerpt": "fais au mieux"}]);
    let reply = write(
        &mut conversation,
        &lines,
        "./news/actualites-tech.md",
        chosen,
    );
    assert!(!reply.is_error, "{reply:?}");
}

/// A question whose every offered value is already bound is not asked.
#[test]
fn a_question_over_values_already_bound_is_not_asked() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = accepted(&asker);
    let confirm = json!({"questions": [{"key": "confirm", "question": "C'est bon ?",
        "options": [{"key": "ok", "label": "Oui", "recommended": true,
            "values": [{"role": "read_source", "value": HN}, {"role": "output_path", "value": DIGEST}]}]}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let reply = conversation.ask(
        &lines,
        &confirm,
        Some("c9"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(!reply.ends_turn, "{reply:?}");
    let result: Value = serde_json::from_str(&reply.text).unwrap();
    assert_eq!(result["asked"], json!([]));
    assert_eq!(result["bound"], json!([]));
    assert_eq!(result["refused"][0]["reason"], "settled");
    assert_eq!(
        result["refused"][0]["settled"],
        json!({"message": "u2", "value": DIGEST})
    );
    assert!(conversation.asked_ids().is_empty());
}

/// Addendum to F2: words Nika had to ask about never name the value they were ambiguous about;
/// a later line that names it does.
#[test]
fn words_nika_asked_about_never_name_the_value() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let mut lines = citations(&[("u1", "les dernières actualités tech de Y Combinator")]);
    ask_source(&mut conversation, &lines, &asker);
    lines.record(
        4,
        Some(("u2".into(), "Je t'ai déjà donné les sources.".into())),
    );
    conversation.answered_by("u2", "Je t'ai déjà donné les sources.", &BTreeMap::new());
    let named = |message: &str, excerpt: &str| {
        json!([{"value": YC, "kind": "named", "role": "read_source", "message": message,
            "excerpt": excerpt}])
    };
    let reply = write(&mut conversation, &lines, YC, named("u1", "Y Combinator"));
    assert!(
        reply.is_error && reply.text.contains("answers `yc_source`"),
        "{reply:?}"
    );
    lines.record(7, Some(("u3".into(), "prends le blog YC".into())));
    let reply = write(&mut conversation, &lines, YC, named("u3", "le blog YC"));
    assert!(!reply.is_error, "{reply:?}");
}

/// A removal must cite words that name the value, and a kept value must still be
/// in the document.
#[test]
fn a_removal_names_the_value_and_a_kept_value_is_in_the_document() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, mut lines) = proposed(&asker);
    lines.record(
        20,
        Some((
            "u3".into(),
            "En fait je voulais les articles d'aujourd'hui.".into(),
        )),
    );
    let rows = json!([retained(TC, "read_source"), retained(DIGEST, "output_path")]);
    let unnamed = json!([{"value": HN, "message": "u3", "excerpt": "les articles d'aujourd'hui"}]);
    let reply = write_removing(
        &mut conversation,
        &lines,
        &format!("{TC} {DIGEST}"),
        (rows, unnamed),
    );
    assert!(
        reply.is_error && reply.text.contains("does not name"),
        "{reply:?}"
    );
    lines.record(23, Some(("u4".into(), "enleve hacker news".into())));
    let rows = json!([retained(TC, "read_source"), retained(DIGEST, "output_path")]);
    let named = json!([{"value": HN, "message": "u4", "excerpt": "hacker news"}]);
    let reply = write_removing(
        &mut conversation,
        &lines,
        &format!("{TC} {DIGEST}"),
        (rows, named),
    );
    assert!(!reply.is_error, "{reply:?}");
    // A value stated as kept that the document no longer carries.
    let gone = json!([retained(TC, "read_source"), retained(DIGEST, "output_path")]);
    let reply = write(&mut conversation, &lines, DIGEST, gone);
    assert!(
        reply.is_error && reply.text.contains("no longer carries"),
        "{reply:?}"
    );
}

/// A value the person named but did not type is refused as answered with what to state instead,
/// never as words missing from their line: the author retried other excerpts of the same line for
/// ever (live gauntlet, 2026-10-16: « Grok 4.7 (xAI) » for `xai/grok-4.7`, four refusals).
#[test]
fn a_named_value_stated_as_answered_is_told_to_be_named() {
    let said = "Pour l'étape IA du workflow utilise aussi Grok 4.7 (xAI) stp.";
    let lines = citations(&[("u1", said)]);
    let mut conversation = Conversation::default();
    let row = |kind: &str| {
        json!([{"value": "xai/grok-4.7", "kind": kind, "role": "run_model", "message": "u1",
            "excerpt": "Grok 4.7 (xAI)"}])
    };
    let source = "model: xai/grok-4.7";
    let reply = write(&mut conversation, &lines, source, row("answered"));
    assert!(reply.is_error, "{reply:?}");
    assert!(reply.text.contains("state it as named"), "{reply:?}");
    assert!(
        !reply.text.contains("which is not in the person's line"),
        "{reply:?}"
    );
    let reply = write(&mut conversation, &lines, source, row("named"));
    assert!(!reply.is_error, "{reply:?}");
}

/// Words that give a value its replacement remove it without naming it: the replacing value,
/// typed in the line, stands for the removal (the tickets revision: 48 hours raised to 72).
#[test]
fn words_that_give_a_replacement_remove_the_value_they_replace() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, mut lines) = proposed(&asker);
    let le_monde = "https://www.lemonde.fr/international/";
    let said = format!("prends plutot {le_monde} a la place");
    lines.record(20, Some(("u3".into(), said.clone())));
    let typed = json!({"value": le_monde, "kind": "answered", "role": "read_source",
        "message": "u3", "excerpt": le_monde});
    let rows = json!([
        retained(HN, "read_source"),
        retained(DIGEST, "output_path"),
        typed
    ]);
    let source = format!("{HN} {le_monde} {DIGEST}");
    let unnamed = json!([{"value": TC, "message": "u3", "excerpt": said}]);
    let reply = write_removing(&mut conversation, &lines, &source, (rows.clone(), unnamed));
    assert!(
        reply.is_error && reply.text.contains("does not name"),
        "{reply:?}"
    );
    let replaced = json!([{"value": TC, "message": "u3", "excerpt": said,
        "replaced_by": le_monde}]);
    let reply = write_removing(&mut conversation, &lines, &source, (rows, replaced));
    assert!(!reply.is_error, "{reply:?}");
    assert!(
        conversation.dropped().is_empty(),
        "{:?}",
        conversation.dropped()
    );
}

/// A value whose choice the person left to Nika is replaced within that choice: the line that
/// delegated it stands for removing the value it replaces (the news rename).
#[test]
fn a_value_whose_choice_was_left_to_nika_is_replaced_within_it() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, mut lines) = proposed(&asker);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let which = json!({"questions": [{"key": "sources", "role": "read_source",
        "question": "Quelles sources ?"}]});
    let waiting = conversation.ask(&lines, &which, Some("c9"), (&mut mint, &mut |_: &str| None));
    assert!(waiting.ends_turn, "{waiting:?}");
    lines.record(20, Some(("u3".into(), "Oui, fais au mieux.".into())));
    let read = BTreeMap::from([("sources".to_owned(), Pick::Delegated)]);
    conversation.answered_by("u3", "Oui, fais au mieux.", &read);
    let le_monde = "https://www.lemonde.fr/international/";
    let chosen = json!({"value": le_monde, "kind": "delegated", "role": "read_source",
        "message": "u3", "excerpt": "fais au mieux"});
    let rows = json!([
        retained(HN, "read_source"),
        retained(DIGEST, "output_path"),
        chosen
    ]);
    let removed = json!([{"value": TC, "message": "u3", "excerpt": "fais au mieux"}]);
    let source = format!("{HN} {le_monde} {DIGEST}");
    let reply = write_removing(&mut conversation, &lines, &source, (rows, removed));
    assert!(!reply.is_error, "{reply:?}");
}

/// A question that recommends an option has a default: the ask door refuses it with that reason,
/// the proposal shows it in `open`, and a question no default can stand for is asked.
#[test]
fn a_question_with_a_default_is_refused_and_one_without_is_asked() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = citations(&[(
        "u1",
        "envoie chaque soir les rappels de l'agenda à mon associé",
    )]);
    let questions = json!({"questions": [
        {"key": "field", "question": "Quel champ montrer pour chaque rendez-vous ?", "options": [
            {"key": "title", "label": "Le titre", "recommended": true,
             "values": [{"role": "value", "value": "title"}]},
            {"key": "id", "label": "L'identifiant", "values": [{"role": "value", "value": "id"}]}]},
        {"key": "partner", "question": "Quel est l'e-mail de ton associé ?", "free_text": true}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let reply = conversation.ask(
        &lines,
        &questions,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(reply.ends_turn, "{reply:?}");
    let keys: Vec<&str> = (conversation.questions().iter())
        .map(|q| q.key.as_str())
        .collect();
    assert_eq!(keys, ["partner"]);
    let text: Value = serde_json::from_str(&reply.text).expect("the reply is JSON");
    let refused = text["refused"].as_array().cloned().unwrap_or_default();
    assert_eq!(refused.len(), 1, "one question refused");
    assert_eq!(
        (refused[0]["key"].as_str(), refused[0]["reason"].as_str()),
        (Some("field"), Some("default"))
    );
    let why = refused[0]["why"].as_str().unwrap_or_default();
    assert!(
        why.contains("propose with this choice in `open`"),
        "the author reads where the choice goes"
    );
}

/// An output in the project always has a name the author derives: a question for one is not
/// asked, unless the person's later words ask to change it.
#[test]
fn an_output_name_is_not_asked_unless_the_persons_words_reopen_it() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let mut lines = citations(&[("u1", "un digest des news tech dans un dossier du projet")]);
    let name = |reopens: Option<Value>| {
        let mut question = json!({"key": "output", "role": "output_path",
            "question": "Quel nom pour le fichier ?", "free_text": true});
        if let Some(reopens) = reopens {
            question["reopens"] = reopens;
        }
        json!({"questions": [question]})
    };
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let reply = conversation.ask(
        &lines,
        &name(None),
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(
        !reply.ends_turn && conversation.questions().is_empty(),
        "{reply:?}"
    );
    let text: Value = serde_json::from_str(&reply.text).expect("the reply is JSON");
    let refused = text["refused"].as_array().cloned().unwrap_or_default();
    assert_eq!(refused.len(), 1, "one question refused");
    assert_eq!(
        (refused[0]["key"].as_str(), refused[0]["reason"].as_str()),
        (Some("output"), Some("output"))
    );
    let why = refused[0]["why"].as_str().unwrap_or_default();
    assert!(
        why.contains("always has a name you can derive"),
        "the author reads why it is not asked"
    );
    lines.record(4, Some(("u2".into(), "change le nom du fichier".into())));
    let reopens = json!({"message": "u2", "excerpt": "change le nom"});
    let args = name(Some(reopens));
    let reply = conversation.ask(&lines, &args, Some("c2"), (&mut mint, &mut |_: &str| None));
    assert!(reply.ends_turn, "{reply:?}");
    assert_eq!(conversation.questions().len(), 1);
}

/// The workflow a conversation saved is kept with the witness of its bytes across a reopen and a
/// new request: what a later revision of it replaces.
#[test]
fn the_saved_workflow_survives_a_reopen_and_a_new_request() {
    let lines = citations(&[
        ("u1", "lis ./notes/semaine.md"),
        ("u2", "refais tout autrement"),
    ]);
    let mut conversation = Conversation::default();
    conversation.saved_at("./weekly.nika", "nika: weekly\n");
    let witness = Witness::of(b"nika: weekly\n").0;
    assert_eq!(
        conversation.saved(),
        Some(("weekly.nika", witness.as_str()))
    );
    let reopened = Conversation::restored(&conversation.kept()).unwrap();
    assert_eq!(reopened.saved(), conversation.saved());
    let replaced = json!({"message": "u2", "excerpt": "refais tout autrement"});
    assert!(!conversation.replace(&lines, &replaced).is_error);
    assert_eq!(
        conversation.saved().map(|(path, _)| path),
        Some("weekly.nika")
    );
}

/// The initial explicit mandate can cover preparation and saving once; a later instruction or
/// a reopened conversation cannot turn the same words into another save or run.
#[test]
fn an_initial_preparation_mandate_is_scoped_and_consumed_once() {
    let line = "Crée le workflow, enregistre et lance maintenant";
    let mut lines = citations(&[("u1", line)]);
    let mut conversation = Conversation::default();
    assert!(
        !write(
            &mut conversation,
            &lines,
            "nika: hello\ntasks: {}\n",
            json!([])
        )
        .is_error
    );
    let args = json!({"acts":["save","run"],"authorized_by":{"message":"u1","excerpt":line}});
    let acts = conversation
        .propose(&lines, &args, ("S1".into(), Vec::new()))
        .unwrap()
        .unwrap();
    assert!(acts.save && acts.run && acts.revised);
    assert!(
        conversation
            .propose(&lines, &args, ("S1".into(), Vec::new()))
            .is_err()
    );
    let mut restored = Conversation::restored(&conversation.kept()).unwrap();
    assert!(!write(&mut restored, &lines, "nika: hello\ntasks: {}\n", json!([])).is_error);
    assert!(
        restored
            .propose(&lines, &args, ("S1".into(), Vec::new()))
            .is_err()
    );
    let mut fresh = Conversation::default();
    assert!(!write(&mut fresh, &lines, "nika: hello\ntasks: {}\n", json!([])).is_error);
    lines.record(8, Some(("u2".into(), "Finalement, ne lance rien".into())));
    assert!(
        fresh
            .propose(&lines, &args, ("S1".into(), Vec::new()))
            .is_err()
    );
}

#[test]
fn a_bound_answer_cannot_answer_another_question_with_the_same_offer_key() {
    let asker = Arc::new(Incarnation);
    let first_id = QuestionId::new("first".into(), &asker);
    let second_id = QuestionId::new("second".into(), &asker);
    let make = |id: QuestionId, key: &str, value: &str| {
        AskedQuestion::new(id, key, "Choose a source").with_options(
            vec![
                nika_session_change::work::Offer::new("yes", "Use it", true).with_values(vec![
                    nika_session_change::work::OfferValue::new(ValueRole::ReadSource, value, None),
                ]),
            ],
            true,
            false,
        )
    };
    let mut conversation = Conversation {
        asked: vec![
            make(first_id.clone(), "first", HN),
            make(second_id.clone(), "second", TC),
        ],
        ..Conversation::default()
    };
    let said = conversation.answered_one_by(&first_id, "u2", "yes", &BTreeMap::new());
    assert_eq!(said.len(), 1);
    assert_eq!(conversation.asked_ids(), std::slice::from_ref(&second_id));
    assert_eq!(conversation.bindings().len(), 1);
    assert_eq!(conversation.bindings()[0].value, HN);
    assert_eq!(conversation.bindings()[0].key.as_deref(), Some("first"));
    assert!(!conversation.answered.contains_key("second"));
    let stale = conversation.answered_one_by(&first_id, "u3", "yes", &BTreeMap::new());
    assert_eq!(stale, Vec::<String>::new());
    assert_eq!(conversation.asked_ids(), [second_id]);
    assert_eq!(conversation.bindings().len(), 1);
}

/// A saved byte witness prevents a false proposal reminder without reviving the proposal.
#[test]
fn saved_candidate_bytes_need_no_new_proposal_after_reopen_but_new_bytes_do() {
    let lines = citations(&[("u1", "Create and save this workflow.")]);
    let source = "nika: hello\ntasks: {}\n";
    let mut conversation = Conversation::default();
    assert!(!write(&mut conversation, &lines, source, json!([])).is_error);
    assert!(
        conversation.unproposed(),
        "a new unsaved draft still needs a proposal"
    );
    let unsaved = Conversation::restored(&conversation.kept()).unwrap();
    assert!(unsaved.unproposed());
    conversation.saved_at("hello.nika", source);
    assert!(!conversation.unproposed());
    let mut resumed = Conversation::restored(&conversation.kept()).unwrap();
    assert!(!resumed.unproposed());
    assert!(resumed.shown.is_none());
    assert!(resumed.shown_verdict().is_none());
    assert!(resumed.questions().is_empty() && resumed.open().is_empty());
    assert_eq!(resumed.consumed_through, conversation.consumed_through);
    assert!(
        !write(
            &mut resumed,
            &lines,
            "nika: changed\ntasks: {}\n",
            json!([])
        )
        .is_error
    );
    assert!(
        resumed.unproposed(),
        "the old save never covers new candidate bytes"
    );
    assert!(
        Conversation::restored(&resumed.kept())
            .unwrap()
            .unproposed()
    );
    assert_eq!(resumed.saved(), conversation.saved());
    // An identical rewrite changes the revision number, not the saved bytes or any authority.
    assert!(!write(&mut resumed, &lines, source, json!([])).is_error);
    assert!(!resumed.unproposed());
    assert!(resumed.shown_verdict().is_none());
}

/// Questions the person dropped (« annuler ») close with nothing bound, and asked again each is
/// refused as dropped, naming the line that dropped it (gauntlet wave 3).
#[test]
fn a_dropped_question_binds_nothing_and_is_never_asked_again() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = citations(&[("u1", "un résumé des news tech"), ("u2", "annuler")]);
    let format = json!({"questions": [{"key": "format", "question": "Quel format ?",
        "free_text": true, "options": []}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let first = conversation.ask(
        &lines,
        &format,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(first.ends_turn, "{first:?}");
    let dropped = conversation.drop_questions("u2");
    let keys: Vec<&str> = dropped.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["format"]);
    assert!(conversation.questions().is_empty());
    assert!(conversation.bindings().is_empty());
    let again = conversation.ask(
        &lines,
        &format,
        Some("c2"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(!again.ends_turn, "{again:?}");
    let text: Value = serde_json::from_str(&again.text).expect("the reply is JSON");
    assert_eq!(text["refused"][0]["reason"], "dropped", "{text}");
    let why = text["refused"][0]["why"].as_str().unwrap_or_default();
    assert!(why.contains("with u2"), "{why}");
    assert!(conversation.questions().is_empty());
}

/// A discarded proposal over a saved workflow leaves the saved bytes as the candidate, as the
/// next revision with the values they still carry: a later change revises what is saved, never
/// the discarded draft (live gauntlet r08, 2026-10-16: it came back as a second workflow). With
/// nothing saved, no candidate stands and the next one follows the discarded revision's number.
#[test]
fn a_discarded_proposal_leaves_the_saved_workflow_as_the_candidate() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = proposed(&asker);
    let saved = format!("nika: digest\n# {HN} {TC} {DIGEST}\n");
    conversation.saved_at("digest.nika", &saved);
    let renamed = format!("nika: other-digest\n# {HN} {DIGEST}\n");
    assert!(!write(&mut conversation, &lines, &renamed, json!([])).is_error);
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S2".into(), Vec::new())),
        Ok(None)
    );
    let shown = conversation.candidate().unwrap().number;
    let left = conversation.discarded(Some(saved.clone()));
    assert_eq!(
        left,
        Discarded::Saved {
            discarded: shown,
            revision: shown + 1,
            path: "digest.nika".to_owned()
        }
    );
    let candidate = conversation.candidate().unwrap();
    assert_eq!(
        (candidate.number, candidate.source.as_str()),
        (shown + 1, saved.as_str())
    );
    assert!(
        !conversation.unproposed(),
        "the saved bytes need no proposal"
    );
    assert!(conversation.shown_verdict().is_none() && conversation.open().is_empty());
    let note = left.note().unwrap();
    assert!(
        note.contains("digest.nika") && note.contains("discarded"),
        "{note}"
    );
    // Nothing was shown any more: a second discard discards nothing.
    assert_eq!(conversation.discarded(Some(saved)), Discarded::Nothing);
}

#[test]
fn a_discarded_proposal_with_nothing_saved_leaves_no_candidate() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = proposed(&asker);
    let shown = conversation.candidate().unwrap().number;
    assert_eq!(
        conversation.discarded(None),
        Discarded::Unsaved { discarded: shown }
    );
    assert!(conversation.candidate().is_none() && !conversation.unproposed());
    let source = format!("{HN} {DIGEST}");
    assert!(!write(&mut conversation, &lines, &source, json!([])).is_error);
    assert_eq!(conversation.candidate().unwrap().number, shown + 1);
}
