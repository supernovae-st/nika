// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The environment plane of the capability boundary (NEP-0005 · spec 01
//! §permits env) — the two canonical name floors and the ONE composition
//! law every child-process spawn family runs.
//!
//! A child process environment is COMPOSED, never inherited:
//! [`RUNNER_FLOOR_ENV_VARS`] ∪ the declared `permits.env:` passthrough ∪
//! the task's authored `env:` map, minus [`DANGEROUS_ENV_VARS`]. The
//! composition is [`compose_child_env`] — pure (the ambient read is
//! injected), so the law is testable without touching the real
//! environment and both spawn families (the exec runner · the MCP stdio
//! client) run the SAME function. The static twin (`nika-check` ·
//! `NIKA-AUTH-009` dead grants) reads the same lists — engine ≡ check ≡
//! reference by construction.

use std::collections::BTreeMap;

/// Environment variables ALWAYS stripped from a spawned child's environment —
/// the injection vectors that grant code execution or library injection with no
/// dangerous flag in the command itself (the "env-var injection" class). The
/// strip is independent of any pre-validation and wins even over an explicit
/// `env` set (the floor wins — a workflow does not pass these).
///
/// The ONE canonical list (ADR-095 Layer 3): every subprocess-spawn site runs
/// the clean-slate posture (NEP-0005 · spec 01 §permits env) — the child
/// environment is COMPOSED via [`compose_child_env`], the spawn starts from
/// `env_clear`, applies exactly that map, and strips THIS list last, so a
/// floor name smuggled into the map still dies. A `permits.env:` entry naming
/// one of these is an inert dead grant, flagged at check (`NIKA-AUTH-009`).
pub const DANGEROUS_ENV_VARS: &[&str] = &[
    // Dynamic-linker library injection (run attacker code in any dynamically
    // linked child) — Linux + macOS (incl. the macOS fallback paths · P2).
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "GCONV_PATH", // glibc iconv module load → code execution (P2)
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "DYLD_FALLBACK_FRAMEWORK_PATH",
    // Shell startup-file sourcing on non-interactive `sh`/bash.
    "BASH_ENV",
    "ENV",
    // Tool-specific command hooks (RCE with no flags).
    "GIT_SSH_COMMAND",
    "GIT_SSH",
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
    "GIT_PROXY_COMMAND", // config-driven git RCE (P2)
    "GIT_CONFIG_GLOBAL", // point git at an attacker config (core.pager/fsmonitor)
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_PARAMETERS", // inline config (core.pager) with no file at all (O4-A)
    "GIT_CONFIG_COUNT",      // the enumerated form of the same inline class (O4-A)
    "GIT_TEMPLATE_DIR",      // attacker hooks copied into a new repo
    "LESSOPEN",              // pager-input-preprocess command injection (P2)
    "HOSTALIASES",           // attacker file read during hostname resolution (P2)
    "TERMINFO",              // load a crafted terminfo entry (P2)
    "TERMINFO_DIRS",         // terminfo search-path override (P2)
    "TERMCAP",               // crafted termcap string executed by some pagers (P2)
    // Interpreter pre-exec hooks.
    "PYTHONSTARTUP",
    "PYTHONPATH", // inject a module into any python that imports (P2)
    "PERL5OPT",
    "PERL5LIB",
    "RUBYOPT",
    "RUBYLIB", // ruby search-path override (O4-A)
    "NODE_OPTIONS",
    "NODE_PATH",            // node module search-path override (O4-A)
    "JAVA_TOOL_OPTIONS",    // JVM -agentlib/-agentpath hook (O4-A)
    "_JAVA_OPTIONS",        // the same class, honored even when the other is set (O4-A)
    "DOTNET_STARTUP_HOOKS", // .NET startup hook assembly (O4-A)
    "OPENSSL_CONF",         // OpenSSL config → module load (O4-A)
    "OPENSSL_MODULES",      // OpenSSL provider search path (O4-A)
    // Field-splitting injection for shell-mode commands.
    "IFS",
];

/// The runner env floor — the ONLY names a child process may inherit from
/// the engine's environment without a declared `permits.env:` grant
/// (NEP-0005 · spec 01 §permits env · the list is normative there as the
/// implicit MAXIMUM: an engine passes at most these, and may pass fewer).
///
/// A loader path, a home for tool caches, scratch, locale and timezone —
/// what a child needs to RUN, and nothing that names a credential. The
/// composition order is floor ∪ declared passthrough ∪ the task's authored
/// `env:` map, and [`DANGEROUS_ENV_VARS`] strips last (floor ∩ dangerous
/// is empty by construction — asserted in tests). ONE canonical list, two
/// consumers (the exec runner's spawn site and the MCP stdio scrub), so
/// the two spawn families cannot drift.
pub const RUNNER_FLOOR_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "TZ", "USER", "LOGNAME",
];

/// Whether `name` is on the dangerous-name floor — the check-side twin's
/// predicate (`NIKA-AUTH-009` · a granting `permits.env:` entry is an
/// inert dead grant) and the composition's final strip, one source.
#[must_use]
pub fn is_dangerous_env_name(name: &str) -> bool {
    DANGEROUS_ENV_VARS.contains(&name)
}

/// Compose a child process environment (NEP-0005 · spec 01 §permits env) —
/// the ONE law every spawn family runs (the exec runner · the MCP stdio
/// client): [`RUNNER_FLOOR_ENV_VARS`] ∪ the declared `permits.env:`
/// passthrough (both resolved through `lookup`, the spawn site's ambient
/// reader) ∪ the AUTHORED task map (wins on a same-name collision), minus
/// [`DANGEROUS_ENV_VARS`] (stripped last — no grant overrides that floor).
///
/// Pure: the ambient read is injected, so the law is testable without
/// touching the real environment. A looked-up name that is absent passes
/// nothing (no error · no empty-string synthesis · NEP-0005 definitions).
pub fn compose_child_env(
    lookup: impl Fn(&str) -> Option<String>,
    passthrough: &[String],
    authored: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for name in RUNNER_FLOOR_ENV_VARS {
        if let Some(value) = lookup(name) {
            env.insert((*name).to_owned(), value);
        }
    }
    for name in passthrough {
        if let Some(value) = lookup(name) {
            env.insert(name.clone(), value);
        }
    }
    for (name, value) in authored {
        env.insert(name.clone(), value.clone());
    }
    for name in DANGEROUS_ENV_VARS {
        env.remove(*name);
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake ambient environment for the pure composition law.
    fn ambient(name: &str) -> Option<String> {
        match name {
            "PATH" => Some("/usr/bin:/bin".to_owned()),
            "HOME" => Some("/home/op".to_owned()),
            "AWS_SECRET_ACCESS_KEY" => Some("hunter2".to_owned()),
            "CI_COMMIT_SHA" => Some("abc123".to_owned()),
            "LD_PRELOAD" => Some("/tmp/evil.so".to_owned()),
            _ => None,
        }
    }

    #[test]
    fn compose_floor_passes_ambient_credential_does_not() {
        // NEP-0005 law 1 · undeclared = the floor and nothing else.
        let env = compose_child_env(ambient, &[], &BTreeMap::new());
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
        assert_eq!(env.get("HOME").map(String::as_str), Some("/home/op"));
        assert!(
            !env.contains_key("AWS_SECRET_ACCESS_KEY"),
            "an ambient credential must never cross undeclared"
        );
    }

    #[test]
    fn compose_declared_passthrough_passes_absent_name_passes_nothing() {
        // NEP-0005 law 2 · presence grants REACH, ambient supplies the value
        // or nothing (no error · no empty-string synthesis).
        let names = vec!["CI_COMMIT_SHA".to_owned(), "NOT_SET_ANYWHERE".to_owned()];
        let env = compose_child_env(ambient, &names, &BTreeMap::new());
        assert_eq!(env.get("CI_COMMIT_SHA").map(String::as_str), Some("abc123"));
        assert!(!env.contains_key("NOT_SET_ANYWHERE"));
    }

    #[test]
    fn compose_authored_wins_over_passthrough_and_floor() {
        // NEP-0005 law 6 · the authored map applies AFTER the passthrough.
        let names = vec!["CI_COMMIT_SHA".to_owned()];
        let mut authored = BTreeMap::new();
        authored.insert("CI_COMMIT_SHA".to_owned(), "authored".to_owned());
        authored.insert("PATH".to_owned(), "/decl".to_owned());
        let env = compose_child_env(ambient, &names, &authored);
        assert_eq!(
            env.get("CI_COMMIT_SHA").map(String::as_str),
            Some("authored")
        );
        assert_eq!(env.get("PATH").map(String::as_str), Some("/decl"));
    }

    #[test]
    fn compose_dangerous_floor_wins_over_every_grant() {
        // NEP-0005 law 3 · a dangerous name is never passable: not via the
        // passthrough (an inert dead grant) and not via the authored map.
        let names = vec!["LD_PRELOAD".to_owned()];
        let mut authored = BTreeMap::new();
        authored.insert("BASH_ENV".to_owned(), "/tmp/hook".to_owned());
        let env = compose_child_env(ambient, &names, &authored);
        assert!(!env.contains_key("LD_PRELOAD"));
        assert!(!env.contains_key("BASH_ENV"));
        assert!(is_dangerous_env_name("LD_PRELOAD"));
        assert!(!is_dangerous_env_name("CI_COMMIT_SHA"));
    }

    #[test]
    fn the_o4_a_inline_config_class_is_dangerous() {
        // O4-A · the same-RCE-with-no-file class (red team 2026-07-23):
        // inline git config + the interpreter pre-exec hooks join the floor.
        for name in [
            "GIT_CONFIG_PARAMETERS",
            "GIT_CONFIG_COUNT",
            "JAVA_TOOL_OPTIONS",
            "_JAVA_OPTIONS",
            "DOTNET_STARTUP_HOOKS",
            "RUBYLIB",
            "NODE_PATH",
            "OPENSSL_CONF",
            "OPENSSL_MODULES",
        ] {
            assert!(is_dangerous_env_name(name), "{name} must be on the floor");
        }
    }

    /// EVERY entry, not the eleven somebody happened to name.
    ///
    /// The list had 40 entries and 11 named assertions, so 29 were
    /// carried by nothing at all: deleting `DYLD_INSERT_LIBRARIES` —
    /// which makes macOS load arbitrary code into every dynamically
    /// linked child — left all 126 tests green (2026-08-02). A list is
    /// only as strong as the weakest entry nobody checks, and the credential
    /// header leak of the same day was this exact shape: one name tried,
    /// the rest assumed.
    ///
    /// Iterate the const. Never the examples.
    #[test]
    fn every_dangerous_name_is_refused_and_never_reaches_a_child() {
        let ambient_all = |name: &str| -> Option<String> {
            DANGEROUS_ENV_VARS
                .contains(&name)
                .then(|| "/tmp/attacker".to_owned())
        };
        for name in DANGEROUS_ENV_VARS {
            assert!(
                is_dangerous_env_name(name),
                "{name} is on the list but the predicate does not know it"
            );
            // Three doors, all shut: the ambient environment, an explicit
            // passthrough grant, and the authored map.
            let grants = vec![(*name).to_owned()];
            let mut authored = BTreeMap::new();
            authored.insert((*name).to_owned(), "/tmp/hook".to_owned());
            let env = compose_child_env(ambient_all, &grants, &authored);
            assert!(
                !env.contains_key(*name),
                "{name} reached the child despite the dangerous floor"
            );
        }
    }

    /// The names that can never LEAVE the list.
    ///
    /// The test above iterates the const, which proves every listed name
    /// is enforced — and proves nothing about a name that was deleted,
    /// because the loop simply stops visiting it. Found by mutation:
    /// dropping `DYLD_INSERT_LIBRARIES` left the iterating test green
    /// (2026-08-02). Iterating catches an entry the code forgot to
    /// enforce; only a floor catches an entry someone removed.
    ///
    /// This is a ratchet, not a copy of the const. The const may grow
    /// freely; this says what it may never shed, and each line carries
    /// the mechanism that makes removal a remote-code-execution door.
    const FLOOR: &[(&str, &str)] = &[
        (
            "LD_PRELOAD",
            "loads attacker code into any dynamically linked child (linux)",
        ),
        ("LD_AUDIT", "same, via the linker's audit interface"),
        (
            "LD_LIBRARY_PATH",
            "redirects library resolution to attacker paths",
        ),
        ("GCONV_PATH", "glibc iconv module load, then execution"),
        ("DYLD_INSERT_LIBRARIES", "the macOS twin of LD_PRELOAD"),
        ("DYLD_LIBRARY_PATH", "macOS library redirection"),
        (
            "DYLD_FALLBACK_LIBRARY_PATH",
            "macOS library redirection, fallback path",
        ),
        ("BASH_ENV", "sources a file on every non-interactive bash"),
        ("ENV", "the same for POSIX sh"),
        (
            "GIT_SSH_COMMAND",
            "runs an arbitrary command on every fetch",
        ),
        (
            "GIT_EXTERNAL_DIFF",
            "runs an arbitrary command on every diff",
        ),
        (
            "GIT_CONFIG_PARAMETERS",
            "inline config, so RCE with no file on disk",
        ),
        (
            "PYTHONSTARTUP",
            "sources a file into every interactive python",
        ),
        ("PYTHONPATH", "shadows any importable module"),
        ("PERL5OPT", "injects switches into every perl"),
        ("RUBYOPT", "the same for ruby"),
        ("NODE_OPTIONS", "injects --require into every node"),
        ("IFS", "re-splits words in every unquoted shell expansion"),
        ("LESSOPEN", "runs a preprocessor on every file less opens"),
    ];

    #[test]
    fn the_floor_never_leaves_the_dangerous_list() {
        for (name, why) in FLOOR {
            assert!(
                DANGEROUS_ENV_VARS.contains(name),
                "{name} was removed from the dangerous floor — it {why}"
            );
        }
    }

    /// The floor is case-INSENSITIVE on the platforms that are, and the
    /// list is spelled in one case. A child env is a map, so a lookalike
    /// spelling is a different key — this pins which way the predicate
    /// actually answers rather than assuming it.
    #[test]
    fn the_predicate_answers_on_exact_names_only() {
        assert!(is_dangerous_env_name("LD_PRELOAD"));
        assert!(!is_dangerous_env_name("LD_PRELOAD_EXTRA"));
        assert!(!is_dangerous_env_name("MY_LD_PRELOAD"));
        assert!(!is_dangerous_env_name(""));
    }

    #[test]
    fn floor_and_dangerous_lists_are_disjoint() {
        // The structural invariant the composition order relies on: a floor
        // name stripped by the dangerous pass would be a silent self-break.
        for name in RUNNER_FLOOR_ENV_VARS {
            assert!(
                !DANGEROUS_ENV_VARS.contains(name),
                "{name} is on both the runner floor and the dangerous floor"
            );
        }
    }
}
