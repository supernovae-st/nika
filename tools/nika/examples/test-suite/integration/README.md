# Integration Test Suite

This directory contains integration test workflows for testing Nika↔NovaNet connectivity.

## Prerequisites

```bash
# Start test infrastructure
docker compose -f docker-compose.test.yml --profile integration up -d

# Wait for Neo4j to be healthy
docker compose -f docker-compose.test.yml --profile integration ps

# For e2e tests, set your API key
export ANTHROPIC_API_KEY=sk-ant-...
```

## Test Workflows

| File | Description | Profile |
|------|-------------|---------|
| `01-mcp-connection.nika.yaml` | Basic MCP connectivity test | integration |
| `02-entity-traversal.nika.yaml` | Graph search and traversal | integration |
| `03-content-generation.nika.yaml` | Full content pipeline | e2e |
| `04-all-verbs.nika.yaml` | All 5 verbs integration | e2e |
| `05-parallel-locales.nika.yaml` | for_each parallelism | e2e |

## Running Tests

### Individual Workflow

```bash
# Basic connectivity (no LLM needed)
nika examples/test-suite/integration/01-mcp-connection.nika.yaml

# With LLM (requires API key)
ANTHROPIC_API_KEY=sk-ant-... nika examples/test-suite/integration/03-content-generation.nika.yaml
```

### All Integration Tests

```bash
# Run all via docker-compose
docker compose -f docker-compose.test.yml --profile integration up --abort-on-container-exit
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `NOVANET_MCP_NEO4J_URI` | Neo4j Bolt URI | `bolt://localhost:7687` |
| `NOVANET_MCP_NEO4J_USER` | Neo4j username | `neo4j` |
| `NOVANET_MCP_NEO4J_PASSWORD` | Neo4j password | `novanetpassword` |
| `ANTHROPIC_API_KEY` | Claude API key | - |
| `OPENAI_API_KEY` | OpenAI API key | - |

## Cleanup

```bash
# Stop and remove containers
docker compose -f docker-compose.test.yml --profile integration down

# Remove volumes too
docker compose -f docker-compose.test.yml --profile integration down -v
```
