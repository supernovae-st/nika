// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a probe that cannot read its screen fails"
)]
#![allow(
    clippy::disallowed_types,
    reason = "the probe reads the child's RSS and CPU time with ps"
)]
//! The echo-latency probe of `docs/qa/tui/RECEPTION.md` (performance
//! thresholds). Ignored in ordinary runs; run it alone:
//!
//! ```text
//! cargo test -p nika-tui --test qa_latency --locked -- --ignored --nocapture
//! ```
//!
//! Each keystroke is written to the PTY and timed until its glyph comes back
//! in the bytes the renderer writes (a two-byte UTF-8 glyph, which no escape
//! sequence contains), the next key only after the echo: 200 keys per
//! scenario, nearest-rank p50/p95/p99. It also reports the bytes written per
//! keystroke (the redraw cost on the wire), the child's CPU time per
//! keystroke (the redraw cost in the process, from `ps`, 10 ms resolution on
//! macOS), the resident memory, and what a key typed during a busy turn does.

#[path = "qa_support/child.rs"]
mod child;
mod qa_support;

use std::fmt::Write as _;
use std::io::Write as _;
use std::process::Command;
use std::time::{Duration, Instant};

use child::{BANNER, BUSY, DONE, Release};
use qa_support::{FREE, Term};

const KEYS: usize = 200;
const GLYPHS: [char; 4] = ['é', 'ü', 'ø', 'å'];
/// Glyphs per word: the probe types words, never one word wider than the
/// composer.
const WORD: usize = 20;

/// The child side, re-invoked by the probe; nothing in a plain run.
#[test]
fn qa_child_host() {
    child::host();
}

/// One `ps` field of `pid`, trimmed.
fn ps(pid: i32, field: &str) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", &format!("{field}="), "-p", &pid.to_string()])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Resident memory in KiB.
fn rss_kib(pid: i32) -> Option<u64> {
    ps(pid, "rss")?.parse().ok()
}

/// CPU time in milliseconds from `ps -o time=` (`[[dd-]hh:]mm:ss[.ff]`).
fn cpu_ms(pid: i32) -> Option<u64> {
    let text = ps(pid, "time")?;
    let (clock, fraction) = text.split_once('.').unwrap_or((text.as_str(), "0"));
    let mut seconds = 0u64;
    for part in clock.split([':', '-']) {
        seconds = seconds * 60 + part.parse::<u64>().ok()?;
    }
    let centis: u64 = format!("{fraction:0<2}").get(..2)?.parse().ok()?;
    Some(seconds * 1000 + centis * 10)
}

/// Nearest-rank percentile of sorted samples.
fn rank(sorted: &[Duration], percent: usize) -> Duration {
    let index = (sorted.len() * percent).div_ceil(100).saturating_sub(1);
    sorted.get(index).copied().unwrap_or_default()
}

fn ms(duration: Duration) -> String {
    let micros = duration.as_micros();
    format!("{}.{:02}", micros / 1000, (micros % 1000) / 10)
}

/// Type `KEYS` glyphs, each timed from the write to its echo.
fn echo_line(term: &mut Term, scenario: &str) -> String {
    let pid = term.pid();
    let cpu_before = cpu_ms(pid);
    let mut samples = Vec::with_capacity(KEYS);
    let mut bytes = Vec::with_capacity(KEYS);
    for index in 0..KEYS {
        if index % WORD == 0 && index > 0 {
            // A space (not timed) keeps every word narrower than the
            // composer: a wider one is clipped, not echoed (see
            // qa_input `a_word_wider_than_the_composer_stays_visible`).
            term.send(" ");
            term.settle(Duration::from_millis(5));
        }
        let glyph = GLYPHS[index % GLYPHS.len()];
        let mut buf = [0u8; 4];
        let encoded = glyph.encode_utf8(&mut buf);
        term.settle(Duration::from_millis(2));
        let mark = term.mark();
        let sent = Instant::now();
        term.send(encoded);
        let seen = term.spin_until_bytes(mark, encoded.as_bytes());
        samples.push(seen - sent);
        term.settle(Duration::from_millis(3));
        bytes.push(term.mark() - mark);
    }
    let cpu = match (cpu_before, cpu_ms(pid)) {
        (Some(before), Some(after)) => format!(
            "{} ms/key",
            ms(Duration::from_millis(after.saturating_sub(before))
                / u32::try_from(KEYS).unwrap_or(1))
        ),
        _ => "n/a".to_owned(),
    };
    samples.sort_unstable();
    bytes.sort_unstable();
    format!(
        "{scenario:<44} p50 {:>7} p95 {:>7} p99 {:>7} max {:>7} ms · {:>5} B/key · cpu {cpu} · rss {} KiB",
        ms(rank(&samples, 50)),
        ms(rank(&samples, 95)),
        ms(rank(&samples, 99)),
        ms(samples.last().copied().unwrap_or_default()),
        bytes.get(bytes.len() / 2).copied().unwrap_or(0),
        rss_kib(pid).map_or_else(|| "n/a".to_owned(), |kib| kib.to_string()),
    )
}

/// A key typed during a busy turn: how long until it shows, and whether it
/// showed before the turn ended.
fn during_busy_turn() -> String {
    let release = Release::new("latency");
    let mut term = child::spawn("slow-free", Some(release.path()), 100, 32);
    term.wait_prompt(FREE);
    term.send("work\r");
    term.wait_text(BUSY);
    let mark = term.mark();
    let sent = Instant::now();
    term.send("é");
    let hold = Duration::from_secs(2);
    while sent.elapsed() < hold && !term.raw_since(mark).contains('é') {
        term.pump();
        std::thread::sleep(Duration::from_millis(1));
    }
    let during = term.raw_since(mark).contains('é');
    let released = Instant::now();
    release.open();
    let shown = term.spin_until_bytes(mark, "é".as_bytes());
    term.wait_text(DONE);
    format!(
        "key typed during a busy turn (spinner turning) · shown during the turn: {during} · shown {} ms after the release ({} ms after the key)",
        ms(shown.saturating_duration_since(released)),
        ms(shown - sent)
    )
}

/// What the busy row costs while it turns: bytes written over a 2 s window
/// of a held turn, per 100 ms spinner step (the activity budget is per row
/// per step).
fn spinner_cost(presentation: &str) -> String {
    let release = Release::new(&format!("spinner-{presentation}"));
    let mode = format!("slow-free:0:{presentation}");
    let mut term = child::spawn(&mode, Some(release.path()), 100, 32);
    term.wait_prompt(FREE);
    term.send("work\r");
    term.wait_text(BUSY);
    term.settle(Duration::from_millis(300));
    let mark = term.mark();
    let window = Duration::from_secs(2);
    term.settle(window);
    let written = term.mark() - mark;
    release.open();
    term.wait_text(DONE);
    let steps = window.as_millis() / 100;
    format!(
        "busy spinner, {presentation:<6} 100x32                  {written} B over 2 s · {} B per 100 ms step",
        u128::try_from(written).unwrap_or(0) / steps
    )
}

/// The idle proto: time to the first prompt, then the echo line.
fn proto(args: &[&str], cols: u16, rows: u16, scenario: &str) -> Vec<String> {
    let started = Instant::now();
    let mut term = Term::proto(args, cols, rows);
    term.wait_prompt(FREE);
    let first = started.elapsed();
    vec![
        format!("{scenario:<44} first prompt {} ms", ms(first)),
        echo_line(&mut term, scenario),
    ]
}

/// The child opening on `lines` transcript lines.
fn big(lines: usize, focus: bool, cols: u16, rows: u16) -> Vec<String> {
    let place = if focus { "focus" } else { "inline" };
    let scenario = format!("{place} {cols}x{rows} · {lines} transcript lines");
    let started = Instant::now();
    let mut term = child::spawn(&format!("big:{lines}:{place}"), None, cols, rows);
    term.wait_prompt(FREE);
    let first = started.elapsed();
    let opening = term.mark();
    let banner = term.screen.seen(BANNER);
    vec![
        format!(
            "{scenario:<44} first prompt {} ms · {} KiB written at open · banner kept {banner}",
            ms(first),
            opening / 1024
        ),
        echo_line(&mut term, &scenario),
    ]
}

/// A turn whose busy row receives `labels` labels as fast as they come.
fn flood(labels: usize) -> String {
    let mut term = child::spawn(&format!("flood:{labels}"), None, 100, 32);
    term.wait_prompt(FREE);
    let mark = term.mark();
    let sent = Instant::now();
    term.send("work\r");
    term.wait_text(DONE);
    format!(
        "busy row flooded with {labels} labels             turn {} ms · {} KiB written",
        ms(sent.elapsed()),
        (term.mark() - mark) / 1024
    )
}

#[test]
#[ignore = "probe: prints echo latency percentiles; run with --ignored --nocapture"]
fn echo_latency_probe() {
    let mut report = String::new();
    let load = Command::new("uptime")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default();
    let _ = writeln!(
        report,
        "nika-tui echo latency probe · {KEYS} keys per scenario · {load}"
    );
    let mut lines = proto(&[], 80, 24, "proto inline 80x24 idle");
    lines.extend(proto(&["--focus"], 120, 40, "proto focus 120x40 idle"));
    lines.extend(big(10_000, true, 120, 40));
    lines.extend(big(10_000, false, 80, 24));
    lines.push(during_busy_turn());
    lines.push(spinner_cost("inline"));
    lines.push(spinner_cost("focus"));
    lines.push(flood(100_000));
    for line in lines {
        let _ = writeln!(report, "{line}");
    }
    let _ = std::io::stdout().write_all(report.as_bytes());
}
