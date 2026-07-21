// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The production [`EventSink`] lanes — `--json` (NDJSON verbatim), the
//! live TTY fold (`RunView` repaint per event), and the run journal
//! ([`TraceFileSink`] · `.nika/traces/` · teed beside either via [`Tee`]).
//!
//! All are consumers of the SAME stream (the fold law · spec §3): the
//! sink shape decides the surface, never the runtime. The sink contract
//! is INFALLIBLE (a write error never changes the run's verdict — it is
//! buffered and surfaced at the end).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use nika_event::Event;
use nika_runtime::EventSink;
use nika_types::id::EventId;
use nika_types::timestamp::Timestamp;
use uuid::Uuid;

use crate::display::render::{stream_header, stream_settled_line, stream_summary};
use crate::display::state::str_field;
use crate::{RunView, Theme, frame, frame_with_outputs, verdict_frame};

// The chain primitive + genesis tag live in the forensics crate — the
// sink WRITES the same chain the walk verifies (one constant, one hash).
use nika_dap::chain::CHAIN_GENESIS;
use nika_event::source_id::sha256_hex;

/// Where run journals land, relative to the run's CWD (the workspace
/// root by convention — the editor extension watches exactly this
/// directory, and spec §3.3 prints it in the final frame).
pub(super) const TRACE_DIR: &str = ".nika/traces";

/// How the live fold reaches the terminal (spec §3.5 reduced surfaces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderMode {
    /// Rich TTY: in-place repaint per event (cursor control · the default).
    Live,
    /// Plain: silent fold, ONE final storyboard frame (no animation · no
    /// cursor escapes) — `--no-progress` and the piped/CI default.
    Plain,
    /// Quiet: silent fold, the COMPACT verdict card only (errors always) —
    /// `--quiet`.
    Quiet,
}

/// Writes one NDJSON line per event to the wrapped writer (the `--json`
/// lane · "NDJSON events verbatim · CI/agents" · spec §3). Never
/// coloured. Flushes per event so a tailing agent sees liveness.
pub struct JsonSink<W: Write> {
    writer: W,
    /// The first write error, buffered (the sink contract is infallible
    /// w.r.t. the run · the caller checks this after the run).
    error: Option<std::io::Error>,
}

impl<W: Write> JsonSink<W> {
    /// Wrap a writer (typically `io::stdout().lock()`).
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            error: None,
        }
    }

    /// The buffered write error, if delivery ever failed.
    pub fn into_error(self) -> Option<std::io::Error> {
        self.error
    }
}

impl<W: Write> EventSink for JsonSink<W> {
    fn emit(&mut self, event: Event) {
        if self.error.is_some() {
            return; // already broken · stop touching a dead pipe
        }
        // serde_json::to_writer never fails on a valid Event (only the
        // writer can) — fold any error into the buffer.
        let result = serde_json::to_writer(&mut self.writer, &event)
            .map_err(std::io::Error::from)
            .and_then(|()| self.writer.write_all(b"\n"))
            .and_then(|()| self.writer.flush());
        if let Err(e) = result {
            self.error = Some(e);
        }
    }
}

/// The run-journal state: which disk lane, if any, this sink drives.
enum Lane {
    /// `--no-trace-file` / `NIKA_NO_TRACE_FILE` / a non-workspace run
    /// (`examples run` stages a temp file) — emit is a no-op by design,
    /// so the caller keeps ONE code path whether journaling or not.
    Disabled,
    /// Enabled but not yet on disk — the file is NAMED from the first
    /// event's identity (its `UUIDv7` id carries the run's mint time),
    /// so nothing can open before the stream starts.
    Pending,
    /// Open and appending one NDJSON line per event.
    Open(BufWriter<File>),
}

/// The run journal — the same EVENT stream as [`JsonSink`], each line
/// `<dir>/<ISO-compact>-<short-id>.ndjson` (spec §3.3 final frame ·
/// `.nika/traces/` in production). The flight recorder (`nika trace
/// show|replay`), `--resume`, and the editor extension's runs view all
/// read this file back — it exists so EVERY run leaves a journal, not
/// only the ones piped through `--json`.
///
/// Three constraints shape it:
/// - **Lazy open** — file creation waits for the FIRST emit: the name
///   needs the first event's `UUIDv7` id + wall timestamp, and a run
///   that never starts (audit refusal · composition failure) must not
///   litter an empty file.
/// - **Infallible** (the [`EventSink`] contract) — an fs error (read-only
///   checkout · disk full) is buffered, never panics, never changes the
///   run's verdict or its primary bytes; the caller surfaces it AFTER
///   the run as a stderr note.
/// - **Rider, never a surface** — it tees BESIDE the chosen primary lane
///   (via [`Tee`]); with the sink disabled or broken, the primary lane's
///   output stays byte-identical.
pub(super) struct TraceFileSink {
    /// The journal directory (created on first emit · `TRACE_DIR` in
    /// production · a temp dir under test). Meaningless when disabled.
    dir: PathBuf,
    lane: Lane,
    /// The opened file's path (`None` until the lazy open · stays `None`
    /// when disabled or when the open itself failed).
    path: Option<PathBuf>,
    /// The first fs error, buffered (the sink contract is infallible
    /// w.r.t. the run · the caller surfaces this after the run).
    error: Option<std::io::Error>,
    /// The tamper-evidence chain: sha256 hex of the LAST line's exact
    /// bytes (genesis: sha256 of [`CHAIN_GENESIS`]) — injected into the
    /// NEXT line as its `chain` field. After the run, this is the HEAD.
    chain: String,
    /// Lines actually written (the anchor trio's count).
    written: usize,
}

impl TraceFileSink {
    /// An enabled journal rooted at `dir` (lazy — no fs effect here).
    pub(super) fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            lane: Lane::Pending,
            path: None,
            error: None,
            chain: sha256_hex(CHAIN_GENESIS),
            written: 0,
        }
    }

    /// A permanently-silent journal (`emit` = no-op · zero fs effects) —
    /// the opt-out shape that keeps the caller's wiring branch-free.
    #[must_use]
    pub(super) fn disabled() -> Self {
        Self {
            dir: PathBuf::new(),
            lane: Lane::Disabled,
            path: None,
            error: None,
            chain: sha256_hex(CHAIN_GENESIS),
            written: 0,
        }
    }

    /// The journal file's path, once the lazy open happened.
    #[must_use]
    pub(super) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The buffered fs error, if journaling ever failed.
    #[must_use]
    pub(super) fn into_error(self) -> Option<std::io::Error> {
        self.error
    }

    /// The chain HEAD — sha256 of the last written line's exact bytes.
    /// Printing it (CI logs · scrollback) is the free external anchor
    /// that upgrades tamper-EVIDENT toward attributable.
    #[must_use]
    pub(super) fn chain_head(&self) -> &str {
        &self.chain
    }

    /// Lines written — the anchor trio's middle term (the C2SP
    /// checkpoint shape: origin · size · root).
    #[must_use]
    pub(super) fn chain_len(&self) -> usize {
        self.written
    }

    /// Durability point — called ONCE before the anchor is advertised:
    /// `flush()` reaches the page cache only; a power loss after the
    /// anchor printed but before writeback would leave a shorter-but-
    /// clean prefix on disk, and the anchor mismatch would FORGE a
    /// tamper alarm against the operator's own hardware. One fsync per
    /// run buys an honest anchor.
    pub(super) fn finalize(&mut self) {
        if self.error.is_some() {
            return;
        }
        if let Lane::Open(writer) = &mut self.lane {
            let result = writer.flush().and_then(|()| writer.get_ref().sync_data());
            if let Err(e) = result {
                self.error = Some(e);
                self.lane = Lane::Disabled;
            }
        }
    }

    /// Create the directory + the journal file, named from the first
    /// event's identity.
    ///
    /// The runtime does not stamp `event.run` today, so the first
    /// event's own id IS the run-unique mint (`SystemStamper` `UUIDv7` ·
    /// ADR-033); `run` is preferred whenever a future runtime sets it.
    fn open(&mut self, first: &Event) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let id = first.run.as_ref().map_or(first.id.uuid, |r| r.uuid);
        let path = self.dir.join(trace_file_name(first.timestamp, id));
        // `create_new` refuses to clobber: two runs in the same SECOND can
        // share the 4-hex short id (16 random bits — a real risk under a
        // parallel CI matrix), and truncating a sibling run's journal would
        // be silent data loss. The fallback name carries the full 32-hex id
        // (uuid-unique), so it cannot collide again.
        let create = |p: &Path| {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            // The journal can carry sensitive task output — owner-only
            // (0600) from creation on unix; elsewhere the platform default.
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            opts.open(p)
        };
        let (file, path) = match create(&path) {
            Ok(f) => (f, path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let full = self.dir.join(format!(
                    "{}-{}.ndjson",
                    compact_ts(first.timestamp),
                    id.as_simple()
                ));
                (create(&full)?, full)
            }
            Err(e) => return Err(e),
        };
        self.lane = Lane::Open(BufWriter::new(file));
        self.path = Some(path);
        Ok(())
    }
}

impl EventSink for TraceFileSink {
    fn emit(&mut self, event: Event) {
        if self.error.is_some() {
            return; // already broken · stop touching a dead lane
        }
        if matches!(self.lane, Lane::Pending)
            && let Err(e) = self.open(&event)
        {
            // The failed open is FINAL (the error gate above short-circuits
            // every later emit) — no retry storm against a read-only disk.
            self.error = Some(e);
            return;
        }
        let Lane::Open(writer) = &mut self.lane else {
            return; // Disabled — a deliberate no-op
        };
        // One JSON document per line + flush per event (the watcher tails
        // for liveness). The journal line = the event object PLUS a
        // `chain` field — sha256 hex of the PREVIOUS line's exact bytes
        // (genesis: CHAIN_GENESIS). Hashing written bytes, never a
        // re-serialization, is what makes `trace verify` total: no
        // canonical-JSON contract to drift. Event consumers ignore the
        // extra field (tolerant serde — pinned in verify tests).
        let chain = self.chain.clone();
        let result = serde_json::to_value(&event)
            .map_err(std::io::Error::from)
            .and_then(|mut value| {
                let Some(obj) = value.as_object_mut() else {
                    // A non-object event is a WRITER defect — fail loudly
                    // here; a silent chainless line would read as an
                    // Unchained/Broken verdict (an attack) downstream.
                    return Err(std::io::Error::other(
                        "event did not serialize to a JSON object",
                    ));
                };
                obj.insert("chain".to_owned(), serde_json::Value::String(chain));
                let line = serde_json::to_string(&value).map_err(std::io::Error::from)?;
                writer.write_all(line.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
                Ok(sha256_hex(line.as_bytes()))
            });
        // Chain state commits only AFTER the bytes landed: a failed
        // write must never advance the head (a later "N events · chain
        // X" note would otherwise describe bytes no file holds).
        match result {
            Ok(next) => {
                self.chain = next;
                self.written += 1;
            }
            Err(e) => {
                self.error = Some(e);
                // L4: retire the lane NOW — BufWriter's Drop re-flushes
                // ignoring errors, and a recovered fs would complete the
                // torn line AFTER the error was reported (the file's
                // post-error content must not depend on the environment).
                self.lane = Lane::Disabled;
            }
        }
    }
}

/// The journal's timestamp component: RFC 3339 compacted for a path —
/// second precision (drop `.fff`) and `:` → `-` (colons are illegal on
/// Windows paths and hostile in macOS Finder). `2026-06-11T14:02:33.123Z`
/// becomes `2026-06-11T14-02-33Z`.
fn compact_ts(ts: Timestamp) -> String {
    let iso = ts.to_string();
    // Display always renders `…SS.mmmZ`; the guard keeps this total if
    // that ever changes (worst case: the full string, still path-safe
    // after the replace below except the dot — acceptable, never wrong).
    let seconds = iso.split('.').next().unwrap_or(&iso);
    format!("{}Z", seconds.replace(':', "-"))
}

/// The journal file name (spec §3.3 final frame): `<ISO-compact>-<short>
/// .ndjson`, e.g. `2026-06-11T14-02-33Z-a3f2.ndjson`. The short id is
/// the LAST 4 hex chars of the `UUIDv7` — the tail is the random part
/// (the v7 PREFIX encodes coarse mint-time and is near-constant across
/// weeks, so it would disambiguate nothing the timestamp doesn't).
fn trace_file_name(ts: Timestamp, id: Uuid) -> String {
    let simple = id.as_simple().to_string();
    let short = &simple[simple.len().saturating_sub(4)..];
    format!("{}-{short}.ndjson", compact_ts(ts))
}

/// Fans one event stream into two sinks — `emit` delivers to BOTH (one
/// clone per event · events are small values). This is how the run
/// journal rides beside every primary surface without forking `drive`:
/// the primary lane `a` keeps its exact bytes; the rider `b` (the trace
/// file) observes the same stream. Both sides are [`EventSink`]s, so
/// infallibility composes — neither can veto the run or the other lane.
pub(super) struct Tee<A: EventSink, B: EventSink> {
    a: A,
    b: B,
}

impl<A: EventSink, B: EventSink> Tee<A, B> {
    /// Pair the primary surface `a` with the rider `b`.
    pub(super) fn new(a: A, b: B) -> Self {
        Self { a, b }
    }

    /// Take both lanes back after the run (the caller reads the fold's
    /// verdict from `a` and surfaces buffered errors from each side).
    pub(super) fn into_parts(self) -> (A, B) {
        (self.a, self.b)
    }
}

impl<A: EventSink, B: EventSink> EventSink for Tee<A, B> {
    fn emit(&mut self, event: Event) {
        // Primary first (the user-facing surface leads) · rider second.
        self.a.emit(event.clone());
        self.b.emit(event);
    }
}

/// Folds each event into a [`RunView`] and repaints the frame (the live
/// TTY lane · spec §3). Repaints are event-driven PLUS the spinner
/// ticks: a timer rider (`spawn_spinner`) advances `tick` while a
/// task is running, so a long `infer`/`agent` wait breathes instead of
/// freezing between settles (braille frames only ever render on the
/// animated unicode Live surface — every sober register is tick-blind
/// by the glyph law).
pub struct FoldSink<W: Write> {
    writer: W,
    theme: Theme,
    view: RunView,
    /// The surface this sink paints (spec §3.5) — `Live` repaints in place
    /// (TTY only · cursor control), `Plain`/`Quiet` fold silently and the
    /// caller prints ONE final frame (no escape noise in a captured log).
    mode: RenderMode,
    /// Paint the shape tails (`→ {…} · 312B · 90 tok`) on completed rows.
    /// The interactive-TTY comprehension surface — the run verb enables it
    /// for `Live` only (pipes · CI · `--no-outputs` stay byte-unchanged).
    outputs: bool,
    /// Lines painted by the previous frame (to clear before the redraw).
    last_lines: usize,
    /// The spinner phase — advanced by the timer rider, read by every
    /// repaint (events repaint at the CURRENT phase, never reset it).
    tick: usize,
    /// The LIVING MAP's topology — the checked projection the run verb
    /// injects before driving. Every Live repaint redraws the wire art
    /// with each node painted by its CURRENT state (the same geometry
    /// `graph --format ascii` speaks); runs whose shape the wire law
    /// refuses fall back to the frame's own wave-column line.
    map: Option<(crate::verbs::graph::GraphDoc, Vec<Vec<usize>>)>,
    error: Option<std::io::Error>,
}

/// The shared handle the spinner rider and the event stream both hold —
/// the heartbeat's `Arc<Mutex<…>>` precedent applied to the fold. On the
/// run's current-thread executor the two never contend for real (the
/// rider only runs at await points); the lock is the proof, not a hot
/// path.
pub(super) type SharedFold<W> = std::sync::Arc<std::sync::Mutex<FoldSink<W>>>;

/// The [`EventSink`] face of a shared fold — lock, apply, repaint. A
/// poisoned lock (another holder panicked) goes silent: the render is
/// best-effort by contract, the verdict never rides here.
pub(super) struct FoldHandle<W: Write>(pub(super) SharedFold<W>);

impl<W: Write> EventSink for FoldHandle<W> {
    fn emit(&mut self, event: Event) {
        if let Ok(mut fold) = self.0.lock() {
            fold.emit(event);
        }
    }
}

/// The spinner rider — ticks the shared fold ~10×/s while the run sits
/// at an await point (exactly the provider/subprocess wait a frozen
/// frame misreads as a hang). The caller aborts it the moment the run
/// settles; the executor's drop reaps it regardless.
pub(super) fn spawn_spinner<W: Write + Send + 'static>(
    fold: SharedFold<W>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tick.tick().await; // the immediate first tick is not a frame
        loop {
            tick.tick().await;
            match fold.lock() {
                Ok(mut fold) => fold.spin(),
                Err(_) => return, // poisoned — best-effort silence
            }
        }
    })
}

impl<W: Write> FoldSink<W> {
    /// Wrap a writer + the resolved theme + the render mode. `Live` does the
    /// in-place repaint (TTY); `Plain`/`Quiet` fold silently for the caller's
    /// final-frame print.
    pub fn new(writer: W, theme: Theme, mode: RenderMode) -> Self {
        Self {
            writer,
            theme,
            view: RunView::new(),
            mode,
            outputs: false,
            last_lines: 0,
            tick: 0,
            map: None,
            error: None,
        }
    }

    /// One spinner beat — advance the phase and repaint, but ONLY while
    /// something is actually running on the animated Live surface (a
    /// settled screen must not churn; sober registers never tick).
    pub(super) fn spin(&mut self) {
        if self.mode != RenderMode::Live
            || !self.theme.animate
            || self.error.is_some()
            || !self
                .view
                .rows()
                .iter()
                .any(|r| r.state == crate::TaskState::Running)
        {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        if let Err(e) = self.repaint() {
            self.error = Some(e);
        }
    }

    /// Enable the shape tails on completed rows (the interactive-TTY
    /// comprehension surface). Off by default — every existing register
    /// keeps its exact bytes until a caller opts in.
    pub fn show_outputs(&mut self, on: bool) {
        self.outputs = on;
    }

    /// The folded view (the caller renders the FINAL frame + the failure
    /// card from it after the run · the verdict lives here).
    pub fn view(&self) -> &RunView {
        &self.view
    }

    /// Inject the static wave plan (task ids per wave · from the check
    /// report) — feeds the ∥ lane markers + the DAG-shape glyph. Side
    /// information only: the fold itself never changes.
    pub fn set_plan(&mut self, waves: Vec<Vec<String>>) {
        self.view.set_plan(waves);
    }

    /// Print the final frame once (the `Plain`/`Quiet` lanes · the caller
    /// calls this after the run · a no-op buffered error stays buffered).
    /// `Quiet` paints the compact verdict card; `Plain` CLOSES its
    /// narration (#321 — the rows spoke at settle · the meter + the
    /// failure card end the story, never a repeated storyboard).
    pub fn print_final(&mut self) {
        if self.error.is_some() {
            return;
        }
        let lines = match self.mode {
            RenderMode::Quiet => verdict_frame(&self.view, &self.theme),
            RenderMode::Plain => stream_summary(&self.view, &self.theme),
            _ if self.outputs => frame_with_outputs(&self.view, &self.theme, 0),
            _ => frame(&self.view, &self.theme, 0),
        };
        for line in &lines {
            if let Err(e) = writeln!(self.writer, "{line}") {
                self.error = Some(e);
                return;
            }
        }
        if let Err(e) = self.writer.flush() {
            self.error = Some(e);
        }
    }

    /// The buffered write error, if any.
    pub fn into_error(self) -> Option<std::io::Error> {
        self.error
    }

    /// Take the buffered write error out of a SHARED fold (the spinner
    /// arc keeps ownership behind the lock — `into_error`'s by-value
    /// twin for that seam).
    pub(super) fn take_error(&mut self) -> Option<std::io::Error> {
        self.error.take()
    }

    /// Inject the run's checked topology — the living map draws from the
    /// NEXT repaint on. Live surface only; sober modes never store it.
    pub fn set_map(&mut self, doc: crate::verbs::graph::GraphDoc, waves: Vec<Vec<usize>>) {
        if self.mode == RenderMode::Live && self.theme.accents {
            self.view.external_map = true;
            self.map = Some((doc, waves));
        }
    }

    /// The wire art with every node painted by its current state —
    /// `None` when no topology was injected or the wire law refuses the
    /// shape (the frame's own wave-column line covers that run).
    fn living_map(&self) -> Option<String> {
        let (doc, waves) = self.map.as_ref()?;
        let states: std::collections::BTreeMap<&str, &crate::display::state::TaskRow> = self
            .view
            .rows()
            .iter()
            .map(|r| (r.id.as_str(), r))
            .collect();
        let theme = self.theme;
        let tick = self.tick;
        // Zero-alloc live probe: the rows ARE the state — a 10 Hz repaint
        // asks, it never copies.
        let rows = self.view.rows();
        let running = |id: &str| {
            rows.iter()
                .any(|r| r.id == id && matches!(r.state, crate::display::state::TaskState::Running))
        };
        let graph = crate::wires::wire_graph(doc, waves);
        crate::wires::render_with(
            &graph,
            theme,
            &move |id, verb| {
                use crate::display::state::TaskState;
                let row = states.get(id).copied();
                match row.map(|r| &r.state) {
                    Some(TaskState::Running) => (
                        theme.verb_spin(Some(verb), tick),
                        theme.paint(crate::display::theme::Role::Strong, id),
                    ),
                    Some(TaskState::Ok) => (
                        theme.verb_glyph(verb),
                        theme.paint(crate::display::theme::Role::Good, id),
                    ),
                    Some(TaskState::Failed) => (
                        theme.verb_glyph(verb),
                        theme.paint(crate::display::theme::Role::Bad, id),
                    ),
                    Some(TaskState::Skipped | TaskState::Cancelled) => (
                        theme.paint(crate::display::theme::Role::Dim, "⊘ "),
                        theme.paint(crate::display::theme::Role::Dim, id),
                    ),
                    _ => (
                        theme.paint(crate::display::theme::Role::Dim, "· "),
                        theme.paint(crate::display::theme::Role::Dim, id),
                    ),
                }
            },
            Some((&running, tick)),
        )
    }

    fn repaint(&mut self) -> std::io::Result<()> {
        // Move the cursor up over the previous frame and clear from there
        // down (ANSI · the spinner family). TTY-only — `emit` gates this.
        // A REDRAW rides inside a DEC-2026 synchronized-output frame
        // (`CSI ?2026h … ?2026l`): kitty/WezTerm/Ghostty/Rio hold the
        // screen until the frame closes (no clear-then-paint flicker);
        // every other terminal ignores the pair (zero-cost no-op). The
        // FIRST paint appends without clearing — nothing can tear, so it
        // stays escape-free (the first-frame law the tests pin). A write
        // error mid-frame leaves the pair unclosed once, harmlessly: the
        // sink goes silent (buffered error) and DEC-2026 terminals
        // time the frame out on their own.
        let redraw = self.last_lines > 0;
        if redraw {
            write!(self.writer, "\x1b[?2026h")?;
            write!(self.writer, "\x1b[{}A", self.last_lines)?;
            write!(self.writer, "\x1b[0J")?;
        }
        let mut lines = if self.outputs {
            frame_with_outputs(&self.view, &self.theme, self.tick)
        } else {
            frame(&self.view, &self.theme, self.tick)
        };
        if let Some(art) = self.living_map() {
            // The map leads the frame — blank-separated, repainted with
            // the storyboard so the running node's motion turns in place.
            let mut led: Vec<String> = art.lines().map(str::to_owned).collect();
            led.push(String::new());
            led.extend(lines);
            lines = led;
        }
        for line in &lines {
            writeln!(self.writer, "{line}")?;
        }
        if redraw {
            write!(self.writer, "\x1b[?2026l")?;
        }
        self.last_lines = lines.len();
        self.writer.flush()
    }

    /// The plain narration (#321): the header the moment the workflow
    /// starts, one storyboard line the moment each task settles — the
    /// SAME content the final frame would show, streamed (zero cursor
    /// control · a piped consumer sees progress when it happens, and a
    /// local-model run never reads as a hang). Non-narrating events
    /// write nothing (no flush churn on dispatch/checkpoint frames).
    fn narrate(&mut self, event: &Event) -> std::io::Result<()> {
        use nika_event::EventKind;
        let lines: Vec<String> = match event.kind {
            EventKind::WorkflowStarted => stream_header(&self.view, &self.theme),
            EventKind::TaskCompleted
            | EventKind::TaskFailed
            | EventKind::TaskSkipped
            | EventKind::TaskCancelled
            | EventKind::TaskCacheHit => str_field(event, "task")
                .and_then(|task| stream_settled_line(&self.view, task, &self.theme, self.outputs))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        if lines.is_empty() {
            return Ok(());
        }
        for line in &lines {
            writeln!(self.writer, "{line}")?;
        }
        self.writer.flush()
    }
}

impl<W: Write> EventSink for FoldSink<W> {
    fn emit(&mut self, event: Event) {
        if self.error.is_some() {
            return;
        }
        self.view.apply(&event);
        // `Live` repaints in place (TTY cursor control) · `Plain`
        // narrates at settle (#321 · zero escapes) · `Quiet` folds
        // silently for the compact final card.
        let painted = match self.mode {
            RenderMode::Live => self.repaint(),
            RenderMode::Plain => self.narrate(&event),
            _ => Ok(()),
        };
        if let Err(e) = painted {
            self.error = Some(e);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn anchor_advertises_the_full_64_hex_head() {
        // #333 · byte-exact parity with `trace verify`: the taught loop
        // (compare the two printed heads) must close with `==`, so the
        // anchor carries the WHOLE sha256 hex — never a truncated form.
        let head = "a".repeat(64);
        let line = anchor_line(
            std::path::Path::new(".nika/traces/x.ndjson"),
            7,
            &head,
            false,
        );
        assert_eq!(
            line,
            format!("trace: .nika/traces/x.ndjson · 7 events · chain {head}")
        );
        assert!(
            line.ends_with(&head),
            "the head is printed whole, byte-comparable against trace verify's"
        );
    }

    #[test]
    fn json_sink_writes_one_ndjson_line_per_event() {
        let events = demo::success();
        let n = events.len();
        let mut buf = Vec::new();
        {
            let mut sink = JsonSink::new(&mut buf);
            for ev in &events {
                sink.emit(ev.clone());
            }
            assert!(sink.into_error().is_none(), "the vec writer never fails");
        }
        let text = String::from_utf8(buf).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), n, "one line per event");
        // Every line is a standalone JSON document (the NDJSON contract).
        for line in lines {
            let _: serde_json::Value =
                serde_json::from_str(line).expect("each line is one JSON Event");
        }
    }

    #[test]
    fn json_sink_is_never_coloured() {
        let mut buf = Vec::new();
        let mut sink = JsonSink::new(&mut buf);
        for ev in demo::failure() {
            sink.emit(ev);
        }
        let text = String::from_utf8(buf).expect("utf8");
        assert!(!text.contains('\x1b'), "--json carries zero ANSI escapes");
    }

    /// #321 — the plain lane NARRATES: the header at workflow start, one
    /// storyboard line at each settle, the meter as the close — top to
    /// bottom ONCE (no repeated frame) · zero cursor escapes (the
    /// CI-capture contract) · a piped consumer sees progress live.
    #[test]
    fn fold_sink_plain_narrates_at_settle_then_closes_with_the_meter() {
        let theme = Theme::new(false, true, false);
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Plain);
            // The run verb injects the plan BEFORE driving — the streamed
            // header counts from it (rows don't exist at header time).
            sink.set_plan(vec![
                vec!["fetch_top".to_owned()],
                vec!["extract_ai".to_owned()],
                vec!["summarize".to_owned()],
                vec!["write_md".to_owned(), "notify_slack".to_owned()],
            ]);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert_eq!(sink.view().verdict, Some(true), "success folded");
            sink.print_final();
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        // The header opens the story, plan-counted, permits greeting next.
        assert!(
            lines[0].contains("veille-news · 5 tasks"),
            "header first, counted from the plan: {}",
            lines[0]
        );
        assert!(lines[1].contains("permits"), "the greeting: {}", lines[1]);
        // One line per settled task, in SETTLE order, exactly once each.
        assert_eq!(
            text.matches("fetch_top").count(),
            1,
            "a row speaks once — never a repeated storyboard: {text}"
        );
        let fetch = lines.iter().position(|l| l.contains("fetch_top"));
        let write = lines.iter().position(|l| l.contains("write_md"));
        let meter = lines.iter().position(|l| l.contains("done"));
        assert!(
            fetch < write && write < meter,
            "settle order, then the meter closes: {text}"
        );
        assert!(
            lines[meter.expect("meter")].contains("5/5 done"),
            "the close counts the run: {text}"
        );
        // ZERO cursor escapes — the CI-capture contract survives.
        assert!(
            !text.contains('\x1b'),
            "non-interactive lane leaks no ANSI: {text}"
        );
    }

    #[test]
    fn fold_sink_quiet_prints_only_the_verdict_card() {
        let theme = Theme::new(false, true, false);
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Quiet);
            for ev in demo::success() {
                sink.emit(ev);
            }
            sink.print_final();
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        // Quiet = the verdict line only · NO per-task storyboard · no escapes.
        assert!(text.contains("veille-news"), "verdict line: {text}");
        assert!(
            !text.contains("fetch_top"),
            "quiet hides the per-task rows: {text}"
        );
        assert!(!text.contains('\x1b'), "quiet leaks no ANSI: {text}");
    }

    #[test]
    fn fold_sink_interactive_repaints_in_place() {
        let theme = Theme::new(false, false, false);
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Live);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        // The Live lane DOES use cursor control (the live redraw).
        assert!(text.contains('\x1b'), "Live repaints in place");
    }

    #[test]
    fn fold_sink_failure_keeps_the_false_verdict() {
        let theme = Theme::new(false, false, false);
        let mut buf = Vec::new();
        let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Plain);
        for ev in demo::failure() {
            sink.emit(ev);
        }
        assert_eq!(sink.view().verdict, Some(false));
    }

    /// The outputs knob is OPT-IN per sink: the same output-carrying
    /// stream paints tails only after `show_outputs(true)` — the piped
    /// (`Plain`) register stays byte-free of tails unless a caller
    /// explicitly asks (the run verb only ever asks for `Live`).
    #[test]
    fn fold_sink_tails_are_opt_in() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let theme = Theme::new(false, true, false);
        let completed = demo::bare_event(EventKind::TaskCompleted, 5)
            .with_field(KeyValue::new("task", Value::String("audit".into())))
            .with_field(KeyValue::new("output", Value::String("[1,2]".into())));
        let render = |opt_in: bool| {
            let mut buf = Vec::new();
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Plain);
            sink.show_outputs(opt_in);
            sink.emit(completed.clone());
            sink.print_final();
            String::from_utf8(buf).expect("utf8")
        };
        assert!(!render(false).contains("->"), "default: no tails");
        assert!(
            render(true).contains("-> [2] · 5B"),
            "opted in: the tail rides: {}",
            render(true)
        );
    }

    /// The repaint clears the PRIOR frame only when one exists (`last_lines >
    /// 0`). On the FIRST Live event there is nothing to clear, so no cursor-up
    /// escape is written — a mutated guard (`== 0` / `>= 0`) would emit a
    /// spurious `\x1b[0A` over an empty screen, so the first frame must be
    /// escape-free (theme has color off · the only ANSI here would be cursor
    /// control).
    #[test]
    fn fold_sink_live_first_frame_writes_no_cursor_jump() {
        let theme = Theme::new(false, false, false);
        let mut buf = Vec::new();
        let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Live);
        let events = demo::success();
        sink.emit(events[0].clone()); // the FIRST event · last_lines was 0
        let text = String::from_utf8(buf).expect("utf8");
        assert!(
            !text.contains('\x1b'),
            "no prior frame to clear → no cursor-up escape: {text:?}"
        );
    }

    // ───────────────────────── run journal (TraceFileSink · Tee) ─────

    /// The spec §3.3 name form: `<ISO-compact>-<short>.ndjson` — second
    /// precision, `:` → `-`, the short id = the LAST 4 hex of the uuid.
    #[test]
    fn trace_file_name_matches_the_spec_form() {
        let id = Uuid::from_u128(0xa3f2);
        assert_eq!(
            trace_file_name(Timestamp::from_unix_ms(0), id),
            "1970-01-01T00-00-00Z-a3f2.ndjson"
        );
        // Milliseconds are dropped, never rounded (1.5s stays second 1).
        assert_eq!(
            trace_file_name(Timestamp::from_unix_ms(1500), id),
            "1970-01-01T00-00-01Z-a3f2.ndjson"
        );
    }

    /// Lazy open: a sink that never receives an event leaves ZERO fs
    /// footprint — no directory, no empty journal file.
    #[test]
    fn trace_sink_is_lazy_zero_events_zero_fs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("traces");
        {
            let sink = TraceFileSink::new(&dir);
            assert!(sink.path().is_none());
            assert!(sink.into_error().is_none());
        }
        assert!(!dir.exists(), "no emit → the directory is never created");
    }

    /// The journal can carry sensitive task output — on unix it is
    /// owner-only (0600) from creation.
    #[cfg(unix)]
    #[test]
    fn trace_sink_journal_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("traces");
        let mut sink = TraceFileSink::new(&dir);
        for ev in &demo::success() {
            sink.emit(ev.clone());
        }
        let path = sink.path().expect("opened on first emit").to_path_buf();
        let mode = std::fs::metadata(&path)
            .expect("stat the journal")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "journal perms: {mode:o}");
    }

    /// The journal is the `--json` lane SEMANTICALLY: same events in,
    /// each line carrying one extra committed key (`chain`) — key order
    /// is NOT byte-mirrored (the Value round-trip sorts keys).
    #[test]
    fn trace_sink_journal_mirrors_the_json_lane_semantically() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("traces");
        let events = demo::success();

        let mut expected = Vec::new();
        let mut json = JsonSink::new(&mut expected);
        for ev in &events {
            json.emit(ev.clone());
        }

        let mut sink = TraceFileSink::new(&dir);
        for ev in &events {
            sink.emit(ev.clone());
        }
        let path = sink.path().expect("opened on first emit").to_path_buf();
        assert!(path.starts_with(&dir), "journal lives under its dir");
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("ndjson"),
            "the spec extension"
        );
        assert!(sink.into_error().is_none(), "a writable dir never errors");

        let written = std::fs::read(&path).expect("journal readable");
        // The journal = the --json lane PLUS the chain field (0.96): each
        // line parses to the SAME event with one extra key committing to
        // the previous line's bytes. Semantic mirror, chained.
        let jl: Vec<serde_json::Value> = String::from_utf8(written)
            .expect("utf8")
            .lines()
            .map(|l| serde_json::from_str(l).expect("journal line parses"))
            .collect();
        let el: Vec<serde_json::Value> = String::from_utf8(expected)
            .expect("utf8")
            .lines()
            .map(|l| serde_json::from_str(l).expect("json line parses"))
            .collect();
        assert_eq!(jl.len(), el.len(), "one NDJSON line per event, both lanes");
        for (j, e) in jl.iter().zip(&el) {
            let mut j = j.clone();
            let chain = j
                .as_object_mut()
                .and_then(|o| o.remove("chain"))
                .expect("every journal line carries its chain");
            assert_eq!(chain.as_str().map(str::len), Some(64), "a full sha256 hex");
            assert_eq!(&j, e, "the event itself mirrors the --json lane");
        }
    }

    /// The opt-out shape: `disabled()` swallows every event with zero fs
    /// effects — the caller's wiring stays branch-free.
    #[test]
    fn trace_sink_disabled_never_touches_disk() {
        let mut sink = TraceFileSink::disabled();
        for ev in demo::success() {
            sink.emit(ev);
        }
        assert!(sink.path().is_none(), "disabled never opens");
        assert!(sink.into_error().is_none(), "disabled never errors");
    }

    /// Order-recording sink — proves the Tee delivery contract.
    struct RecordingSink {
        tag: char,
        log: std::rc::Rc<std::cell::RefCell<Vec<(char, u128)>>>,
    }

    impl EventSink for RecordingSink {
        fn emit(&mut self, event: Event) {
            self.log
                .borrow_mut()
                .push((self.tag, event.id.uuid.as_u128()));
        }
    }

    /// Tee fans each event to BOTH sinks, primary (`a`) first — per event,
    /// not batched (the rider observes the same liveness the surface does).
    #[test]
    fn tee_delivers_to_both_primary_first_per_event() {
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let a = RecordingSink {
            tag: 'a',
            log: std::rc::Rc::clone(&log),
        };
        let b = RecordingSink {
            tag: 'b',
            log: std::rc::Rc::clone(&log),
        };
        let mut tee = Tee::new(a, b);
        let events = demo::success();
        tee.emit(events[0].clone());
        tee.emit(events[1].clone());
        let (a, b) = tee.into_parts();
        assert_eq!((a.tag, b.tag), ('a', 'b'), "parts come back in order");
        let id0 = events[0].id.uuid.as_u128();
        let id1 = events[1].id.uuid.as_u128();
        assert_eq!(
            *log.borrow(),
            vec![('a', id0), ('b', id0), ('a', id1), ('b', id1)],
            "primary first · rider second · per event"
        );
    }

    /// Infallible under a broken destination: the error is buffered ONCE,
    /// later emits are silent no-ops, nothing panics (the run's verdict
    /// never depends on the journal).
    #[test]
    fn trace_sink_buffers_the_error_and_goes_silent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // A FILE where the directory must go → create_dir_all fails on
        // every platform (no chmod games · works under any uid).
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, b"not a dir").expect("fixture");
        let mut sink = TraceFileSink::new(blocker.join("traces"));
        for ev in demo::success() {
            sink.emit(ev); // first emit fails the open · the rest no-op
        }
        assert!(sink.path().is_none(), "a failed open never half-opens");
        assert!(sink.into_error().is_some(), "the fs error is buffered");
    }

    /// Two runs colliding on the same second + short id: the second open
    /// falls back to the FULL 32-hex id — never truncates a sibling.
    #[test]
    fn trace_sink_collision_falls_back_to_the_full_id_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("traces");
        let events = demo::success();

        let mut first = TraceFileSink::new(&dir);
        let mut second = TraceFileSink::new(&dir);
        for ev in &events {
            first.emit(ev.clone());
            second.emit(ev.clone()); // same ids · same second → collision
        }
        let p1 = first.path().expect("first opened").to_path_buf();
        let p2 = second.path().expect("second opened").to_path_buf();
        assert_ne!(p1, p2, "the fallback picked a different name");
        assert!(second.into_error().is_none(), "collision is not an error");
        let full = events[0].id.uuid.as_simple().to_string();
        assert!(
            p2.to_string_lossy().contains(&full),
            "fallback carries the full uuid: {p2:?}"
        );
        // Both journals intact — byte-identical streams, zero clobbering.
        assert_eq!(
            std::fs::read(&p1).expect("first journal"),
            std::fs::read(&p2).expect("second journal")
        );
    }

    /// Every Live REDRAW rides inside one DEC-2026 synchronized-output
    /// frame: `?2026h` opens BEFORE the cursor-up + clear, `?2026l`
    /// closes AFTER the last repainted line — the flicker-free contract
    /// on kitty/WezTerm/Ghostty (a no-op pair everywhere else). The
    /// FIRST paint (append-only) carries no pair.
    #[test]
    fn fold_sink_live_redraws_inside_synchronized_frames() {
        let theme = Theme::new(false, false, false);
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Live);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        let opens = text.matches("\x1b[?2026h").count();
        let closes = text.matches("\x1b[?2026l").count();
        let frames = demo::success().len();
        assert_eq!(opens, frames - 1, "every redraw opens a frame: {text:?}");
        assert_eq!(opens, closes, "every open closes");
        // The pair BRACKETS the redraw: the open is IMMEDIATELY followed
        // by the cursor-up escape (h before any clearing), and the very
        // last write closes the final frame (l after the repaint).
        assert!(
            text.contains("\x1b[?2026h\x1b["),
            "h opens before the cursor-up: {text:?}"
        );
        assert!(
            text.ends_with("\x1b[?2026l"),
            "the last write closes the frame"
        );
    }

    /// The spinner beat: while a task runs on the animated Live surface,
    /// `spin()` advances the braille phase and repaints; once everything
    /// settled it writes NOTHING (a finished screen never churns).
    #[test]
    fn spin_advances_the_running_glyph_and_stops_at_settle() {
        use crate::display::theme::ROUNDTRIP;
        let theme = Theme::new(false, false, true); // colour off · animated
        let mut fold = FoldSink::new(Vec::new(), theme, RenderMode::Live);
        // Feed the demo stream up to the FIRST running state — the
        // prefix length is the stream's business, not this test's.
        let events = demo::success();
        let mut fed = 0usize;
        for event in &events {
            fold.emit(event.clone());
            fed += 1;
            if fold
                .view()
                .rows()
                .iter()
                .any(|r| r.state == crate::TaskState::Running)
            {
                break;
            }
        }
        assert!(
            fold.view()
                .rows()
                .iter()
                .any(|r| r.state == crate::TaskState::Running),
            "the demo stream never ran a task?"
        );
        let before = fold.writer.len();
        fold.spin();
        fold.spin();
        let painted = String::from_utf8_lossy(&fold.writer[before..]).into_owned();
        // Two beats = two frames of the running verb's OWN motion (the
        // demo's running row is `invoke` → roundtrip, ticks 1 then 2).
        assert!(
            painted.contains(ROUNDTRIP[1]) && painted.contains(ROUNDTRIP[2]),
            "{painted:?}"
        );

        // Settle everything — the beat goes silent.
        for event in events.iter().skip(fed) {
            fold.emit(event.clone());
        }
        let settled = fold.writer.len();
        fold.spin();
        assert_eq!(fold.writer.len(), settled, "no churn after settle");
    }

    /// The sober lanes are tick-blind: Plain never repaints on spin.
    #[test]
    fn spin_is_a_no_op_off_the_live_lane() {
        let theme = Theme::new(false, false, true);
        let mut fold = FoldSink::new(Vec::new(), theme, RenderMode::Plain);
        for event in demo::success().iter().take(3) {
            fold.emit(event.clone());
        }
        let before = fold.writer.len();
        fold.spin();
        assert_eq!(fold.writer.len(), before, "Plain stays event-driven");
    }
}

/// Where the run journal's `trace:` pointer lands (per lane).
#[derive(Clone, Copy)]
pub(super) enum TraceNote {
    /// The human storytelling surfaces (`Live` · `Plain`) — the spec §3.3
    /// final-frame pointer, printed under the frame.
    Stdout,
    /// The machine lanes (`--json` · `--output json`) — their stdout is a
    /// byte-exact contract, so the pointer rides the diagnostic stream.
    Stderr,
    /// `--quiet` — the compact-card promise holds (no pointer · the
    /// journal is still written · an fs error still reaches stderr).
    Silent,
}

/// Surface the run journal AFTER the run — NEVER the exit code (the sink
/// contract: journaling is a rider, a broken rider is a note, not a
/// failure). An fs error goes to stderr with the path when one was opened;
/// a written journal prints its `trace:` pointer per [`TraceNote`].
pub(super) fn surface_trace(
    mut trace: TraceFileSink,
    note: TraceNote,
    autopsy: Option<&str>,
    workflow_hash: Option<&str>,
) -> Option<std::path::PathBuf> {
    // The run seal (S2 · verifiable runs): when a run-key exists on this
    // machine, the journal's LAST line is the signature that binds the
    // whole chain (head · count · workflow hash) to it. Emitted BEFORE
    // the durability point so the seal's own bytes are covered by the
    // fsync; additive — an absent key leaves the journal as today.
    let mut sealed = false;
    if let Some(hash) = workflow_hash
        && let Some((sk, pk_box)) = crate::seal::load_signing_key()
        && let Some(ev) = crate::seal::seal_event(
            EventId::generate(),
            Timestamp::from_unix_ms(now_millis()),
            trace.chain_head(),
            trace.chain_len(),
            hash,
            env!("CARGO_PKG_VERSION"),
            &sk,
            &pk_box,
        )
    {
        trace.emit(ev);
        sealed = true;
    }
    // Durability BEFORE advertisement: the anchor must describe bytes
    // that survive a power loss (flush reaches the page cache only).
    trace.finalize();
    let path = trace.path().map(std::path::Path::to_path_buf);
    // Read the anchor parts BEFORE into_error consumes the sink — they
    // are only ADVERTISED after the error gate below passes.
    let head = trace.chain_head().to_owned();
    let count = trace.chain_len();
    if let Some(e) = trace.into_error() {
        // Name the file when the failure struck AFTER the open (a partial
        // journal on disk) — the operator sees exactly what to distrust.
        match &path {
            Some(p) => eprintln!(
                "nika run: trace file {}: {e} — the run itself is unaffected",
                p.display()
            ),
            None => eprintln!("nika run: trace file: {e} — the run itself is unaffected"),
        }
        return None;
    }
    let path = path?; // disabled · or a run that emitted zero events
    let anchor = anchor_line(&path, count, &head, sealed);
    match note {
        TraceNote::Stdout => {
            println!("    {anchor}");
            // The autopsy line — a FAILED run teaches its own forensics:
            // the journal it just wrote replays, peeks and time-travels.
            if let Some(task) = autopsy {
                println!(
                    "    autopsy: nika trace peek {} {task} · replay: nika trace replay {} · or F5 in VS Code",
                    path.display(),
                    path.display()
                );
            }
        }
        TraceNote::Stderr => {
            eprintln!("nika run: {anchor}");
        }
        TraceNote::Silent => {}
    }
    // The published path rides back so a PAUSED machine lane can teach
    // the exact resume command over it (the pause sibling of `autopsy:`).
    Some(path)
}

/// The advertised anchor: the FULL 64-hex chain head — byte-exact parity
/// with what `nika trace verify` prints, so the taught loop (compare the
/// two heads) closes with `==`, never a prefix match (#333). The scrollback
/// head is the ONE thing a whole-file rewrite cannot touch (the writer-side
/// review's M4 — 32 hex was already unforgeable; full width costs only
/// line length and buys equality).
fn anchor_line(path: &std::path::Path, count: usize, head: &str, sealed: bool) -> String {
    let proof = if sealed { " · sealed" } else { "" };
    format!(
        "trace: {} · {count} events · chain {head}{proof}",
        path.display()
    )
}

/// Wall-clock milliseconds for the seal's stamp (the L4 boundary — the
/// journal's own clock; runtime crates ride `ClockDyn`).
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
