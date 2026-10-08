// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A nested run's native stack (spec `14-composition.md` · the
//! `NIKA-SEC-003` bound is the only nesting limit). Same-thread stack
//! mechanics only: nothing here dispatches a task, folds a definition or
//! offers a product option.
//!
//! The runtime polls a child run INSIDE its parent's dispatch, on the
//! parent's thread: the borrowed, non-`Send` child future. Each nesting
//! level therefore stacks the run's whole poll chain once more on ONE
//! native stack, measured unoptimized (aarch64) at 934 KiB a level, most
//! of it the task pipeline's own poll frames: eight child edges needed
//! 8.4 MiB of main thread, and a root with one child the whole 2 MiB of a
//! test thread, before the depth gate could speak. Boxing does not change
//! that: the frames are the polls.
//!
//! [`grown`] owns one such future and creates, polls and destroys it with
//! at least the red zone (4 MiB) of stack: in place while the current
//! stack has that much left, otherwise on a fresh 8 MiB segment of the
//! SAME thread, released when the call returns. The future, its borrows,
//! its waker and its executor are unchanged. Nothing is spawned, and
//! dropping it destroys the run at once: a timeout, a dropped run or an
//! unwinding panic still owns the whole attempt tree.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// The stack a nested level may use before the next nesting boundary
/// looks again. One unoptimized level measured 934 KiB (aarch64), a root
/// run with its leaf about 1.1 MiB; four MiB keep a margin for heavier
/// leaves, other targets and growing frames.
const RED_ZONE: usize = 4 * 1024 * 1024;

/// A grown segment: the 8 MiB a CLI root run already has on the main
/// thread, so a grown level is never poorer than a root.
const SEGMENT: usize = 8 * 1024 * 1024;

type Run<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Create the future `create` returns, then poll and destroy it, each
/// phase with at least the red zone of stack on the calling thread. The
/// output, the waker and the drop order stay the future's own; a settled
/// run is destroyed before its output is returned.
pub fn grown<'a, T: 'a>(create: impl FnOnce() -> Run<'a, T>) -> impl Future<Output = T> + 'a {
    Grown {
        run: Some(on_grown_stack(create)),
    }
}

/// One nested run, owned from creation to destruction.
struct Grown<'a, T> {
    /// `None` once settled: fused, it never polls a finished run again.
    run: Option<Run<'a, T>>,
}

impl<T> Grown<'_, T> {
    /// Destroy the run on a guaranteed stack, settled or still pending
    /// (dropping a pending run unwinds every level below it).
    fn release(&mut self) {
        if let Some(run) = self.run.take() {
            on_grown_stack(move || drop(run));
        }
    }
}

impl<T> Future for Grown<'_, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let this = self.get_mut();
        let Some(run) = this.run.as_mut() else {
            return Poll::Pending;
        };
        let polled = on_grown_stack(|| run.as_mut().poll(cx));
        if polled.is_ready() {
            this.release();
        }
        polled
    }
}

impl<T> Drop for Grown<'_, T> {
    fn drop(&mut self) {
        self.release();
    }
}

/// Run `work` with at least [`RED_ZONE`] of stack: on the current stack
/// when it has that much left, otherwise on a fresh [`SEGMENT`] of this
/// same thread (`stacker`: a guard-paged mapping, unmapped on return; a
/// panic inside unwinds back onto the caller's stack).
fn on_grown_stack<R>(work: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(RED_ZONE, SEGMENT, work)
}
