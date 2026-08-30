// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! File-plumbing exec operands — the native-first/002 program set as a
//! capability predicate (B05 / B29 · issue 1295).
//!
//! `permits.exec: ["cat"]` grants the PROGRAM, not a host-file read. The
//! checker and the exec runner both walk this table so a literal
//! `["cat", "/etc/passwd"]` cannot check-green and run-green. In-tree
//! relative operands stay the runtime jail's (a `cat README.md` the
//! author granted via `permits.fs.read` is not this door).

/// Programs native-first/002 names as file plumbing. ONE list: the hint
/// classifier and this capability door cannot drift.
pub const FILE_PLUMBING_PROGRAMS: &[&str] = &[
    "cat", "tee", "cp", "mv", "mkdir", "touch", "head", "tail", "ls",
];

/// Path-shaped argv operands of a file-plumbing program.
///
/// Flags (`-n`, `--number`) and stdin (`-`) are skipped; everything after
/// `--` is an operand. A program whose basename is not in
/// [`FILE_PLUMBING_PROGRAMS`] yields nothing — silence, never a guess.
#[must_use]
pub fn file_plumbing_path_operands<'a>(program: &str, args: &'a [&str]) -> Vec<&'a str> {
    let base = program.rsplit(['/', '\\']).next().unwrap_or(program);
    if !FILE_PLUMBING_PROGRAMS.contains(&base) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut after_ddash = false;
    for a in args {
        if !after_ddash && *a == "--" {
            after_ddash = true;
            continue;
        }
        if !after_ddash && a.starts_with('-') && *a != "-" {
            continue;
        }
        if *a == "-" {
            continue;
        }
        out.push(*a);
    }
    out
}

/// Lexically, does the path leave the workflow's root?
///
/// Absolute, home-relative (`~` · `$HOME` · `${HOME}`), and a `..` climb
/// that goes past the anchor do; `a/../b` does not. Purely textual — the
/// check has no filesystem; symlink escapes stay the runtime gate's.
#[must_use]
pub fn path_leaves_workspace(path: &str) -> bool {
    if path.starts_with('/')
        || path.starts_with('\\')
        || path.starts_with('~')
        || path.starts_with("$HOME")
        || path.starts_with("${HOME}")
    {
        return true;
    }
    let mut depth: i32 = 0;
    for seg in path.split(['/', '\\']) {
        match seg {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            _ => depth += 1,
        }
    }
    false
}

/// The first file-plumbing operand that leaves the workspace and the
/// jail does not admit. `None` is silence: not file plumbing, an in-tree
/// relative, a templated island, or an explicit host grant.
#[must_use]
pub fn file_plumbing_host_escape<'a>(
    program: &str,
    args: &'a [&str],
    cwd: Option<&str>,
    admits: impl Fn(&str) -> bool,
) -> Option<&'a str> {
    for operand in file_plumbing_path_operands(program, args) {
        if operand.contains("${{") {
            continue;
        }
        let Some(resolved) = resolve_file_operand(operand, cwd) else {
            continue;
        };
        if !path_leaves_workspace(&resolved) {
            continue;
        }
        if admits(&resolved) {
            continue;
        }
        return Some(operand);
    }
    None
}

/// Resolve a file-plumbing operand against a literal cwd. `None` = the
/// identity is not statically knowable (a templated cwd).
fn resolve_file_operand(path: &str, cwd: Option<&str>) -> Option<String> {
    if path.starts_with('/') || path.starts_with('~') || path.starts_with('$') {
        return Some(path.to_owned());
    }
    match cwd {
        None => Some(path.to_owned()),
        Some(c) if c.contains("${{") => None,
        Some(c) if c == "." || c == "./" => Some(path.to_owned()),
        Some(c) => Some(format!("{}/{path}", c.trim_end_matches('/'))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_plumbing_host_passwd_is_an_escape() {
        let args = ["/etc/passwd"];
        let hit = file_plumbing_host_escape("cat", &args, None, |_| false);
        assert_eq!(hit, Some("/etc/passwd"));
        let granted = file_plumbing_host_escape("cat", &args, None, |p| p == "/etc/passwd");
        assert_eq!(
            granted, None,
            "an explicit host grant is the operator's act"
        );
    }

    #[test]
    fn a_cwd_readme_is_not_this_door() {
        let args = ["README.md"];
        assert_eq!(
            file_plumbing_host_escape("cat", &args, None, |_| false),
            None,
            "in-tree relative stays the runtime jail's"
        );
        assert_eq!(
            file_plumbing_host_escape("cat", &["./README.md"], None, |_| false),
            None
        );
    }

    #[test]
    fn basename_cat_is_still_file_plumbing() {
        let args = ["/etc/passwd"];
        assert_eq!(
            file_plumbing_host_escape("/usr/bin/cat", &args, None, |_| false),
            Some("/etc/passwd")
        );
    }

    #[test]
    fn flags_and_stdin_are_not_paths() {
        assert!(file_plumbing_path_operands("cat", &["-n", "-"]).is_empty());
        assert_eq!(
            file_plumbing_path_operands("cat", &["-n", "--", "/etc/passwd"]),
            vec!["/etc/passwd"]
        );
        assert!(file_plumbing_path_operands("echo", &["/etc/passwd"]).is_empty());
    }

    #[test]
    fn a_relative_climb_leaves_the_workspace() {
        assert!(path_leaves_workspace("../secret"));
        assert!(path_leaves_workspace("/etc/passwd"));
        assert!(!path_leaves_workspace("README.md"));
        assert!(!path_leaves_workspace("a/../b"));
        let args = ["../secret"];
        assert_eq!(
            file_plumbing_host_escape("cat", &args, None, |_| false),
            Some("../secret")
        );
    }
}
