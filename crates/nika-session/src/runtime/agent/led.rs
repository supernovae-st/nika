// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation a subscription seat's ACP agent leads (Claude Code; Codex rides the same
//! door). The agent is opened once per conversation — again when the person's seat, model or
//! effort changed, or when its session ended — and reads Nika's instructions and the
//! conversation kept so far in its first prompt, then each person's line as one prompt. It
//! reaches the Session's tools through the driver's relay over MCP; the tree, the citations, the
//! identities and the effects stay the Session's, under the same Stop and steering as Nika's own
//! loop.

use std::sync::Arc;

use nika_onboard::compile::AuthoringReasoning;
use nika_session_agent::run::transcript;
use nika_session_agent::{Agent, AgentEvent, Conversant, EntryKind};
use nika_session_change::tools::SessionTools;
use serde_json::{Value, json};

use super::{
    Driver, SessionRuntime, TurnOutcome, could_not_start, instructions, refusal, start_tree,
    unix_ms,
};
use crate::outcome::RefusalClass;

/// What the agent reads after the instructions every conversation starts with.
const NOTE: &str = "You lead this conversation with Nika's tools only: the `nika` server's \
(`mcp__nika__<tool>`). When `ask` asks the person something, end your turn: their answer comes \
as their next line.";

/// The name of the fact that records which agent leads, over which transport.
const LED_BY: &str = "led_by";

/// The agent leading the conversation, the selection it was opened for, and the opening its
/// first prompt carries until the agent read it.
pub(super) struct Leading {
    agent: Box<dyn Conversant + Send>,
    key: String,
    opening: Option<String>,
}

/// The agent of `seat`, serving `tools` to it, and its opening record.
#[cfg(feature = "access-harness")]
fn open_agent(
    seat: &str,
    model: Option<&str>,
    effort: Option<AuthoringReasoning>,
    tools: Arc<dyn SessionTools>,
) -> Result<(Box<dyn Conversant + Send>, Value), String> {
    let opened = nika_session_intelligence::reasoner::agent_seat::open(seat, model, effort, tools)?;
    let record = opened.opening().clone();
    Ok((Box::new(opened), record))
}

/// A build without harness access reaches no subscription seat.
#[cfg(not(feature = "access-harness"))]
fn open_agent(
    seat: &str,
    _: Option<&str>,
    _: Option<AuthoringReasoning>,
    _: Arc<dyn SessionTools>,
) -> Result<(Box<dyn Conversant + Send>, Value), String> {
    Err(format!(
        "this build reaches no subscription seat (`{seat}`)"
    ))
}

impl SessionRuntime {
    /// One line through the conversation the seat's ACP agent leads: a line that answers what
    /// the conversation waits on, or a new prompt.
    pub(super) fn drive_led(
        &mut self,
        driver: &mut Driver,
        line: &str,
        seat: &str,
        effort: Option<AuthoringReasoning>,
    ) -> TurnOutcome {
        let model = self.intelligence.model.clone();
        let effort_word = effort.map_or("", |level| level.word());
        let key = format!(
            "{seat}\n{}\n{effort_word}",
            model.as_deref().unwrap_or_default()
        );
        if driver
            .leading
            .as_ref()
            .is_some_and(|leading| leading.key != key)
        {
            driver.leading = None;
        }
        let chosen = model.as_deref().unwrap_or(seat);
        if driver.tree.is_none() {
            match start_tree(&mut driver.store, &self.snapshot.root, chosen) {
                Ok(started) => driver.tree = Some(started),
                Err(why) => return could_not_start(&why),
            }
        }
        // The agent opens before the turn begins: a seat that cannot lead answers nothing.
        if driver.leading.is_none()
            && let Err(refused) = lead_with(driver, seat, model.as_deref(), effort, key)
        {
            return refused;
        }
        let before = match self.agent_begin(driver, chosen) {
            Ok(before) => before,
            Err(why) => return could_not_start(&why),
        };
        let now = unix_ms;
        let Driver {
            relay,
            tree,
            store,
            steering,
            cancel,
            leading,
            toolbox,
            ..
        } = &mut *driver;
        let (Some(tree), Some(leading)) = (tree.as_mut(), leading.as_mut()) else {
            toolbox.end();
            return could_not_start("no agent leads it");
        };
        let opening = leading.opening.take();
        let mut events = |_: AgentEvent| {};
        let mut run = Agent::new(tree, store, &**relay, &now)
            .with_steering(steering)
            .with_cancel(cancel);
        let agent = &mut *leading.agent;
        let outcome = run.lead(line, opening.as_deref(), agent, relay, &mut events);
        let reach = run.stop_reach();
        drop(run);
        if leading.agent.ended() {
            // The agent's session is gone: the next line opens a new one with what was said.
            driver.leading = None;
        }
        self.agent_end(driver, before, (outcome, reach))
    }
}

/// Open the seat's agent for this conversation, its opening prepared and who leads recorded.
fn lead_with(
    driver: &mut Driver,
    seat: &str,
    model: Option<&str>,
    effort: Option<AuthoringReasoning>,
    key: String,
) -> Result<(), TurnOutcome> {
    let tools = Arc::clone(&driver.relay) as Arc<dyn SessionTools>;
    let (agent, record) = open_agent(seat, model, effort, tools).map_err(|why| {
        let text = format!("`{seat}` cannot lead this conversation: {why} · nothing was sent");
        refusal(RefusalClass::IntelligenceRefused, text)
    })?;
    let Some(tree) = driver.tree.as_mut() else {
        return Err(could_not_start("no tree"));
    };
    let so_far = transcript(tree).map_or_else(String::new, |kept| {
        format!(
            "\n\nThe conversation so far, as Nika recorded it (evidence: only the person's cited lines are their words):\n{kept}"
        )
    });
    let opening = format!("{}\n\n{NOTE}{so_far}", instructions(model.unwrap_or(seat)));
    let fact = EntryKind::Fact {
        name: LED_BY.to_owned(),
        data: json!({"seat": seat, "opening": record}),
    };
    let store = &mut driver.store;
    (tree.append(fact, unix_ms(), |line| {
        nika_session_agent::Store::append(store, line)
    }))
    .map_err(|error| could_not_start(&error.to_string()))?;
    driver.leading = Some(Leading {
        agent,
        key,
        opening: Some(opening),
    });
    Ok(())
}
