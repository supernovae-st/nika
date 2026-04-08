// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! SuperNovae Registry API Client
//!
//! HTTP client for the SuperNovae package registry.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  REGISTRY API CLIENT                                                        │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  RegistryClient                                                             │
//! │  ├── base_url: https://registry.supernovae.studio/api/v1                    │
//! │  ├── client: reqwest::Client (connection pooling, rustls)                   │
//! │  └── timeout: 30s default                                                   │
//! │                                                                             │
//! │  API Endpoints:                                                             │
//! │  GET  /packages/:name              → Package metadata                       │
//! │  GET  /packages/:name/versions     → All versions                           │
//! │  GET  /packages/:name/:version     → Specific version                       │
//! │  GET  /search?q=:query             → Search packages                        │
//! │  GET  /packages/:name/:version/download → Download tarball                  │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika::registry::api::RegistryClient;
//!
//! let client = RegistryClient::new();
//!
//! // Search for packages
//! let results = client.search("workflow").await?;
//!
//! // Get package info
//! let info = client.get_package("@nika/core").await?;
//!
//! // Download package
//! let bytes = client.download("@nika/core", "1.0.0").await?;
//! ```

use std::path::PathBuf;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default registry URL
pub const DEFAULT_REGISTRY_URL: &str = "https://registry.supernovae.studio/api/v1";

/// Environment variable to override registry URL
pub const REGISTRY_URL_ENV: &str = "NIKA_REGISTRY_URL";

/// Default request timeout in seconds
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Errors that can occur during registry API operations.
#[derive(Error, Debug)]
pub enum RegistryApiError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Version not found: {0}@{1}")]
    VersionNotFound(String, String),

    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Rate limited: retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },
}

/// Package metadata from the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    /// Package name (e.g., "@nika/core")
    pub name: String,

    /// Latest version
    pub latest_version: String,

    /// Short description
    #[serde(default)]
    pub description: Option<String>,

    /// Package authors
    #[serde(default)]
    pub authors: Option<Vec<String>>,

    /// SPDX license
    #[serde(default)]
    pub license: Option<String>,

    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,

    /// Keywords for search
    #[serde(default)]
    pub keywords: Option<Vec<String>>,

    /// Download count (all versions)
    #[serde(default)]
    pub downloads: Option<u64>,

    /// Available versions (newest first)
    #[serde(default)]
    pub versions: Vec<String>,

    /// Created timestamp (ISO 8601)
    #[serde(default)]
    pub created_at: Option<String>,

    /// Last updated timestamp (ISO 8601)
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Version-specific metadata from the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Package name
    pub name: String,

    /// Version string
    pub version: String,

    /// Description
    #[serde(default)]
    pub description: Option<String>,

    /// Dependencies (name -> version constraint)
    #[serde(default)]
    pub dependencies: Option<std::collections::HashMap<String, String>>,

    /// Skills provided by this version
    #[serde(default)]
    pub skills: Option<Vec<SkillInfo>>,

    /// Tarball size in bytes
    #[serde(default)]
    pub size: Option<u64>,

    /// SHA256 checksum of tarball
    #[serde(default)]
    pub checksum: Option<String>,

    /// Published timestamp (ISO 8601)
    #[serde(default)]
    pub published_at: Option<String>,
}

/// Skill information within a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    /// Skill name/alias
    pub name: String,

    /// Relative path within package
    pub path: String,

    /// Description
    #[serde(default)]
    pub description: Option<String>,
}

/// Search result item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Package name
    pub name: String,

    /// Latest version
    pub version: String,

    /// Description
    #[serde(default)]
    pub description: Option<String>,

    /// Keywords
    #[serde(default)]
    pub keywords: Option<Vec<String>>,

    /// Download count
    #[serde(default)]
    pub downloads: Option<u64>,

    /// Search relevance score (0.0 - 1.0)
    #[serde(default)]
    pub score: Option<f64>,
}

/// Search response from the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    /// Total matching packages
    pub total: usize,

    /// Current page
    pub page: usize,

    /// Items per page
    pub per_page: usize,

    /// Search results
    pub results: Vec<SearchResult>,
}

/// SuperNovae Registry API Client.
///
/// Thread-safe, connection-pooled HTTP client for the package registry.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    client: Client,
    base_url: String,
}

impl RegistryClient {
    /// Create a new registry client with default settings.
    ///
    /// Uses `NIKA_REGISTRY_URL` env var or falls back to the default registry.
    ///
    /// Returns an error if the HTTP client cannot be built (e.g., TLS init failure).
    pub fn new() -> Result<Self, RegistryApiError> {
        let base_url =
            std::env::var(REGISTRY_URL_ENV).unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self { client, base_url })
    }

    /// Create a client with a custom base URL.
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn with_url(base_url: impl Into<String>) -> Result<Self, RegistryApiError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.into(),
        })
    }

    /// Create a client with custom timeout.
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn with_timeout(timeout_secs: u64) -> Result<Self, RegistryApiError> {
        let base_url =
            std::env::var(REGISTRY_URL_ENV).unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self { client, base_url })
    }

    /// Get package metadata.
    ///
    /// # Arguments
    ///
    /// * `name` - Package name (e.g., "@nika/core")
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let client = RegistryClient::new();
    /// let info = client.get_package("@nika/core").await?;
    /// println!("Latest version: {}", info.latest_version);
    /// ```
    pub async fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryApiError> {
        let url = format!("{}/packages/{}", self.base_url, encode_package_name(name));

        let response = self.client.get(&url).send().await?;

        match response.status().as_u16() {
            200 => response
                .json::<PackageInfo>()
                .await
                .map_err(|e| RegistryApiError::InvalidResponse(e.to_string())),
            404 => Err(RegistryApiError::PackageNotFound(name.to_string())),
            429 => {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60);
                Err(RegistryApiError::RateLimited { retry_after })
            }
            status => {
                let message = response.text().await.unwrap_or_default();
                Err(RegistryApiError::ApiError { status, message })
            }
        }
    }

    /// Get specific version metadata.
    ///
    /// # Arguments
    ///
    /// * `name` - Package name
    /// * `version` - Version string (e.g., "1.0.0")
    pub async fn get_version(
        &self,
        name: &str,
        version: &str,
    ) -> Result<VersionInfo, RegistryApiError> {
        let url = format!(
            "{}/packages/{}/{}",
            self.base_url,
            encode_package_name(name),
            version
        );

        let response = self.client.get(&url).send().await?;

        match response.status().as_u16() {
            200 => response
                .json::<VersionInfo>()
                .await
                .map_err(|e| RegistryApiError::InvalidResponse(e.to_string())),
            404 => Err(RegistryApiError::VersionNotFound(
                name.to_string(),
                version.to_string(),
            )),
            429 => {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60);
                Err(RegistryApiError::RateLimited { retry_after })
            }
            status => {
                let message = response.text().await.unwrap_or_default();
                Err(RegistryApiError::ApiError { status, message })
            }
        }
    }

    /// Get all available versions for a package.
    ///
    /// # Arguments
    ///
    /// * `name` - Package name
    ///
    /// # Returns
    ///
    /// Vector of version strings, newest first.
    pub async fn get_versions(&self, name: &str) -> Result<Vec<String>, RegistryApiError> {
        let url = format!(
            "{}/packages/{}/versions",
            self.base_url,
            encode_package_name(name)
        );

        let response = self.client.get(&url).send().await?;

        match response.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct VersionsResponse {
                    versions: Vec<String>,
                }
                let resp: VersionsResponse = response
                    .json()
                    .await
                    .map_err(|e| RegistryApiError::InvalidResponse(e.to_string()))?;
                Ok(resp.versions)
            }
            404 => Err(RegistryApiError::PackageNotFound(name.to_string())),
            429 => {
                let retry_after = 60;
                Err(RegistryApiError::RateLimited { retry_after })
            }
            status => {
                let message = response.text().await.unwrap_or_default();
                Err(RegistryApiError::ApiError { status, message })
            }
        }
    }

    /// Search for packages.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query
    /// * `page` - Page number (1-indexed)
    /// * `per_page` - Results per page (default 20, max 100)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let client = RegistryClient::new();
    /// let results = client.search("workflow", 1, 20).await?;
    /// for pkg in results.results {
    ///     println!("{}: {}", pkg.name, pkg.description.unwrap_or_default());
    /// }
    /// ```
    pub async fn search(
        &self,
        query: &str,
        page: usize,
        per_page: usize,
    ) -> Result<SearchResponse, RegistryApiError> {
        let url = format!(
            "{}/search?q={}&page={}&per_page={}",
            self.base_url,
            urlencoding::encode(query),
            page,
            per_page.min(100)
        );

        let response = self.client.get(&url).send().await?;

        match response.status().as_u16() {
            200 => response
                .json::<SearchResponse>()
                .await
                .map_err(|e| RegistryApiError::InvalidResponse(e.to_string())),
            429 => {
                let retry_after = 60;
                Err(RegistryApiError::RateLimited { retry_after })
            }
            status => {
                let message = response.text().await.unwrap_or_default();
                Err(RegistryApiError::ApiError { status, message })
            }
        }
    }

    /// Download package tarball.
    ///
    /// # Arguments
    ///
    /// * `name` - Package name
    /// * `version` - Version to download
    ///
    /// # Returns
    ///
    /// Raw bytes of the tarball (gzipped tar archive).
    pub async fn download(&self, name: &str, version: &str) -> Result<Vec<u8>, RegistryApiError> {
        let url = format!(
            "{}/packages/{}/{}/download",
            self.base_url,
            encode_package_name(name),
            version
        );

        let response = self.client.get(&url).send().await?;

        match response.status().as_u16() {
            200 => response
                .bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(RegistryApiError::from),
            404 => Err(RegistryApiError::VersionNotFound(
                name.to_string(),
                version.to_string(),
            )),
            429 => {
                let retry_after = 60;
                Err(RegistryApiError::RateLimited { retry_after })
            }
            status => {
                let message = response.text().await.unwrap_or_default();
                Err(RegistryApiError::ApiError { status, message })
            }
        }
    }

    /// Download and extract package to a directory.
    ///
    /// # Arguments
    ///
    /// * `name` - Package name
    /// * `version` - Version to download
    /// * `target_dir` - Directory to extract to
    ///
    /// # Returns
    ///
    /// Path to the extracted package directory.
    pub async fn download_and_extract(
        &self,
        name: &str,
        version: &str,
        target_dir: &PathBuf,
    ) -> Result<PathBuf, RegistryApiError> {
        use flate2::read::GzDecoder;
        use tar::Archive;

        let bytes = self.download(name, version).await?;

        // Create target directory
        std::fs::create_dir_all(target_dir)?;

        // Extract tarball
        let gz = GzDecoder::new(bytes.as_slice());
        let mut archive = Archive::new(gz);
        archive.unpack(target_dir)?;

        Ok(target_dir.clone())
    }

    /// Check if a package exists.
    pub async fn package_exists(&self, name: &str) -> Result<bool, RegistryApiError> {
        match self.get_package(name).await {
            Ok(_) => Ok(true),
            Err(RegistryApiError::PackageNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Check if a specific version exists.
    pub async fn version_exists(
        &self,
        name: &str,
        version: &str,
    ) -> Result<bool, RegistryApiError> {
        match self.get_version(name, version).await {
            Ok(_) => Ok(true),
            Err(RegistryApiError::VersionNotFound(_, _)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Get the base URL being used.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Encode package name for URL path.
///
/// Handles scoped packages: `@scope/name` -> `@scope%2Fname`
fn encode_package_name(name: &str) -> String {
    // URL encode the slash in scoped packages
    name.replace('/', "%2F")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_package_name() {
        assert_eq!(encode_package_name("@nika/core"), "@nika%2Fcore");
        assert_eq!(encode_package_name("simple-pkg"), "simple-pkg");
        assert_eq!(
            encode_package_name("@workflows/seo-audit"),
            "@workflows%2Fseo-audit"
        );
    }

    #[test]
    fn test_registry_client_default() {
        let client = RegistryClient::new().unwrap();
        // Should use default URL if env var not set
        assert!(client.base_url.contains("registry") || client.base_url.contains("supernovae"));
    }

    #[test]
    fn test_registry_client_with_url() {
        let client = RegistryClient::with_url("https://custom.registry.local/api").unwrap();
        assert_eq!(client.base_url, "https://custom.registry.local/api");
    }

    #[test]
    fn test_package_info_deserialize() {
        let json = r#"{
            "name": "@nika/core",
            "latest_version": "1.0.0",
            "description": "Core skills",
            "versions": ["1.0.0", "0.9.0"]
        }"#;

        let info: PackageInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, "@nika/core");
        assert_eq!(info.latest_version, "1.0.0");
        assert_eq!(info.versions.len(), 2);
    }

    #[test]
    fn test_version_info_deserialize() {
        let json = r#"{
            "name": "@nika/core",
            "version": "1.0.0",
            "description": "Core skills package",
            "skills": [
                {"name": "brainstorm", "path": "skills/brainstorm.md"}
            ]
        }"#;

        let info: VersionInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, "@nika/core");
        assert_eq!(info.version, "1.0.0");
        assert!(info.skills.is_some());
        assert_eq!(info.skills.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_search_response_deserialize() {
        let json = r#"{
            "total": 42,
            "page": 1,
            "per_page": 20,
            "results": [
                {
                    "name": "@nika/core",
                    "version": "1.0.0",
                    "description": "Core package",
                    "score": 0.95
                }
            ]
        }"#;

        let response: SearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.total, 42);
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].name, "@nika/core");
    }

    #[test]
    fn test_skill_info_deserialize() {
        let json = r#"{
            "name": "brainstorm",
            "path": "skills/brainstorm.skill.md",
            "description": "Collaborative ideation"
        }"#;

        let skill: SkillInfo = serde_json::from_str(json).unwrap();
        assert_eq!(skill.name, "brainstorm");
        assert_eq!(skill.path, "skills/brainstorm.skill.md");
    }

    #[test]
    fn test_registry_api_error_display() {
        let err = RegistryApiError::PackageNotFound("@test/pkg".to_string());
        assert_eq!(err.to_string(), "Package not found: @test/pkg");

        let err = RegistryApiError::VersionNotFound("@test/pkg".to_string(), "1.0.0".to_string());
        assert_eq!(err.to_string(), "Version not found: @test/pkg@1.0.0");

        let err = RegistryApiError::RateLimited { retry_after: 60 };
        assert_eq!(err.to_string(), "Rate limited: retry after 60 seconds");
    }

    #[test]
    fn test_package_info_optional_fields() {
        let json = r#"{
            "name": "@minimal/pkg",
            "latest_version": "0.1.0",
            "versions": []
        }"#;

        let info: PackageInfo = serde_json::from_str(json).unwrap();
        assert!(info.description.is_none());
        assert!(info.authors.is_none());
        assert!(info.license.is_none());
        assert!(info.downloads.is_none());
    }
}
