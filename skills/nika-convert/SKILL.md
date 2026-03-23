---
name: nika-convert
description: >-
  Convert other automation formats to Nika YAML workflows (.nika.yaml). Supports
  conversion from GitHub Actions, Makefiles, shell scripts, docker-compose,
  CI/CD pipelines, and task runners to Nika schema nika/workflow@0.12 format.
  Use when users want to migrate, port, or translate existing automation to
  Nika .nika.yaml workflows.
---

# Convert to Nika Workflows

Translate existing automation formats into `.nika.yaml` workflows.

## Conversion Principles

1. **Map actions to verbs**: shell commands -> `exec:`, API calls -> `fetch:`, LLM calls -> `infer:`, tool calls -> `invoke:`, complex autonomous steps -> `agent:`
2. **Preserve dependencies**: job/step ordering -> `depends_on:`
3. **Map variables**: env vars -> `inputs:` or `{{$env.VAR}}`
4. **Map parallelism**: matrix/loops -> `for_each:`
5. **Map outputs**: file writes -> `artifact:`

## GitHub Actions to Nika

### Before (GitHub Actions)

```yaml
# .github/workflows/build.yml
name: Build and Test
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: npm install
      - run: npm test
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: npm run lint
  deploy:
    needs: [test, lint]
    runs-on: ubuntu-latest
    steps:
      - run: npm run build
      - run: npm run deploy
```

### After (Nika)

```yaml
schema: nika/workflow@0.12
workflow: build-and-test

tasks:
  - id: test
    exec:
      command: "npm install && npm test"
      shell: true

  - id: lint
    exec: "npm run lint"

  - id: deploy
    depends_on: [test, lint]
    exec:
      command: "npm run build && npm run deploy"
      shell: true
```

### Mapping Reference

| GitHub Actions | Nika |
|---------------|------|
| `jobs:` | `tasks:` |
| `needs: [job]` | `depends_on: [task]` |
| `run:` | `exec:` |
| `env:` | `exec: { env: {...} }` or `inputs:` |
| `strategy.matrix:` | `for_each:` |
| `${{ secrets.X }}` | `{{$env.X}}` |
| `uses: action@v1` | `exec:` or `invoke:` |

## Makefile to Nika

### Before (Makefile)

```makefile
.PHONY: build test deploy

build:
	cargo build --release

test: build
	cargo test --lib

deploy: test
	rsync -avz target/release/ server:/app/
```

### After (Nika)

```yaml
schema: nika/workflow@0.12
workflow: rust-pipeline

tasks:
  - id: build
    exec: "cargo build --release"

  - id: test
    depends_on: [build]
    exec: "cargo test --lib"

  - id: deploy
    depends_on: [test]
    exec: "rsync -avz target/release/ server:/app/"
```

## Shell Script to Nika

### Before (bash)

```bash
#!/bin/bash
DATA=$(curl -s https://api.example.com/data)
SUMMARY=$(echo "$DATA" | jq -r '.summary')
echo "Report: $SUMMARY" > report.txt
```

### After (Nika)

```yaml
schema: nika/workflow@0.12
workflow: api-report

artifacts:
  dir: ./output

tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"

  - id: extract
    depends_on: [fetch_data]
    with:
      data: $fetch_data
    exec: "echo '{{with.data}}' | jq -r '.summary'"

  - id: report
    depends_on: [extract]
    with:
      summary: $extract
    exec: "echo 'Report: {{with.summary}}'"
    artifact:
      path: report.txt
      format: text
```

### Enhanced version (using LLM instead of jq)

```yaml
schema: nika/workflow@0.12
workflow: api-report-ai
model: gpt-4.1-mini

artifacts:
  dir: ./output

tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"

  - id: summarize
    depends_on: [fetch_data]
    with:
      data: $fetch_data
    infer: "Summarize this API response in one paragraph: {{with.data}}"
    max_tokens: 200
    artifact:
      path: report.txt
      format: text
```

## Docker Compose to Nika

### Before (docker-compose)

```yaml
services:
  db:
    image: postgres
    environment:
      POSTGRES_DB: myapp
  api:
    build: .
    depends_on: [db]
    ports: ["8080:8080"]
```

### After (Nika -- orchestration only)

```yaml
schema: nika/workflow@0.12
workflow: docker-stack

tasks:
  - id: start_db
    exec: "docker compose up -d db"

  - id: wait_db
    depends_on: [start_db]
    exec: "docker compose exec db pg_isready --timeout=30"

  - id: start_api
    depends_on: [wait_db]
    exec: "docker compose up -d api"

  - id: health_check
    depends_on: [start_api]
    fetch:
      url: "http://localhost:8080/health"
    retry:
      max_attempts: 5
      delay: 3
```

## CI/CD Pipeline to Nika

### Matrix Builds

```yaml
# CI: matrix: { node: [16, 18, 20] }
# Nika equivalent:
schema: nika/workflow@0.12
workflow: matrix-test

tasks:
  - id: test
    for_each: ["16", "18", "20"]
    as: node_version
    exec:
      command: |
        nvm use {{with.node_version}}
        npm test
      shell: true
```

## Conversion Checklist

- [ ] Identify all steps/commands/jobs
- [ ] Map each to the appropriate Nika verb
- [ ] Establish `depends_on:` from original dependency graph
- [ ] Convert variables to `inputs:` or `{{$env.VAR}}`
- [ ] Convert loops/matrix to `for_each:`
- [ ] Add `artifact:` for any file outputs
- [ ] Set `schema: nika/workflow@0.12`
- [ ] Use `.nika.yaml` extension
- [ ] Validate with `nika check`
- [ ] Consider: which `exec:` steps could be enhanced with `infer:` or `fetch:`?

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| Keeping `${{ }}` GitHub syntax | Convert to `{{...}}` Nika templates |
| Ignoring dependency ordering | Map `needs:` / Make deps to `depends_on:` |
| One giant `exec:` for everything | Split into separate tasks for parallelism |
| Forgetting `.nika.yaml` extension | Must use `.nika.yaml`, not `.yaml` |
| Not adding AI-enhanced steps | Consider `infer:` for text processing, summaries |

## Validation

```bash
nika check workflow.nika.yaml
```
