// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::{Value, json};

/// [`OpenAPI`](https://spec.openapis.org/oas/v3.1.0) document of the live HTTP surface.
///
/// Cancel, artifact, and `POST /v1/run` paths are omitted until those
/// authorities exist.
pub(crate) fn document() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "nika serve",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Authenticated loopback remote execution. Cancel, artifacts, and POST /v1/run are absent."
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
                    "outputs": {
                        "type": "object",
                        "description": "Declared workflow outputs; present only after settlement when supplied by the execution adapter",
                        "additionalProperties": true
                    },
                    "receipt": {"$ref": "#/components/schemas/JobReceipt"},
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
            "JobReceipt": job_receipt_schema(),
            "JobStatusOnly": {
                "type": "object",
                "additionalProperties": false,
                "description": "Status only. Redacted diagnosis lives on GET /v1/jobs/{id} and SSE, never here.",
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

fn job_receipt_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Terminal binding to the exact immutable admitted execution",
        "required": ["job_id", "execution_id", "trace_id", "snapshot_digest"],
        "properties": {
            "job_id": {"type": "string", "format": "uuid"},
            "execution_id": {"type": "string", "minLength": 1},
            "trace_id": {"type": "string", "minLength": 1},
            "snapshot_digest": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "chain_head": {"type": "string", "minLength": 1}
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
                    "400": error_named("Invalid JSON or idempotency key"),
                    "401": error_ref(),
                    "408": error_named("Request deadline"),
                    "409": error_named("Idempotency key already bound to another request"),
                    "413": error_named("Request body exceeds the configured limit"),
                    "415": error_named("Content-Type or Content-Encoding refused"),
                    "422": error_named("Admission or contained workflow name refused"),
                    "503": error_named("Execution queue or durable store unavailable"),
                    "507": error_named("Durable job capacity exhausted")
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
                "summary": "Status only; diagnosis lives on GET /v1/jobs/{id} and SSE",
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
                    "200": {"description": "text/event-stream; terminal data adds declared outputs and receipt when available; failures add redacted {code,message}"},
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
    error_named("Error envelope")
}

fn error_named(description: &'static str) -> Value {
    json!({"description": description, "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}})
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
        let description = spec["info"]["description"].as_str().expect("description");
        assert!(
            description.contains("POST /v1/run"),
            "description must name the absent run door"
        );
        let post = spec["paths"]["/v1/jobs"]["post"]["responses"]
            .as_object()
            .expect("post responses");
        for status in [
            "200", "202", "400", "401", "408", "409", "413", "415", "422", "503", "507",
        ] {
            assert!(post.contains_key(status), "missing POST {status}");
        }
        let status_summary = spec["paths"]["/v1/jobs/{id}/status"]["get"]["summary"]
            .as_str()
            .expect("status summary");
        assert!(
            status_summary.contains("diagnosis"),
            "status route must say diagnosis lives elsewhere"
        );
        let receipt = &spec["components"]["schemas"]["JobReceipt"];
        assert_eq!(receipt["additionalProperties"], false);
        assert_eq!(
            receipt["required"],
            serde_json::json!(["job_id", "execution_id", "trace_id", "snapshot_digest"])
        );
        assert_eq!(
            receipt["properties"]["snapshot_digest"]["pattern"],
            "^[0-9a-f]{64}$"
        );
        assert!(
            !spec["components"]["schemas"]["Job"]["required"]
                .as_array()
                .expect("job required fields")
                .iter()
                .any(|field| field == "outputs" || field == "receipt"),
            "terminal result fields remain optional for legacy and unavailable adapters"
        );
    }
}
