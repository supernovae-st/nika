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
        }
    }

    /// The glyph: ✓ done · ↻ repairing · ● working (the renderer's busy
    /// row and the plain loop share it; the loader turns beside ● only).
    #[must_use]
    pub const fn glyph(&self) -> char {
        if self.done {
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
        "judge" | "judge_native" | "judge_semantic" | "judge_transform" => Phase::Checking,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
