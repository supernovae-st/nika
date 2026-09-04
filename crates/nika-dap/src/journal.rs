// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run journal's WRITE half — descended from `nika-cli`'s run verb
//! 2026-07-22 (the 15k wall · compute descends, render stays): the
//! production journal sink ([`TraceFileSink`] · `.nika/traces/` · the
//! `trace:` pointer's file), the `--json` NDJSON lane ([`JsonSink`]),
//! and the fan-out combinator ([`Tee`]) that tees the journal BESIDE
//! any primary surface.
//!
//! One format, one home: the writer speaks the SAME chain the walk in
//! [`crate::chain`] verifies (one constant · one hash — the genesis tag
//! is imported, never duplicated), the tolerant reader in
//! [`crate::recover`] parses these exact bytes back, and the resume
//! fold in [`crate::resume`] builds the skip plan from them. The
//! journal directory constant lives in [`crate::store::TRACE_DIR`]
//! (the store scan's home) — the caller names it, never this module.
//!
//! All lanes are consumers of the SAME stream (the fold law · spec §3):
//! the sink shape decides the surface, never the runtime. The sink
//! contract is INFALLIBLE (a write error never changes the run's
//! verdict — it is buffered and surfaced at the end).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use nika_event::Event;
use nika_runtime::EventSink;
use nika_types::id::{EventId, ExecutionId, TraceId};
use nika_types::timestamp::Timestamp;
use uuid::Uuid;

// The chain primitive + genesis tag live beside the walk — the sink
// WRITES the same chain the walk verifies (one constant, one hash).
use crate::chain::CHAIN_GENESIS;
use nika_event::source_id::sha256_hex;

/// The chain state EVERY lane drives identically (the module's « one
/// constant · one hash » law made a type): sha256 hex of the LAST
/// line's exact bytes rides the NEXT line as its `chain` field. The
/// head commits only after the bytes landed — a failed write must never
/// advance it, or a later "N events · chain X" note would describe
/// bytes no surface holds.
struct ChainState {
    chain: String,
}

impl ChainState {
    /// The genesis head — sha256 of the shared tag the walk verifies
    /// against.
    fn genesis() -> Self {
        Self {
            chain: sha256_hex(CHAIN_GENESIS),
        }
    }

    /// Serialize `event` as one journal line with the `chain` field
    /// inserted, plus the head those exact bytes mint — WITHOUT
    /// committing it (the caller commits once the write lands). Hashing
    /// the written bytes, never a re-serialization, is what makes
    /// `trace verify` total: no canonical-JSON contract to drift. Event
    /// consumers ignore the extra field (tolerant serde — pinned in
    /// verify tests).
    fn line<T: serde::Serialize>(&self, record: &T) -> std::io::Result<(String, String)> {
        let mut value = serde_json::to_value(record).map_err(std::io::Error::from)?;
        let Some(obj) = value.as_object_mut() else {
            // A non-object event is a WRITER defect — fail loudly here;
            // a silent chainless line would read as an Unchained/Broken
            // verdict (an attack) downstream.
            return Err(std::io::Error::other(
                "event did not serialize to a JSON object",
            ));
        };
        obj.insert(
            "chain".to_owned(),
            serde_json::Value::String(self.chain.clone()),
        );
        let line = serde_json::to_string(&value).map_err(std::io::Error::from)?;
        let next = sha256_hex(line.as_bytes());
        Ok((line, next))
    }

    /// Commit the head the landed bytes minted.
    fn commit(&mut self, next: String) {
        self.chain = next;
    }

    /// The current head — the last landed line's hash (the genesis
    /// before anything landed).
    fn head(&self) -> &str {
        &self.chain
    }
}

/// Writes one NDJSON line per event to the wrapped writer (the `--json`
/// lane · "NDJSON events verbatim · CI/agents" · spec §3). Never
/// coloured. Flushes per event so a tailing agent sees liveness. Each
/// line carries the `chain` field — the stream is a first-class journal
/// (ADR-099 §5 follow-on): a captured stream verifies, and a broken one
/// is refused on resume like any forged journal.
pub struct JsonSink<W: Write> {
    writer: W,
    /// The first write error, buffered (the sink contract is infallible
    /// w.r.t. the run · the caller checks this after the run).
    error: Option<std::io::Error>,
    /// The tamper-evidence chain, shared with the file lane.
    chain: ChainState,
}

impl<W: Write> JsonSink<W> {
    /// Wrap a writer (typically `io::stdout().lock()`).
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            error: None,
            chain: ChainState::genesis(),
        }
    }

    /// The buffered write error, if delivery ever failed.
    pub fn into_error(self) -> Option<std::io::Error> {
        self.error
    }

    /// Borrow the buffered delivery error without reviving the failed sink.
    pub fn error(&self) -> Option<&std::io::Error> {
        self.error.as_ref()
    }

    /// Append one arbitrary JSON object to this stream under the same chain
    /// state as runtime events.
    ///
    /// # Errors
    /// Returns a buffered event-delivery error, a serialization error, or the
    /// writer error. The chain advances only after the complete line lands.
    pub fn write_record<T: serde::Serialize>(&mut self, record: &T) -> std::io::Result<()> {
        if let Some(error) = self.error.as_ref() {
            return Err(std::io::Error::new(error.kind(), error.to_string()));
        }
        if let Err(error) = self.write_record_inner(record) {
            self.error = Some(std::io::Error::new(error.kind(), error.to_string()));
            return Err(error);
        }
        Ok(())
    }

    fn write_record_inner<T: serde::Serialize>(&mut self, record: &T) -> std::io::Result<()> {
        let (line, next) = self.chain.line(record)?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        self.chain.commit(next);
        Ok(())
    }
}

impl<W: Write> EventSink for JsonSink<W> {
    fn emit(&mut self, event: Event) {
        if self.error.is_some() {
            return; // already broken · stop touching a dead pipe
        }
        if let Err(error) = self.write_record_inner(&event) {
            self.error = Some(error);
        }
    }
}

/// The run-journal state: which disk lane, if any, this sink drives.
enum Lane {
    /// `--no-trace-file` / `NIKA_NO_TRACE_FILE` / a non-workspace run
    /// (`try` stages a temp file) — emit is a no-op by design,
    /// so the caller keeps ONE code path whether journaling or not.
    Disabled,
    /// Enabled but not yet on disk — the file is named lazily from the
    /// service trace identity when bound, or the first event's identity for
    /// legacy callers, so nothing can open before the stream starts.
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
/// - **Lazy open** — file creation waits for the FIRST emit: the name uses
///   the bound typed trace ID (legacy callers fall back to event identity)
///   plus the event timestamp, and a run that never starts (audit refusal ·
///   composition failure) must not litter an empty file.
/// - **Infallible** (the [`EventSink`] contract) — an fs error (read-only
///   checkout · disk full) is buffered, never panics, never changes the
///   run's verdict or its primary bytes; the caller surfaces it AFTER
///   the run as a stderr note.
/// - **Rider, never a surface** — it tees BESIDE the chosen primary lane
///   (via [`Tee`]); with the sink disabled or broken, the primary lane's
///   output stays byte-identical.
pub struct TraceFileSink {
    /// The journal directory (created on first emit · the store's
    /// `TRACE_DIR` in production · a temp dir under test). Meaningless
    /// when disabled.
    dir: PathBuf,
    /// Service-minted root identity. When present, every journaled event is
    /// stamped with this execution and the physical journal is named from
    /// the corresponding typed trace identity rather than inferred from an
    /// arbitrary first event.
    execution: Option<ExecutionId>,
    trace: Option<TraceId>,
    lane: Lane,
    /// The opened file's path (`None` until the lazy open · stays `None`
    /// when disabled or when the open itself failed).
    path: Option<PathBuf>,
    /// The first fs error, buffered (the sink contract is infallible
    /// w.r.t. the run · the caller surfaces this after the run).
    error: Option<std::io::Error>,
    /// The tamper-evidence chain, shared with the stream lane — after
    /// the run, its head is the HEAD the anchor trio prints.
    chain: ChainState,
    /// Lines actually written (the anchor trio's count).
    written: usize,
    /// The liveness lease held for the journal's lifetime (ADR-129 ·
    /// `<trace>.lock`): `None` when disabled, when the open failed, or
    /// when the lease could not be taken (fail-open: the store then reads
    /// `unknown`, never a guess).
    lease: Option<crate::liveness::Lease>,
}

impl TraceFileSink {
    /// An enabled journal rooted at `dir` (lazy — no fs effect here).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            execution: None,
            trace: None,
            lane: Lane::Pending,
            path: None,
            error: None,
            chain: ChainState::genesis(),
            written: 0,
            lease: None,
        }
    }

    /// A permanently-silent journal (`emit` = no-op · zero fs effects) —
    /// the opt-out shape that keeps the caller's wiring branch-free.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            dir: PathBuf::new(),
            execution: None,
            trace: None,
            lane: Lane::Disabled,
            path: None,
            error: None,
            chain: ChainState::genesis(),
            written: 0,
            lease: None,
        }
    }

    /// Bind this journal to one service-admitted root execution.
    ///
    /// The two IDs stay distinct types even though the execution UUID bytes
    /// deterministically seed the W3C root trace ID. The trace ID addresses
    /// the file; the execution ID annotates every event written to it.
    #[must_use]
    pub fn for_execution(mut self, execution: ExecutionId, trace: TraceId) -> Self {
        debug_assert_eq!(TraceId::from(execution), trace);
        self.execution = Some(execution);
        self.trace = Some(trace);
        self
    }

    /// The journal file's path, once the lazy open happened.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether this journal is the permanently-silent opt-out shape —
    /// the fact a teaching surface needs BEFORE any lazy open: a door
    /// taught toward a disabled journal is a door to a file that will
    /// never exist (`path()` is lazily `None` on BOTH shapes early, so
    /// it cannot carry this distinction).
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        matches!(self.lane, Lane::Disabled)
    }

    /// The buffered fs error, if journaling ever failed.
    #[must_use]
    pub fn into_error(self) -> Option<std::io::Error> {
        self.error
    }

    /// The chain HEAD — sha256 of the last written line's exact bytes.
    /// Printing it (CI logs · scrollback) is the free external anchor
    /// that upgrades tamper-EVIDENT toward attributable.
    #[must_use]
    pub fn chain_head(&self) -> &str {
        self.chain.head()
    }

    /// Lines written — the anchor trio's middle term (the C2SP
    /// checkpoint shape: origin · size · root).
    #[must_use]
    pub fn chain_len(&self) -> usize {
        self.written
    }

    /// Durability point — called ONCE before the anchor is advertised:
    /// `flush()` reaches the page cache only; a power loss after the
    /// anchor printed but before writeback would leave a shorter-but-
    /// clean prefix on disk, and the anchor mismatch would FORGE a
    /// tamper alarm against the operator's own hardware. One fsync per
    /// run buys an honest anchor.
    pub fn finalize(&mut self) {
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

    /// Create the directory + the journal file, addressed by the bound trace
    /// identity or, for a legacy caller, the first event's identity.
    ///
    /// Execution identity names the journal directly. Legacy events fall
    /// back to run identity, then event identity, without a timestamp scan.
    fn open(&mut self, first: &Event) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let id = self.trace.map_or_else(
            || journal_identity(first),
            |trace| Uuid::from_bytes(trace.bytes),
        );
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
        // The liveness lease (ADR-129): held until this sink drops — the
        // kernel releases it when the process ends, however it ends. A
        // lease that cannot be taken leaves the reader honest (`unknown`).
        self.lease = crate::liveness::hold(&path).ok();
        self.lane = Lane::Open(BufWriter::new(file));
        self.path = Some(path);
        Ok(())
    }
}

fn journal_identity(first: &Event) -> Uuid {
    first.execution.map_or_else(
        || first.run.map_or(first.id.uuid, |run| run.uuid),
        |id| id.uuid,
    )
}

impl EventSink for TraceFileSink {
    fn emit(&mut self, mut event: Event) {
        if let Some(execution) = self.execution {
            event.execution = Some(execution);
        }
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
        // for liveness). The line shape (event + `chain` field · hash of
        // the written bytes) lives in ChainState — one law, both lanes.
        let result = self.chain.line(&event).and_then(|(line, next)| {
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
            writer.flush()?;
            Ok(next)
        });
        // Chain state commits only AFTER the bytes landed: a failed
        // write must never advance the head (a later "N events · chain
        // X" note would otherwise describe bytes no file holds).
        match result {
            Ok(next) => {
                self.chain.commit(next);
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
/// journal rides beside every primary surface without forking the
/// drive: the primary lane `a` keeps its exact bytes; the rider `b`
/// (the trace file) observes the same stream. Both sides are
/// [`EventSink`]s, so infallibility composes — neither can veto the
/// run or the other lane.
pub struct Tee<A: EventSink, B: EventSink> {
    a: A,
    b: B,
}

impl<A: EventSink, B: EventSink> Tee<A, B> {
    /// Pair the primary surface `a` with the rider `b`.
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }

    /// Take both lanes back after the run (the caller reads the fold's
    /// verdict from `a` and surfaces buffered errors from each side).
    pub fn into_parts(self) -> (A, B) {
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

/// The run seal (S2 · verifiable runs): when a run-key exists on this
/// machine, the journal's LAST line is the signature that binds the
/// whole chain (head · count · workflow hash) to it. Called BEFORE the
/// durability point so the seal's own bytes are covered by the fsync;
/// additive — an absent key leaves the journal as today. Returns
/// `true` when the seal event landed. (Descended from the run verb's
/// `surface_trace` seal block 2026-07-22 — the journal seals itself in
/// the journal's home; `CARGO_PKG_VERSION` is the one workspace version
/// on both sides of the old seam, so the sealed bytes are unchanged.)
/// The teardown-less path: the classic four `covers` fields
/// ([`seal_journal_with`] folds the F-P2 teardown in).
pub fn seal_journal(trace: &mut TraceFileSink, workflow_hash: Option<&str>) -> bool {
    seal_journal_with(trace, workflow_hash, None)
}

/// [`seal_journal`] with the run's teardown facts (F-P2 · LOT-1): the
/// seal's `covers` attests the receipt digest, the budgets ρ, the
/// effects ε and the failed run's quarantine fold (F-P14) the run
/// settled with — the run's END is as attested as its boot. `None`
/// keeps the classic covers (byte-unchanged).
pub fn seal_journal_with(
    trace: &mut TraceFileSink,
    workflow_hash: Option<&str>,
    teardown: Option<&crate::seal::SealTeardown>,
) -> bool {
    // A disabled journal (`nika try` · `--no-trace-file`) has nothing to
    // sign. Consulting the keychain anyway is a hang after a green card.
    if trace.is_disabled() {
        return false;
    }
    // F-P8 · the law's third leg: the named memory rejections land in the
    // journal BEFORE the seal mints, so the chain the seal signs covers
    // the evidence its `covers["memory"]` counts (the seal commits to
    // every prior line — these included).
    if let Some(teardown) = teardown {
        crate::memory::journal_rejected(trace, &teardown.memory_rejected);
    }
    let mut sealed = false;
    if let Some(hash) = workflow_hash
        && let Some((sk, pk_box)) = crate::seal::load_signing_key()
        && let Some(ev) = crate::seal::seal_event_with(
            EventId::generate(),
            Timestamp::from_unix_ms(now_millis()),
            trace.chain_head(),
            trace.chain_len(),
            hash,
            env!("CARGO_PKG_VERSION"),
            teardown,
            &sk,
            &pk_box,
        )
    {
        trace.emit(ev);
        sealed = true;
    }
    sealed
}

/// Wall-clock milliseconds for the seal's stamp (the L4 boundary — the
/// journal's own clock; runtime crates ride `ClockDyn`). The teardown-side
/// epilogue events ([`crate::memory`]'s `memory_entry_rejected`) stamp
/// from the same voice.
pub(crate) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_display::demo;
    use nika_types::id::{ExecutionId, RunId};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct FailFirstFlush {
        bytes: Rc<RefCell<Vec<u8>>>,
        failed: bool,
    }

    impl Write for FailFirstFlush {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.bytes.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.failed {
                Ok(())
            } else {
                self.failed = true;
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
        }
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

    #[test]
    fn json_sink_exposes_a_buffered_runtime_error_before_settlement() {
        let mut sink = JsonSink::new(BrokenWriter);
        sink.emit(demo::success().remove(0));
        assert_eq!(
            sink.error().map(std::io::Error::kind),
            Some(std::io::ErrorKind::BrokenPipe)
        );
        assert!(
            sink.into_error().is_some(),
            "the buffered error stays owned"
        );
    }

    #[test]
    fn json_sink_stays_dead_after_a_terminal_record_flush_fails() {
        let bytes = Rc::new(RefCell::new(Vec::new()));
        let writer = FailFirstFlush {
            bytes: Rc::clone(&bytes),
            failed: false,
        };
        let mut sink = JsonSink::new(writer);
        assert!(
            sink.write_record(&serde_json::json!({"kind": "workflow_completed"}))
                .is_err(),
            "the first flush fails"
        );
        assert!(
            sink.write_record(&serde_json::json!({"kind": "run_settled"}))
                .is_err(),
            "a failed sink refuses every later record"
        );
        let raw = String::from_utf8(bytes.borrow().clone()).expect("utf8");
        assert_eq!(raw.lines().count(), 1, "no stale-head line is appended");
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

    #[test]
    fn execution_identity_names_the_trace_without_a_scan() {
        let event = demo::success()
            .into_iter()
            .next()
            .expect("demo event")
            .with_run(RunId::from_bytes([2; 16]))
            .with_execution(ExecutionId::from_bytes([3; 16]));
        assert_eq!(journal_identity(&event), Uuid::from_bytes([3; 16]));
    }

    #[test]
    fn bound_trace_identity_names_and_stamps_the_journal() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let execution = ExecutionId::from_bytes([0x3a; 16]);
        let trace = TraceId::from(execution);
        let mut sink = TraceFileSink::new(tmp.path()).for_execution(execution, trace);
        let event = demo::success().into_iter().next().expect("demo event");

        sink.emit(event);

        let path = sink.path().expect("bound journal path");
        assert!(
            path.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|name| name.ends_with("-3a3a.ndjson")),
            "the typed trace identity addresses the physical journal: {}",
            path.display()
        );
        let raw = std::fs::read_to_string(path).expect("journal bytes");
        let recovered = crate::recover::recover_events(&raw, &path.display().to_string())
            .expect("typed event stream");
        assert_eq!(recovered.events[0].execution, Some(execution));
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

    /// The journal is the `--json` lane BYTE FOR BYTE: same events in,
    /// same `ChainState`, same line shape — since the ADR-099 §5
    /// follow-on the stream carries the chain too, so nothing is left
    /// to differ.
    #[test]
    fn trace_sink_journal_mirrors_the_json_lane_byte_for_byte() {
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
        // Both lanes drive the SAME ChainState over the SAME events:
        // the journal file is the --json stream BYTE FOR BYTE (the
        // 0.96 « journal = stream + chain key » relation is promoted —
        // the stream carries the chain too since the ADR-099 §5
        // follow-on, so nothing is left to differ).
        assert_eq!(
            written, expected,
            "the journal file IS the --json stream, byte for byte"
        );
    }

    /// The `--json` lane speaks the chain the walk verifies: line N+1's
    /// `chain` field is sha256 of line N's exact bytes, genesis first.
    /// Mutation-sensitive by construction — drop the insert or break
    /// the advance and this goes red.
    #[test]
    fn json_sink_lines_carry_the_walkable_chain() {
        let mut buf = Vec::new();
        {
            let mut sink = JsonSink::new(&mut buf);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert!(sink.into_error().is_none(), "the vec writer never fails");
        }
        let text = String::from_utf8(buf).expect("utf8");
        let mut prev = sha256_hex(CHAIN_GENESIS);
        let mut n = 0usize;
        for line in text.lines() {
            let value: serde_json::Value =
                serde_json::from_str(line).expect("each line is one JSON Event");
            let chain = value["chain"].as_str().expect("the chain field rides");
            assert_eq!(chain, prev, "line {n} commits to the previous line's bytes");
            prev = sha256_hex(line.as_bytes());
            n += 1;
        }
        assert!(n > 1, "the demo stream holds several events");
    }

    #[test]
    fn json_sink_chains_a_terminal_non_event_record() {
        let mut buf = Vec::new();
        {
            let mut sink = JsonSink::new(&mut buf);
            sink.emit(demo::success().remove(0));
            sink.write_record(&serde_json::json!({"kind": "run_settled"}))
                .expect("terminal record writes");
            assert!(sink.into_error().is_none(), "the vec writer never fails");
        }
        let text = String::from_utf8(buf).expect("utf8");
        assert!(
            matches!(
                crate::chain::walk(&text),
                crate::chain::Verdict::Intact { events: 2, .. }
            ),
            "the generic terminal record closes the same chain: {text}"
        );
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
}
