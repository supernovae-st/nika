// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Embeddings catalog codegen — TOML schema, FK validation, Rust emission.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use crate::emit::{rstr, str_slice_expr};
use crate::error::CodegenError;
use crate::providers::ProviderEntry;
use crate::schema::{EMBEDDINGS_SCHEMA, assert_schema};
use crate::tags::{tags_slice_expr, validate_tags};

// ─── TOML schema types ──────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct EmbeddingsFile {
    pub schema: String,
    pub embeddings: Vec<EmbeddingEntry>,
}

#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct EmbeddingEntry {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub dimensions: u32,
    pub max_input_tokens: u32,
    pub normalized_by_default: bool,
    pub similarity: String,
    pub input_per_million: f64,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub extra_tags: Vec<String>,
}

// ─── Public codegen entry ───────────────────────────────────────────────

pub fn parse_embeddings_bytes(
    raw: &[u8],
    path: &Path,
    providers: &[ProviderEntry],
) -> Result<Vec<EmbeddingEntry>, CodegenError> {
    let raw_str = std::str::from_utf8(raw).map_err(|e| CodegenError::SchemaValidation {
        context: path.display().to_string(),
        reason: format!("file is not valid UTF-8: {e}"),
    })?;
    let file: EmbeddingsFile =
        toml::from_str(raw_str).map_err(|source| CodegenError::TomlParse {
            path: path.to_path_buf(),
            source,
        })?;

    assert_schema(EMBEDDINGS_SCHEMA, &file.schema, path)?;

    validate_embeddings(&file.embeddings, providers, path)?;
    Ok(file.embeddings)
}

/// Codegen entry — parse TOML bytes + emit Rust source. The FK check
/// requires the canonical providers list (parsed from `llm-providers.toml`).
pub fn codegen_embeddings(
    toml_bytes: &[u8],
    providers: &[ProviderEntry],
) -> Result<String, CodegenError> {
    let synthetic = Path::new("<bytes>:embeddings.toml");
    let entries = parse_embeddings_bytes(toml_bytes, synthetic, providers)?;
    Ok(generate_embeddings_rs(&entries))
}

// ─── Validation ─────────────────────────────────────────────────────────

fn validate_embeddings(
    entries: &[EmbeddingEntry],
    providers: &[ProviderEntry],
    path: &Path,
) -> Result<(), CodegenError> {
    let ctx = path.display().to_string();
    let known_providers: HashSet<String> = providers
        .iter()
        .map(|p| p.id.to_ascii_lowercase())
        .collect();

    let mut seen_ids: HashSet<String> = HashSet::new();
    for e in entries {
        assert_ascii_key("embedding id", &e.id)
            .map_err(|r| CodegenError::schema_validation(&ctx, r))?;
        if !seen_ids.insert(e.id.to_ascii_lowercase()) {
            return Err(CodegenError::schema_validation(
                &ctx,
                format!("duplicate embedding id: {:?}", e.id),
            ));
        }
        if e.dimensions == 0 {
            return Err(CodegenError::schema_validation(
                &ctx,
                format!("embedding {:?}: dimensions must be > 0", e.id),
            ));
        }
        if e.max_input_tokens == 0 {
            return Err(CodegenError::schema_validation(
                &ctx,
                format!("embedding {:?}: max_input_tokens must be > 0", e.id),
            ));
        }
        if !e.input_per_million.is_finite() || e.input_per_million < 0.0 {
            return Err(CodegenError::schema_validation(
                &ctx,
                format!(
                    "embedding {:?}: input_per_million must be a finite non-negative number",
                    e.id
                ),
            ));
        }
        if e.model.is_empty() {
            return Err(CodegenError::schema_validation(
                &ctx,
                format!("embedding {:?}: model wire id is empty", e.id),
            ));
        }
        if e.description.is_empty() {
            return Err(CodegenError::schema_validation(
                &ctx,
                format!("embedding {:?}: description is empty", e.id),
            ));
        }
        validate_similarity(&e.similarity, &e.id)
            .map_err(|r| CodegenError::schema_validation(&ctx, r))?;
        validate_tags(&e.tags, &e.id).map_err(|r| CodegenError::schema_validation(&ctx, r))?;
        if !known_providers.contains(&e.provider.to_ascii_lowercase()) {
            return Err(CodegenError::foreign_key(
                format!("embedding {:?}", e.id),
                format!(
                    "provider {:?} is not declared in llm-providers.toml",
                    e.provider
                ),
            ));
        }
    }
    Ok(())
}

fn validate_similarity(s: &str, id: &str) -> Result<(), String> {
    match s {
        "cosine" | "dot-product" | "l2" => Ok(()),
        _ => Err(format!("embedding {id:?}: unknown similarity {s:?}")),
    }
}

fn similarity_variant(s: &str) -> &'static str {
    match s {
        "cosine" => "Cosine",
        "dot-product" => "DotProduct",
        "l2" => "L2",
        _ => panic_validated("similarity", s),
    }
}

fn assert_ascii_key(kind: &str, s: &str) -> Result<(), String> {
    if !s.is_ascii() {
        return Err(format!(
            "{kind} {s:?} contains non-ASCII characters (ASCII-only ids/aliases required for case-insensitive phf lookup)"
        ));
    }
    if s.is_empty() {
        return Err(format!("{kind} is empty"));
    }
    Ok(())
}

#[allow(
    clippy::panic,
    reason = "validated upstream — branch is unreachable in valid input"
)]
fn panic_validated(field: &str, value: &str) -> ! {
    panic!(
        "internal invariant violated: {field}={value:?} reached variant mapping without prior validation"
    );
}

// ─── Rust source emission ───────────────────────────────────────────────

#[must_use]
pub(crate) fn generate_embeddings_rs(embeddings: &[EmbeddingEntry]) -> String {
    let mut out = String::with_capacity(4_096);
    let _ = writeln!(
        out,
        "// GENERATED by build.rs from data/embeddings.toml. DO NOT EDIT."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "pub static ALL_EMBEDDINGS: &[crate::types::Embedding] = &["
    );

    for e in embeddings {
        let _ = writeln!(out, "    crate::types::Embedding {{");
        let _ = writeln!(out, "        id: {},", rstr(&e.id));
        let _ = writeln!(out, "        provider: {},", rstr(&e.provider));
        let _ = writeln!(out, "        model: {},", rstr(&e.model));
        let _ = writeln!(out, "        dimensions: {},", e.dimensions);
        let _ = writeln!(out, "        max_input_tokens: {},", e.max_input_tokens);
        let _ = writeln!(
            out,
            "        normalized_by_default: {},",
            e.normalized_by_default
        );
        let _ = writeln!(
            out,
            "        similarity: crate::types::Similarity::{},",
            similarity_variant(&e.similarity)
        );
        let _ = writeln!(out, "        input_per_million: {:?},", e.input_per_million);
        let _ = writeln!(out, "        description: {},", rstr(&e.description));
        let _ = writeln!(out, "        tags: {},", tags_slice_expr(&e.tags));
        let _ = writeln!(
            out,
            "        extra_tags: {},",
            str_slice_expr(&e.extra_tags)
        );
        let _ = writeln!(out, "    }},");
    }

    let _ = writeln!(out, "];");
    let _ = writeln!(out);

    let mut builder = phf_codegen::Map::<unicase::UniCase<&str>>::new();
    let entries: Vec<(String, String)> = embeddings
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i.to_string()))
        .collect();
    for (k, v) in &entries {
        builder.entry(unicase::UniCase::ascii(k.as_str()), v.as_str());
    }
    let _ = writeln!(
        out,
        "pub static EMBEDDING_INDEX: phf::Map<unicase::UniCase<&'static str>, usize> = {};",
        builder.build()
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderEntry;

    fn fixture_provider() -> ProviderEntry {
        ProviderEntry {
            id: "demo".to_string(),
            name: "Demo".to_string(),
            aliases: vec![],
            env_var: "X".to_string(),
            key_prefixes: vec![],
            default_model: "m".to_string(),
            cheap_model: "m".to_string(),
            requires_key: false,
            description: "x".to_string(),
            models: vec![],
            api_dialect: None,
            tags: vec![],
            extra_tags: vec![],
        }
    }

    fn fixture_embedding() -> EmbeddingEntry {
        EmbeddingEntry {
            id: "demo-emb".to_string(),
            provider: "demo".to_string(),
            model: "demo-emb-v1".to_string(),
            dimensions: 1536,
            max_input_tokens: 8192,
            normalized_by_default: true,
            similarity: "cosine".to_string(),
            input_per_million: 0.1,
            description: "demo embedding".to_string(),
            tags: vec![],
            extra_tags: vec![],
        }
    }

    #[test]
    fn validate_similarity_closed_set() {
        for ok in ["cosine", "dot-product", "l2"] {
            assert!(validate_similarity(ok, "x").is_ok());
        }
        assert!(validate_similarity("nope", "x").is_err());
    }

    #[test]
    fn validate_embeddings_fk_check_passes() {
        let providers = vec![fixture_provider()];
        let embeddings = vec![fixture_embedding()];
        validate_embeddings(&embeddings, &providers, Path::new("/x")).unwrap();
    }

    #[test]
    fn validate_embeddings_fk_check_fails() {
        let providers: Vec<ProviderEntry> = vec![]; // no providers
        let embeddings = vec![fixture_embedding()];
        let err = validate_embeddings(&embeddings, &providers, Path::new("/x")).unwrap_err();
        match err {
            CodegenError::ForeignKey { reason, .. } => {
                assert!(reason.contains("not declared"));
            }
            other => panic!("expected ForeignKey, got {other:?}"),
        }
    }

    #[test]
    fn validate_embeddings_negative_input_rate_rejected() {
        let providers = vec![fixture_provider()];
        let mut e = fixture_embedding();
        e.input_per_million = -1.0;
        let err = validate_embeddings(&[e], &providers, Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("non-negative"), "got: {msg}");
    }

    #[test]
    fn validate_embeddings_zero_dimensions_rejected() {
        let providers = vec![fixture_provider()];
        let mut e = fixture_embedding();
        e.dimensions = 0;
        let err = validate_embeddings(&[e], &providers, Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("dimensions"), "got: {msg}");
    }

    #[test]
    fn validate_embeddings_duplicate_id_rejected() {
        let providers = vec![fixture_provider()];
        let e1 = fixture_embedding();
        let e2 = fixture_embedding();
        let err = validate_embeddings(&[e1, e2], &providers, Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duplicate embedding id"), "got: {msg}");
    }

    #[test]
    fn generate_emits_static_names() {
        let s = generate_embeddings_rs(&[fixture_embedding()]);
        assert!(s.contains("pub static ALL_EMBEDDINGS"));
        assert!(s.contains("pub static EMBEDDING_INDEX"));
        assert!(s.contains("crate::types::Embedding"));
        assert!(s.contains("crate::types::Similarity::Cosine"));
    }

    #[test]
    fn idempotent_emission_embeddings() {
        let e = fixture_embedding();
        let a = generate_embeddings_rs(&[fixture_embedding()]);
        let b = generate_embeddings_rs(&[e]);
        assert_eq!(a, b);
    }

    // ─── Mutation-kill arc 2026-06-11 (vector 39 · 47-survivor sweep) ────────

    #[test]
    fn similarity_variant_exhaustive() {
        assert_eq!(similarity_variant("cosine"), "Cosine");
        assert_eq!(similarity_variant("dot-product"), "DotProduct");
        assert_eq!(similarity_variant("l2"), "L2");
    }

    #[test]
    fn assert_ascii_key_rejects_non_ascii_and_empty() {
        assert!(assert_ascii_key("id", "ok-id").is_ok());
        assert!(assert_ascii_key("id", "café").is_err());
        assert!(assert_ascii_key("id", "").is_err());
    }
}
