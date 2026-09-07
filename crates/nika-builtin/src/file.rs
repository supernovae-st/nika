// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! File builtins (5) — read · write · edit · glob · grep (stdlib §File).
//! Each composes the injected kernel `Fs*Dyn` seams.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_kernel::io::fs::{FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};

use crate::permits::{FsAccess, FsBoundary};
use crate::{Args, BuiltinFailure, BuiltinOutcome, req_str, strict_bool, strict_u64};

#[cfg(test)]
mod glob_grep_tests;
#[cfg(test)]
mod write_tests;

/// `nika:read` — text (default) or binary. Returns the file content.
pub(crate) async fn read<F: FsReadDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-READ-001";
    const C3: &str = "NIKA-BUILTIN-READ-003";
    let path = req_str(args, "path", C1)?;
    if strict_bool(args, "binary", false, C1)? {
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
/// `path:` names a FILE, never a directory · a directory-shaped path is a
/// coded refusal that teaches the file-inside form (see `directory_refusal`).
pub(crate) async fn write<F: FsReadDyn + FsWriteDyn + FsMetaDyn>(
    fs: &F,
    args: &Args,
) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-WRITE-001";
    const C2: &str = "NIKA-BUILTIN-WRITE-002";
    let path = req_str(args, "path", C1)?;
    let content = write_content(args, C1)?;
    let overwrite = strict_bool(args, "overwrite", true, C1)?;
    let create_dirs = strict_bool(args, "create_dirs", false, C1)?;

    // `nika:write` writes FILES. A path that NAMES a directory — a trailing
    // separator (`out/replies/`), a `.`/`..` last component, or a name that
    // already IS a directory — used to reach the seam and come back as
    // `path not found` then `Is a directory (os error 21)`: neither says what
    // to do. Measured on Harness-Bench 005 (0.118.7 · gpt-4o-mini): 8 failed
    // writes over 6 of the agent's 11 turns. Refuse BEFORE any effect and
    // teach the file-inside form · the refusal never creates the directory
    // (no new effect class), it only names the one the author meant.
    if names_a_directory(path) || is_existing_dir(fs, path).await {
        return Err(directory_refusal(C1, path));
    }
    // The `Not a directory (os error 20)` half: a FILE stands where a parent
    // directory is needed. Measured on Harness-Bench 005 with the two teaches
    // above in place (gpt-4o-mini): the model "pre-created" `out/replies`
    // with an empty `nika:write`, then every `out/replies/<name>` write died
    // `path already exists: out/replies` — the seam's parent creation, not
    // the cause — for the rest of the run. Name the file, its size, and the
    // fact that nothing here deletes it.
    if let Some((blocker, len)) = file_in_the_way(fs, path).await {
        return Err(file_in_the_way_refusal(C1, path, &blocker, len));
    }

    if !overwrite && fs.exists(Path::new(path)).await {
        return Err(BuiltinFailure::new(
            C2,
            format!("`{path}` exists and overwrite: false"),
        ));
    }
    // `create_dirs:` is a load-bearing SAFETY arg: false (the default) means
    // "do not scatter directories — fail if the parent is missing" (a typo'd
    // path should surface, not silently materialize a tree). The atomic-write
    // seam auto-creates the parent unconditionally to place its temp sibling,
    // so honouring `create_dirs: false` requires the builtin to gate the write
    // BEFORE the seam ever runs. An empty parent (bare filename → cwd) always
    // exists, so it is skipped.
    if let Some(parent) = Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        if create_dirs {
            fs.create_dir_all(parent)
                .await
                .map_err(|e| BuiltinFailure::new(C1, format!("create_dirs failed: {e}")))?;
        } else if !fs.exists(parent).await {
            return Err(BuiltinFailure::new(
                C1,
                format!(
                    "parent directory `{}` does not exist — pass `create_dirs: true` to create it",
                    parent.display()
                ),
            ));
        }
    }
    let written = if overwrite {
        fs.write(Path::new(path), &content).await
    } else {
        fs.write_new(Path::new(path), &content).await
    };
    written.map_err(|e| match e {
        FsError::AlreadyExists { .. } if !overwrite => {
            BuiltinFailure::new(C2, format!("`{path}` exists and overwrite: false"))
        }
        other => BuiltinFailure::new(C1, format!("write failed: {other}")),
    })?;
    Ok(serde_json::Value::String(path.to_owned()))
}

/// A path whose last component names a DIRECTORY, not a file. `Path`
/// normalizes a trailing separator away (`Path::new("a/b/").file_name()` is
/// `Some("b")`), so the raw string is the only witness of `out/replies/`;
/// `file_name() == None` catches the rest (`.` · `..` · `/` · `a/..`). An
/// empty path is left to the seam — it is not this teach's shape.
fn names_a_directory(path: &str) -> bool {
    let Some(last) = path.chars().next_back() else {
        return false;
    };
    last == '/' || last == std::path::MAIN_SEPARATOR || Path::new(path).file_name().is_none()
}

/// The `Is a directory (os error 21)` half: the NAME already exists and is a
/// directory. Any other verdict (absent · a regular file · an unreadable
/// stat) falls through, so the write's own error keeps its meaning.
async fn is_existing_dir<F: FsMetaDyn>(fs: &F, path: &str) -> bool {
    matches!(fs.metadata(Path::new(path)).await, Ok(meta) if meta.is_dir)
}

/// The nearest EXISTING ancestor of `path` that is not a directory — a
/// regular file (or anything else that is not a directory) standing where
/// the parent chain needs a directory. Walks upward from the immediate
/// parent; the first ancestor that exists decides: a directory means the
/// chain is sound (`None`), anything else is the blocker. Absent ancestors
/// are skipped (they are `create_dirs:`' business); an empty parent (a bare
/// filename → cwd) is never a candidate.
async fn file_in_the_way<F: FsMetaDyn>(fs: &F, path: &str) -> Option<(String, u64)> {
    let ancestors = Path::new(path)
        .ancestors()
        .skip(1)
        .filter(|p| !p.as_os_str().is_empty());
    for ancestor in ancestors {
        match fs.metadata(ancestor).await {
            Ok(meta) if meta.is_dir => return None,
            Ok(meta) => return Some((ancestor.display().to_string(), meta.len)),
            Err(_) => {}
        }
    }
    None
}

/// The refusal for a file in the parent chain · it names the blocker, its
/// size, the fact that `nika:write` never deletes, and the mistake that
/// usually put it there (an empty write meant as a `mkdir`).
fn file_in_the_way_refusal(
    code: &'static str,
    path: &str,
    blocker: &str,
    len: u64,
) -> BuiltinFailure {
    BuiltinFailure::new(
        code,
        format!(
            "`{blocker}` is a file ({len} bytes), not a directory — `{path}` cannot be \
             created beneath it, and `nika:write` never deletes: write the file under \
             another directory, or have the workflow's author remove `{blocker}` first. \
             (An empty `nika:write` at a directory's path creates a FILE there, never \
             the directory — write the first file inside it with `create_dirs: true` \
             instead.)"
        ),
    )
}

/// The teaching refusal · it names the path, the shape, and the exact call
/// that works (an agent reads this and its next turn is the right one).
fn directory_refusal(code: &'static str, path: &str) -> BuiltinFailure {
    let inside = path.trim_end_matches(['/', std::path::MAIN_SEPARATOR]);
    BuiltinFailure::new(
        code,
        format!(
            "`{path}` names a directory, not a file — `nika:write` writes files. \
             Write the file inside it (`{inside}/<name>`) with `create_dirs: true` \
             and the directory is created."
        ),
    )
}

/// Resolve `content:` to bytes — a string is written VERBATIM; the binary
/// pass-through object (`{ bytes_base64 }`) is decoded to its raw bytes; ANY
/// OTHER JSON value (array · object · number · bool · null) serializes to
/// canonical JSON. Because a string is verbatim, a JSON string you built by
/// hand (e.g. `nika:jq … | tojson`) is NOT double-encoded — the two paths
/// produce identical bytes.
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
        // `null` is almost always a missing upstream value (an unset var, a
        // skipped task) — silently writing the bytes `null` is the silent
        // data-corruption the no-coercion ethos forbids. Loud, with the
        // escape hatch named (write the literal string "null" verbatim).
        Some(serde_json::Value::Null) => Err(BuiltinFailure::new(
            code,
            "`content:` is null — usually a missing upstream value; pass the \
             string \"null\" to write that literal text",
        )),
        // Any other JSON value serializes to canonical JSON: "write
        // structured data to a .json file" is the one step it reads as (jq
        // and every config language serialize by default), resolved here
        // instead of surfacing a runtime type failure that `check` cannot
        // see statically (jq output types are not inferable).
        Some(other) => serde_json::to_vec(other)
            .map_err(|e| BuiltinFailure::new(code, format!("`content:` is not serializable: {e}"))),
        None => Err(BuiltinFailure::new(code, "`content:` is required")),
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
    // `count:` is a strict optional integer — a present non-integer (the
    // `count: "3"` string slip) is a LOUD error, never silently the "replace
    // ALL" default (which would over-edit the file · the intent-inversion
    // footgun class, sibling of overwrite/has_header).
    let edited = match strict_u64(args, "count", C1)? {
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

/// A `nika:glob` result: the file list (the value a consumer receives —
/// unchanged, forever) beside the OBS-E `warning` naming the directory
/// matches the walk left out, when there were any.
pub(crate) struct Globbed {
    pub(crate) paths: serde_json::Value,
    pub(crate) warning: Option<String>,
}

/// `nika:glob` — sorted-lexicographically match (stdlib §glob), with the
/// report of what the match set does NOT carry ([`dropped_directories`]).
pub(crate) async fn glob_reported<F: FsReadDyn + FsListDyn + FsMetaDyn>(
    fs: &F,
    args: &Args,
) -> Result<Globbed, BuiltinFailure> {
    const C: &str = "NIKA-BUILTIN-GLOB-001";
    let pattern = req_str(args, "pattern", C)?;
    // The kernel `glob(root, pattern)` matches `pattern` against the
    // root-RELATIVE path of each entry. Every pattern is split into its
    // longest LITERAL directory prefix (the walk root) + the relative
    // remainder: an ABSOLUTE pattern because a cwd-relative match can never
    // see it (the F2 footgun), a RELATIVE one so the walk — and the permits
    // gate that fences it — anchors at the directory the author actually
    // named (`hiring/inbox/*.md` walks `./hiring/inbox`, not the whole cwd:
    // a scoped `permits.fs.read` boundary must accept a scoped glob).
    let (root, rel_pattern) = split_pattern_root(pattern);
    let root = Path::new(root.as_ref());
    let matches = match fs.glob(root, rel_pattern).await {
        Ok(matches) => matches,
        // A missing walk root means the match set is empty, not an error —
        // the historical cwd-walk contract (`[]` for `gone-dir/*.md`), now
        // uniform across relative AND absolute patterns.
        Err(nika_kernel::fs::FsError::NotFound { .. }) => Vec::new(),
        Err(e) => return Err(BuiltinFailure::new(C, format!("invalid pattern: {e}"))),
    };
    let excludes = exclude_patterns(args);
    let silenced = |p: &String| excludes.iter().any(|ex| simple_glob(ex, p));
    let walked: Vec<String> = matches
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    // The report reads the walker's RAW set (an excluded file is still a
    // file the walker returned), and an author's `exclude:` silences the
    // directories it names — that drop is the author's own.
    let dropped: Vec<String> = dropped_directories(fs, root, rel_pattern, &walked)
        .await
        .into_iter()
        .filter(|p| !silenced(p))
        .collect();
    let mut paths: Vec<String> = walked.into_iter().filter(|p| !silenced(p)).collect();
    paths.sort();
    Ok(Globbed {
        paths: serde_json::Value::Array(paths.into_iter().map(serde_json::Value::String).collect()),
        warning: dropped_warning(pattern, &dropped),
    })
}

/// The directories under `root` that match `pattern` — the entries the
/// kernel walk never returns (`FsListDyn::glob` yields non-directory
/// entries only), named so a `*.md` that also matched a folder called
/// `item-07.md` cannot settle as a bare success (V9 wave 3 · p10: a
/// 12-candidate batch fanned over 11 and the trace read `succeeded`).
/// The grammar is the walker's own (`globset` · `literal_separator`) —
/// a second spelling would drift.
///
/// A single-segment pattern (`*.md`) needs neither descent nor stat: the
/// walker returned EVERY matching non-directory child of the root, so a
/// matching child it did not return is a directory by construction. A
/// pattern with a `/` or a `**` descends the way the walker does (hidden
/// directories are not entered · a symlink is a leaf) and asks
/// `metadata` which children are directories. `metadata` FOLLOWS links,
/// so a directory reached through a symlink is told apart by its
/// canonical path: one that is not the canonical root joined with the
/// child's relative path was reached through a link (a loop, or a tree
/// outside the walk) and is neither entered nor named — measured: a
/// `sub/loop -> ..` link made the report walk forever while the value
/// had long settled. Advisory by design: a listing that fails names
/// nothing — the value has already settled; this only decides what the
/// frame SAYS.
async fn dropped_directories<F: FsReadDyn + FsListDyn + FsMetaDyn>(
    fs: &F,
    root: &Path,
    pattern: &str,
    returned: &[String],
) -> Vec<String> {
    let Ok(matcher) = globset::GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map(|glob| glob.compile_matcher())
    else {
        return Vec::new();
    };
    let returned: BTreeSet<&str> = returned.iter().map(String::as_str).collect();
    let descend = pattern.contains('/') || pattern.contains("**");
    let mut dropped = Vec::new();
    // The root's canonical form, computed once: a child is a real member of
    // the walk only when its own canonical path is this root plus its
    // relative path (`std::fs::canonicalize` resolves every link on the way).
    let canonical_root = match fs.canonicalize(root).await {
        Ok(canonical) => canonical,
        Err(_) => root.to_path_buf(),
    };
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(children) = fs.list_dir(&dir).await else {
            continue;
        };
        for child in children {
            let text = child.to_string_lossy().into_owned();
            if returned.contains(text.as_str()) {
                continue; // a non-directory the walker returned
            }
            let Ok(rel) = child.strip_prefix(root) else {
                continue;
            };
            let hit = matcher.is_match(rel);
            if !descend {
                if hit {
                    dropped.push(text);
                }
                continue;
            }
            let Ok(meta) = fs.metadata(&child).await else {
                continue;
            };
            if !meta.is_dir {
                continue;
            }
            // A symlink is a leaf for the walker, so it is one for the report:
            // neither entered (a loop would never settle) nor named.
            let Ok(canonical) = fs.canonicalize(&child).await else {
                continue;
            };
            if canonical != canonical_root.join(rel) {
                continue;
            }
            if hit {
                dropped.push(text);
            }
            let hidden = child
                .file_name()
                .is_some_and(|name| name.as_encoded_bytes().first() == Some(&b'.'));
            if !hidden {
                stack.push(child);
            }
        }
    }
    dropped.sort();
    dropped
}

/// The OBS-E sentence for the directories a glob left out — the first
/// five named, the rest counted (a warning is a line, not a listing).
fn dropped_warning(pattern: &str, dropped: &[String]) -> Option<String> {
    const NAMED: usize = 5;
    let n = dropped.len();
    if n == 0 {
        return None;
    }
    let (noun, verb) = if n == 1 {
        ("directory", "was")
    } else {
        ("directories", "were")
    };
    let mut names = dropped
        .iter()
        .take(NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if n > NAMED {
        let _ = write!(names, " (+{} more)", n - NAMED);
    }
    Some(format!(
        "nika:glob returns files only · {n} {noun} also matched `{pattern}` and {verb} left out: {names}"
    ))
}

/// The directory a `nika:glob` of `pattern` walks FROM — the boundary the
/// dispatcher gates on (`pub(crate)` so `guarded_glob` enforces the SAME
/// root this fn globs, not a stale `.`). The literal directory prefix for
/// any pattern that has one · `.` otherwise (see [`split_pattern_root`]).
pub(crate) fn glob_walk_root(pattern: &str) -> Cow<'_, str> {
    split_pattern_root(pattern).0
}

/// Split a glob `pattern` into the directory to walk FROM and the pattern
/// to match relative to it.
///
/// EVERY pattern is re-rooted at its longest literal directory prefix
/// (every leading component up to the first one containing a glob meta
/// char `*`/`?`/`[`) and the remainder becomes the relative pattern:
/// `/tmp/x/file.txt` → (`/tmp/x`, `file.txt`) · `/data/**/*.rs` →
/// (`/data`, `**/*.rs`) · `hiring/inbox/*.md` → (`./hiring/inbox`,
/// `*.md`). Without this the kernel's root-relative matcher could never
/// see an absolute pattern match (silent `[]`), and a RELATIVE pattern
/// walked — and permits-gated — the WHOLE cwd instead of the directory
/// the author named. The relative root keeps its `./` prefix so every
/// returned path keeps the exact historical byte shape (`./items/a.md` —
/// run traces and registry oracles hash these strings).
fn split_pattern_root(pattern: &str) -> (Cow<'_, str>, &str) {
    if !Path::new(pattern).is_absolute() {
        // Strip a leading `./` — the walker matches against the root-RELATIVE
        // path of each entry (`strip_prefix(".")` → no `./` segment), so a
        // retained `./` in the pattern matches NOTHING (`./**/*.rs` → silent
        // `[]`). The spec's own example uses the `./`-prefixed form, so
        // `./**/*.rs` MUST behave exactly like `**/*.rs`.
        let p = pattern.strip_prefix("./").unwrap_or(pattern);
        let first_meta = p.find(['*', '?', '[']).unwrap_or(p.len());
        return match p[..first_meta].rfind('/') {
            // No literal directory prefix — the cwd IS the root.
            None => (Cow::Borrowed("."), p),
            Some(i) => (Cow::Owned(format!("./{}", &p[..i])), &p[i + 1..]),
        };
    }
    // Find the byte offset of the last `/` BEFORE the first glob meta char —
    // everything up to and including it is the literal directory root.
    let first_meta = pattern.find(['*', '?', '[']).unwrap_or(pattern.len());
    let split_at = pattern[..first_meta].rfind('/').unwrap_or(0);
    if split_at == 0 {
        // The meta char (or the whole pattern) sits in the root segment —
        // walk from `/` with the full path-after-root as the pattern.
        return (
            Cow::Borrowed("/"),
            pattern.strip_prefix('/').unwrap_or(pattern),
        );
    }
    let root = &pattern[..split_at];
    let rel = &pattern[split_at + 1..];
    (Cow::Borrowed(root), rel)
}

/// Read `exclude:` as either a single pattern string OR a list of them
/// (both spec-legal — a one-pattern exclude is the common case and was
/// silently dropped when only arrays were read).
fn exclude_patterns(args: &Args) -> Vec<String> {
    match args.get("exclude") {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
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
pub(crate) async fn grep<F: FsReadDyn + FsListDyn>(
    fs: &F,
    boundary: &FsBoundary,
    args: &Args,
) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-GREP-001";
    let pattern = req_str(args, "pattern", C)?;
    let root = args
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(".");
    let regex = build_regex(pattern, strict_bool(args, "case_insensitive", false, C)?)
        .map_err(|e| BuiltinFailure::new(C, format!("invalid pattern: {e}")))?;

    let files = fs.glob(Path::new(root), "**").await.map_err(|e| {
        // grep is a recursive DIRECTORY walk · a `path:` that names a FILE
        // makes `read_dir` fail with ENOTDIR ("Not a directory (os error
        // 20)") — a cryptic OS error. Name the real contract instead.
        match e {
            FsError::Io { .. } if is_not_a_directory(&e) => BuiltinFailure::new(
                C,
                format!("`path:` `{root}` must be a directory — grep walks a tree, not a file"),
            ),
            other => BuiltinFailure::new(C, format!("walk failed: {other}")),
        }
    })?;
    let mut hits = Vec::new();
    for file in files {
        // Re-enforce the boundary PER MATCHED FILE. The walk yields a symlink's
        // IN-boundary path, but `read_to_string` FOLLOWS it — canonicalize-
        // confine refuses a link whose target escapes the declared boundary
        // (the per-file sibling of `read`'s guard · grep was the one fs builtin
        // missing it · symlinked-leaf read bypass). An undeclared/unbounded
        // boundary short-circuits to Ok (the engine floor · MockFs tests are
        // unaffected). An out-of-boundary leaf is skipped like any unreadable.
        if boundary
            .enforce(fs, &file.to_string_lossy(), FsAccess::Read)
            .await
            .is_err()
        {
            continue;
        }
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

/// Whether an [`FsError::Io`] is the ENOTDIR class (a `read_dir` on a
/// file). The kernel folds ENOTDIR into the generic `Io` arm (no typed
/// variant · adding one is a Gate-12 kernel change), so this matches the
/// reason text — the std display (`"Not a directory"`) OR the raw unix
/// code (`os error 20`). Friendly-message-only: a miss just falls back to
/// the generic "walk failed", never a wrong verdict.
fn is_not_a_directory(e: &FsError) -> bool {
    match e {
        FsError::Io { reason } => {
            reason.contains("Not a directory") || reason.contains("os error 20")
        }
        _ => false,
    }
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
    }

    #[tokio::test]
    async fn write_serializes_structured_content_to_json() {
        // A non-string JSON value serializes to canonical JSON — "write
        // structured data to a .json file" in one step, no runtime type
        // failure (and no `check` blind-spot, since jq output types are not
        // statically inferable).
        let fs = MockFs::new();
        for (i, content) in [
            serde_json::json!([1, 2, 3]),
            serde_json::json!({ "a": 1, "b": "x" }),
            serde_json::json!(42),
            serde_json::json!(true),
        ]
        .into_iter()
        .enumerate()
        {
            let path = format!("out{i}.json");
            write(
                &fs,
                &args(serde_json::json!({ "path": path, "content": content.clone() })),
            )
            .await
            .expect("structured content serializes");
            let back = read(&fs, &args(serde_json::json!({ "path": path })))
                .await
                .expect("read back");
            let text = back.as_str().expect("text view");
            let parsed: serde_json::Value = serde_json::from_str(text).expect("valid JSON written");
            assert_eq!(parsed, content, "round-trips through canonical JSON");
        }
        // A hand-built JSON string is written VERBATIM (not double-encoded),
        // so `nika:jq … | tojson` then write produces identical bytes.
        write(
            &fs,
            &args(serde_json::json!({ "path": "v.json", "content": "[1,2,3]" })),
        )
        .await
        .expect("string verbatim");
        let back = read(&fs, &args(serde_json::json!({ "path": "v.json" })))
            .await
            .expect("read back");
        assert_eq!(back.as_str(), Some("[1,2,3]"));

        // `null` content is LOUD (a missing upstream value), never a silent
        // four-byte "null" write.
        let null_write = write(
            &fs,
            &args(serde_json::json!({ "path": "n.json", "content": null })),
        )
        .await;
        assert!(
            matches!(&null_write, Err(f) if f.code == "NIKA-BUILTIN-WRITE-001"
                && f.message.contains("null")),
            "{null_write:?}"
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
    async fn write_overwrite_string_false_is_a_loud_error_not_silent_clobber() {
        // The data-loss footgun: a STRING "false" (the common YAML/JSON slip)
        // must be a LOUD WRITE-001, never silently coerced to the `true`
        // default — which would overwrite the file the author meant to
        // protect. `nika check` does not type-check literal arg values, so the
        // runtime strict-bool reader is the last line of defense.
        let fs = MockFs::new().with_file("precious.txt", "PRECIOUS");
        let loud = write(
            &fs,
            &args(serde_json::json!({
                "path": "precious.txt", "content": "CLOBBER", "overwrite": "false"
            })),
        )
        .await;
        assert!(
            matches!(&loud, Err(f) if f.code == "NIKA-BUILTIN-WRITE-001"
                && f.message.contains("overwrite") && f.message.contains("boolean")),
            "string overwrite is a loud WRITE-001, not a silent clobber: {loud:?}"
        );
        // The file is untouched — the guard fired before any write.
        assert_eq!(
            fs.read_to_string(Path::new("precious.txt"))
                .await
                .expect("still there"),
            "PRECIOUS",
            "the protected file survives the string-false footgun"
        );
    }

    #[tokio::test]
    async fn write_create_dirs_false_refuses_a_missing_parent() {
        // false (the default) → a missing parent is a LOUD refusal, not a
        // silently-materialized tree (the seam would otherwise auto-create).
        let fs = MockFs::new();
        let refused = write(
            &fs,
            &args(serde_json::json!({ "path": "missing/x.txt", "content": "hi" })),
        )
        .await;
        assert!(
            matches!(&refused, Err(f) if f.code == "NIKA-BUILTIN-WRITE-001"
                && f.message.contains("create_dirs: true")),
            "missing parent + create_dirs:false must refuse with the fix hint: {refused:?}"
        );
        // …a parent that already exists is fine without create_dirs.
        let fs = MockFs::new().with_file("here/sibling.txt", "x");
        write(
            &fs,
            &args(serde_json::json!({ "path": "here/x.txt", "content": "hi" })),
        )
        .await
        .expect("existing parent needs no create_dirs");
        // …and create_dirs:true still materializes the tree.
        let fs = MockFs::new();
        write(
            &fs,
            &args(
                serde_json::json!({ "path": "fresh/deep/x.txt", "content": "hi", "create_dirs": true }),
            ),
        )
        .await
        .expect("create_dirs:true creates the parent");
        // …and a bare filename (empty parent = cwd) never trips the guard.
        let fs = MockFs::new();
        write(
            &fs,
            &args(serde_json::json!({ "path": "bare.txt", "content": "hi" })),
        )
        .await
        .expect("bare filename writes to cwd");
    }

    #[tokio::test]
    async fn write_refuses_a_path_that_names_a_directory() {
        // Harness-Bench 005 (0.118.7 · gpt-4o-mini · proxy trace kept): the
        // model asked for the DIRECTORY — `nika:write { path: "out/replies/",
        // content: "", create_dirs: true }` — and got `path not found`. The
        // refusal now NAMES the shape and the call that works.
        let fs = MockFs::new();
        let trailing = write(
            &fs,
            &args(serde_json::json!({
                "path": "out/replies/", "content": "", "create_dirs": true
            })),
        )
        .await;
        assert!(
            matches!(&trailing, Err(f) if f.code == "NIKA-BUILTIN-WRITE-001"
                && f.message.contains("out/replies/")
                && f.message.contains("names a directory")
                && f.message.contains("create_dirs: true")),
            "a trailing slash is taught, never `path not found`: {trailing:?}"
        );
        // The teach VERBATIM — this string IS the fix (an agent reads it and
        // its next turn is the right one), so it is pinned, not sampled.
        assert_eq!(
            trailing.as_ref().err().map(|f| f.message.as_str()),
            Some(
                "`out/replies/` names a directory, not a file — `nika:write` writes files. \
                 Write the file inside it (`out/replies/<name>`) with `create_dirs: true` \
                 and the directory is created."
            )
        );
        // …and NOTHING was created — a refusal carries no effect.
        assert!(
            fs.file_paths().is_empty(),
            "the refusal wrote nothing: {:?}",
            fs.file_paths()
        );
        // `.` and `..` are the same shape (`Path::file_name()` → None).
        let dotdot = write(
            &fs,
            &args(serde_json::json!({ "path": "..", "content": "x" })),
        )
        .await;
        assert!(
            matches!(&dotdot, Err(f) if f.message.contains("names a directory")),
            "`..` names a directory too: {dotdot:?}"
        );
        // …while a bare filename (empty parent = cwd) is untouched by the gate.
        write(
            &fs,
            &args(serde_json::json!({ "path": "bare.txt", "content": "hi" })),
        )
        .await
        .expect("a bare filename still writes");
    }

    #[tokio::test]
    async fn write_refuses_an_existing_directory_target() {
        // The `Is a directory (os error 21)` half — what the same model got
        // once a sibling call had created `out/replies`. Refused before the
        // seam, with the same teach, and the taught form still writes.
        let fs = MockFs::new().with_file("out/replies/001.txt", "first");
        let refused = write(
            &fs,
            &args(serde_json::json!({ "path": "out/replies", "content": "" })),
        )
        .await;
        assert!(
            matches!(&refused, Err(f) if f.code == "NIKA-BUILTIN-WRITE-001"
                && f.message.contains("out/replies")
                && f.message.contains("names a directory")
                && f.message.contains("create_dirs: true")),
            "an existing directory target is taught: {refused:?}"
        );
        let ok = write(
            &fs,
            &args(serde_json::json!({
                "path": "out/replies/002.txt", "content": "second", "create_dirs": true
            })),
        )
        .await
        .expect("the taught form writes");
        assert_eq!(
            ok,
            serde_json::Value::String("out/replies/002.txt".to_owned())
        );
        assert_eq!(
            fs.read_to_string(Path::new("out/replies/002.txt"))
                .await
                .expect("landed"),
            "second"
        );
        // …and the sibling the directory already held is untouched.
        assert_eq!(
            fs.read_to_string(Path::new("out/replies/001.txt"))
                .await
                .expect("intact"),
            "first"
        );
    }

    #[tokio::test]
    async fn write_beneath_a_file_names_the_file_in_the_way() {
        // Harness-Bench 005 again, on the binary that carries the two teaches
        // above (gpt-4o-mini): the model "pre-created" `out/replies` with an
        // EMPTY write (a file), then every `out/replies/<name>` write died
        // `path already exists: out/replies` — with and without `create_dirs`
        // — for the rest of the run. The refusal now names the blocker.
        let fs = MockFs::new().with_file("out/replies", "");
        let with_dirs = write(
            &fs,
            &args(serde_json::json!({
                "path": "out/replies/002.txt", "content": "hi", "create_dirs": true
            })),
        )
        .await;
        assert_eq!(
            with_dirs
                .as_ref()
                .err()
                .map(|f| (f.code, f.message.as_str())),
            Some((
                "NIKA-BUILTIN-WRITE-001",
                "`out/replies` is a file (0 bytes), not a directory — `out/replies/002.txt` \
                 cannot be created beneath it, and `nika:write` never deletes: write the \
                 file under another directory, or have the workflow's author remove \
                 `out/replies` first. (An empty `nika:write` at a directory's path creates \
                 a FILE there, never the directory — write the first file inside it with \
                 `create_dirs: true` instead.)"
            ))
        );
        // Without `create_dirs` the same blocker is named (the old path went
        // through the seam and came back `path already exists`).
        let plain = write(
            &fs,
            &args(serde_json::json!({ "path": "out/replies/002.txt", "content": "hi" })),
        )
        .await;
        assert!(
            matches!(&plain, Err(f) if f.code == "NIKA-BUILTIN-WRITE-001"
                && f.message.contains("`out/replies` is a file (0 bytes)")),
            "the blocker is named without create_dirs too: {plain:?}"
        );
        // A deeper target names the SAME blocker (the nearest existing
        // ancestor decides), and its size is the file's.
        let fs = MockFs::new().with_file("out/replies", "marker");
        let deep = write(
            &fs,
            &args(serde_json::json!({
                "path": "out/replies/2026/002.txt", "content": "hi", "create_dirs": true
            })),
        )
        .await;
        assert!(
            matches!(&deep, Err(f) if f.message.contains("`out/replies` is a file (6 bytes)")
                && f.message.contains("`out/replies/2026/002.txt` cannot be created")),
            "the nearest existing ancestor is the blocker: {deep:?}"
        );
        // …and nothing was written anywhere by the refusals.
        assert_eq!(
            fs.file_paths(),
            vec![Path::new("out/replies").to_path_buf()]
        );
        // A directory parent is not in the way: the taught form still writes.
        let fs = MockFs::new().with_file("out/replies/001.txt", "first");
        write(
            &fs,
            &args(serde_json::json!({
                "path": "out/replies/002.txt", "content": "second", "create_dirs": true
            })),
        )
        .await
        .expect("a directory parent is sound");
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
    async fn edit_count_string_is_loud_not_a_silent_replace_all() {
        // The intent-inversion footgun (sibling of overwrite:"false"): a
        // STRING count "2" parsed via a lax `and_then(as_u64)` reads as None
        // and falls through to replace-ALL — the author wanted 2 replaced but
        // silently gets every match. Now strict: a present non-integer count is
        // a LOUD EDIT-001, and the file is left untouched.
        let fs = MockFs::new().with_file("d.txt", "a a a a");
        let loud = edit(
            &fs,
            &args(serde_json::json!({
                "path": "d.txt", "find": "a", "replace": "b", "count": "2"
            })),
        )
        .await;
        assert!(
            matches!(&loud, Err(f) if f.code == "NIKA-BUILTIN-EDIT-001"
                && f.message.contains("count") && f.message.contains("integer")),
            "string count is a loud EDIT-001, not a silent replace-all: {loud:?}"
        );
        // The file is untouched — the guard fired before the write.
        let after = read(&fs, &args(serde_json::json!({ "path": "d.txt" })))
            .await
            .expect("ok");
        assert_eq!(
            after,
            serde_json::Value::String("a a a a".to_owned()),
            "the file survives the string-count footgun (no over-edit)"
        );
    }

    #[tokio::test]
    async fn write_path_gating_is_the_policy_layer_job() {
        // CANARY (crate spec §4 honest gap): this layer passes `path:`
        // verbatim to the fs seam — CWD confinement / traversal rejection
        // is `nika-policy`'s contract (L1.5 · design locked, impl gated).
        // When policy lands between the verbs and this dispatcher, THIS
        // pin flips to an expect-reject — until then the delegation is
        // explicit, not accidental. (An anchor file makes the `..` parent
        // exist so the create_dirs:false guard is orthogonal to the point
        // under test — traversal delegation, not parent existence.)
        let fs = MockFs::new().with_file("../anchor.txt", "x");
        let out = write(
            &fs,
            &args(serde_json::json!({ "path": "../escape.txt", "content": "x" })),
        )
        .await
        .expect("delegated today");
        assert_eq!(out, serde_json::Value::String("../escape.txt".to_owned()));
    }
}
