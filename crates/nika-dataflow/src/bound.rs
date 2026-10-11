// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The bound a rehearsal room puts on the values a run builds. A rehearsal vouches for nothing a
//! run builds past what its room holds, so the room hands the run a [`ValueBound`]: the scopes
//! that evaluate the run's fields carry it, every string [`render`](crate::expr::render) builds
//! and every value [`render_json`](crate::expr::render_json) builds (a CEL list included) is
//! counted against it where it is built, and one over it is refused
//! ([`DataflowError::ValueBound`]). The first crossing is kept for the room to report. A run no
//! room starts carries no bound and builds as before.

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::DataflowError;

/// The most bytes one value a run builds may hold, and the first value that held more.
#[derive(Debug)]
#[non_exhaustive]
pub struct ValueBound {
    limit: u64,
    /// The size of the first value over `limit` (0: none yet).
    crossed: AtomicU64,
}

impl ValueBound {
    /// A bound of `limit` bytes per value.
    #[must_use]
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            crossed: AtomicU64::new(0),
        }
    }

    /// The most bytes one value may hold.
    #[must_use]
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// The size of the first value a run built over the bound, once one did.
    #[must_use]
    pub fn crossed(&self) -> Option<u64> {
        Some(self.crossed.load(Ordering::Acquire)).filter(|bytes| *bytes > 0)
    }

    /// `bytes` fit, or the value is refused and its size kept as the first crossing.
    fn admit(&self, bytes: u64) -> Result<(), DataflowError> {
        if bytes <= self.limit {
            return Ok(());
        }
        let _ = (self.crossed).compare_exchange(0, bytes, Ordering::AcqRel, Ordering::Acquire);
        Err(DataflowError::ValueBound {
            bytes,
            limit: self.limit,
        })
    }
}

/// Whether a value of `bytes` fits the scope's `bound`; any size fits no bound.
pub(crate) fn fits(bound: Option<&ValueBound>, bytes: usize) -> Result<(), DataflowError> {
    bound.map_or(Ok(()), |bound| {
        bound.admit(u64::try_from(bytes).unwrap_or(u64::MAX))
    })
}

/// The bytes `value` holds as compact JSON, counted without building its text.
pub(crate) fn json_size(value: &Value) -> usize {
    struct Count(usize);
    impl io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    let _ = serde_json::to_writer(&mut count, value);
    count.0
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests;
