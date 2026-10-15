// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;
use nika_session_change::outcome::Incarnation;

struct Capture(Arc<Mutex<Vec<Value>>>);

impl Desk for Capture {
    fn read(&mut self, _: &str, _: Option<u64>, _: Option<u64>) -> Result<String, String> {
        panic!("unexpected read")
    }
    fn parse(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn report(&mut self, _: &str, _: bool) -> Result<String, String> {
        panic!("unexpected report")
    }
    fn explain(&mut self, _: &str) -> Result<String, String> {
        panic!("unexpected explain")
    }
    fn language(&mut self, _: &str, _: Option<&str>) -> Result<String, String> {
        panic!("unexpected language")
    }
    fn models(&mut self, _: &str, _: &Value) -> Result<String, String> {
        panic!("unexpected models")
    }
    fn knowledge(&mut self, _: Option<&str>, _: Option<&str>) -> Result<String, String> {
        panic!("unexpected knowledge")
    }
    fn compose(&mut self, _: &str, _: &[Value]) -> Result<String, String> {
        panic!("unexpected compose")
    }
    fn verify(
        &mut self,
        _: &str,
        (_, answers, _): (&str, &BTreeMap<String, String>, Option<&str>),
        _: (&[Resolution], &[Resolution]),
        presentation: &Value,
    ) -> Result<String, String> {
        assert_eq!(answers, &BTreeMap::new());
        self.0.lock().unwrap().push(presentation.clone());
        Ok("verified scope".into())
    }
    fn judged_now(&mut self, _: &str, _: &str, _: (&[Resolution], &[Resolution])) -> Vec<String> {
        vec![]
    }
    fn model_facts(&mut self, _: &str) -> Option<ModelFacts> {
        None
    }
}

fn output() -> Value {
    json!({"key":"output", "question":"Where should the summary go?", "role":"output_path",
        "options":[
            {"key":"md", "label":"Markdown", "recommended":true,
             "values":[{"role":"output_path", "value":"./news/digest.md"}]},
            {"key":"html", "label":"Web page", "recommended":false,
             "values":[{"role":"output_path", "value":"./news/digest.html"}]}]})
}

fn written() -> (Toolbox, Arc<Mutex<Vec<Value>>>) {
    let mut citations = Citations::default();
    citations.record(
        1,
        Some(("u1".into(), "a summary of technology news in a file".into())),
    );
    let tools = Toolbox::new(Conversation::default(), Arc::new(Mutex::new(citations)));
    let captured = Arc::new(Mutex::new(vec![]));
    let asker = Arc::new(Incarnation);
    tools.begin(
        Box::new(Capture(captured.clone())),
        Box::new(move |context| QuestionId::new(context.to_owned(), &asker)),
    );
    let written = tools.call(ToolCall::new(
        "candidate_write",
        json!({"source":"nika: news\n# writes ./news/digest.md\n"}),
    ));
    assert!(!written.is_error, "{written:?}");
    (tools, captured)
}

#[test]
fn a_proposal_transports_all_validated_choices_without_human_answers_or_consent() {
    let (tools, captured) = written();
    let reply =
        tools.call(ToolCall::new("propose", json!({"open":[output()]})).with_meta("proposal-call"));
    assert!(!reply.is_error, "{reply:?}");
    let open = tools.with(|conversation| {
        assert_eq!(conversation.bindings().len(), 0);
        assert_eq!(conversation.answers(), BTreeMap::new());
        assert_eq!(conversation.questions().len(), 0);
        conversation.open().to_vec()
    });
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].options.len(), 2);
    assert_eq!(open[0].options[1].label, "Web page");
    assert_eq!(
        *captured.lock().unwrap(),
        vec![json!({"kind":"proposal", "pending_choices":open, "selected":null})]
    );
    assert_eq!(tools.end().acts, None);
}

#[test]
fn an_invalid_proposal_never_reaches_verification_as_presentation() {
    let (tools, captured) = written();
    let mut choice = output();
    choice["options"][1]["recommended"] = json!(true);
    let reply = tools.call(ToolCall::new("propose", json!({"open":[choice]})));
    assert!(reply.is_error);
    assert!(
        reply.text.contains("recommends exactly one option"),
        "{reply:?}"
    );
    assert_eq!(*captured.lock().unwrap(), Vec::<Value>::new());
    assert_eq!(tools.with(|conversation| conversation.open().len()), 0);
    assert_eq!(tools.end().acts, None);
}
