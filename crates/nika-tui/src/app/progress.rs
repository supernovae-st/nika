// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Session's typed activity, drained from the turn's queue on the
//! shell's thread: to the workspace card as a typed update, timed by the
//! shell's own clock since the turn began, and to the busy row as the
//! Session's own line. Every run observation still goes to the desk.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use nika_display::activity::{Activity, ToolMark, ToolState, tool_words};
use nika_display::activity_card::{Update, role_words};

use super::Shell;
use crate::composer::Composer;
use crate::lang::{Voice, fill};
use crate::model::{Conversation, UiState};
use crate::session::feed::Observed;

/// The card's update for one activity the shell saw `at` since the turn
/// began: a call or tool when the Session reported one, its phase otherwise.
/// A tool step reads as `said` ([`tool_said`]), never its name; its name ties
/// its attempts together, so a failure a later attempt finishes reads as
/// retried, never as a red failure (NIK-173).
fn update<'a>(activity: &'a Activity, at: Duration, said: Option<&'a str>) -> Update<'a> {
    match &activity.call {
        Some(call) => Update::call(
            call.ordinal,
            &call.role,
            &call.model,
            activity.phase,
            call.state,
            at,
        ),
        None => match &activity.tool {
            Some(tool) => Update::attempt(
                activity.phase,
                &tool.name,
                said.unwrap_or(&activity.note),
                tool.state,
            ),
            None => Update::phase(activity.phase, &activity.note, activity.done),
        },
    }
}

/// A tool step in the person's words, in `voice`: what the tool does, never its name (live
/// gauntlet 4: « ✓ Generating · `candidate_write` · 28 ms »), a failure said with why as the run
/// reported it, then the time it took (live gauntlet wave 8, 2026-10-19: « échec · 3 ms » with no
/// reason in seven reports).
fn tool_said(tool: &ToolMark, voice: Voice) -> String {
    let words = voice.say(tool_words(&tool.name));
    let said = if tool.state == ToolState::Failed {
        let failed = fill(voice.say("{tool} failed"), &[("{tool}", words)]);
        match tool.reply.as_deref().and_then(failure_reason) {
            Some(why) => format!("{failed} · {why}"),
            None => failed,
        }
    } else {
        words.to_owned()
    };
    let elapsed = tool
        .elapsed_ms
        .map_or_else(String::new, |ms| format!(" · {ms} ms"));
    format!("{said}{elapsed}")
}

/// Why a step failed, from its reply's excerpt: the `error` or `message` of a JSON object, else
/// its first line, on one line and at most 100 characters; `None` when it says nothing.
fn failure_reason(reply: &str) -> Option<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(reply).ok();
    let field = (parsed.as_ref())
        .and_then(|value| value.get("error").or_else(|| value.get("message")))
        .and_then(serde_json::Value::as_str);
    let text = field.unwrap_or_else(|| {
        reply
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
    });
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.is_empty() {
        return None;
    }
    let mut chars = words.chars();
    let head: String = chars.by_ref().take(100).collect();
    Some(if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    })
}

/// The busy row's words for `activity`: a tool step as the card says it, a call's producer in
/// plain words (`audit_interpretation` reads « check its reading of your request »), any
/// other activity as the Session said it.
fn busy_line(activity: &Activity, said: Option<&str>, voice: Voice) -> String {
    if let Some(said) = said {
        return format!("{} {said}", activity.glyph());
    }
    let line = activity.line();
    match &activity.call {
        Some(call) => {
            let role = format!(" · {} · ", call.role);
            let words = format!(" · {} · ", voice.say(role_words(&call.role)));
            line.replacen(&role, &words, 1)
        }
        None => line,
    }
}

impl<C: Conversation + 'static> Shell<C> {
    /// Change the state and the draft while a scrolled transcript keeps its
    /// reading position: the rows they add or remove are below it.
    pub(super) fn keep_reading<T>(
        &mut self,
        update: impl FnOnce(&mut UiState, &mut Composer) -> T,
    ) -> T {
        let (state, desk, composer) = (&mut self.state, &self.desk, &mut self.composer);
        crate::scroll::preserve_reading_and_draft(state, desk, composer, update)
    }

    /// Show `busy` in the busy row, whose rows are the live area's.
    pub(super) fn set_busy(&mut self, busy: String) {
        self.keep_reading(|state, _| state.busy = Some(busy));
    }

    /// Drain the turn's queue: each typed activity to the card and the busy
    /// row (a finished one beside the current one), every run observation to
    /// the desk. `true` when anything arrived.
    pub(super) fn drain(
        &mut self,
        queue: &Receiver<Observed>,
        started: Instant,
        (base, last_done): (&mut Option<String>, &mut Option<String>),
        pulse: &mut super::pulse::Pulse,
    ) -> bool {
        let mut runs = Vec::new();
        let mut any = false;
        for observed in queue.try_iter() {
            any = true;
            let Observed::Activity(activity) = observed else {
                if matches!(observed, Observed::Asked { .. }) {
                    pulse.run_asked(Instant::now());
                }
                runs.push(observed);
                continue;
            };
            pulse.observe(&activity);
            let at = started.elapsed();
            let voice = self.state.voice();
            let said = (activity.tool.as_ref()).map(|tool| tool_said(tool, voice));
            crate::scroll::preserve_reading(&mut self.state, &self.desk, &self.composer, |state| {
                state.observe(update(&activity, at, said.as_deref()));
            });
            let line = busy_line(&activity, said.as_deref(), voice);
            if activity.done {
                *last_done = Some(line);
            } else {
                *base = Some(line);
            }
        }
        if !runs.is_empty() {
            self.desk.observe(runs.into_iter());
        }
        any
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use nika_display::activity::Phase;

    /// A phase activity becomes a phase update with the Session's words; the
    /// shell's time rides only on a call.
    #[test]
    fn a_phase_activity_keeps_its_phase_and_words() {
        let activity = Activity::now(Phase::Checking, "checking the proposal");
        match update(&activity, Duration::from_secs(9), None) {
            Update::Phase { phase, note, done } => {
                assert_eq!(phase, Phase::Checking);
                assert_eq!(note, "checking the proposal");
                assert!(!done);
            }
            other => panic!("not a phase: {other:?}"),
        }
        let done = Activity::done(Phase::Understanding, "recorded 2 requirements");
        assert!(matches!(
            update(&done, Duration::ZERO, None),
            Update::Phase { done: true, .. }
        ));
    }

    #[test]
    fn failed_tools_reach_the_card_as_failed_even_though_they_are_done() {
        use nika_display::activity::tool_activity;
        use nika_display::activity_card::ActivityCard;

        for (state, row) in [
            (
                ToolState::Failed,
                "✗ Generating · reading a project file failed · 7 ms",
            ),
            (
                ToolState::Finished,
                "✓ Generating · reading a project file · 7 ms",
            ),
        ] {
            let activity = tool_activity("call-1", "read", state, Some(7));
            assert!(activity.done, "both terminal reports have finished");
            let said = (activity.tool.as_ref()).map(|tool| tool_said(tool, Voice::default()));
            assert!(matches!(
                update(&activity, Duration::ZERO, said.as_deref()),
                Update::Attempt { state: reported, tool: "read", .. } if reported == state
            ));
            let mut card = ActivityCard::new();
            card.observe(update(&activity, Duration::ZERO, said.as_deref()));
            assert_eq!(card.lines(false), ["Working", row]);
        }
    }

    /// A failed step says why, as the run reported its reply: a JSON reply's `error`, or its first
    /// line, cut to one short line; a failure with no reply stays as it was (live gauntlet wave 8:
    /// « échec · 3 ms » with no reason in seven reports).
    #[test]
    fn a_failed_tool_says_why_it_failed() {
        use crate::lang::Lang;
        use nika_display::activity::tool_activity;

        let failed = |reply: Option<&str>| {
            let mut activity = tool_activity("call-1", "propose", ToolState::Failed, Some(3));
            activity.tool = (activity.tool).map(|mark| mark.with_reply(reply.map(str::to_owned)));
            let Some(tool) = activity.tool else {
                panic!("a tool mark");
            };
            tool
        };
        let french = Voice {
            lang: Lang::Fr,
            ascii: false,
        };
        assert_eq!(
            tool_said(
                &failed(Some("no draft to propose yet\nwrite one first")),
                Voice::default()
            ),
            "proposing the workflow failed · no draft to propose yet · 3 ms"
        );
        assert_eq!(
            tool_said(&failed(Some(r#"{"error":"a question waits"}"#)), french),
            "proposition du workflow : échec · a question waits · 3 ms"
        );
        assert_eq!(
            tool_said(&failed(None), french),
            "proposition du workflow : échec · 3 ms"
        );
        let long = "x".repeat(140);
        let said = tool_said(&failed(Some(&long)), Voice::default());
        assert!(
            said.contains(&format!("{}… · 3 ms", "x".repeat(100))),
            "{said}"
        );
    }

    /// A tool step that failed and that a later attempt of the same tool finished keeps its row in
    /// the card, retried and muted, never the red failure (NIK-173: « pas d'échecs internes
    /// affichés en rouge »), in English and in French; a failure no later attempt finished stays
    /// red to the end. The busy row says each attempt as it comes.
    #[test]
    fn a_retried_tool_failure_reads_muted_and_a_standing_one_stays_red() {
        use crate::lang::Lang;
        use crate::model::{Committed, Kind};
        use nika_display::activity_card::{ActivityCard, Ending};
        use nika_display::theme::Role;

        let failed = tool_activity_for("read", ToolState::Failed, 7);
        let retried = tool_activity_for("read", ToolState::Finished, 3);
        let standing = tool_activity_for("propose", ToolState::Failed, 2);
        let rows = |voice: Voice| {
            let mut card = ActivityCard::new();
            for activity in [&failed, &retried, &standing] {
                let said = (activity.tool.as_ref()).map(|tool| tool_said(tool, voice));
                card.observe(update(activity, Duration::ZERO, said.as_deref()));
            }
            card.settle(Ending::Completed, None);
            card.lines_in(voice.ascii, &|en: &str| voice.say(en).to_owned())
        };
        let english = rows(Voice::default());
        assert_eq!(
            english[1..],
            [
                "↺ Generating · reading a project file failed · 7 ms · retried",
                "✓ Generating · reading a project file · 3 ms",
                "✗ Generating · proposing the workflow failed · 2 ms",
            ]
        );
        let french = rows(Voice {
            lang: Lang::Fr,
            ascii: false,
        });
        assert_eq!(
            french[1],
            "↺ Génération · lecture d'un fichier du projet : échec · 7 ms · réessayé"
        );
        assert!(french[3].starts_with("✗ Génération · "), "{french:?}");
        let plain = rows(Voice {
            lang: Lang::En,
            ascii: true,
        });
        assert!(plain[1].starts_with("r Generating - ") && plain[1].ends_with(" - retried"));
        // The kept story paints the retried row muted and the standing failure red.
        let block = Committed::new(Kind::Activity, english.join("\n"));
        let lines = crate::render::block_lines(&block, true, false);
        let tone = |at: usize| lines[at].spans[1].style;
        let style = |role| crate::visual::role::style(role, true);
        assert_eq!(tone(1), style(Role::Dim));
        assert_eq!(tone(2), style(Role::Good));
        assert_eq!(tone(3), style(Role::Bad));
        // The busy row says each attempt as it comes: the retry's success replaces the failure.
        let said = (retried.tool.as_ref()).map(|tool| tool_said(tool, Voice::default()));
        assert_eq!(
            busy_line(&retried, said.as_deref(), Voice::default()),
            "✓ reading a project file · 3 ms"
        );
    }

    /// A terminal tool report of `name` that took `ms`.
    fn tool_activity_for(name: &str, state: ToolState, ms: u64) -> Activity {
        nika_display::activity::tool_activity("call", name, state, Some(ms))
    }

    /// The card and the busy row say what a conversation's tool does and what a call is for,
    /// never the tool's or the role's name (live gauntlet 4: « ✓ Generating ·
    /// `candidate_write` · 28 ms », « call 2 · `audit_interpretation` · requested
    /// deepseek/deepseek-v4-pro »), in the person's language; a name no words are known for
    /// stays as it came.
    #[test]
    fn tools_and_calls_read_in_words_not_by_their_names() {
        use crate::lang::Lang;
        use nika_display::activity::{CallState, call_activity, tool_activity};
        use nika_display::activity_card::ActivityCard;

        let write = tool_activity("toolu_1", "candidate_write", ToolState::Finished, Some(28));
        let failed = tool_activity("toolu_2", "propose", ToolState::Failed, Some(2));
        let call = call_activity(2, "audit_interpretation", "deepseek/v4", CallState::Started);
        let unknown = tool_activity("toolu_3", "telescope", ToolState::Finished, None);
        let card = |voice: Voice| {
            let mut card = ActivityCard::new();
            let mut busy = Vec::new();
            for activity in [&write, &failed, &call, &unknown] {
                let said = (activity.tool.as_ref()).map(|tool| tool_said(tool, voice));
                card.observe(update(activity, Duration::ZERO, said.as_deref()));
                busy.push(busy_line(activity, said.as_deref(), voice));
            }
            (
                card.lines_in(false, &|en: &str| voice.say(en).to_owned()),
                busy,
            )
        };
        let (rows, busy) = card(Voice::default());
        let text = format!("{}\n{}", rows.join("\n"), busy.join("\n"));
        for words in [
            "✓ Generating · writing the draft · 28 ms",
            "✗ Generating · proposing the workflow failed · 2 ms",
            "● call 2 · check its reading of your request · requested deepseek/v4 · running",
            "● request started · check its reading of your request · requested deepseek/v4",
            "telescope",
        ] {
            assert!(text.contains(words), "{words}\n{text}");
        }
        for name in ["candidate_write", "propose ", "audit_interpretation"] {
            assert!(!text.contains(name), "{name}\n{text}");
        }
        let french = Voice {
            lang: Lang::Fr,
            ascii: false,
        };
        let (rows, _) = card(french);
        let text = rows.join("\n");
        for words in [
            "écriture du brouillon · 28 ms",
            "proposition du workflow : échec · 2 ms",
            "vérifier sa lecture de votre demande",
        ] {
            assert!(text.contains(words), "{words}\n{text}");
        }
    }
}
