//! File mention resolver for @file syntax
//!
//! Extracts and resolves @file mentions in chat messages.
//! Handles @path/to/file.ext patterns while excluding emails like user@example.com.

use regex::Regex;
use std::path::Path;

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
        let re = Regex::new(r"(?:^|[^a-zA-Z0-9])@([\w./\-]+\.\w+)").unwrap();

        re.captures_iter(input)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Resolve file mentions and return expanded prompt
    ///
    /// Replaces @file mentions with XML-wrapped file contents:
    /// `<file path="...">content</file>`
    ///
    /// Missing files are left as-is (not replaced).
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
            let file_path = base_dir.join(&mention);
            if file_path.exists() {
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => {
                        let replacement = format!(
                            "<file path=\"{}\">\n{}\n</file>",
                            mention,
                            content.trim_end()
                        );
                        result = result.replace(&format!("@{}", mention), &replacement);
                    }
                    Err(_) => {
                        // File exists but couldn't be read - leave mention as-is
                        continue;
                    }
                }
            }
            // Missing files are left as-is
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
}
