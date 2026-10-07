// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The answer rounds of native authoring, kept by the server (S09).
//!
//! A fresh native round that leaves a native plan is kept here under an unpredictable token,
//! beside the exact input it answered; the plan never travels through the caller. A later
//! zero-call round presents the token and repeats that input. No implicit entry, byte or
//! lifetime quota: the whole input and plan are kept by ONE bound server (a restart forgets
//! every token). An operator may explicitly limit entries and lifetime; an explicit expiry
//! uses the monotonic clock and is never renewed by use. This is not a deduplication of paid
//! work: a lost first answer leaves no token, and a new fresh round may spend again.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::v2::{Input, TOKEN_HEX};

/// The kept rounds of one bound server.
pub(in crate::server) struct Replays {
    inner: Mutex<Inner>,
    capacity: Option<usize>,
    ttl: Option<Duration>,
}

#[derive(Default)]
struct Inner {
    entries: BTreeMap<String, Arc<Entry>>,
    /// Places held by fresh rounds still at work.
    reserved: usize,
}

/// One kept round: the exact input it answered and the plan it produced.
pub(super) struct Entry {
    kept_at: Instant,
    pub(super) input: Input,
    pub(super) plan: Value,
}

impl Entry {
    fn live_at(&self, ttl: Option<Duration>, now: Instant) -> bool {
        ttl.is_none_or(|ttl| now.saturating_duration_since(self.kept_at) < ttl)
    }
}

/// A place held for the round a fresh request may leave, taken before any provider call:
/// a request the store could not continue is refused before it spends.
pub(super) struct Reservation(Option<Arc<Replays>>);

impl Replays {
    pub(super) fn new(capacity: Option<usize>, ttl: Option<Duration>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            capacity,
            ttl,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hold a place for one round, or `None` when an explicit capacity is full. Rounds past
    /// an explicit lifetime are forgotten first: a later replay refuses, never regenerates.
    pub(super) fn reserve(self: &Arc<Self>) -> Option<Reservation> {
        let mut inner = self.lock();
        if self.ttl.is_some() {
            let now = Instant::now();
            inner
                .entries
                .retain(|_, entry| entry.live_at(self.ttl, now));
        }
        if self
            .capacity
            .is_some_and(|capacity| inner.entries.len().saturating_add(inner.reserved) >= capacity)
        {
            return None;
        }
        inner.reserved = inner.reserved.checked_add(1)?;
        Some(Reservation(Some(Arc::clone(self))))
    }

    /// The kept round a token names while it lives; its lifetime is never extended.
    /// Forget the round `token` names: a candidate its judge did not accept is never replayed
    /// to that judge again through it.
    pub(super) fn forget(&self, token: &str) {
        self.lock().entries.remove(token);
    }

    pub(super) fn get(&self, token: &str) -> Option<Arc<Entry>> {
        let mut inner = self.lock();
        let entry = Arc::clone(inner.entries.get(token)?);
        if entry.live_at(self.ttl, Instant::now()) {
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
    /// Keep this whole round under a new token. `None` — no replay promise — only when no
    /// randomness was available; the answer itself is unaffected.
    pub(super) fn keep(mut self, input: Input, plan: Value) -> Option<String> {
        let replays = self.0.take()?;
        let token = token();
        let mut inner = replays.lock();
        inner.reserved = inner.reserved.saturating_sub(1);
        let token = token?;
        let entry = Entry {
            kept_at: Instant::now(),
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
        let replays = Replays::new(Some(2), Some(Duration::from_secs(60)));
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
        let replays = Replays::new(Some(1), Some(Duration::from_millis(30)));
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
    fn a_round_beyond_two_mib_keeps_its_whole_input_and_plan() {
        let replays = defaults();
        let big = "x".repeat(2 * 1024 * 1024 + 1);
        let plan = serde_json::json!({"large": big, "last": "end of plan"});
        let token = replays
            .reserve()
            .expect("a place")
            .keep(input(&big), plan.clone())
            .expect("the whole round is kept");
        let kept = replays.get(&token).expect("the token resolves");
        assert_eq!(kept.input, input(&big));
        assert_eq!(kept.plan, plan);
        assert_eq!(replays.held(), (1, 0));
    }

    fn defaults() -> Arc<Replays> {
        let config = super::super::native::NativeAuthoring::new(
            "vllm/fixture",
            nika_providers::ProvidersConfig::new(),
        );
        super::super::native::Seat::open(&config)
            .expect("the default seat opens without provider calls")
            .replays
    }

    #[test]
    fn default_retention_keeps_more_than_thirty_two_rounds_without_expiry() {
        let replays = defaults();
        let mut tokens = Vec::new();
        for index in 0..65 {
            tokens.push(
                replays
                    .reserve()
                    .expect("a place without an implicit count")
                    .keep(
                        input(&format!("request {index}")),
                        serde_json::json!({"index": index}),
                    )
                    .expect("kept"),
            );
        }
        // Model time beyond both former TTLs without a wall-clock wait or renewing a token.
        let later = Instant::now() + Duration::from_secs(2 * 24 * 3600);
        let held = replays.reserve().expect("no implicit expiry or count");
        assert_eq!(replays.held(), (65, 1));
        for (index, token) in tokens.iter().enumerate() {
            let kept = replays.get(token).expect("every round still resolves");
            assert!(kept.live_at(replays.ttl, later), "no default expiry");
            assert_eq!(kept.input, input(&format!("request {index}")));
            assert_eq!(kept.plan, serde_json::json!({"index": index}));
        }
        drop(held);
        assert_eq!(replays.held(), (65, 0));
    }
}
