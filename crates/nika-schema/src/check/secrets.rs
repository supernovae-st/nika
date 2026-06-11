// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Secret-leak detection — the masking boundary, checked statically.
//!
//! Per spec `04-variables.md` §secrets · the engine masks its OWN output
//! (logs · traces · journal), but it CANNOT follow a `secrets.X` that the
//! author routes into a subprocess or tool which re-emits it into captured
//! output. This scan flags exactly that class BEFORE the run ·
//!
//! - a `secrets.X` reference in an `exec:` command / stdin / env value
//!   (the subprocess can echo it to a captured stdout/stderr)
//! - a `secrets.X` reference in an `invoke:`/`agent` tool argument (the
//!   tool can return it in its bound output)
//!
//! A `secrets.X` in an `infer:`/`agent` prompt is NOT flagged — it goes to
//! the provider as designed (the operator chose that provider), and the
//! response is the model's, not a verbatim echo of the secret.

use crate::expression::{NamespaceRef, expr_refs, scan_templates};
use crate::raw::{RawAction, RawWorkflow};

/// A secret that escapes the masking boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SecretLeak {
    /// The task whose effect can re-emit the secret.
    pub task: String,
    /// The secret name (`secrets.<name>`).
    pub secret: String,
    /// Where it flows (`exec.command`, `exec.env`, `invoke.args`, …).
    pub sink: &'static str,
}

/// Scan a workflow for secret values escaping the masking boundary.
#[must_use]
pub(super) fn scan_leaks(wf: &RawWorkflow) -> Vec<SecretLeak> {
    let declared: Vec<&str> = wf.secrets.iter().map(|(n, _)| n.value.as_str()).collect();
    if declared.is_empty() {
        return Vec::new();
    }
    let mut leaks = Vec::new();

    for task in &wf.tasks {
        let id = &task.value.id.value;
        let mut flag = |sink: &'static str, text: &str| {
            for secret in secrets_in(text, &declared) {
                leaks.push(SecretLeak {
                    task: id.clone(),
                    secret,
                    sink,
                });
            }
        };
        match &task.value.action {
            RawAction::Exec(a) => {
                for frag in a.command.text_fragments() {
                    flag("exec.command", frag);
                }
                if let Some(stdin) = &a.stdin {
                    flag("exec.stdin", &stdin.value);
                }
                for (_, v) in &a.env {
                    flag("exec.env", &v.value);
                }
            }
            RawAction::Invoke(a) => {
                if let Some(args) = &a.args {
                    flag("invoke.args", &args.value.to_string());
                }
            }
            // infer/agent prompts go to the provider by design (not a leak);
            // a secret in an agent prompt is the same provider-bound case.
            RawAction::Infer(_) | RawAction::Agent(_) => {}
        }
    }
    leaks
}

/// The declared secret names genuinely REFERENCED as `${{ secrets.<name> }}`
/// in `text` — via the real expression extractor, not a substring scan.
///
/// `scan_templates` finds only true `${{ … }}` islands (literal prose
/// mentioning `secrets.api_key` outside an island is NOT a reference), and
/// `expr_refs` decomposes each island into typed [`NamespaceRef`]s (so
/// `mysecrets.api_key` resolves to root `mysecrets`, never `secrets`). This
/// is the same path the analyzer uses for `NIKA-VAR-001`, so the leak scan
/// and reference resolution agree by construction.
fn secrets_in(text: &str, declared: &[&str]) -> Vec<String> {
    let Ok(islands) = scan_templates(text) else {
        // A malformed island is a parse error the analyzer already
        // reports (`check` ran analyze() first); nothing to flag here.
        return Vec::new();
    };
    let mut found = Vec::new();
    for island in &islands {
        for r in expr_refs(&island.expr) {
            if let NamespaceRef::Secrets(name) = r
                && declared.contains(&name.as_str())
                && !found.iter().any(|f| f == &name)
            {
                found.push(name);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn leaks_of(yaml: &str) -> Vec<SecretLeak> {
        scan_leaks(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    const SECRETS: &str = "\
secrets:
  api_key:
    source: vault
    key: prod/key
";

    #[test]
    fn secret_into_exec_command_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow: leak\n{SECRETS}tasks:\n  - id: t\n    exec: {{ command: \"curl -H 'Auth: ${{{{ secrets.api_key }}}}' x\" }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].secret, "api_key");
        assert_eq!(l[0].sink, "exec.command");
    }

    #[test]
    fn secret_into_exec_env_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow: leak\n{SECRETS}tasks:\n  - id: t\n    exec:\n      command: \"printenv\"\n      env:\n        TOKEN: \"${{{{ secrets.api_key }}}}\"\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "exec.env");
    }

    #[test]
    fn secret_into_invoke_args_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow: leak\n{SECRETS}tasks:\n  - id: t\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"x\", content: \"${{{{ secrets.api_key }}}}\" }} }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "invoke.args");
    }

    #[test]
    fn secret_into_infer_prompt_is_not_a_leak() {
        let yaml = format!(
            "nika: v1\nworkflow: ok\n{SECRETS}tasks:\n  - id: t\n    infer: {{ prompt: \"use ${{{{ secrets.api_key }}}}\", max_tokens: 10 }}\n"
        );
        assert!(leaks_of(&yaml).is_empty(), "provider-bound by design");
    }

    #[test]
    fn no_secrets_declared_no_scan() {
        let yaml =
            "nika: v1\nworkflow: none\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n";
        assert!(leaks_of(yaml).is_empty());
    }

    #[test]
    fn prefix_name_is_not_a_false_match() {
        // `secrets.api_key_backup` must NOT match a declared `api_key`.
        let yaml = format!(
            "nika: v1\nworkflow: ok\n{SECRETS}tasks:\n  - id: t\n    exec: {{ command: \"echo ${{{{ secrets.api_key_backup }}}}\" }}\n"
        );
        // api_key_backup is undeclared → it's a NIKA-VAR-001 elsewhere, but
        // here the point is the leak scan does not mis-attribute it to api_key.
        let l = leaks_of(&yaml);
        assert!(
            l.iter().all(|x| x.secret != "api_key"),
            "no false prefix match"
        );
    }
}

#[cfg(test)]
mod regression {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn leaks(yaml: &str) -> Vec<SecretLeak> {
        scan_leaks(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    const S: &str = "secrets:\n  api_key:\n    source: vault\n    key: x\n";

    #[test]
    fn literal_prose_mentioning_secret_is_not_a_leak() {
        // No ${{ }} island → no reference → no leak (was a false positive).
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    exec: {{ command: \"echo 'set secrets.api_key in vault'\" }}\n"
        );
        assert!(leaks(&y).is_empty(), "prose mention is not a reference");
    }

    #[test]
    fn longer_root_does_not_phantom_match() {
        // `mysecrets.api_key` resolves to root `mysecrets`, never `secrets`.
        // (mysecrets is itself NIKA-VAR-001 elsewhere; here: no api_key leak.)
        let y = format!(
            "nika: v1\nworkflow: w\n{S}vars: {{ mysecrets: \"x\" }}\ntasks:\n  - id: t\n    exec: {{ command: \"echo ${{{{ vars.mysecrets }}}}\" }}\n"
        );
        assert!(
            leaks(&y).iter().all(|l| l.secret != "api_key"),
            "no phantom prefix match"
        );
    }

    #[test]
    fn real_interpolated_secret_still_leaks() {
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    exec: {{ command: \"curl -H ${{{{ secrets.api_key }}}}\" }}\n"
        );
        let l = leaks(&y);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].secret, "api_key");
    }
}
