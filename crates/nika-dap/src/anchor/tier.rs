// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The SEALED / ANCHORED tier evaluations — the forensics half of
//! `nika trace verify`'s ladder (the rendering + exit mapping stays in
//! the CLI). Pure over their inputs: candidates are injected, the
//! sidecar arrives pre-located, and every failure is a reason string
//! the caller maps to its taxonomy — a forged seal or anchor is never
//! a softer finding than the forgery class.

use std::io::Cursor;

use super::{AnchorSidecar, pk32_of_box, sidecar_path, verify_offline};

/// A verified seal's attestation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealVerdict {
    /// The seal's `key_id` (the matched key's fingerprint).
    pub key_id: String,
    /// Where the matched key came from (the custody source string).
    pub source: String,
    /// The matched key's raw ed25519 half (the anchor tier reuses it).
    pub pk32: [u8; 32],
}

/// The seal ladder's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealTier {
    /// The journal's last line is not a `run_sealed` event.
    Unsealed,
    /// The signature verifies against a custody key.
    Sealed(SealVerdict),
    /// A seal-shaped line that fails ANY check — the forgery class.
    Forged(String),
}

/// The SEALED tier, pure over its inputs (tests inject candidates):
/// parse the terminal line, bind `covers` to the chain, match the
/// `key_id` to a candidate, and verify the minisign signature over
/// the proof layer's preimage.
#[must_use]
pub fn seal_tier(
    line: Option<&serde_json::Value>,
    events: usize,
    candidates: &[(String, String)],
) -> SealTier {
    let Some(line) = line else {
        return SealTier::Unsealed;
    };
    if line.get("kind").and_then(|k| k.as_str()) != Some("run_sealed") {
        return SealTier::Unsealed;
    }
    let field = |name: &str| -> Option<&serde_json::Value> {
        line.get("fields")?
            .as_array()?
            .iter()
            .find(|f| f.get("key").and_then(|k| k.as_str()) == Some(name))
            .and_then(|f| f.get("value"))
    };
    let string_field = |name: &str| field(name).and_then(|v| v.as_str());
    let malformed =
        |what: &str| SealTier::Forged(format!("the run_sealed line is malformed: {what}"));
    if field("seal_format").and_then(serde_json::Value::as_i64) != Some(1) {
        return malformed("seal_format is not 1");
    }
    if string_field("alg") != Some("ed25519") {
        return malformed("alg is not ed25519");
    }
    let (Some(covers_raw), Some(key_id), Some(sig)) = (
        string_field("covers"),
        string_field("key_id"),
        string_field("sig"),
    ) else {
        return malformed("covers · key_id · sig must all be present");
    };
    let covers: serde_json::Value = match serde_json::from_str(covers_raw) {
        Ok(covers) => covers,
        Err(_) => return malformed("covers is not a JSON object"),
    };
    // The seal must bind the chain it rides: covers.head IS the seal
    // line's own chain field, and covers.events the count before it.
    let chain = line.get("chain").and_then(|c| c.as_str()).unwrap_or("");
    if covers.get("head").and_then(|h| h.as_str()) != Some(chain) {
        return SealTier::Forged(
            "the seal's covers.head is not its chain field (a transplanted seal?)".to_owned(),
        );
    }
    let covered = u64::try_from(events.saturating_sub(1)).unwrap_or(0);
    if covers.get("events").and_then(serde_json::Value::as_u64) != Some(covered) {
        return SealTier::Forged(format!(
            "the seal covers {} events but the chain holds {events} complete lines",
            covers
                .get("events")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        ));
    }
    // The key_id picks the candidate — a mismatch across ALL of them
    // is the honest failure, never a pass.
    let matched = candidates
        .iter()
        .find(|(pk_box, _)| crate::seal::fingerprint(pk_box) == key_id);
    let Some((pk_box, source)) = matched else {
        return SealTier::Forged(format!(
            "the seal names key {} — no candidate matches (not --key, ~/.nika/keys/run-signing.pub, or the retired.pub ledger)",
            crate::escape_tty(key_id)
        ));
    };
    let Some(pk32) = pk32_of_box(pk_box) else {
        return SealTier::Forged(format!("the matched key from {source} does not parse"));
    };
    let verified = verify_seal_signature(&covers, sig, pk_box);
    if !verified {
        return SealTier::Forged(format!(
            "the run_sealed signature does not verify against key {} ({source})",
            crate::escape_tty(key_id)
        ));
    }
    SealTier::Sealed(SealVerdict {
        key_id: key_id.to_owned(),
        source: source.clone(),
        pk32,
    })
}

/// The minisign verification over the proof layer's ONE
/// canonicalization (the seal minting's own idiom, mirrored).
fn verify_seal_signature(covers: &serde_json::Value, sig: &str, pk_box: &str) -> bool {
    let preimage = nika_runtime::proof::preimage(nika_runtime::proof::HashDomain::Trace, 1, covers);
    let Ok(sig_box) = minisign::SignatureBox::from_string(sig) else {
        return false;
    };
    let Ok(pk) =
        minisign::PublicKeyBox::from_string(pk_box).and_then(minisign::PublicKey::from_box)
    else {
        return false;
    };
    minisign::verify(
        &pk,
        &sig_box,
        Cursor::new(preimage.as_bytes()),
        true,
        false,
        false,
    )
    .is_ok()
}

/// The anchor ladder's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorTier {
    /// No sidecar and none required.
    NotPresent,
    /// `--anchored` asked and the sidecar is absent (the ENV class).
    Required,
    /// Everything verified offline.
    Anchored(super::VerifiedAnchor),
    /// A present sidecar that fails any check (the forgery class).
    Gap(String),
}

/// The ANCHORED tier: load + verify the sidecar against the recomputed
/// head and the SEAL's key (the anchor must be the same key's — the
/// `verify_offline` contract).
#[must_use]
pub fn anchor_tier(
    trace: &str,
    head32: Option<[u8; 32]>,
    seal: &SealVerdict,
    required: bool,
) -> AnchorTier {
    let path = sidecar_path(trace);
    if !path.exists() {
        return if required {
            AnchorTier::Required
        } else {
            AnchorTier::NotPresent
        };
    }
    let sidecar: AnchorSidecar = match super::load_sidecar(&path) {
        Ok(sidecar) => sidecar,
        Err(e) => return AnchorTier::Gap(e),
    };
    let Some(head32) = head32 else {
        return AnchorTier::Gap(
            "the chain head is not 64 lowercase-hex chars — the forensics library is newer than this CLI"
                .to_owned(),
        );
    };
    match verify_offline(&sidecar, &head32, &seal.pk32, &seal.key_id) {
        Ok(verified) => AnchorTier::Anchored(verified),
        Err(reason) => AnchorTier::Gap(reason),
    }
}

/// The last COMPLETE journal line as parsed JSON (`events` counts the
/// verified lines — the seal candidate is the last of them; a torn
/// tail is excluded by construction). The line parsed once during the
/// walk; the tier parses it again — a malformed one simply has no
/// seal to offer.
#[must_use]
pub fn last_complete_line(raw: &str, events: usize) -> Option<serde_json::Value> {
    let line = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .nth(events.saturating_sub(1))?;
    serde_json::from_str(line).ok()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::chain::CHAIN_GENESIS;
    use nika_event::source_id::sha256_hex;

    /// A fresh minisign keypair (the custody test idiom — the box is
    /// trimmed exactly as the loaders hand it back).
    fn keypair() -> (String, minisign::SecretKey) {
        let pair =
            minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair");
        (
            pair.pk
                .to_box()
                .expect("pk box")
                .to_string()
                .trim()
                .to_owned(),
            pair.sk,
        )
    }

    /// A chained journal sealed with the given key — the seal line is
    /// the terminal event, chained like every other.
    fn sealed_journal(kinds: &[&str], sk: &minisign::SecretKey, pk_box: &str) -> String {
        let mut chain = sha256_hex(CHAIN_GENESIS);
        let mut out = String::new();
        for kind in kinds {
            let mut v = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
                "timestamp": 1000, "kind": kind, "run": null,
                "correlation": null, "fields": []
            });
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = sha256_hex(line.as_bytes());
            out.push_str(&line);
            out.push('\n');
        }
        let seal = crate::seal::seal_event(
            nika_types::id::EventId::generate(),
            nika_types::timestamp::Timestamp::from_unix_ms(1_700_000_000_000),
            &chain,
            kinds.len(),
            "wf-hash-test",
            "0.105.0-test",
            sk,
            pk_box,
        )
        .expect("the seal mints");
        let mut v = serde_json::to_value(&seal).expect("seal json");
        v["chain"] = serde_json::Value::String(chain);
        out.push_str(&serde_json::to_string(&v).expect("seal line"));
        out.push('\n');
        out
    }

    /// The pure tier: a good seal attains Sealed against its own
    /// candidate; every forgery class is named.
    #[test]
    fn the_seal_tier_verifies_and_names_every_forgery() {
        let (pk_box, sk) = keypair();
        let journal = sealed_journal(&["workflow_started", "task_completed"], &sk, &pk_box);
        let events = 3;
        let line = last_complete_line(&journal, events);
        let candidates = vec![(pk_box.clone(), "test".to_owned())];
        let SealTier::Sealed(verdict) = seal_tier(line.as_ref(), events, &candidates) else {
            unreachable!("a good seal attains Sealed")
        };
        assert_eq!(verdict.key_id, crate::seal::fingerprint(&pk_box));
        assert_eq!(verdict.source, "test");

        // No seal line at all → Unsealed.
        let plain = "{\"kind\":\"workflow_completed\",\"chain\":\"x\"}";
        let plain_json: serde_json::Value = serde_json::from_str(plain).expect("json");
        assert!(matches!(
            seal_tier(Some(&plain_json), 1, &candidates),
            SealTier::Unsealed
        ));
        // A tampered signature → Forged.
        let forged = journal.replace("trusted comment", "truzted comment");
        let line = last_complete_line(&forged, events);
        assert!(matches!(
            seal_tier(line.as_ref(), events, &candidates),
            SealTier::Forged(_)
        ));
        // A key-id mismatch across ALL candidates → Forged.
        let (other_box, _) = keypair();
        let strangers = vec![(other_box, "stranger".to_owned())];
        let line = last_complete_line(&journal, events);
        let SealTier::Forged(reason) = seal_tier(line.as_ref(), events, &strangers) else {
            unreachable!("a key-id mismatch never passes")
        };
        assert!(reason.contains("no candidate matches"), "{reason}");
    }

    /// NEP-0012 law 2 · the OSC52 class: an artifact-originated `key_id`
    /// (the journal is untrusted input) reaches the user-facing reason
    /// ESCAPED AT BIRTH — the escape lives where the string is born, so
    /// no render layer can forget it.
    #[test]
    fn a_forged_key_id_reaches_the_reason_escaped() {
        let line = serde_json::json!({
            "kind": "run_sealed",
            "chain": "abc",
            "fields": [
                {"key": "seal_format", "value": 1},
                {"key": "alg", "value": "ed25519"},
                {"key": "covers", "value": "{\"head\":\"abc\",\"events\":0}"},
                {"key": "key_id", "value": "\u{1b}]52;;Y2xpcA==\u{7}deadbeef"},
                {"key": "sig", "value": "untrusted-sig"}
            ]
        });
        let SealTier::Forged(reason) = seal_tier(Some(&line), 1, &[]) else {
            unreachable!("a key with no candidate is the honest failure")
        };
        assert!(
            !reason.chars().any(char::is_control),
            "the reason is escaped at birth: {reason:?}"
        );
        assert!(reason.contains("52;;Y2xpcA==deadbeef"), "{reason}");
    }

    /// A seal lifted onto a journal it did not mint (covers.head no
    /// longer matches the chain field) is forged, not sealed.
    #[test]
    fn a_transplanted_seal_is_forged_at_the_tier() {
        let (pk_box, sk) = keypair();
        let journal = sealed_journal(&["workflow_started", "task_completed"], &sk, &pk_box);
        let raws: Vec<&str> = journal.lines().collect();
        let mut first: serde_json::Value = serde_json::from_str(raws[0]).expect("first");
        first["kind"] = serde_json::Value::String("workflow_started_x".to_owned());
        let first_raw = serde_json::to_string(&first).expect("first raw");
        let new_head = sha256_hex(first_raw.as_bytes());
        let mut second: serde_json::Value = serde_json::from_str(raws[1]).expect("second");
        second["chain"] = serde_json::Value::String(new_head.clone());
        let second_raw = serde_json::to_string(&second).expect("second raw");
        let seal_head = sha256_hex(second_raw.as_bytes());
        let mut seal: serde_json::Value = serde_json::from_str(raws[2]).expect("seal");
        seal["chain"] = serde_json::Value::String(seal_head);
        let forged = format!(
            "{first_raw}\n{second_raw}\n{}\n",
            serde_json::to_string(&seal).expect("seal raw")
        );
        let line = last_complete_line(&forged, 3);
        let candidates = vec![(pk_box, "test".to_owned())];
        let SealTier::Forged(reason) = seal_tier(line.as_ref(), 3, &candidates) else {
            unreachable!("a transplanted seal is forged")
        };
        assert!(
            reason.contains("covers.head is not its chain field"),
            "{reason}"
        );
    }

    /// The frozen LIVE fixture at tier level: the sidecar verifies
    /// against the seal's key — the anchor tier's full ladder, offline.
    #[test]
    fn the_frozen_fixture_climbs_the_tiers_offline() {
        use crate::anchor::fixtures;
        let sk_box = fixtures::SECRET_BOX;
        let sk = minisign::SecretKeyBox::from_string(sk_box)
            .and_then(|b| b.into_secret_key(Some(String::new())))
            .expect("the fixture key opens");
        let journal = fixtures::JOURNAL;
        let events = 6;
        let line = last_complete_line(journal, events);
        let candidates = vec![(fixtures::PUBLIC_BOX.trim().to_owned(), "fixture".to_owned())];
        let SealTier::Sealed(verdict) = seal_tier(line.as_ref(), events, &candidates) else {
            unreachable!("the frozen journal's seal verifies")
        };
        let _ = sk; // the box pair round-trips through custody parsing
        let sidecar = fixtures::sidecar();
        let verified = crate::anchor::verify_offline(
            &sidecar,
            &fixtures::head32(),
            &verdict.pk32,
            &verdict.key_id,
        )
        .expect("the frozen anchor verifies against the seal's key");
        assert_eq!(verified.log_index, "34612959");
    }
}

// ── The ladder EVALUATION (descended from nika-cli 2026-07-21) ──────

/// The evaluated ladder, pre-render: every leg's verdict, the highest
/// honestly-attained tier, the exit class, and the per-tier report
/// lines. The lines are DATA (what the proof honestly says); the
/// CLI's `VerbOutput` envelope + exit code are the render. Descended
/// from `verbs::trace_verify::tiered` (the 15k wall — compute
/// descends, render stays).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierReport {
    /// The seal leg's verdict.
    pub seal: SealTier,
    /// The anchor leg's verdict.
    pub anchor: AnchorTier,
    /// The replay leg's verdict.
    pub replay: ReplayTier,
    /// The highest honestly-attained tier.
    pub attained: AttainedTier,
    /// The exit class the verdicts map to.
    pub exit: TierExit,
    /// The per-tier report lines, in ladder order (the chain's OK
    /// line is NOT among them — it predates the ladder and stays the
    /// CLI's byte-locked surface).
    pub lines: Vec<String>,
}

/// The highest honestly-attained tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttainedTier {
    /// Chain intact, nothing more proven.
    Ok,
    /// The `run_sealed` signature verifies.
    Sealed,
    /// The external anchor verifies offline.
    Anchored,
    /// The journal re-executes identically.
    Replayed,
}

/// The exit class the verdicts map to (the house taxonomy — the CLI
/// carries the numeric codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TierExit {
    /// The reported tier holds.
    Ok,
    /// A broken or forged claim (FILE).
    File,
    /// A missing input or unchained journal (ENV).
    Env,
}

/// The replay leg's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayTier {
    /// `--replay` was not passed.
    NotAsked,
    /// The journal re-executes identically.
    Replayed,
    /// The fresh run diverges (the reproduce report text rides).
    Diverged(String),
    /// The reproduce path cannot run for this journal — stated
    /// honestly, never faked.
    NotAttempted(String),
}

/// The neutral reproduce outcome the CLI injects (its shim owns the
/// file plumbing + the `VerbOutput`-class mapping; the ladder owns
/// what the outcome MEANS).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayCompare {
    /// Every comparable task reproduced.
    Reproduced,
    /// Any divergence (the reproduce report text).
    Diverged(String),
    /// The comparison cannot run (the honest why).
    CannotRun(String),
}

/// The tier evaluation: seal leg → anchor leg → replay leg, then the
/// attained tier + exit class + report lines. `torn` is carried only
/// for the OK-line decision, which stays the CLI's.
#[must_use]
pub fn evaluate(
    trace: &str,
    raw: &str,
    events: usize,
    head: &str,
    require_anchor: bool,
    replay: Option<&ReplayCompare>,
    candidates: &[(String, String)],
) -> TierReport {
    let mut lines = Vec::new();
    let seal = seal_tier(last_complete_line(raw, events).as_ref(), events, candidates);
    let verdict = match &seal {
        SealTier::Unsealed => {
            return TierReport {
                seal,
                anchor: AnchorTier::NotPresent,
                replay: ReplayTier::NotAsked,
                attained: AttainedTier::Ok,
                exit: TierExit::Ok,
                lines,
            };
        }
        SealTier::Forged(reason) => {
            lines.push(format!("SEAL FORGED — {reason}"));
            return TierReport {
                seal,
                anchor: AnchorTier::NotPresent,
                replay: ReplayTier::NotAsked,
                attained: AttainedTier::Ok,
                exit: TierExit::File,
                lines,
            };
        }
        SealTier::Sealed(v) => {
            lines.push(format!(
                "SEALED — the run_sealed signature verifies · key {} ({})",
                v.key_id, v.source
            ));
            v.clone()
        }
    };
    let Some((anchor, attained, mut exit)) =
        anchor_leg(trace, head, &verdict, require_anchor, &mut lines)
    else {
        return TierReport {
            seal,
            anchor: AnchorTier::Required,
            replay: ReplayTier::NotAsked,
            attained: AttainedTier::Sealed,
            exit: TierExit::Env,
            lines,
        };
    };
    let replay = match replay {
        Some(ReplayCompare::Reproduced) => {
            lines.push("REPLAYED — the journal re-executes identically".to_owned());
            ReplayTier::Replayed
        }
        Some(ReplayCompare::Diverged(text)) => {
            lines
                .push("REPLAY DIVERGED — the fresh run does not reproduce this journal".to_owned());
            lines.push(text.clone());
            exit = TierExit::File;
            ReplayTier::Diverged(text.clone())
        }
        Some(ReplayCompare::CannotRun(why)) => {
            lines.push(format!("REPLAYED — not attempted: {why}"));
            ReplayTier::NotAttempted(why.clone())
        }
        None => {
            lines.push(
                "REPLAYED — not attempted (pass --replay <fresh.ndjson> — verify never re-executes)"
                    .to_owned(),
            );
            ReplayTier::NotAsked
        }
    };
    let attained = if matches!(&replay, ReplayTier::Replayed) && exit == TierExit::Ok {
        AttainedTier::Replayed
    } else {
        attained
    };
    TierReport {
        seal,
        anchor,
        replay,
        attained,
        exit,
        lines,
    }
}

/// The anchor leg: verify the sidecar offline and voice the outcome —
/// `None` when `--anchored` was REQUIRED and the sidecar is absent
/// (the caller's early-ENV return).
fn anchor_leg(
    trace: &str,
    head: &str,
    verdict: &SealVerdict,
    require_anchor: bool,
    lines: &mut Vec<String>,
) -> Option<(AnchorTier, AttainedTier, TierExit)> {
    let anchor = anchor_tier(trace, super::hex_decode(head), verdict, require_anchor);
    let (attained, exit) = match &anchor {
        AnchorTier::Anchored(verified) => {
            lines.push(format!(
                "ANCHORED — rekor index {} · checkpoint + inclusion proof verified offline\n  rfc3161 gen_time {} (the trusted time)",
                crate::escape_tty(&verified.log_index),
                crate::escape_tty(&verified.gen_time)
            ));
            (AttainedTier::Anchored, TierExit::Ok)
        }
        AnchorTier::NotPresent => {
            lines.push(
                "ANCHORED — no sidecar (`nika trace anchor` notarizes the head outside the journal)"
                    .to_owned(),
            );
            (AttainedTier::Sealed, TierExit::Ok)
        }
        AnchorTier::Required => {
            lines.push("ANCHORED — REQUIRED but no <trace>.anchor.json sidecar exists".to_owned());
            return None;
        }
        AnchorTier::Gap(reason) => {
            lines.push(format!("ANCHOR FORGED — {reason}"));
            lines.push("  reported tier: SEALED (the anchor vouches for nothing)".to_owned());
            (AttainedTier::Sealed, TierExit::File)
        }
    };
    Some((anchor, attained, exit))
}
