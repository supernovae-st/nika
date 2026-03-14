# Advanced Nika Workflow Examples

v0.14 advanced patterns demonstrating production-grade workflow architecture.

## Examples

| File | Features Demonstrated |
|------|----------------------|
| `ci-cd-pipeline.nika.yaml` | Parallel stages, quality gates, stop conditions, deployment |
| `saga-pattern.nika.yaml` | Compensating transactions, rollback chains, checkpointing |
| `workflow-composition.nika.yaml` | `include:`, `invoke_workflow:`, modular design |
| `graceful-shutdown-demo.nika.yaml` | Checkpoints, resume, connection draining |
| `parallel-entity-generation.nika.yaml` | Nested `for_each`, decompose, MCP integration |
| `jobs-daemon-config.toml` | Cron, webhooks, file watching, intervals |

## v0.14 Features Used

### Workflow Composition
```yaml
include:
  - path: ./shared/quality-checks.nika.yaml
    prefix: quality_

invoke_workflow:
  path: "./workflows/{{use.type}}-generator.nika.yaml"
  inputs: { topic: "{{use.topic}}" }
  outputs: { content: draft_content }
```

### Jobs Daemon
```toml
[[jobs.cron]]
name = "daily-audit"
schedule = "0 6 * * *"
workflow = "workflows/audit.nika.yaml"

[[jobs.webhook]]
name = "github-push"
path = "/webhooks/github"
auth.type = "hmac-sha256"
```

### Saga Pattern
```yaml
- id: compensate_payment
  fetch: { url: "${PAYMENT_API}/refund" }
  trigger: on_rollback
  rollback_for: process_payment
```

### Graceful Shutdown
```yaml
checkpoint:
  enabled: true
  frequency: per_batch
  storage: ./.nika/checkpoints/
```

## Running Examples

```bash
# CI/CD Pipeline
nika run examples/advanced/ci-cd-pipeline.nika.yaml

# Jobs Daemon
nika jobs start --config examples/advanced/jobs-daemon-config.toml

# Resumable Batch Processing
nika run examples/advanced/graceful-shutdown-demo.nika.yaml
# Interrupt with Ctrl+C, then resume:
nika run examples/advanced/graceful-shutdown-demo.nika.yaml --resume
```

## Required Environment Variables

```bash
# CI/CD
export SLACK_WEBHOOK_URL=...
export IMAGE_NAME=...
export COMMIT_SHA=...

# Saga Pattern
export INVENTORY_API=...
export PAYMENT_API=...
export SHIPPING_API=...

# Jobs Daemon
export GITHUB_WEBHOOK_SECRET=...
export STRIPE_WEBHOOK_SECRET=...
```

## Architecture Patterns

### Nested Parallelism
```
process_entities (3 concurrent)
├── Entity A
│   └── generate_locales (5 concurrent)
│       ├── fr-FR
│       ├── en-US
│       └── ...
├── Entity B
│   └── generate_locales (5 concurrent)
└── Entity C
    └── generate_locales (5 concurrent)

Total: 3 × 5 = 15 concurrent tasks
```

### Saga Rollback
```
reserve_inventory → process_payment → create_shipment
                                            ↓ FAIL
                         ← compensate_payment ←
        ← compensate_inventory ←
```

### Checkpoint Resume
```
Batch 1 ✓ → Batch 2 ✓ → Batch 3 [INTERRUPT]
                              ↓
                        Save checkpoint
                              ↓
                        [RESUME]
                              ↓
                        Batch 3 → Batch 4 → Done
```

## Related Documentation

- [v0.14 Complete Plan](../../docs/plans/2026-02-27-v014-complete-plan.md)
- [Jobs Daemon Spec](../../docs/plans/2026-02-27-cli-dx-enhancements.md)
- [Workflow Composition](../../docs/plans/2026-02-27-v014-complete-plan.md#7-workflow-composition)
