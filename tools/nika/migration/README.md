# Nika v0.28 Migration System

Automated workspace restructure from monolithic crate to 6-crate workspace.

## Quick Start

```bash
# Review migration plan (dry run)
./migration/run-migration.sh --dry-run

# Run full migration (interactive)
./migration/run-migration.sh

# Run without confirmation prompts
./migration/run-migration.sh --no-confirm
```

## Migration Overview

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  BEFORE (v0.27)                        AFTER (v0.28)                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  tools/nika/                           tools/nika/                              │
│  ├── Cargo.toml (single crate)        ├── Cargo.toml (workspace)               │
│  └── src/                              └── crates/                              │
│      ├── ast/                              ├── nika-core/                       │
│      ├── dag/                              │   └── src/                         │
│      ├── core/                             │       ├── ast/                     │
│      ├── error.rs                          │       ├── dag/                     │
│      ├── runtime/                          │       ├── core/                    │
│      ├── binding/                          │       └── error.rs                 │
│      ├── event/                            │                                    │
│      ├── provider/                         ├── nika-runtime/                    │
│      ├── mcp/                              │   └── src/                         │
│      ├── tui/                              │       ├── runtime/                 │
│      └── main.rs                           │       ├── binding/                 │
│                                            │       └── event/                   │
│                                            │                                    │
│                                            ├── nika-provider/                   │
│                                            │   └── src/                         │
│                                            │       ├── rig.rs                   │
│                                            │       └── native/                  │
│                                            │                                    │
│                                            ├── nika-mcp/                        │
│                                            │   └── src/                         │
│                                            │       ├── client.rs                │
│                                            │       └── types.rs                 │
│                                            │                                    │
│                                            ├── nika-tui/                        │
│                                            │   └── src/                         │
│                                            │       ├── app/                     │
│                                            │       ├── views/                   │
│                                            │       └── widgets/                 │
│                                            │                                    │
│                                            └── nika-cli/                        │
│                                                └── src/                         │
│                                                    └── main.rs                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Migration Steps

| Step | Script | Description | Output |
|------|--------|-------------|--------|
| 1 | `01-setup.sh` | Create workspace structure | Workspace Cargo.toml, stub crates |
| 2 | `02-move-core.sh` | Move AST, DAG, error | nika-core crate |
| 3 | `03-move-runtime.sh` | Move runtime, binding, event | nika-runtime crate |
| 4 | `04-move-provider.sh` | Move provider modules | nika-provider crate |
| 5 | `05-move-mcp.sh` | Move MCP client | nika-mcp crate |
| 6 | `06-move-tui.sh` | Move TUI modules | nika-tui crate |
| 7 | `07-build-cli.sh` | Build CLI binary | nika-cli crate |
| 8 | `08-final-verification.sh` | Full workspace verification | Migration report |

## Safety Features

### Checkpoints
Each step creates a checkpoint before making changes:

```bash
# List all checkpoints
./migration/rollback.sh --list

# Show checkpoint details
./migration/rollback.sh --info 02-core-moved

# Rollback to checkpoint
./migration/rollback.sh --checkpoint 02-core-moved
```

### Atomic Steps
Each step is atomic:
1. Move files
2. Rewrite imports
3. Build crate
4. Run tests
5. Create checkpoint
6. Git commit

If any step fails, the migration stops.

### Import Rewriting
All imports are automatically rewritten:

| Before | After |
|--------|-------|
| `use crate::ast` | `use nika_core::ast` |
| `use crate::runtime` | `use nika_runtime::runtime` |
| `use crate::provider` | `use nika_provider` |
| `use crate::mcp` | `use nika_mcp` |
| `use crate::tui` | `use nika_tui` |

### Verification
Each step verifies:
- ✅ Crate builds without errors
- ✅ All tests pass
- ✅ No clippy warnings
- ✅ Import rewriting complete

## Usage Examples

### Full Migration
```bash
# Run full migration with confirmation
./migration/run-migration.sh

# Output:
# - 8 checkpoints created
# - 8 git commits
# - migration/MIGRATION_REPORT.md
```

### Resume from Step
```bash
# Continue from step 5 (if step 4 completed)
./migration/run-migration.sh --step 5
```

### Rollback
```bash
# Rollback to previous checkpoint
./migration/rollback.sh

# Rollback 2 steps
./migration/rollback.sh --steps 2

# Rollback to specific checkpoint
./migration/rollback.sh --checkpoint 03-runtime-moved
```

### Manual Step-by-Step
```bash
# Run each step individually
./migration/01-setup.sh
./migration/02-move-core.sh
./migration/03-move-runtime.sh
# ... etc
```

## Verification Commands

After migration completes:

```bash
# Full workspace build
cargo build --workspace --all-targets

# All tests
cargo test --workspace --lib

# Clippy
cargo clippy --workspace -- -D warnings

# Documentation
cargo doc --workspace --no-deps

# Run binary
cargo run -p nika-cli -- --version
```

## Troubleshooting

### Migration Fails at Step N
```bash
# Check logs
cat migration/migration.log | tail -n 50

# Review checkpoint
./migration/rollback.sh --info <checkpoint-name>

# Rollback and retry
./migration/rollback.sh
./migration/run-migration.sh --step N
```

### Import Rewriting Issues
All import rewrites use `sed`:
- Pattern: `use crate::X` → `use nika_Y::X`
- Files backed up with `.bak` extension
- Manual review: `find crates -name "*.rs.bak"`

### Build Failures
Each step runs `cargo build` and `cargo test`. If they fail:
1. Check compiler errors in `migration.log`
2. Review import rewrites
3. Check for missing dependencies

### Test Count Discrepancy
Final verification compares test counts:
- Baseline: 4,433 tests (v0.27)
- If count is lower, some tests may have been lost
- Review `migration/checkpoints/*/test-count.txt`

## File Structure

```
migration/
├── README.md                    # This file
├── run-migration.sh             # Master runner
├── rollback.sh                  # Rollback utility
├── 01-setup.sh                  # Step 1: Setup
├── 02-move-core.sh              # Step 2: Move core
├── 03-move-runtime.sh           # Step 3: Move runtime
├── 04-move-provider.sh          # Step 4: Move provider
├── 05-move-mcp.sh               # Step 5: Move MCP
├── 06-move-tui.sh               # Step 6: Move TUI
├── 07-build-cli.sh              # Step 7: Build CLI
├── 08-final-verification.sh     # Step 8: Verify
├── migration.log                # Execution log
├── checkpoints/                 # Checkpoint storage
│   ├── 00-initial-state/
│   ├── 01-workspace-created/
│   ├── 02-core-moved/
│   └── ... (one per step)
└── MIGRATION_REPORT.md          # Final report
```

## Expected Timeline

| Step | Duration | Cumulative |
|------|----------|------------|
| 1. Setup | 30s | 30s |
| 2. Move core | 1min | 1m 30s |
| 3. Move runtime | 1min | 2m 30s |
| 4. Move provider | 1min | 3m 30s |
| 5. Move MCP | 1min | 4m 30s |
| 6. Move TUI | 1min | 5m 30s |
| 7. Build CLI | 30s | 6m |
| 8. Final verification | 2min | 8m |

**Total:** ~8 minutes (with test execution)

## Post-Migration Checklist

After successful migration:

- [ ] Review `migration/MIGRATION_REPORT.md`
- [ ] Update `CHANGELOG.md` with v0.28 release notes
- [ ] Update `README.md` with workspace structure
- [ ] Update `CLAUDE.md` documentation
- [ ] Update `tools/nika/CLAUDE.md` with crate info
- [ ] Verify all 4,433 tests pass
- [ ] Run full clippy check (zero warnings)
- [ ] Tag release: `git tag v0.28.0`
- [ ] Push to remote: `git push origin main --tags`

## Rollback Plan

If migration needs to be aborted:

1. **Immediate rollback:**
   ```bash
   ./migration/rollback.sh
   ```

2. **Review git reflog:**
   ```bash
   git reflog | head -n 20
   ```

3. **Manual reset (last resort):**
   ```bash
   git reset --hard <commit-sha-before-migration>
   ```

## Architecture Benefits

### Before (Monolithic)
- Single 50k+ LOC crate
- 20+ minute incremental builds
- Difficult to test in isolation
- Circular dependencies possible
- No clear boundaries

### After (Workspace)
- 6 focused crates
- 5-10 minute incremental builds
- Independent crate testing
- Clear dependency hierarchy
- Enforced separation of concerns

### Dependency Graph
```
nika-cli
  ├── nika-tui
  │   ├── nika-runtime
  │   │   └── nika-core
  │   └── nika-core
  ├── nika-runtime
  │   └── nika-core
  ├── nika-provider
  │   └── nika-core
  └── nika-mcp
      └── nika-core

(nika-core has zero workspace dependencies)
```

## Support

If you encounter issues:
1. Check `migration/migration.log`
2. Review checkpoint info: `./migration/rollback.sh --info <name>`
3. Search for error codes in logs
4. Use rollback to restore previous state
5. Report issues with logs attached

## Success Metrics

Migration is successful when:
- ✅ All 6 crates build without errors
- ✅ All 4,433 tests pass
- ✅ Zero clippy warnings
- ✅ Documentation builds
- ✅ Binary runs and shows version
- ✅ 8 git commits created
- ✅ Migration report generated
