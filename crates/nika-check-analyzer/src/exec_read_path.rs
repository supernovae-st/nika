// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Literal `exec:` argv paths a known *reader* will open — a check HINT.
//!
//! Only `grep` / `rg` / `ag`: those programs open the file operand for
//! reading. `echo ./file` and `touch ./out` name a path they do not read,
//! so they stay silent (the PERMITS rung already left program semantics
//! to the run). A `${{ }}` token stays the run's verdict. Advisory.

use nika_schema::raw::{RawAction, RawCommand, RawWorkflow};

/// Stable rule id (`nika explain` teaches the same token).
pub const RULE: &str = "exec-read-path/001";

/// One site: task id, rule id, advice body (the wrap adds the kind).
pub type Site = (String, &'static str, String);

/// Scan argv-form `exec:` tasks for a literal path token the jail omits.
#[must_use]
pub fn scan(wf: &RawWorkflow) -> Vec<Site> {
    let Some(permits) = wf.permits.as_ref().map(|p| &p.value) else {
        return Vec::new(); // absent block: AUTH-006 already owns the exec
    };
    if !permits.allows_exec() {
        return Vec::new(); // the exec-category finding owns the task
    }
    let mut out = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        let RawAction::Exec(exec) = &task.value.action else {
            continue;
        };
        let RawCommand::Argv(parts) = &exec.command else {
            continue; // shell form: the PERMITS rung already scoped that door
        };
        let mut elements = parts.iter().map(|p| p.value.as_str());
        let Some(program) = elements.next() else {
            continue;
        };
        if program.contains("${{") {
            continue; // resolved program is the run's
        }
        if !permits.allows_program(program) {
            continue; // the exec allowlist finding owns this argv
        }
        let args: Vec<&str> = elements.collect();
        let cwd = exec.cwd.as_ref().map(|c| c.value.as_str());
        if cwd.is_some_and(|c| c.contains("${{")) {
            continue; // resolved identity is unknowable
        }
        push_uncovered_paths(id, program, &args, cwd, permits, &mut out);
    }
    out
}

fn push_uncovered_paths(
    id: &str,
    program: &str,
    args: &[&str],
    cwd: Option<&str>,
    permits: &nika_cap::Permits,
    out: &mut Vec<Site>,
) {
    if !is_read_search(program) {
        return;
    }
    for token in grep_file_operands(args) {
        if !is_literal_path_token(token) {
            continue;
        }
        let Some(resolved) = resolve_against_cwd(token, cwd) else {
            continue;
        };
        if permits.jail_admits_read(&resolved) {
            continue;
        }
        out.push((
            id.to_owned(),
            RULE,
            format!(
                "the argv names `{token}` (cwd-resolved `{resolved}`). If `{program}` \
                 opens that path for reading, check that `permits.fs.read` covers \
                 `{resolved}` — OS access control can still deny a granted path"
            ),
        ));
    }
}

/// Path tokens grep/rg/ag will treat as files. `-e`/`--regexp` take a
/// pattern (`grep -e ./literal ./input` must not treat `./literal` as a file).
fn grep_file_operands<'a>(args: &'a [&str]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut options = true;
    let mut needs_pattern = !args.iter().any(|arg| {
        matches!(*arg, "-e" | "--regexp" | "-f" | "--file")
            || arg.starts_with("--regexp=")
            || arg.starts_with("--file=")
            || (arg.starts_with("-e") && arg.len() > 2)
            || (arg.starts_with("-f") && arg.len() > 2)
    });
    let mut args = args.iter().copied();
    while let Some(arg) = args.next() {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options && arg.starts_with('-') && arg != "-" {
            match arg {
                "-e" | "--regexp" | "-g" | "--glob" | "--iglob" | "-A" | "-B" | "-C"
                | "--after-context" | "--before-context" | "--context" | "-m" | "--max-count" => {
                    if args.next().is_none() {
                        return Vec::new();
                    }
                }
                "-f" | "--file" => {
                    let Some(path) = args.next() else {
                        return Vec::new();
                    };
                    out.push(path);
                }
                "-i"
                | "-n"
                | "-r"
                | "-R"
                | "-v"
                | "-w"
                | "-l"
                | "-q"
                | "-s"
                | "-F"
                | "-E"
                | "--ignore-case"
                | "--line-number"
                | "--recursive"
                | "--invert-match"
                | "--word-regexp"
                | "--files-with-matches"
                | "--quiet"
                | "--fixed-strings" => {}
                flag if flag.starts_with("--file=") => out.push(&flag[7..]),
                flag if flag.starts_with("-f") && !flag.starts_with("--") => out.push(&flag[2..]),
                flag if flag.starts_with("--regexp=")
                    || (flag.starts_with("-e") && !flag.starts_with("--")) => {}
                // Unknown option arity cannot prove where the file operands start.
                _ => return Vec::new(),
            }
            continue;
        }
        if needs_pattern {
            needs_pattern = false;
        } else {
            out.push(arg);
        }
    }
    out
}

fn is_read_search(program: &str) -> bool {
    matches!(
        program.rsplit(['/', '\\']).next().unwrap_or(program),
        "grep" | "rg" | "ag"
    )
}

/// Explicit path form only: `./x`, `../x`, `/abs`, `~/x`. A bare word is
/// a pattern or a flag operand, not a path we can claim.
fn is_literal_path_token(token: &str) -> bool {
    if token.is_empty() || token.contains("${{") || token == "-" {
        return false;
    }
    if token.starts_with('-') {
        return false;
    }
    token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with('/')
        || token.starts_with("~/")
}

fn resolve_against_cwd(path: &str, cwd: Option<&str>) -> Option<String> {
    if path.starts_with('/') || path.starts_with('~') {
        return Some(path.to_owned());
    }
    let rel = path.strip_prefix("./").unwrap_or(path);
    match cwd {
        None => Some(path.to_owned()),
        Some(c) if c.contains("${{") => None,
        Some(c) if c == "." || c == "./" => Some(path.to_owned()),
        Some(c) => Some(format!("{}/{rel}", c.trim_end_matches('/'))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn sites_of(yaml: &str) -> Vec<Site> {
        let parsed = parse(yaml, FileId::new(0), ParseMode::Strict);
        assert!(parsed.is_ok(), "fixture must parse");
        parsed.map(|wf| scan(&wf)).unwrap_or_default()
    }

    fn grep_yaml(fs: &str, file: &str) -> String {
        format!(
            "\
nika: t
permits:
  exec: [\"grep\"]
{fs}tasks:
  probe:
    exec: {{ command: [\"grep\", \"READY\", \"{file}\"] }}
"
        )
    }

    #[test]
    fn a_literal_grep_file_outside_fs_read_is_named() {
        let sites = sites_of(&grep_yaml("", "./build-status.txt"));
        assert_eq!(sites.len(), 1, "{sites:?}");
        assert_eq!(sites[0].0, "probe");
        assert_eq!(sites[0].1, RULE);
        assert!(
            sites[0].2.contains("./build-status.txt")
                && sites[0].2.contains("permits.fs.read")
                && sites[0].2.contains("If")
                && !sites[0].2.contains("add \""),
            "{}",
            sites[0].2
        );
    }

    #[test]
    fn the_grant_that_covers_the_file_is_silent() {
        let sites = sites_of(&grep_yaml(
            "  fs: { read: [\"./build-status.txt\"] }\n",
            "./build-status.txt",
        ));
        assert!(sites.is_empty(), "{sites:?}");
    }

    #[test]
    fn a_computed_path_stays_silent() {
        let yaml = "\
nika: t
inputs:
  file: { type: string }
permits:
  exec: [\"grep\"]
tasks:
  probe:
    exec: { command: [\"grep\", \"READY\", \"${{ inputs.file }}\"] }
";
        assert!(sites_of(yaml).is_empty(), "computed tokens are the run's");
    }

    #[test]
    fn an_interpreter_script_is_left_to_the_finding() {
        let yaml = "\
nika: t
permits:
  exec: [\"bash\"]
tasks:
  leg:
    exec: { command: [\"bash\", \"./leg.sh\"] }
";
        assert!(
            sites_of(yaml).is_empty(),
            "interpreter scripts are the PERMITS finding"
        );
    }

    #[test]
    fn a_bare_word_is_not_a_path() {
        let yaml = "\
nika: t
permits:
  exec: [\"grep\"]
tasks:
  probe:
    exec: { command: [\"grep\", \"READY\", \"build-status.txt\"] }
";
        assert!(
            sites_of(yaml).is_empty(),
            "a bare word is a pattern, not a path claim"
        );
    }

    #[test]
    fn absent_permits_are_silent_here() {
        let yaml = "\
nika: t
tasks:
  probe:
    exec: { command: [\"grep\", \"READY\", \"./build-status.txt\"] }
";
        assert!(sites_of(yaml).is_empty());
    }

    #[test]
    fn echo_of_a_literal_path_is_silent() {
        let yaml = "\
nika: t
permits:
  exec: [\"echo\"]
tasks:
  probe:
    exec: { command: [\"echo\", \"./literal\"] }
";
        assert!(sites_of(yaml).is_empty(), "echo does not read the token");
    }

    #[test]
    fn touch_of_an_output_path_is_silent() {
        let yaml = "\
nika: t
permits:
  exec: [\"touch\"]
tasks:
  probe:
    exec: { command: [\"touch\", \"./output\"] }
";
        assert!(
            sites_of(yaml).is_empty(),
            "touch writes; it is not a read-search"
        );
    }

    #[test]
    fn a_cwd_is_folded_into_the_suggested_grant() {
        let yaml = "\
nika: t
permits:
  exec: [\"grep\"]
tasks:
  probe:
    exec: { command: [\"grep\", \"READY\", \"./file.txt\"], cwd: \"sub\" }
";
        let sites = sites_of(yaml);
        assert_eq!(sites.len(), 1, "{sites:?}");
        assert!(
            sites[0].2.contains("`sub/file.txt`") && sites[0].2.contains("If"),
            "cwd-resolved path must appear: {}",
            sites[0].2
        );
        assert!(
            !sites[0].2.contains("add \""),
            "must not prescribe a grant: {}",
            sites[0].2
        );
    }

    #[test]
    fn grep_e_pattern_is_not_a_file_operand() {
        let yaml = "\
nika: t
permits:
  exec: [\"grep\"]
tasks:
  probe:
    exec: { command: [\"grep\", \"-e\", \"./literal\", \"./input\"] }
";
        let sites = sites_of(yaml);
        assert_eq!(sites.len(), 1, "{sites:?}");
        assert!(
            !sites[0].2.contains("./literal"),
            "-e ./literal is a pattern: {}",
            sites[0].2
        );
        assert!(
            sites[0].2.contains("./input"),
            "the file operand remains: {}",
            sites[0].2
        );
    }
    #[test]
    fn path_shaped_patterns_and_option_values_are_not_file_operands() {
        assert_eq!(
            grep_file_operands(&["./pattern", "./input"]),
            vec!["./input"]
        );
        assert_eq!(
            grep_file_operands(&["--", "./pattern", "./input"]),
            vec!["./input"]
        );
        assert_eq!(
            grep_file_operands(&["-g", "./glob", "pattern", "./input"]),
            vec!["./input"]
        );
        assert_eq!(
            grep_file_operands(&["-f", "./patterns", "./input"]),
            vec!["./patterns", "./input"]
        );
        assert!(grep_file_operands(&["--unknown", "./value", "./input"]).is_empty());
    }
}
