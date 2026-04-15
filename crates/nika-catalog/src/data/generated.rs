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
#[cfg(any(feature = "mcp", feature = "providers", feature = "embeddings"))]
use unicase::UniCase;

#[cfg(feature = "mcp")]
include!(concat!(env!("OUT_DIR"), "/mcp_servers.rs"));

#[cfg(feature = "providers")]
include!(concat!(env!("OUT_DIR"), "/providers.rs"));

#[cfg(feature = "embeddings")]
include!(concat!(env!("OUT_DIR"), "/embeddings.rs"));

#[cfg(feature = "capabilities")]
include!(concat!(env!("OUT_DIR"), "/model_capabilities.rs"));

/// Case-insensitive MCP server lookup by id or alias. Returns `None` when unknown.
#[cfg(feature = "mcp")]
#[must_use]
pub fn find_mcp_server(name: &str) -> Option<&'static crate::types::McpServer> {
    let idx = *MCP_INDEX.get(&UniCase::ascii(name))?;
    ALL_MCP_SERVERS.get(idx)
}

/// Case-insensitive provider lookup by id or alias. Returns `None` when unknown.
#[cfg(feature = "providers")]
#[must_use]
pub fn find_provider(name: &str) -> Option<&'static crate::types::Provider> {
    let idx = *PROVIDER_INDEX.get(&UniCase::ascii(name))?;
    ALL_PROVIDERS.get(idx)
}

/// Case-insensitive embedding lookup by canonical id. Returns `None` when unknown.
#[cfg(feature = "embeddings")]
#[must_use]
pub fn find_embedding(id: &str) -> Option<&'static crate::types::Embedding> {
    let idx = *EMBEDDING_INDEX.get(&UniCase::ascii(id))?;
    ALL_EMBEDDINGS.get(idx)
}

// Tests require all three generated catalogs (cross-catalog invariants,
// provider FK on embeddings, etc.). Only runs under the `full` feature set.
#[cfg(all(test, feature = "mcp", feature = "providers", feature = "embeddings"))]
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
        // Pinned to detect silent additions/removals. Session 2b follow-up
        // added 4 Chinese frontier providers (moonshot/qwen/minimax/zhipu)
        // driving the count from 21 to 25.
        assert_eq!(ALL_PROVIDERS.len(), 25);
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
                p.id,
                p.default_model,
                wires,
            );
            assert!(
                wires.contains(&p.cheap_model),
                "{:?}: cheap_model {:?} not in models list (wires: {:?})",
                p.id,
                p.cheap_model,
                wires,
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
                    p.id,
                    m.id,
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
            assert!(
                seen.insert(k),
                "case-insensitive collision on id {:?}",
                s.id
            );
            for a in s.aliases {
                let k = a.to_ascii_lowercase();
                assert!(
                    seen.insert(k),
                    "case-insensitive collision on alias {a:?} (server {:?})",
                    s.id
                );
            }
        }
    }

    #[test]
    fn no_case_insensitive_collision_in_provider_keys() {
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for p in ALL_PROVIDERS {
            let k = p.id.to_ascii_lowercase();
            assert!(
                seen.insert(k),
                "case-insensitive collision on provider id {:?}",
                p.id
            );
            for a in p.aliases {
                let k = a.to_ascii_lowercase();
                assert!(
                    seen.insert(k),
                    "case-insensitive collision on alias {a:?} (provider {:?})",
                    p.id
                );
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
                        pkg.identifier,
                        s.id,
                    ),
                    _ => assert!(
                        pkg.runner.is_none(),
                        "non-pypi package {:?} on {:?} has runner set",
                        pkg.identifier,
                        s.id,
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
                e.id,
                e.provider,
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
                e.id,
                e.input_per_million,
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
            "puppeteer",
            "brave-search",
            "brave",
            "google-maps",
            "github",
            "gitlab",
            "postgres",
            "neon",
            "fetch",
            "sqlite",
            "stability",
            "deepgram",
            "weaviate",
            "chroma",
            "milvus",
            "posthog",
            "coinbase",
            "fly",
            "asana",
            "raygun",
            "buildkite",
            "bing",
            "elevenlabs",
            "todoist",
            "trello",
            "semrush",
        ];
        for alias in banned_on_npm {
            if let Some(s) = find_mcp_server(alias) {
                let has_npm = s
                    .packages
                    .iter()
                    .any(|p| p.registry_type == RegistryType::Npm);
                assert!(
                    !has_npm,
                    "{alias:?}: banned from shipping as npm — verify package exists and use a non-npm distribution if reviving",
                );
            }
        }
    }

    // ─── Tag invariants (runtime end-to-end checks) ─────────────────

    #[test]
    fn every_mcp_has_safety_tag_xor() {
        // Mirrors the build.rs `validate_mcp_safety_tag` invariant — runtime
        // belt-and-braces so a build-time bypass (e.g. cached build, edited
        // generated.rs by hand) still trips on `cargo test`.
        use crate::types::Tag;
        for s in ALL_MCP_SERVERS {
            let ro = s.tags.contains(&Tag::ReadOnly);
            let dest = s.tags.contains(&Tag::Destructive);
            assert!(
                ro ^ dest,
                "MCP {:?}: must have exactly ONE of ReadOnly / Destructive (got ro={ro}, destructive={dest})",
                s.id,
            );
        }
    }

    #[test]
    fn no_provider_carries_both_budget_and_frontier() {
        use crate::types::Tag;
        for p in ALL_PROVIDERS {
            let budget = p.tags.contains(&Tag::Budget);
            let frontier = p.tags.contains(&Tag::Frontier);
            assert!(
                !(budget && frontier),
                "provider {:?}: Budget and Frontier are mutually exclusive",
                p.id,
            );
        }
    }

    #[test]
    fn every_embedding_carries_embedding_or_reranker() {
        use crate::types::Tag;
        for e in ALL_EMBEDDINGS {
            let emb = e.tags.contains(&Tag::Embedding);
            let rerank = e.tags.contains(&Tag::Reranker);
            assert!(
                emb || rerank,
                "embedding {:?}: must carry Tag::Embedding or Tag::Reranker",
                e.id,
            );
        }
    }

    #[test]
    fn every_entry_tags_are_sorted_and_unique() {
        // Sort order is **kebab-case alphabetical** (matches build.rs
        // `validate_tags`), NOT `Tag::Ord` declaration order — these differ
        // (e.g. Tag::Official < Tag::ReadOnly by declaration but
        // "official" < "read-only" by string).
        for s in ALL_MCP_SERVERS {
            for w in s.tags.windows(2) {
                assert!(
                    w[0].as_str() < w[1].as_str(),
                    "MCP {:?}: tags not kebab-sorted ({:?} >= {:?})",
                    s.id,
                    w[0].as_str(),
                    w[1].as_str(),
                );
            }
        }
        for p in ALL_PROVIDERS {
            for w in p.tags.windows(2) {
                assert!(
                    w[0].as_str() < w[1].as_str(),
                    "provider {:?}: tags not kebab-sorted",
                    p.id,
                );
            }
        }
        for e in ALL_EMBEDDINGS {
            for w in e.tags.windows(2) {
                assert!(
                    w[0].as_str() < w[1].as_str(),
                    "embedding {:?}: tags not kebab-sorted",
                    e.id,
                );
            }
        }
    }

    #[test]
    fn extra_tags_are_passthrough_and_unrestricted() {
        // Sanity: every entry has an `extra_tags` slice (possibly empty),
        // accessible via the public field — this exercises the Cargo feature
        // gate at runtime and prevents the field from being silently dropped.
        for s in ALL_MCP_SERVERS {
            let _: &[&'static str] = s.extra_tags;
        }
        for p in ALL_PROVIDERS {
            let _: &[&'static str] = p.extra_tags;
        }
        for e in ALL_EMBEDDINGS {
            let _: &[&'static str] = e.extra_tags;
        }
    }

    #[test]
    fn anthropic_provider_carries_expected_tags() {
        // Spot-check that Commit 3's curated tag assignment survives codegen.
        use crate::types::Tag;
        let p = find_provider("anthropic").expect("anthropic provider present");
        assert!(
            p.tags.contains(&Tag::Frontier),
            "anthropic should carry Frontier"
        );
        assert!(
            p.tags.contains(&Tag::PromptCaching),
            "anthropic should carry PromptCaching"
        );
        assert!(
            p.tags.contains(&Tag::Vision),
            "anthropic should carry Vision"
        );
        assert!(
            p.tags.contains(&Tag::ExtendedThinking),
            "anthropic should carry ExtendedThinking"
        );
    }

    #[test]
    fn stripe_mcp_carries_official_and_destructive() {
        // Spot-check that vendor-published MCPs are properly flagged.
        use crate::types::Tag;
        let s = find_mcp_server("stripe").expect("stripe MCP present");
        assert!(s.tags.contains(&Tag::Official), "stripe should be Official");
        assert!(
            s.tags.contains(&Tag::Destructive),
            "stripe should be Destructive (payments)"
        );
    }
}
