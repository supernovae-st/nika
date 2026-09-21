// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `run:` declaration resolved to its execution seams (F-P3) — the
//! stamper, the clock and the transport backoff one run reads. Split from
//! `compose.rs` at the 1500-line file cap when the backoff joined the
//! seams; the bodies moved verbatim.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use nika_clock::DeclaredClock;
use nika_kernel::clock::ClockDyn;
use nika_providers::Backoff;
use nika_schema::types::RunDecl;

/// The `run:` declaration resolved to its execution seams (F-P3 · ONE
/// law, ONE home): which stamper mints event identities, which clock the
/// run's deadlines/sleeps/durations measure, and the retry jitter
/// stream's seed. An absent `run:` block (or an empty declaration)
/// resolves to the exact status quo — system stamps · system clock ·
/// the zero jitter stream.
///
/// The declared contradictions (`entropy: ambient` × `clock: virtual` ·
/// `entropy: none | seeded` × `clock: system`) are refused at PARSE, so
/// this resolution reads the post-refusal space; a caller bypassing the
/// parser still lands deterministic-entropy on the deterministic seams
/// (the fail-safe direction — never the ambient one).
pub struct RunSeams {
    /// The event-identity seam: deterministic (seq→UUID · +10ms/event)
    /// or system (`UUIDv7` · wall clock).
    stamps_deterministic: bool,
    /// The run's ONE clock (FDB/VOPR law) — deadlines · sleeps ·
    /// durations all measure it.
    pub clock: DeclaredClock,
    /// The retry jitter stream's seed (`(seed, task, attempt)` —
    /// replay-stable by construction).
    pub jitter_seed: u64,
}

impl RunSeams {
    /// Resolve the authored `run:` block (`None` = absent) to its seams.
    #[must_use]
    pub fn of(decl: Option<&RunDecl>) -> Self {
        let decl = decl.copied().unwrap_or_default();
        let entropy = decl.entropy_or_default();
        Self {
            stamps_deterministic: entropy.is_deterministic(),
            clock: match decl.clock_or_default() {
                nika_schema::types::RunClock::Virtual => DeclaredClock::r#virtual(),
                _ => DeclaredClock::system(),
            },
            jitter_seed: entropy.jitter_seed(),
        }
    }

    /// The stamper this resolution picks (the composer's event-identity
    /// seam — deterministic under `entropy: none | seeded`, the live
    /// system stamper otherwise).
    #[must_use]
    pub fn stamper(&self) -> Box<dyn crate::Stamper> {
        if self.stamps_deterministic {
            Box::new(crate::DeterministicStamper::new())
        } else {
            Box::new(crate::SystemStamper::new())
        }
    }

    /// The transport backoff this resolution picks — the provider
    /// layer's bounded retry on a rate-limited or overloaded seat (429 ·
    /// 503 · 529 · `Retry-After`) sleeps on the run's ONE clock. A
    /// `clock: virtual` run waits zero wall seconds (the wait is
    /// recorded on the frame, never slept); the system clock sleeps for
    /// real. The registry's own default is the system clock: a
    /// composition that forgot this seam let a virtual-clock run sleep
    /// real seconds on a 429 (the product-convergence war room · L4).
    #[must_use]
    pub fn backoff(&self) -> Arc<dyn Backoff> {
        Arc::new(DeclaredBackoff(self.clock.clone()))
    }
}

/// The run's declared clock as the transport backoff's sleep seam (the
/// [`nika_providers::ClockBackoff`] shape over [`DeclaredClock`], which
/// names its variant in `Debug` instead of deriving it).
#[derive(Clone)]
struct DeclaredBackoff(DeclaredClock);

impl fmt::Debug for DeclaredBackoff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let clock = if self.0.as_virtual().is_some() {
            "virtual"
        } else {
            "system"
        };
        write!(f, "DeclaredBackoff({clock})")
    }
}

impl Backoff for DeclaredBackoff {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.0.sleep(duration))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_schema::types::RunEntropy;

    // ── L4 · the declared clock governs the transport backoff ─────────

    /// A `clock: virtual` run must not sleep on a rate-limited seat: the
    /// backoff rides the declared clock, and the virtual one returns at
    /// once — a 60 s wait costs no wall time (the frame records it).
    #[tokio::test]
    async fn the_virtual_clock_backoff_never_sleeps() {
        let decl = RunDecl::new(None, Some(nika_schema::types::RunClock::Virtual));
        let backoff = RunSeams::of(Some(&decl)).backoff();
        let start = std::time::Instant::now();
        backoff.sleep(Duration::from_secs(60)).await;
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "a virtual wait is instant, took {:?}",
            start.elapsed()
        );
        assert_eq!(format!("{backoff:?}"), "DeclaredBackoff(virtual)");
    }

    /// …and the system clock — the status quo, the default of every run
    /// without a `run:` block — really waits.
    #[tokio::test]
    async fn the_system_clock_backoff_really_waits() {
        let backoff = RunSeams::of(None).backoff();
        let start = std::time::Instant::now();
        backoff.sleep(Duration::from_millis(30)).await;
        assert!(
            start.elapsed() >= Duration::from_millis(30),
            "the system clock sleeps for real, took {:?}",
            start.elapsed()
        );
        assert_eq!(format!("{backoff:?}"), "DeclaredBackoff(system)");
    }

    // ── F-P3 · the run: declaration resolves to its seams ────────────

    #[test]
    fn absent_run_block_resolves_to_the_status_quo() {
        for decl in [None, Some(&RunDecl::default())] {
            let seams = RunSeams::of(decl);
            assert_eq!(seams.jitter_seed, 0, "the zero stream, unchanged");
            assert!(
                seams.clock.as_virtual().is_none(),
                "the system clock, unchanged"
            );
            // The live stamper mints UUIDv7 (ADR-033) — the ambient lane.
            let mut stamper = seams.stamper();
            let (id, _) = stamper.next();
            assert_eq!(id.uuid.get_version_num(), 7, "ambient = UUIDv7");
        }
    }

    #[test]
    fn seeded_resolves_every_deterministic_seam() {
        let decl = RunDecl::new(Some(RunEntropy::Seeded(42)), None);
        let seams = RunSeams::of(Some(&decl));
        assert_eq!(seams.jitter_seed, 42, "the seed keys the jitter stream");
        assert!(
            seams.clock.as_virtual().is_some(),
            "deterministic entropy implies the virtual clock (durations may \
             not ride the wall clock when journals replay byte-identical)"
        );
        // The deterministic stamper is seq-keyed (id 1 · t 10ms) — replay
        // law: same stream in, same bytes out.
        let mut stamper = seams.stamper();
        let (id, ts) = stamper.next();
        assert_eq!(ts.unix_ms(), 10, "+10ms from zero");
        let mut twin = RunSeams::of(Some(&decl)).stamper();
        assert_eq!(twin.next().0, id, "two runs mint the same first id");
    }

    #[test]
    fn entropy_none_pins_the_zero_stream_and_virtual_clock() {
        let decl = RunDecl::new(Some(RunEntropy::None), None);
        let seams = RunSeams::of(Some(&decl));
        assert_eq!(seams.jitter_seed, 0, "none = the fixed zero stream");
        assert!(seams.clock.as_virtual().is_some());
        let mut stamper = seams.stamper();
        assert_eq!(stamper.next().1.unix_ms(), 10, "deterministic stamps");
    }

    #[test]
    fn an_explicit_virtual_clock_composes_under_ambient_entropy() {
        // The legal testing configuration (F-P3): virtual deadlines, live
        // event stamps — no determinism claim is made.
        let decl = RunDecl::new(None, Some(nika_schema::types::RunClock::Virtual));
        let seams = RunSeams::of(Some(&decl));
        assert!(seams.clock.as_virtual().is_some());
        let mut stamper = seams.stamper();
        assert_eq!(
            stamper.next().0.uuid.get_version_num(),
            7,
            "ambient entropy keeps the live stamper"
        );
    }
}
