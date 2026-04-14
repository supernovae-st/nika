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
//! and materialised at `$OUT_DIR/{mcp_servers,providers}.rs`.
//!
//! Shape:
//!
//! * `ALL_MCP_SERVERS: &[McpServer]` — flat, iteration-friendly.
//! * `MCP_INDEX: phf::Map<UniCase<&'static str>, usize>` — O(1) lookup
//!   from id or alias to an index into `ALL_MCP_SERVERS`.
//! * `ALL_PROVIDERS: &[Provider]` — same pattern for LLM providers.
//! * `PROVIDER_INDEX: phf::Map<UniCase<&'static str>, usize>`.

// `UniCase::ascii(...)` expressions appear in the phf_codegen output.
use unicase::UniCase;

include!(concat!(env!("OUT_DIR"), "/mcp_servers.rs"));
include!(concat!(env!("OUT_DIR"), "/providers.rs"));
include!(concat!(env!("OUT_DIR"), "/embeddings.rs"));

/// Case-insensitive MCP server lookup by id or alias. Returns `None` when unknown.
#[must_use]
pub fn find_mcp_server(name: &str) -> Option<&'static crate::types::McpServer> {
    let idx = *MCP_INDEX.get(&UniCase::ascii(name))?;
    ALL_MCP_SERVERS.get(idx)
}

/// Case-insensitive provider lookup by id or alias. Returns `None` when unknown.
#[must_use]
pub fn find_provider(name: &str) -> Option<&'static crate::types::Provider> {
    let idx = *PROVIDER_INDEX.get(&UniCase::ascii(name))?;
    ALL_PROVIDERS.get(idx)
}

/// Case-insensitive embedding lookup by canonical id. Returns `None` when unknown.
#[must_use]
pub fn find_embedding(id: &str) -> Option<&'static crate::types::Embedding> {
    let idx = *EMBEDDING_INDEX.get(&UniCase::ascii(id))?;
    ALL_EMBEDDINGS.get(idx)
}

#[cfg(test)]
#[allow(clippy::panic, clippy::expect_used)]
mod tests {
    use super::*;

    // ─── Core invariants ─────────────────────────────────────────────

    #[test]
    fn mcp_servers_count_pinned() {
        // Pinned to detect silent additions/removals. Bump intentionally
        // when the catalog gains or drops an entry.
        assert_eq!(ALL_MCP_SERVERS.len(), 105);
    }

    #[test]
    fn providers_count_pinned() {
        // Pinned to detect silent additions/removals.
        assert_eq!(ALL_PROVIDERS.len(), 21);
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

    #[test]
    fn every_server_alias_roundtrips_through_index() {
        for s in ALL_MCP_SERVERS {
            for a in s.aliases {
                assert_eq!(
                    find_mcp_server(a).map(|x| x.id),
                    Some(s.id),
                    "alias {a:?} on server {:?} did not resolve",
                    s.id,
                );
            }
        }
    }

    #[test]
    fn every_provider_id_roundtrips() {
        for p in ALL_PROVIDERS {
            assert_eq!(
                find_provider(p.id).map(|x| x.id),
                Some(p.id),
                "provider {:?} not resolvable via PROVIDER_INDEX",
                p.id,
            );
        }
    }

    #[test]
    fn every_provider_alias_roundtrips() {
        for p in ALL_PROVIDERS {
            for a in p.aliases {
                assert_eq!(
                    find_provider(a).map(|x| x.id),
                    Some(p.id),
                    "alias {a:?} on provider {:?} did not resolve",
                    p.id,
                );
            }
        }
    }

    #[test]
    fn every_provider_has_models_with_default_and_cheap() {
        for p in ALL_PROVIDERS {
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
    fn all_model_token_limits_are_non_zero() {
        for p in ALL_PROVIDERS {
            for m in p.models {
                assert!(
                    m.context_window_tokens > 0 && m.max_output_tokens > 0,
                    "provider {:?} model {:?}: zero token limit",
                    p.id, m.id,
                );
            }
        }
    }

    // ─── Case-insensitive lookup invariants ─────────────────────────

    #[test]
    fn no_case_insensitive_collision_in_mcp_keys() {
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
        for p in ALL_PROVIDERS {
            let k = p.id.to_ascii_lowercase();
            assert!(seen.insert(k), "case-insensitive collision on provider id {:?}", p.id);
            for a in p.aliases {
                let k = a.to_ascii_lowercase();
                assert!(seen.insert(k), "case-insensitive collision on alias {a:?} (provider {:?})", p.id);
            }
        }
    }

    #[test]
    fn no_unexpected_cross_catalog_collision() {
        // Services that are BOTH an LLM provider AND an MCP server (e.g.
        // perplexity = sonar LLM + search tool) are legitimate. This test
        // only flags UNEXPECTED overlaps — extend the allow-list when a
        // new dual-role service is added deliberately.
        const KNOWN_OVERLAP: &[&str] = &["perplexity"];
        let mut mcp_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for s in ALL_MCP_SERVERS {
            mcp_keys.insert(s.id.to_ascii_lowercase());
            for a in s.aliases {
                mcp_keys.insert(a.to_ascii_lowercase());
            }
        }
        for p in ALL_PROVIDERS {
            let id_k = p.id.to_ascii_lowercase();
            assert!(
                !mcp_keys.contains(&id_k) || KNOWN_OVERLAP.contains(&id_k.as_str()),
                "provider id {:?} also exists as MCP key — if intentional, add to KNOWN_OVERLAP",
                p.id,
            );
            for a in p.aliases {
                let ak = a.to_ascii_lowercase();
                assert!(
                    !mcp_keys.contains(&ak) || KNOWN_OVERLAP.contains(&ak.as_str()),
                    "provider alias {a:?} (on {:?}) also exists as MCP key — if intentional, add to KNOWN_OVERLAP",
                    p.id,
                );
            }
        }
    }

    #[test]
    fn all_keys_are_ascii() {
        for s in ALL_MCP_SERVERS {
            assert!(s.id.is_ascii(), "non-ASCII server id {:?}", s.id);
            for a in s.aliases {
                assert!(a.is_ascii(), "non-ASCII alias {a:?} on server {:?}", s.id);
            }
        }
        for p in ALL_PROVIDERS {
            assert!(p.id.is_ascii(), "non-ASCII provider id {:?}", p.id);
            for a in p.aliases {
                assert!(a.is_ascii(), "non-ASCII alias {a:?} on provider {:?}", p.id);
            }
        }
    }

    #[test]
    fn mixed_case_lookup_resolves() {
        for variant in ["filesystem", "FILESYSTEM", "FileSystem", "filesysTEM"] {
            assert_eq!(
                find_mcp_server(variant).map(|s| s.id),
                Some("filesystem"),
                "variant {variant:?} did not resolve",
            );
        }
        for variant in ["anthropic", "ANTHROPIC", "Anthropic", "AnThRoPiC"] {
            assert_eq!(
                find_provider(variant).map(|p| p.id),
                Some("anthropic"),
                "variant {variant:?} did not resolve to anthropic",
            );
        }
    }

    // ─── Distribution model invariants ───────────────────────────────

    #[test]
    fn runner_only_on_pypi_packages() {
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
    fn restored_entries_use_new_distributions() {
        use crate::types::{AuthMode, PyRunner, RegistryType};
        let qdrant = find_mcp_server("qdrant").expect("qdrant restored");
        assert_eq!(qdrant.packages[0].registry_type, RegistryType::Pypi);
        assert_eq!(qdrant.packages[0].runner, Some(PyRunner::Uvx));

        let intercom = find_mcp_server("intercom").expect("intercom restored");
        assert!(intercom.packages.is_empty());
        assert_eq!(intercom.remotes[0].auth, AuthMode::OAuth);

        let grafana = find_mcp_server("grafana").expect("grafana restored");
        assert_eq!(grafana.packages[0].registry_type, RegistryType::Oci);
    }

    // ─── Embeddings ──────────────────────────────────────────────────

    #[test]
    fn embeddings_non_empty() {
        assert!(!ALL_EMBEDDINGS.is_empty());
    }

    #[test]
    fn every_embedding_roundtrips() {
        for e in ALL_EMBEDDINGS {
            assert_eq!(
                find_embedding(e.id).map(|x| x.id),
                Some(e.id),
                "embedding {:?} not resolvable via EMBEDDING_INDEX",
                e.id,
            );
        }
    }

    #[test]
    fn every_embedding_references_known_provider() {
        for e in ALL_EMBEDDINGS {
            assert!(
                find_provider(e.provider).is_some(),
                "embedding {:?} references unknown provider {:?}",
                e.id, e.provider,
            );
        }
    }

    #[test]
    fn embedding_dimensions_are_positive() {
        for e in ALL_EMBEDDINGS {
            assert!(e.dimensions > 0, "embedding {:?} has zero dimensions", e.id);
            assert!(
                e.max_input_tokens > 0,
                "embedding {:?} has zero max_input_tokens",
                e.id,
            );
        }
    }

    #[test]
    fn embedding_pricing_non_negative_finite() {
        for e in ALL_EMBEDDINGS {
            assert!(
                e.input_per_million.is_finite() && e.input_per_million >= 0.0,
                "embedding {:?} has invalid price {}",
                e.id, e.input_per_million,
            );
        }
    }

    #[test]
    fn voyage_3_large_is_1024_dims() {
        // Compile-time compatibility check — voyage-3-large is famously 1024.
        // If this test fails, either the catalog drifted or Voyage changed
        // its API (rare, bump schema version and update every consumer).
        let e = find_embedding("voyage/voyage-3-large").expect("voyage-3-large present");
        assert_eq!(e.dimensions, 1024);
    }

    // ─── Banned npm packages ────────────────────────────────────────

    #[test]
    fn banned_aliases_must_not_ship_an_npm_package() {
        // These 26 aliases are known to NOT exist on npm (deprecated
        // Anthropic reference servers, fabricated package names, or
        // zero-download abandoned forks). They MAY ship in this catalog
        // if a non-npm distribution exists (pypi, oci, remote), but they
        // MUST NEVER be re-added as an npm package — npm install would
        // fail or pull deprecated code.
        use crate::types::RegistryType;
        let banned_on_npm = [
            "puppeteer", "brave-search", "brave", "google-maps",
            "github", "gitlab", "postgres", "neon",
            "fetch", "sqlite", "stability", "deepgram", "weaviate",
            "chroma", "milvus", "posthog", "coinbase", "fly", "asana",
            "raygun", "buildkite", "bing", "elevenlabs",
            "todoist", "trello", "semrush",
        ];
        for alias in banned_on_npm {
            if let Some(s) = find_mcp_server(alias) {
                let has_npm = s.packages.iter().any(|p| p.registry_type == RegistryType::Npm);
                assert!(
                    !has_npm,
                    "{alias:?}: banned from shipping as npm — verify package exists and use a non-npm distribution if reviving",
                );
            }
        }
    }
}
