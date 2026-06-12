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
/// true · `create_dirs:` default false (stdlib §write). A BINARY
/// `content:` value — the opaque-bytes object an upstream tool produced
/// (`nika:read binary: true` → `{ bytes_base64, len }`) — is written
/// as-is (builtins-v0.1.md:130 · the value carries its own type), so
/// read→write round-trips bytes without a decoder step in the workflow.
pub(crate) async fn write<F: FsReadDyn + FsWriteDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-WRITE-001";
    const C2: &str = "NIKA-BUILTIN-WRITE-002";
    let path = req_str(args, "path", C1)?;
    let content = write_content(args, C1)?;
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
    fs.write(Path::new(path), &content)
        .await
        .map_err(|e| BuiltinFailure::new(C1, format!("write failed: {e}")))?;
    Ok(serde_json::Value::String(path.to_owned()))
}

/// Resolve `content:` to bytes — text (string) verbatim, or the binary
/// pass-through object. Any OTHER shape is loud (numbers/arrays don't
/// silently stringify; the binary object with a corrupt payload names
/// the corruption).
fn write_content(args: &Args, code: &'static str) -> Result<Vec<u8>, BuiltinFailure> {
    match args.get("content") {
        Some(serde_json::Value::String(text)) => Ok(text.clone().into_bytes()),
        Some(serde_json::Value::Object(obj)) if obj.contains_key("bytes_base64") => {
            let encoded = obj
                .get("bytes_base64")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    BuiltinFailure::new(code, "`content.bytes_base64` must be a string")
                })?;
            crate::data::base64_decode(encoded)
                .map_err(|e| BuiltinFailure::new(code, format!("binary `content:` corrupt: {e}")))
        }
        Some(other) => Err(BuiltinFailure::new(
            code,
            format!(
                "`content:` must be a string or a binary `{{ bytes_base64 }}` object (got {other})"
            ),
        )),
        None => Err(BuiltinFailure::new(code, "`content:` (string) is required")),
    }
}

/// `nika:edit` — literal find/replace-all (NOT regex · stdlib §edit).
///
/// Read-modify-write: the kernel `write` is atomic per-write (temp +
/// rename), but concurrent edits of the SAME path can lose an update —
/// the engine's DAG ordering is the serialization layer for that.
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
        Some(cap) => {
            let cap = usize::try_from(cap)
                .map_err(|_| BuiltinFailure::new(C1, "`count:` is out of range"))?;
            original.replacen(find, replace, cap)
        }
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

/// A minimal `**`/`*` glob for the exclude filter (`**` crosses `/`,
/// `*` stops at `/`). Iterative position-set DP — polynomial worst-case
/// and zero recursion, so a model-supplied `exclude:` can't trigger the
/// exponential backtracking / stack blowup a naive recursive matcher
/// invites.
fn simple_glob(pattern: &str, text: &str) -> bool {
    enum Tok {
        /// `**` — matches any run, including `/`.
        Any,
        /// `*` — matches any run of non-`/` bytes.
        Seg,
        /// One literal byte.
        Ch(u8),
    }
    let p = pattern.as_bytes();
    let mut toks = Vec::with_capacity(p.len());
    let mut i = 0;
    while i < p.len() {
        if p[i] == b'*' {
            if p.get(i + 1) == Some(&b'*') {
                toks.push(Tok::Any);
                i += 2;
            } else {
                toks.push(Tok::Seg);
                i += 1;
            }
        } else {
            toks.push(Tok::Ch(p[i]));
            i += 1;
        }
    }
    let t = text.as_bytes();
    // reached[j] = the tokens consumed so far can match text[..j].
    let mut reached = vec![false; t.len() + 1];
    reached[0] = true;
    for tok in toks {
        let mut next = vec![false; t.len() + 1];
        match tok {
            Tok::Ch(c) => {
                for j in 0..t.len() {
                    if reached[j] && t[j] == c {
                        next[j + 1] = true;
                    }
                }
            }
            Tok::Seg => {
                for j in 0..=t.len() {
                    if reached[j] {
                        next[j] = true;
                        let mut k = j;
                        while k < t.len() && t[k] != b'/' {
                            k += 1;
                            next[k] = true;
                        }
                    }
                }
            }
            Tok::Any => {
                if let Some(first) = reached.iter().position(|&r| r) {
                    for slot in &mut next[first..] {
                        *slot = true;
                    }
                }
            }
        }
        reached = next;
    }
    reached[t.len()]
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
        // The walk yields directories (EISDIR), binary files (InvalidData),
        // raced deletions (NotFound) and unreadable entries alike — grep
        // semantics skip them all (`grep -rs`): the spec allocates only
        // GREP-001 (invalid pattern) and no partial-result warning channel
        // exists at v0.1, so the skip is deliberate and total.
        let Ok(text) = fs.read_to_string(&file).await else {
            continue;
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
    async fn write_binary_content_round_trips_read_binary() {
        // The spec clause (builtins-v0.1.md:130): a binary content value
        // from an upstream tool is written AS-IS — read binary:true →
        // write → read binary:true must round-trip the exact bytes with
        // no decoder step in the workflow.
        let payload: Vec<u8> = vec![0x00, 0xff, 0x7f, 0x80, 0x0a, 0xfe];
        let fs = MockFs::new().with_file("blob.bin", payload.clone());
        let read_out = read(
            &fs,
            &args(serde_json::json!({ "path": "blob.bin", "binary": true })),
        )
        .await
        .expect("binary read");
        // Feed the read's OWN output object straight into write.
        let wrote = write(
            &fs,
            &args(serde_json::json!({ "path": "copy.bin", "content": read_out })),
        )
        .await
        .expect("binary write");
        assert_eq!(wrote, serde_json::Value::String("copy.bin".to_owned()));
        let back = read(
            &fs,
            &args(serde_json::json!({ "path": "copy.bin", "binary": true })),
        )
        .await
        .expect("re-read");
        assert_eq!(
            back["len"],
            serde_json::json!(payload.len()),
            "byte-exact round-trip"
        );
        assert_eq!(back["bytes_base64"], crate::data::base64_encode(&payload));

        // Corrupt payload is loud, not a silent garbage write.
        let corrupt = write(
            &fs,
            &args(serde_json::json!({
                "path": "x.bin", "content": { "bytes_base64": "not valid!" }
            })),
        )
        .await;
        assert!(
            matches!(&corrupt, Err(f) if f.code == "NIKA-BUILTIN-WRITE-001"
                && f.message.contains("corrupt")),
            "{corrupt:?}"
        );
        // A non-string/non-binary shape is loud too (numbers never
        // silently stringify).
        let wrong = write(
            &fs,
            &args(serde_json::json!({ "path": "y.txt", "content": 42 })),
        )
        .await;
        assert!(
            matches!(&wrong, Err(f) if f.code == "NIKA-BUILTIN-WRITE-001"
                && f.message.contains("must be a string or a binary")),
            "{wrong:?}"
        );
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
        // Files keyed under "./" so the mock's root `.` finds them.
        let fs = MockFs::new()
            .with_file("./b.rs", "")
            .with_file("./a.rs", "")
            .with_file("./target/x.rs", "");
        let all = glob(&fs, &args(serde_json::json!({ "pattern": "**" })))
            .await
            .expect("ok");
        let names: Vec<&str> = all
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.as_str().expect("string"))
            .collect();
        assert_eq!(
            names,
            vec!["./a.rs", "./b.rs", "./target/x.rs"],
            "sorted lexicographically"
        );
        // The exclude predicate over the same tree drops target/.
        let filtered = glob(
            &fs,
            &args(serde_json::json!({ "pattern": "**", "exclude": ["**/target/**"] })),
        )
        .await
        .expect("ok");
        let kept: Vec<&str> = filtered
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.as_str().expect("string"))
            .collect();
        assert_eq!(kept, vec!["./a.rs", "./b.rs"]);
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
        // Empty pattern matches only empty text.
        assert!(simple_glob("", ""));
        assert!(!simple_glob("", "x"));
        // Trailing-star edge: matches through to the end.
        assert!(simple_glob("a*", "abc"));
        assert!(simple_glob("a**", "a/b/c"));
    }

    // ── Gate 6 · property test (crate spec §5 · glob determinism) ───────

    /// The pre-DP recursive matcher — the semantic REFERENCE the
    /// iterative rewrite must agree with (kept test-only; exponential
    /// on adversarial input, harmless at proptest's small bounds).
    fn naive_glob(p: &[u8], t: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                naive_glob(&p[2..], t) || (!t.is_empty() && naive_glob(p, &t[1..]))
            }
            Some(b'*') => {
                naive_glob(&p[1..], t) || (!t.is_empty() && t[0] != b'/' && naive_glob(p, &t[1..]))
            }
            Some(&c) => !t.is_empty() && t[0] == c && naive_glob(&p[1..], &t[1..]),
        }
    }

    proptest::proptest! {
        /// The DP matcher is EXTENSIONALLY EQUAL to the recursive
        /// reference over the full small-input space (both star forms ·
        /// separators · literals).
        #[test]
        fn dp_glob_agrees_with_the_recursive_reference(
            pattern in "[ab/*]{0,8}",
            text in "[ab/]{0,10}",
        ) {
            proptest::prop_assert_eq!(
                simple_glob(&pattern, &text),
                naive_glob(pattern.as_bytes(), text.as_bytes()),
                "pattern={:?} text={:?}", pattern, text
            );
        }
    }

    #[test]
    fn simple_glob_is_polynomial_on_adversarial_patterns() {
        // The classic exponential-backtracking killer: many stars against
        // a long non-matching text. COMPLETION is the proof — a
        // backtracking matcher faces ~2^12 branch points over 2 000
        // chars (≈10³⁶ operations · never finishes), the DP answers in
        // ~52k cells. No wall-clock assertion: timing oracles trip under
        // full-workspace CPU contention (observed: this test red in the
        // 34-crate run, green in isolation) while the algorithmic
        // property they meant to pin is load-independent — the test
        // harness timeout is the hang backstop.
        let pattern = "*a*a*a*a*a*a*a*a*a*a*a*a*b";
        let text = "a".repeat(2_000);
        assert!(!simple_glob(pattern, &text));
        // A leading ** against a long text — linear-ish, no stack risk.
        let long = "x".repeat(100_000);
        assert!(!simple_glob("**Y", &long));
        assert!(simple_glob("**x", &long));
    }

    #[tokio::test]
    async fn write_path_gating_is_the_policy_layer_job() {
        // CANARY (crate spec §4 honest gap): this layer passes `path:`
        // verbatim to the fs seam — CWD confinement / traversal rejection
        // is `nika-policy`'s contract (L1.5 · design locked, impl gated).
        // When policy lands between the verbs and this dispatcher, THIS
        // pin flips to an expect-reject — until then the delegation is
        // explicit, not accidental.
        let fs = MockFs::new();
        let out = write(
            &fs,
            &args(serde_json::json!({ "path": "../escape.txt", "content": "x" })),
        )
        .await
        .expect("delegated today");
        assert_eq!(out, serde_json::Value::String("../escape.txt".to_owned()));
    }
}
