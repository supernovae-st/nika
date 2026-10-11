---
name: "nika-authoring-structured-extraction-from-documents"
description: "Solves requests that turn prose documents into structured typed records by extracting explicit named fields. Enforces preserve-nulls and no-invention, applies schema validation, then stores the result"
nika_skill_role: "authoring"
nika_family: "family:structured-extraction-from-documents"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

# Extract named fields from prose into typed records (nulls preserved, validated, stored)

## Scope
Solves requests that turn prose documents into structured typed records by extracting explicit named fields. Enforces preserve-nulls and no-invention, applies schema validation, then stores the result. Matches the meeting-actions and contract-guard gallery examples.

## When to use
- User names fields to extract from a prose document and wants absent ones kept as null
- Schema validation and artifact storage are required after extraction
- Quality constraints no-invention and preserve-nulls are stated or implied

## When not to use
- Free-form summarization or drafting (use draft skill)
- Simple file read without extraction (use pattern:read-known-file)
- Pure numeric aggregation without prose extraction (use aggregate-number)

## Facets
```json
{"goal": ["extract"], "source": ["local-file"], "transformation": ["structured-extraction", "schema-validation"], "effect": ["write-artifact"], "authority": ["automatic"], "control_flow": ["linear"]}
```

## Required business information (human-owned)
- source document path
- named fields to extract
- output schema
- storage path for records

## Machine-owned holes
- jq expressions inside nika:jq or extract
- glob patterns for permits.fs
- prompt text for infer
- schema literal for returns/validate

## Required capabilities
- StructuredExtractor
- Validator
- ArtifactWriter

## Procedure
1. 1. Declare typed inputs for paths and schema; use const for fixed values (inputs.typed, const.forms)
2. 2. Grant permits.fs.read for source and permits.fs.write for output; add tools nika:read, nika:validate, nika:write (permits.default-deny)
3. 3. Read source with invoke nika:read (pattern:read-known-file)
4. 4. Run infer + returns (or schema) for structured extraction; absent fields become null (pattern:structured-extraction, infer.output, returns.typed-door)
5. 5. Validate result with invoke nika:validate (pattern:validate-records)
6. 6. Write validated records with invoke nika:write + overwrite/create_dirs (pattern:write-artifact)
7. 7. Wire data edges with with: bindings and outputs: references (with.is-the-edge, outputs.forms)

## Compatible patterns
- pattern:structured-extraction
- pattern:validate-records
- pattern:write-artifact
- pattern:typed-output
- pattern:extract-then-law

## Compatible blocks
- block:infer-structured-schema
- block:validate-diff-convert

## Authority and effects
- permits.fs.read (source glob), permits.fs.write (output path); no secrets; human gate only if nika:prompt is added for approval

## Common mistakes
- Putting tasks.* inside infer prompt (NIKA-VAR-021)
- Using both schema and returns on same infer (NIKA-TYPE-003)
- Forgetting null preservation instruction in prompt

## Diagnostic remedies
- `NIKA-TYPE-003` · choose either returns or schema, never both
- `NIKA-VAR-021` · hoist reference into with: binding
- `NIKA-PARSE-013` · rename extract binding away from reserved names

## Validation recipe
- nika check workflow.nika --native-strict
- nika run --model mock/echo in isolated workspace; compare extracted nulls and validated records against expected JSON

## Positive example intent
Extract decisions, owners and deadlines from the meeting transcript; keep absent fields null, validate against schema and store the typed records.

## Counterexample intent
Write a free-text summary of the transcript (this skill requires structured field extraction with null preservation, not open drafting)
