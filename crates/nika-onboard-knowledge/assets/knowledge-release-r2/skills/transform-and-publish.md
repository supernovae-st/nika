---
name: "nika-authoring-transform-and-publish"
description: "Solves requests that convert or reshape one file (CSV, image, rows) into a new artifact (chart, media, formatted file) and write the result to disk. Uses deterministic builtins (nika:convert, nika:cha"
nika_skill_role: "authoring"
nika_family: "family:transform-and-publish"
nika_skill_status: "candidate"
drafter: "xai/grok-4"
critic: "deepseek/deepseek-chat"
pin: "nika 0.120.3 (578352a31)"
spec_sha: "4b6eaadde483bcc9db9c05b022afbedfb107f37e"
---

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
- batch processing multiple files with for_each and group fan-in (group.<name> unresolved at run)

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
2. add permits: { fs: {read:[source], write:[out]}, tools:[nika:read,nika:convert,nika:chart,nika:write,nika:image_generate] }
3. task read_source: invoke nika:read with path:${{const.source_path}}
4. task transform: invoke nika:convert or nika:chart or nika:image_generate, binding input via with:
5. task write_result: invoke nika:write with path:${{const.output_path}}, content:${{with.transformed}}, overwrite:true, create_dirs:true
6. outputs: { result: ${{tasks.write_result.output}} }

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
- using columns: instead of has_header: on nika:convert (n60-convert-columns-arg)
- shell: under exec allowlist (n58-shell-under-allowlist)
- missing fs.write glob (NIKA-AUTH-006)
- writing to directory without create_dirs (runtime error)

## Diagnostic remedies
- `NIKA-AUTH-006` · add exact fs.write glob to permits
- `n60-convert-columns-arg` · use has_header + formula_guard
- `n68-required-arg-missing` · supply content: for nika:write

## Validation recipe
- nika check workflow.nika --native-strict
- nika run workflow.nika --model mock/echo --var source=... --var out=...; compare outputs.result path exists and is non-empty

## Positive example intent
Convert orders.csv to a bar chart SVG and write it to ./out/totals.svg

## Counterexample intent
Draft a French report from the CSV totals (belongs to draft-from-facts; this skill only does deterministic transform+write)
