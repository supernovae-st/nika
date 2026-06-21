// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika doctor` — environment diagnosis (spec `nika-cli` §8 · §2 v0.81 floor).
//!
//! Diagnose-ONLY · the human keeps the hand: every problem prints the exact fix
//! COMMAND, never mutates PATH / shell / config (`--fix` is refused v1 · spec
//! §8). Provider keys are checked PRESENT-NOT-PRINTED — only `is_set` is
//! observed, the value is never bound into a variable, so no secret can reach
//! stdout / stderr / a trace (alignment Rule 1). No network, no phone-home: v1
//! does NOT ping providers (it cannot confirm a local server is up without a
//! probe), it reports the configured surface and prints the fix.
//!
//! Exit · `0` (a diagnosis is informational) · `3` (ENV · spec §4) only when
//! there is NO inference path at all (zero cloud keys present AND zero local
//! providers in the catalog — a broken/empty build). An unset cloud key is a
//! `⚠` (advisory · you may use a different provider or a local server), never a
//! `✖`: with no workflow in hand `doctor` cannot know which provider you need.

use std::fmt::Write as _;

use nika_providers::{ProviderRegistry, ProvidersConfig};

use crate::verbs::{VerbOutput, exit};

/// Severity of one diagnosis line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    /// `✔` healthy.
    Ok,
    /// `⚠` advisory (the run may still work).
    Warn,
    /// `✖` a hard environment problem (drives `exit 3`).
    Fail,
}

impl Level {
    fn glyph(self) -> char {
        match self {
            Self::Ok => '✔',
            Self::Warn => '⚠',
            Self::Fail => '✖',
        }
    }
}

/// One diagnosis line · a problem carries the exact PRINTED fix (never run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub level: Level,
    pub label: String,
    pub detail: String,
    pub fix: Option<String>,
}

/// A provider's environment facts — key PRESENCE only, never the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderProbe {
    pub id: String,
    pub requires_key: bool,
    pub key_present: bool,
    /// The conventional env var to name in the fix (e.g. `ANTHROPIC_API_KEY`).
    pub fix_var: String,
}

/// The injected environment facts `diagnose` reasons over — PURE · testable.
/// The CLI fills it from the real env; tests pass synthetic probes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Probe {
    pub version: String,
    /// `Some(path)` when a user config file exists.
    pub config_path: Option<String>,
    pub providers: Vec<ProviderProbe>,
}

/// Pure diagnosis → ordered findings (binary · config · cloud providers ·
/// local summary · the inference-readiness gate).
pub(crate) fn diagnose(probe: &Probe) -> Vec<Finding> {
    let mut out = vec![
        Finding {
            level: Level::Ok,
            label: "binary".to_owned(),
            detail: format!("v{} · self-contained (spec v1 embedded)", probe.version),
            fix: None,
        },
        match &probe.config_path {
            Some(path) => Finding {
                level: Level::Ok,
                label: "config".to_owned(),
                detail: path.clone(),
                fix: None,
            },
            None => Finding {
                level: Level::Warn,
                label: "config".to_owned(),
                detail: "none — built-in defaults".to_owned(),
                fix: None,
            },
        },
    ];

    let mut cloud_keys = 0_usize;
    let mut local_ids: Vec<&str> = Vec::new();
    for p in &probe.providers {
        if !p.requires_key {
            local_ids.push(&p.id);
        } else if p.key_present {
            cloud_keys += 1;
            out.push(Finding {
                level: Level::Ok,
                label: "provider".to_owned(),
                detail: format!("{} — key present", p.id),
                fix: None,
            });
        } else {
            out.push(Finding {
                level: Level::Warn,
                label: "provider".to_owned(),
                detail: format!("{} — {} unset", p.id, p.fix_var),
                fix: Some(format!("export {}=…", p.fix_var)),
            });
        }
    }

    if !local_ids.is_empty() {
        out.push(Finding {
            level: Level::Ok,
            label: "local".to_owned(),
            detail: format!(
                "{} provider(s) ({}) — no key · needs a running server (doctor does not ping · v1)",
                local_ids.len(),
                local_ids.join(" · ")
            ),
            fix: None,
        });
    }

    // The ONE Fail that drives exit 3 · no inference path AT ALL (a broken or
    // empty catalog). A merely-unset cloud key is a ⚠ above, never fatal.
    if cloud_keys == 0 && local_ids.is_empty() {
        out.push(Finding {
            level: Level::Fail,
            label: "providers".to_owned(),
            detail: "no inference provider available — neither a cloud key nor a local server"
                .to_owned(),
            fix: Some(
                "export <PROVIDER>_API_KEY=…  · or run a local server (ollama · llama.cpp · vLLM)"
                    .to_owned(),
            ),
        });
    }

    out
}

/// Exit code · `ENV(3)` when any `Fail`, else `OK(0)`.
pub(crate) fn exit_code(findings: &[Finding]) -> u8 {
    if findings.iter().any(|f| f.level == Level::Fail) {
        exit::ENV
    } else {
        exit::OK
    }
}

/// Render findings as the `nika doctor` report (spec §8 layout · glyph · label
/// padded · detail · an indented `fix:` line under a problem).
pub(crate) fn render(findings: &[Finding]) -> String {
    let mut s = String::new();
    for f in findings {
        let _ = writeln!(s, "{} {:<10} {}", f.level.glyph(), f.label, f.detail);
        if let Some(fix) = &f.fix {
            let _ = writeln!(s, "  fix: {fix}");
        }
    }
    s
}

/// Build the real probe from the environment (PRESENCE-only key checks · the
/// value is never bound) + the canonical provider catalog, then diagnose.
#[must_use]
pub fn run() -> VerbOutput {
    let registry = ProviderRegistry::without_http(ProvidersConfig::new());
    let providers = registry
        .profiles()
        .iter()
        // `mock` is the in-crate test backend, not an operator-facing provider.
        .filter(|p| p.id != "mock")
        .map(|p| {
            let candidates = p.env_candidates();
            ProviderProbe {
                id: p.id.to_owned(),
                requires_key: p.requires_key,
                // PRESENT-NOT-PRINTED: only presence is observed · the value is
                // never bound (alignment Rule 1 · no secret ever surfaces).
                key_present: candidates.iter().any(|v| env_present(v)),
                // The conventional var (last candidate · `ANTHROPIC_API_KEY`)
                // is the friendliest fix; the `NIKA_<ID>_API_KEY` form always
                // works too but reads less familiar.
                fix_var: candidates
                    .last()
                    .cloned()
                    .unwrap_or_else(|| format!("NIKA_{}_API_KEY", p.id.to_uppercase())),
            }
        })
        .collect();
    let probe = Probe {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        config_path: config_path(),
        providers,
    };
    let findings = diagnose(&probe);
    let code = exit_code(&findings);
    VerbOutput {
        text: render(&findings),
        code,
    }
}

/// Whether an env var is SET and non-empty — PRESENT-NOT-PRINTED. The value is
/// never bound (`var_os(...).is_some`), so no secret can surface (alignment
/// Rule 1). This is the doctor verb's sanctioned `std::env` boundary — the same
/// justification the compose root's key-read carries: checking key PRESENCE is
/// not a secret READ, and routing a presence-bool through the kernel vault seam
/// would be ceremony with no payoff.
#[allow(clippy::disallowed_methods)]
fn env_present(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// `~/.nika/config.toml` when it exists (presence only · never read). `HOME` is
/// a path, not a secret — the scoped allow mirrors `env_present`'s.
#[allow(clippy::disallowed_methods)]
fn config_path() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home)
        .join(".nika")
        .join("config.toml");
    path.exists().then(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(id: &str, var: &str, present: bool) -> ProviderProbe {
        ProviderProbe {
            id: id.to_owned(),
            requires_key: true,
            key_present: present,
            fix_var: var.to_owned(),
        }
    }
    fn local(id: &str) -> ProviderProbe {
        ProviderProbe {
            id: id.to_owned(),
            requires_key: false,
            key_present: false,
            fix_var: String::new(),
        }
    }

    #[test]
    fn key_present_is_ok_and_exits_zero() {
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: Some("~/.nika/config.toml".to_owned()),
            providers: vec![cloud("anthropic", "ANTHROPIC_API_KEY", true)],
        };
        let f = diagnose(&probe);
        let prov = f
            .iter()
            .find(|f| f.label == "provider")
            .expect("provider line");
        assert_eq!(prov.level, Level::Ok);
        assert!(prov.detail.contains("key present"));
        assert_eq!(exit_code(&f), exit::OK);
    }

    #[test]
    fn unset_key_is_a_warn_with_a_fix_not_a_fail() {
        // An unset cloud key is advisory — exit stays 0 (a local provider is a
        // valid path · doctor cannot know which provider the user needs).
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![
                cloud("anthropic", "ANTHROPIC_API_KEY", false),
                local("ollama"),
            ],
        };
        let f = diagnose(&probe);
        let prov = f
            .iter()
            .find(|f| f.label == "provider")
            .expect("provider line");
        assert_eq!(prov.level, Level::Warn);
        assert_eq!(
            prov.fix.as_deref(),
            Some("export ANTHROPIC_API_KEY=…"),
            "the exact fix command is printed"
        );
        assert!(
            !f.iter().any(|f| f.level == Level::Fail),
            "a local path exists"
        );
        assert_eq!(exit_code(&f), exit::OK);
    }

    #[test]
    fn never_prints_a_secret_value() {
        // The probe carries only a bool — there is no field a value could ride.
        // Assert the rendered report carries the VAR NAME, never a value.
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![cloud("openai", "OPENAI_API_KEY", false)],
        };
        let text = render(&diagnose(&probe));
        assert!(text.contains("OPENAI_API_KEY"), "names the var: {text}");
        assert!(
            !text.contains("sk-"),
            "no secret-shaped value leaks: {text}"
        );
    }

    #[test]
    fn no_provider_at_all_fails_with_exit_three() {
        // A broken/empty catalog — no cloud key AND no local server: the real
        // "cannot infer" environment error (spec §4 ENV).
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![cloud("anthropic", "ANTHROPIC_API_KEY", false)],
        };
        let f = diagnose(&probe);
        assert!(f.iter().any(|f| f.level == Level::Fail));
        assert_eq!(exit_code(&f), exit::ENV);
    }

    #[test]
    fn local_provider_alone_is_a_usable_path_exit_zero() {
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![local("ollama"), local("vllm")],
        };
        let f = diagnose(&probe);
        let loc = f.iter().find(|f| f.label == "local").expect("local line");
        assert_eq!(loc.level, Level::Ok);
        assert!(loc.detail.contains("ollama") && loc.detail.contains("vllm"));
        assert_eq!(exit_code(&f), exit::OK, "a local path is usable");
    }

    /// Each severity prints a DISTINCT glyph — a `Default::default()` mutant
    /// (the null char `'\0'`) would erase the level cue the operator scans for.
    #[test]
    fn level_glyphs_are_distinct() {
        assert_eq!(Level::Ok.glyph(), '✔');
        assert_eq!(Level::Warn.glyph(), '⚠');
        assert_eq!(Level::Fail.glyph(), '✖');
    }

    /// `env_present` is a PRESENCE check (set + non-empty) — read against real
    /// vars so no racy `set_var` is needed. PATH is always set + non-empty
    /// (kills the `-> false` constant + the `!is_empty` negation); a name
    /// nothing sets is absent (kills the `-> true` constant).
    #[test]
    fn env_present_reflects_the_real_environment() {
        assert!(env_present("PATH"), "PATH is set + non-empty");
        assert!(!env_present("NIKA_CLI_DEFINITELY_UNSET_VARIABLE_XYZZY"));
    }

    #[test]
    fn the_real_catalog_has_no_fail_and_renders() {
        // The wired run() over the canonical catalog: local providers exist, so
        // there is never a hard Fail (exit 0) even with no keys in the test env.
        let out = run();
        assert_eq!(out.code, exit::OK, "the catalog always offers a path");
        assert!(out.text.contains("binary"), "renders the binary line");
        assert!(!out.text.contains("mock"), "the test backend is hidden");
    }
}
