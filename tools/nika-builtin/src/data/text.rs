// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Text tools: chunk, token_count

use crate::{BuiltinTool, BuiltinError, __sealed};
use serde::Deserialize;
use std::future::Future;
use std::pin::Pin;

pub struct ChunkTool;

impl __sealed::Sealed for ChunkTool {}

#[derive(Debug, Deserialize)]
struct ChunkParams {
    text: String,
    #[serde(default = "default_chunk_size")]
    chunk_size: usize,
    #[serde(default)]
    overlap: usize,
    #[serde(default = "default_chunk_mode")]
    mode: String,
}

fn default_chunk_size() -> usize {
    1000
}
fn default_chunk_mode() -> String {
    "text".into()
}

impl BuiltinTool for ChunkTool {
    fn name(&self) -> &'static str {
        "chunk"
    }

    fn description(&self) -> &'static str {
        "Split text into chunks for RAG pipelines (supports text and markdown modes)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to split into chunks"
                },
                "chunk_size": {
                    "type": "integer",
                    "description": "Maximum characters per chunk (default: 1000)",
                    "default": 1000,
                    "minimum": 1
                },
                "overlap": {
                    "type": "integer",
                    "description": "Character overlap between chunks (default: 0)",
                    "default": 0
                },
                "mode": {
                    "type": "string",
                    "enum": ["text", "markdown"],
                    "description": "Chunking mode: 'text' (character boundaries) or 'markdown' (respects headings)",
                    "default": "text"
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: ChunkParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::InvalidArgs {
                    tool: "nika:chunk".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            if params.chunk_size == 0 {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:chunk".into(),
                    reason: "chunk_size must be at least 1".into(),
                });
            }

            if params.text.is_empty() {
                return serde_json::to_string(&serde_json::json!({
                    "chunks": [],
                    "count": 0,
                    "mode": params.mode,
                    "chunk_size": params.chunk_size,
                    "overlap": params.overlap,
                }))
                .map_err(|e| BuiltinError::Other {
                    tool: "nika:chunk".into(),
                    reason: format!("Serialization failed: {e}"),
                });
            }

            use text_splitter::{ChunkConfig, MarkdownSplitter, TextSplitter};
            let config = ChunkConfig::new(params.chunk_size)
                .with_overlap(params.overlap)
                .map_err(|e| BuiltinError::Other {
                    tool: "nika:chunk".into(),
                    reason: format!("Invalid chunk config: {e}"),
                })?;

            let chunks: Vec<&str> = match params.mode.as_str() {
                "markdown" => MarkdownSplitter::new(config).chunks(&params.text).collect(),
                _ => TextSplitter::new(config).chunks(&params.text).collect(),
            };

            let count = chunks.len();
            let result = serde_json::json!({
                "chunks": chunks,
                "count": count,
                "mode": params.mode,
                "chunk_size": params.chunk_size,
                "overlap": params.overlap,
            });

            serde_json::to_string(&result).map_err(|e| BuiltinError::Other {
                tool: "nika:chunk".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:token_count — heuristic token counting
// ═══════════════════════════════════════════════════════════════════════════

pub struct TokenCountTool;

impl __sealed::Sealed for TokenCountTool {}

#[derive(Debug, Deserialize)]
struct TokenCountParams {
    text: String,
    #[serde(default = "default_tokenizer")]
    model: String,
}

fn default_tokenizer() -> String {
    "heuristic".into()
}

impl BuiltinTool for TokenCountTool {
    fn name(&self) -> &'static str {
        "token_count"
    }

    fn description(&self) -> &'static str {
        "Estimate token count for text (heuristic: chars/4)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to count tokens for"
                },
                "model": {
                    "type": "string",
                    "description": "Tokenizer model: 'heuristic' (default, chars/4)",
                    "default": "heuristic"
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: TokenCountParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::InvalidArgs {
                    tool: "nika:token_count".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            // Heuristic: ~4 characters per token (OpenAI's own approximation)
            // Use char count, not byte length, for correct non-ASCII estimation.
            let char_count = params.text.chars().count();
            let token_count = char_count / 4;

            let result = serde_json::json!({
                "tokens": token_count,
                "characters": char_count,
                "model": params.model,
            });

            serde_json::to_string(&result).map_err(|e| BuiltinError::Other {
                tool: "nika:token_count".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:jq — Full jq expression evaluation on JSON data
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    // ── chunk ───────────────────────────────────────────────────
    #[tokio::test]
    async fn chunk_splits_text() {
        let tool = ChunkTool;
        let long_text = "a".repeat(3000);
        let result = tool
            .call(serde_json::json!({"text": long_text, "chunk_size": 1000}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["count"].as_u64().unwrap() >= 3);
        assert_eq!(parsed["mode"], "text");
    }

    #[tokio::test]
    async fn chunk_small_text_single_chunk() {
        let tool = ChunkTool;
        let result = tool
            .call(r#"{"text": "Hello world", "chunk_size": 1000}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 1);
    }

    #[tokio::test]
    async fn chunk_empty_text() {
        let tool = ChunkTool;
        let result = tool
            .call(r#"{"text": "", "chunk_size": 1000}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 0);
        assert_eq!(parsed["chunks"], json!([]));
    }

    #[tokio::test]
    async fn chunk_markdown_mode() {
        let tool = ChunkTool;
        let md = "# Title\n\nParagraph one.\n\n# Another\n\nParagraph two.";
        let result = tool
            .call(serde_json::json!({"text": md, "mode": "markdown", "chunk_size": 30}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["count"].as_u64().unwrap() >= 2);
        assert_eq!(parsed["mode"], "markdown");
    }

    // ── token_count ─────────────────────────────────────────────
    #[tokio::test]
    async fn token_count_heuristic() {
        let tool = TokenCountTool;
        // 100 chars → ~25 tokens
        let text = "a".repeat(100);
        let result = tool
            .call(serde_json::json!({"text": text}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["tokens"], 25);
        assert_eq!(parsed["characters"], 100);
        assert_eq!(parsed["model"], "heuristic");
    }

    #[tokio::test]
    async fn token_count_empty() {
        let tool = TokenCountTool;
        let result = tool.call(r#"{"text": ""}"#.into()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["tokens"], 0);
        assert_eq!(parsed["characters"], 0);
    }

    #[tokio::test]
    async fn token_count_non_ascii() {
        let tool = TokenCountTool;
        // "héllo" = 5 chars, 6 bytes in UTF-8 — must use char count not byte count
        let result = tool
            .call(r#"{"text": "héllo café résumé"}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        // 17 chars (h-é-l-l-o- -c-a-f-é- -r-é-s-u-m-é), not 20 bytes
        assert_eq!(parsed["characters"], 17);
        assert_eq!(parsed["tokens"], 4); // 17/4 = 4
    }

    #[tokio::test]
    async fn chunk_size_zero_errors() {
        let tool = ChunkTool;
        let result = tool
            .call(r#"{"text": "hello", "chunk_size": 0}"#.into())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("chunk_size must be at least 1"));
    }

    #[tokio::test]
    async fn chunk_overlap_exceeds_size_errors() {
        let tool = ChunkTool;
        let result = tool
            .call(r#"{"text": "hello world", "chunk_size": 10, "overlap": 20}"#.into())
            .await;
        assert!(result.is_err());
    }
}
