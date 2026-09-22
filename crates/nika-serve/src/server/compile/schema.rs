// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `OpenAPI` fragments of the compile door. Every bound cites the constant the
//! handler enforces, so the published contract cannot drift from the refusal.
//! Literals stay shallow on purpose: `json!` expands recursively per token.

use nika_onboard::compile::{AuthoringCognition, COMPILE_WIRE_VERSION};
use serde_json::{Value, json};

use super::{
    MAX_COMPILE_ANSWER_KEY_BYTES, MAX_COMPILE_ANSWERS, MAX_COMPILE_BODY_BYTES,
    MAX_COMPILE_LITERAL_BYTES, MAX_COMPILE_NAME_BYTES, MAX_COMPILE_SOURCE_BYTES,
    MAX_COMPILE_TEXT_BYTES,
};

/// The core's own word, so the published contract cannot drift from the judge.
const DETERMINISTIC_ONLY: &str = AuthoringCognition::DeterministicOnly.word();

pub(in crate::server) fn path() -> Value {
    let responses = json!({
        "200": {
            "description": "Authoring outcome — ready, incomplete and refused are all data",
            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/CompileOutcome"}}}
        },
        "401": error("Error envelope"),
        "408": error("Request deadline"),
        "413": error("Encoded body above the compile ceiling"),
        "415": error("Content-Type or Content-Encoding refused"),
        "422": error("malformed_compile_request · compile_version_unsupported · compile_mode_unsupported · compile_cognition_unsupported · compile_limit"),
        "500": error("Compiler machinery failure; nothing is echoed"),
        "503": error("compile_busy — every compile slot is in use")
    });
    json!({"post": {
        "summary": "Author a candidate workflow without running it (stateless · source-only)",
        "description": "The HTTP transport of the same Compile core as `nika compile`. Foundation scope: CREATE resolves an exact embedded skeleton name (or `hello`), EDIT changes one existing constant, answers are explicit JSON literals; any other intent is answered `incomplete` with no substitute workflow. Creates no job, run, approval or trace, writes no file, contacts no provider: ambient keys are never consent. `check_preview` is a REVIEW of the source only, never admission: POST /v1/jobs judges a candidate again. Questions carry stable keys; answering is a new request, not a session.",
        "requestBody": {"required": true, "content": {"application/json": {"schema": {"$ref": "#/components/schemas/CompileRequest"}}}},
        "responses": responses
    }})
}

pub(in crate::server) fn request() -> Value {
    let properties = json!({
        "compile_version": {"type": "integer", "const": COMPILE_WIRE_VERSION},
        "mode": {"type": "string", "enum": ["create", "edit"], "description": "create requires `intent` and forbids `source`/`change`; edit requires `source` and `change` and forbids `intent`"},
        "intent": {"type": "string", "maxLength": MAX_COMPILE_TEXT_BYTES},
        "workflow_id": {"type": "string", "maxLength": MAX_COMPILE_NAME_BYTES, "description": "Names a created workflow. On edit the core refuses it as data: an edit cannot rename its accepted base"},
        "source": {"type": "string", "maxLength": MAX_COMPILE_SOURCE_BYTES, "description": "Accepted `.nika` source, inline. The caller owns source selection, revision checks and materialization"},
        "change": change(),
        "answers": answers(),
        "cognition": {"type": "string", "const": DETERMINISTIC_ONLY, "description": "The only authoring cognition of this build. Any other value is refused; no authoring model is contacted"}
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": format!("Generation {COMPILE_WIRE_VERSION} of the compile request. The encoded body is limited to {MAX_COMPILE_BODY_BYTES} bytes (or the listener's lower ceiling). Byte bounds below are UTF-8 bytes. Unknown fields, a present null, duplicate keys (including inside `answers`) and positional arrays are refused. No field names a host path: an EDIT base travels inline in `source`."),
        "required": ["compile_version", "mode"],
        "properties": properties
    })
}

fn change() -> Value {
    let set_constant = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "value"],
        "properties": {
            "name": {"type": "string", "maxLength": MAX_COMPILE_NAME_BYTES, "description": "Bare constant name, not a path"},
            "value": {"description": format!("One JSON literal, judged exactly as sent (at most {MAX_COMPILE_LITERAL_BYTES} bytes)")}
        }
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "minProperties": 1,
        "maxProperties": 1,
        "properties": {
            "text": {"type": "string", "maxLength": MAX_COMPILE_TEXT_BYTES, "description": "`Set const.NAME to JSON_LITERAL`, or `Set const.NAME` answered through `answers`"},
            "set_constant": set_constant
        }
    })
}

fn answers() -> Value {
    json!({
        "type": "object",
        "maxProperties": MAX_COMPILE_ANSWERS,
        "propertyNames": {"maxLength": MAX_COMPILE_ANSWER_KEY_BYTES},
        "description": format!("Question key → one JSON literal with its type preserved, judged exactly as sent (at most {MAX_COMPILE_LITERAL_BYTES} bytes each)"),
        "additionalProperties": true
    })
}

pub(in crate::server) fn outcome() -> Value {
    let properties = json!({
        "compile_version": {"type": "integer", "const": COMPILE_WIRE_VERSION},
        "status": {"type": "string", "enum": ["ready", "incomplete", "refused"], "description": "Completeness of authoring, never permission to execute"},
        "candidate": {"type": ["string", "null"], "description": "Ordinary `.nika` source; may still be incomplete"},
        "questions": {"type": "array", "items": question()},
        "diagnostics": {"type": "array", "items": diagnostic()},
        "requested_boundary": {"type": ["object", "null"], "description": "The candidate's requested permits, derived by Check. Requested, never granted", "additionalProperties": true},
        "requested_trigger": {"type": ["object", "null"], "description": "The trigger the request names (kind · source_hint · event_hint · cadence · at · payload_input · status · timezone · missed · overlap · ceiling), stated beside the candidate whose bytes carry no cadence, host or event. A requirement the operator binds through the schedule contract, never a grant or a schedule row; the last four are the answered binding values (null until answered)", "additionalProperties": true},
        "check_preview": preview(),
        "provenance": provenance()
    });
    json!({
        "type": "object",
        "description": "The engine-owned machine document of one authoring result — the document `nika compile --json` prints, without the CLI-only `written`. No field grants authority, writes or executes source.",
        "required": ["compile_version", "status", "candidate", "questions", "diagnostics", "requested_boundary", "requested_trigger", "check_preview", "provenance"],
        "properties": properties
    })
}

fn question() -> Value {
    json!({
        "type": "object",
        "required": ["key", "label", "type", "why", "mandatory"],
        "properties": {
            "key": {"type": "string", "description": "Stable semantic hole path such as `const.request`, never a session id"},
            "label": {"type": "string"},
            "type": {"type": "string", "enum": ["text", "literal", "choice"], "description": "text: a JSON string · literal: one JSON value · choice: a JSON string that is the `key` of one of `options`"},
            "why": {"type": "string"},
            "mandatory": {"type": "boolean", "description": "false: the value belongs to a binding outside the program (a schedule's timezone, missed-run and overlap policies, per-run ceiling) and never blocks a ready candidate"},
            "options": {"type": "array", "description": "Present on a choice question only: the admissible answers, keys spelled by the owning grammar", "items": {"type": "object", "required": ["key", "label"], "properties": {"key": {"type": "string"}, "label": {"type": "string"}}}}
        }
    })
}

fn diagnostic() -> Value {
    json!({
        "type": "object",
        "required": ["kind", "target", "message"],
        "properties": {
            "kind": {"type": "string", "enum": ["applied", "missed", "unknown", "requiresHuman", "refused"]},
            "target": {"type": "string"},
            "message": {"type": "string", "description": "For a reader; never parsed to recover compiler state"}
        }
    })
}

fn preview() -> Value {
    json!({
        "type": ["object", "null"],
        "description": "The engine's pure Check report over the source only. No child file, skill, credential probe, access plan or admission was evaluated",
        "required": ["scope", "report"],
        "properties": {
            "scope": {"type": "string", "const": "sourceOnly"},
            "report": {"type": "object", "additionalProperties": true}
        }
    })
}

fn provenance() -> Value {
    json!({
        "type": "object",
        "description": "Reproduction metadata; neither program identity nor run evidence",
        "required": ["compiler_version", "spec_pin", "skeleton", "cognition"],
        "properties": {
            "compiler_version": {"type": "string"},
            "spec_pin": {"type": "string"},
            "skeleton": {"type": ["string", "null"]},
            "cognition": {"type": "string", "const": DETERMINISTIC_ONLY}
        }
    })
}

fn error(description: &'static str) -> Value {
    json!({"description": description, "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}})
}
