# v0.49 Session Plan — Display v3 + Remaining Cleanup

## Scope

### A. Display v3 (4 phases, 17 tasks)

#### Phase 1: Renderer Trait Extraction
1. Define `Renderer` trait with 7 methods (set_task_layers, init_tasks, last_rendered_id, render_kind, render_new_events, render_summary, render_quiet_summary, stats)
2. Replace `RunRenderer` enum dispatch with `Box<dyn Renderer>` + 3 factory functions
3. Add `TestRenderer` for mocking in tests

#### Phase 2: Wire 5 Dead Event Emissions
4. Thread EventLog through binding/resolve.rs
5. Emit BindingDefaultApplied, BindingTransformApplied, BindingEnvResolved + NativeModelLoaded
6. Add MediaCleanup GC stub in cas.rs

#### Phase 3: Testability + LiveRenderer Tests
7. Refactor summary.rs — extract format_* pure functions from print_* wrappers
8. Add 20 summary tests (insta snapshots)
9. Add LiveRenderer event rendering tests via hidden()
10. Add format_output_preview tests

#### Phase 4: CLI UX Polish (indicatif advanced)
11. Live {elapsed} + {prefix} split on task bars
12. Overall bar: {wide_bar} + {eta} + {percent} + cost key
13. Agent turn progress bar (red mini bar)
14. Dynamic for_each sub-bars
15. StreamingDelta event + live token counter
16. NikaError::suggested_fix() method
17. Defensive: ProgressFinish::Abandon + terminal resize

### B. Daemon L1-L4 (DEFERRED to v0.49)
- L1: Persistent DaemonClient connections (for TUI perf)
- L2: Server pipelining (multiple requests per connection)
- L3: DaemonError sub-types (StorageError, WatchError)
- L4: DaemonRequest::Shutdown IPC

### C. Discovery Leftovers
- M1 (rxing 108s): External dep, can't fix — accept
- M3 (jsonschema feature-gate): Low ROI, defer
- M12 (lower_action clones): Profile first, may already be optimal

### D. model: required for LLM verbs
- Make `model:` field required for infer/agent verbs (currently defaults)
- ~81 workflow files + AST analyzer + course/showcase generators affected
- Separate session recommended (high blast radius)

## Priority Order
1. Display v3 Phase 1 (Renderer trait) — enables all subsequent phases
2. Display v3 Phase 3 (Testability) — highest value, independent
3. Display v3 Phase 2 (Dead events) — medium value
4. Display v3 Phase 4 (UX polish) — nice-to-have, can be incremental

## Testing
```bash
cargo test --workspace --lib          # 8,220+ tests
cargo test -p nika-engine --lib -- display  # Display tests
cargo clippy --workspace -- -D warnings     # Zero warnings
```
