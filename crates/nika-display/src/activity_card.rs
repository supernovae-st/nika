// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The card a door keeps of one turn's activity: what the Session reported,
//! folded into a few rows, then settled with the time the door measured.
//!
//! - The phase under way is ONE row, replaced by the next phase reported.
//! - Finished phases stay.
//! - Every repair report shares ONE row that counts them.
//! - The workflow author's calls (writing and model review) share ONE row:
//!   the latest call, its role in plain words, the model the policy requested
//!   (never a served identity) and where it stands. The heading counts them,
//!   repair calls and stopped ones, and says that conversation routing and
//!   decision-service calls are not counted here.
//! - Any other line (a run's story) is a step, kept as said.
//! - A tool step that failed keeps its row. Once a later attempt of the same tool finishes in
//!   the same phase, that row reads as recovered: `↺` (`r` in ASCII) and « retried », never the
//!   failure's `✗`, and nothing in the heading says a failure. A failure no later attempt
//!   finished keeps its `✗`, to the end.
//!
//! A returned call is not a successful one: it returned, maybe with a failure.
//! The words of a correction sent while the turn works live here too
//! ([`crate::activity_card::correction_queued`] · [`crate::activity_card::correction_unsent`]).
//! Pure: no clock and no terminal; the door passes the offsets it measured
//! since the turn began. No percentage, and no phase, model or usage the
//! Session did not report.
//!
//! The card's own words are English
//! ([`ActivityCard::lines`](crate::activity_card::ActivityCard::lines)); a door
//! that knows the person's language says them in it
//! ([`ActivityCard::lines_in`](crate::activity_card::ActivityCard::lines_in)):
//! each of the card's phrases, whole with its `{…}` values, goes through the
//! door's words, and the values (counts, times, models) and the Session's own
//! words are set in after, as they are.

use std::time::Duration;

use crate::activity::{CallState, Phase, ToolState};

/// The most rows one card keeps; older ones are counted, not shown.
pub const ROWS: usize = 12;

/// One report the card folds.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Update<'a> {
    /// A phase the Session reported, its words, and whether it finished.
    Phase {
        /// The phase.
        phase: Phase,
        /// The Session's words.
        note: &'a str,
        /// Finished (`true`) or under way.
        done: bool,
    },
    /// A tool report, whose terminal state distinguishes failure from success. Its words name
    /// the tool: a later report with the same words in the same phase is the same tool's.
    Tool {
        /// The phase the Session reported.
        phase: Phase,
        /// The Session’s words.
        note: &'a str,
        /// Where the tool stands.
        state: ToolState,
    },
    /// A tool report that names its tool apart from its words (an attempt's words change with
    /// its time or its failure): a later attempt of the same tool that finishes in the same
    /// phase recovers this one's failure.
    Attempt {
        /// The phase the Session reported.
        phase: Phase,
        /// The tool, as the run named it (`read`): it ties the attempts together, never shown.
        tool: &'a str,
        /// The words the row says.
        note: &'a str,
        /// Where the attempt stands.
        state: ToolState,
    },
    /// One physical call, as the producer reported it.
    Call {
        /// One-based within the producer's activity scope.
        ordinal: u32,
        /// The producer's exact role (`plan` · `fill` · `fill-repair` …).
        role: &'a str,
        /// The model the policy requested.
        model: &'a str,
        /// The phase the producer gave the call.
        phase: Phase,
        /// Where the call stands.
        state: CallState,
        /// When the door saw this report, since the turn began.
        at: Duration,
    },
    /// Any other line, kept as said (a run's story).
    Step(&'a str),
}

impl<'a> Update<'a> {
    /// A phase report.
    #[must_use]
    pub const fn phase(phase: Phase, note: &'a str, done: bool) -> Self {
        Self::Phase { phase, note, done }
    }

    /// A tool report with its producer’s terminal state intact.
    #[must_use]
    pub const fn tool(phase: Phase, note: &'a str, state: ToolState) -> Self {
        Self::Tool { phase, note, state }
    }

    /// An attempt of the tool `tool`, said in `note`, with its producer's terminal state intact.
    #[must_use]
    pub const fn attempt(phase: Phase, tool: &'a str, note: &'a str, state: ToolState) -> Self {
        Self::Attempt {
            phase,
            tool,
            note,
            state,
        }
    }

    /// A call report the door saw `at` since the turn began.
    #[must_use]
    pub const fn call(
        ordinal: u32,
        role: &'a str,
        model: &'a str,
        phase: Phase,
        state: CallState,
        at: Duration,
    ) -> Self {
        Self::Call {
            ordinal,
            role,
            model,
            phase,
            state,
            at,
        }
    }
}

/// How the turn the card watched ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ending {
    /// The turn answered.
    Completed,
    /// The Session stopped the preparation at the human's request.
    Stopped,
    /// The turn ended on a refusal: it did not complete.
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Done,
    Failed,
    /// A failure a later attempt of the same tool finished: retried, not failed.
    Recovered,
    Working,
    Repairing,
    Step,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    mark: Mark,
    phase: Option<Phase>,
    text: String,
    seen: usize,
    /// The tool a tool row reports, which ties its attempts together.
    tool: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Call {
    ordinal: u32,
    role: String,
    model: String,
    phase: Phase,
    state: CallState,
    started: Option<Duration>,
    took: Option<Duration>,
}

/// One turn's activity, folded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ActivityCard {
    rows: Vec<Row>,
    omitted: usize,
    call: Option<Call>,
    calls: u32,
    repairs: u32,
    stopped: u32,
    ended: Option<(Ending, Option<Duration>)>,
}

impl ActivityCard {
    /// An empty card, live.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Still receiving the turn's reports.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.ended.is_none()
    }

    /// Fold one report; `false` when nothing shown changes.
    pub fn observe(&mut self, update: Update<'_>) -> bool {
        match update {
            Update::Phase { phase, note, done } => self.phase(phase, note, done),
            Update::Tool { phase, note, state } => self.tool(phase, note, note, state),
            Update::Attempt {
                phase,
                tool,
                note,
                state,
            } => self.tool(phase, tool, note, state),
            Update::Call {
                ordinal,
                role,
                model,
                phase,
                state,
                at,
            } => {
                // Ordinals restart in every producer scope (a stronger seat's
                // compile is a new one), so an ordinal names a call only while
                // that call runs: each start is a new call, and a return is the
                // running call's only when the ordinal matches it.
                let running = (self.call.as_ref())
                    .filter(|call| call.state == CallState::Started && call.ordinal == ordinal);
                let started = match state {
                    CallState::Started => Some(at),
                    CallState::Finished | CallState::Cancelled => running.and_then(|c| c.started),
                };
                if state == CallState::Started || running.is_none() {
                    // A call first seen returned still counts; its time stays unknown.
                    self.calls += 1;
                    self.repairs += u32::from(phase == Phase::Repairing);
                }
                self.stopped += u32::from(state == CallState::Cancelled);
                let took = (state != CallState::Started)
                    .then(|| started.map(|from| at.saturating_sub(from)))
                    .flatten();
                self.call = Some(Call {
                    ordinal,
                    role: role.to_owned(),
                    model: model.to_owned(),
                    phase,
                    state,
                    started,
                    took,
                });
                true
            }
            Update::Step(line) => self.push(Mark::Step, None, line),
        }
    }

    /// One report of the tool `tool`, said in `note`: an attempt that finishes recovers every
    /// earlier failure of the same tool in the same phase.
    fn tool(&mut self, phase: Phase, tool: &str, note: &str, state: ToolState) -> bool {
        let mark = match state {
            ToolState::Started => Mark::Working,
            ToolState::Finished => Mark::Done,
            ToolState::Failed => Mark::Failed,
        };
        let mut recovered = false;
        if state == ToolState::Finished {
            let same = |row: &&mut Row| {
                row.mark == Mark::Failed
                    && row.phase == Some(phase)
                    && row.tool.as_deref() == Some(tool)
            };
            for row in self.rows.iter_mut().filter(same) {
                row.mark = Mark::Recovered;
                recovered = true;
            }
        }
        let shown = self.marked(phase, note, mark);
        if let Some(row) = self.rows.last_mut().filter(|row| row.mark == mark) {
            row.tool = Some(tool.to_owned());
        }
        shown || recovered
    }

    fn phase(&mut self, phase: Phase, note: &str, done: bool) -> bool {
        let mark = match (done, phase) {
            (true, _) => Mark::Done,
            (false, Phase::Repairing) => Mark::Repairing,
            (false, _) => Mark::Working,
        };
        self.marked(phase, note, mark)
    }

    fn marked(&mut self, phase: Phase, note: &str, mark: Mark) -> bool {
        if mark == Mark::Repairing {
            let seen = (self.rows.iter().position(|row| row.mark == Mark::Repairing))
                .map_or(0, |at| self.rows.remove(at).seen);
            self.rows.push(Row {
                mark,
                phase: Some(phase),
                text: note.to_owned(),
                seen: seen + 1,
                tool: None,
            });
            return true;
        }
        if (self.rows.last()).is_some_and(|row| row.mark == Mark::Working) {
            self.rows.pop();
        }
        self.push(mark, Some(phase), note)
    }

    fn push(&mut self, mark: Mark, phase: Option<Phase>, text: &str) -> bool {
        let last = self.rows.last();
        if last.is_some_and(|row| row.mark == mark && row.phase == phase && row.text == text) {
            return false;
        }
        self.rows.push(Row {
            mark,
            phase,
            text: text.to_owned(),
            seen: 1,
            tool: None,
        });
        if self.rows.len() > ROWS {
            self.rows.remove(0);
            self.omitted += 1;
        }
        true
    }

    /// The turn ended after `took` (the door's own clock), as `ending` says;
    /// `false` when nothing changes. A settled card keeps its time; a stop or
    /// a failure told after a plain end still replaces it, never the reverse.
    pub fn settle(&mut self, ending: Ending, took: Option<Duration>) -> bool {
        match self.ended {
            None => self.ended = Some((ending, took)),
            Some((Ending::Completed, kept)) if ending != Ending::Completed => {
                self.ended = Some((ending, kept.or(took)));
            }
            Some(_) => return false,
        }
        true
    }

    /// The heading and the rows, in the glyph column in use.
    #[must_use]
    pub fn lines(&self, ascii: bool) -> Vec<String> {
        self.lines_in(ascii, &|words: &str| words.to_owned())
    }

    /// [`Self::lines`] with the card's own phrases said through `say`: each
    /// English phrase, whole with its `{…}` values (`took {took}`, `{n} author
    /// calls`), comes back in the door's language, and the values and the
    /// Session's words are set in after it, unchanged. A phrase `say` keeps
    /// stays English.
    #[must_use]
    pub fn lines_in(&self, ascii: bool, say: &dyn Fn(&str) -> String) -> Vec<String> {
        let mut lines = vec![self.heading(ascii, say)];
        lines.extend(self.rows.iter().map(|row| row_words(row, ascii, say)));
        if let Some(call) = &self.call {
            lines.push(call_words(call, ascii, say));
        }
        lines
    }

    fn heading(&self, ascii: bool, say: &dyn Fn(&str) -> String) -> String {
        let sep = if ascii { " - " } else { " · " };
        let running = (self.call.as_ref()).filter(|call| call.state == CallState::Started);
        let mut head = match (self.ended, running) {
            (None, Some(call)) => say(word(call.phase)),
            (None, None) => match self.rows.last() {
                Some(row) if row.mark == Mark::Repairing => say("Repairing"),
                Some(Row {
                    mark: Mark::Working,
                    phase: Some(phase),
                    ..
                }) => say(word(*phase)),
                _ => say("Working"),
            },
            (Some((ending, took)), _) => {
                let what = say(match ending {
                    Ending::Completed => "Settled",
                    Ending::Stopped => "Stopped by you",
                    Ending::Failed => "Not completed",
                });
                took.map_or_else(
                    || what.clone(),
                    |took| {
                        let took = say("took {took}").replace("{took}", &took_in(took, say));
                        format!("{what}{sep}{took}")
                    },
                )
            }
        };
        if self.calls > 0 {
            // Only the workflow author's own calls (writing and model review) are
            // observed here; the heading says what it does not count.
            let calls = counted(self.calls, ("1 author call", "{n} author calls"), say);
            head = format!("{head}{sep}{calls}");
            if self.repairs > 0 {
                let repairs = counted(self.repairs, (", {n} for repair", ", {n} for repair"), say);
                head.push_str(&repairs);
            }
            if self.stopped > 0 {
                head.push_str(&counted(
                    self.stopped,
                    (", 1 stopped", ", {n} stopped"),
                    say,
                ));
            }
            head = format!("{head}{sep}{}", say(NOT_COUNTED));
        }
        if self.omitted > 0 {
            let omitted = counted(
                u32::try_from(self.omitted).unwrap_or(u32::MAX),
                ("1 earlier updates omitted", "{n} earlier updates omitted"),
                say,
            );
            head = format!("{head}{sep}{omitted}");
        }
        head
    }
}

/// `count` in its phrase said through `say`: the one form for 1, the other
/// form for any other count, its `{n}` set in after.
fn counted(count: u32, (one, many): (&str, &str), say: &dyn Fn(&str) -> String) -> String {
    let phrase = if count == 1 { one } else { many };
    say(phrase).replace("{n}", &count.to_string())
}

/// The display word of a phase.
const fn word(phase: Phase) -> &'static str {
    match phase {
        Phase::Understanding => "Understanding",
        Phase::Knowledge => "Choosing knowledge",
        Phase::Authoring => "Generating",
        Phase::Checking => "Checking",
        Phase::Repairing => "Repairing",
    }
}

/// One row: the Session's glyph (`✓` done · `✗` failed · `↺` failed then retried · `●` under
/// way · `↻` repair, or their ASCII twins), the phase's word and the Session's words, a
/// recovered failure ending with « retried ». A step is kept exactly as said.
fn row_words(row: &Row, ascii: bool, say: &dyn Fn(&str) -> String) -> String {
    let sep = if ascii { " - " } else { " · " };
    let glyph = match (row.mark, ascii) {
        (Mark::Step, _) => return row.text.clone(),
        (Mark::Done, false) => "✓",
        (Mark::Failed, false) => "✗",
        (Mark::Recovered, false) => "↺",
        (Mark::Working, false) => "●",
        (Mark::Repairing, false) => "↻",
        (Mark::Done, true) => "ok",
        (Mark::Failed, true) => "x",
        (Mark::Recovered | Mark::Repairing, true) => "r",
        (Mark::Working, true) => ">",
    };
    let mut text = row.phase.map_or_else(String::new, |p| say(word(p)));
    if !row.text.is_empty() {
        if !text.is_empty() {
            text.push_str(sep);
        }
        text.push_str(&row.text);
    }
    if row.seen > 1 {
        let seen = say("reported {n} times").replace("{n}", &row.seen.to_string());
        text = format!("{text}{sep}{seen}");
    }
    if row.mark == Mark::Recovered {
        text = format!("{text}{sep}{}", say("retried"));
    }
    format!("{glyph} {text}")
}

/// The call row: its ordinal, role, requested model and where it stands. A
/// returned call wears no success mark: it may have returned a failure. A call
/// the person stopped wears no failure mark either: `⊘` (the theme's cancelled
/// mark, `-` in ASCII), never a failure's `✖` or `x` (a stop is the person's
/// own act, not a failure).
fn call_words(call: &Call, ascii: bool, say: &dyn Fn(&str) -> String) -> String {
    let sep = if ascii { " - " } else { " · " };
    let (glyph, state) = match (call.state, call.took) {
        (CallState::Started, _) => (if ascii { ">" } else { "●" }, say("running")),
        (CallState::Finished, Some(took)) => (
            if ascii { "o" } else { "○" },
            say("returned after {took}").replace("{took}", &took_in(took, say)),
        ),
        (CallState::Finished, None) => (if ascii { "o" } else { "○" }, say("returned")),
        (CallState::Cancelled, _) => (
            if ascii { "-" } else { "⊘" },
            say("stopped, a charge may be unknown"),
        ),
    };
    let ordinal = say("call {n}").replace("{n}", &call.ordinal.to_string());
    // The model the policy requested is a value: set in after its words.
    let requested = say("requested {model}").replace("{model}", &call.model);
    format!(
        "{glyph} {ordinal}{sep}{}{sep}{requested}{sep}{state}",
        say(role_words(&call.role))
    )
}

/// What the heading's count leaves out, until those calls are observed.
const NOT_COUNTED: &str = "conversation routing and decision-service calls not counted";

/// A producer role in plain words for the card. The exact role stays in the
/// event and the receipts; a role this map does not know is shown as it came.
#[must_use]
pub fn role_words(role: &str) -> &str {
    match role {
        "conversation" => "answer you",
        "interpret_request" => "read your request",
        "audit_interpretation" => "check its reading of your request",
        "document" => "write the workflow",
        "plan" | "sketch" | "native" | "proposal" => "plan the workflow",
        "revision" | "edit" => "revise the workflow",
        "fill" => "write a step",
        "transform" => "write a data transformation",
        "judge_request" | "judge" | "judge_clause" | "judge_semantic" => "review your request",
        "judge_part" => "review a part of your request",
        "judge_point" => "find the step a missing part points to",
        "judge_observed_part" => "check one part against the trial run",
        "judge_observed" => "review a trial run",
        "judge_extra" | "judge_locate" | "judge_native" | "judge_transform" => {
            "review the workflow"
        }
        "fill-repair" => "repair a step",
        "transform-repair" | "transform_repair" => "repair a data transformation",
        "repair" | "sketch-repair" | "native-repair" | "revision-repair" | "source-recovery"
        | "recovery" | "document-repair" => "repair the workflow",
        other => other,
    }
}

/// The start of `text` on one line, quoted in the glyph column in use.
#[must_use]
pub fn quoted(text: &str, ascii: bool) -> String {
    const SHOWN: usize = 40;
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut shown: String = words.chars().take(SHOWN).collect();
    let (open, close, cut) = if ascii {
        ("\"", "\"", "...")
    } else {
        ("« ", " »", "…")
    };
    if words.chars().count() > SHOWN {
        shown.push_str(cut);
    }
    format!("{open}{shown}{close}")
}

/// The notice for a correction queued while a turn works (`replaced`: an
/// earlier one was, and stays in the input history).
#[must_use]
pub fn correction_queued(text: &str, replaced: bool, ascii: bool) -> String {
    correction_queued_in(text, replaced, ascii, &|words: &str| words.to_owned())
}

/// [`correction_queued`], its phrase said through `say` (whole, with its
/// `{sep}` and `{draft}`), the person's words set in last, as typed.
#[must_use]
pub fn correction_queued_in(
    text: &str,
    replaced: bool,
    ascii: bool,
    say: &dyn Fn(&str) -> String,
) -> String {
    let sep = if ascii { " - " } else { " · " };
    let phrase = if replaced {
        "correction replaced (the earlier one stays in history) {draft}{sep}the preparation stops, then it is sent"
    } else {
        "correction queued {draft}{sep}the preparation stops, then it is sent"
    };
    (say(phrase).replace("{sep}", sep)).replace("{draft}", &quoted(text, ascii))
}

/// The notice for a queued correction not sent when the turn ended: back in
/// the box, or (`kept_out`: a question takes only an answer typed after it)
/// kept whole in the notice itself.
#[must_use]
pub fn correction_unsent(text: &str, kept_out: bool, ascii: bool) -> String {
    correction_unsent_in(text, kept_out, ascii, &|words: &str| words.to_owned())
}

/// [`correction_unsent`], its phrase said through `say` (whole, with its
/// `{sep}` and `{draft}`), the person's words set in last, as typed.
#[must_use]
pub fn correction_unsent_in(
    text: &str,
    kept_out: bool,
    ascii: bool,
    say: &dyn Fn(&str) -> String,
) -> String {
    let sep = if ascii { " - " } else { " · " };
    if kept_out {
        let phrase = "your correction was not sent{sep}the question above takes only an answer typed after it{sep}Up recalls it, whole:\n{draft}";
        (say(phrase).replace("{sep}", sep)).replace("{draft}", text)
    } else {
        let phrase =
            "the answer above came before your correction {draft}{sep}it is in the box, not sent";
        (say(phrase).replace("{sep}", sep)).replace("{draft}", &quoted(text, ascii))
    }
}

/// A measured duration in words: seconds, then minutes and seconds.
#[must_use]
pub fn took_words(took: Duration) -> String {
    took_in(took, &|words: &str| words.to_owned())
}

/// [`took_words`], its one phrase (`under 1 s`) said through `say`.
fn took_in(took: Duration, say: &dyn Fn(&str) -> String) -> String {
    match took.as_secs() {
        0 => say("under 1 s"),
        secs @ 1..=59 => format!("{secs} s"),
        secs => format!("{} min {:02} s", secs / 60, secs % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    fn call(n: u32, role: &str, phase: Phase, state: CallState, at: u64) -> Update<'_> {
        Update::call(n, role, "deepseek/v4", phase, state, S(at))
    }

    /// A door's words say the correction notices whole, their separator and the
    /// quoted draft set in after; the person's words keep a brace they hold, and
    /// the plain forms say the English they said before.
    #[test]
    fn a_door_says_the_correction_notices_in_its_language() {
        let said = |words: &str| -> String {
            match words {
                "correction queued {draft}{sep}the preparation stops, then it is sent" => {
                    "correction en attente {draft}{sep}la préparation s'arrête, puis elle est envoyée"
                }
                other => other,
            }
            .to_owned()
        };
        // The person's words keep a brace they hold: they are set in last.
        assert_eq!(
            correction_queued_in("fix {sep} it", false, false, &said),
            "correction en attente « fix {sep} it » · la préparation s'arrête, puis elle est envoyée"
        );
        let plain = |words: &str| words.to_owned();
        for (text, replaced, ascii) in [("x", false, false), ("x", true, true)] {
            assert_eq!(
                correction_queued_in(text, replaced, ascii, &plain),
                correction_queued(text, replaced, ascii)
            );
            assert_eq!(
                correction_unsent_in(text, replaced, ascii, &plain),
                correction_unsent(text, replaced, ascii)
            );
        }
        assert_eq!(
            correction_unsent("short", true, false),
            "your correction was not sent · the question above takes only an answer typed after it · Up recalls it, whole:\nshort"
        );
    }

    /// Said through a door's words, every phrase of the card is the door's,
    /// and every value (a count, a time, the model) and the Session's own words
    /// stay as they are; a phrase the door keeps stays English.
    #[test]
    fn a_door_says_the_cards_own_words_in_its_language() {
        let said = |words: &str| -> String {
            match words {
                "Settled" => "Terminé",
                "took {took}" => "en {took}",
                "under 1 s" => "moins d'1 s",
                "1 author call" => "1 appel auteur",
                "{n} author calls" => "{n} appels auteur",
                "call {n}" => "appel {n}",
                "plan the workflow" => "planifier le workflow",
                "requested {model}" => "demandé {model}",
                "returned after {took}" => "revenu après {took}",
                "Understanding" => "Compréhension",
                "Generating" => "Génération",
                "retried" => "réessayé",
                other => other,
            }
            .to_owned()
        };
        let mut card = ActivityCard::new();
        card.observe(Update::phase(
            Phase::Understanding,
            "reading your line",
            true,
        ));
        card.observe(call(1, "plan", Phase::Authoring, CallState::Started, 0));
        card.observe(call(1, "plan", Phase::Authoring, CallState::Finished, 75));
        card.settle(Ending::Completed, Some(Duration::from_millis(400)));
        assert_eq!(
            card.lines_in(false, &said),
            [
                format!("Terminé · en moins d'1 s · 1 appel auteur · {NOT_COUNTED}"),
                "✓ Compréhension · reading your line".to_owned(),
                "○ appel 1 · planifier le workflow · demandé deepseek/v4 · revenu après 1 min 15 s"
                    .to_owned(),
            ]
        );
        assert_eq!(
            card.lines_in(true, &|words: &str| words.to_owned()),
            card.lines(true)
        );
        // A recovered failure says « retried » in the door's language; its words stay the
        // Session's.
        let mut retried = ActivityCard::new();
        retried.observe(Update::attempt(
            Phase::Authoring,
            "read",
            "lecture : échec",
            ToolState::Failed,
        ));
        retried.observe(Update::attempt(
            Phase::Authoring,
            "read",
            "lecture",
            ToolState::Finished,
        ));
        assert_eq!(
            retried.lines_in(false, &said)[1],
            "↺ Génération · lecture : échec · réessayé"
        );
    }

    /// The phase under way is one row the next phase replaces; a finished phase
    /// stays; repairs share one counted row and name the heading; the words are
    /// the Session's own and no percentage appears.
    #[test]
    fn phases_fold_into_few_rows_and_name_the_heading() {
        let mut card = ActivityCard::new();
        let reading = Update::phase(Phase::Understanding, "reading your line", false);
        assert!(card.observe(reading));
        assert_eq!(
            card.lines(false),
            ["Understanding", "● Understanding · reading your line"]
        );
        card.observe(Update::phase(Phase::Understanding, "recorded 6", true));
        card.observe(Update::phase(Phase::Authoring, "authoring · m", false));
        assert_eq!(
            card.lines(false),
            [
                "Generating",
                "✓ Understanding · recorded 6",
                "● Generating · authoring · m",
            ]
        );
        let repair = Update::phase(Phase::Repairing, "a stronger model reads it", false);
        card.observe(repair);
        card.observe(repair);
        let lines = card.lines(false);
        assert_eq!(lines[0], "Repairing");
        assert_eq!(
            lines.last().map(String::as_str),
            Some("↻ Repairing · a stronger model reads it · reported 2 times")
        );
        assert_eq!(lines.iter().filter(|l| l.starts_with('↻')).count(), 1);
        assert!(!lines.join("\n").contains('%'));
        assert!(card.observe(Update::Step("x")));
        assert!(
            !card.observe(Update::Step("x")),
            "repeated at once: kept once"
        );
    }

    /// A failed tool finished, but did not succeed: its typed state owns the mark, even when the
    /// note is identical, and later work may continue. Once a later attempt of the same tool
    /// finishes in the same phase, the failure keeps its row but reads as retried (`↺`, `r` in
    /// ASCII), never the failure's `✗` (NIK-173: no internal failure later work recovered
    /// reads as a red failure), and the heading says no failure.
    #[test]
    fn a_failure_a_later_attempt_finishes_keeps_its_row_and_reads_as_retried() {
        let mut card = ActivityCard::new();
        card.observe(Update::tool(Phase::Authoring, "read", ToolState::Started));
        card.observe(Update::tool(Phase::Authoring, "read", ToolState::Failed));
        assert_eq!(card.lines(false), ["Working", "✗ Generating · read"]);
        assert_eq!(card.lines(true), ["Working", "x Generating - read"]);
        assert!(card.is_live(), "one tool failure does not end the turn");
        card.observe(Update::tool(Phase::Authoring, "read", ToolState::Started));
        assert!(card.observe(Update::tool(Phase::Authoring, "read", ToolState::Finished)));
        assert_eq!(
            card.lines(false),
            [
                "Working",
                "↺ Generating · read · retried",
                "✓ Generating · read"
            ]
        );
        card.settle(Ending::Completed, None);
        assert_eq!(
            card.lines(true),
            [
                "Settled",
                "r Generating - read - retried",
                "ok Generating - read"
            ]
        );
        // Attempts named apart from their words: the time and the failure change the words.
        let mut named = ActivityCard::new();
        let failed = Update::attempt(
            Phase::Authoring,
            "read",
            "reading a file failed · 7 ms",
            ToolState::Failed,
        );
        let again = Update::attempt(
            Phase::Authoring,
            "read",
            "reading a file · 3 ms",
            ToolState::Finished,
        );
        named.observe(failed);
        named.observe(failed);
        named.observe(again);
        named.settle(Ending::Completed, Some(S(2)));
        assert_eq!(
            named.lines(false),
            [
                "Settled · took 2 s",
                "↺ Generating · reading a file failed · 7 ms · retried",
                "✓ Generating · reading a file · 3 ms",
            ]
        );
        assert!(!named.lines(false).join("\n").contains('✗'), "never red");
    }

    /// A failure no later attempt finished keeps its `✗` to the end, settled too: another
    /// tool finishing, the same tool finishing in another phase or failing again recovers
    /// nothing.
    #[test]
    fn a_failure_no_later_attempt_finishes_stays_a_failure_to_the_end() {
        let mut card = ActivityCard::new();
        card.observe(Update::attempt(
            Phase::Authoring,
            "read",
            "reading a file failed",
            ToolState::Failed,
        ));
        card.observe(Update::attempt(
            Phase::Authoring,
            "write",
            "writing the draft",
            ToolState::Finished,
        ));
        card.observe(Update::attempt(
            Phase::Checking,
            "read",
            "reading a file",
            ToolState::Finished,
        ));
        card.observe(Update::attempt(
            Phase::Authoring,
            "read",
            "reading a file failed again",
            ToolState::Failed,
        ));
        card.settle(Ending::Completed, Some(S(9)));
        assert_eq!(
            card.lines(false),
            [
                "Settled · took 9 s",
                "✗ Generating · reading a file failed",
                "✓ Generating · writing the draft",
                "✓ Checking · reading a file",
                "✗ Generating · reading a file failed again",
            ]
        );
        assert!(card.lines(true)[1].starts_with("x Generating - "));
        assert!(!card.lines(false).join("\n").contains("retried"));
    }

    /// Calls share one row with the requested model and the door's time; the
    /// heading names the running call's phase and counts calls, repair calls and
    /// stopped ones. A returned call is never marked a success; a call first seen
    /// returned still counts, its time unknown.
    #[test]
    fn calls_share_one_row_and_the_heading_counts_them() {
        let mut card = ActivityCard::new();
        card.observe(call(1, "plan", Phase::Authoring, CallState::Started, 2));
        assert_eq!(
            card.lines(false),
            [
                "Generating · 1 author call · conversation routing and decision-service calls not counted",
                "● call 1 · plan the workflow · requested deepseek/v4 · running"
            ]
        );
        card.observe(call(1, "plan", Phase::Authoring, CallState::Finished, 14));
        card.observe(call(
            2,
            "fill-repair",
            Phase::Repairing,
            CallState::Started,
            15,
        ));
        assert!(card.lines(false)[0].starts_with("Repairing · 2 author calls, 1 for repair · "));
        card.observe(call(
            2,
            "fill-repair",
            Phase::Repairing,
            CallState::Cancelled,
            80,
        ));
        card.observe(call(3, "judge", Phase::Checking, CallState::Finished, 90));
        let lines = card.lines(false);
        assert_eq!(
            lines[0],
            format!("Working · 3 author calls, 1 for repair, 1 stopped · {NOT_COUNTED}")
        );
        assert_eq!(lines.iter().filter(|l| l.contains(" call ")).count(), 1);
        assert_eq!(
            lines.last().map(String::as_str),
            Some("○ call 3 · review your request · requested deepseek/v4 · returned")
        );
        assert!(
            !lines.join("\n").contains('✓'),
            "a returned call is no success"
        );
        let mut timed = ActivityCard::new();
        timed.observe(call(7, "fill", Phase::Authoring, CallState::Started, 10));
        timed.observe(call(7, "fill", Phase::Authoring, CallState::Finished, 75));
        assert!(timed.lines(false)[1].ends_with("returned after 1 min 05 s"));
        let mut stopped = ActivityCard::new();
        stopped.observe(call(1, "fill", Phase::Authoring, CallState::Started, 1));
        stopped.observe(call(1, "fill", Phase::Authoring, CallState::Cancelled, 4));
        assert!(stopped.lines(false)[1].ends_with("stopped, a charge may be unknown"));
        // A stop is the person's own act, never a failure: no failure glyph in either column.
        assert!(stopped.lines(false)[1].starts_with("⊘ call 1 · "));
        assert_eq!(
            stopped.lines(true)[1],
            "- call 1 - write a step - requested deepseek/v4 - stopped, a charge may be unknown"
        );
    }

    /// The call row says what a call does in plain words; a role the map does not
    /// know is shown exactly as the producer named it, never guessed.
    #[test]
    fn the_call_row_names_its_role_in_plain_words() {
        for (role, words) in [
            ("plan", "plan the workflow"),
            ("sketch", "plan the workflow"),
            ("fill", "write a step"),
            ("judge_request", "review your request"),
            ("judge_part", "review a part of your request"),
            ("judge_point", "find the step a missing part points to"),
            ("judge_extra", "review the workflow"),
            (
                "judge_observed_part",
                "check one part against the trial run",
            ),
            ("judge_observed", "review a trial run"),
            ("judge_locate", "review the workflow"),
            ("sketch-repair", "repair the workflow"),
            ("source-recovery", "repair the workflow"),
            ("fill-repair", "repair a step"),
            ("transform", "write a data transformation"),
            ("a-new-role", "a-new-role"),
        ] {
            let mut card = ActivityCard::new();
            card.observe(call(1, role, Phase::Authoring, CallState::Started, 0));
            let row = card.lines(false).pop().unwrap_or_default();
            assert_eq!(
                row,
                format!("● call 1 · {words} · requested deepseek/v4 · running")
            );
        }
    }

    /// A turn can run several producer scopes (the seat's compile, then a stronger
    /// seat's), and each numbers its calls from 1 again: a new start is always a new
    /// call, so a first scope of one call and a second scope count two calls, not one.
    #[test]
    fn calls_of_a_restarted_scope_are_counted_again() {
        let mut card = ActivityCard::new();
        card.observe(call(1, "plan", Phase::Authoring, CallState::Started, 1));
        card.observe(call(1, "plan", Phase::Authoring, CallState::Finished, 9));
        card.observe(call(
            1,
            "fill-repair",
            Phase::Repairing,
            CallState::Started,
            10,
        ));
        assert_eq!(
            card.lines(false)[0],
            format!("Repairing · 2 author calls, 1 for repair · {NOT_COUNTED}")
        );
        card.observe(call(
            1,
            "fill-repair",
            Phase::Repairing,
            CallState::Finished,
            30,
        ));
        let lines = card.lines(false);
        assert_eq!(
            lines[0],
            format!("Working · 2 author calls, 1 for repair · {NOT_COUNTED}")
        );
        assert!(lines[1].ends_with("returned after 20 s"), "{}", lines[1]);
    }

    /// The end settles the heading with the door's time; a stop or a failure told
    /// later replaces a plain end and keeps its time; nothing replaces a stop.
    #[test]
    fn the_end_tells_completed_stopped_and_failed_apart() {
        let mut card = ActivityCard::new();
        card.observe(Update::phase(Phase::Authoring, "authoring", false));
        assert!(card.settle(Ending::Completed, Some(S(42))));
        assert!(!card.is_live());
        assert_eq!(card.lines(false)[0], "Settled · took 42 s");
        assert!(!card.settle(Ending::Completed, Some(S(1))));
        assert!(card.settle(Ending::Stopped, None));
        assert_eq!(card.lines(false)[0], "Stopped by you · took 42 s");
        assert!(
            !card.settle(Ending::Failed, None),
            "a stop is never replaced"
        );
        let mut failed = ActivityCard::new();
        failed.settle(Ending::Completed, Some(S(5)));
        assert!(failed.settle(Ending::Failed, None));
        assert_eq!(failed.lines(false), ["Not completed · took 5 s"]);
        let mut bare = ActivityCard::new();
        bare.settle(Ending::Completed, None);
        assert_eq!(bare.lines(false), ["Settled"]);
    }

    /// The ASCII column gives every glyph and separator its twin and keeps the
    /// Session's words; the card keeps at most its bound of rows.
    #[test]
    fn ascii_twins_and_the_bound() {
        let mut card = ActivityCard::new();
        card.observe(Update::phase(Phase::Checking, "checking", true));
        let again = Update::phase(Phase::Repairing, "again", false);
        card.observe(again);
        card.observe(again);
        card.observe(call(1, "fill", Phase::Authoring, CallState::Cancelled, 3));
        card.settle(Ending::Stopped, Some(S(3)));
        let lines = card.lines(true);
        assert!(lines.iter().all(|l| l.is_ascii()), "{lines:?}");
        assert_eq!(
            lines[0],
            format!("Stopped by you - took 3 s - 1 author call, 1 stopped - {NOT_COUNTED}")
        );
        let mut long = ActivityCard::new();
        for n in 0..(ROWS + 4) {
            long.observe(Update::Step(&format!("step {n}")));
        }
        let lines = long.lines(false);
        assert_eq!(lines.len(), ROWS + 1);
        assert_eq!(lines[0], "Working · 4 earlier updates omitted");
        assert_eq!(lines.last().map(String::as_str), Some("step 15"));
        long.observe(Update::Step("été·beta"));
        assert!(long.lines(true).last().is_some_and(|l| l == "été·beta"));
    }

    /// The notices quote the start of a correction on one line, in both glyph
    /// columns, and say where it went; kept out of the box, it is kept whole,
    /// every line of it, beyond the 40 characters a quote shows.
    #[test]
    fn correction_notices_quote_it_and_keep_it_whole_when_unsent() {
        let long = "use   the CSV\nexport instead of the JSON one and keep the header row";
        let queued = correction_queued(long, false, false);
        assert!(
            queued.starts_with("correction queued « use the CSV export instead of the JSON o… »"),
            "{queued}"
        );
        assert!(queued.ends_with("the preparation stops, then it is sent"));
        assert!(correction_queued("x", true, false).starts_with("correction replaced"));
        assert!(correction_queued(long, true, true).is_ascii());
        assert_eq!(
            correction_unsent("short", false, false),
            "the answer above came before your correction « short » · it is in the box, not sent"
        );
        let kept = correction_unsent(long, true, false);
        assert!(kept.ends_with(&format!(":\n{long}")), "{kept}");
        assert!(kept.contains("was not sent") && kept.contains("Up recalls it"));
        assert!(correction_unsent("short", true, true).is_ascii());
    }

    #[test]
    fn a_measured_duration_reads_in_seconds_then_minutes() {
        assert_eq!(took_words(Duration::from_millis(400)), "under 1 s");
        assert_eq!(took_words(S(42)), "42 s");
        assert_eq!(took_words(S(60)), "1 min 00 s");
        assert_eq!(took_words(S(3_725)), "62 min 05 s");
    }
}
