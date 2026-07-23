// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The data-as-code sink (NEP-0006 · LAW-AUTH-0327 · spec 10 §the
//! data-as-code sink) — the CLOSED v1 code-bearing classes and the ONE
//! classification predicate the check twin, the run twin, and the
//! reference oracle all read (the engine ≡ reference differential per
//! LAW-AUTH-0319 keeps the mirrored lists honest · one fixture per class).
//!
//! Classification reads a URL PATH only (the query carries no verdict ·
//! NEP-0006 law 4), case-insensitively on the path's final extension.
//! Byte-sniffing is the declared v1 residual.

/// One code-bearing class: its teaching name + its extensions.
type ClassRow = (&'static str, &'static [&'static str]);

/// The three CLOSED v1 classes (NEP-0006 · only a NEP amends them). Each
/// class is justified by a named RCE mechanism: the serialized-executable
/// family deserializes by running code (the HF loader-RCE vector), a
/// script exists to be run, a binary/module is code by definition. The
/// deliberate exclusions (`.safetensors` · `.npy`/`.npz` · archives ·
/// templates) and their reasons live in the NEP.
const CODE_BEARING_CLASSES: &[ClassRow] = &[
    (
        "serialized-executable",
        &[
            ".pkl", ".pickle", ".dill", ".joblib", ".pt", ".pth", ".ckpt",
        ],
    ),
    (
        "script/interpreter",
        &[
            ".py", ".sh", ".bash", ".zsh", ".ps1", ".bat", ".cmd", ".rb", ".pl", ".php", ".js",
            ".mjs", ".ipynb",
        ],
    ),
    (
        "executable binary/module",
        &[".exe", ".dll", ".so", ".dylib", ".wasm", ".jar"],
    ),
];

/// Classify a URL PATH (not a full URL — the caller extracts the path
/// with the WHATWG parser it already holds): `Some((class, extension))`
/// when the path's final extension names a code-bearing class, `None`
/// for the unbounded inert world.
#[must_use]
pub fn code_bearing_path_class(path: &str) -> Option<(&'static str, &'static str)> {
    // O7-C · a trailing slash or dot is a display artifact, not a class
    // change (`/setup.sh/` and `/setup.sh.` are the same fetch of the
    // same artifact — the edge case blade, 2026-07-23).
    let path = path.trim_end_matches(['/', '.']);
    let segment = path.rsplit('/').next().unwrap_or(path);
    // O7-A · the encoded-extension bypass (red-team 2026-07-23): the
    // verdict reads the DECODED final segment exactly once — the same
    // single decode URI normalizers apply (RFC 3986 §2.3), so
    // `legacy%2epkl` classifies like `legacy.pkl` and double-encoded
    // `%252e` stays inert (the origin decodes once too).
    let segment = percent_decode_once(segment);
    let segment = segment.as_str();
    let dot = segment.rfind('.')?;
    let ext = &segment[dot..];
    for (class, exts) in CODE_BEARING_CLASSES {
        if let Some(hit) = exts
            .iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(ext))
        {
            return Some((class, hit));
        }
    }
    // O7-B · the versioned native form (red-team 2026-07-23): `lib.so.1.2`
    // IS the on-disk shape dlopen loads — strip trailing `.N` groups and
    // re-match the native class only (a versioned script name like
    // `model.pt.2` is not a convention and stays out).
    let base = segment.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    if base.len() != segment.len() {
        let dot = base.rfind('.')?;
        let ext = &base[dot..];
        if let Some((class, exts)) = CODE_BEARING_CLASSES
            .iter()
            .find(|(class, _)| *class == "executable binary/module")
            && let Some(hit) = exts
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(ext))
        {
            return Some((class, hit));
        }
    }
    None
}

/// Decode the `%XX` octets of a path segment ONCE (best-effort · an
/// invalid triplet is left as-is, matching URI normalizer posture).
fn percent_decode_once(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push(u8::try_from((hi << 4) | lo).unwrap_or_default());
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_hit_per_class_and_case_insensitive() {
        // One probe per class keeps this mirror honest with the oracle's
        // (the conformance fixtures 018-022 are the cross-repo half).
        assert_eq!(
            code_bearing_path_class("/models/legacy.pkl"),
            Some(("serialized-executable", ".pkl"))
        );
        assert_eq!(
            code_bearing_path_class("/setup.sh"),
            Some(("script/interpreter", ".sh"))
        );
        assert_eq!(
            code_bearing_path_class("/lib/native.DYLIB"),
            Some(("executable binary/module", ".dylib"))
        );
    }

    #[test]
    fn trailing_slash_and_dot_are_display_artifacts_not_a_class_change() {
        // O7-C · the same artifact, decorated.
        assert_eq!(
            code_bearing_path_class("/dl/setup.sh/"),
            Some(("script/interpreter", ".sh"))
        );
        assert_eq!(
            code_bearing_path_class("/dl/setup.sh."),
            Some(("script/interpreter", ".sh"))
        );
    }

    #[test]
    fn the_inert_world_stays_none() {
        for p in [
            "/q3/rows.csv",
            "/feed.json",
            "/report.pdf",
            "/weights.safetensors",
            "/data.npz",
            "/bundle.tar.gz",
            "/no-extension",
            "/",
            "",
        ] {
            assert_eq!(code_bearing_path_class(p), None, "{p}");
        }
    }

    #[test]
    fn the_encoded_extension_decodes_once() {
        // O7-A · the bypass the red team found: an encoded extension is
        // the same verdict as the decoded one (one decode, like the
        // origin's resolver).
        assert_eq!(
            code_bearing_path_class("/models/legacy%2epkl"),
            Some(("serialized-executable", ".pkl"))
        );
        assert_eq!(
            code_bearing_path_class("/dl/setup%2esh"),
            Some(("script/interpreter", ".sh"))
        );
        // A double-encoded octet stays inert (the origin decodes once too).
        assert_eq!(code_bearing_path_class("/models/legacy%252epkl"), None);
        // A malformed triplet is left as-is (no crash · no verdict).
        assert_eq!(code_bearing_path_class("/models/legacy%2pkl"), None);
    }

    #[test]
    fn the_versioned_native_matches_its_class_only() {
        // O7-B · the on-disk dlopen form.
        assert_eq!(
            code_bearing_path_class("/lib/libevil.so.1.2.3"),
            Some(("executable binary/module", ".so"))
        );
        assert_eq!(
            code_bearing_path_class("/lib/native.dylib.4"),
            Some(("executable binary/module", ".dylib"))
        );
        // The strip never rescues a NON-native class (a versioned script
        // name is not a convention).
        assert_eq!(code_bearing_path_class("/models/model.pt.2"), None);
        assert_eq!(code_bearing_path_class("/dl/setup.sh.2"), None);
    }

    #[test]
    fn the_final_extension_decides_never_a_middle_one() {
        // `.py.txt` is a text file whose NAME mentions py — inert. The
        // final extension is the classification surface (NEP-0006).
        assert_eq!(code_bearing_path_class("/notes/script.py.txt"), None);
        assert_eq!(
            code_bearing_path_class("/notes/archive.txt.py"),
            Some(("script/interpreter", ".py"))
        );
    }
}
