//! Document Links Handler
//!
//! Makes paths and URLs in workflows clickable:
//! - `include:` file paths → file:// links
//! - `fetch: url:` URLs → clickable http(s) links
//! - `context:` file paths → file:// links
//! - `skills:` file paths → file:// links

#[cfg(feature = "lsp")]
use std::str::FromStr;

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

/// Section context for tracking which YAML block we are inside.
#[cfg(feature = "lsp")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    /// Inside a `context:` / `context: files:` block
    Context,
    /// Inside a `skills:` block
    Skills,
    /// Not in a special section
    None,
}

/// Compute document links for the document.
///
/// Scans line-by-line for:
/// - `path:` / `include:` values → file:// links
/// - `url:` values starting with http(s) → clickable URL links
/// - File paths under `context:` or `skills:` sections → file:// links
#[cfg(feature = "lsp")]
pub fn compute_document_links(text: &str) -> Vec<DocumentLink> {
    let mut links = Vec::new();
    let mut current_section = Section::None;

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comment lines
        if trimmed.starts_with('#') {
            continue;
        }

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Track section context: detect top-level `context:` and `skills:` blocks.
        // A line with zero or minimal indent that starts a new key resets the section.
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            if trimmed.starts_with("context:") {
                current_section = Section::Context;
            } else if trimmed.starts_with("skills:") {
                current_section = Section::Skills;
            } else {
                // Any other top-level key exits the section
                current_section = Section::None;
            }
        }

        // Pattern 1: `url:` values → HTTP(S) links
        if let Some(link) = try_url_link(line, line_idx) {
            links.push(link);
            continue;
        }

        // Pattern 2: `path:` or `include:` values → file links
        if let Some(link) = try_path_link(line, line_idx) {
            links.push(link);
            continue;
        }

        // Pattern 3: Aliased file paths under context:/skills: sections
        // e.g. `brand: ./context/brand.md` under `context: files:`
        if current_section != Section::None && indent > 0 {
            if let Some(link) = try_section_file_link(line, line_idx, current_section) {
                links.push(link);
            }
        }
    }

    links
}

/// Try to extract an HTTP(S) URL from a `url:` line.
///
/// Matches: `url: "https://..."`, `url: 'https://...'`, `url: https://...`
#[cfg(feature = "lsp")]
fn try_url_link(line: &str, line_idx: usize) -> Option<DocumentLink> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("url:")?;
    let rest = rest.trim();

    let (value, quoted) = strip_quotes(rest);
    if !value.starts_with("http://") && !value.starts_with("https://") {
        return None;
    }

    // Validate that it parses as a URI
    Uri::from_str(value).ok()?;

    let (start_col, end_col) = find_value_range(line, rest, value, quoted);
    let target = Uri::from_str(value).ok();

    Some(DocumentLink {
        range: Range {
            start: Position {
                line: line_idx as u32,
                character: start_col,
            },
            end: Position {
                line: line_idx as u32,
                character: end_col,
            },
        },
        target,
        tooltip: Some("Open URL".to_string()),
        data: None,
    })
}

/// Try to extract a file path from `path:` or `include:` lines.
///
/// Matches: `path: ./partials/setup.nika.yaml`, `include: ./shared.nika.yaml`
/// Also handles quoted variants.
#[cfg(feature = "lsp")]
fn try_path_link(line: &str, line_idx: usize) -> Option<DocumentLink> {
    let trimmed = line.trim();

    // Strip optional YAML list prefix (e.g., `- path:` → `path:`)
    let key_part = trimmed.strip_prefix("- ").unwrap_or(trimmed);

    // Try both `path:` and `include:` prefixes
    let (rest, tooltip) = if let Some(r) = key_part.strip_prefix("path:") {
        (r, "Open file")
    } else if let Some(r) = key_part.strip_prefix("include:") {
        (r, "Open included workflow")
    } else {
        return None;
    };

    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    let (value, quoted) = strip_quotes(rest);

    // Reject URLs (these belong to try_url_link via `url:`)
    if value.starts_with("http://") || value.starts_with("https://") {
        return None;
    }

    // Reject empty or obviously non-path values
    if value.is_empty() {
        return None;
    }

    let file_uri = path_to_file_uri(value);
    let target = Uri::from_str(&file_uri).ok();

    let (start_col, end_col) = find_value_range(line, rest, value, quoted);

    Some(DocumentLink {
        range: Range {
            start: Position {
                line: line_idx as u32,
                character: start_col,
            },
            end: Position {
                line: line_idx as u32,
                character: end_col,
            },
        },
        target,
        tooltip: Some(tooltip.to_string()),
        data: None,
    })
}

/// Try to extract a file path from a section entry like `brand: ./context/brand.md`.
///
/// Only active when inside a `context:` or `skills:` block.
#[cfg(feature = "lsp")]
fn try_section_file_link(line: &str, line_idx: usize, section: Section) -> Option<DocumentLink> {
    let trimmed = line.trim();

    // Must be a key: value pair
    let colon_pos = trimmed.find(':')?;
    let value_part = trimmed[colon_pos + 1..].trim();

    if value_part.is_empty() {
        return None;
    }

    let (value, quoted) = strip_quotes(value_part);

    // Must look like a file path (starts with ./ or ../ or /)
    if !value.starts_with("./") && !value.starts_with("../") && !value.starts_with('/') {
        return None;
    }

    let file_uri = path_to_file_uri(value);
    let target = Uri::from_str(&file_uri).ok();

    let tooltip = match section {
        Section::Context => "Open context file",
        Section::Skills => "Open skill file",
        Section::None => return None,
    };

    // Find the value range in the original line
    let (start_col, end_col) = find_value_range(line, value_part, value, quoted);

    Some(DocumentLink {
        range: Range {
            start: Position {
                line: line_idx as u32,
                character: start_col,
            },
            end: Position {
                line: line_idx as u32,
                character: end_col,
            },
        },
        target,
        tooltip: Some(tooltip.to_string()),
        data: None,
    })
}

/// Strip surrounding quotes (single or double) from a value.
/// Returns the inner value and whether quotes were present.
#[cfg(feature = "lsp")]
fn strip_quotes(s: &str) -> (&str, bool) {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        return (&s[1..s.len() - 1], true);
    }
    (s, false)
}

/// Find the column range of a value within the full line.
///
/// If the value is quoted, the range covers just the inner value (excluding quotes).
/// If unquoted, the range covers the entire value text.
#[cfg(feature = "lsp")]
fn find_value_range(line: &str, rest_trimmed: &str, value: &str, quoted: bool) -> (u32, u32) {
    // Find where `rest_trimmed` starts in the line
    let rest_offset = line.find(rest_trimmed).unwrap_or_default();

    if quoted {
        // Skip the opening quote
        let start = rest_offset + 1;
        let end = start + value.len();
        (start as u32, end as u32)
    } else {
        let start = rest_offset;
        let end = start + value.len();
        (start as u32, end as u32)
    }
}

/// Convert a raw file path to a file:// URI string.
///
/// For relative paths (starting with `.`), we produce `file://` + the raw path.
/// The LSP client is responsible for resolving relative paths against the workspace root.
#[cfg(feature = "lsp")]
fn path_to_file_uri(path: &str) -> String {
    if path.starts_with('/') {
        format!("file://{}", path)
    } else {
        // Relative path -- use file:// with the raw relative path.
        // LSP clients resolve this against the workspace root.
        format!("file://{}", path)
    }
}

#[cfg(test)]
#[cfg(feature = "lsp")]
mod tests {
    use super::*;

    #[test]
    fn url_in_fetch_block() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"
      method: GET
"#;
        let links = compute_document_links(text);
        assert_eq!(links.len(), 1, "Expected 1 link, got: {:?}", links);

        let link = &links[0];
        assert_eq!(link.range.start.line, 5);
        assert_eq!(
            link.target.as_ref().map(|u| u.as_str()),
            Some("https://api.example.com/data")
        );
        assert_eq!(link.tooltip.as_deref(), Some("Open URL"));
    }

    #[test]
    fn file_path_in_include() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_
"#;
        let links = compute_document_links(text);
        assert!(
            links.iter().any(|l| {
                l.tooltip.as_deref() == Some("Open file")
                    && l.target
                        .as_ref()
                        .is_some_and(|u| u.as_str().contains("partials/setup.nika.yaml"))
            }),
            "Expected a file link for path:, got: {:?}",
            links
        );
    }

    #[test]
    fn context_file_path() {
        let text = r#"context:
  files:
    brand: ./context/brand.md
    data: ./context/data.json
tasks:
  - id: step1
    infer: "test"
"#;
        let links = compute_document_links(text);
        assert_eq!(links.len(), 2, "Expected 2 context links, got: {:?}", links);

        assert!(links
            .iter()
            .all(|l| l.tooltip.as_deref() == Some("Open context file")));
        assert!(links.iter().any(|l| l
            .target
            .as_ref()
            .is_some_and(|u| u.as_str().contains("brand.md"))));
        assert!(links.iter().any(|l| l
            .target
            .as_ref()
            .is_some_and(|u| u.as_str().contains("data.json"))));
    }

    #[test]
    fn skills_file_path() {
        let text = r#"skills:
  summarize: ./skills/summarize.nika.yaml
  translate: ./skills/translate.nika.yaml
tasks:
  - id: step1
    infer: "test"
"#;
        let links = compute_document_links(text);
        assert_eq!(links.len(), 2, "Expected 2 skill links, got: {:?}", links);

        assert!(links
            .iter()
            .all(|l| l.tooltip.as_deref() == Some("Open skill file")));
        assert!(links.iter().any(|l| l
            .target
            .as_ref()
            .is_some_and(|u| u.as_str().contains("summarize.nika.yaml"))));
    }

    #[test]
    fn quoted_vs_unquoted_paths() {
        let text_quoted = "    path: \"./partials/setup.nika.yaml\"\n";
        let text_unquoted = "    path: ./partials/setup.nika.yaml\n";
        let text_single_quoted = "    path: './partials/setup.nika.yaml'\n";

        let links_quoted = compute_document_links(text_quoted);
        let links_unquoted = compute_document_links(text_unquoted);
        let links_single = compute_document_links(text_single_quoted);

        assert_eq!(links_quoted.len(), 1, "Quoted path should produce 1 link");
        assert_eq!(
            links_unquoted.len(),
            1,
            "Unquoted path should produce 1 link"
        );
        assert_eq!(
            links_single.len(),
            1,
            "Single-quoted path should produce 1 link"
        );

        // All three should resolve to the same target
        let target_q = links_quoted[0].target.as_ref().unwrap().as_str();
        let target_u = links_unquoted[0].target.as_ref().unwrap().as_str();
        let target_s = links_single[0].target.as_ref().unwrap().as_str();
        assert_eq!(target_q, target_u);
        assert_eq!(target_q, target_s);

        // Quoted range should exclude the quote characters
        // "    path: \"./partials/setup.nika.yaml\""
        //            ^                            ^
        // col 10 is the opening quote, col 11 is the start of the value
        let range_q = &links_quoted[0].range;
        let range_u = &links_unquoted[0].range;
        assert_eq!(
            range_q.start.character,
            range_u.start.character + 1,
            "Quoted range start should be 1 past unquoted (skips opening quote)"
        );
    }

    #[test]
    fn comment_lines_skipped() {
        let text = r#"schema: nika/workflow@0.12
# url: "https://commented-out.com/api"
# path: ./commented/file.yaml
tasks:
  - id: step1
    fetch:
      url: https://real.example.com/data
"#;
        let links = compute_document_links(text);
        assert_eq!(
            links.len(),
            1,
            "Only real URL should produce a link, got: {:?}",
            links
        );
        assert!(links[0]
            .target
            .as_ref()
            .is_some_and(|u| u.as_str().contains("real.example.com")));
    }

    #[test]
    fn no_links_in_empty_document() {
        let links = compute_document_links("");
        assert!(links.is_empty());

        let links_whitespace = compute_document_links("   \n\n  \n");
        assert!(links_whitespace.is_empty());
    }
}
