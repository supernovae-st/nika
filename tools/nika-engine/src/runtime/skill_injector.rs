//! Skill Injector - Prepends skill content to agent system prompts
//!
//! The SkillInjector loads skill files (both local and pkg: URIs) and caches them
//! for efficient reuse. When an agent task specifies skills, the injector prepends
//! the skill content to the agent's system prompt.
//!
//! # Example
//!
//! ```yaml
//! skills:
//!   seo: ./skills/seo-writer.skill.md
//!   brand: pkg:@supernovae/skills@1.0.0/brand.md
//!
//! tasks:
//!   - id: generate
//!     agent:
//!       prompt: "Write content"
//!       skills: [seo, brand]  # Skills injected into system prompt
//! ```
//!
//! # Architecture
//!
//! ```text
//! Workflow start:
//!   1. Parse skills: block → HashMap<alias, path>
//!   2. Resolve paths (local or pkg: URI)
//!
//! Agent task execution:
//!   3. SkillInjector.inject(system_prompt, skill_names, skills_map, base_dir)
//!   4. For each skill: load from cache or read file
//!   5. Prepend skill content to system prompt
//! ```

use dashmap::DashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tracing::debug;

use crate::ast::skill_def::resolve_skill_path;
use crate::error::NikaError;

/// Thread-safe skill content cache
///
/// Uses DashMap for concurrent access without external locking.
/// Keys are cache keys (resolved path string), values are skill content.
pub struct SkillInjector {
    /// Cache: resolved_path -> file content
    cache: DashMap<String, Arc<str>>,
    /// Integrity hashes: skill_path -> expected blake3 hash (from nika.toml [skills.integrity])
    integrity_map: std::collections::HashMap<String, String>,
}

impl SkillInjector {
    /// Create a new SkillInjector with empty cache
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
            integrity_map: std::collections::HashMap::new(),
        }
    }

    /// Create a SkillInjector with integrity verification (Nika Shield).
    ///
    /// The integrity map contains skill_path -> expected blake3 hash pairs
    /// from `[skills.integrity]` in nika.toml.
    pub fn with_integrity(integrity: std::collections::HashMap<String, String>) -> Self {
        Self {
            cache: DashMap::new(),
            integrity_map: integrity,
        }
    }

    /// Load a skill file, using cache if available
    ///
    /// # Arguments
    /// * `skill_path` - The skill path from YAML (local path or `pkg:` URI)
    /// * `base_dir` - Base directory for resolving relative local paths
    ///
    /// # Returns
    /// * `Ok(Arc<str>)` - Skill file content (from cache or freshly loaded)
    /// * `Err(NikaError::SkillLoadError)` - If file cannot be read
    ///
    /// # Example
    /// ```ignore
    /// let injector = SkillInjector::new();
    /// let content = injector.load_skill("./skills/seo.skill.md", Path::new("/project")).await?;
    /// ```
    pub async fn load_skill(
        &self,
        skill_path: &str,
        base_dir: &Path,
    ) -> Result<Arc<str>, NikaError> {
        // Resolve the skill path (handles both local and pkg: URIs)
        let resolved_path = resolve_skill_path(skill_path, base_dir)?;
        let cache_key = resolved_path.to_string_lossy().to_string();

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key) {
            debug!(skill_path = %skill_path, "Skill loaded from cache");
            return Ok(Arc::clone(&cached));
        }

        // SECURITY: Pre-read size check to prevent DoS via huge skill files
        const MAX_SKILL_BYTES: u64 = 1_048_576; // 1 MiB
        let metadata =
            fs::metadata(&resolved_path)
                .await
                .map_err(|e| NikaError::SkillLoadError {
                    skill: skill_path.to_string(),
                    reason: format!("Cannot stat file '{}': {}", resolved_path.display(), e),
                })?;
        if metadata.len() > MAX_SKILL_BYTES {
            return Err(NikaError::SkillLoadError {
                skill: skill_path.to_string(),
                reason: format!(
                    "Skill file too large: {} bytes (max {} bytes)",
                    metadata.len(),
                    MAX_SKILL_BYTES
                ),
            });
        }

        // Load file content
        let content =
            fs::read_to_string(&resolved_path)
                .await
                .map_err(|e| NikaError::SkillLoadError {
                    skill: skill_path.to_string(),
                    reason: format!("Failed to read file '{}': {}", resolved_path.display(), e),
                })?;

        // Nika Shield: integrity verification (NIKA-271)
        if let Some(expected_hash) = self.integrity_map.get(skill_path) {
            let actual_hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            if actual_hash != *expected_hash {
                return Err(NikaError::SkillLoadError {
                    skill: skill_path.to_string(),
                    reason: format!(
                        "[NIKA-271] Skill integrity mismatch: expected {}, got {}",
                        expected_hash, actual_hash
                    ),
                });
            }
            debug!(skill_path = %skill_path, hash = %actual_hash, "Skill integrity verified");
        }

        let content: Arc<str> = content.into();

        // Cache for future use
        self.cache.insert(cache_key, Arc::clone(&content));
        debug!(skill_path = %skill_path, resolved = %resolved_path.display(), "Skill loaded and cached");

        Ok(content)
    }

    /// Inject skills into a system prompt
    ///
    /// Loads each referenced skill and prepends it to the base system prompt.
    /// Skills are separated by newlines and clearly marked with headers.
    ///
    /// # Arguments
    /// * `base_prompt` - The agent's original system prompt (may be None)
    /// * `skill_names` - List of skill aliases to inject
    /// * `skills_map` - Workflow-level skills: HashMap<alias, path>
    /// * `base_dir` - Base directory for resolving relative paths
    ///
    /// # Returns
    /// * `Ok(String)` - Complete system prompt with skills prepended
    /// * `Err(NikaError)` - If any skill fails to load
    ///
    /// # Example
    /// ```ignore
    /// let injector = SkillInjector::new();
    /// let skills_map = [("seo".to_string(), "./skills/seo.md".to_string())].into();
    /// let prompt = injector.inject(
    ///     Some("Be helpful"),
    ///     &["seo"],
    ///     &skills_map,
    ///     Path::new("/project"),
    /// ).await?;
    /// ```
    pub async fn inject(
        &self,
        base_prompt: Option<&str>,
        skill_names: &[&str],
        skills_map: &std::collections::HashMap<String, String>,
        base_dir: &Path,
    ) -> Result<String, NikaError> {
        if skill_names.is_empty() {
            // No skills to inject - return base prompt or empty string
            return Ok(base_prompt.unwrap_or_default().to_string());
        }

        let mut parts: Vec<String> = Vec::with_capacity(skill_names.len() + 1);

        // Load each skill
        for skill_name in skill_names {
            // Get path from skills map
            let skill_path =
                skills_map
                    .get(*skill_name)
                    .ok_or_else(|| NikaError::SkillLoadError {
                        skill: skill_name.to_string(),
                        reason: format!(
                            "Skill '{}' not found in workflow skills: block. Available: {:?}",
                            skill_name,
                            skills_map.keys().collect::<Vec<_>>()
                        ),
                    })?;

            // Load skill content (uses cache)
            let content = self.load_skill(skill_path, base_dir).await?;
            // Add skill with header for clarity
            // Trim trailing whitespace from content to prevent double newlines
            parts.push(format!(
                "# Skill: {}\n\n{}",
                skill_name,
                content.as_ref().trim_end()
            ));
        }

        // Add base prompt at the end (if present)
        if let Some(base) = base_prompt {
            if !base.is_empty() {
                parts.push(base.to_string());
            }
        }

        Ok(parts.join("\n"))
    }

    /// Clear the skill cache (useful for testing or hot-reloading)
    pub fn clear_cache(&self) {
        self.cache.clear();
        debug!("Skill cache cleared");
    }

    /// Get the number of cached skills
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Check if a skill is cached
    pub fn is_cached(&self, skill_path: &str, base_dir: &Path) -> bool {
        if let Ok(resolved) = resolve_skill_path(skill_path, base_dir) {
            let cache_key = resolved.to_string_lossy().to_string();
            self.cache.contains_key(&cache_key)
        } else {
            false
        }
    }
}

impl Default for SkillInjector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tokio::fs::write;

    async fn setup_test_skills() -> (TempDir, HashMap<String, String>) {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        tokio::fs::create_dir_all(&skills_dir).await.unwrap();

        // Create test skill files
        let seo_path = skills_dir.join("seo.skill.md");
        write(
            &seo_path,
            "# SEO Writer\n\nYou are an expert SEO content writer.\n",
        )
        .await
        .unwrap();

        let brand_path = skills_dir.join("brand.skill.md");
        write(
            &brand_path,
            "# Brand Voice\n\nMaintain a friendly, professional tone.\n",
        )
        .await
        .unwrap();

        let mut skills_map = HashMap::new();
        skills_map.insert("seo".to_string(), "./skills/seo.skill.md".to_string());
        skills_map.insert("brand".to_string(), "./skills/brand.skill.md".to_string());

        (temp_dir, skills_map)
    }

    #[tokio::test]
    async fn test_load_skill_success() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        let content = injector
            .load_skill(skills_map.get("seo").unwrap(), temp_dir.path())
            .await
            .unwrap();

        assert!(content.contains("SEO Writer"));
        assert!(content.contains("expert SEO content writer"));
    }

    #[tokio::test]
    async fn test_load_skill_caching() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        // First load
        let content1 = injector
            .load_skill(skills_map.get("seo").unwrap(), temp_dir.path())
            .await
            .unwrap();

        assert_eq!(injector.cache_size(), 1);

        // Second load (should use cache)
        let content2 = injector
            .load_skill(skills_map.get("seo").unwrap(), temp_dir.path())
            .await
            .unwrap();

        // Arc pointers should be the same (same cached instance)
        assert!(Arc::ptr_eq(&content1, &content2));
    }

    #[tokio::test]
    async fn test_load_skill_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let injector = SkillInjector::new();

        let result = injector
            .load_skill("./nonexistent.skill.md", temp_dir.path())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::SkillLoadError { .. }));
    }

    #[tokio::test]
    async fn test_inject_single_skill() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        let result = injector
            .inject(Some("Be helpful"), &["seo"], &skills_map, temp_dir.path())
            .await
            .unwrap();

        assert!(result.contains("# Skill: seo"));
        assert!(result.contains("SEO Writer"));
        assert!(result.contains("Be helpful"));
    }

    #[tokio::test]
    async fn test_inject_multiple_skills() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        let result = injector
            .inject(
                Some("Base prompt"),
                &["seo", "brand"],
                &skills_map,
                temp_dir.path(),
            )
            .await
            .unwrap();

        // Both skills should be present
        assert!(result.contains("# Skill: seo"));
        assert!(result.contains("# Skill: brand"));
        assert!(result.contains("SEO Writer"));
        assert!(result.contains("Brand Voice"));
        // Base prompt at the end
        assert!(result.contains("Base prompt"));
    }

    #[tokio::test]
    async fn test_inject_no_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skills_map = HashMap::new();
        let injector = SkillInjector::new();

        let result = injector
            .inject(Some("Base prompt"), &[], &skills_map, temp_dir.path())
            .await
            .unwrap();

        assert_eq!(result, "Base prompt");
    }

    #[tokio::test]
    async fn test_inject_no_base_prompt() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        let result = injector
            .inject(None, &["seo"], &skills_map, temp_dir.path())
            .await
            .unwrap();

        assert!(result.contains("# Skill: seo"));
        assert!(result.contains("SEO Writer"));
    }

    #[tokio::test]
    async fn test_inject_skill_not_in_map() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        let result = injector
            .inject(Some("Base"), &["nonexistent"], &skills_map, temp_dir.path())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        if let NikaError::SkillLoadError { skill, reason } = err {
            assert_eq!(skill, "nonexistent");
            assert!(reason.contains("not found in workflow skills: block"));
        } else {
            panic!("Expected SkillLoadError");
        }
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        // Load a skill
        injector
            .load_skill(skills_map.get("seo").unwrap(), temp_dir.path())
            .await
            .unwrap();
        assert_eq!(injector.cache_size(), 1);

        // Clear cache
        injector.clear_cache();
        assert_eq!(injector.cache_size(), 0);
    }

    #[tokio::test]
    async fn test_is_cached() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        let skill_path = skills_map.get("seo").unwrap();

        // Not cached initially
        assert!(!injector.is_cached(skill_path, temp_dir.path()));

        // Load skill
        injector
            .load_skill(skill_path, temp_dir.path())
            .await
            .unwrap();

        // Now cached
        assert!(injector.is_cached(skill_path, temp_dir.path()));
    }

    #[tokio::test]
    async fn test_default_impl() {
        let injector = SkillInjector::default();
        assert_eq!(injector.cache_size(), 0);
    }

    #[tokio::test]
    async fn test_inject_empty_base_prompt() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = SkillInjector::new();

        let result = injector
            .inject(Some(""), &["seo"], &skills_map, temp_dir.path())
            .await
            .unwrap();

        // Should have skill but no empty string at end
        assert!(result.contains("# Skill: seo"));
        assert!(!result.ends_with("\n\n")); // No double newline from empty base
    }

    #[tokio::test]
    async fn test_concurrent_loads() {
        let (temp_dir, skills_map) = setup_test_skills().await;
        let injector = Arc::new(SkillInjector::new());

        let skill_path = skills_map.get("seo").unwrap().clone();
        let base_dir = temp_dir.path().to_path_buf();

        // Spawn multiple concurrent loads
        let mut handles = vec![];
        for _ in 0..10 {
            let inj = Arc::clone(&injector);
            let path = skill_path.clone();
            let dir = base_dir.clone();
            handles.push(tokio::spawn(
                async move { inj.load_skill(&path, &dir).await },
            ));
        }

        // All should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        // Should only have one entry (deduplicated)
        assert_eq!(injector.cache_size(), 1);
    }

    #[tokio::test]
    async fn test_inject_fails_on_missing_skill_file() {
        let temp_dir = TempDir::new().unwrap();
        let mut skills_map = HashMap::new();
        // Path exists in map but file doesn't exist on disk
        skills_map.insert("ghost".to_string(), "./skills/ghost.md".to_string());

        let injector = SkillInjector::new();
        let result = injector
            .inject(Some("Base"), &["ghost"], &skills_map, temp_dir.path())
            .await;

        assert!(
            result.is_err(),
            "inject must fail when skill file is missing"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(err, NikaError::SkillLoadError { .. }),
            "Expected NIKA-270 SkillLoadError, got: {err}"
        );
    }
}
