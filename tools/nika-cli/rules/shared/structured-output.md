## Structured Output — 4-Layer Defense

`structured:` enforces schema-validated JSON output with automatic retry and repair.
The prompt MUST be **natural language** — NEVER mention JSON or the schema in the prompt.

```yaml
- id: extract
  infer: "Tell me about Alice, a 30-year-old Rust and Python developer"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        age: { type: number, minimum: 0 }
        skills: { type: array, items: { type: string }, minItems: 1 }
      required: [name, age, skills]
    enable_repair: true
    max_retries: 3
    repair_model: claude-haiku-4-5
```

**4 layers**: tool injection (provider-native) → JSON schema validation → retry with feedback → LLM repair.

Result: valid JSON matching the schema. Same result on ALL providers.

**Different from** `output: { format: json }` which is formatting only — no validation, no repair.
