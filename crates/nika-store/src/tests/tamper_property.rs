// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! Gate 6 (F-P8): the tamper CLOSURE — for ANY byte flip in a signed entry
//! file, recall either decodes the same envelope (a no-op flip: whitespace
//! the codec ignores) or REJECTS; and NO relabeling of a decoded envelope
//! ever verifies (the label inside the preimage is the only label that
//! counts). The complement of the acceptance pairs: they pin the named
//! instances, this pins the whole class.

use crate::{
    MEMORY_ROOT, RecallVerdict, RejectReason, UnsignedEntry, recall_verified, remember_signed,
    store_dir,
};
use nika_cap::Integrity;
use proptest::prelude::*;

fn keypair() -> (minisign::PublicKey, minisign::SecretKey) {
    let pair =
        minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair mints");
    (pair.pk, pair.sk)
}

/// A small envelope-frame strategy (nested values, unicode, numbers) — the
/// JCS canonicalization must survive whatever JSON carries.
fn frame_strategy() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        any::<String>().prop_map(serde_json::Value::from),
        any::<u64>().prop_map(serde_json::Value::from),
        any::<bool>().prop_map(serde_json::Value::from),
    ];
    leaf.prop_recursive(3, 8, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::from),
            prop::collection::btree_map(any::<String>(), inner, 0..4)
                .prop_map(|m| serde_json::json!({"content": m})),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// ANY single-byte flip in the signed file: Admitted ⟹ the decoded
    /// envelope is byte-identical to what was signed (a flip in the sig
    /// box's UNSIGNED comment or in codec-ignored whitespace is cosmetic);
    /// Rejected ⟹ the envelope or its signature genuinely changed (the
    /// untouched envelope is never false-negative rejected). The closure
    /// the SMSR theorem needs.
    #[test]
    fn any_byte_flip_either_noops_or_rejects(
        frame in frame_strategy(),
        label_untrusted in any::<bool>(),
        (index, mask) in any::<(prop::sample::Index, u8)>()
            .prop_filter("a real flip", |(_, mask)| *mask != 0),
    ) {
        let root = tempfile::tempdir().expect("tempdir");
        let root = root.path().join(MEMORY_ROOT);
        let (pk, sk) = keypair();
        let dir = store_dir(&root, "default").expect("the store dir");
        let label = if label_untrusted {
            Integrity::untrusted("proptest-origin")
        } else {
            Integrity::trusted()
        };
        let written = remember_signed(
            &dir,
            UnsignedEntry::new(frame, label, "default".to_owned(), "run-p".to_owned(), 1_700_000_000_000),
            &sk,
        )
        .expect("the honest write lands");
        let honest_preimage = written.preimage();
        let honest_sig = written.sig.clone();
        let path = dir.join(crate::entry_file_name(&written));

        let mut bytes = std::fs::read(&path).expect("the file reads");
        let at = index.index(bytes.len());
        bytes[at] ^= mask;
        std::fs::write(&path, &bytes).expect("the flip lands");

        let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
        prop_assert_eq!(verdicts.len(), 1, "one entry rides the store");
        let v = &verdicts[0];
        // The closure, both directions:
        //  · Admitted ⟹ the decoded envelope is byte-identical to what was
        //    signed (no semantic change is EVER admitted — a flip in the
        //    sig's unsigned comment is cosmetic, the envelope is the law);
        //  · Rejected ⟹ the envelope or its signature genuinely changed
        //    (the untouched envelope is never false-negative rejected).
        match &v.verdict {
            RecallVerdict::Admitted => {
                let entry = v.entry.as_ref().expect("an admitted entry carries its envelope");
                prop_assert_eq!(
                    entry.preimage(),
                    honest_preimage,
                    "only the byte-identical envelope is ever admitted"
                );
            }
            RecallVerdict::Rejected(_) => {
                if let Some(entry) = &v.entry {
                    prop_assert!(
                        entry.preimage() != honest_preimage || entry.sig != honest_sig,
                        "the untouched envelope must never be rejected (a false negative)"
                    );
                }
            }
        }
    }

    /// NO relabeling of a decoded envelope ever verifies — trusted→untrusted,
    /// untrusted→trusted, witness rewritten: every well-formed relabel is a
    /// signature mismatch by construction. (A relabel that breaks the wire
    /// shape itself — witness dropped · non-object label — is `Malformed`,
    /// pinned in api.rs; it never verifies either.)
    #[test]
    fn no_relabeling_ever_verifies(
        frame in frame_strategy(),
        born_untrusted in any::<bool>(),
        claimed in prop::sample::select(vec![
            serde_json::json!({"kind": "trusted"}),
            serde_json::json!({"kind": "untrusted", "source": "attacker"}),
            serde_json::json!({"kind": "untrusted", "source": "proptest-origin"}),
        ]),
    ) {
        let root = tempfile::tempdir().expect("tempdir");
        let root = root.path().join(MEMORY_ROOT);
        let (pk, sk) = keypair();
        let dir = store_dir(&root, "default").expect("the store dir");
        let born = if born_untrusted {
            Integrity::untrusted("proptest-origin")
        } else {
            Integrity::trusted()
        };
        let written = remember_signed(
            &dir,
            UnsignedEntry::new(frame, born.clone(), "default".to_owned(), "run-p".to_owned(), 1_700_000_000_000),
            &sk,
        )
        .expect("the honest write lands");
        let path = dir.join(crate::entry_file_name(&written));
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the file reads"))
                .expect("the envelope parses");
        let original_label = value["label"].clone();
        value["label"] = claimed.clone();
        std::fs::write(&path, serde_json::to_string_pretty(&value).expect("serializes"))
            .expect("the relabel lands");
        let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
        let v = &verdicts[0];
        if claimed == original_label {
            prop_assert_eq!(v.verdict.clone(), RecallVerdict::Admitted, "a same-value rewrite is no relabel");
        } else {
            prop_assert_eq!(
                v.verdict.clone(),
                RecallVerdict::Rejected(RejectReason::BadSignature),
                "a relabel IS a signature mismatch (born {:?} → claimed {:?})",
                born,
                claimed,
            );
        }
    }
}
