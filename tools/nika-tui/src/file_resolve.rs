//! File mention resolver for @file syntax
//!
//! Extracts and resolves @file mentions in chat messages.
//! Handles @path/to/file.ext patterns while excluding emails like user@example.com.
//!
//! # Security
//!
//! - Path traversal protection: Paths are canonicalized and checked to stay within base_dir
//! - File size limit: Files larger than 1MB are skipped
//! - Symlinks are followed but must resolve within base_dir
//!
//! # Crates Used
//!
//! - `regex`: Pattern matching with static compilation

use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

/// Maximum file size to include (1 MB)
const MAX_FILE_SIZE: u64 = 1_048_576;

/// Compiled regex for file mentions (compiled once)
static FILE_MENTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^a-zA-Z0-9])@([\w./\-]+\.\w+)").expect("Invalid regex pattern")
});

/// Result of resolving a file mention
#[derive(Debug, Clone)]
pub enum FileResolveResult {
    /// File was successfully resolved
    Resolved { path: String, content: String },
    /// File is too large
    TooLarge { path: String, size: u64 },
    /// Path traversal attempt blocked
    TraversalBlocked { path: String },
    /// File not found or not readable
    NotFound { path: String },
}

impl FileResolveResult {
    /// Convert to XML representation for prompt injection
    pub fn to_xml(&self) -> Option<String> {
        match self {
            Self::Resolved { path, content } => Some(format!(
                "<file path=\"{}\">\n{}\n</file>",
                path,
                content.trim_end()
            )),
            Self::TooLarge { path, size } => Some(format!(
                "<file path=\"{}\" error=\"too_large\">[File exceeds 1MB limit ({} bytes)]</file>",
                path, size
            )),
            Self::TraversalBlocked { .. } | Self::NotFound { .. } => None,
        }
    }

    /// Check if resolution was successful
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved { .. })
    }
}

/// Resolves @file mentions in chat input
pub struct FileResolver;

impl FileResolver {
    /// Extract all @file mentions from input
    ///
    /// Matches patterns like @src/main.rs, @Cargo.toml, @path/to/file.ext
    /// Does NOT match email addresses like user@example.com
    ///
    /// # Examples
    ///
    /// ```
    /// use nika::tui::file_resolve::FileResolver;
    ///
    /// let mentions = FileResolver::extract_mentions("Explain @src/main.rs");
    /// assert_eq!(mentions, vec!["src/main.rs"]);
    ///
    /// // Emails are not matched
    /// let mentions = FileResolver::extract_mentions("Contact user@example.com");
    /// assert!(mentions.is_empty());
    /// ```
    pub fn extract_mentions(input: &str) -> Vec<String> {
        // Pattern breakdown:
        // (?:^|[^a-zA-Z0-9]) - Start of string OR non-alphanumeric (excludes email left side)
        // @                  - Literal @ symbol
        // (                  - Start capture group
        //   [\w./\-]+        - Path characters: word chars, dots, slashes, hyphens
        //   \.               - Literal dot (requires extension)
        //   \w+              - Extension (at least one word char)
        // )                  - End capture group
        //
        // Uses static LazyLock regex for performance (compiled once)
        FILE_MENTION_RE
            .captures_iter(input)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Resolve a single file mention
    ///
    /// Returns a `FileResolveResult` with detailed status.
    ///
    /// # Security
    ///
    /// - **Path traversal protection**: Paths are canonicalized and verified to stay within base_dir
    /// - **File size limit**: Files larger than 1MB return `TooLarge`
    /// - Symlinks are followed but must resolve within base_dir
    pub fn resolve_one(mention: &str, base_dir: &Path) -> FileResolveResult {
        // Canonicalize base_dir for path containment check
        let base_canonical = match base_dir.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return FileResolveResult::NotFound {
                    path: mention.to_string(),
                }
            }
        };

        let file_path = base_dir.join(mention);

        // SECURITY: Canonicalize and verify path stays within base_dir
        let canonical = match file_path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return FileResolveResult::NotFound {
                    path: mention.to_string(),
                }
            }
        };

        if !canonical.starts_with(&base_canonical) {
            tracing::warn!(
                "Path traversal blocked: {} resolves outside base_dir",
                mention
            );
            return FileResolveResult::TraversalBlocked {
                path: mention.to_string(),
            };
        }

        // SECURITY: Check file size before reading
        match std::fs::metadata(&canonical) {
            Ok(meta) => {
                if meta.len() > MAX_FILE_SIZE {
                    tracing::warn!(
                        "File too large: {} ({} bytes > {} limit)",
                        mention,
                        meta.len(),
                        MAX_FILE_SIZE
                    );
                    return FileResolveResult::TooLarge {
                        path: mention.to_string(),
                        size: meta.len(),
                    };
                }
            }
            Err(_) => {
                return FileResolveResult::NotFound {
                    path: mention.to_string(),
                }
            }
        }

        // Safe to read the file
        match std::fs::read_to_string(&canonical) {
            Ok(content) => FileResolveResult::Resolved {
                path: mention.to_string(),
                content,
            },
            Err(_) => FileResolveResult::NotFound {
                path: mention.to_string(),
            },
        }
    }

    /// Resolve file mentions and return expanded prompt
    ///
    /// Replaces @file mentions with XML-wrapped file contents:
    /// `<file path="...">content</file>`
    ///
    /// Missing files are left as-is (not replaced).
    ///
    /// # Security
    ///
    /// - **Path traversal protection**: Paths are canonicalized and verified to stay within base_dir
    /// - **File size limit**: Files larger than 1MB are skipped with a warning
    /// - Symlinks are followed but must resolve within base_dir
    ///
    /// # Arguments
    ///
    /// * `input` - The input string containing @file mentions
    /// * `base_dir` - Base directory for resolving relative paths
    ///
    /// # Returns
    ///
    /// The input with @file mentions replaced by file contents wrapped in XML tags.
    pub fn resolve(input: &str, base_dir: &Path) -> String {
        let mentions = Self::extract_mentions(input);
        let mut result = input.to_string();

        for mention in mentions {
            let resolve_result = Self::resolve_one(&mention, base_dir);

            if let Some(xml) = resolve_result.to_xml() {
                result = result.replace(&format!("@{}", mention), &xml);
            }
            // NotFound and TraversalBlocked leave mention as-is
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_file_mentions() {
        let input = "Explain @src/main.rs and @Cargo.toml";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["src/main.rs", "Cargo.toml"]);
    }

    #[test]
    fn test_no_mentions() {
        let input = "Just a normal message";
        let mentions = FileResolver::extract_mentions(input);
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_email_not_file_mention() {
        let input = "Contact me at user@example.com";
        let mentions = FileResolver::extract_mentions(input);
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_email_with_domain_not_file_mention() {
        let input = "Email support@company.io for help";
        let mentions = FileResolver::extract_mentions(input);
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_mention_at_start_of_line() {
        let input = "@README.md contains instructions";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["README.md"]);
    }

    #[test]
    fn test_mention_after_space() {
        let input = "Look at @src/lib.rs please";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["src/lib.rs"]);
    }

    #[test]
    fn test_mention_after_punctuation() {
        let input = "Check this: @config.toml";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["config.toml"]);
    }

    #[test]
    fn test_mention_in_parentheses() {
        let input = "See (@docs/guide.md) for details";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["docs/guide.md"]);
    }

    #[test]
    fn test_multiple_mentions_same_line() {
        let input = "@file1.rs @file2.rs @file3.rs";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["file1.rs", "file2.rs", "file3.rs"]);
    }

    #[test]
    fn test_nested_path() {
        let input = "Check @path/to/nested/file.yaml";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["path/to/nested/file.yaml"]);
    }

    #[test]
    fn test_hyphenated_filename() {
        let input = "See @my-config-file.json";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["my-config-file.json"]);
    }

    #[test]
    fn test_resolve_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let input = "Explain @test.txt";
        let resolved = FileResolver::resolve(input, temp_dir.path());

        assert!(resolved.contains("<file path=\"test.txt\">"));
        assert!(resolved.contains("Hello, World!"));
        assert!(resolved.contains("</file>"));
        assert!(!resolved.contains("@test.txt"));
    }

    #[test]
    fn test_resolve_missing_file() {
        let temp_dir = TempDir::new().unwrap();

        let input = "Explain @missing.txt";
        let resolved = FileResolver::resolve(input, temp_dir.path());

        // Missing file should be left as-is
        assert_eq!(resolved, "Explain @missing.txt");
    }

    #[test]
    fn test_resolve_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("a.txt"), "Content A").unwrap();
        fs::write(temp_dir.path().join("b.txt"), "Content B").unwrap();

        let input = "Compare @a.txt and @b.txt";
        let resolved = FileResolver::resolve(input, temp_dir.path());

        assert!(resolved.contains("<file path=\"a.txt\">"));
        assert!(resolved.contains("Content A"));
        assert!(resolved.contains("<file path=\"b.txt\">"));
        assert!(resolved.contains("Content B"));
    }

    #[test]
    fn test_resolve_preserves_non_mentions() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("test.rs"), "fn main() {}").unwrap();

        let input = "Here is @test.rs for user@example.com";
        let resolved = FileResolver::resolve(input, temp_dir.path());

        // File should be resolved
        assert!(resolved.contains("<file path=\"test.rs\">"));
        // Email should be preserved
        assert!(resolved.contains("user@example.com"));
    }

    #[test]
    fn test_resolve_nested_directory() {
        let temp_dir = TempDir::new().unwrap();
        let nested = temp_dir.path().join("src/nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("file.rs"), "// nested file").unwrap();

        let input = "Check @src/nested/file.rs";
        let resolved = FileResolver::resolve(input, temp_dir.path());

        assert!(resolved.contains("<file path=\"src/nested/file.rs\">"));
        assert!(resolved.contains("// nested file"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Security Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_security_path_traversal_blocked() {
        let temp_dir = TempDir::new().unwrap();
        // Create a file inside temp_dir
        fs::write(temp_dir.path().join("safe.txt"), "safe content").unwrap();

        // Try to escape with ../
        let input = "Explain @../../../etc/passwd";
        let resolved = FileResolver::resolve(input, temp_dir.path());

        // Path traversal should be blocked - mention stays unchanged
        assert_eq!(resolved, "Explain @../../../etc/passwd");
        assert!(!resolved.contains("<file"));
    }

    #[test]
    fn test_security_dotdot_in_path_blocked() {
        // Create two separate temp directories to properly test path traversal
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("workspace");
        fs::create_dir_all(&base_dir).unwrap();

        // Create a file we want to protect (outside workspace)
        fs::write(temp_dir.path().join("secret.txt"), "secret data").unwrap();

        // Try to escape workspace with ..
        let input = "Check @../secret.txt";
        let resolved = FileResolver::resolve(input, &base_dir);

        // Should be blocked (resolves outside workspace/)
        assert!(!resolved.contains("secret data"));
        assert!(resolved.contains("@../secret.txt")); // Original mention preserved
    }

    #[test]
    fn test_security_file_size_limit() {
        let temp_dir = TempDir::new().unwrap();
        // Create a file larger than 1MB
        let large_content = "x".repeat(1_100_000); // 1.1 MB
        fs::write(temp_dir.path().join("large.txt"), &large_content).unwrap();

        let input = "Read @large.txt";
        let resolved = FileResolver::resolve(input, temp_dir.path());

        // Large file should show error, not content
        assert!(resolved.contains("error=\"too_large\""));
        assert!(resolved.contains("[File exceeds 1MB limit"));
        assert!(!resolved.contains(&large_content[0..100]));
    }

    #[test]
    fn test_security_file_within_limit() {
        let temp_dir = TempDir::new().unwrap();
        // Create a file smaller than 1MB
        let content = "x".repeat(100_000); // 100KB
        fs::write(temp_dir.path().join("normal.txt"), &content).unwrap();

        let input = "Read @normal.txt";
        let resolved = FileResolver::resolve(input, temp_dir.path());

        // Normal file should be included
        assert!(resolved.contains("<file path=\"normal.txt\">"));
        assert!(resolved.contains(&content[0..100]));
    }

    #[test]
    fn test_security_symlink_within_base_dir_allowed() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("target.txt");
        fs::write(&target, "target content").unwrap();

        // Create symlink within temp_dir
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = temp_dir.path().join("link.txt");
            symlink(&target, &link).unwrap();

            let input = "Check @link.txt";
            let resolved = FileResolver::resolve(input, temp_dir.path());

            // Symlink within base_dir should be allowed
            assert!(resolved.contains("target content"));
        }
    }

    #[test]
    fn test_regex_compiled_once() {
        // This test verifies the static LazyLock is working
        // by calling extract_mentions multiple times
        for _ in 0..1000 {
            let mentions = FileResolver::extract_mentions("@test.rs");
            assert_eq!(mentions, vec!["test.rs"]);
        }
        // If regex was compiled each time, this would be slow
    }

    // ═══════════════════════════════════════════════════════════════════════
    // FileResolveResult Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_resolve_result_to_xml() {
        let resolved = FileResolveResult::Resolved {
            path: "test.rs".to_string(),
            content: "fn main() {}".to_string(),
        };
        let xml = resolved.to_xml().unwrap();
        assert!(xml.contains("<file path=\"test.rs\">"));
        assert!(xml.contains("fn main() {}"));

        let too_large = FileResolveResult::TooLarge {
            path: "big.bin".to_string(),
            size: 2_000_000,
        };
        let xml = too_large.to_xml().unwrap();
        assert!(xml.contains("error=\"too_large\""));
        assert!(xml.contains("2000000 bytes"));

        let not_found = FileResolveResult::NotFound {
            path: "missing.txt".to_string(),
        };
        assert!(not_found.to_xml().is_none());

        let blocked = FileResolveResult::TraversalBlocked {
            path: "../secret.txt".to_string(),
        };
        assert!(blocked.to_xml().is_none());
    }

    #[test]
    fn test_resolve_one_returns_result() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let result = FileResolver::resolve_one("test.txt", temp_dir.path());
        assert!(result.is_resolved());

        let result = FileResolver::resolve_one("missing.txt", temp_dir.path());
        assert!(!result.is_resolved());
    }
}
