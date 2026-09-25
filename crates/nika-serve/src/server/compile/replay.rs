// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The answer rounds of native authoring, kept by the server (S09).
//!
//! A fresh native round that leaves a native plan is kept here under an unpredictable token,
//! beside the exact input it answered; the plan never travels through the caller. A later
//! zero-call round presents the token and repeats that input. Bounded in entries and in bytes
//! per entry, owned by ONE bound server (a restart forgets every token), expiring on the
//! monotonic clock and never renewed by use. This is not a deduplication of paid work: a lost
//! first answer leaves no token, and a new fresh round may spend again.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::v2::{Input, TOKEN_HEX};

/// The most bytes one kept round holds: its input and its plan.
pub(super) const MAX_ENTRY_BYTES: usize = 2 * 1024 * 1024;

/// The kept rounds of one bound server.
pub(in crate::server) struct Replays {
    inner: Mutex<Inner>,
    capacity: usize,
    ttl: Duration,
}

#[derive(Default)]
struct Inner {
    entries: BTreeMap<String, Arc<Entry>>,
    /// Places held by fresh rounds still at work.
    reserved: usize,
}

/// One kept round: the exact input it answered and the plan it produced.
pub(super) struct Entry {
    expires: Instant,
    pub(super) input: Input,
    pub(super) plan: Value,
}

/// A place held for the round a fresh request may leave, taken before any provider call:
/// a request the store could not continue is refused before it spends.
pub(super) struct Reservation(Option<Arc<Replays>>);

impl Replays {
    pub(super) fn new(capacity: usize, ttl: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            capacity,
            ttl,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hold a place for one round, or `None` when every place is kept or held. Expired
    /// rounds are forgotten first: a later replay of their token refuses, never regenerates.
    pub(super) fn reserve(self: &Arc<Self>) -> Option<Reservation> {
        let mut inner = self.lock();
        let now = Instant::now();
        inner.entries.retain(|_, entry| entry.expires > now);
        if inner.entries.len() + inner.reserved >= self.capacity {
            return None;
        }
        inner.reserved += 1;
        Some(Reservation(Some(Arc::clone(self))))
    }

    /// The kept round a token names while it lives; its lifetime is never extended.
    pub(super) fn get(&self, token: &str) -> Option<Arc<Entry>> {
        let mut inner = self.lock();
        let entry = Arc::clone(inner.entries.get(token)?);
        if entry.expires > Instant::now() {
            return Some(entry);
        }
        inner.entries.remove(token);
        None
    }
}

#[cfg(test)]
impl Replays {
    /// Kept rounds and places held, as a test observes them.
    pub(in crate::server) fn held(&self) -> (usize, usize) {
        let inner = self.lock();
        (inner.entries.len(), inner.reserved)
    }
}

impl Reservation {
    /// Keep this round under a new token. `None` — no replay promise — when the round exceeds
    /// the per-entry bound or no randomness was available; the answer itself is unaffected.
    pub(super) fn keep(mut self, input: Input, plan: Value) -> Option<String> {
        let replays = self.0.take()?;
        let bytes = input.bytes() + plan.to_string().len();
        let token = (bytes <= MAX_ENTRY_BYTES).then(token).flatten();
        let mut inner = replays.lock();
        inner.reserved = inner.reserved.saturating_sub(1);
        let token = token?;
        let entry = Entry {
            expires: Instant::now() + replays.ttl,
            input,
            plan,
        };
        inner.entries.insert(token.clone(), Arc::new(entry));
        Some(token)
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(replays) = self.0.take() {
            let mut inner = replays.lock();
            inner.reserved = inner.reserved.saturating_sub(1);
        }
    }
}

/// 256 bits from the operating system, lowercase hex.
fn token() -> Option<String> {
    let mut bytes = [0_u8; TOKEN_HEX / 2];
    getrandom::fill(&mut bytes).ok()?;
    Some(
        bytes
            .iter()
            .fold(String::with_capacity(TOKEN_HEX), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            }),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn input(intent: &str) -> Input {
        Input::Create {
            intent: intent.to_owned(),
            workflow_id: None,
        }
    }

    #[test]
    fn a_kept_round_is_found_by_its_token_only_and_holds_its_place() {
        let replays = Replays::new(2, Duration::from_secs(60));
        let token = replays
            .reserve()
            .expect("a place")
            .keep(input("a"), serde_json::json!({"plan": 1}))
            .expect("kept");
        assert_eq!(token.len(), TOKEN_HEX);
        assert!(
            token
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(replays.get(&token).expect("found").input, input("a"));
        assert!(replays.get(&"0".repeat(TOKEN_HEX)).is_none());
        // One place kept, one held: the third request is refused before it spends.
        let held = replays.reserve().expect("the second place");
        assert!(replays.reserve().is_none());
        drop(held);
        assert!(
            replays.reserve().is_some(),
            "a dropped hold frees its place"
        );
    }

    #[test]
    fn an_expired_round_is_forgotten_and_frees_its_place() {
        let replays = Replays::new(1, Duration::from_millis(30));
        let token = replays
            .reserve()
            .expect("a place")
            .keep(input("a"), serde_json::json!({}))
            .expect("kept");
        assert!(replays.reserve().is_none());
        std::thread::sleep(Duration::from_millis(60));
        assert!(replays.get(&token).is_none(), "expired, never renewed");
        assert!(replays.reserve().is_some());
    }

    #[test]
    fn a_round_beyond_the_entry_bound_leaves_no_token_and_no_hold() {
        let replays = Replays::new(1, Duration::from_secs(60));
        let big = "x".repeat(MAX_ENTRY_BYTES);
        let kept = replays
            .reserve()
            .expect("a place")
            .keep(input(&big), serde_json::json!({}));
        assert!(kept.is_none());
        assert!(replays.reserve().is_some(), "the hold was released");
    }
}
