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
    /// A `run_sealed` frame exists but is NOT the journal's last complete
    /// line: lines were chained AFTER the seal. The seal is emitted as a
    /// journal's last line and covers every line before it (see
    /// [`crate::seal`]), so anything after it is outside everything it
    /// attests — and re-chaining needs only write access, never the key.
    /// The append-after-seal forgery class, never a crash (a killed run
    /// leaves no seal at all).
    Buried {
        /// FILE line number (1-based) of the buried `run_sealed` frame.
        line: usize,
        /// How many complete lines are chained after it.
        trailing: usize,
    },
    /// A seal whose `key_id` matches NO candidate: the signature is not
    /// judged at all. An absent key is a missing input (the ENV class),
    /// never evidence of forgery — the honest « I cannot attribute this ».
    Unattributable(String),
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
        // The 2026-07-30 adversarial pass: this used to be FORGED (exit
        // FILE) — an intact, genuinely sealed journal verified WITHOUT its
        // public key was told « SEAL FORGED » on no evidence whatsoever.
        // Forgery is a CLAIM and needs proof: a key that is not in custody
        // proves nothing about the signature, it only means the verifier
        // cannot judge it (the third-party case the transparency artifact
        // exists for). Missing input → the ENV class, non-zero so a gate
        // still fails closed, and never an accusation.
        return SealTier::Unattributable(format!(
            "the seal names key {} — no candidate carries it (not --key, ~/.nika/keys/run-signing.pub, or the retired.pub ledger)",
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

/// Is a `run_sealed` frame BURIED — present among the verified lines but
/// not the last of them? Returns `(FILE line, trailing complete lines)`.
///
/// The seal is emitted as a journal's last line ([`crate::seal`]) and its
/// `covers` binds the head + count BEFORE it, so a sealed journal with
/// lines after its seal is tampered: the trailing lines are outside
/// everything the signature attests, and re-chaining them needs only
/// write access — the very move the SEALED tier exists to catch.
///
/// `events` is the walk's VERIFIED line count, so a torn tail is already
/// excluded: a crash mid-write right after sealing leaves the seal as the
/// last COMPLETE line and is not buried (a crash is not a forgery).
fn buried_seal(raw: &str, events: usize) -> Option<(usize, usize)> {
    let verified: Vec<(usize, &str)> = raw
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .take(events)
        .collect();
    let (pos, (lineno, _)) = verified.iter().enumerate().find(|(_, (_, line))| {
        serde_json::from_str::<serde_json::Value>(line)
            .is_ok_and(|v| v.get("kind").and_then(|k| k.as_str()) == Some("run_sealed"))
    })?;
    let trailing = verified.len().saturating_sub(pos + 1);
    (trailing > 0).then_some((lineno + 1, trailing))
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
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
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

    /// The audit's anchor-contract arm (2026-07-29 · run 2): `--anchored`
    /// on an UNSEALED journal must REFUSE (ENV — the tier needs a seal to
    /// build on, so it is unattainable), never exit Ok with the
    /// requirement unevaluated. The unflagged path stays the quiet Ok it
    /// always was.
    #[test]
    fn an_unsealed_journal_refuses_a_required_anchor_loudly() {
        let (pk_box, _) = keypair();
        let journal = sealed_journal(
            &["workflow_started", "task_completed"],
            &keypair().1,
            &pk_box,
        );
        // Strip the seal line: an intact-chain, unsealed journal.
        let unsealed: String = journal.lines().take(2).fold(String::new(), |mut s, l| {
            use std::fmt::Write as _;
            let _ = writeln!(s, "{l}");
            s
        });
        let report = evaluate(
            "unsealed.ndjson",
            &unsealed,
            2,
            &"0".repeat(64),
            true, // --anchored
            false,
            None,
            &[],
        );
        assert_eq!(report.exit, TierExit::Env, "the requirement refuses");
        assert!(
            report
                .lines
                .iter()
                .any(|l| l.contains("REQUIRED") && l.contains("unsealed")),
            "the refusal names why: {:?}",
            report.lines
        );
        // The unflagged path: same journal, no requirement → quiet Ok.
        let report = evaluate(
            "unsealed.ndjson",
            &unsealed,
            2,
            &"0".repeat(64),
            false,
            false,
            None,
            &[],
        );
        assert_eq!(report.exit, TierExit::Ok);
        // nika#1384 · quiet no more: the absent seal names itself.
        assert_eq!(report.lines.len(), 1, "{:?}", report.lines);
        assert!(
            report.lines[0].starts_with("UNSEALED"),
            "{:?}",
            report.lines
        );
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
        // A key-id mismatch across ALL candidates → UNATTRIBUTABLE (never
        // Forged: not having the key proves nothing about the signature).
        let (other_box, _) = keypair();
        let strangers = vec![(other_box, "stranger".to_owned())];
        let line = last_complete_line(&journal, events);
        let SealTier::Unattributable(reason) = seal_tier(line.as_ref(), events, &strangers) else {
            unreachable!("a key-id mismatch never passes as Sealed, and is not forgery")
        };
        assert!(reason.contains("no candidate carries it"), "{reason}");
    }

    /// The 2026-07-30 adversarial pass · APPEND-AFTER-SEAL: a valid seal
    /// with lines chained after it is the forgery class (exit FILE), and
    /// the report names WHERE the seal is + HOW MANY lines follow. The
    /// mutation that proves the test: the same journal WITHOUT the
    /// appended line attains SEALED.
    #[test]
    fn a_buried_seal_is_the_append_forgery_never_a_silence() {
        let (pk_box, sk) = keypair();
        let journal = sealed_journal(&["workflow_started", "workflow_completed"], &sk, &pk_box);
        let candidates = vec![(pk_box.clone(), "test".to_owned())];

        // The control arm (the mutation): unappended → SEALED.
        let clean = evaluate(
            "t.ndjson",
            &journal,
            3,
            &"0".repeat(64),
            false,
            false,
            None,
            &candidates,
        );
        assert!(
            matches!(clean.seal, SealTier::Sealed(_)),
            "the unappended journal seals: {:?}",
            clean.seal
        );

        // One line chained AFTER the seal — write access only, no key.
        let seal_line = journal.lines().last().expect("the seal line");
        let appended = serde_json::json!({
            "id": {"uuid": "01912345-0000-7000-8000-0000000000ff"},
            "timestamp": 9999, "kind": "task_completed", "run": null,
            "correlation": null, "fields": [{"key": "task", "value": "appended"}],
            "chain": sha256_hex(seal_line.as_bytes()),
        });
        let forged = format!(
            "{journal}{}\n",
            serde_json::to_string(&appended).expect("appended line")
        );

        let report = evaluate(
            "t.ndjson",
            &forged,
            4,
            &"0".repeat(64),
            false,
            false,
            None,
            &candidates,
        );
        assert_eq!(
            report.seal,
            SealTier::Buried {
                line: 3,
                trailing: 1
            },
            "the buried seal is located + counted: {:?}",
            report.seal
        );
        assert_eq!(report.exit, TierExit::File, "the forgery class");
        assert_eq!(
            report.attained,
            AttainedTier::Ok,
            "nothing above chain-intact is claimed"
        );
        let said = report.lines.join("\n");
        assert!(said.contains("SEAL BURIED"), "{said}");
        assert!(
            said.contains("line 3") && said.contains("1 line(s)"),
            "the report names where + how many: {said}"
        );
    }

    /// The distinction the class must never lose: a journal TORN mid-write
    /// right after sealing is a crash, not an append — the walk excludes
    /// the torn line from `events`, so the seal is still the last COMPLETE
    /// line and the ladder climbs normally.
    #[test]
    fn a_torn_tail_after_the_seal_is_a_crash_not_a_buried_seal() {
        let (pk_box, sk) = keypair();
        let journal = sealed_journal(&["workflow_started", "workflow_completed"], &sk, &pk_box);
        let torn = format!("{journal}{{\"id\":{{\"uuid\":\"0191");
        let candidates = vec![(pk_box, "test".to_owned())];
        // events = 3: the walk's VERIFIED count excludes the torn line.
        let report = evaluate(
            "t.ndjson",
            &torn,
            3,
            &"0".repeat(64),
            false,
            false,
            None,
            &candidates,
        );
        assert!(
            matches!(report.seal, SealTier::Sealed(_)),
            "a crash mid-write never reads as tampering: {:?}",
            report.seal
        );
        assert_eq!(report.exit, TierExit::Ok);
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
        // (The class moved Forged → Unattributable 2026-07-30; the escape
        // property is the subject and must survive the reclassification.)
        let SealTier::Unattributable(reason) = seal_tier(Some(&line), 1, &[]) else {
            unreachable!("a key with no candidate is unattributable, not forged")
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
            .ok()
            .and_then(|b| crate::seal::open_fixture_box(&b))
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
    require_seal: bool,
    replay: Option<&ReplayCompare>,
    candidates: &[(String, String)],
) -> TierReport {
    let mut lines = Vec::new();
    // Before the seal leg: is a seal BURIED under appended lines? Every
    // leg below reads the LAST complete line, so an append after the seal
    // used to make the whole ladder go quiet — the seal invisible, the
    // exit 0, and the walk's own headline blaming a crash for what is the
    // append-after-seal forgery (measured 2026-07-30 · both an ordinary
    // frame and a `workflow_completed` appended after a valid seal
    // verified rc=0). The seal names the run's END; lines past it are not
    // that run's.
    if let Some((line, trailing)) = buried_seal(raw, events) {
        lines.push(format!(
            "SEAL BURIED — the journal carries a run_sealed frame at line {line} with \
             {trailing} line(s) chained AFTER it: the seal is a journal's last line and \
             covers everything before it, so those lines are outside all it attests\n  \
             appending + re-chaining needs only write access, never the key — this is \
             tampering, not a crash (a killed run leaves no seal at all)"
        ));
        return terminal_seal_report(
            SealTier::Buried { line, trailing },
            AnchorTier::NotPresent,
            TierExit::File,
            lines,
        );
    }
    let seal = seal_tier(last_complete_line(raw, events).as_ref(), events, candidates);
    let verdict = match seal_leg(&seal, require_anchor, require_seal, &mut lines) {
        Ok(verdict) => verdict,
        Err((anchor, exit)) => return terminal_seal_report(seal, anchor, exit, lines),
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

/// A ladder that stopped at the seal leg: nothing above chain-intact is
/// claimed, and no later leg was consulted.
fn terminal_seal_report(
    seal: SealTier,
    anchor: AnchorTier,
    exit: TierExit,
    lines: Vec<String>,
) -> TierReport {
    TierReport {
        seal,
        anchor,
        replay: ReplayTier::NotAsked,
        attained: AttainedTier::Ok,
        exit,
        lines,
    }
}

/// The seal leg: voice the seal verdict and hand the anchor leg its key —
/// `Err((anchor, exit))` when the ladder stops here (every class but
/// `Sealed`). The tier vocabulary is CLOSED in this module: a variant
/// added without teaching this match must not fall through to a silent
/// Ok (the false-green class).
fn seal_leg(
    seal: &SealTier,
    require_anchor: bool,
    require_seal: bool,
    lines: &mut Vec<String>,
) -> Result<SealVerdict, (AnchorTier, TierExit)> {
    match seal {
        SealTier::Sealed(v) => {
            lines.push(format!(
                "SEALED — the run_sealed signature verifies · key {} ({})",
                v.key_id, v.source
            ));
            Ok(v.clone())
        }
        SealTier::Unsealed => {
            // nika#1384 · the seal tier's absence is STATED, never silent.
            // Measured on 0.116.2: a journal with its run_sealed line cut
            // verified « OK · chain intact » with the SEALED line simply
            // gone and the same exit as a clean run — the cheapest edit a
            // trace can suffer left no mark. The line names the three
            // honest causes; the exit stays Ok (the journal is what it is)
            // unless the operator REQUIRED the tier, which is the ENV
            // class `--anchored` already answers for a missing sidecar.
            if require_seal {
                lines.push(
                    "SEALED — REQUIRED but the journal is unsealed (no run_sealed \
                     line): a keyless run, a run killed before its seal, or a \
                     journal cut after it — a missing seal is a missing input"
                        .to_owned(),
                );
            } else {
                lines.push(
                    "UNSEALED — no run_sealed frame: the run was not signed (no \
                     signing key at run time · a run killed before its seal · or a \
                     journal cut after it) — `nika sign` seals future runs · \
                     `--sealed` requires the tier"
                        .to_owned(),
                );
            }
            // The 2026-07-29 audit (run 2 · the anchor contract's silent
            // arm): this return used to swallow `require_anchor` —
            // `--anchored` on an UNSEALED journal exited Ok without a word
            // (measured: missing sidecar rc=0, forged sidecar rc=0, while
            // the sealed path honors 3/2 exactly). The anchor verifies
            // against the seal's key, so the tier is UNATTAINABLE here —
            // the honest refusal: exit ENV (the `--anchored` contract's own
            // class for a required tier that cannot be attained), the
            // requirement named out loud.
            if require_anchor {
                lines.push(
                    "ANCHORED — REQUIRED but the journal is unsealed (no run_sealed \
                     line): the anchor verifies against the seal's key, so the \
                     tier is unattainable"
                        .to_owned(),
                );
                return Err((AnchorTier::Required, TierExit::Env));
            }
            if require_seal {
                return Err((AnchorTier::NotPresent, TierExit::Env));
            }
            Err((AnchorTier::NotPresent, TierExit::Ok))
        }
        SealTier::Forged(reason) => {
            lines.push(format!("SEAL FORGED — {reason}"));
            Err((AnchorTier::NotPresent, TierExit::File))
        }
        SealTier::Unattributable(reason) => {
            lines.push(format!(
                "SEAL UNATTRIBUTABLE — {reason}\n  the signature is NOT judged: an absent \
                 key is a missing input, never evidence of forgery — pass --key <pub> or \
                 `nika key trust` the signer to climb to SEALED"
            ));
            Err((AnchorTier::NotPresent, TierExit::Env))
        }
        SealTier::Buried { .. } => {
            lines.push(
                "SEAL BURIED — the ladder cannot judge a buried seal (checked above)".to_owned(),
            );
            Err((AnchorTier::NotPresent, TierExit::File))
        }
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
