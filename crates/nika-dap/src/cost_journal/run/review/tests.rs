// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A review's custody over a real held lease: a live review holds it, and
//! every ending (decline, expiry, an interrupted or settled admission)
//! releases it; one approval admits once; the first verdict stands; keys
//! replay or conflict and leave with their review; a job's authority is
//! claimed once.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::super::super::{Cleared, clear};
use super::*;
use nika_fs::OwnedDir;

fn project() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn cleared(root: &std::path::Path) -> Cleared {
    let dir = OwnedDir::open(root).unwrap();
    clear(&dir, "review")
        .unwrap()
        .expect("a fresh project clears")
}

fn lease_free(root: &std::path::Path) -> bool {
    let dir = OwnedDir::open(root).unwrap();
    clear(&dir, "probe").unwrap().is_ok()
}

const W: &str = "0123456789abcdef";

fn pending(reviews: &mut Reviews<Cleared>, id: &str, root: &std::path::Path, now: Instant) {
    let view = json!({"state": "pending", "witness_sha256": W});
    let request = json!({"workflow": "w.nika"});
    reviews.insert(id.into(), (view, request), cleared(root), now);
}

#[test]
fn a_live_review_holds_the_lease_and_a_decline_releases_it() {
    let root = project();
    let now = Instant::now();
    let mut reviews = Reviews::default();
    pending(&mut reviews, "rev-1", root.path(), now);
    assert!(!lease_free(root.path()), "a pending review holds the lease");
    let forged = reviews.decide("rev-1", "ffff", true, (now, "t")).err();
    assert_eq!(forged, Some(ReviewRefusal::WitnessMismatch));
    let declined = reviews.decide("rev-1", W, false, (now, "t")).unwrap();
    assert_eq!(declined["state"], "declined");
    assert_eq!(declined["decided_at"], "t");
    assert!(lease_free(root.path()), "a decline released it");
    assert_eq!(
        reviews.decide("rev-1", W, false, (now, "t")).unwrap()["state"],
        "declined"
    );
    assert_eq!(
        reviews.decide("rev-1", W, true, (now, "t")).err(),
        Some(ReviewRefusal::Decided)
    );
    let request = json!({"workflow": "w.nika"});
    assert_eq!(
        reviews.take("rev-1", W, &request, now).err(),
        Some(ReviewRefusal::Declined)
    );
    assert_eq!(reviews.view("rev-x").err(), Some(ReviewRefusal::Unknown));
}

#[test]
fn one_approval_admits_once_and_the_first_verdict_stands() {
    let root = project();
    let now = Instant::now();
    let mut reviews = Reviews::default();
    pending(&mut reviews, "rev-1", root.path(), now);
    let request = json!({"workflow": "w.nika"});
    assert_eq!(
        reviews.take("rev-1", W, &request, now).err(),
        Some(ReviewRefusal::NotApproved)
    );
    reviews.decide("rev-1", W, true, (now, "t")).unwrap();
    let other = json!({"workflow": "w.nika", "access": "api"});
    assert_eq!(
        reviews.take("rev-1", W, &other, now).err(),
        Some(ReviewRefusal::RequestMismatch)
    );
    assert_eq!(
        reviews.view("rev-1").unwrap()["state"],
        "approved",
        "another request never spends it"
    );
    let held = reviews
        .take("rev-1", W, &request, now)
        .expect("the one winner");
    assert_eq!(
        reviews.take("rev-1", W, &request, now).err(),
        Some(ReviewRefusal::Busy)
    );
    drop(held);
    assert!(
        lease_free(root.path()),
        "the taken authority left with its admission"
    );
    let account: AccountView = Box::new(|| Some(json!({"attempts": 0})));
    reviews.settle("rev-1", "consumed", Some(("job-1", account)), None);
    reviews.settle(
        "rev-1",
        "failed",
        None,
        Some(("review_admission_failed", "late")),
    );
    let consumed = reviews.view("rev-1").unwrap();
    assert_eq!(consumed["state"], "consumed", "the first verdict stands");
    assert_eq!(consumed["job"]["id"], "job-1");
    assert_eq!(consumed["account"]["attempts"], 0, "the account reads live");
    assert_eq!(
        reviews.take("rev-1", W, &request, now).err(),
        Some(ReviewRefusal::Consumed)
    );
    assert_eq!(
        reviews.decide("rev-1", W, true, (now, "t")).unwrap()["state"],
        "consumed"
    );
}

#[test]
fn a_lifetime_ends_expired_and_releases_the_lease() {
    let root = project();
    let now = Instant::now();
    let mut reviews = Reviews::default();
    pending(&mut reviews, "rev-1", root.path(), now);
    reviews.decide("rev-1", W, true, (now, "t")).unwrap();
    let later = now + REVIEW_TTL;
    assert!(reviews.expired_at("rev-1", later));
    let request = json!({"workflow": "w.nika"});
    assert_eq!(
        reviews.take("rev-1", W, &request, later).err(),
        Some(ReviewRefusal::Expired)
    );
    assert_eq!(reviews.view("rev-1").unwrap()["state"], "expired");
    assert!(lease_free(root.path()), "expiry released the lease");
}

#[test]
fn keys_replay_conflict_and_leave_with_their_evicted_review() {
    let now = Instant::now();
    let mut reviews: Reviews<()> = Reviews::default();
    let view = json!({"state": "pending", "witness_sha256": W});
    reviews.insert("rev-0".into(), (view, json!({})), (), now);
    reviews.bind_key(
        "key-0".into(),
        "d0".into(),
        KeyAnswer::Review("rev-0".into()),
    );
    assert_eq!(
        reviews.replay("key-0", "d0"),
        Some(Replay::Same(KeyAnswer::Review("rev-0".into())))
    );
    assert_eq!(reviews.replay("key-0", "other"), Some(Replay::Conflict));
    assert_eq!(reviews.replay("absent", "d0"), None);
    reviews.decide("rev-0", W, false, (now, "t")).unwrap();
    for n in 1..=REVIEWS_RETAINED {
        let view = json!({"state": "pending", "witness_sha256": W});
        reviews.insert(format!("rev-{n}"), (view, json!({})), (), now);
        reviews
            .decide(&format!("rev-{n}"), W, false, (now, "t"))
            .unwrap();
    }
    assert_eq!(
        reviews.view("rev-0").err(),
        Some(ReviewRefusal::Unknown),
        "the oldest left"
    );
    assert_eq!(
        reviews.replay("key-0", "d0"),
        None,
        "and its key with it: a replay is a new review"
    );
    assert!(reviews.view(&format!("rev-{REVIEWS_RETAINED}")).is_ok());
}

#[test]
fn a_jobs_authority_is_claimed_once() {
    let claims: Claims<&str> = Claims::default();
    assert!(matches!(claims.claim("job-1"), Claim::Absent));
    claims.attach("job-1", "authority");
    assert!(matches!(claims.claim("job-1"), Claim::Owned("authority")));
    assert!(
        matches!(claims.claim("job-1"), Claim::Duplicate),
        "a second run never runs it"
    );
    claims.release("job-1");
    assert!(matches!(claims.claim("job-1"), Claim::Absent));
}

/// C6 · the witness digests exactly the nonce, then the binding: an
/// independent SHA-256 over those bytes agrees, and changing either one
/// changes the witness.
#[test]
fn a_witness_is_the_digest_of_the_nonce_then_the_binding() {
    use sha2::{Digest as _, Sha256};
    let nonce: [u8; 32] = Sha256::digest(uuid::Uuid::new_v4().as_bytes()).into();
    let binding = r#"[{"view":1},"candidate"]"#;
    let mut other_nonce = nonce;
    other_nonce[0] ^= 1;
    let digest = Sha256::new()
        .chain_update(nonce)
        .chain_update(binding.as_bytes())
        .finalize();
    let expected = digest.iter().fold(String::new(), |mut hex, byte| {
        use std::fmt::Write as _;
        write!(hex, "{byte:02x}").unwrap();
        hex
    });
    assert_eq!(review_witness(&nonce, binding), expected);
    assert_ne!(review_witness(&other_nonce, binding), expected);
    assert_ne!(review_witness(&nonce, "[]"), expected);
}
