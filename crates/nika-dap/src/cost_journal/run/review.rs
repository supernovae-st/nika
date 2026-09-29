// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A cost review's custody between its framing and its one admission, for a
//! host that answers across requests (Serve's cost-review door, C6): which
//! reviews are live, how each ended, which review-local keys name them, and so
//! when the authority a review holds (its [`super::Cleared`] lease inside the
//! host's `H`) is released. Descended from Serve at its 15k wall with its
//! words and codes unchanged. In memory only: a restart forgets everything and
//! nothing is ever re-granted.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// A review lives this long from creation (an approval never extends it).
pub const REVIEW_TTL: Duration = Duration::from_secs(300);
/// Terminal reviews (and their keys) retained for lookup, oldest first out.
pub const REVIEWS_RETAINED: usize = 256;

/// Why a review cannot move as asked; `code` and `message` are the wire words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReviewRefusal {
    /// No review by that id (never, restarted or evicted).
    Unknown,
    /// Its lifetime ended.
    Expired,
    /// The presented witness is not the review's.
    WitnessMismatch,
    /// It already holds another decision.
    Decided,
    /// No `approve_once` yet.
    NotApproved,
    /// It was declined.
    Declined,
    /// Another admission holds it now.
    Busy,
    /// One admission already used it.
    Consumed,
    /// The admission's request is not the reviewed one.
    RequestMismatch,
}

impl ReviewRefusal {
    /// The stable wire code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unknown => "review_unknown",
            Self::Expired => "review_expired",
            Self::WitnessMismatch => "review_witness_mismatch",
            Self::Decided => "review_decided",
            Self::NotApproved => "review_not_approved",
            Self::Declined => "review_declined",
            Self::Busy => "review_busy",
            Self::Consumed => "review_consumed",
            Self::RequestMismatch => "review_request_mismatch",
        }
    }

    /// The one-sentence teaching.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Unknown => {
                "no review by that id on this server run: reviews live in memory only, and a restart or eviction loses them (nothing is re-granted: request a fresh review)"
            }
            Self::Expired => {
                "the review expired (300 seconds from creation): request a fresh review"
            }
            Self::WitnessMismatch => "witness_sha256 is not this review's witness",
            Self::Decided => "this review already holds another decision",
            Self::NotApproved => "this review has no approve_once decision yet",
            Self::Declined => "this review was declined",
            Self::Busy => "another admission holds this review now",
            Self::Consumed => {
                "this review was already used by one admission: request a fresh review"
            }
            Self::RequestMismatch => {
                "this job's workflow, inputs or access are not the reviewed request"
            }
        }
    }
}

/// What a review-local key already answers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyAnswer {
    /// The review this key opened.
    Review(String),
    /// The no-review answer it received.
    NotRequired(Value),
}

/// A replayed key: the same request bytes, or other bytes (a conflict).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Replay {
    /// The key's first answer.
    Same(KeyAnswer),
    /// The key is bound to other request bytes.
    Conflict,
}

/// The live account a consumed review projects on each read (the host's).
pub type AccountView = Box<dyn Fn() -> Option<Value> + Send + Sync>;

struct Entry<H> {
    view: Value,
    request: Value,
    held: Option<H>,
    created: Instant,
    account: Option<AccountView>,
}

/// The reviews one server run holds, generic over what a live review holds.
pub struct Reviews<H> {
    entries: BTreeMap<String, Entry<H>>,
    retained: VecDeque<String>,
    keys: BTreeMap<String, (String, KeyAnswer)>,
    key_order: VecDeque<String>,
}

impl<H> Default for Reviews<H> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            retained: VecDeque::new(),
            keys: BTreeMap::new(),
            key_order: VecDeque::new(),
        }
    }
}

impl<H> std::fmt::Debug for Reviews<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reviews")
            .field("entries", &self.entries.len())
            .field("keys", &self.keys.len())
            .finish_non_exhaustive()
    }
}

impl<H> Reviews<H> {
    /// The answer `key` already gives for these request bytes (`digest`).
    #[must_use]
    pub fn replay(&self, key: &str, digest: &str) -> Option<Replay> {
        let (bound, answer) = self.keys.get(key)?;
        Some(if bound == digest {
            Replay::Same(answer.clone())
        } else {
            Replay::Conflict
        })
    }

    /// Bind a review-local key to its first answer (the oldest key leaves
    /// past the retention).
    pub fn bind_key(&mut self, key: String, digest: String, answer: KeyAnswer) {
        if self.keys.insert(key.clone(), (digest, answer)).is_none() {
            self.key_order.push_back(key);
        }
        while self.key_order.len() > REVIEWS_RETAINED {
            if let Some(old) = self.key_order.pop_front() {
                self.keys.remove(&old);
            }
        }
    }

    /// Hold one new pending review: its public document (with its
    /// `witness_sha256`), the request it frames, and what it holds.
    pub fn insert(
        &mut self,
        id: String,
        (view, request): (Value, Value),
        held: H,
        created: Instant,
    ) {
        let entry = Entry {
            view,
            request,
            held: Some(held),
            created,
            account: None,
        };
        self.entries.insert(id, entry);
    }

    /// End every live review whose lifetime is over (what it held goes too).
    pub fn sweep(&mut self, now: Instant) {
        let over: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.held.is_some() && expired(e.created, now))
            .map(|(id, _)| id.clone())
            .collect();
        for id in over {
            self.terminal(&id, "expired", None);
        }
    }

    /// The review's current document (a consumed one reads its account live).
    ///
    /// # Errors
    /// [`ReviewRefusal::Unknown`].
    pub fn view(&self, id: &str) -> Result<Value, ReviewRefusal> {
        let entry = self.entries.get(id).ok_or(ReviewRefusal::Unknown)?;
        let mut view = entry.view.clone();
        if let Some(account) = entry.account.as_ref().and_then(|read| read()) {
            view["account"] = account;
        }
        Ok(view)
    }

    /// Record one decision on a pending review, stamped `decided_at` (the
    /// host's wall clock); the same decision again reads it.
    ///
    /// # Errors
    /// Unknown, expired, a mismatched witness, or another decision.
    pub fn decide(
        &mut self,
        id: &str,
        witness: &str,
        approve: bool,
        (now, decided_at): (Instant, &str),
    ) -> Result<Value, ReviewRefusal> {
        self.check(id, witness, now)?;
        let word = if approve { "approved" } else { "declined" };
        let state = self.state(id);
        if state != "pending" {
            return if state == word || (approve && is_after_approval(&state)) {
                self.view(id)
            } else {
                Err(ReviewRefusal::Decided)
            };
        }
        if approve {
            if let Some(entry) = self.entries.get_mut(id) {
                entry.view["state"] = json!("approved");
            }
        } else {
            self.terminal(id, "declined", None);
        }
        if let Some(entry) = self.entries.get_mut(id) {
            entry.view["decided_at"] = json!(decided_at);
        }
        self.view(id)
    }

    /// Take an approved review for one admission (the single winner); every
    /// other state, and another request, refuses without moving it.
    ///
    /// # Errors
    /// Unknown, expired, a mismatched witness or request, or its state.
    pub fn take(
        &mut self,
        id: &str,
        witness: &str,
        request: &Value,
        now: Instant,
    ) -> Result<H, ReviewRefusal> {
        self.check(id, witness, now)?;
        let entry = self.entries.get_mut(id).ok_or(ReviewRefusal::Unknown)?;
        let refused = match entry.view["state"].as_str().unwrap_or_default() {
            "approved" => None,
            "pending" => Some(ReviewRefusal::NotApproved),
            "declined" => Some(ReviewRefusal::Declined),
            "admitting" => Some(ReviewRefusal::Busy),
            _ => Some(ReviewRefusal::Consumed),
        };
        if let Some(refused) = refused {
            return Err(refused);
        }
        if entry.request != *request {
            return Err(ReviewRefusal::RequestMismatch);
        }
        let held = entry.held.take().ok_or(ReviewRefusal::Unknown)?;
        entry.view["state"] = json!("admitting");
        Ok(held)
    }

    /// End an admission: `consumed` with its job and live account view, or a
    /// terminal state that created no job. A review already ended keeps its
    /// first verdict; nothing is ever refunded.
    pub fn settle(
        &mut self,
        id: &str,
        state: &str,
        job: Option<(&str, AccountView)>,
        refusal: Option<(&str, &str)>,
    ) {
        if !self.terminal(id, state, refusal) {
            return;
        }
        if let (Some(entry), Some((job, account))) = (self.entries.get_mut(id), job) {
            entry.view["job"] = json!({"id": job});
            entry.account = Some(account);
        }
    }

    /// Whether `id`'s lifetime is over at `now`.
    #[must_use]
    pub fn expired_at(&self, id: &str, now: Instant) -> bool {
        self.entries
            .get(id)
            .is_some_and(|e| expired(e.created, now))
    }

    fn check(&mut self, id: &str, witness: &str, now: Instant) -> Result<(), ReviewRefusal> {
        self.sweep(now);
        let entry = self.entries.get(id).ok_or(ReviewRefusal::Unknown)?;
        let own = entry.view["witness_sha256"].as_str().unwrap_or_default();
        if !constant_time_eq(own.as_bytes(), witness.as_bytes()) {
            return Err(ReviewRefusal::WitnessMismatch);
        }
        if entry.view["state"] == "expired" {
            return Err(ReviewRefusal::Expired);
        }
        Ok(())
    }

    fn state(&self, id: &str) -> String {
        self.entries
            .get(id)
            .and_then(|e| e.view["state"].as_str())
            .unwrap_or_default()
            .to_owned()
    }

    /// End a review: drop what it held, keep its document, and retain it among
    /// the newest terminal reviews. `false` when it had already ended.
    fn terminal(&mut self, id: &str, word: &str, refusal: Option<(&str, &str)>) -> bool {
        let Some(entry) = self.entries.get_mut(id) else {
            return false;
        };
        if !matches!(
            entry.view["state"].as_str(),
            Some("pending" | "approved" | "admitting")
        ) {
            return false;
        }
        entry.held = None;
        entry.view["state"] = json!(word);
        if let Some((code, message)) = refusal {
            entry.view["refusal"] = json!({"code": code, "message": message});
        }
        self.retained.push_back(id.to_owned());
        while self.retained.len() > REVIEWS_RETAINED {
            if let Some(evicted) = self.retained.pop_front() {
                self.entries.remove(&evicted);
                let bound = KeyAnswer::Review(evicted);
                self.keys.retain(|_, (_, answer)| *answer != bound);
                self.key_order.retain(|key| self.keys.contains_key(key));
            }
        }
        true
    }
}

/// A review's witness: the lowercase hex SHA-256 of the host's fresh private
/// `nonce` followed by the exact `binding` the host composed. The host draws
/// the nonce and composes the binding; this custody digests them and compares
/// a presented witness in constant time. A witness names one review's binding
/// and is never authenticated journal evidence.
#[must_use]
pub fn review_witness(nonce: &[u8; 32], binding: &str) -> String {
    nika_event::source_id::sha256_hex(&[nonce.as_slice(), binding.as_bytes()].concat())
}

/// Byte comparison whose time does not depend on where two equal-length
/// inputs first differ.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0_u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

fn expired(created: Instant, now: Instant) -> bool {
    now.checked_duration_since(created)
        .is_some_and(|age| age >= REVIEW_TTL)
}

fn is_after_approval(state: &str) -> bool {
    matches!(state, "admitting" | "consumed" | "refused" | "failed")
}

/// Admitted jobs' held authority until their run claims it (`None` once
/// claimed, until the run ends). Dropping an entry releases what it holds.
pub struct Claims<V>(std::sync::Mutex<BTreeMap<String, Option<V>>>);

/// What a job's run finds for its id.
#[derive(Debug)]
#[non_exhaustive]
pub enum Claim<V> {
    /// The authority its admission attached.
    Owned(V),
    /// Another run of this job already claimed it: this one is a duplicate.
    Duplicate,
    /// No authority (never attached, released, or lost to a restart).
    Absent,
}

impl<V> Default for Claims<V> {
    fn default() -> Self {
        Self(std::sync::Mutex::new(BTreeMap::new()))
    }
}

impl<V> std::fmt::Debug for Claims<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Claims").finish_non_exhaustive()
    }
}

impl<V> Claims<V> {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Option<V>>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Attach a job's authority before its task can run.
    pub fn attach(&self, job: &str, held: V) {
        self.lock().insert(job.to_owned(), Some(held));
    }

    /// The first run of `job` owns its authority; any other is a duplicate.
    pub fn claim(&self, job: &str) -> Claim<V> {
        match self.lock().get_mut(job) {
            Some(slot) => slot.take().map_or(Claim::Duplicate, Claim::Owned),
            None => Claim::Absent,
        }
    }

    /// Forget a job's authority (its run ended, or it ended queued).
    pub fn release(&self, job: &str) {
        let released = self.lock().remove(job);
        drop(released);
    }
}

#[cfg(test)]
mod tests;
