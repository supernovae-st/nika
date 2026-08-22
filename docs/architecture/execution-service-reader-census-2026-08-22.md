# Execution-service reader census · 2026-08-22

Scope: W04's production `nika run` and ARM execution paths. This census names
workflow-definition readers; it does not classify trace replay, ARM ledger
replay, output writes, or static verbs such as `nika check` as execution-world
readers.

```text
file root ── OwnedDir ───────────────┐
stdin root ─ owned bytes ─┐          │
                          ▼          ▼
                  ExecutionSnapshot capture
                  root · children · skills
                             │
                             ▼
                     ExecutionService admit
                             │
                 ┌───────────┴───────────┐
                 ▼                       ▼
              CLI run                 ARM fire
                 └── immutable context ──┘
```

| Phase | Root bytes | Child/skill bytes | Authority after return |
|---|---|---|---|
| CLI preview and diagnostics | `RunSource::capture` reads a file or stdin once | the existing static diagnostic reader may inspect composition before admission | none; this is the pre-effect parity gate |
| File/ARM admission | `ExecutionSnapshot::capture` reads through held `OwnedDir` | same held `OwnedDir`, eagerly and transitively | immutable `ExecutionSnapshot` only |
| Stdin admission | `ExecutionSnapshot::capture_root_bytes` consumes the acquired root bytes | held current-project `OwnedDir`, eagerly and transitively | immutable `ExecutionSnapshot` only |
| Runtime, pause continuation, child recursion | snapshot text | snapshot text | no filesystem reader callback or mutable pathname |

Measured structural facts:

- `nika-cli` and `nika-arm` depend on `nika-execution`; neither
  `nika-execution` nor `nika-arm` depends on `nika-cli`.
- `ProdChildRunner` contains no optional snapshot, `read_to_string`, legacy
  constructor, filesystem closure-digest walker, or path resolver.
- runtime composition requires `&AdmittedWorld`; a `world: None` lane is not
  representable.
- ARM execution contains no subprocess command, localhost bridge, or directory
  scan for a latest trace. The service-issued execution/trace identity crosses
  claim, run, receipt, and replay directly.

The reader count after `ExecutionSnapshot` capture is therefore exactly zero
for workflow definitions. Counted-source tests guard admission and execution;
CLI structural tests guard the adapter and child-runner shape.
