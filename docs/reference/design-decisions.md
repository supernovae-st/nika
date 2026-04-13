# 12 — Design Decisions

> Key architectural decisions and their rationale: why YAML, why 5 verbs, why DAG, why CAS, why three-phase AST, why AGPL.

## Decision 1: YAML as the Workflow Language

**Choice**: YAML (`.nika.yaml` extension) over JSON, TOML, custom DSL, or Python SDK.

**Rationale**:
- **Readability**: YAML is the most human-readable structured format. AI workflows are fundamentally about prompts (natural language), and YAML preserves multiline strings naturally with `|` and `>` scalars.
- **Familiarity**: YAML is the lingua franca of DevOps (Kubernetes, GitHub Actions, Docker Compose, Ansible). Developers already know it.
- **Tooling**: Existing YAML support in every editor. The LSP builds on tree-sitter-yaml for error-recovery parsing.
- **Declarative**: Workflows should describe *what* to do, not *how* to do it. YAML's declarative nature matches this philosophy.

**Tradeoffs accepted**:
- YAML's indentation sensitivity can be confusing (mitigated by the LSP diagnostics)
- No standard schema validation (mitigated by the three-phase pipeline)
- YAML bombs are possible (mitigated by `from_str_with_budget()`)

## Decision 2: Exactly Five Verbs

**Choice**: `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:` -- no more, no fewer.

**Rationale**: The five verbs cover all AI workflow primitives:

| Verb | Primitive | External System |
|------|-----------|----------------|
| `infer:` | Text generation | LLM provider |
| `exec:` | Command execution | Operating system |
| `fetch:` | Data retrieval | HTTP servers |
| `invoke:` | Tool calling | MCP servers |
| `agent:` | Autonomous reasoning | LLM + tools |

Any conceivable AI workflow task can be expressed as a composition of these five primitives. Adding a sixth verb would create overlap; removing one would create gaps.

**Design constraint**: Each verb is a single action with a single return type (string). This simplicity enables uniform DAG wiring -- every task produces a string that downstream tasks can consume via `with:` bindings.

## Decision 3: DAG-Based Execution

**Choice**: Directed Acyclic Graph (DAG) over linear execution, event-driven, or actor model.

**Rationale**:
- **Automatic parallelism**: Tasks without dependencies run concurrently without explicit concurrency annotations
- **Deterministic ordering**: DAG topology guarantees that dependencies are always satisfied before a task runs
- **Visualization**: DAGs are naturally visualizable (the TUI renders them in the Studio view)
- **Cycle detection**: Impossible to create infinite loops at the structural level

**Tradeoffs accepted**:
- DAGs cannot express feedback loops (mitigated by `agent:` which has its own internal loop)
- Dynamic DAG modification at runtime is not supported (mitigated by `decompose:` for dynamic sub-task expansion)
- The DAG is immutable after construction (by design, for thread safety)

## Decision 4: Three-Phase AST Pipeline

**Choice**: Raw -> Analyzed -> Lowered, with strict phase separation.

**Rationale**: See [02-ast-three-phase-pipeline.md](02-ast-three-phase-pipeline.md) for the detailed treatment. The key reasons:

1. **Phase 1 (Parse)** captures source locations. Without spans, error messages are useless in IDEs.
2. **Phase 2 (Analyze)** validates before executing. Catching a typo before an expensive LLM call is worth the analysis time.
3. **Phase 3 (Lower)** bridges types. The zero-I/O core uses `TaskId(u32)` for efficiency; the runtime uses `Arc<str>` for concurrent sharing.

**Why not two phases?** Combining Parse and Analyze would require parsing to know about semantic constraints (valid provider names, cycle detection). Combining Analyze and Lower would require the zero-I/O core to depend on runtime types (tokio, reqwest).

## Decision 5: Zero-I/O Core

**Choice**: `nika-core` performs no I/O whatsoever.

**Rationale**:
- **Fast compilation**: No async runtime, no HTTP client, no file system operations
- **Embeddability**: Can be used in WebAssembly or other constrained environments
- **Testing**: Pure functions are trivially testable (proptest, insta snapshots)
- **LSP sharing**: The same analysis code powers both the embedded and standalone LSP without importing the engine

**Enforcement**: `nika-core`'s `Cargo.toml` has no dependencies on `tokio`, `reqwest`, `rmcp`, or any I/O library. The dependency list is exclusively parsing and data-structure crates.

## Decision 6: Content-Addressable Storage (CAS)

**Choice**: Blake3-hashed CAS for all binary data (images, PDFs, audio, video).

**Rationale**:
- **Deduplication**: Identical files are stored once regardless of how many tasks reference them
- **Immutability**: Once stored, a blob never changes (the hash guarantees content integrity)
- **Addressability**: Any task can reference any blob by hash, eliminating path management
- **Concurrency**: Atomic writes via `O_EXCL` prevent race conditions on concurrent stores
- **GC safety**: The lockfile guard prevents GC from collecting blobs during execution

**Why blake3 over SHA-256?** blake3 is 3-5x faster than SHA-256 on modern CPUs and supports parallelism via SIMD. The hash prefix (`blake3:`) enables algorithm migration in the future.

## Decision 7: Event Sourcing

**Choice**: Full event sourcing with 41 event variants for workflow observability.

**Rationale**:
- **Audit trail**: Every action is recorded with a timestamp and sequence ID
- **Replay**: NDJSON trace files can be replayed for debugging
- **TUI integration**: The broadcast channel enables real-time event streaming to the UI
- **Testing**: `NoopEmitter` provides zero-overhead event suppression in tests

**Why not just logging?** Structured events (serializable, typed) are more useful than unstructured log lines. Events can be queried, filtered, aggregated, and visualized. Log lines cannot.

## Decision 8: rig-core for LLM Providers

**Choice**: Delegate all LLM interactions to rig-core rather than maintaining individual API clients.

**Rationale**:
- **Multi-provider**: rig-core supports Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI out of the box
- **Tool calling**: rig-core's `ToolDyn` trait provides a clean abstraction for MCP tool integration
- **Streaming**: rig-core handles streaming across providers with a unified API
- **Maintenance**: Provider API changes are handled upstream by the rig-core team

**Tradeoffs accepted**:
- rmcp version conflict (rig-core bundles rmcp 0.13 internally; Nika uses rmcp 0.16). Resolved via `NikaMcpTool` wrapper.
- rig-core's stop_sequences support is incomplete. Resolved via `additional_params` workaround.

## Decision 9: rmcp for MCP Protocol

**Choice**: Use the official rmcp SDK (0.16) for MCP protocol implementation.

**Rationale**:
- **Official SDK**: rmcp is the Rust reference implementation of MCP
- **Transport support**: stdio and SSE transports out of the box
- **Protocol compliance**: Guaranteed compatibility with MCP server implementations

## Decision 10: AGPL-3.0-or-later License

**Choice**: AGPL over MIT, Apache-2.0, or other permissive licenses.

**Rationale**: Protects open source from cloud exploitation. Any company running Nika as a service must release their modifications. This aligns with the project's values:

> "All Nika crates must use AGPL-3.0-or-later, not MIT. Protects open source from cloud exploitation." -- project memory

The AGPL copyleft extends to network use (Section 13), meaning SaaS providers cannot use Nika without contributing back.

## Decision 11: Immutable DAGs

**Choice**: DAGs are immutable after construction.

**Rationale**:
- **Thread safety**: The DAG is shared across tokio tasks via `Arc`. Immutability means no locks are needed.
- **Reasoning**: The execution engine can reason about the DAG structure without worrying about concurrent modification.
- **Visualization**: The TUI can render the DAG without synchronization concerns.

**Tradeoff**: `for_each` expansion and `decompose` create synthetic tasks at runtime using indexed IDs (e.g., `"task[0]"`, `"task[1]"`), but these are handled in the runner, not in the DAG itself.

## Decision 12: SmallVec for Dependencies

**Choice**: `SmallVec<[T; 4]>` for dependency lists instead of `Vec<T>`.

**Rationale**: Profiling showed that 95% of workflow tasks have 0-4 dependencies. SmallVec stores up to 4 elements on the stack, avoiding heap allocation in the common case. For the 5% of tasks with more dependencies, it seamlessly falls back to heap allocation.

This optimization is applied in:
- `dag/flow.rs`: `SmallVec<[Arc<str>; 4]>`
- `dag/indexed.rs`: `SmallVec<[TaskId; 4]>`

## Decision 13: with: Bindings over Implicit Data Flow

**Choice**: Explicit `with:` declarations for data binding rather than automatic task output forwarding.

**Rationale**:
- **Clarity**: Every data dependency is visible in the YAML
- **Transforms**: Pipe chains (`| upper | trim`) are only possible with explicit bindings
- **Defaults**: The `??` fallback operator requires knowing the binding exists
- **Type safety**: Environment, context, and input bindings are distinguished from task references
- **DAG edges**: The analyzer extracts implicit dependencies from `with:` bindings, ensuring the DAG is correct

**Alternative considered**: GitHub Actions-style `${{ steps.step1.outputs.result }}`. Rejected because it mixes binding declaration and usage, making it harder to detect invalid references at analysis time.

## Decision 14: Separate TUI Crate

**Choice**: The TUI is a separate, optional crate (`nika-tui`) feature-gated on the binary.

**Rationale**:
- **Binary size**: Without the TUI, the binary is significantly smaller (no ratatui, crossterm, git2, openssl, tree-sitter)
- **Compile time**: TUI changes do not trigger engine recompilation
- **Embeddability**: The engine can be used as a library without TUI dependencies
- **CI/CD**: Server environments (no terminal) can skip TUI compilation entirely

## Decision 15: Schema Versioning

**Choice**: `schema: nika/workflow@0.12` with explicit version numbering and feature gates.

**Rationale**:
- **Backward compatibility**: Each schema version defines which features are available
- **Migration hints**: `SchemaVersion::migration_hint()` guides users to upgrade
- **Feature gates**: `supports_mcp()`, `supports_skills()`, `supports_with()` enable gradual feature adoption
- **Zero users policy**: With zero production users, only `@0.12` matters. No backward compatibility burden.
