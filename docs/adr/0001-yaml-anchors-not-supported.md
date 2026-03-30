# ADR-0001: YAML Anchors Not Supported

## Status: Accepted

## Context

YAML anchors (`&anchor` / `*alias`) are rejected by our parser (`marked-yaml` 0.8)
with NIKA-160. Users occasionally request this for DRY workflow definitions.

## Decision

We do NOT support YAML anchors. The workaround is `include:` for shared task blocks.

## Rationale

- `marked-yaml` provides span tracking (line:col) critical for error messages and LSP
- `saphyr` (already a dep) supports anchors but is serde-based -- no span tracking
- Switching parser = rewrite 500+ lines of Node-based parsing + span system
- `include:` with `prefix:` achieves the same DRY goal at workflow level
- Anchor bombs are a security risk; `saphyr`'s `budget.rs` mitigates but adds complexity

## Consequences

- Users use `include:` instead of anchors for shared definitions
- Error message on anchors: NIKA-160 with suggestion to use `include:`
- Parser remains `marked-yaml`-based with full span tracking for LSP and diagnostics
