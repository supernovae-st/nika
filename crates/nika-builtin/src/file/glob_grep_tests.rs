// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The glob and grep tests of the file builtins — carved out of `file.rs` when
//! the left-out-directory report took the file past the 1500-line wall. A
//! child module of `file`, so the private helpers (`dropped_warning` ·
//! `split_pattern_root` · `simple_glob` · …) are reachable through `super::*`.

use nika_kernel_mock::MockFs;

use super::*;

/// The value-only door the glob tests read (the report is pinned through
/// the dispatcher, on a real tree).
async fn glob<F: FsListDyn + FsMetaDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    glob_reported(fs, args).await.map(|globbed| globbed.paths)
}

fn args(v: serde_json::Value) -> Args {
    match v {
        serde_json::Value::Object(map) => map,
        other => panic!("test arg must be an object, got {other}"),
    }
}

#[tokio::test]
async fn grep_sorts_by_path_then_line() {
    let fs = MockFs::new()
        .with_file("proj/b.txt", "no\nTODO: two\n")
        .with_file("proj/a.txt", "TODO: one\n");
    let out = grep(
        &fs,
        &FsBoundary::unbounded(),
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
        &FsBoundary::unbounded(),
        &args(serde_json::json!({ "pattern": "(unclosed", "path": "proj" })),
    )
    .await;
    assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-GREP-001"));
}

#[test]
fn dropped_warning_names_five_and_counts_the_rest() {
    assert!(
        dropped_warning("*.md", &[]).is_none(),
        "nothing left out, nothing said"
    );
    let one =
        dropped_warning("./items/*.md", &["./items/item-07.md".to_owned()]).expect("one directory");
    assert_eq!(
        one,
        "nika:glob returns files only · 1 directory also matched `./items/*.md` and was left out: ./items/item-07.md"
    );
    let many: Vec<String> = (1..=7).map(|i| format!("./d/{i}.md")).collect();
    let seven = dropped_warning("./d/*.md", &many).expect("seven directories");
    assert!(seven.starts_with("nika:glob returns files only · 7 directories also matched `./d/*.md` and were left out: ./d/1.md, ./d/2.md, ./d/3.md, ./d/4.md, ./d/5.md (+2 more)"), "{seven}");
    assert!(
        !seven.contains("./d/6.md"),
        "the sixth is counted, not named: {seven}"
    );
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

#[tokio::test]
async fn glob_exclude_accepts_a_bare_string() {
    // The Finding #5 fix: a single-pattern `exclude:` string (the
    // natural one-pattern form) was silently dropped when only arrays
    // were read.
    let fs = MockFs::new()
        .with_file("./keep.rs", "")
        .with_file("./drop.rs", "");
    let out = glob(
        &fs,
        &args(serde_json::json!({ "pattern": "**", "exclude": "**drop**" })),
    )
    .await
    .expect("ok");
    let paths: Vec<&str> = out
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    assert_eq!(paths, vec!["./keep.rs"], "string exclude drops drop.rs");
}

#[tokio::test]
async fn glob_matches_an_absolute_pattern() {
    // F2 · the silent-empty footgun: an absolute pattern matching an
    // existing file used to return `[]` (the kernel matches the pattern
    // against cwd-relative paths · an absolute one never matched). Now
    // it re-roots at the literal dir prefix and matches.
    let fs = MockFs::new()
        .with_file("/tmp/x/file.txt", "")
        .with_file("/tmp/x/other.md", "")
        .with_file("/tmp/y/elsewhere.txt", "");

    // An exact absolute file path returns it.
    let exact = glob(
        &fs,
        &args(serde_json::json!({ "pattern": "/tmp/x/file.txt" })),
    )
    .await
    .expect("ok");
    assert_eq!(
        exact,
        serde_json::json!(["/tmp/x/file.txt"]),
        "an absolute file pattern matches it (was silently [])"
    );

    // An absolute glob pattern matches under its root only.
    let starred = glob(&fs, &args(serde_json::json!({ "pattern": "/tmp/x/*.txt" })))
        .await
        .expect("ok");
    assert_eq!(
        starred,
        serde_json::json!(["/tmp/x/file.txt"]),
        "absolute *.txt under /tmp/x — not /tmp/y"
    );

    // An absolute `**` recurses under its root.
    let recursive = glob(&fs, &args(serde_json::json!({ "pattern": "/tmp/**" })))
        .await
        .expect("ok");
    assert_eq!(
        recursive,
        serde_json::json!(["/tmp/x/file.txt", "/tmp/x/other.md", "/tmp/y/elsewhere.txt"]),
        "absolute /tmp/** spans both subtrees"
    );

    // An absolute no-match is a clean [] (not an error).
    let empty = glob(
        &fs,
        &args(serde_json::json!({ "pattern": "/tmp/x/nope.zip" })),
    )
    .await
    .expect("ok");
    assert_eq!(empty, serde_json::json!([]), "absolute no-match is []");
}

#[tokio::test]
async fn glob_relative_patterns_still_work() {
    // The F2 fix must not regress the relative (cwd-rooted) path.
    let fs = MockFs::new()
        .with_file("./a.rs", "")
        .with_file("./b.rs", "");
    let out = glob(&fs, &args(serde_json::json!({ "pattern": "**" })))
        .await
        .expect("ok");
    assert_eq!(out, serde_json::json!(["./a.rs", "./b.rs"]));
}

#[test]
fn split_pattern_root_relative_vs_absolute() {
    // Relative → the literal directory prefix, `./`-prefixed (the walk
    // AND the permits gate anchor at the directory the author named —
    // a scoped boundary accepts a scoped glob) · returned paths keep
    // the historical `./…` byte shape.
    assert_eq!(
        split_pattern_root("src/**/*.rs"),
        ("./src".into(), "**/*.rs")
    );
    assert_eq!(split_pattern_root("**"), (".".into(), "**"));
    assert_eq!(
        split_pattern_root("hiring/inbox/*.md"),
        ("./hiring/inbox".into(), "*.md")
    );
    // A leading `./` is stripped from the MATCH pattern — the walker
    // matches root-relative paths (no `./`), so `./**/*.rs` MUST behave
    // as `**/*.rs` (the spec example uses the `./` form; keeping it
    // returned a silent empty match).
    assert_eq!(split_pattern_root("./**/*.rs"), (".".into(), "**/*.rs"));
    assert_eq!(split_pattern_root("./src/*.rs"), ("./src".into(), "*.rs"));
    assert_eq!(split_pattern_root("./file.txt"), (".".into(), "file.txt"));
    // No meta char at all — the whole relative path is literal: walk its
    // parent, match its name.
    assert_eq!(
        split_pattern_root("docs/guide.md"),
        ("./docs".into(), "guide.md")
    );
    // Absolute exact file → the parent dir + the file name.
    assert_eq!(
        split_pattern_root("/tmp/x/file.txt"),
        ("/tmp/x".into(), "file.txt")
    );
    // Absolute with a meta char → split at the last `/` before it.
    assert_eq!(
        split_pattern_root("/data/**/*.rs"),
        ("/data".into(), "**/*.rs")
    );
    assert_eq!(split_pattern_root("/var/*.log"), ("/var".into(), "*.log"));
    // A meta char in the first segment after root → walk from `/`.
    assert_eq!(split_pattern_root("/*.txt"), ("/".into(), "*.txt"));
    // A bare absolute file under root → (`/`, name).
    assert_eq!(split_pattern_root("/hosts"), ("/".into(), "hosts"));
    // The public root accessor agrees.
    assert_eq!(glob_walk_root("/tmp/x/file.txt"), "/tmp/x");
    assert_eq!(glob_walk_root("rel/**"), "./rel");
}

#[test]
fn exclude_patterns_reads_string_or_array() {
    let none = exclude_patterns(&args(serde_json::json!({})));
    assert!(none.is_empty(), "absent exclude = empty");
    let one = exclude_patterns(&args(serde_json::json!({ "exclude": "**/target/**" })));
    assert_eq!(
        one,
        vec!["**/target/**".to_owned()],
        "bare string → one pattern"
    );
    let many = exclude_patterns(&args(
        serde_json::json!({ "exclude": ["**/target/**", "*.tmp"] }),
    ));
    assert_eq!(many, vec!["**/target/**".to_owned(), "*.tmp".to_owned()]);
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
async fn grep_on_a_file_path_names_the_directory_contract() {
    // The low-priority DX fix: grep is a recursive DIRECTORY walk · a
    // `path:` naming a FILE used to surface the cryptic "Not a directory
    // (os error 20)". It now names the real contract. Proven on the REAL
    // fs (MockFs is a HashMap · never raises ENOTDIR).
    use nika_fs::TokioFs;
    let dir = std::env::temp_dir().join(format!(
        "nika-grep-enotdir-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    let file = dir.join("a.txt");
    std::fs::write(&file, b"hello\n").expect("write");
    let out = grep(
        &TokioFs,
        &FsBoundary::unbounded(),
        &args(serde_json::json!({
            "pattern": "hello", "path": file.to_string_lossy()
        })),
    )
    .await;
    assert!(
        matches!(&out, Err(f) if f.code == "NIKA-BUILTIN-GREP-001"
            && f.message.contains("must be a directory")),
        "grep on a file names the directory contract: {out:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
