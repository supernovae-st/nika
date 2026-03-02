//! Lockfile parsing for exact package versions (v0.17+)
//!
//! Reads `spn.lock` to resolve exact package versions instead of using "latest".
//! This ensures reproducible builds and avoids version drift.
//!
//! # Format
//!
//! ```yaml
//! packages:
//!   - name: "@workflows/seo-audit"
//!     version: "1.2.0"
//!     checksum: "sha256:abc123..."
//!
//!   - name: "@agents/researcher"
//!     version: "2.0.0"
//!     checksum: "sha256:def456..."
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during lockfile operations.
#[derive(Error, Debug)]
pub enum LockfileError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    YamlParseError(String),

    #[error("YAML serialize error: {0}")]
    YamlSerializeError(String),

    #[error("Lockfile not found at: {0}")]
    NotFound(String),
}

/// A single locked package entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    /// Full package name (e.g., "@workflows/seo-audit")
    pub name: String,

    /// Exact version (e.g., "1.2.0")
    pub version: String,

    /// Package checksum for integrity verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

/// The lockfile containing all locked package versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    /// List of locked packages
    pub packages: Vec<LockEntry>,
}

impl Lockfile {
    /// Create an empty lockfile.
    pub fn new() -> Self {
        Self {
            packages: Vec::new(),
        }
    }

    /// Load lockfile from the current directory or a specified path.
    ///
    /// Returns an empty lockfile if `spn.lock` doesn't exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nika::registry::lockfile::Lockfile;
    ///
    /// let lockfile = Lockfile::load(None).unwrap();
    /// if let Some(version) = lockfile.find_version("@workflows/seo-audit") {
    ///     println!("Locked version: {}", version);
    /// }
    /// ```
    pub fn load(path: Option<&Path>) -> Result<Self, LockfileError> {
        let lockfile_path = if let Some(p) = path {
            p.to_path_buf()
        } else {
            PathBuf::from("spn.lock")
        };

        if !lockfile_path.exists() {
            // Return empty lockfile if file doesn't exist
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(&lockfile_path)?;
        let lockfile: Lockfile = crate::serde_yaml::from_str(&content)
            .map_err(|e| LockfileError::YamlParseError(e.to_string()))?;
        Ok(lockfile)
    }

    /// Find the locked version for a given package name.
    ///
    /// Returns `None` if the package is not in the lockfile.
    ///
    /// # Examples
    ///
    /// ```
    /// use nika::registry::lockfile::Lockfile;
    ///
    /// let mut lockfile = Lockfile::new();
    /// // Assuming lockfile is populated...
    /// if let Some(version) = lockfile.find_version("@workflows/seo-audit") {
    ///     println!("Version: {}", version);
    /// }
    /// ```
    pub fn find_version(&self, name: &str) -> Option<&str> {
        self.packages
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.version.as_str())
    }

    /// Add or update a package entry in the lockfile.
    pub fn upsert(&mut self, name: String, version: String, checksum: Option<String>) {
        if let Some(entry) = self.packages.iter_mut().find(|p| p.name == name) {
            entry.version = version;
            entry.checksum = checksum;
        } else {
            self.packages.push(LockEntry {
                name,
                version,
                checksum,
            });
        }
    }

    /// Remove a package from the lockfile.
    pub fn remove(&mut self, name: &str) -> bool {
        if let Some(pos) = self.packages.iter().position(|p| p.name == name) {
            self.packages.remove(pos);
            true
        } else {
            false
        }
    }

    /// Save the lockfile to disk atomically.
    ///
    /// Uses temp+rename pattern from util::fs to ensure durability.
    /// This prevents corruption if the process crashes during write.
    pub fn save(&self, path: Option<&Path>) -> Result<(), LockfileError> {
        let lockfile_path = if let Some(p) = path {
            p.to_path_buf()
        } else {
            PathBuf::from("spn.lock")
        };

        let content = crate::serde_yaml::to_string(&self)
            .map_err(|e| LockfileError::YamlSerializeError(e.to_string()))?;

        // SECURITY: Atomic write prevents corruption on crash
        crate::util::fs::atomic_write(&lockfile_path, content.as_bytes())?;
        Ok(())
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_new() {
        let lockfile = Lockfile::new();
        assert!(lockfile.packages.is_empty());
    }

    #[test]
    fn test_find_version() {
        let mut lockfile = Lockfile::new();
        lockfile.packages.push(LockEntry {
            name: "@workflows/seo-audit".to_string(),
            version: "1.2.0".to_string(),
            checksum: Some("sha256:abc123".to_string()),
        });
        lockfile.packages.push(LockEntry {
            name: "@agents/researcher".to_string(),
            version: "2.0.0".to_string(),
            checksum: None,
        });

        assert_eq!(
            lockfile.find_version("@workflows/seo-audit"),
            Some("1.2.0")
        );
        assert_eq!(lockfile.find_version("@agents/researcher"), Some("2.0.0"));
        assert_eq!(lockfile.find_version("@workflows/missing"), None);
    }

    #[test]
    fn test_upsert_new() {
        let mut lockfile = Lockfile::new();
        lockfile.upsert(
            "@workflows/test".to_string(),
            "1.0.0".to_string(),
            Some("sha256:test".to_string()),
        );

        assert_eq!(lockfile.packages.len(), 1);
        assert_eq!(lockfile.packages[0].name, "@workflows/test");
        assert_eq!(lockfile.packages[0].version, "1.0.0");
        assert_eq!(
            lockfile.packages[0].checksum,
            Some("sha256:test".to_string())
        );
    }

    #[test]
    fn test_upsert_existing() {
        let mut lockfile = Lockfile::new();
        lockfile.packages.push(LockEntry {
            name: "@workflows/test".to_string(),
            version: "1.0.0".to_string(),
            checksum: None,
        });

        lockfile.upsert(
            "@workflows/test".to_string(),
            "2.0.0".to_string(),
            Some("sha256:new".to_string()),
        );

        assert_eq!(lockfile.packages.len(), 1);
        assert_eq!(lockfile.packages[0].version, "2.0.0");
        assert_eq!(
            lockfile.packages[0].checksum,
            Some("sha256:new".to_string())
        );
    }

    #[test]
    fn test_remove() {
        let mut lockfile = Lockfile::new();
        lockfile.packages.push(LockEntry {
            name: "@workflows/test".to_string(),
            version: "1.0.0".to_string(),
            checksum: None,
        });

        assert!(lockfile.remove("@workflows/test"));
        assert_eq!(lockfile.packages.len(), 0);
        assert!(!lockfile.remove("@workflows/missing"));
    }

    #[test]
    fn test_load_missing_file() {
        // Loading a non-existent file should return an empty lockfile
        let result = Lockfile::load(Some(Path::new("/tmp/nonexistent-spn.lock")));
        assert!(result.is_ok());
        assert!(result.unwrap().packages.is_empty());
    }
}
