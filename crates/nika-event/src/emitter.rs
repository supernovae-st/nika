// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`Emitter`] — the sink trait for [`Event`]s, plus two L0 impls.
//!
//! The trait is **object-safe** (`Vec<Box<dyn Emitter>>` works) so a run
//! can fan events out to several sinks. Both L0 impls are pure (zero I/O,
//! zero async); an I/O-backed emitter (file, IPC, the future Connectome)
//! lives at a higher layer and reuses this exact contract.

use std::sync::Mutex;

use crate::error::EventError;
use crate::event::Event;

/// A sink that accepts emitted [`Event`]s.
///
/// `emit` returns `Result` so I/O-backed implementations (higher layers)
/// can surface failures; the L0 impls here only ever fail with
/// [`EventError::BufferFull`] (and [`NoOpEmitter`] never fails).
pub trait Emitter: Send + Sync {
    /// Record one event. Implementations MUST NOT panic.
    ///
    /// # Errors
    ///
    /// Returns [`EventError`] if the sink cannot accept the event (e.g. a
    /// bounded buffer is full, or a serializer rejected the payload).
    fn emit(&self, event: Event) -> Result<(), EventError>;
}

/// An emitter that drops every event. Zero-cost. Useful as a default and
/// in benchmarks where the sink should not be the bottleneck.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpEmitter;

impl Emitter for NoOpEmitter {
    fn emit(&self, _event: Event) -> Result<(), EventError> {
        Ok(())
    }
}

/// An emitter that collects events into an in-memory buffer.
///
/// Optionally bounded: once `capacity` events are buffered, further
/// [`emit`](Emitter::emit) calls fail with [`EventError::BufferFull`].
/// An unbounded buffer ([`InMemoryEmitter::unbounded`]) never fails.
///
/// Interior mutability via [`std::sync::Mutex`] keeps `emit(&self, …)`
/// object-safe while staying L0 (a mutex is not I/O).
#[derive(Debug, Default)]
pub struct InMemoryEmitter {
    buffer: Mutex<Vec<Event>>,
    capacity: Option<usize>,
}

impl InMemoryEmitter {
    /// An unbounded collector. `emit` never returns an error.
    ///
    /// Equivalent to [`Default::default`] (mutation-equivalent · the
    /// `cargo mutants` "replace with `Default::default()`" mutant is a
    /// true equivalent, not a test gap).
    #[must_use]
    pub fn unbounded() -> Self {
        Self {
            buffer: Mutex::new(Vec::new()),
            capacity: None,
        }
    }

    /// A bounded collector that refuses events past `capacity`.
    #[must_use]
    pub fn bounded(capacity: usize) -> Self {
        Self {
            buffer: Mutex::new(Vec::new()),
            capacity: Some(capacity),
        }
    }

    /// Number of buffered events.
    ///
    /// Returns `0` if the lock is poisoned (a prior panic while held) —
    /// L0 never panics in `src/`, so poisoning is a test-only concern and
    /// degrading to `0` keeps this accessor panic-free per the zero-unwrap
    /// gate.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.lock().map_or(0, |b| b.len())
    }

    /// Whether the buffer is empty (or the lock is poisoned).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drain all buffered events, leaving the buffer empty.
    ///
    /// Returns an empty `Vec` if the lock is poisoned (panic-free degrade).
    #[must_use]
    pub fn drain(&self) -> Vec<Event> {
        self.buffer
            .lock()
            .map_or_else(|_| Vec::new(), |mut b| std::mem::take(&mut *b))
    }
}

impl Emitter for InMemoryEmitter {
    fn emit(&self, event: Event) -> Result<(), EventError> {
        let Ok(mut buffer) = self.buffer.lock() else {
            // Poisoned lock: surface a distinct error rather than panicking
            // (zero-unwrap gate). NOT BufferFull — that would collide with a
            // legitimate `bounded(0)` emitter (rust-pro review LOW-2).
            return Err(EventError::LockPoisoned);
        };
        if let Some(cap) = self.capacity
            && buffer.len() >= cap
        {
            return Err(EventError::BufferFull { capacity: cap });
        }
        buffer.push(event);
        Ok(())
    }
}
