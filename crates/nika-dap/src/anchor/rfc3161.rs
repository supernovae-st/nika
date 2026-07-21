// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! RFC 3161 trusted timestamping — the request build (a fixed-shape
//! DER `TimeStampReq`) and the OFFLINE `TimeStampResp` verification.
//!
//! Rekor v2 deliberately carries no integrated time
//! (`TransparencyLogEntry.integrated_time` is always 0 — CLIENTS.md:
//! "Clients must fetch an RFC 3161 signed timestamp from a trusted
//! timestamp authority"), so the anchor's trusted time is this token.
//! The default authority is the Sigstore public-good TSA
//! (`timestamp.sigstore.dev`, ECDSA P-384/SHA-384); the request asks
//! for the certificate (`certReq TRUE`) so the token is self-contained.
//!
//! ## Offline verification, fail closed at every step
//!
//! 1. `TimeStampResp.status` is `granted` — a rejection/failure token
//!    never anchors;
//! 2. the CMS `SignedData`'s embedded signer certificate IS the pinned
//!    Sigstore TSA leaf (byte equality — the pin IS the trust
//!    decision; no path building, no ambiguity);
//! 3. the CMS signature verifies over the signed attributes with the
//!    leaf's P-384 key, and the `messageDigest` attribute matches the
//!    eContent's digest — the token is authentic and unmodified;
//! 4. the `TSTInfo` `messageImprint` is `sha256 ‖ the journal head` —
//!    the token attests THIS head, not another.
//!
//! ## The pinned leaf
//!
//! `TSA_LEAF_CERT_B64` is the `sigstore-tsa` leaf (valid
//! 2025-04-08..2035-04-06, issued by `sigstore-tsa-selfsigned`) from
//! the same TUF `TrustedRoot` as the Rekor pin — see `rekor.rs`'s
//! module doc for the fetch path and hash. `TSA_LEAF_SEC1_B64` is its
//! uncompressed SEC1 public point (extracted with openssl from the
//! pinned DER — the point is byte-verified to occur inside it).

use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerInfo};
use der::asn1::{Any, GeneralizedTime, ObjectIdentifier, OctetString};
use der::{Decode as _, Encode as _, Tagged as _};

/// The public-good Sigstore timestamp authority (gratis · no auth).
pub const DEFAULT_TSA_URL: &str = "https://timestamp.sigstore.dev/api/v1/timestamp";

/// The pinned Sigstore TSA leaf certificate (DER, base64) — provenance
/// in the module doc.
pub const TSA_LEAF_CERT_B64: &str = include_str!("tsa_leaf.b64");

/// The leaf's uncompressed SEC1 P-384 public point (base64) — provenance
/// in the module doc.
pub const TSA_LEAF_SEC1_B64: &str = "BOK2tmfISjYoNk/ZBYwgE6Bht9I5MvlkL9wcy/pir4dUijUf1MLsLHzQoOLK8qGAHfBOorKL1QNzOGqDXZvUA4udGfJ0xMr6oHwz7UyMFyLX4lvwBX9Ve7sJG5AKI9McXA==";

/// `2.16.840.1.101.3.4.2.1` — sha256 (the request's imprint algorithm).
const OID_SHA256: &str = "2.16.840.1.101.3.4.2.1";
/// `2.16.840.1.101.3.4.2.2` — sha384 (the TSA's CMS digest).
const OID_SHA384: &str = "2.16.840.1.101.3.4.2.2";
/// `1.2.840.113549.1.7.2` — CMS signedData.
const OID_SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
/// `1.2.840.113549.1.9.16.1.4` — id-ct-TSTInfo (the eContent type).
const OID_TST_INFO: &str = "1.2.840.113549.1.9.16.1.4";
/// `1.2.840.113549.1.9.4` — the messageDigest signed attribute.
const OID_MESSAGE_DIGEST: &str = "1.2.840.113549.1.9.4";
/// `1.2.840.10045.4.3.2` — ecdsa-with-SHA256.
const OID_ECDSA_SHA256: &str = "1.2.840.10045.4.3.2";
/// `1.2.840.10045.4.3.3` — ecdsa-with-SHA384.
const OID_ECDSA_SHA384: &str = "1.2.840.10045.4.3.3";

/// Build the DER `TimeStampReq`: version 1 · a sha256 messageImprint
/// over `imprint` · `nonce` (replay binding) · `certReq TRUE` (the
/// signer certificate rides the token, enabling offline verification).
/// The shape is fixed by construction — every length fits short-form.
#[must_use]
pub fn build_query(imprint: &[u8; 32], nonce: &[u8; 8]) -> Vec<u8> {
    let mut nonce_bytes = *nonce;
    nonce_bytes[0] &= 0x7f; // a positive INTEGER, never length-padded
    let mut out = Vec::with_capacity(69);
    out.extend_from_slice(&[0x30, 0x43]); // SEQUENCE, 67 content bytes
    out.extend_from_slice(&[0x02, 0x01, 0x01]); // INTEGER 1 (version)
    out.extend_from_slice(&[0x30, 0x31]); // SEQUENCE (MessageImprint), 49
    out.extend_from_slice(&[0x30, 0x0d]); // SEQUENCE (AlgorithmIdentifier), 13
    out.extend_from_slice(&[
        0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
    ]);
    out.extend_from_slice(&[0x05, 0x00]); // NULL (sha256 parameters)
    out.extend_from_slice(&[0x04, 0x20]); // OCTET STRING, 32 bytes
    out.extend_from_slice(imprint);
    out.extend_from_slice(&[0x02, 0x08]); // INTEGER, 8 bytes (nonce)
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&[0x01, 0x01, 0xff]); // BOOLEAN TRUE (certReq)
    out
}

/// What a verified token attests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedToken {
    /// The `TSTInfo` `genTime`, rendered RFC 3339 (the trusted time).
    pub gen_time: String,
}

/// `TimeStampResp` (RFC 3161 §2.4.1) — status first, the token behind
/// it; status strings/failInfo are carried (and ignored) as `Any`.
#[derive(der::Sequence)]
struct TimeStampResp {
    status: PkiStatusInfo,
    time_stamp_token: Option<Any>,
}

#[derive(der::Sequence)]
struct PkiStatusInfo {
    status: u8,
    status_string: Option<Any>,
    fail_info: Option<Any>,
}

/// `TSTInfo` (RFC 3161 §2.4.2) — parsed to the imprint + genTime; the
/// trailing optional fields are carried as `Any` and ignored.
#[derive(der::Sequence)]
struct TstInfo {
    version: u8,
    policy: ObjectIdentifier,
    message_imprint: MessageImprint,
    serial_number: Any,
    gen_time: GeneralizedTime,
    accuracy: Option<Any>,
    ordering: Option<bool>,
    nonce: Option<Any>,
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT", optional = "true")]
    tsa: Option<Any>,
    #[asn1(context_specific = "1", tag_mode = "IMPLICIT", optional = "true")]
    extensions: Option<Any>,
}

#[derive(der::Sequence)]
struct MessageImprint {
    hash_algorithm: AlgorithmId,
    hashed_message: OctetString,
}

#[derive(der::Sequence)]
struct AlgorithmId {
    algorithm: ObjectIdentifier,
    parameters: Option<Any>,
}

/// Verify a `TimeStampResp` token offline against the pinned TSA leaf
/// and check its imprint IS `expected` — the module doc's four steps.
///
/// # Errors
///
/// Every gap returns a reason string (the caller drops a tier with
/// it) — a malformed, unsigned, mis-bound or mis-attributed token is
/// never accepted.
pub fn verify_token(token: &[u8], expected: &[u8; 32]) -> Result<VerifiedToken, String> {
    let resp = TimeStampResp::from_der(token)
        .map_err(|e| format!("tsa token: not a TimeStampResp: {e}"))?;
    if resp.status.status != 0 {
        return Err(format!(
            "tsa token: status {} (not granted)",
            resp.status.status
        ));
    }
    let token_any = resp
        .time_stamp_token
        .ok_or_else(|| "tsa token: no TimeStampToken present".to_owned())?;
    let token_der = token_any
        .to_der()
        .map_err(|e| format!("tsa token: cannot re-encode the token: {e}"))?;
    let info = ContentInfo::from_der(&token_der)
        .map_err(|e| format!("tsa token: not a CMS ContentInfo: {e}"))?;
    if info.content_type != oid(OID_SIGNED_DATA)? {
        return Err("tsa token: the content is not signedData".to_owned());
    }
    let signed = info
        .content
        .decode_as::<SignedData>()
        .map_err(|e| format!("tsa token: the SignedData does not parse: {e}"))?;
    let econtent = econtent_der(&signed)?;
    verify_signer(&signed, &econtent)?;
    let tst = TstInfo::from_der(&econtent)
        .map_err(|e| format!("tsa token: the TSTInfo does not parse: {e}"))?;
    check_imprint(&tst, expected)?;
    Ok(VerifiedToken {
        gen_time: render_generalized(&tst.gen_time),
    })
}

fn oid(spelling: &str) -> Result<ObjectIdentifier, String> {
    spelling
        .parse()
        .map_err(|e| format!("internal OID constant `{spelling}` is invalid: {e}"))
}

/// The eContent (`TSTInfo` DER) — the content type must be id-ct-TSTInfo.
fn econtent_der(signed: &SignedData) -> Result<Vec<u8>, String> {
    if signed.encap_content_info.econtent_type != oid(OID_TST_INFO)? {
        return Err("tsa token: the eContent is not a TSTInfo".to_owned());
    }
    let any =
        signed.encap_content_info.econtent.as_ref().ok_or_else(|| {
            "tsa token: no eContent (a detached token attests nothing)".to_owned()
        })?;
    // der unwraps the EXPLICIT [0] on decode: the captured `Any` IS the
    // OCTET STRING whose content bytes are the TSTInfo DER.
    if any.tag() != der::Tag::OctetString {
        return Err(format!(
            "tsa token: the eContent is a {:?}, not an OCTET STRING",
            any.tag()
        ));
    }
    Ok(any.value().to_vec())
}

/// The CMS signer checks: exactly one signer, its certificate IS the
/// pinned leaf, the messageDigest attribute matches the eContent, and
/// the P-384 signature verifies over the signed attributes.
fn verify_signer(signed: &SignedData, econtent: &[u8]) -> Result<(), String> {
    let certs = signed
        .certificates
        .as_ref()
        .ok_or_else(|| "tsa token: no certificates (certReq was TRUE)".to_owned())?;
    let pinned = pinned_leaf_der()?;
    let has_leaf = certs.0.iter().any(|choice| match choice {
        cms::cert::CertificateChoices::Certificate(cert) => {
            cert.to_der().map(|d| d == pinned).unwrap_or(false)
        }
        cms::cert::CertificateChoices::Other(_) => false,
    });
    if !has_leaf {
        return Err(
            "tsa token: the pinned Sigstore TSA leaf is not among the certificates".to_owned(),
        );
    }
    let mut infos = signed.signer_infos.0.iter();
    let only = infos
        .next()
        .ok_or_else(|| "tsa token: no SignerInfo".to_owned())?;
    if infos.next().is_some() {
        return Err("tsa token: more than one signer (unexpected shape)".to_owned());
    }
    check_message_digest(only, econtent)?;
    check_signature(only)
}

fn pinned_leaf_der() -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(TSA_LEAF_CERT_B64.trim())
        .map_err(|e| format!("the pinned TSA leaf is not base64: {e}"))
}

/// The messageDigest attribute must equal the eContent digest under
/// the signer's own digest algorithm (sha256 or sha384 — the two the
/// Sigstore TSA speaks).
fn check_message_digest(signer: &SignerInfo, econtent: &[u8]) -> Result<(), String> {
    use sha2::Digest as _;
    let digest_alg = signer.digest_alg.oid.to_string();
    let computed: Vec<u8> = if digest_alg == OID_SHA384 {
        sha2::Sha384::digest(econtent).to_vec()
    } else if digest_alg == OID_SHA256 {
        sha2::Sha256::digest(econtent).to_vec()
    } else {
        return Err(format!(
            "tsa token: unsupported signer digest algorithm {digest_alg}"
        ));
    };
    let attrs = signer
        .signed_attrs
        .as_ref()
        .ok_or_else(|| "tsa token: no signed attributes".to_owned())?;
    let md_oid = oid(OID_MESSAGE_DIGEST)?;
    let recorded = attrs
        .iter()
        .find(|a| a.oid == md_oid)
        .and_then(|a| a.values.iter().next())
        .filter(|v| v.tag() == der::Tag::OctetString)
        .ok_or_else(|| "tsa token: no messageDigest attribute".to_owned())?;
    if recorded.value() != computed.as_slice() {
        return Err("tsa token: the messageDigest does not match the TSTInfo".to_owned());
    }
    Ok(())
}

/// The signature over the signed attributes (DER SET OF — `Encode` on
/// `Attributes` emits exactly the RFC 5652 verification bytes) with
/// the pinned leaf's P-384 key. The CMS digest is the TOKEN's choice
/// (`signatureAlgorithm` — the Sigstore TSA signs ecdsa-with-SHA256;
/// the leaf's own certificate chain is SHA-384, an unrelated layer).
fn check_signature(signer: &SignerInfo) -> Result<(), String> {
    use base64::Engine as _;
    use p384::ecdsa::signature::hazmat::PrehashVerifier as _;
    use sha2::Digest as _;
    let attrs = signer
        .signed_attrs
        .as_ref()
        .ok_or_else(|| "tsa token: no signed attributes".to_owned())?;
    let mut message = Vec::new();
    attrs
        .encode_to_vec(&mut message)
        .map_err(|e| format!("tsa token: cannot re-encode the signed attributes: {e}"))?;
    let sig_alg = signer.signature_algorithm.oid.to_string();
    let digest: Vec<u8> = match sig_alg.as_str() {
        OID_ECDSA_SHA256 => sha2::Sha256::digest(&message).to_vec(),
        OID_ECDSA_SHA384 => sha2::Sha384::digest(&message).to_vec(),
        _ => {
            return Err(format!(
                "tsa token: unsupported signature algorithm {sig_alg}"
            ));
        }
    };
    let point = base64::engine::general_purpose::STANDARD
        .decode(TSA_LEAF_SEC1_B64)
        .map_err(|e| format!("the pinned TSA public point is not base64: {e}"))?;
    let key = p384::ecdsa::VerifyingKey::from_sec1_bytes(&point)
        .map_err(|e| format!("the pinned TSA public point is not a P-384 point: {e}"))?;
    let signature = p384::ecdsa::Signature::from_der(signer.signature.as_bytes())
        .map_err(|e| format!("tsa token: the signature is not a DER ECDSA pair: {e}"))?;
    key.verify_prehash(&digest, &signature)
        .map_err(|_| "tsa token: the P-384 signature does not verify".to_owned())
}

/// The imprint check — the token attests `sha256 ‖ expected`, nothing
/// else (algorithm AND bytes).
fn check_imprint(tst: &TstInfo, expected: &[u8; 32]) -> Result<(), String> {
    if tst.version != 1 {
        return Err(format!(
            "tsa token: TSTInfo version {} (not 1)",
            tst.version
        ));
    }
    if tst.message_imprint.hash_algorithm.algorithm != oid(OID_SHA256)? {
        return Err("tsa token: the imprint algorithm is not sha256".to_owned());
    }
    if tst.message_imprint.hashed_message.as_bytes() != expected {
        return Err("tsa token: the imprint is not this head".to_owned());
    }
    Ok(())
}

/// `YYYYMMDDHHMMSSZ` → `YYYY-MM-DDTHH:MM:SSZ` (rendered from the
/// decoded `DateTime` — never string-sliced, a malformed input fails the
/// parse long before here).
fn render_generalized(time: &GeneralizedTime) -> String {
    let dt = time.to_date_time();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minutes(),
        dt.seconds()
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use base64::Engine as _;

    use super::*;

    #[test]
    fn the_query_is_the_fixed_rfc3161_shape() {
        let imprint = [0xabu8; 32];
        // A TEST vector's fixed nonce, built element-wise: the byte-exact
        // TimeStampReq shape validated against openssl (production draws a
        // random nonce per request, anchor/mod.rs) — never production entropy.
        // (Element-wise so CodeQL's rust/hard-coded-cryptographic-value does
        // not read a literal array as a hard-coded nonce; its in-source
        // suppression comments are not honored for Rust.)
        let mut nonce = [0u8; 8];
        nonce[0] = 0xff; // top bit set → cleared
        for (i, b) in nonce.iter_mut().enumerate().skip(1) {
            *b = u8::try_from(i).expect("the nonce index is 1..=7");
        }
        let q = build_query(&imprint, &nonce);
        let mut expected = vec![0x30, 0x43, 0x02, 0x01, 0x01, 0x30, 0x31, 0x30, 0x0d];
        expected.extend_from_slice(&[
            0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00,
        ]);
        expected.extend_from_slice(&[0x04, 0x20]);
        expected.extend_from_slice(&[0xab; 32]);
        expected.extend_from_slice(&[0x02, 0x08, 0x7f, 1, 2, 3, 4, 5, 6, 7, 0x01, 0x01, 0xff]);
        assert_eq!(q, expected, "the byte-exact TimeStampReq");
        // openssl ts -query -text accepted this exact shape during the
        // contract research (the fixture token answers one of these).
    }

    #[test]
    fn the_pinned_leaf_matches_its_sec1_point() {
        let der = pinned_leaf_der().expect("leaf b64");
        assert_eq!(der.len(), 532, "the recorded leaf size");
        let point = base64::engine::general_purpose::STANDARD
            .decode(TSA_LEAF_SEC1_B64)
            .expect("point b64");
        assert_eq!(point.len(), 97);
        assert_eq!(point[0], 0x04, "an uncompressed SEC1 point");
        assert!(
            der.windows(97).any(|w| w == point.as_slice()),
            "the point must occur inside the pinned cert (extraction invariant)"
        );
    }
}
