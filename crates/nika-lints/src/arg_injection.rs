// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `arg-injection` rule set — argument-injection advisories for the
//! array (`execve`) command form (spec `02-verbs.md` §exec Security).
//!
//! The array form is the structural fix for *command* injection (an
//! interpolated value is one literal argv token · no shell to parse it).
//! But a literal argv token can still be **argument**-injected: many common
//! binaries interpret a leading-`-` token as an OPTION, and several of those
//! options run a command or read/write an arbitrary file (CAPEC-6). So an
//! interpolated value that resolves to `-o ProxyCommand=…` (ssh),
//! `-c core.sshCommand=…` (git), `--checkpoint-action=exec=…` (tar), or
//! `--config <file>` (curl) flips the program's behavior even with no shell.
//!
//! This pass warns when an array command invokes a known
//! argument-injection-prone binary with an interpolated value in a position
//! the binary would parse as an option (before any `--` end-of-options
//! separator). The canonical fix is the `--` separator (everything after it
//! is positional, never a flag); for binaries whose vector is not a leading
//! dash (git `ext::` URLs, tar `-I` via env) the fix is to validate the
//! value. Warnings only — never errors (the structural `permits.exec` +
//! sandbox are the boundaries; detection is defense-in-depth, never the
//! gate · 2503.00061).
//!
//! The catalog is authored from the public argument-injection facts
//! (`GTFOArgs` · `SonarSource` argument-injection-vectors · CAPEC-6) — the
//! FACTS (which flag of which binary is dangerous), not their files.
//!
//! ## Scope limits (documented · not silent · review P1)
//!
//! This advisory is deliberately precise — it covers ONE shape: an
//! interpolated value in a catalog binary's ARGV form before any `--`. It does
//! NOT cover, by design:
//! - **A dynamically-selected program** (`["${{ const.tool }}", …]`) — the
//!   catalog cannot match an interpolated `argv[0]`; that is `permits.exec`'s
//!   (the allowlist) concern.
//! - **Dangerous ENV injection** (`env: { GIT_SSH_COMMAND: "${{ … }}" }`) —
//!   the runner's dangerous-env scrub + `permits` own that; this pass reads
//!   only `command` argv.
//! - **A shell-form command that uses a shell feature AND interpolates into a
//!   catalog binary** (`"ssh ${{ host }} | tee x"`) — `one-obvious-way/008`
//!   stays silent (a genuine pipeline) and this pass only sees the ARRAY form,
//!   so neither fires. Use the array form for any interpolated value.

use nika_schema::expression::scan_templates;
use nika_schema::raw::{RawAction, RawCommand, RawTask, RawWorkflow};

use super::preference_rules::Lint;

/// How to neutralize a binary's argument-injection vector — drives the
/// advisory's suggestion (the prior `dash_dash_helps: bool` could not express
/// the awk/sed "the value IS the program" case, and leaked git's `ext::`
/// clause onto every binary · review P2).
#[derive(Clone, Copy)]
enum FixKind {
    /// `--` end-of-options neutralizes the flag vector (the value is then a
    /// positional, never an option) — tar/zip/unzip.
    DashDash,
    /// `--` does NOT help (the dangerous flag takes a value, e.g. `ssh -o`, or
    /// is config-driven, e.g. `git -c`) — validate the value is not flag-shaped.
    ValidateValue,
    /// The interpolated value is itself executed AS code (awk/sed program text,
    /// psql `-c`) — `--` and leading-dash checks do not help; never pass
    /// untrusted input in that position.
    ProgramText,
}

/// A known argument-injection-prone binary + the concrete vector to cite in
/// the advisory (so the author knows WHY the value is dangerous).
struct ArgInjectionBinary {
    /// The program basename (matched case-insensitively).
    name: &'static str,
    /// The dangerous-flag vector (one short clause · cited in the message).
    vector: &'static str,
    /// How to neutralize it (drives the suggestion text).
    fix: FixKind,
}

/// The catalog (authored from `GTFOArgs` · `SonarSource` · CAPEC-6 FACTS).
/// Kept deliberately to binaries with a WELL-DOCUMENTED command-exec or
/// arbitrary-file argument-injection vector — precision over coverage.
const BINARIES: &[ArgInjectionBinary] = &[
    ArgInjectionBinary {
        name: "ssh",
        vector: "-o ProxyCommand=/-o LocalCommand= run a command; -F reads a config",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "scp",
        vector: "-o ProxyCommand= / -S <program> run a command",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "sftp",
        vector: "-o ProxyCommand= / -F <config> run a command",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "rsync",
        vector: "-e/--rsh= and --rsync-path= run a command",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "git",
        vector: "-c core.sshCommand=/core.pager=, --upload-pack=, --exec=, ext:: URLs run a command",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "tar",
        vector: "--checkpoint-action=exec=, -I/--use-compress-program=, --to-command= run a command",
        fix: FixKind::DashDash,
    },
    ArgInjectionBinary {
        // bsdtar (the default `tar` on macOS) shares tar's vectors under a
        // distinct basename — matched separately (review P1 · catalog hole).
        name: "bsdtar",
        vector: "--checkpoint-action=exec=, --use-compress-program=, --to-command= run a command",
        fix: FixKind::DashDash,
    },
    ArgInjectionBinary {
        name: "curl",
        vector: "-K/--config reads creds from a file; -o/--output and --upload-file read/write files",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "wget",
        vector: "--use-askpass= runs a command; -O/-i/--post-file read/write files",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "find",
        vector: "-exec/-execdir run a command; -fprintf writes a file",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "awk",
        vector: "argv after the program text is data, but the program text itself runs system()",
        fix: FixKind::ProgramText,
    },
    ArgInjectionBinary {
        name: "sed",
        vector: "the GNU `e` command runs a command; `w` writes a file",
        fix: FixKind::ProgramText,
    },
    ArgInjectionBinary {
        name: "zip",
        vector: "-T -TT=<cmd> runs a command",
        fix: FixKind::DashDash,
    },
    ArgInjectionBinary {
        name: "unzip",
        vector: "-o overwrites files outside the target",
        fix: FixKind::DashDash,
    },
    ArgInjectionBinary {
        name: "psql",
        vector: "-c '\\! cmd' shells out; -o/-f read/write files",
        fix: FixKind::ProgramText,
    },
    ArgInjectionBinary {
        // review P1 · catalog hole: mysql is commonly deployed + has a
        // well-documented exec vector.
        name: "mysql",
        vector: "-e/--execute runs SQL; --init-command runs SQL on connect; \\! shells out",
        fix: FixKind::ProgramText,
    },
    ArgInjectionBinary {
        name: "gpg",
        vector: "--exec= and config/keyring options read/write arbitrary files",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "openssl",
        vector: "s_client -cert/-key and dgst options read arbitrary files",
        fix: FixKind::ValidateValue,
    },
    ArgInjectionBinary {
        name: "socat",
        vector: "an EXEC: / SYSTEM: address runs a command",
        fix: FixKind::ProgramText,
    },
];

/// Run the `arg-injection` advisory pass over a parsed workflow.
///
/// Output is deterministic · in task order. Warnings only.
#[must_use]
pub fn arg_injection(wf: &RawWorkflow) -> Vec<Lint> {
    let mut lints = Vec::new();
    for task in wf.tasks.iter().map(|t| &t.value) {
        check_task(task, &mut lints);
    }
    lints
}

/// The basename of an argv program token (`/usr/bin/ssh` → `ssh`), lowercased.
/// A leading `${{ }}` program is not a catalog name, so it is left as-is and
/// simply won't match (an interpolated PROGRAM is `one-obvious-way/008`'s and
/// `permits.exec`'s concern, not this pass's).
fn argv_basename(program: &str) -> String {
    program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_lowercase()
}

/// Whether an argv element interpolates a value (`${{ … }}`), handling the
/// `\${{` escape (which is a literal, not an interpolation) via the canonical
/// island scanner.
fn interpolates(element: &str) -> bool {
    scan_templates(element).is_ok_and(|islands| !islands.is_empty())
}

fn check_task(task: &RawTask, lints: &mut Vec<Lint>) {
    let RawAction::Exec(e) = &task.action else {
        return;
    };
    let RawCommand::Argv(parts) = &e.command else {
        return; // the shell form is one-obvious-way/008's concern
    };
    let Some(program) = parts.first() else {
        return;
    };
    let base = argv_basename(&program.value);
    let Some(binary) = BINARIES.iter().find(|b| b.name == base) else {
        return; // not a known argument-injection-prone binary
    };

    // An interpolated value AFTER argv[0] and BEFORE any `--` end-of-options
    // separator can be parsed as an option — the argument-injection surface.
    // Everything after a literal `--` element is positional (never a flag),
    // so it is NOT flagged (the canonical guard already applied).
    let flagged = parts
        .iter()
        .skip(1)
        .take_while(|p| p.value != "--")
        .any(|p| interpolates(&p.value));
    if !flagged {
        return;
    }

    let suggestion = match binary.fix {
        FixKind::DashDash => format!(
            "insert a `--` element before the interpolated value (everything after \
             `--` is positional, never parsed as a flag), e.g. `[\"{}\", …, \"--\", \
             \"${{{{ … }}}}\"]`",
            binary.name
        ),
        FixKind::ValidateValue => {
            let git_note = if binary.name == "git" {
                " (or `ext::`)"
            } else {
                ""
            };
            format!(
                "validate the interpolated value does not begin with `-`{git_note} before it \
                 reaches `{}` — a `--` separator does NOT neutralize this vector",
                binary.name
            )
        }
        FixKind::ProgramText => format!(
            "never pass an interpolated value where `{}` reads it AS code/program text — \
             a `--` separator and leading-dash checks do not help; pass untrusted input \
             only as a positional DATA argument",
            binary.name
        ),
    };

    lints.push(Lint::new(
        "arg-injection/001",
        task.id.value.clone(),
        task.id.span,
        format!(
            "`{}` is argument-injection-prone ({}); an interpolated value that resolves \
             to a flag-shaped string is parsed as an option even with no shell",
            binary.name, binary.vector
        ),
        suggestion,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    fn lints_of(yaml: &str) -> Vec<Lint> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        arg_injection(&wf)
    }

    fn one_lint(prog: &str) -> Lint {
        let yaml = format!(
            "nika: ai\nconst:\n  x: \"v\"\ntasks:\n  t:\n    exec:\n      command: [\"{prog}\", \"${{{{ const.x }}}}\"]\n"
        );
        let mut ls = lints_of(&yaml);
        assert_eq!(ls.len(), 1, "exactly one /001 for {prog}");
        ls.remove(0)
    }

    #[test]
    fn every_catalogued_binary_actually_fires() {
        // Eleven of the nineteen were exercised by name; the other eight
        // rode on the catalog's reputation. A row nothing reaches is a row
        // that can stop working without anything going red.
        for b in BINARIES {
            let l = one_lint(b.name);
            assert!(
                l.message.contains(b.name),
                "{} is catalogued but its lint does not name it: {}",
                b.name,
                l.message
            );
            assert!(!l.suggestion.is_empty(), "{} fires with no way out", b.name);
        }
    }

    #[test]
    fn the_catalog_never_loses_a_binary() {
        // Walking BINARIES proves what is in it — it cannot notice a row
        // that was deleted. Each name here is a documented command-exec or
        // arbitrary-file vector; losing one loses the advisory, silently.
        const FLOOR: &[&str] = &[
            "ssh", "scp", "sftp", "rsync", "git", "tar", "bsdtar", "curl", "wget", "find", "awk",
            "sed", "zip", "unzip", "psql", "mysql", "gpg", "openssl", "socat",
        ];
        for name in FLOOR {
            assert!(
                BINARIES.iter().any(|b| b.name == *name),
                "{name} left the catalog · interpolating into its argv stops warning"
            );
        }
    }

    #[test]
    fn bsdtar_catalog_hole_is_closed() {
        // review P1 · bsdtar shares tar's vectors under a distinct basename.
        let l = one_lint("bsdtar");
        assert!(l.message.contains("bsdtar"));
        assert!(l.suggestion.contains("--"), "bsdtar → dash-dash fix");
    }

    #[test]
    fn mysql_catalog_hole_is_closed() {
        let l = one_lint("mysql");
        assert!(l.message.contains("mysql"));
    }

    #[test]
    fn ext_clause_is_git_only_not_leaked() {
        // review P2 · the `ext::` clause must appear ONLY for git.
        assert!(
            one_lint("git").suggestion.contains("ext::"),
            "git names ext::"
        );
        for non_git in ["ssh", "scp", "curl", "wget", "rsync"] {
            assert!(
                !one_lint(non_git).suggestion.contains("ext::"),
                "{non_git} must NOT mention ext::"
            );
        }
    }

    #[test]
    fn program_text_binaries_get_the_right_advice() {
        // review P2 · awk/sed/socat: the danger is the value-as-code, so the
        // suggestion is "never pass untrusted input there", NOT "validate `-`".
        for prog in ["awk", "sed", "socat", "mysql"] {
            let s = one_lint(prog).suggestion;
            assert!(
                s.contains("AS code") || s.contains("program text"),
                "{prog}: {s}"
            );
            assert!(
                !s.contains("begin with `-`"),
                "{prog} must not give the wrong fix"
            );
        }
    }

    #[test]
    fn flags_interpolated_value_for_a_catalog_binary() {
        let yaml = "\
nika: ai
const:
  host: \"example.com\"
tasks:
  connect:
    exec:
      command: [\"ssh\", \"${{ const.host }}\", \"uptime\"]
";
        let lints = lints_of(yaml);
        assert_eq!(lints.len(), 1);
        assert_eq!(lints[0].rule, "arg-injection/001");
        assert_eq!(lints[0].task_id, "connect");
        assert!(lints[0].message.contains("ssh"));
        // ssh's vector is not dash-dash-fixable → the suggestion says validate.
        assert!(
            lints[0].suggestion.contains("validate"),
            "{}",
            lints[0].suggestion
        );
    }

    #[test]
    fn dash_dash_suggestion_for_a_positional_binary() {
        let yaml = "\
nika: ai
const:
  file: \"data.tar\"
tasks:
  extract:
    exec:
      command: [\"tar\", \"-xf\", \"${{ const.file }}\"]
";
        let lints = lints_of(yaml);
        assert_eq!(lints.len(), 1);
        assert!(
            lints[0].suggestion.contains("--"),
            "{}",
            lints[0].suggestion
        );
    }

    #[test]
    fn silent_when_dash_dash_guards_the_value() {
        // The value is AFTER a `--` separator → positional, never a flag.
        let yaml = "\
nika: ai
const:
  file: \"x\"
tasks:
  extract:
    exec:
      command: [\"tar\", \"-xf\", \"--\", \"${{ const.file }}\"]
";
        assert!(
            lints_of(yaml).is_empty(),
            "the `--` guard suppresses the advisory"
        );
    }

    #[test]
    fn silent_without_interpolation() {
        let yaml = "\
nika: ai
tasks:
  connect:
    exec:
      command: [\"ssh\", \"host.example.com\", \"uptime\"]
";
        assert!(
            lints_of(yaml).is_empty(),
            "a fully-literal command is safe to author"
        );
    }

    #[test]
    fn silent_for_a_non_catalog_binary() {
        let yaml = "\
nika: ai
const:
  msg: \"hello\"
tasks:
  say:
    exec:
      command: [\"echo\", \"${{ const.msg }}\"]
";
        assert!(
            lints_of(yaml).is_empty(),
            "echo is not argument-injection-prone"
        );
    }

    #[test]
    fn silent_for_the_shell_form() {
        // The shell form is one-obvious-way/008's concern, not this pass's.
        let yaml = "\
nika: ai
const:
  host: \"h\"
tasks:
  connect:
    exec:
      shell: \"ssh ${{ const.host }} uptime\"
";
        assert!(
            lints_of(yaml).is_empty(),
            "shell form is /008, not arg-injection"
        );
    }

    #[test]
    fn basename_resolves_an_absolute_path() {
        let yaml = "\
nika: ai
const:
  url: \"https://x\"
tasks:
  fetch:
    exec:
      command: [\"/usr/bin/curl\", \"${{ const.url }}\"]
";
        let lints = lints_of(yaml);
        assert_eq!(
            lints.len(),
            1,
            "absolute path resolves to the curl basename"
        );
        assert!(lints[0].message.contains("curl"));
    }
}
