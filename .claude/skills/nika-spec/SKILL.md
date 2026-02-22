---
name: nika-spec
description: Route to Nika spec. Use when user mentions spec, schema, actions, workflow, YAML syntax, error codes, or any Nika feature question.
allowed-tools: Read, Grep
---

# Nika Spec Router

> **NEVER duplicate spec content here. Always read the source.**

## Source of Truth

```
spec/SPEC.md
```

**Always read the spec file.** This skill is a ROUTER, not a COPY.

## How to Use

### For any Nika question:

```bash
Read spec/SPEC.md
```

### For specific topics:

| Topic | Spec Section | Grep Pattern |
|-------|--------------|--------------|
| Actions | Section 4 | `grep "### infer\|### exec\|### fetch" spec/SPEC.md` |
| Use block | Section 6 | `grep -A20 "## 6. Use Block" spec/SPEC.md` |
| Templates | Section 7 | `grep -A15 "## 7. Template" spec/SPEC.md` |
| Errors | Section 11 | `grep "NIKA-" spec/SPEC.md` |
| DAG/Flow | Section 5 | `grep -A20 "## 5. Flow" spec/SPEC.md` |

## Spec Structure (12 Sections)

1. Unified Vocabulary
2. Workflow
3. Task
4. **Actions** (infer, exec, fetch)
5. Flow (DAG)
6. **Use Block**
7. Template
8. Output
9. Runtime
10. Strict Mode
11. **Error Codes**
12. Code Architecture

## Anti-Hallucination

Before answering ANY Nika feature question:

1. **Read spec/SPEC.md**
2. **Quote the relevant section**
3. **Never invent features**

If it's not in spec/SPEC.md, it doesn't exist.
