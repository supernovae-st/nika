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
    let segment = path.rsplit('/').next().unwrap_or(path);
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
    None
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
