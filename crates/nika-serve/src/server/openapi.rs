// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::{Value, json};

/// [`OpenAPI`](https://spec.openapis.org/oas/v3.1.0) document of the live HTTP surface.
///
/// Artifact and `POST /v1/run` paths are omitted until those authorities exist.
pub(crate) fn document() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "nika serve",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Authenticated loopback remote execution and declarative schedules. Artifacts, schedule list/delete/trigger/backfill, /v1/arm, and POST /v1/run are absent."
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
            },
            "IfMatch": {
                "name": "If-Match", "in": "header", "required": false,
                "schema": {"type": "string", "maxLength": 96}
            },
            "IfNoneMatch": {
                "name": "If-None-Match", "in": "header", "required": false,
                "schema": {"type": "string", "const": "*"}
            }
        },
        "schemas": schemas()
    })
}

fn schemas() -> Value {
    json!({
            "Health": health_schema(),
            "WorkflowList": workflow_list_schema(),
            "WorkflowMetadata": workflow_metadata_schema(),
            "JobStatus": {
                "type": "string",
                "enum": ["queued", "running", "interrupted", "paused", "succeeded", "failed", "cancelled"]
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
            "JobOrigin": job_origin_schema(),
            "JobReceipt": job_receipt_schema(),
            "JobEvent": job_event_schema(),
            "TraceVerification": trace_verification_schema(),
            "JobStatusOnly": {
                "type": "object",
                "additionalProperties": false,
                "description": "Status only. Redacted diagnosis lives on GET /v1/jobs/{id} and SSE, never here.",
                "required": ["status"],
                "properties": {
                    "status": {"$ref": "#/components/schemas/JobStatus"}
                }
            },
            "ExecutionSnapshot": {
                "type": "object",
                "additionalProperties": false,
                "description": "Immutable byte-owned execution world. Unit bytes are canonical lowercase hexadecimal and digests are canonical lowercase SHA-256. The decoded unit aggregate is limited to 16 MiB and the complete encoded request to 33 MiB. This object is the request body itself, not a path-bearing wrapper.",
                "required": ["format_version", "root", "digest", "units"],
                "properties": {
                    "format_version": {"type": "integer", "const": 1},
                    "root": {"type": "string", "minLength": 1, "maxLength": 4096},
                    "digest": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "units": {
                        "type": "array",
                        "maxItems": 256,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["path", "kind", "digest", "bytes_hex"],
                            "properties": {
                                "path": {"type": "string", "minLength": 1, "maxLength": 4096},
                                "kind": {"type": "integer", "minimum": 0, "maximum": 3},
                                "digest": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                                "bytes_hex": {"type": "string", "pattern": "^(?:[0-9a-f]{2})*$"}
                            }
                        }
                    }
                }
            },
            "SnapshotValidationAck": {
                "type": "object",
                "additionalProperties": false,
                "description": "Compact remote acknowledgement that the exact snapshot was revalidated. This is not the engine's public full check report; SDK callers retain the engine-owned report captured with the snapshot and return it only after this acknowledgement succeeds.",
                "required": ["status", "snapshot_digest", "root", "units"],
                "properties": {
                    "status": {"type": "string", "const": "accepted"},
                    "snapshot_digest": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "root": {"type": "string", "minLength": 1},
                    "units": {"type": "integer", "minimum": 1}
                }
            },
            "SchedulePut": schedule_put_schema(),
            "ScheduleApply": schedule_apply_schema(),
            "ScheduleStatus": {
                "type": "object",
                "description": "Normalized definition, origin, distinct schedule revision, active/pause state, due verdict, bounded next slots with shift evidence, earliest wake hint, and last durable decision.",
                "additionalProperties": true
            },
            "Error": error_schema()
    })
}

fn health_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["status", "service", "engine_version", "build_sha", "spec_sha", "api_version", "engineVersion", "buildSha", "specSha", "machineProtocolVersion", "snapshotFormatVersion", "checkReportVersion", "eventFormatVersion", "traceFormatVersion", "supportedCapabilities"],
        "properties": {
            "status": {"type": "string", "const": "ok"},
            "service": {"type": "string", "const": "nika-serve"},
            "engine_version": {"type": "string", "minLength": 1},
            "build_sha": {"type": "string", "minLength": 1},
            "spec_sha": {"type": "string", "minLength": 1},
            "api_version": {"type": "string", "minLength": 1},
            "engineVersion": {"type": "string", "minLength": 1},
            "buildSha": {"type": "string", "minLength": 1},
            "specSha": {"type": "string", "minLength": 1},
            "machineProtocolVersion": {"type": "integer", "minimum": 1},
            "snapshotFormatVersion": {"type": "integer", "minimum": 1},
            "checkReportVersion": {"type": "integer", "minimum": 1},
            "eventFormatVersion": {"type": "integer", "minimum": 1},
            "traceFormatVersion": {"type": "integer", "minimum": 1},
            "supportedCapabilities": {"type": "array", "items": {"type": "string"}, "uniqueItems": true}
        }
    })
}

fn workflow_list_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["workflows"],
        "properties": {"workflows": {"type": "array", "items": {"type": "string", "minLength": 1}}}
    })
}

fn workflow_metadata_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["workflow"],
        "properties": {"workflow": {"type": "string", "minLength": 1}}
    })
}

fn job_event_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["sequence", "kind", "status"],
        "properties": {
            "sequence": {"type": "integer", "minimum": 1},
            "kind": {"type": ["string", "null"]},
            "status": {"anyOf": [{"$ref": "#/components/schemas/JobStatus"}, {"type": "null"}]},
            "code": {"type": "string"},
            "message": {"type": "string"},
            "outputs": {"type": "object", "additionalProperties": true},
            "receipt": {"$ref": "#/components/schemas/JobReceipt"}
        }
    })
}

fn error_schema() -> Value {
    json!({
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
            "origin": {"$ref": "#/components/schemas/JobOrigin"},
            "chain_head": {"type": "string", "minLength": 1}
        }
    })
}

fn job_origin_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["kind"],
                "properties": {
                    "kind": {"type": "string", "const": "manual"}
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "kind", "schedule_origin", "schedule_id", "schedule_revision",
                    "slot_id", "decision", "scheduled_for", "fired_at", "arm_generation"
                ],
                "properties": {
                    "kind": {"type": "string", "const": "schedule"},
                    "schedule_origin": {"type": "string", "enum": ["project", "api"]},
                    "schedule_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 255,
                        "description": "Origin-local identifier, bounded to 255 UTF-8 bytes by the server"
                    },
                    "schedule_revision": {
                        "type": "string",
                        "pattern": "^sha256:[0-9a-f]{64}$"
                    },
                    "slot_id": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "decision": {"type": "string", "enum": ["scheduled", "catch_up"]},
                    "scheduled_for": {"type": "string", "format": "date-time"},
                    "fired_at": {"type": "string", "format": "date-time"},
                    "arm_generation": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
                }
            }
        ]
    })
}

fn trace_verification_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Run-scoped typed verdict. Unavailable is an honest refusal: this server has no remote trace-journal authority and never scans or returns filesystem paths.",
        "required": ["verdict", "reason"],
        "properties": {
            "verdict": {"type": "string", "enum": ["unavailable"]},
            "reason": {"type": "string", "enum": ["run_not_terminal", "trace_journal_unavailable"]},
            "trace_id": {"type": "string", "minLength": 1}
        }
    })
}

fn paths() -> Value {
    json!({
        "/health": health_path(),
        "/v1/openapi.json": openapi_path(),
        "/v1/workflows": workflow_list_path(),
        "/v1/workflows/{name}": workflow_metadata_path(),
        "/v1/jobs": jobs_path(),
        "/v1/check": check_path(),
        "/v1/jobs/{id}": job_path(),
        "/v1/jobs/{id}/status": job_status_path(),
        "/v1/jobs/{id}/events": job_events_path(),
        "/v1/jobs/{id}/cancel": job_cancel_path(),
        "/v1/jobs/{id}/trace/verify": job_trace_verify_path(),
        "/v1/schedules/{id}": schedule_path()
    })
}

fn schedule_apply_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["applied", "changed", "status"],
        "properties": {
            "applied": {"type": "boolean", "const": true},
            "changed": {"type": "boolean"},
            "status": {"$ref": "#/components/schemas/ScheduleStatus"}
        }
    })
}

fn schedule_put_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["workflow", "when", "maxCostUsd", "missed"],
        "properties": {
            "workflow": {"type": "string", "pattern": "^[^/].*\\.nika\\.yaml$", "maxLength": 1024},
            "when": {
                "oneOf": [
                    {"type": "object", "additionalProperties": false, "required": ["kind", "at"], "properties": {"kind": {"const": "once"}, "at": {"type": "string", "format": "date-time"}}},
                    {"type": "object", "additionalProperties": false, "required": ["kind", "expression"], "properties": {"kind": {"const": "cadence"}, "expression": {"type": "string", "maxLength": 4096}}}
                ]
            },
            "maxCostUsd": {"type": "number", "exclusiveMinimum": 0},
            "missed": {"type": "string", "enum": ["catch-up", "catch-up-once", "skip"]},
            "maxLatenessSeconds": {"type": "integer", "minimum": 0},
            "overlap": {"type": "string", "enum": ["skip", "queue", "replace"]},
            "afterSkip": {"type": "string", "enum": ["next_slot", "on_completion"]},
            "jitter": {"type": "string", "enum": ["hash"]},
            "tolerance": {"type": "string"},
            "active": {"type": "boolean"},
            "pauseReason": {"type": "string", "maxLength": 1024},
            "pauseUntil": {"type": "string", "format": "date"}
        }
    })
}

fn schedule_path() -> Value {
    json!({
        "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string", "minLength": 1, "maxLength": 255}}],
        "get": {
            "summary": "Read one declarative resident schedule",
            "responses": {"200": {"description": "Fresh planned status", "headers": {"ETag": {"schema": {"type": "string"}}}, "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ScheduleStatus"}}}}, "401": error_ref(), "404": error_ref()}
        },
        "put": {
            "summary": "Create or revision-conditionally update one resident schedule",
            "description": "Create requires If-None-Match: *. Update requires the exact ETag in If-Match. Identical lost-response retries are unchanged and retain the revision.",
            "parameters": [{"$ref": "#/components/parameters/IfNoneMatch"}, {"$ref": "#/components/parameters/IfMatch"}],
            "requestBody": {"required": true, "content": {"application/json": {"schema": {"$ref": "#/components/schemas/SchedulePut"}}}},
            "responses": {"200": {"description": "Applied or unchanged", "headers": {"ETag": {"schema": {"type": "string"}}}, "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ScheduleApply"}}}}, "401": error_ref(), "412": error_ref(), "413": error_ref(), "415": error_ref(), "422": error_ref(), "503": error_ref()}
        }
    })
}

fn health_path() -> Value {
    json!({"get": {
        "security": [],
        "summary": "Public process liveness",
        "responses": {"200": {"description": "Engine identity only", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Health"}}}}}
    }})
}

fn openapi_path() -> Value {
    json!({"get": {
        "summary": "This document",
        "responses": {"200": {"description": "OpenAPI 3.1"}, "401": {"description": "Bearer required"}}
    }})
}

fn workflow_list_path() -> Value {
    json!({"get": {
        "summary": "Contained workflow names",
        "responses": {"200": {"description": "Relative .nika.yaml names", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/WorkflowList"}}}}, "401": error_ref()}
    }})
}

fn workflow_metadata_path() -> Value {
    json!({"get": {
        "summary": "Workflow metadata without source bytes",
        "parameters": [{"name": "name", "in": "path", "required": true, "schema": {"type": "string"}}],
        "responses": {"200": {"description": "Contained name", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/WorkflowMetadata"}}}}, "401": error_ref(), "404": error_ref()}
    }})
}

fn jobs_path() -> Value {
    json!({"post": {
        "summary": "Admit immutable snapshot bytes as a durable job",
        "description": "The server decodes and readmits this exact body through ExecutionService and never interprets a caller filesystem path. Idempotency binds to the exact snapshot payload bytes.",
        "parameters": [{"$ref": "#/components/parameters/IdempotencyKey"}],
        "requestBody": snapshot_request_body(),
        "responses": {
            "202": {"description": "Created", "content": json_job()},
            "200": {"description": "Idempotent replay", "content": json_job()},
            "400": error_named("Invalid idempotency key"), "401": error_ref(),
            "408": error_named("Request deadline"),
            "409": error_named("Idempotency key already bound to another request"),
            "413": error_named("Encoded body or decoded snapshot resource limit"),
            "415": error_named("Content-Type or Content-Encoding refused"),
            "422": error_named("Malformed, unsupported, tampered, or semantically refused snapshot"),
            "503": error_named("Execution queue or durable store unavailable"),
            "507": error_named("Durable job capacity exhausted")
        }
    }})
}

fn check_path() -> Value {
    json!({"post": {
        "summary": "Judge immutable snapshot bytes without creating a job",
        "description": "Runs the same decode and ExecutionService readmission as POST /v1/jobs over the exact request body.",
        "requestBody": snapshot_request_body(),
        "responses": {
            "200": {"description": "Compact snapshot validation acknowledgement, not the full engine check report", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/SnapshotValidationAck"}}}},
            "401": error_ref(), "408": error_named("Request deadline"),
            "413": error_named("Encoded body or decoded snapshot resource limit"),
            "415": error_named("Content-Type or Content-Encoding refused"),
            "422": error_named("Malformed, unsupported, tampered, or semantically refused snapshot")
        }
    }})
}

fn job_path() -> Value {
    json!({"get": {
        "summary": "Job identity and status", "parameters": [job_id_param()],
        "responses": {"200": {"description": "Job", "content": json_job()}, "401": error_ref(), "404": error_ref()}
    }})
}

fn job_status_path() -> Value {
    json!({"get": {
        "summary": "Status only; diagnosis lives on GET /v1/jobs/{id} and SSE",
        "parameters": [job_id_param()],
        "responses": {"200": {"description": "Status", "content": json_status()}, "401": error_ref(), "404": error_ref()}
    }})
}

fn job_events_path() -> Value {
    json!({"get": {
        "summary": "Job event SSE",
        "parameters": [job_id_param(), {"$ref": "#/components/parameters/LastEventId"}],
        "responses": {
            "200": {
                "description": "text/event-stream; sends bounded retry guidance and cursor-neutral heartbeat comments; Last-Event-ID replays only persisted events after that sequence; terminal data adds declared outputs and receipt when available; failures add redacted {code,message}",
                "content": {"text/event-stream": {"schema": {"type": "string"}, "x-nika-event-schema": {"$ref": "#/components/schemas/JobEvent"}}}
            },
            "400": error_ref(), "401": error_ref(), "404": error_ref()
        }
    }})
}

fn job_cancel_path() -> Value {
    json!({"post": {
        "summary": "Idempotently cancel a queued, running, or paused job",
        "description": "Cancellation signals the run-scoped engine token before durable terminal settlement. A terminal replay returns the existing result unchanged.",
        "parameters": [job_id_param()],
        "responses": {"200": {"description": "Cancelled or already terminal job", "content": json_job()}, "401": error_ref(), "404": error_ref(), "503": error_ref()}
    }})
}

fn job_trace_verify_path() -> Value {
    json!({"get": {
        "summary": "Return the run-scoped trace verification verdict",
        "description": "Returns a typed honest refusal while no remote trace-journal authority exists. It never scans a trace directory or exposes a filesystem path.",
        "parameters": [job_id_param()],
        "responses": {"200": {"description": "Typed trace verdict", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/TraceVerification"}}}}, "401": error_ref(), "404": error_ref()}
    }})
}

fn snapshot_request_body() -> Value {
    json!({
        "required": true,
        "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ExecutionSnapshot"}}}
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
    use nika_cadence::firing::{ArmGeneration, SlotId};
    use nika_cadence::{ScheduleDecision, ScheduleOrigin, ScheduleRevision};
    use serde_json::Value;

    use crate::{JobId, JobOrigin, JobReceipt};

    use super::document;

    const LIVE_PATHS: &[&str] = &[
        "/health",
        "/v1/workflows",
        "/v1/workflows/{name}",
        "/v1/check",
        "/v1/jobs",
        "/v1/jobs/{id}",
        "/v1/jobs/{id}/status",
        "/v1/jobs/{id}/events",
        "/v1/jobs/{id}/cancel",
        "/v1/jobs/{id}/trace/verify",
        "/v1/schedules/{id}",
        "/v1/openapi.json",
    ];

    const ABSENT_PATHS: &[&str] = &[
        "/v1/jobs/{id}/artifacts",
        "/v1/run",
        "/v1/schedules",
        "/v1/schedules/{id}/trigger",
        "/v1/schedules/{id}/backfill",
        "/v1/arm",
    ];

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
        assert_eq!(
            receipt["properties"]["origin"]["$ref"],
            "#/components/schemas/JobOrigin"
        );
        assert!(
            !spec["components"]["schemas"]["Job"]["required"]
                .as_array()
                .expect("job required fields")
                .iter()
                .any(|field| field == "outputs" || field == "receipt"),
            "terminal result fields remain optional for legacy and unavailable adapters"
        );
        for (path, schema) in [
            ("/health", "Health"),
            ("/v1/workflows", "WorkflowList"),
            ("/v1/workflows/{name}", "WorkflowMetadata"),
        ] {
            assert_eq!(
                spec["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
                    ["$ref"],
                format!("#/components/schemas/{schema}"),
                "{path} must expose its machine response schema"
            );
        }
        assert_eq!(
            spec["paths"]["/v1/jobs/{id}/events"]["get"]["responses"]["200"]["content"]["text/event-stream"]
                ["x-nika-event-schema"]["$ref"],
            "#/components/schemas/JobEvent"
        );
        assert_eq!(
            spec["components"]["schemas"]["JobEvent"]["required"],
            serde_json::json!(["sequence", "kind", "status"])
        );
    }

    #[test]
    fn job_origin_schema_tracks_serialized_manual_and_schedule_receipts() {
        let spec = document();
        let variants = spec["components"]["schemas"]["JobOrigin"]["oneOf"]
            .as_array()
            .expect("origin variants");
        assert_eq!(variants.len(), 2);

        let job_id = JobId::parse("3d6f0a7d-27d4-4e48-b17a-fc6fdb40255b").expect("job id");
        let manual = JobReceipt::new(
            job_id.clone(),
            "exec-manual",
            "trace-manual",
            "a".repeat(64),
            None,
        )
        .expect("manual receipt");
        assert_variant_matches_receipt(&variants[0], &manual, "manual");

        let revision = ScheduleRevision::from_wire(&format!("sha256:{}", "b".repeat(64)))
            .expect("schedule revision");
        let slot = SlotId::from_wire(&"c".repeat(64)).expect("slot id");
        let generation = ArmGeneration::from_wire(&"d".repeat(64)).expect("arm generation");
        let origin = JobOrigin::schedule(
            ScheduleOrigin::Api,
            "nightly",
            &revision,
            &slot,
            ScheduleDecision::CatchUp,
            "2026-08-31T01:00:00Z".parse().expect("scheduled timestamp"),
            "2026-08-31T01:00:01Z".parse().expect("fired timestamp"),
            &generation,
        )
        .expect("scheduled origin");
        let scheduled = JobReceipt::with_origin(
            job_id,
            "exec-scheduled",
            "trace-scheduled",
            "e".repeat(64),
            Some("chain-head".to_owned()),
            origin,
        )
        .expect("scheduled receipt");
        assert_variant_matches_receipt(&variants[1], &scheduled, "schedule");

        let schedule_schema = &variants[1];
        assert_eq!(
            schedule_schema["properties"]["schedule_origin"]["enum"],
            serde_json::json!(["project", "api"])
        );
        assert_eq!(
            schedule_schema["properties"]["decision"]["enum"],
            serde_json::json!(["scheduled", "catch_up"])
        );
        for field in ["slot_id", "arm_generation"] {
            assert_eq!(
                schedule_schema["properties"][field]["pattern"],
                "^[0-9a-f]{64}$"
            );
        }
        assert_eq!(
            schedule_schema["properties"]["schedule_revision"]["pattern"],
            "^sha256:[0-9a-f]{64}$"
        );
    }

    fn assert_variant_matches_receipt(schema: &Value, receipt: &JobReceipt, kind: &str) {
        let receipt = serde_json::to_value(receipt).expect("serialize receipt");
        let origin = receipt["origin"].as_object().expect("serialized origin");
        let properties = schema["properties"].as_object().expect("schema properties");
        let required = schema["required"].as_array().expect("required fields");

        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["kind"]["const"], kind);
        assert_eq!(origin["kind"], kind);
        assert_eq!(origin.len(), properties.len());
        assert_eq!(required.len(), properties.len());
        for field in origin.keys() {
            assert!(
                properties.contains_key(field),
                "undocumented origin field: {field}"
            );
        }
        for field in required {
            let field = field.as_str().expect("required field name");
            assert!(
                origin.contains_key(field),
                "serialized origin omitted {field}"
            );
        }
    }
}
