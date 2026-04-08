// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! JSON Schemas for Structured Output
//!
//! These schemas are used with the `output:` block in workflows
//! to validate LLM responses and ensure structured data.
//!
//! Usage in workflows:
//! ```yaml
//! output:
//!   schema_file: "./schemas/article.schema.json"
//!   retry: 3
//! ```

use super::ContextFile;

/// Directory for schema files
pub const SCHEMAS_DIR: &str = "schemas";

/// Article schema for content generation
pub const SCHEMA_ARTICLE: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Article",
  "description": "Schema for generated articles and blog posts",
  "type": "object",
  "properties": {
    "title": {
      "type": "string",
      "description": "Article title",
      "minLength": 10,
      "maxLength": 100
    },
    "subtitle": {
      "type": "string",
      "description": "Optional subtitle or tagline",
      "maxLength": 150
    },
    "author": {
      "type": "string",
      "description": "Author name"
    },
    "date": {
      "type": "string",
      "format": "date",
      "description": "Publication date (YYYY-MM-DD)"
    },
    "category": {
      "type": "string",
      "enum": ["technology", "business", "science", "lifestyle", "opinion"],
      "description": "Article category"
    },
    "tags": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "minItems": 1,
      "maxItems": 10,
      "description": "Relevant tags for the article"
    },
    "excerpt": {
      "type": "string",
      "description": "Short summary for previews",
      "minLength": 50,
      "maxLength": 300
    },
    "content": {
      "type": "string",
      "description": "Full article content in markdown",
      "minLength": 500
    },
    "seo": {
      "type": "object",
      "properties": {
        "meta_title": {
          "type": "string",
          "maxLength": 60
        },
        "meta_description": {
          "type": "string",
          "maxLength": 160
        },
        "keywords": {
          "type": "array",
          "items": { "type": "string" }
        }
      },
      "required": ["meta_title", "meta_description"]
    }
  },
  "required": ["title", "content", "excerpt", "tags"]
}
"##;

/// Code review result schema
pub const SCHEMA_CODE_REVIEW: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "CodeReviewResult",
  "description": "Schema for code review analysis results",
  "type": "object",
  "properties": {
    "summary": {
      "type": "string",
      "description": "Brief summary of the review"
    },
    "overall_rating": {
      "type": "string",
      "enum": ["approve", "request_changes", "comment"],
      "description": "Overall review decision"
    },
    "severity_score": {
      "type": "integer",
      "minimum": 0,
      "maximum": 10,
      "description": "Overall severity (0=perfect, 10=critical issues)"
    },
    "issues": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "type": {
            "type": "string",
            "enum": ["security", "performance", "bug", "style", "documentation", "best_practice"]
          },
          "severity": {
            "type": "string",
            "enum": ["critical", "major", "minor", "suggestion"]
          },
          "file": {
            "type": "string"
          },
          "line": {
            "type": "integer"
          },
          "description": {
            "type": "string"
          },
          "suggestion": {
            "type": "string"
          }
        },
        "required": ["type", "severity", "description"]
      }
    },
    "highlights": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "description": "Positive aspects of the code"
    },
    "statistics": {
      "type": "object",
      "properties": {
        "files_reviewed": { "type": "integer" },
        "lines_reviewed": { "type": "integer" },
        "issues_found": { "type": "integer" },
        "security_issues": { "type": "integer" },
        "performance_issues": { "type": "integer" }
      }
    }
  },
  "required": ["summary", "overall_rating", "issues"]
}
"##;

/// Meeting notes schema
pub const SCHEMA_MEETING_NOTES: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "MeetingNotes",
  "description": "Schema for structured meeting notes",
  "type": "object",
  "properties": {
    "meeting_title": {
      "type": "string"
    },
    "date": {
      "type": "string",
      "format": "date"
    },
    "duration_minutes": {
      "type": "integer",
      "minimum": 1
    },
    "attendees": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "role": { "type": "string" },
          "present": { "type": "boolean" }
        },
        "required": ["name"]
      }
    },
    "agenda_items": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "topic": { "type": "string" },
          "discussion": { "type": "string" },
          "decisions": {
            "type": "array",
            "items": { "type": "string" }
          }
        },
        "required": ["topic", "discussion"]
      }
    },
    "action_items": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "task": { "type": "string" },
          "owner": { "type": "string" },
          "due_date": { "type": "string", "format": "date" },
          "priority": {
            "type": "string",
            "enum": ["high", "medium", "low"]
          },
          "status": {
            "type": "string",
            "enum": ["pending", "in_progress", "completed"]
          }
        },
        "required": ["task", "owner"]
      }
    },
    "key_decisions": {
      "type": "array",
      "items": { "type": "string" }
    },
    "follow_up_meeting": {
      "type": "object",
      "properties": {
        "date": { "type": "string", "format": "date" },
        "topics": {
          "type": "array",
          "items": { "type": "string" }
        }
      }
    }
  },
  "required": ["meeting_title", "date", "attendees", "action_items"]
}
"##;

/// API endpoint health check result
pub const SCHEMA_HEALTH_CHECK: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "HealthCheckResult",
  "description": "Schema for API health check results",
  "type": "object",
  "properties": {
    "endpoint": {
      "type": "string",
      "format": "uri"
    },
    "timestamp": {
      "type": "string",
      "format": "date-time"
    },
    "status": {
      "type": "string",
      "enum": ["healthy", "degraded", "unhealthy"]
    },
    "response_time_ms": {
      "type": "integer",
      "minimum": 0
    },
    "http_status": {
      "type": "integer"
    },
    "checks": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "passed": { "type": "boolean" },
          "message": { "type": "string" },
          "duration_ms": { "type": "integer" }
        },
        "required": ["name", "passed"]
      }
    },
    "metadata": {
      "type": "object",
      "properties": {
        "version": { "type": "string" },
        "region": { "type": "string" },
        "instance_id": { "type": "string" }
      }
    }
  },
  "required": ["endpoint", "status", "response_time_ms"]
}
"##;

/// Entity extraction schema for knowledge graphs
pub const SCHEMA_ENTITY_EXTRACTION: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "EntityExtractionResult",
  "description": "Schema for extracted entities and relationships",
  "type": "object",
  "properties": {
    "source_text": {
      "type": "string",
      "description": "Original text that was analyzed"
    },
    "entities": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "text": {
            "type": "string",
            "description": "The entity as it appears in text"
          },
          "type": {
            "type": "string",
            "enum": ["person", "organization", "location", "product", "concept", "event", "date", "other"]
          },
          "normalized": {
            "type": "string",
            "description": "Normalized/canonical form"
          },
          "confidence": {
            "type": "number",
            "minimum": 0,
            "maximum": 1
          },
          "attributes": {
            "type": "object",
            "additionalProperties": true
          }
        },
        "required": ["text", "type"]
      }
    },
    "relationships": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "source": {
            "type": "string",
            "description": "Source entity text"
          },
          "relation": {
            "type": "string",
            "description": "Type of relationship"
          },
          "target": {
            "type": "string",
            "description": "Target entity text"
          },
          "confidence": {
            "type": "number",
            "minimum": 0,
            "maximum": 1
          }
        },
        "required": ["source", "relation", "target"]
      }
    },
    "summary": {
      "type": "string",
      "description": "Brief summary of extracted knowledge"
    }
  },
  "required": ["entities", "relationships"]
}
"##;

/// Social media post schema
pub const SCHEMA_SOCIAL_POST: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "SocialMediaPost",
  "description": "Schema for social media content",
  "type": "object",
  "properties": {
    "platform": {
      "type": "string",
      "enum": ["twitter", "linkedin", "instagram", "facebook", "tiktok"]
    },
    "content": {
      "type": "string",
      "description": "Post text content"
    },
    "hashtags": {
      "type": "array",
      "items": { "type": "string" },
      "maxItems": 30
    },
    "mentions": {
      "type": "array",
      "items": { "type": "string" }
    },
    "media": {
      "type": "object",
      "properties": {
        "type": {
          "type": "string",
          "enum": ["image", "video", "carousel", "none"]
        },
        "alt_text": { "type": "string" },
        "aspect_ratio": { "type": "string" }
      }
    },
    "schedule": {
      "type": "object",
      "properties": {
        "optimal_time": { "type": "string" },
        "timezone": { "type": "string" }
      }
    },
    "call_to_action": {
      "type": "string"
    },
    "character_count": {
      "type": "integer"
    }
  },
  "required": ["platform", "content"]
}
"##;

/// Returns all schema files
pub fn get_schema_files() -> Vec<ContextFile> {
    vec![
        ContextFile {
            filename: "article.schema.json",
            dir: SCHEMAS_DIR,
            content: SCHEMA_ARTICLE,
        },
        ContextFile {
            filename: "code-review.schema.json",
            dir: SCHEMAS_DIR,
            content: SCHEMA_CODE_REVIEW,
        },
        ContextFile {
            filename: "meeting-notes.schema.json",
            dir: SCHEMAS_DIR,
            content: SCHEMA_MEETING_NOTES,
        },
        ContextFile {
            filename: "health-check.schema.json",
            dir: SCHEMAS_DIR,
            content: SCHEMA_HEALTH_CHECK,
        },
        ContextFile {
            filename: "entity-extraction.schema.json",
            dir: SCHEMAS_DIR,
            content: SCHEMA_ENTITY_EXTRACTION,
        },
        ContextFile {
            filename: "social-post.schema.json",
            dir: SCHEMAS_DIR,
            content: SCHEMA_SOCIAL_POST,
        },
    ]
}
