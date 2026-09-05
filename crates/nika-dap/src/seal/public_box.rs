// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Public custody projection: only decoded public-key material may leave it.

/// Decode the public key, then reconstruct its box. The minisign box wrapper
/// alone validates nothing, and its parser ignores the comment and trailing
/// lines; forwarding the original string could therefore disclose unrelated
/// material. Engine-generated boxes retain their exact bytes and fingerprint.
pub(super) fn canonical(text: &str) -> Option<String> {
    let payload = text.lines().nth(1)?;
    let public = decode_payload(payload)?;
    Some(public.to_box().ok()?.to_string().trim().to_owned())
}

/// Bind the signing projection to the already-open secret. Compare the whole
/// canonical box, including the key number: minisign's `PublicKey` equality
/// compares only public material, not that number or the algorithm.
pub(super) fn for_signing(secret: &minisign::SecretKey, text: &str) -> Option<String> {
    let public = canonical(text)?;
    let expected = minisign::PublicKey::from_secret_key(secret)
        .ok()?
        .to_box()
        .ok()?
        .to_string();
    (public == expected.trim()).then_some(public)
}

fn decode_payload(payload: &str) -> Option<minisign::PublicKey> {
    // Bound the only string the decoder allocates; never copy the untrusted
    // comment or trailer. Minisign owns the exact public-key encoding check.
    if payload.len() > 128 {
        return None;
    }
    let public = minisign::PublicKey::from_base64(payload).ok()?;
    // from_bytes accepts arbitrary algorithm bytes; public keys in minisign
    // use Ed (ED identifies prehashed signatures, not a public-key format).
    public.to_bytes().starts_with(b"Ed").then_some(public)
}

/// Decode a verification ledger as whole public-key boxes. Keep each validated
/// historical box's comment bytes: existing fingerprints bind that wire form.
/// Unlike the live projection this reader never re-mints an old identity.
pub(crate) fn parse_many(text: &str) -> Vec<String> {
    let mut boxes = Vec::new();
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    while let Some(comment) = lines.next() {
        if !comment.starts_with("untrusted comment:") {
            continue;
        }
        let Some(payload) = lines.next() else { break };
        if decode_payload(payload).is_some() {
            boxes.push(format!("{comment}\n{payload}"));
        }
    }
    boxes
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::super::{load_from_files, public_from_files};
    use super::canonical;

    fn key_files() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().expect("fixture directory");
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("fixture keypair");
        let public = pair.pk.to_box().expect("public box").to_string();
        let secret = pair.sk.to_box(None).expect("secret box").to_string();
        std::fs::write(dir.path().join("key"), &secret).expect("fixture secret file");
        (dir, public, secret)
    }

    #[test]
    fn trust_and_signing_reject_a_private_box_in_the_public_slot() {
        let (dir, _, secret) = key_files();
        let key = dir.path().join("key");
        let public = dir.path().join("pub");
        std::fs::write(&public, secret).expect("deliberately misplaced fixture key");
        assert!(
            public_from_files(&key, &public).is_none(),
            "the trust and rotation reader must not return private material"
        );
        assert!(
            load_from_files(&key, &public).is_none(),
            "the signing reader must not carry private material as its public box"
        );
    }

    #[test]
    fn trust_and_signing_drop_untrusted_comments_and_trailing_payload() {
        let (dir, public, _) = key_files();
        let key = dir.path().join("key");
        let path = dir.path().join("pub");
        let payload = public.lines().nth(1).expect("public key payload");
        let hostile =
            format!("untrusted comment: SYNTHETIC-SECRET\n{payload}\nSYNTHETIC-TRAILER\n");
        std::fs::write(&path, hostile).expect("fixture public file");
        assert_eq!(
            public_from_files(&key, &path).as_deref(),
            Some(public.trim()),
            "only the canonical public box can reach trust and the retired ledger"
        );
        let (_, projected) = load_from_files(&key, &path).expect("the public key is valid");
        assert_eq!(
            projected,
            public.trim(),
            "the signer uses the same projection"
        );
    }

    #[test]
    fn trust_and_signing_reject_a_non_key_public_file() {
        let (dir, _, _) = key_files();
        let key = dir.path().join("key");
        let path = dir.path().join("pub");
        for text in ["", "SYNTHETIC-NOT-A-KEY", "untrusted comment: stub\nAAAA\n"] {
            std::fs::write(&path, text).expect("fixture invalid public file");
            assert!(public_from_files(&key, &path).is_none());
            assert!(load_from_files(&key, &path).is_none());
        }
    }

    #[test]
    fn signing_refuses_a_public_box_from_another_key() {
        let (dir, _, _) = key_files();
        let key = dir.path().join("key");
        let path = dir.path().join("pub");
        let other = minisign::KeyPair::generate_unencrypted_keypair().expect("other fixture key");
        std::fs::write(&path, other.pk.to_box().expect("other public").to_string())
            .expect("mismatched fixture");
        assert!(
            load_from_files(&key, &path).is_none(),
            "different public material"
        );
    }

    #[test]
    fn signing_refuses_another_key_number_even_with_the_same_public_material() {
        let (dir, public, _) = key_files();
        let key = dir.path().join("key");
        let path = dir.path().join("pub");
        let public_key = minisign::PublicKey::from_base64(public.lines().nth(1).expect("payload"))
            .expect("public");
        let mut bytes = public_key.to_bytes();
        bytes[2] ^= 1; // Same public material, different minisign key number.
        let renumbered = minisign::PublicKey::from_bytes(&bytes).expect("renumbered fixture");
        std::fs::write(
            &path,
            renumbered.to_box().expect("renumbered public").to_string(),
        )
        .expect("renumbered fixture");
        assert!(
            load_from_files(&key, &path).is_none(),
            "key number must bind too"
        );
    }

    #[test]
    fn an_unknown_public_key_algorithm_is_not_an_enrollment() {
        let (_, public, _) = key_files();
        let public_key = minisign::PublicKey::from_base64(public.lines().nth(1).expect("payload"))
            .expect("public");
        let mut bytes = public_key.to_bytes();
        bytes[..2].copy_from_slice(b"ZZ");
        let unknown = minisign::PublicKey::from_bytes(&bytes)
            .expect("library permits raw algorithm bytes")
            .to_box()
            .expect("fixture box")
            .to_string();
        assert!(canonical(&unknown).is_none());
        assert!(super::parse_many(&unknown).is_empty());
    }

    #[test]
    fn trace_candidates_reject_a_private_box_misfiled_as_public() {
        let (_, public, secret) = key_files();
        let mut candidates = Vec::new();
        super::super::extend_candidate_pubkeys(&mut candidates, &secret, "fixture");
        assert!(
            candidates.is_empty(),
            "a private box is not a verification candidate"
        );
        super::super::extend_candidate_pubkeys(&mut candidates, &public, "fixture");
        assert_eq!(
            candidates,
            vec![(public.trim().to_owned(), "fixture".to_owned())]
        );
    }

    #[test]
    fn historical_comment_bytes_keep_their_original_fingerprint() {
        let (_, public, _) = key_files();
        let payload = public.lines().nth(1).expect("public payload");
        let historical = format!("untrusted comment: earlier enrollment\n{payload}");
        assert_eq!(super::parse_many(&historical), vec![historical.clone()]);
        assert_ne!(
            super::super::fingerprint(&historical),
            super::super::fingerprint(public.trim())
        );
        assert_eq!(canonical(&historical).as_deref(), Some(public.trim()));
    }

    proptest::proptest! {
        #[test]
        fn public_projection_is_independent_of_untrusted_metadata(
            comment in "[^\\r\\n]{0,256}",
            trailer in ".{0,256}",
        ) {
            let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("fixture keypair");
            let public = pair.pk.to_box().expect("public box").to_string();
            let payload = public.lines().nth(1).expect("public key payload");
            let input = format!("{comment}\n{payload}\n{trailer}");
            let projected = canonical(&input);
            proptest::prop_assert_eq!(projected.as_deref(), Some(public.trim()));
        }
    }
}
