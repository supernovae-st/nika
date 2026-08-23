// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::{Value, json};

/// [`OpenAPI`](https://spec.openapis.org/oas/v3.1.0) document of the live HTTP surface.
///
/// Cancel and artifact paths are omitted until those authorities exist.
pub(crate) fn document() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "nika serve",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Authenticated loopback remote execution. Cancel and artifacts are absent."
        },
        "servers": [{"url": "http://127.0.0.1"}],
        "security": [{"bearerAuth": []}],
        "components": components(),
        "paths": paths(),
    })
}

fn components() -> Value {
    json!({
        "securitySchemes": {
            "bearerAuth": {
                "type": "http",
                "scheme": "bearer",
                "description": "Exactly one Authorization: Bearer value from the token file"
            }
        },
        "parameters": {
            "IdempotencyKey": {
                "name": "Idempotency-Key",
                "in": "header",
                "required": true,
                "schema": {"type": "string", "minLength": 1, "maxLength": 255}
            },
            "LastEventId": {
                "name": "Last-Event-ID",
                "in": "header",
                "required": false,
                "schema": {"type": "string", "pattern": "^(0|[1-9][0-9]*)$"}
            }
        },
        "schemas": {
            "JobStatus": {
                "type": "string",
                "enum": ["queued", "running", "interrupted", "paused", "succeeded", "failed"]
            },
            "Job": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "status"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "status": {"$ref": "#/components/schemas/JobStatus"},
                    "execution_id": {"type": "string"},
                    "trace_id": {"type": "string"},
                    "error": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["code", "message"],
                        "properties": {
                            "code": {"type": "string"},
                            "message": {"type": "string"}
                        }
                    }
                }
            },
            "JobStatusOnly": {
                "type": "object",
                "additionalProperties": false,
                "required": ["status"],
                "properties": {
                    "status": {"$ref": "#/components/schemas/JobStatus"}
                }
            },
            "CreateJob": {
                "type": "object",
                "additionalProperties": false,
                "required": ["workflow"],
                "properties": {
                    "workflow": {"type": "string"}
                }
            },
            "Error": {
                "type": "object",
                "additionalProperties": false,
                "required": ["error"],
                "properties": {
                    "error": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["code", "message"],
                        "properties": {
                            "code": {"type": "string"},
                            "message": {"type": "string"}
                        }
                    }
                }
            }
        }
    })
}

fn paths() -> Value {
    json!({
        "/health": {
            "get": {
                "security": [],
                "summary": "Public process liveness",
                "responses": {"200": {"description": "Engine identity only"}}
            }
        },
        "/v1/openapi.json": {
            "get": {
                "summary": "This document",
                "responses": {
                    "200": {"description": "OpenAPI 3.1"},
                    "401": {"description": "Bearer required"}
                }
            }
        },
        "/v1/workflows": {
            "get": {
                "summary": "Contained workflow names",
                "responses": {"200": {"description": "Relative .nika.yaml names"}, "401": error_ref()}
            }
        },
        "/v1/workflows/{name}": {
            "get": {
                "summary": "Workflow metadata without source bytes",
                "parameters": [{"name": "name", "in": "path", "required": true, "schema": {"type": "string"}}],
                "responses": {"200": {"description": "Contained name"}, "401": error_ref(), "404": error_ref()}
            }
        },
        "/v1/jobs": {
            "post": {
                "summary": "Admit a job",
                "parameters": [{"$ref": "#/components/parameters/IdempotencyKey"}],
                "requestBody": {
                    "required": true,
                    "content": {"application/json": {"schema": {"$ref": "#/components/schemas/CreateJob"}}}
                },
                "responses": {
                    "202": {"description": "Created", "content": json_job()},
                    "200": {"description": "Idempotent replay", "content": json_job()},
                    "401": error_ref(),
                    "422": error_ref()
                }
            }
        },
        "/v1/jobs/{id}": {
            "get": {
                "summary": "Job identity and status",
                "parameters": [job_id_param()],
                "responses": {
                    "200": {"description": "Job", "content": json_job()},
                    "401": error_ref(),
                    "404": error_ref()
                }
            }
        },
        "/v1/jobs/{id}/status": {
            "get": {
                "summary": "Status only",
                "parameters": [job_id_param()],
                "responses": {
                    "200": {"description": "Status", "content": json_status()},
                    "401": error_ref(),
                    "404": error_ref()
                }
            }
        },
        "/v1/jobs/{id}/events": {
            "get": {
                "summary": "Job event SSE",
                "parameters": [
                    job_id_param(),
                    {"$ref": "#/components/parameters/LastEventId"}
                ],
                "responses": {
                    "200": {"description": "text/event-stream; data is {sequence,kind,status} plus optional redacted error {code,message}"},
                    "400": error_ref(),
                    "401": error_ref(),
                    "404": error_ref()
                }
            }
        }
    })
}

fn job_id_param() -> Value {
    json!({"name": "id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}})
}

fn json_job() -> Value {
    json!({"application/json": {"schema": {"$ref": "#/components/schemas/Job"}}})
}

fn json_status() -> Value {
    json!({"application/json": {"schema": {"$ref": "#/components/schemas/JobStatusOnly"}}})
}

fn error_ref() -> Value {
    json!({"description": "Error envelope", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}})
}

#[cfg(test)]
mod tests {
    use super::document;

    const LIVE_PATHS: &[&str] = &[
        "/health",
        "/v1/workflows",
        "/v1/workflows/{name}",
        "/v1/jobs",
        "/v1/jobs/{id}",
        "/v1/jobs/{id}/status",
        "/v1/jobs/{id}/events",
        "/v1/openapi.json",
    ];

    const ABSENT_PATHS: &[&str] = &["/v1/jobs/{id}/cancel", "/v1/jobs/{id}/artifacts", "/v1/run"];

    #[test]
    fn document_is_openapi_31_and_lists_only_live_paths() {
        let spec = document();
        assert_eq!(spec["openapi"], "3.1.0");
        let paths = spec["paths"].as_object().expect("paths");
        for path in LIVE_PATHS {
            assert!(paths.contains_key(*path), "missing {path}");
        }
        for path in ABSENT_PATHS {
            assert!(!paths.contains_key(*path), "must stay absent: {path}");
        }
        let rendered = spec.to_string();
        assert!(!rendered.contains("s3cret"));
        assert!(!rendered.contains("Bearer token-value"));
        assert!(!rendered.contains("/v1/run"));
        assert!(
            spec["paths"]["/v1/jobs"]["post"]["responses"]
                .as_object()
                .expect("post responses")
                .contains_key("422")
        );
    }
}
