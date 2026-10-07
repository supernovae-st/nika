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
//!
//! A returned call is not a successful one: it returned, maybe with a failure.
//! The words of a correction sent while the turn works live here too
//! ([`crate::activity_card::correction_queued`] · [`crate::activity_card::correction_unsent`]).
//! Pure: no clock and no terminal; the door passes the offsets it measured
//! since the turn began. No percentage, and no phase, model or usage the
//! Session did not report.

use std::time::Duration;

use crate::activity::{CallState, Phase};

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

    fn phase(&mut self, phase: Phase, note: &str, done: bool) -> bool {
        let mark = match (done, phase) {
            (true, _) => Mark::Done,
            (false, Phase::Repairing) => Mark::Repairing,
            (false, _) => Mark::Working,
        };
        if mark == Mark::Repairing {
            let seen = (self.rows.iter().position(|row| row.mark == Mark::Repairing))
                .map_or(0, |at| self.rows.remove(at).seen);
            self.rows.push(Row {
                mark,
                phase: Some(phase),
                text: note.to_owned(),
                seen: seen + 1,
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
        let mut lines = vec![self.heading(ascii)];
        lines.extend(self.rows.iter().map(|row| row_words(row, ascii)));
        if let Some(call) = &self.call {
            lines.push(call_words(call, ascii));
        }
        lines
    }

    fn heading(&self, ascii: bool) -> String {
        let sep = if ascii { " - " } else { " · " };
        let running = (self.call.as_ref()).filter(|call| call.state == CallState::Started);
        let mut head = match (self.ended, running) {
            (None, Some(call)) => word(call.phase).to_owned(),
            (None, None) => match self.rows.last() {
                Some(row) if row.mark == Mark::Repairing => "Repairing".to_owned(),
                Some(Row {
                    mark: Mark::Working,
                    phase: Some(phase),
                    ..
                }) => word(*phase).to_owned(),
                _ => "Working".to_owned(),
            },
            (Some((ending, took)), _) => {
                let what = match ending {
                    Ending::Completed => "Settled",
                    Ending::Stopped => "Stopped by you",
                    Ending::Failed => "Not completed",
                };
                took.map_or_else(
                    || what.to_owned(),
                    |took| format!("{what}{sep}took {}", took_words(took)),
                )
            }
        };
        if self.calls > 0 {
            // Only the workflow author's own calls (writing and model review) are
            // observed here; the heading says what it does not count.
            let plural = if self.calls == 1 { "" } else { "s" };
            head = format!("{head}{sep}{} author call{plural}", self.calls);
            if self.repairs > 0 {
                head = format!("{head}, {} for repair", self.repairs);
            }
            if self.stopped > 0 {
                head = format!("{head}, {} stopped", self.stopped);
            }
            head = format!("{head}{sep}{NOT_COUNTED}");
        }
        if self.omitted > 0 {
            head = format!("{head}{sep}{} earlier updates omitted", self.omitted);
        }
        head
    }
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

/// One row: the Session's glyph (`✓` done · `●` under way · `↻` repair, or
/// their ASCII twins), the phase's word and the Session's words. A step is
/// kept exactly as said.
fn row_words(row: &Row, ascii: bool) -> String {
    let sep = if ascii { " - " } else { " · " };
    let glyph = match (row.mark, ascii) {
        (Mark::Step, _) => return row.text.clone(),
        (Mark::Done, false) => "✓",
        (Mark::Working, false) => "●",
        (Mark::Repairing, false) => "↻",
        (Mark::Done, true) => "ok",
        (Mark::Working, true) => ">",
        (Mark::Repairing, true) => "r",
    };
    let mut text = row.phase.map_or_else(String::new, |p| word(p).to_owned());
    if !row.text.is_empty() {
        if !text.is_empty() {
            text.push_str(sep);
        }
        text.push_str(&row.text);
    }
    if row.seen > 1 {
        text = format!("{text}{sep}reported {} times", row.seen);
    }
    format!("{glyph} {text}")
}

/// The call row: its ordinal, role, requested model and where it stands. A
/// returned call wears no success mark: it may have returned a failure.
fn call_words(call: &Call, ascii: bool) -> String {
    let sep = if ascii { " - " } else { " · " };
    let (glyph, state) = match (call.state, call.took) {
        (CallState::Started, _) => (if ascii { ">" } else { "●" }, "running".to_owned()),
        (CallState::Finished, Some(took)) => (
            if ascii { "o" } else { "○" },
            format!("returned after {}", took_words(took)),
        ),
        (CallState::Finished, None) => (if ascii { "o" } else { "○" }, "returned".to_owned()),
        (CallState::Cancelled, _) => (
            if ascii { "x" } else { "✖" },
            "stopped, a charge may be unknown".to_owned(),
        ),
    };
    format!(
        "{glyph} call {}{sep}{}{sep}requested {}{sep}{state}",
        call.ordinal,
        role_words(&call.role),
        call.model
    )
}

/// What the heading's count leaves out, until those calls are observed.
const NOT_COUNTED: &str = "conversation routing and decision-service calls not counted";

/// A producer role in plain words for the card. The exact role stays in the
/// event and the receipts; a role this map does not know is shown as it came.
fn role_words(role: &str) -> &str {
    match role {
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
        | "recovery" => "repair the workflow",
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
    let sep = if ascii { " - " } else { " · " };
    let what = if replaced {
        "correction replaced (the earlier one stays in history)"
    } else {
        "correction queued"
    };
    format!(
        "{what} {}{sep}the preparation stops, then it is sent",
        quoted(text, ascii)
    )
}

/// The notice for a queued correction not sent when the turn ended: back in
/// the box, or (`kept_out`: a question takes only an answer typed after it)
/// kept whole in the notice itself.
#[must_use]
pub fn correction_unsent(text: &str, kept_out: bool, ascii: bool) -> String {
    let sep = if ascii { " - " } else { " · " };
    if kept_out {
        format!(
            "your correction was not sent{sep}the question above takes only an answer typed after it{sep}Up recalls it, whole:\n{text}"
        )
    } else {
        format!(
            "the answer above came before your correction {}{sep}it is in the box, not sent",
            quoted(text, ascii)
        )
    }
}

/// A measured duration in words: seconds, then minutes and seconds.
#[must_use]
pub fn took_words(took: Duration) -> String {
    match took.as_secs() {
        0 => "under 1 s".to_owned(),
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
