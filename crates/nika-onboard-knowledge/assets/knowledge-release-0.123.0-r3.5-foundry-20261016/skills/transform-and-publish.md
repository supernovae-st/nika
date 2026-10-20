---
name: "nika-authoring-transform-and-publish"
description: "Solves requests that convert or reshape one file (CSV, image, rows) into a new artifact (chart, media, formatted file) and write the result to disk. Uses deterministic builtins (nika:convert, nika:cha"
nika_skill_role: "authoring"
nika_family: "family:transform-and-publish"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.123.0-preview.1 (5873050df)"
spec_sha: "ac7e64cdd6f491d3b84f85bf25503b8bb6274b26"
---

> Unverified draft: every code step checked on nika 0.123.0-preview.1 (5873050df), none run.

# Transform artifact and publish result

## Scope
Solves requests that convert or reshape one file (CSV, image, rows) into a new artifact (chart, media, formatted file) and write the result to disk. Uses deterministic builtins (nika:convert, nika:chart, nika:image_generate, nika:write) under explicit fs.write and tools permits; never invents content or crosses into drafting/research.

## When to use
- request names a source file + desired output format/artifact (chart, image, converted file)
- goal is transform + publish or generate-media with write-artifact effect
- transformation is deterministic (convert, chart, fx) or media generation from prompt+provider

## When not to use
- drafting prose or summaries from facts (use draft-from-facts pattern instead)
- reading multiple files or discovering folders (use read-known-file or EnumerableSource)
- any human approval gate or secret egress required
- many files at once (use batch-process-and-report)

## Facets
```json
{"goal": ["transform", "publish", "generate-media"], "source": [], "transformation": [], "effect": ["write-artifact", "media-artifact"], "authority": [], "control_flow": []}
```

## Required business information (human-owned)
- source file path or folder
- output file path
- chart semantics or image prompt/provider
- overwrite/create_dirs policy

## Machine-owned holes
- jq expressions for nika:jq or extract
- exact fs.read/fs.write globs
- permits.tools list
- task topology and with: bindings

## Required capabilities
- FormatConverter
- ArtifactWriter
- MediaRenderer
- RecordTransformer

## Procedure
1. declare nika: and const: for source_path/output_path (human values)
2. grant permits.fs.read on the source file, permits.fs.write on the output file, and permits.tools for nika:read, nika:chart, nika:image_generate and nika:write (nika:convert is pure and needs no grant)
3. read_source: nika:read with path: "${{ const.source_path }}"
4. task transform: invoke nika:convert or nika:chart or nika:image_generate, binding input via with:
5. write_result: nika:write with path: "${{ const.output_path }}", content: "${{ with.transformed }}", overwrite: true and create_dirs: true
6. outputs: result reads "${{ tasks.write_result.output }}", the written path

## Compatible patterns
- pattern:generate-media
- pattern:render-chart
- pattern:write-artifact
- pattern:parse-csv-records
- pattern:read-known-file

## Compatible blocks
- block:chart-from-rows
- block:csv-filter-total-report
- block:permits-full-boundary

## Authority and effects
- permits.fs.write for output path, permits.tools for nika:chart/nika:write/nika:image_generate; no secrets egress; no human gate required

## Common mistakes
- expecting a written CSV to keep the source's column order: nika:convert writes the listed columns first (columns: [region, amount]), then the others sorted
- shell: under exec allowlist (n58-shell-under-allowlist)
- missing fs.write glob (NIKA-AUTH-006)
- writing to directory without create_dirs (runtime error)

## Diagnostic remedies
- `NIKA-AUTH-006` · add exact fs.write glob to permits
- `NIKA-BUILTIN-001` · name only the arguments the tool declares (nika:convert: input, from, to, has_header, formula_guard, columns)
- `n68-required-arg-missing` · supply content: for nika:write

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo in a copy of the project; compare outputs.result (the written path) and the file's content

## Positive example intent
Convert orders.csv to a bar chart SVG and write it to ./out/totals.svg

## Counterexample intent
Draft a French report from the CSV totals (belongs to draft-from-facts; this skill only does deterministic transform+write)
