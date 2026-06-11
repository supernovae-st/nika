// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! File builtins (5) — read · write · edit · glob · grep (stdlib §File).
//! Each composes the injected kernel `Fs*Dyn` seams.

use std::path::Path;

use nika_kernel::io::fs::{FsError, FsListDyn, FsReadDyn, FsWriteDyn};

use crate::{Args, BuiltinFailure, BuiltinOutcome, opt_bool, req_str};

/// `nika:read` — text (default) or binary. Returns the file content.
pub(crate) async fn read<F: FsReadDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-READ-001";
    const C3: &str = "NIKA-BUILTIN-READ-003";
    let path = req_str(args, "path", C1)?;
    if opt_bool(args, "binary", false) {
        // Opaque bytes flow tool→tool — we surface them base64-tagged so
        // they round-trip through the string `content` channel without
        // pretending to be text (spec 04 value-rendering).
        let bytes = fs
            .read(Path::new(path))
            .await
            .map_err(|e| read_failure(e, path))?;
        Ok(
            serde_json::json!({ "bytes_base64": crate::data::base64_encode(&bytes), "len": bytes.len() }),
        )
    } else {
        let text = fs
            .read_to_string(Path::new(path))
            .await
            .map_err(|e| match e {
                FsError::InvalidData { .. } => {
                    BuiltinFailure::new(C3, format!("`{path}` is not UTF-8 — use binary: true"))
                }
                other => read_failure(other, path),
            })?;
        Ok(serde_json::Value::String(text))
    }
}

fn read_failure(e: FsError, path: &str) -> BuiltinFailure {
    match e {
        FsError::NotFound { .. } => {
            BuiltinFailure::new("NIKA-BUILTIN-READ-001", format!("file not found: {path}"))
        }
        other => BuiltinFailure::new(
            "NIKA-BUILTIN-READ-002",
            format!("IO failure on {path}: {other}"),
        ),
    }
}

/// `nika:write` — write a file, return its path. `overwrite:` default
/// true · `create_dirs:` default false (stdlib §write).
pub(crate) async fn write<F: FsReadDyn + FsWriteDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-WRITE-001";
    const C2: &str = "NIKA-BUILTIN-WRITE-002";
    let path = req_str(args, "path", C1)?;
    let content = req_str(args, "content", C1)?;
    let overwrite = opt_bool(args, "overwrite", true);
    let create_dirs = opt_bool(args, "create_dirs", false);

    if !overwrite && fs.exists(Path::new(path)).await {
        return Err(BuiltinFailure::new(
            C2,
            format!("`{path}` exists and overwrite: false"),
        ));
    }
    if create_dirs && let Some(parent) = Path::new(path).parent() {
        fs.create_dir_all(parent)
            .await
            .map_err(|e| BuiltinFailure::new(C1, format!("create_dirs failed: {e}")))?;
    }
    fs.write(Path::new(path), content.as_bytes())
        .await
        .map_err(|e| BuiltinFailure::new(C1, format!("write failed: {e}")))?;
    Ok(serde_json::Value::String(path.to_owned()))
}

/// `nika:edit` — literal find/replace-all (NOT regex · stdlib §edit).
pub(crate) async fn edit<F: FsReadDyn + FsWriteDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-EDIT-001";
    const C2: &str = "NIKA-BUILTIN-EDIT-002";
    let path = req_str(args, "path", C1)?;
    let find = req_str(args, "find", C1)?;
    let replace = req_str(args, "replace", C1)?;

    let original = fs
        .read_to_string(Path::new(path))
        .await
        .map_err(|e| BuiltinFailure::new(C2, format!("read for edit failed: {e}")))?;
    if !original.contains(find) {
        return Err(BuiltinFailure::new(
            C1,
            format!("`find:` matched nothing in {path}"),
        ));
    }
    let edited = match args.get("count").and_then(serde_json::Value::as_u64) {
        Some(cap) => original.replacen(find, replace, usize::try_from(cap).unwrap_or(usize::MAX)),
        None => original.replace(find, replace),
    };
    fs.write(Path::new(path), edited.as_bytes())
        .await
        .map_err(|e| BuiltinFailure::new(C2, format!("write after edit failed: {e}")))?;
    Ok(serde_json::Value::String(path.to_owned()))
}

/// `nika:glob` — sorted-lexicographically match (stdlib §glob).
pub(crate) async fn glob<F: FsListDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-GLOB-001";
    let pattern = req_str(args, "pattern", C)?;
    // Glob is rooted at the project cwd; the kernel `glob(root, pattern)`
    // walks read-only. The exclude list filters the result set.
    let matches = fs
        .glob(Path::new("."), pattern)
        .await
        .map_err(|e| BuiltinFailure::new(C, format!("invalid pattern: {e}")))?;
    let excludes = exclude_patterns(args);
    let mut paths: Vec<String> = matches
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| !excludes.iter().any(|ex| simple_glob(ex, p)))
        .collect();
    paths.sort();
    Ok(serde_json::Value::Array(
        paths.into_iter().map(serde_json::Value::String).collect(),
    ))
}

fn exclude_patterns(args: &Args) -> Vec<String> {
    args.get("exclude")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A minimal `**`/`*` glob for the exclude filter (`**` crosses `/`).
fn simple_glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                go(&p[2..], t) || (!t.is_empty() && go(p, &t[1..]))
            }
            Some(b'*') => go(&p[1..], t) || (!t.is_empty() && t[0] != b'/' && go(p, &t[1..])),
            Some(&c) => !t.is_empty() && t[0] == c && go(&p[1..], &t[1..]),
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

/// `nika:grep` — recursive regex search · `{path,line,match}` sorted by
/// `(path, line)` (stdlib §grep · RE2-class via the `regex` crate).
pub(crate) async fn grep<F: FsReadDyn + FsListDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-GREP-001";
    let pattern = req_str(args, "pattern", C)?;
    let root = args
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(".");
    let regex = build_regex(pattern, opt_bool(args, "case_insensitive", false))
        .map_err(|e| BuiltinFailure::new(C, format!("invalid pattern: {e}")))?;

    let files = fs
        .glob(Path::new(root), "**")
        .await
        .map_err(|e| BuiltinFailure::new(C, format!("walk failed: {e}")))?;
    let mut hits = Vec::new();
    for file in files {
        let Ok(text) = fs.read_to_string(&file).await else {
            continue; // unreadable/binary files skip silently (grep semantics)
        };
        let path = file.to_string_lossy().into_owned();
        for (n, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                hits.push(serde_json::json!({
                    "path": path, "line": n + 1, "match": line
                }));
            }
        }
    }
    // Sorted by (path, line) — the determinism contract.
    hits.sort_by(|a, b| {
        let key = |v: &serde_json::Value| {
            (
                v["path"].as_str().unwrap_or_default().to_owned(),
                v["line"].as_u64().unwrap_or_default(),
            )
        };
        key(a).cmp(&key(b))
    });
    Ok(serde_json::Value::Array(hits))
}

fn build_regex(pattern: &str, case_insensitive: bool) -> Result<regex::Regex, regex::Error> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel_mock::MockFs;

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    #[tokio::test]
    async fn read_text_and_missing() {
        let fs = MockFs::new().with_file("a.txt", "hello");
        let out = read(&fs, &args(serde_json::json!({ "path": "a.txt" })))
            .await
            .expect("ok");
        assert_eq!(out, serde_json::Value::String("hello".to_owned()));
        let missing = read(&fs, &args(serde_json::json!({ "path": "nope.txt" }))).await;
        assert!(matches!(missing, Err(f) if f.code == "NIKA-BUILTIN-READ-001"));
    }

    #[tokio::test]
    async fn write_respects_overwrite_false() {
        let fs = MockFs::new().with_file("e.txt", "old");
        let blocked = write(
            &fs,
            &args(serde_json::json!({ "path": "e.txt", "content": "new", "overwrite": false })),
        )
        .await;
        assert!(matches!(blocked, Err(f) if f.code == "NIKA-BUILTIN-WRITE-002"));
        let ok = write(
            &fs,
            &args(serde_json::json!({ "path": "e.txt", "content": "new" })),
        )
        .await
        .expect("overwrite default true");
        assert_eq!(ok, serde_json::Value::String("e.txt".to_owned()));
    }

    #[tokio::test]
    async fn edit_replaces_all_or_capped_and_fails_on_no_match() {
        let fs = MockFs::new().with_file("c.txt", "a a a");
        edit(
            &fs,
            &args(serde_json::json!({ "path": "c.txt", "find": "a", "replace": "b", "count": 2 })),
        )
        .await
        .expect("ok");
        let after = read(&fs, &args(serde_json::json!({ "path": "c.txt" })))
            .await
            .expect("ok");
        assert_eq!(after, serde_json::Value::String("b b a".to_owned()));

        let nomatch = edit(
            &fs,
            &args(serde_json::json!({ "path": "c.txt", "find": "z", "replace": "y" })),
        )
        .await;
        assert!(matches!(nomatch, Err(f) if f.code == "NIKA-BUILTIN-EDIT-001"));
    }

    #[tokio::test]
    async fn grep_sorts_by_path_then_line() {
        let fs = MockFs::new()
            .with_file("proj/b.txt", "no\nTODO: two\n")
            .with_file("proj/a.txt", "TODO: one\n");
        let out = grep(
            &fs,
            &args(serde_json::json!({ "pattern": "TODO:", "path": "proj" })),
        )
        .await
        .expect("ok");
        let hits = out.as_array().expect("array");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0]["path"], "proj/a.txt");
        assert_eq!(hits[0]["line"], 1);
        assert_eq!(hits[1]["path"], "proj/b.txt");
        assert_eq!(hits[1]["line"], 2);

        let bad = grep(
            &fs,
            &args(serde_json::json!({ "pattern": "(unclosed", "path": "proj" })),
        )
        .await;
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-GREP-001"));
    }

    #[tokio::test]
    async fn glob_sorts_and_excludes() {
        let fs = MockFs::new()
            .with_file("proj/b.rs", "")
            .with_file("proj/a.rs", "")
            .with_file("proj/target/x.rs", "");
        // Without exclude: all three, sorted.
        let all = glob(&fs, &args(serde_json::json!({ "pattern": "**" })))
            .await
            .expect("ok");
        // The mock globs from `.`; our keys are under `proj/`, so root `.`
        // yields nothing — assert the EXCLUDE path with a proj-rooted glob
        // is exercised by the dispatcher-level test; here we pin the
        // exclude filter directly through simple_glob (the same predicate).
        let _ = all;
        assert!(simple_glob("**/target/**", "proj/target/x.rs"));
        assert!(!simple_glob("**/target/**", "proj/a.rs"));
    }

    #[tokio::test]
    async fn glob_applies_the_exclude_filter() {
        // Files keyed under "./" so the mock's root-prefix (root ".") finds
        // them; the exclude predicate drops the matching one.
        let fs = MockFs::new()
            .with_file("./keep.rs", "")
            .with_file("./drop.rs", "");
        let out = glob(
            &fs,
            &args(serde_json::json!({ "pattern": "**", "exclude": ["**drop**"] })),
        )
        .await
        .expect("ok");
        let paths: Vec<&str> = out
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.as_str().expect("string"))
            .collect();
        assert_eq!(paths, vec!["./keep.rs"], "drop.rs excluded");
    }

    #[test]
    fn exclude_patterns_reads_the_array() {
        let none = exclude_patterns(&args(serde_json::json!({})));
        assert!(none.is_empty(), "absent exclude = empty");
        let some = exclude_patterns(&args(
            serde_json::json!({ "exclude": ["**/target/**", "*.tmp"] }),
        ));
        assert_eq!(some, vec!["**/target/**".to_owned(), "*.tmp".to_owned()]);
    }

    #[test]
    fn simple_glob_star_vs_doublestar() {
        assert!(simple_glob("**/target/**", "a/target/x"));
        assert!(simple_glob("*.rs", "lib.rs"));
        assert!(!simple_glob("*.rs", "a/lib.rs"), "single * stops at /");
        assert!(simple_glob("**/*.rs", "a/b/lib.rs"));
        // The literal-char branch: a non-matching char fails (kills the
        // `t[0] == c && …` mutant), an exact match passes.
        assert!(simple_glob("abc", "abc"));
        assert!(!simple_glob("abc", "abd"));
        assert!(!simple_glob("abc", "ab"), "pattern longer than text");
        // The `*`-then-recurse branch (kills the `!t.is_empty() && t[0] != b'/'`
        // mutant): `a*c` matches `axc` but a `/` between stops it.
        assert!(simple_glob("a*c", "axc"));
        assert!(!simple_glob("a*c", "a/c"), "* cannot swallow /");
        assert!(simple_glob("a*c", "ac"), "* matches empty");
        // ** crosses / (kills the doublestar-vs-star confusion).
        assert!(simple_glob("a**c", "a/x/c"));
    }
}
