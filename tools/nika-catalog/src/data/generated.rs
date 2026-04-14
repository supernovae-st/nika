// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// The body of this module is produced by `build.rs` — it contains a
// `phf::Map` literal whose internal hash-key is necessarily a 20-digit u64.
// Workspace clippy rules (readable literals, pedantic) can't apply to code
// we do not own, so they are relaxed here. Anything authored by hand stays
// under the normal workspace lints.
#![allow(clippy::unreadable_literal, clippy::too_many_lines)]

//! Build-time generated catalogs.
//!
//! The body of this module is produced by `build.rs` from `data/*.toml`
//! and materialised at `$OUT_DIR/mcp_servers.rs`. Phase C step 3
//! establishes the infrastructure; Steps 4 + 5 fill in the full data.
//!
//! Consumers should prefer the shape exposed here over the legacy
//! `ALL_MCP_ALIASES` static. During Phase C transition both coexist.

// `UniCase::ascii(...)` expressions appear in the phf_codegen output.
use unicase::UniCase;

include!(concat!(env!("OUT_DIR"), "/mcp_servers.rs"));
include!(concat!(env!("OUT_DIR"), "/providers.rs"));

/// Case-insensitive lookup by id or alias. Returns `None` when unknown.
#[must_use]
pub fn find_mcp_server(name: &str) -> Option<&'static crate::types::McpServer> {
    let idx = *MCP_INDEX.get(&UniCase::ascii(name))?;
    ALL_MCP_SERVERS.get(idx)
}

/// Case-insensitive provider lookup by id or alias. Returns `None` when unknown.
#[must_use]
pub fn find_provider_v3(name: &str) -> Option<&'static crate::types::ProviderDef> {
    let idx = *PROVIDER_INDEX.get(&UniCase::ascii(name))?;
    ALL_PROVIDERS_V3.get(idx)
}

#[cfg(test)]
#[allow(clippy::panic, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn all_servers_non_empty() {
        assert!(!ALL_MCP_SERVERS.is_empty());
    }

    #[test]
    fn all_servers_count_105() {
        // 102 migrated from src/data/mcp_aliases.rs (v2 npm) +
        // 3 Phase A restorations: qdrant (pypi+uvx), intercom (remote+oauth),
        // grafana (oci). Total = 105.
        assert_eq!(ALL_MCP_SERVERS.len(), 105);
    }

    #[test]
    fn parity_every_legacy_alias_migrated() {
        use crate::data::mcp_aliases::ALL_MCP_ALIASES;
        use crate::types::{McpPricing, RegistryType};

        for legacy in ALL_MCP_ALIASES {
            let migrated = find_mcp_server(legacy.name).unwrap_or_else(|| {
                panic!("legacy alias {:?} not migrated to new catalog", legacy.name)
            });
            assert_eq!(migrated.id, legacy.name);

            // npm identifier preserved 1:1.
            let npm = migrated
                .packages
                .iter()
                .find(|p| p.registry_type == RegistryType::Npm)
                .unwrap_or_else(|| panic!("{:?}: no npm package", legacy.name));
            assert_eq!(
                npm.identifier, legacy.package,
                "{:?}: npm identifier diverged",
                legacy.name,
            );

            // Category preserved.
            assert_eq!(
                migrated.category.as_str(),
                legacy.category,
                "{:?}: category diverged",
                legacy.name,
            );

            // Pricing preserved.
            let legacy_pricing_str = match legacy.pricing {
                McpPricing::Free => "free",
                McpPricing::Freemium => "freemium",
                McpPricing::Paid => "paid",
            };
            let new_pricing_str = match migrated.pricing {
                McpPricing::Free => "free",
                McpPricing::Freemium => "freemium",
                McpPricing::Paid => "paid",
            };
            assert_eq!(
                new_pricing_str, legacy_pricing_str,
                "{:?}: pricing diverged",
                legacy.name,
            );

            // env_var + key_prefix preserved (when legacy had them).
            match (legacy.env_var, migrated.env_vars.first()) {
                (Some(leg_env), Some(new_env)) => {
                    assert_eq!(
                        new_env.name, leg_env,
                        "{:?}: env_var name diverged",
                        legacy.name,
                    );
                    // Legacy had a single `key_prefix: Option<&str>`; the
                    // new spec uses `key_prefixes: &[&str]`. Parity means:
                    // legacy None ↔ empty slice; legacy Some(p) ↔ slice
                    // where p is the first element.
                    match (legacy.key_prefix, new_env.key_prefixes.first().copied()) {
                        (None, None) => {}
                        (Some(lp), Some(np)) => assert_eq!(
                            lp, np,
                            "{:?}: key_prefix first-element diverged",
                            legacy.name,
                        ),
                        (leg, new) => panic!(
                            "{:?}: key_prefix parity mismatch legacy={leg:?} new={new:?}",
                            legacy.name,
                        ),
                    }
                }
                (Some(leg_env), None) => {
                    panic!(
                        "{:?}: legacy had env_var {:?} but new has none",
                        legacy.name, leg_env,
                    );
                }
                (None, Some(new_env)) => {
                    panic!(
                        "{:?}: new has env_var {:?} but legacy had none",
                        legacy.name, new_env.name,
                    );
                }
                (None, None) => {}
            }
        }
    }

    #[test]
    fn restored_entries_use_new_distributions() {
        // Phase A restorations — exercise the new Distribution model end-to-end.
        let qdrant = find_mcp_server("qdrant").expect("qdrant restored");
        assert_eq!(qdrant.packages[0].registry_type, crate::types::RegistryType::Pypi);
        assert_eq!(qdrant.packages[0].runner, Some(crate::types::PyRunner::Uvx));

        let intercom = find_mcp_server("intercom").expect("intercom restored");
        assert!(intercom.packages.is_empty());
        assert_eq!(intercom.remotes[0].auth, crate::types::AuthMode::OAuth);

        let grafana = find_mcp_server("grafana").expect("grafana restored");
        assert_eq!(grafana.packages[0].registry_type, crate::types::RegistryType::Oci);
    }

    #[test]
    fn seed_contains_filesystem() {
        let s = find_mcp_server("filesystem").expect("filesystem seeded");
        assert_eq!(s.id, "filesystem");
        assert_eq!(s.category, crate::types::Category::Anthropic);
        assert_eq!(s.packages.len(), 1);
        assert_eq!(s.packages[0].registry_type, crate::types::RegistryType::Npm);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert!(find_mcp_server("FILESYSTEM").is_some());
        assert!(find_mcp_server("FileSystem").is_some());
        assert!(find_mcp_server("filesystem").is_some());
    }

    #[test]
    fn unknown_server_returns_none() {
        assert!(find_mcp_server("does-not-exist").is_none());
    }

    #[test]
    fn pypi_server_has_runner() {
        let s = find_mcp_server("qdrant").expect("qdrant seeded");
        assert_eq!(s.packages[0].registry_type, crate::types::RegistryType::Pypi);
        assert_eq!(s.packages[0].runner, Some(crate::types::PyRunner::Uvx));
    }

    #[test]
    fn remote_server_has_no_packages() {
        let s = find_mcp_server("intercom").expect("intercom seeded");
        assert!(s.packages.is_empty());
        assert_eq!(s.remotes.len(), 1);
        assert_eq!(s.remotes[0].auth, crate::types::AuthMode::OAuth);
    }

    #[test]
    fn every_server_has_at_least_one_install_path() {
        for s in ALL_MCP_SERVERS {
            assert!(
                !s.packages.is_empty() || !s.remotes.is_empty(),
                "server {:?} has no packages AND no remotes",
                s.id,
            );
        }
    }

    #[test]
    fn every_server_id_roundtrips_through_index() {
        for s in ALL_MCP_SERVERS {
            assert_eq!(
                find_mcp_server(s.id).map(|x| x.id),
                Some(s.id),
                "server {:?} is not resolvable via MCP_INDEX",
                s.id,
            );
        }
    }

    // ─── Providers v3 ────────────────────────────────────────────────

    #[test]
    fn all_providers_v3_count_21() {
        assert_eq!(ALL_PROVIDERS_V3.len(), 21);
    }

    #[test]
    fn parity_every_legacy_provider_migrated() {
        use crate::data::providers::ALL_PROVIDERS;

        for legacy in ALL_PROVIDERS {
            let migrated = find_provider_v3(legacy.id).unwrap_or_else(|| {
                panic!("legacy provider {:?} not migrated to v3", legacy.id)
            });
            assert_eq!(migrated.id, legacy.id);
            assert_eq!(migrated.name, legacy.name, "{:?}: name diverged", legacy.id);
            assert_eq!(
                migrated.env_var, legacy.env_var,
                "{:?}: env_var diverged",
                legacy.id,
            );
            // Legacy `key_prefix: Option<&str>` ↔ new `key_prefixes: &[&str]`.
            // Parity rule: every legacy prefix must still be accepted by v3;
            // v3 may have added more (OpenAI gains sk-proj-).
            match legacy.key_prefix {
                None => assert!(
                    migrated.key_prefixes.is_empty(),
                    "{:?}: legacy had no prefix but v3 has {:?}",
                    legacy.id,
                    migrated.key_prefixes,
                ),
                Some(lp) => assert!(
                    migrated.key_prefixes.contains(&lp),
                    "{:?}: legacy key_prefix {lp:?} not in new key_prefixes {:?}",
                    legacy.id,
                    migrated.key_prefixes,
                ),
            }
            assert_eq!(
                migrated.default_model, legacy.default_model,
                "{:?}: default_model diverged",
                legacy.id,
            );
            assert_eq!(
                migrated.cheap_model, legacy.cheap_model,
                "{:?}: cheap_model diverged",
                legacy.id,
            );
            assert_eq!(
                migrated.requires_key, legacy.requires_key,
                "{:?}: requires_key diverged",
                legacy.id,
            );
            // Aliases: every legacy alias must be present in the new slice (order may differ).
            for a in legacy.aliases {
                assert!(
                    migrated.aliases.contains(a),
                    "{:?}: legacy alias {a:?} missing in v3",
                    legacy.id,
                );
            }
        }
    }

    #[test]
    fn legacy_provider_aliases_resolve_in_v3() {
        use crate::data::providers::ALL_PROVIDERS;
        for p in ALL_PROVIDERS {
            for alias in p.aliases {
                let via_alias = find_provider_v3(alias).unwrap_or_else(|| {
                    panic!("v3 lookup by legacy alias {alias:?} failed")
                });
                assert_eq!(via_alias.id, p.id);
            }
        }
    }

    #[test]
    fn every_provider_has_models_with_default_and_cheap() {
        // default_model and cheap_model must each resolve to a wire id that
        // is present in the models list (catalog self-consistency).
        for p in ALL_PROVIDERS_V3 {
            let wires: Vec<&str> = p.models.iter().map(|m| m.model).collect();
            assert!(
                wires.contains(&p.default_model),
                "{:?}: default_model {:?} not in models list (wires: {:?})",
                p.id, p.default_model, wires,
            );
            assert!(
                wires.contains(&p.cheap_model),
                "{:?}: cheap_model {:?} not in models list (wires: {:?})",
                p.id, p.cheap_model, wires,
            );
        }
    }

    #[test]
    fn every_provider_v3_id_roundtrips() {
        for p in ALL_PROVIDERS_V3 {
            assert_eq!(
                find_provider_v3(p.id).map(|x| x.id),
                Some(p.id),
                "provider {:?} not resolvable via PROVIDER_INDEX",
                p.id,
            );
        }
    }

    #[test]
    fn all_model_token_limits_are_non_zero() {
        for p in ALL_PROVIDERS_V3 {
            for m in p.models {
                assert!(
                    m.context_window_tokens > 0 && m.max_output_tokens > 0,
                    "provider {:?} model {:?}: zero token limit",
                    p.id, m.id,
                );
            }
        }
    }

    // ─── Invariants that locked the P0 case-insensitive collision fix ────

    #[test]
    fn no_case_insensitive_collision_in_mcp_keys() {
        // Every id + alias in the MCP catalog must be unique when lowered.
        // This is the runtime mirror of the build.rs check — if someone ever
        // disables build.rs validation, this test still catches the bug.
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for s in ALL_MCP_SERVERS {
            let k = s.id.to_ascii_lowercase();
            assert!(seen.insert(k), "case-insensitive collision on id {:?}", s.id);
            for a in s.aliases {
                let k = a.to_ascii_lowercase();
                assert!(seen.insert(k), "case-insensitive collision on alias {a:?} (server {:?})", s.id);
            }
        }
    }

    #[test]
    fn no_case_insensitive_collision_in_provider_keys() {
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for p in ALL_PROVIDERS_V3 {
            let k = p.id.to_ascii_lowercase();
            assert!(seen.insert(k), "case-insensitive collision on provider id {:?}", p.id);
            for a in p.aliases {
                let k = a.to_ascii_lowercase();
                assert!(seen.insert(k), "case-insensitive collision on alias {a:?} (provider {:?})", p.id);
            }
        }
    }

    #[test]
    fn all_keys_are_ascii() {
        // UniCase::ascii vs UniCase::new hash mismatch would hit non-ASCII
        // keys silently. build.rs rejects them; this test mirrors the rule
        // at runtime.
        for s in ALL_MCP_SERVERS {
            assert!(s.id.is_ascii(), "non-ASCII server id {:?}", s.id);
            for a in s.aliases {
                assert!(a.is_ascii(), "non-ASCII alias {a:?} on server {:?}", s.id);
            }
        }
        for p in ALL_PROVIDERS_V3 {
            assert!(p.id.is_ascii(), "non-ASCII provider id {:?}", p.id);
            for a in p.aliases {
                assert!(a.is_ascii(), "non-ASCII alias {a:?} on provider {:?}", p.id);
            }
        }
    }

    #[test]
    fn runner_only_on_pypi_packages() {
        // Cross-constraint: `runner` is pypi-only. build.rs rejects violations;
        // this test guards against the generated code drifting from that rule.
        use crate::types::RegistryType;
        for s in ALL_MCP_SERVERS {
            for pkg in s.packages {
                match pkg.registry_type {
                    RegistryType::Pypi => assert!(
                        pkg.runner.is_some(),
                        "pypi package {:?} on {:?} missing runner",
                        pkg.identifier, s.id,
                    ),
                    _ => assert!(
                        pkg.runner.is_none(),
                        "non-pypi package {:?} on {:?} has runner set",
                        pkg.identifier, s.id,
                    ),
                }
            }
        }
    }

    #[test]
    fn mixed_case_lookup_resolves() {
        // Probe the phf map with canonical + uppercase + titlecase forms to
        // confirm the build-time `UniCase::ascii` key and the runtime
        // `UniCase::ascii` probe agree.
        for variant in ["filesystem", "FILESYSTEM", "FileSystem", "filesysTEM"] {
            assert_eq!(
                find_mcp_server(variant).map(|s| s.id),
                Some("filesystem"),
                "variant {variant:?} did not resolve",
            );
        }
        for variant in ["anthropic", "ANTHROPIC", "Anthropic", "AnThRoPiC"] {
            assert_eq!(
                find_provider_v3(variant).map(|p| p.id),
                Some("anthropic"),
                "variant {variant:?} did not resolve to anthropic",
            );
        }
    }
}
