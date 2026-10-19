// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where a run the Live host adapter drives speaks while it runs: its story
//! goes to the busy row as it always did, and what the shell observes of it
//! (the request, then each typed frame of the child's stream) rides a bounded
//! queue the shell drains between frames. What cannot ride it is counted
//! beside it, never through it: a frame the full queue refused, a line of the
//! stream that was no frame this reader types. The queue grants nothing and
//! decides nothing: the run, its review and its gate keep their own doors.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, SyncSender};
use std::sync::{Arc, Mutex};

use nika_display::activity::Activity;
use nika_display::run_story::{RunFrame, RunSink};

use super::legs::Legs;
use crate::lang::Voice;
use crate::workspace::inspect::Inspected;

/// The most observations the queue holds; past it a frame is counted lost.
pub const QUEUE: usize = 4_096;

/// What the shell observes of a run while a turn holds the conversation.
#[derive(Debug)]
#[non_exhaustive]
pub enum Observed {
    /// A run was asked of the door, before its child exists: the workflow,
    /// whether it resumes a paused run, and the look of the exact bytes at
    /// that path when it was asked (`None` when they could not be read).
    Asked {
        /// The workflow, relative to the project's root.
        workflow: String,
        /// The request resumes a paused run (a new leg of it).
        resume: bool,
        /// The runner tells the run's frames, typed; when it does not, only
        /// its story reaches the transcript and the leg cannot follow it.
        typed: bool,
        /// The bytes the run was asked over, as the Session's look read them.
        look: Option<Box<Inspected>>,
        /// Where those bytes reach, as the Session's check declared it for this request
        /// (`None`: not audited here). Declared, never observed.
        world: Option<Box<nika_session::world::World>>,
    },
    /// One typed frame of the run's stream.
    Frame(RunFrame),
    /// One typed activity the Session reported while the turn works: the
    /// shell's activity card and busy row read it; no run leg does.
    Activity(Box<Activity>),
    /// The run was refused before its first frame: its reason, as the stream
    /// said it. No frame is fabricated; the leg asked settles as refused.
    Refused(String),
}

/// What could not reach the shell during one turn, counted beside the queue.
#[derive(Debug, Default)]
pub struct Gap {
    dropped: AtomicUsize,
    unread: AtomicUsize,
    /// What the stream first said of a line the run's own trace could not keep
    /// either (a step's output over the bound both share).
    why: std::sync::Mutex<Option<&'static str>>,
}

impl Gap {
    /// Observations the full (or closed) queue refused.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Lines of the stream that were no frame this reader types.
    #[must_use]
    pub fn unread(&self) -> usize {
        self.unread.load(Ordering::Relaxed)
    }

    /// What the stream first said of a line the run's own trace could not keep
    /// either, when it said it: why the run's journal can end there.
    pub(crate) fn why(&self) -> Option<&'static str> {
        self.why.lock().ok().and_then(|why| *why)
    }
}

/// The shell's end of one turn's queue, lent to the conversation.
#[derive(Clone, Debug)]
pub struct Seen {
    tx: SyncSender<Observed>,
    gap: Arc<Gap>,
}

impl Seen {
    /// The queue `tx` and the counters `gap` the shell reads after draining.
    #[must_use]
    pub fn new(tx: SyncSender<Observed>, gap: Arc<Gap>) -> Self {
        Self { tx, gap }
    }

    /// Queue `observed`, or count it lost; never wait for the shell.
    fn tell(&self, observed: Observed) {
        if self.tx.try_send(observed).is_err() {
            self.gap.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Tell the shell one typed activity of the turn (a phase, an author
    /// call), as a host reports what its Session does: the activity card and
    /// the live line read it. Queued like a run's frame, counted lost when
    /// the queue is full; it grants nothing.
    pub fn activity(&self, activity: Activity) {
        self.tell(Observed::Activity(Box::new(activity)));
    }
}

/// The sink a run is told to: the busy row hears its story, the shell's queue
/// its request and its frames when a shell lent one.
#[derive(Clone, Debug)]
pub struct Feed {
    busy: Sender<String>,
    seen: Option<Seen>,
    legs: Option<Arc<Mutex<Legs>>>,
    /// The voice the run's story says its own phrases in.
    voice: Voice,
}

impl Feed {
    /// A feed speaking to `busy`, and to `seen` when the shell lent it.
    #[must_use]
    pub fn new(busy: Sender<String>, seen: Option<Seen>) -> Self {
        Self {
            busy,
            seen,
            legs: None,
            voice: Voice::default(),
        }
    }

    /// The same feed, the run's story said in `voice` ([`RunSink::say`]).
    pub(crate) const fn speaking(mut self, voice: Voice) -> Self {
        self.voice = voice;
        self
    }

    /// The same feed, also recording each leg in the host's own ledger.
    pub(crate) fn with_legs(mut self, legs: Arc<Mutex<Legs>>) -> Self {
        self.legs = Some(legs);
        self
    }

    /// Tell the shell a run of `workflow` was asked, over the bytes `look` reaching `world`.
    pub(crate) fn asked(
        &self,
        workflow: String,
        resume: bool,
        typed: bool,
        (look, world): (Option<Inspected>, Option<nika_session::world::World>),
    ) {
        if let Some(mut legs) = self.legs.as_ref().and_then(|l| l.lock().ok()) {
            legs.asked();
        }
        if let Some(seen) = &self.seen {
            seen.tell(Observed::Asked {
                workflow,
                resume,
                typed,
                look: look.map(Box::new),
                world: world.map(Box::new),
            });
        }
    }

    /// The busy row's sender: what a story-only runner is told.
    pub(crate) fn busy(&self) -> &Sender<String> {
        &self.busy
    }

    /// One typed activity of the Session: to the shell's queue when it lent
    /// one, typed; otherwise its line to the busy row, as before.
    pub(crate) fn activity(&self, activity: &Activity) {
        match &self.seen {
            Some(seen) => seen.tell(Observed::Activity(Box::new(activity.clone()))),
            None => {
                let _ = self.busy.send(activity.line());
            }
        }
    }
}

impl RunSink for Feed {
    fn said(&self, line: String) {
        let _ = self.busy.send(line);
    }

    fn say(&self, en: &str) -> String {
        self.voice.say(en).to_owned()
    }

    fn refused(&self, why: &str) {
        if let Some(seen) = &self.seen {
            seen.tell(Observed::Refused(why.to_owned()));
        }
    }

    fn frame(&self, frame: RunFrame) {
        if let Some(mut legs) = self.legs.as_ref().and_then(|l| l.lock().ok()) {
            legs.frame(&frame);
        }
        if let Some(seen) = &self.seen {
            seen.tell(Observed::Frame(frame));
        }
    }

    fn unread(&self, why: &'static str) {
        if let Some(seen) = &self.seen {
            seen.gap.unread.fetch_add(1, Ordering::Relaxed);
            // Only a line its trace could not keep either explains a journal lost there.
            if why.contains("trace")
                && let Ok(mut said) = seen.gap.why.lock()
            {
                said.get_or_insert(why);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod feed_tests {
    use super::*;

    const FRAME: &str = r#"{"correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"t"}],"id":{"uuid":"01a0ef11-03a7-74fb-bba0-bfe19b901333"},"kind":"task_scheduled","run":null,"timestamp":1}"#;

    /// The busy row hears the story; the queue the frames, until it is
    /// full, and each frame it refuses is counted; a line that is no frame
    /// is counted unread; nothing waits for the shell.
    #[test]
    fn a_full_queue_counts_its_losses_beside_it() {
        let (busy, said) = std::sync::mpsc::channel();
        let (tx, rx) = std::sync::mpsc::sync_channel(2);
        let gap = Arc::new(Gap::default());
        let feed = Feed::new(busy, Some(Seen::new(tx, Arc::clone(&gap))));
        let mut story = nika_display::run_story::RunStory::default();
        for _ in 0..5 {
            story.tell(FRAME, &feed);
        }
        story.tell("noise", &feed);
        feed.said("a line".to_owned());
        assert_eq!(rx.try_iter().count(), 2, "the queue held its bound");
        assert_eq!((gap.dropped(), gap.unread()), (3, 1));
        assert_eq!(said.try_iter().collect::<Vec<_>>(), ["a line"]);
        drop(rx);
        story.tell(FRAME, &feed);
        assert_eq!(gap.dropped(), 4, "a closed queue loses, never blocks");
    }

    /// A line the stream could not carry is counted unread; what it said of a
    /// line the run's trace could not keep either is kept, the first one, and
    /// nothing else is: a line that is no frame says nothing of the journal.
    #[test]
    fn a_line_the_trace_could_not_keep_keeps_its_reason() {
        let (busy, _said) = std::sync::mpsc::channel();
        let (tx, _rx) = std::sync::mpsc::sync_channel(2);
        let gap = Arc::new(Gap::default());
        let feed = Feed::new(busy, Some(Seen::new(tx, Arc::clone(&gap))));
        let mut story = nika_display::run_story::RunStory::default();
        story.tell("noise", &feed);
        assert_eq!((gap.unread(), gap.why()), (1, None));
        let oversize = "a step's output over 1 MiB is not shown, and the run's trace cannot keep \
                        it either (1 MiB per line): the run goes on, its trace stops there";
        feed.unread(oversize);
        feed.unread("another line the trace could not keep");
        assert_eq!((gap.unread(), gap.why()), (3, Some(oversize)));
    }

    /// The busy row hears the run's story in the voice of the turn: its own
    /// phrases French, the workflow as it is.
    #[test]
    fn the_story_reaches_the_busy_row_in_the_turns_voice() {
        let (busy, said) = std::sync::mpsc::channel();
        let french = Voice {
            lang: crate::lang::Lang::Fr,
            ascii: false,
        };
        let feed = Feed::new(busy, None).speaking(french);
        let mut story = nika_display::run_story::RunStory::default();
        story.tell(
            r#"{"kind":"workflow_started","fields":[{"key":"workflow","value":"w"}]}"#,
            &feed,
        );
        assert_eq!(said.try_iter().collect::<Vec<_>>(), ["en cours · w"]);
    }

    /// A run refused before its first frame reaches the shell's queue as its refusal, by
    /// its reason; the busy row still hears the story's line.
    #[test]
    fn a_refusal_before_the_start_reaches_the_queue_by_its_reason() {
        let (busy, said) = std::sync::mpsc::channel();
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        let gap = Arc::new(Gap::default());
        let feed = Feed::new(busy, Some(Seen::new(tx, Arc::clone(&gap))));
        let mut story = nika_display::run_story::RunStory::default();
        story.tell(
            r#"{"error":{"message":"NIKA-1709 · floor over the ceiling"}}"#,
            &feed,
        );
        let heard: Vec<String> = (rx.try_iter())
            .filter_map(|observed| match observed {
                Observed::Refused(why) => Some(why),
                _ => None,
            })
            .collect();
        assert_eq!(heard, ["NIKA-1709 · floor over the ceiling"]);
        assert_eq!(
            said.try_iter().collect::<Vec<_>>(),
            ["✖ refused · NIKA-1709 · floor over the ceiling"]
        );
    }

    /// Without a shell's queue the feed is the busy row alone.
    #[test]
    fn a_feed_without_a_queue_is_the_busy_row() {
        let (busy, said) = std::sync::mpsc::channel();
        let feed = Feed::new(busy, None);
        let mut story = nika_display::run_story::RunStory::default();
        story.tell(
            r#"{"kind":"workflow_started","fields":[{"key":"workflow","value":"w"}]}"#,
            &feed,
        );
        feed.asked("w.nika".to_owned(), false, true, (None, None));
        assert_eq!(said.try_iter().collect::<Vec<_>>(), ["running · w"]);
    }
}
