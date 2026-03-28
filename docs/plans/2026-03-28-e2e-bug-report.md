# E2E Bug Report — 2026-03-28

> 12 real E2E workflows, 2 real LLM providers (OpenAI GPT-4o-mini), real HTTP calls, real media processing.

---

## BUGS FOUND

### BUG-1: `artifact: { source: alias }` fails with NIKA-281 (HIGH)

**Repro**: Workflow `01-media-import-thumbnail.nika.yaml`, task `original_artifact`
```yaml
- id: original_artifact
  depends_on: [import_red]
  with:
    img: $import_red
  invoke:
    tool: "nika:dimensions"
    params:
      hash: "{{with.img.hash}}"
  artifact:
    path: output/original_copy.png
    source: img
    format: binary
```

**Error**: `Binary artifact source 'img' resolved to hash '{"deduplicated":false,"hash":"blake3:...","mime_type":"image/png",...}' but no media ref matches`

**Root cause**: `source: img` resolves the `with:` binding to the full JSON output of the upstream task. The artifact processor then tries to match this full JSON string as a CAS hash, which obviously fails. The fallback MediaRef lookup at `artifact_processor.rs:224-268` doesn't handle the case where the source value is a JSON object containing a `.hash` field.

**Fix needed**: When `source:` resolves to a JSON string, parse it and extract the `.hash` field for MediaRef lookup. Or better: resolve `source:` using the same template engine so `source: "{{with.img.hash}}"` works.

**Severity**: HIGH — `source:` binding for binary artifacts is broken for all media tasks.

---

### BUG-2: `for_each` documentation uses WRONG syntax (HIGH — doc/schema)

**Wrong (in CLAUDE.md, dx/.claude/rules/nika.md, tools/nika/CLAUDE.md)**:
```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
  infer: "Process: {{with.item}}"
```

**Correct (what parser actually accepts)**:
```yaml
- id: process
  for_each: "{{with.data}}"
  as: item
  concurrency: 3
  infer: "Process: {{with.item}}"
```

**Root cause**: Parser at `nika-core/src/ast/raw/parser.rs:979-1023` only handles `Node::Sequence` (array) and `Node::Scalar` (string). `Node::Mapping` (object form) falls through to error: "for_each must be array or string".

**Impact**: Every AI editor and human reading our docs will write invalid workflows. The JSON schema (`nika-workflow.schema.json`) is CORRECT (expects array or string), but the CLAUDE.md docs are wrong.

**Files to fix**:
- `dx/.claude/rules/nika.md` (main source of truth)
- `tools/nika/CLAUDE.md`
- `nika/CLAUDE.md`
- `tools/nika-cli/rules/claude.md` (+ roo.md, windsurf.md, copilot.md)
- All showcase/course workflows that use object form (if any)

**Alternative fix**: Add `Node::Mapping` support in the parser to accept the object form too, then both syntaxes work.

---

### BUG-3: `trim` transform on non-string gives confusing error (MEDIUM)

**Repro**: `{{with.list_result | trim}}` where `list_result` is a JSON object from mock provider.

**Error**: `Transform 'trim' failed: expected string, got object`

**Improvement**: The error message should suggest `to_string | trim` or `to_json | trim` as alternatives. This applies to ALL string transforms (upper, lower, trim, trim_start, trim_end).

**File**: `nika-engine/src/binding/transforms.rs`

---

### BUG-4: Vision input tokens show 0 (LOW)

**Repro**: Workflow 08 (OpenAI vision) — Provider Breakdown shows `In: 0` for analyze_image.

**Expected**: Input tokens should include the image token estimate (varies by resolution).

**Root cause**: Image tokens aren't counted in the token estimation for vision requests. The cost shows $0.0000 which may undercount vision costs.

**File**: `nika-engine/src/provider/cost.rs` or `nika-engine/src/provider/rig.rs`

---

### BUG-5: `with:` block rejects template expressions (MEDIUM — UX)

**Repro**: Writing `count: "{{with.imgs | length}}"` in a `with:` block gives:
```
[NIKA-151] invalid binding expression '{{with.imgs | length}}': binding paths must start with '$'
```

**Impact**: Users can't compute derived values in `with:`. They must put ALL transforms inline in the verb prompt, which makes complex templates unreadable.

**Workaround**: Use transforms directly in prompt: `Processed {{with.imgs | length}} images.`

**Note**: This might be by design (with: is for bindings, not computation), but the error message should say "template expressions are not allowed in with: blocks — use transforms directly in your prompt template instead".

---

### BUG-6: `extract: markdown` includes CSS in output (LOW)

**Repro**: Workflow 05, `extract_markdown` task.

**Output**:
```markdown
Example Domain

body{background:#eee;width:60vw;margin:15vh auto;font-family:system-ui,sans-serif}h1{font-size:1.5em}div{opacity:0.8}a:link,a:visited{color:#348}

# Example Domain
```

**Expected**: CSS `<style>` content should be stripped from markdown output. The `htmd` converter is including inline CSS styles as plain text.

**File**: `nika-engine/src/runtime/executor/fetch.rs` (markdown extraction)

---

### BUG-7: `extracted_article.txt` has `.txt` extension but contains JSON (LOW — UX)

**Repro**: Workflow 05, extract_article returns structured JSON with `content`, `title`, `excerpt`, `byline` fields.

**Note**: This is correct behavior (Readability returns structured data), but the task output being JSON means artifact extension `.txt` is misleading. Users should use `.json` for article extraction.

**Doc improvement**: Document that `extract: article` returns JSON, not plain text.

---

## THINGS THAT WORK PERFECTLY

- **Auto-promote**: `artifact: { path: ... }` without `format: binary` correctly auto-detects media tasks and writes binary
- **Media pipeline**: import → thumbnail → convert → optimize → strip → thumbhash → dominant_color all work flawlessly
- **Fetch binary**: PNG, JPEG, SVG all correctly stored in CAS and written as artifacts
- **Fetch extract**: All 6 modes tested (markdown, article, metadata, links, jsonpath, full response)
- **OpenAI integration**: Text inference, structured output (JSON schema), vision all work with GPT-4o-mini
- **for_each**: Parallel import of 3 images with concurrency: 3 works correctly
- **DAG execution**: Complex dependency chains (8 tasks, 4 layers) execute in correct order
- **Artifact export**: Both `format: binary` explicit and auto-promoted work correctly

---

## TEST ARTIFACTS (all verified with `file` command)

| File | Size | Type |
|------|------|------|
| auto_promoted.jpg | 926 B | JPEG 100x100 |
| auto_promoted.png | 280 B | PNG 50x50 RGBA |
| converted.jpg | 1,461 B | JPEG 200x150 |
| converted.webp | 174 B | WebP |
| fetched.jpg | 35,588 B | JPEG 239x178 |
| fetched.png | 8,090 B | PNG 100x100 |
| fetched.svg | 8,984 B | SVG |
| optimized.png | 98 B | PNG 200x150 (1-bit) |
| stripped.png | 1,093 B | PNG 200x150 RGBA |
| thumb_32.png | 193 B | PNG 32x24 RGBA |
| thumb_50.png | 280 B | PNG 50x50 RGBA |
| thumb_100.png | 455 B | PNG 100x75 RGBA |
| openai_simple.txt | 14 B | "NIKA_OPENAI_OK" |
| openai_structured.json | 105 B | Valid JSON schema |
| vision_result.txt | 3 B | "Red" (correct!) |

## ANTHROPIC API STATUS

Anthropic key `sk-ant-a...` exists but has **no credits**. All Anthropic tests fail with:
```
Your credit balance is too low to access the Anthropic API.
Please go to Plans & Billing to upgrade or purchase credits.
```

This is NOT a Nika bug — it's an account billing issue.
