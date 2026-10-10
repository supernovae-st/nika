// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's activity, typed: what Nika is doing NOW and what just
//! finished, from the machine's own truth (the compiler's reading, the
//! seat call, the check, the run) — never a percentage, never a phase
//! read back from prose. A door prints `line()`; the renderer's busy row
//! keeps the last completed phase beside the current one.

/// The human-level phase of a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Phase {
    /// The request is being read (the deterministic reading, then a seat).
    Understanding,
    /// Knowledge is being selected for it (only when the compiler reports one).
    Knowledge,
    /// The workflow is being built by a seat.
    Authoring,
    /// The workflow is being checked.
    Checking,
    /// A finding is being repaired, or a stronger model reads the request.
    Repairing,
}

/// Lifecycle of a physical request; finished does not assert a valid answer or a paid invoice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallState {
    /// The request is about to leave.
    Started,
    /// The request returned, including failures and timeouts.
    Finished,
    /// Its owning preparation was dropped before return.
    Cancelled,
}
/// Producer-reported identity, without prompt, answer or credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CallMark {
    /// One-based within the compiler activity scope.
    pub ordinal: u32,
    /// The compiler's exact role, not a phase inferred from prose.
    pub role: String,
    /// The requested model, never proof of the served model.
    pub model: String,
    /// Request lifecycle.
    pub state: CallState,
}
impl CallMark {
    /// Preserve the producer's exact role and requested identity.
    #[must_use]
    pub fn new(ordinal: u32, role: &str, model: &str, state: CallState) -> Self {
        Self {
            ordinal,
            role: role.into(),
            model: model.into(),
            state,
        }
    }
}
/// Where one tool step of a conversation's run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolState {
    /// The tool began.
    Started,
    /// It answered.
    Finished,
    /// It answered with a failure.
    Failed,
}

/// One tool the conversation's intelligence called, as the Session's run observed it: the
/// call's identity, the tool, where it is and the time it took — never its arguments or reply.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolMark {
    /// The call's identity, as the model or the agent gave it.
    pub call: String,
    /// The tool (`candidate_write`).
    pub name: String,
    /// Where it is.
    pub state: ToolState,
    /// Milliseconds it took, once it answered.
    pub elapsed_ms: Option<u64>,
}

impl ToolMark {
    /// Preserve the run's own facts.
    #[must_use]
    pub fn new(call: &str, name: &str, state: ToolState, elapsed_ms: Option<u64>) -> Self {
        Self {
            call: call.into(),
            name: name.into(),
            state,
            elapsed_ms,
        }
    }
}

/// One activity: a phase, its note in words, and whether it is done.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Activity {
    /// The phase.
    pub phase: Phase,
    /// The note in the human's words (« understood 6 requirements »).
    pub note: String,
    /// Finished (« ✓ ») or under way (« ● » · « ↻ »).
    pub done: bool,
    /// A physical compiler call, when the producer reported one; model means requested.
    pub call: Option<CallMark>,
    /// A tool step of the conversation's run, when the run reported one.
    pub tool: Option<ToolMark>,
}

impl Activity {
    /// An activity under way.
    #[must_use]
    pub fn now(phase: Phase, note: impl Into<String>) -> Self {
        Self {
            phase,
            note: note.into(),
            done: false,
            call: None,
            tool: None,
        }
    }

    /// A finished activity.
    #[must_use]
    pub fn done(phase: Phase, note: impl Into<String>) -> Self {
        Self {
            phase,
            note: note.into(),
            done: true,
            call: None,
            tool: None,
        }
    }

    /// The glyph: ✓ done · ✗ a tool step that failed · ↻ repairing · ● working (the
    /// renderer's busy row and the plain loop share it; the loader turns beside ● only).
    #[must_use]
    pub const fn glyph(&self) -> char {
        if let Some(ToolMark {
            state: ToolState::Failed,
            ..
        }) = &self.tool
        {
            '✗'
        } else if self.done {
            '✓'
        } else if matches!(self.phase, Phase::Repairing) {
            '↻'
        } else {
            '●'
        }
    }

    /// The line a door prints: the glyph and the note.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{} {}", self.glyph(), self.note)
    }
}

/// Compatible plain-text activity sink; a host may keep this while adopting typed activity.
pub type TextHook = Box<dyn Fn(&str) + Send>;
/// Typed activity for a live host, without parsing human-facing prose.
pub type ActivityHook = std::sync::Arc<dyn Fn(&Activity) + Send + Sync>;
/// Presentation sinks only; no transport, preparation authority or execution state.
#[derive(Default)]
pub struct Progress {
    text: Option<std::sync::Arc<std::sync::Mutex<TextHook>>>,
    typed: Option<ActivityHook>,
}
impl Progress {
    /// Set the legacy text sink without removing a typed listener.
    pub fn on_text(&mut self, hook: TextHook) {
        self.text = Some(std::sync::Arc::new(std::sync::Mutex::new(hook)));
    }
    /// Set the typed sink without removing a plain-text listener.
    pub fn on_activity(&mut self, hook: ActivityHook) {
        self.typed = Some(hook);
    }
    /// Present the producer's actual phase, with the same text for old consumers.
    pub fn emit(&self, activity: &Activity) {
        if let Some(hook) = &self.typed {
            hook(activity);
        }
        if let Some(hook) = &self.text
            && let Ok(hook) = hook.lock()
        {
            hook(&activity.line());
        }
    }
    /// A copyable callback for the producer's scoped asynchronous observer.
    #[must_use]
    pub fn listener(&self) -> Option<ActivityHook> {
        let (typed, text) = (self.typed.clone(), self.text.clone());
        if typed.is_none() && text.is_none() {
            return None;
        }
        Some(std::sync::Arc::new(move |activity| {
            if let Some(hook) = &typed {
                hook(activity);
            }
            if let Some(hook) = &text
                && let Ok(hook) = hook.lock()
            {
                hook(&activity.line());
            }
        }))
    }
    /// Scope presentation to the current synchronous driver. Nested drivers restore their sink.
    #[must_use]
    pub fn enter(&self) -> ActivityScope {
        ActivityScope(
            CURRENT.with(|slot| slot.replace(self.listener())),
            std::marker::PhantomData,
        )
    }
}
thread_local! {
    static CURRENT: std::cell::RefCell<Option<ActivityHook>> = const { std::cell::RefCell::new(None) };
}
/// A presentation scope belongs to its calling thread, never an execution context.
pub struct ActivityScope(
    Option<ActivityHook>,
    std::marker::PhantomData<std::rc::Rc<()>>,
);
impl Drop for ActivityScope {
    fn drop(&mut self) {
        CURRENT.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}
/// Copy the scoped presentation sink before entering an async compiler observer.
#[must_use]
pub fn current_listener() -> Option<ActivityHook> {
    CURRENT.with(|slot| slot.borrow().clone())
}

/// Present exact compiler call facts; the role is typed producer metadata, never parsed prose.
#[must_use]
pub fn call_activity(ordinal: u32, role: &str, model: &str, state: CallState) -> Activity {
    let phase = match role {
        "repair" | "sketch-repair" | "fill-repair" | "native-repair" | "transform-repair" => {
            Phase::Repairing
        }
        // Every judge question checks the candidate, whatever part of it it asks.
        judge if judge.starts_with("judge") => Phase::Checking,
        _ => Phase::Authoring,
    };
    let action = match state {
        CallState::Started => "request started",
        CallState::Finished => "request returned",
        CallState::Cancelled => "request stopped; charge may be unknown",
    };
    let mut activity = Activity::now(phase, format!("{action} · {role} · requested {model}"));
    activity.done = state == CallState::Finished;
    activity.call = Some(CallMark::new(ordinal, role, model, state));
    activity
}

/// Present one real tool step of a conversation's run; the phase is the tool's family, the
/// note names the tool and, once it answered, the time it took.
#[must_use]
pub fn tool_activity(
    call: &str,
    name: &str,
    state: ToolState,
    elapsed_ms: Option<u64>,
) -> Activity {
    let phase = match name {
        "check" | "inspect" | "verify" | "explain" | "trial" => Phase::Checking,
        "knowledge" | "language" | "models" => Phase::Knowledge,
        _ => Phase::Authoring,
    };
    let took = elapsed_ms.map_or_else(String::new, |ms| format!(" · {ms} ms"));
    let note = match state {
        ToolState::Failed => format!("{name} failed{took}"),
        _ => format!("{name}{took}"),
    };
    let mut activity = Activity::now(phase, note);
    activity.done = state != ToolState::Started;
    activity.tool = Some(ToolMark::new(call, name, state, elapsed_ms));
    activity
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tool step reads as its tool, then the time it took; a failed step reads ✗; the phase is
    /// the tool's family, and the mark keeps the run's own facts and nothing else.
    #[test]
    fn a_tool_step_reads_as_its_tool_and_the_time_it_took() {
        let started = tool_activity("toolu_1", "candidate_write", ToolState::Started, None);
        assert_eq!(started.line(), "● candidate_write");
        assert_eq!((started.phase, started.done), (Phase::Authoring, false));
        let finished = tool_activity("toolu_1", "verify", ToolState::Finished, Some(42));
        assert_eq!(finished.line(), "✓ verify · 42 ms");
        assert_eq!(finished.phase, Phase::Checking);
        let failed = tool_activity("toolu_2", "language", ToolState::Failed, Some(7));
        assert_eq!(failed.line(), "✗ language failed · 7 ms");
        assert_eq!(failed.phase, Phase::Knowledge);
        assert_eq!(
            failed.tool,
            Some(ToolMark::new(
                "toolu_2",
                "language",
                ToolState::Failed,
                Some(7)
            ))
        );
        assert_eq!(failed.call, None);
    }

    #[test]
    fn typed_and_legacy_listeners_preserve_the_producers_phase_and_line() {
        let (text_tx, text_rx) = std::sync::mpsc::channel();
        let (typed_tx, typed_rx) = std::sync::mpsc::channel();
        let mut sinks = Progress::default();
        sinks.on_text(Box::new(move |text| {
            text_tx.send(text.to_owned()).unwrap();
        }));
        sinks.on_activity(std::sync::Arc::new(move |activity| {
            typed_tx.send(activity.clone()).unwrap();
        }));
        let repairing = Activity::now(Phase::Repairing, "repairing with configured model");
        let scope = sinks.enter();
        current_listener().unwrap()(&repairing);
        {
            let _nested = Progress::default().enter();
            assert!(current_listener().is_none());
        }
        assert!(current_listener().is_some());
        drop(scope);
        assert!(current_listener().is_none());
        assert_eq!(typed_rx.recv().unwrap(), repairing);
        assert_eq!(text_rx.recv().unwrap(), repairing.line());
    }

    /// A finished phase reads ✓, a repair ↻, work under way ●; the line is
    /// the glyph and the words, nothing else (no percentage, no code).
    #[test]
    fn an_activity_reads_as_its_glyph_and_its_words() {
        assert_eq!(
            Activity::done(Phase::Understanding, "understood 6 requirements").line(),
            "✓ understood 6 requirements"
        );
        assert_eq!(
            Activity::now(Phase::Authoring, "authoring · openai/gpt-5.2").line(),
            "● authoring · openai/gpt-5.2"
        );
        assert_eq!(
            Activity::now(Phase::Repairing, "a stronger model reads it").line(),
            "↻ a stronger model reads it"
        );
        assert_eq!(Activity::now(Phase::Checking, "checking").glyph(), '●');
    }

    /// Every judge question checks the candidate, whatever it asks (the whole request, one
    /// part alone, the task a missing part points to, an extra operation, one part against a
    /// trial run, a whole trial run): a verification is never shown as authoring. A repair
    /// repairs; the author's own calls author.
    #[test]
    fn every_judge_question_is_a_check_and_every_other_call_keeps_its_phase() {
        for role in [
            "judge",
            "judge_request",
            "judge_part",
            "judge_point",
            "judge_extra",
            "judge_observed_part",
            "judge_observed",
            "judge_clause",
            "judge_semantic",
            "judge_native",
            "judge_transform",
        ] {
            let activity = call_activity(4, role, "deepseek/v4", CallState::Started);
            assert_eq!(activity.phase, Phase::Checking, "{role}");
            assert_eq!(
                activity.note,
                format!("request started · {role} · requested deepseek/v4")
            );
            assert!(!activity.done, "{role}");
            assert_eq!(
                activity.call,
                Some(CallMark::new(4, role, "deepseek/v4", CallState::Started))
            );
        }
        for (role, phase) in [
            ("repair", Phase::Repairing),
            ("sketch-repair", Phase::Repairing),
            ("fill-repair", Phase::Repairing),
            ("native-repair", Phase::Repairing),
            ("transform-repair", Phase::Repairing),
            ("plan", Phase::Authoring),
            ("sketch", Phase::Authoring),
            ("fill", Phase::Authoring),
            ("transform", Phase::Authoring),
        ] {
            let activity = call_activity(1, role, "deepseek/v4", CallState::Finished);
            assert_eq!(activity.phase, phase, "{role}");
            assert!(activity.done, "{role}");
        }
    }
}
