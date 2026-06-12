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

use crate::expression::scan_templates;
use crate::raw::{RawAction, RawCommand, RawTask, RawWorkflow};

use super::preference_rules::Lint;

/// A known argument-injection-prone binary + the concrete vector to cite in
/// the advisory (so the author knows WHY the value is dangerous).
struct ArgInjectionBinary {
    /// The program basename (matched case-insensitively).
    name: &'static str,
    /// The dangerous-flag vector (one short clause · cited in the message).
    vector: &'static str,
    /// Whether the canonical `--` end-of-options separator neutralizes the
    /// flag vector (true for most), so the suggestion can name it.
    dash_dash_helps: bool,
}

/// The catalog (authored from `GTFOArgs` · `SonarSource` · CAPEC-6 FACTS).
/// Kept deliberately to binaries with a WELL-DOCUMENTED command-exec or
/// arbitrary-file argument-injection vector — precision over coverage.
const BINARIES: &[ArgInjectionBinary] = &[
    ArgInjectionBinary {
        name: "ssh",
        vector: "-o ProxyCommand=/-o LocalCommand= run a command; -F reads a config",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "scp",
        vector: "-o ProxyCommand= / -S <program> run a command",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "sftp",
        vector: "-o ProxyCommand= / -F <config> run a command",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "rsync",
        vector: "-e/--rsh= and --rsync-path= run a command",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "git",
        vector: "-c core.sshCommand=/core.pager=, --upload-pack=, --exec=, ext:: URLs run a command",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "tar",
        vector: "--checkpoint-action=exec=, -I/--use-compress-program=, --to-command= run a command",
        dash_dash_helps: true,
    },
    ArgInjectionBinary {
        name: "curl",
        vector: "-K/--config reads creds from a file; -o/--output and --upload-file read/write files",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "wget",
        vector: "--use-askpass= runs a command; -O/-i/--post-file read/write files",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "find",
        vector: "-exec/-execdir run a command; -fprintf writes a file",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "awk",
        vector: "the program text runs system(); -f reads a program file",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "sed",
        vector: "the GNU `e` command runs a command; `w` writes a file",
        dash_dash_helps: false,
    },
    ArgInjectionBinary {
        name: "zip",
        vector: "-T -TT=<cmd> runs a command",
        dash_dash_helps: true,
    },
    ArgInjectionBinary {
        name: "unzip",
        vector: "-o overwrites files outside the target",
        dash_dash_helps: true,
    },
    ArgInjectionBinary {
        name: "psql",
        vector: "-c '\\! cmd' shells out; -o/-f read/write files",
        dash_dash_helps: false,
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

    let suggestion = if binary.dash_dash_helps {
        format!(
            "insert a `--` element before the interpolated value (everything after \
             `--` is positional, never parsed as a flag), e.g. `[\"{}\", …, \"--\", \
             \"${{{{ … }}}}\"]`",
            binary.name
        )
    } else {
        format!(
            "validate the interpolated value does not begin with `-` (or `ext::` for git) \
             before it reaches `{}` — a `--` separator does NOT neutralize this vector",
            binary.name
        )
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
    use crate::{FileId, ParseMode, parse};

    fn lints_of(yaml: &str) -> Vec<Lint> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        arg_injection(&wf)
    }

    #[test]
    fn flags_interpolated_value_for_a_catalog_binary() {
        let yaml = "\
nika: v1
workflow: ai
vars:
  host: \"example.com\"
tasks:
  - id: connect
    exec:
      command: [\"ssh\", \"${{ vars.host }}\", \"uptime\"]
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
nika: v1
workflow: ai
vars:
  file: \"data.tar\"
tasks:
  - id: extract
    exec:
      command: [\"tar\", \"-xf\", \"${{ vars.file }}\"]
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
nika: v1
workflow: ai
vars:
  file: \"x\"
tasks:
  - id: extract
    exec:
      command: [\"tar\", \"-xf\", \"--\", \"${{ vars.file }}\"]
";
        assert!(
            lints_of(yaml).is_empty(),
            "the `--` guard suppresses the advisory"
        );
    }

    #[test]
    fn silent_without_interpolation() {
        let yaml = "\
nika: v1
workflow: ai
tasks:
  - id: connect
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
nika: v1
workflow: ai
vars:
  msg: \"hello\"
tasks:
  - id: say
    exec:
      command: [\"echo\", \"${{ vars.msg }}\"]
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
nika: v1
workflow: ai
vars:
  host: \"h\"
tasks:
  - id: connect
    exec:
      command: \"ssh ${{ vars.host }} uptime\"
";
        assert!(
            lints_of(yaml).is_empty(),
            "shell form is /008, not arg-injection"
        );
    }

    #[test]
    fn basename_resolves_an_absolute_path() {
        let yaml = "\
nika: v1
workflow: ai
vars:
  url: \"https://x\"
tasks:
  - id: fetch
    exec:
      command: [\"/usr/bin/curl\", \"${{ vars.url }}\"]
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
