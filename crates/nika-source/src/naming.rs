// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure lexical program-source naming. No filesystem I/O, no grants,
//! no case folding, no charset widening.
//!
//! Callers still decide allowed path *shape* (absolute CLI paths vs
//! owned relative registry names) and still prove a path is a regular
//! file under their own root, symlink and capability policy.

/// Canonical executable Nika program suffix, including the leading dot.
pub const PROGRAM_SUFFIX: &str = ".nika";

/// Project/workspace configuration filename. Distinct from program sources
/// and from the runtime directory.
pub const PROJECT_FILE_NAME: &str = "nika.yaml";

/// Runtime/internal state directory name. Not a program filename.
pub const RUNTIME_DIR_NAME: &str = ".nika";

/// Retired live-program suffixes. Never accepted, discovered or emitted.
pub const RETIRED_PROGRAM_SUFFIXES: [&str; 2] = [".nika.yaml", ".nika.yml"];

/// Basename glob for a canonical program file in one directory.
pub const PROGRAM_GLOB: &str = "*.nika";

/// Recursive glob for canonical program files.
pub const PROGRAM_GLOB_RECURSIVE: &str = "**/*.nika";

/// Classification of a basename. Paths with separators are [`SourceNameKind::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceNameKind {
    /// `support.nika` / `support.v2.nika` — nonempty stem, exact suffix.
    CanonicalProgram,
    /// Retired live suffixes `.nika.yaml` / `.nika.yml`.
    RetiredProgram,
    /// The project file `nika.yaml`.
    ProjectFile,
    /// The runtime directory name `.nika` (empty program stem).
    RuntimeDir,
    /// Lookalikes, other YAML, mixed case, separators, controls.
    Other,
}

/// Last `/`- or `\`-separated component of a lexical path.
///
/// A trailing `/` or `\` is a directory shape and yields [`None`] — this
/// helper does not normalize it into a file name. Callers that own
/// relative `/`-only names (Serve registry) still reject `\` themselves.
#[must_use]
pub fn path_file_name(path: &str) -> Option<&str> {
    if path.is_empty() || path.ends_with('/') || path.ends_with('\\') {
        return None;
    }
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if name.is_empty() || name == "." || name == ".." {
        None
    } else {
        Some(name)
    }
}

/// Classify a basename. Separators, controls and mixed-case suffixes are
/// [`SourceNameKind::Other`]; this function does not consult the filesystem.
#[must_use]
pub fn classify_file_name(name: &str) -> SourceNameKind {
    if name.is_empty() || !is_clean_basename(name) {
        return SourceNameKind::Other;
    }
    if name == PROJECT_FILE_NAME {
        return SourceNameKind::ProjectFile;
    }
    if name == RUNTIME_DIR_NAME {
        return SourceNameKind::RuntimeDir;
    }
    if RETIRED_PROGRAM_SUFFIXES
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        return SourceNameKind::RetiredProgram;
    }
    match name.strip_suffix(PROGRAM_SUFFIX) {
        Some(stem) if !stem.is_empty() => SourceNameKind::CanonicalProgram,
        _ => SourceNameKind::Other,
    }
}

/// Classify the last component of a `/`-separated path string.
#[must_use]
pub fn classify_path(path: &str) -> SourceNameKind {
    path_file_name(path).map_or(SourceNameKind::Other, classify_file_name)
}

/// Whether `name` is an exact lowercase `*.nika` program basename.
#[must_use]
pub fn is_canonical_program_file_name(name: &str) -> bool {
    classify_file_name(name) == SourceNameKind::CanonicalProgram
}

/// Whether `path`'s last component is a canonical program basename.
#[must_use]
pub fn is_canonical_program_path(path: &str) -> bool {
    classify_path(path) == SourceNameKind::CanonicalProgram
}

/// Whether `name` uses a retired live-program suffix.
#[must_use]
pub fn is_retired_program_file_name(name: &str) -> bool {
    classify_file_name(name) == SourceNameKind::RetiredProgram
}

/// Stem of a canonical program basename (`support.v2.nika` → `support.v2`).
///
/// Does not use `file_stem()`: that would leave `.nika` inside
/// `support.nika.yaml`.
#[must_use]
pub fn program_stem(file_name: &str) -> Option<&str> {
    if !is_canonical_program_file_name(file_name) {
        return None;
    }
    file_name
        .strip_suffix(PROGRAM_SUFFIX)
        .filter(|stem| !stem.is_empty())
}

/// Strip a canonical suffix from a slug or path, keeping directories.
/// `hello.nika` and `hello` yield `hello`; `snippets/a.nika` yields
/// `snippets/a`. Retired suffixes are not stripped — that would be a live alias.
#[must_use]
pub fn typed_stem(name: &str) -> &str {
    if path_file_name(name).is_some_and(is_canonical_program_file_name) {
        name.strip_suffix(PROGRAM_SUFFIX).unwrap_or(name)
    } else {
        name
    }
}

/// Append the canonical suffix to a dest, preserving directories
/// (`workflows/foo` → `workflows/foo.nika`, including native `\` on
/// Windows). Already-canonical paths are unchanged. Trailing separators,
/// controls, empty names and retired suffixes yield [`None`] — this is
/// not a live alias.
#[must_use]
pub fn with_program_suffix(path: &str) -> Option<String> {
    if path.is_empty() || path.ends_with('/') || path.ends_with('\\') || !is_clean_path(path) {
        return None;
    }
    let name = path_file_name(path)?;
    match classify_file_name(name) {
        SourceNameKind::CanonicalProgram => Some(path.to_owned()),
        SourceNameKind::Other => Some(format!("{path}{PROGRAM_SUFFIX}")),
        _ => None,
    }
}

fn is_clean_path(path: &str) -> bool {
    path.bytes().all(|byte| byte >= 0x20 && byte != 0x7f)
}

/// Actionable rename hint for a retired live-program basename.
#[must_use]
pub fn retired_rename_hint(file_name: &str) -> Option<String> {
    if !is_retired_program_file_name(file_name) {
        return None;
    }
    let stem = RETIRED_PROGRAM_SUFFIXES
        .iter()
        .find_map(|suffix| file_name.strip_suffix(suffix))
        .filter(|stem| !stem.is_empty())?;
    Some(format!(
        "`{file_name}` is a retired Nika program suffix — rename to `{stem}{PROGRAM_SUFFIX}`"
    ))
}

fn is_clean_basename(name: &str) -> bool {
    !name.contains('/')
        && !name.contains('\\')
        && name.bytes().all(|byte| byte >= 0x20 && byte != 0x7f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_classifies_canonical_retired_lookalike_and_empty_stem() {
        let cases = [
            (
                "support.nika",
                SourceNameKind::CanonicalProgram,
                Some("support"),
            ),
            (
                "support.v2.nika",
                SourceNameKind::CanonicalProgram,
                Some("support.v2"),
            ),
            ("foo.nika.yaml", SourceNameKind::RetiredProgram, None),
            ("foo.nika.yml", SourceNameKind::RetiredProgram, None),
            (".nika.yaml", SourceNameKind::RetiredProgram, None),
            ("foo.yaml", SourceNameKind::Other, None),
            ("foo.yml", SourceNameKind::Other, None),
            ("foo.nika.evil", SourceNameKind::Other, None),
            ("foo.nika.minisig", SourceNameKind::Other, None),
            ("foo.nika.golden.json", SourceNameKind::Other, None),
            ("foo.NIKA", SourceNameKind::Other, None),
            ("foo.Nika", SourceNameKind::Other, None),
            ("nika.yaml", SourceNameKind::ProjectFile, None),
            (".nika", SourceNameKind::RuntimeDir, None),
            (
                "something.nika",
                SourceNameKind::CanonicalProgram,
                Some("something"),
            ),
            ("", SourceNameKind::Other, None),
            ("foo.nika\0x", SourceNameKind::Other, None),
            ("foo/bar.nika", SourceNameKind::Other, None),
            ("foo\\bar.nika", SourceNameKind::Other, None),
        ];
        for (name, kind, stem) in cases {
            assert_eq!(classify_file_name(name), kind, "{name}");
            assert_eq!(program_stem(name), stem, "stem {name}");
        }
    }

    #[test]
    fn path_classification_uses_basename_and_rejects_trailing_slash_as_dir_shape() {
        assert_eq!(
            classify_path("workflows/support.nika"),
            SourceNameKind::CanonicalProgram
        );
        assert_eq!(
            classify_path("workflows/support.nika.yaml"),
            SourceNameKind::RetiredProgram
        );
        assert_eq!(classify_path("nika.yaml"), SourceNameKind::ProjectFile);
        assert_eq!(classify_path(".nika"), SourceNameKind::RuntimeDir);
        assert_eq!(classify_path(".nika/"), SourceNameKind::Other);
        assert!(is_canonical_program_path("/abs/foo.nika"));
        assert!(is_canonical_program_path(r"C:\abs\foo.nika"));
        assert!(!is_canonical_program_path("../foo.nika.yaml"));
        assert!(!is_canonical_program_path("foo.nika/"));
        assert!(!is_canonical_program_path(r"foo.nika\"));
        assert_eq!(path_file_name("foo.nika/"), None);
        assert_eq!(path_file_name(r"foo.nika\"), None);
        assert_eq!(path_file_name("/"), None);
        assert_eq!(path_file_name(r"\"), None);
        assert_eq!(
            path_file_name(r"C:\workflows\support.nika"),
            Some("support.nika")
        );
    }

    #[test]
    fn suffix_join_and_globs_are_canonical_only() {
        assert_eq!(
            with_program_suffix("support").as_deref(),
            Some("support.nika")
        );
        assert_eq!(
            with_program_suffix("support.nika").as_deref(),
            Some("support.nika")
        );
        assert_eq!(
            with_program_suffix("support.v2").as_deref(),
            Some("support.v2.nika")
        );
        assert_eq!(
            with_program_suffix("workflows/foo").as_deref(),
            Some("workflows/foo.nika")
        );
        assert_eq!(
            with_program_suffix(r"workflows\foo").as_deref(),
            Some(r"workflows\foo.nika")
        );
        assert_eq!(with_program_suffix("foo.nika/"), None);
        assert_eq!(with_program_suffix(r"foo.nika\"), None);
        assert_eq!(with_program_suffix("foo.nika.yaml"), None);
        assert_eq!(with_program_suffix(""), None);
        assert_eq!(with_program_suffix("foo\0bar"), None);
        assert_eq!(PROGRAM_GLOB, "*.nika");
        assert_eq!(PROGRAM_GLOB_RECURSIVE, "**/*.nika");
        assert!(PROGRAM_GLOB.ends_with(PROGRAM_SUFFIX));
        assert_eq!(
            retired_rename_hint("support.nika.yaml").as_deref(),
            Some("`support.nika.yaml` is a retired Nika program suffix — rename to `support.nika`")
        );
        assert_eq!(
            retired_rename_hint("child.nika.yml").as_deref(),
            Some("`child.nika.yml` is a retired Nika program suffix — rename to `child.nika`")
        );
        assert_eq!(retired_rename_hint("support.nika"), None);
        assert_eq!(retired_rename_hint("nika.yaml"), None);
    }

    #[test]
    fn helpers_do_not_case_fold_or_sniff_yaml() {
        assert!(!is_canonical_program_file_name("Foo.Nika"));
        assert!(!is_canonical_program_file_name("foo.NIKA"));
        assert!(!is_retired_program_file_name("foo.yaml"));
        assert!(!is_retired_program_file_name("foo.yml"));
        assert!(!is_canonical_program_file_name("foo.nika.yaml"));
        assert!(is_retired_program_file_name("foo.nika.yaml"));
        assert!(is_retired_program_file_name("foo.nika.yml"));
        assert_eq!(typed_stem("hello.nika"), "hello");
        assert_eq!(typed_stem("hello"), "hello");
        assert_eq!(typed_stem("hello.nika.yaml"), "hello.nika.yaml");
        assert_eq!(typed_stem("snippets/delegate.nika"), "snippets/delegate");
        assert_eq!(typed_stem("snippets/delegate"), "snippets/delegate");
    }
}
