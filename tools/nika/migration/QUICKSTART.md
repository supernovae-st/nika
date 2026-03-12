# Nika v0.28 Migration Quick Start

Run this migration in **8 minutes** to restructure Nika from monolithic crate to 6-crate workspace.

## Prerequisites

```bash
# 1. Check you're in the right place
cd /Users/thibaut/dev/supernovae/nika/tools/nika
pwd  # Should show: .../nika/tools/nika

# 2. Ensure git is clean
git status
# Output should be: "nothing to commit, working tree clean"

# 3. Verify GNU sed is installed (macOS only)
sed --version | grep GNU
# If not found: brew install gnu-sed

# 4. Verify baseline
cargo test --lib --quiet
# Should see: "test result: ok. 4433 passed"
```

## Run Migration

### Option 1: Full Automated (Recommended)

```bash
# Review the plan
./migration/run-migration.sh --dry-run

# Execute migration (interactive with confirmation)
./migration/run-migration.sh

# Non-interactive (skip confirmation)
./migration/run-migration.sh --no-confirm
```

**Duration:** ~8 minutes

### Option 2: Step-by-Step

```bash
# Run each step manually
./migration/01-setup.sh               # 30s - Create workspace
./migration/02-move-core.sh           # 1m  - Move AST/DAG/error
./migration/03-move-runtime.sh        # 1m  - Move executor/binding/event
./migration/04-move-provider.sh       # 1m  - Move LLM providers
./migration/05-move-mcp.sh            # 1m  - Move MCP client
./migration/06-move-tui.sh            # 1m  - Move TUI
./migration/07-build-cli.sh           # 30s - Build binary
./migration/08-final-verification.sh  # 2m  - Verify everything
```

## What Happens

Each step:
1. Creates checkpoint (rollback point)
2. Moves files to new crate
3. Rewrites imports automatically
4. Builds and tests the crate
5. Creates git commit

Progress output:
```
[INFO] Starting Step 1: Setup workspace structure...
[SUCCESS] Workspace structure created
[SUCCESS] Step 1 complete!

[INFO] Starting Step 2: Move nika-core modules...
[INFO] Moving: src/ast/ → crates/nika-core/src/ast/
[INFO] Rewriting imports in: crates/nika-core/src/ast/parser.rs
[SUCCESS] nika-core builds successfully
[SUCCESS] nika-core tests pass
[SUCCESS] Step 2 complete!

... (6 more steps)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  MIGRATION COMPLETE! 🦋
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

## Verify Success

```bash
# All checks should pass
cargo build --workspace --all-targets  # ✅ Builds
cargo test --workspace --lib           # ✅ 4,433 tests pass
cargo clippy --workspace -- -D warnings # ✅ Zero warnings
cargo run -p nika-cli -- --version     # ✅ Shows: nika 0.28.0

# Review migration report
cat migration/MIGRATION_REPORT.md
```

## If Something Goes Wrong

### Rollback

```bash
# Rollback to previous checkpoint
./migration/rollback.sh

# Or rollback to specific checkpoint
./migration/rollback.sh --checkpoint 03-runtime-moved

# List available checkpoints
./migration/rollback.sh --list
```

### Resume

```bash
# Continue from step N (e.g., step 5)
./migration/run-migration.sh --step 5
```

### Debug

```bash
# View logs
tail -n 100 migration/migration.log

# Check checkpoint status
./migration/rollback.sh --info 04-provider-moved

# Verify git state
git status
git log --oneline | head -n 10
```

## Expected Output Structure

After successful migration:

```
tools/nika/
├── Cargo.toml (workspace)
├── migration/
│   ├── MIGRATION_REPORT.md
│   ├── migration.log
│   └── checkpoints/ (8 checkpoints)
└── crates/
    ├── nika-core/
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── ast/
    │       ├── dag/
    │       ├── core/
    │       └── error.rs
    ├── nika-runtime/
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── runtime/
    │       ├── binding/
    │       └── event/
    ├── nika-provider/
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── rig.rs
    │       └── native/
    ├── nika-mcp/
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── client.rs
    │       └── types.rs
    ├── nika-tui/
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── app/
    │       ├── views/
    │       └── widgets/
    └── nika-cli/
        ├── Cargo.toml
        └── src/
            └── main.rs
```

## Git History

8 granular commits:

```bash
git log --oneline --grep="refactor(v0.28)"
```

Output:
```
a1b2c3d refactor(v0.28): final verification complete
b2c3d4e refactor(v0.28): build nika-cli binary crate
c3d4e5f refactor(v0.28): move TUI modules to nika-tui crate
d4e5f6g refactor(v0.28): move MCP modules to nika-mcp crate
e5f6g7h refactor(v0.28): move provider modules to nika-provider crate
f6g7h8i refactor(v0.28): move runtime modules to nika-runtime crate
g7h8i9j refactor(v0.28): move core modules to nika-core crate
h8i9j0k refactor(v0.28): initialize 6-crate workspace structure
```

## Next Steps

After migration completes:

1. **Review report:**
   ```bash
   cat migration/MIGRATION_REPORT.md
   ```

2. **Update documentation:**
   - `CHANGELOG.md` - Add v0.28 release notes
   - `README.md` - Update with workspace structure
   - `CLAUDE.md` - Update architecture section

3. **Tag release:**
   ```bash
   git tag v0.28.0
   git push origin main --tags
   ```

4. **Publish (if applicable):**
   ```bash
   cargo publish -p nika-core
   cargo publish -p nika-runtime
   cargo publish -p nika-provider
   cargo publish -p nika-mcp
   cargo publish -p nika-tui
   cargo publish -p nika-cli
   ```

## Troubleshooting

### "GNU sed not found"
**macOS:**
```bash
brew install gnu-sed
export PATH="/usr/local/opt/gnu-sed/libexec/gnubin:$PATH"
```

### "Working directory not clean"
```bash
git status
git add -A && git commit -m "WIP: before migration"
# or
git stash
```

### "Tests fail during migration"
```bash
# Rollback to last good state
./migration/rollback.sh

# Check what changed
git diff

# Fix issues and retry
./migration/run-migration.sh --step <failed-step>
```

### "Build fails in step N"
```bash
# Check logs
tail -n 50 migration/migration.log

# Review checkpoint
./migration/rollback.sh --info <checkpoint-name>

# Rollback and investigate
./migration/rollback.sh --checkpoint <checkpoint-before-failure>
```

## FAQ

**Q: How long does it take?**
A: ~8 minutes fully automated.

**Q: Can I stop and resume?**
A: Yes! Each step is atomic. Use `--step N` to resume.

**Q: What if I need to rollback?**
A: Run `./migration/rollback.sh`. All checkpoints are preserved.

**Q: Will my tests still pass?**
A: Yes. All 4,433 tests are verified at each step.

**Q: Can I review changes before committing?**
A: Yes. Each step commits, but you can review with `git show HEAD`.

**Q: What about my local changes?**
A: Commit or stash them first. Migration requires clean working directory.

## Support

If you encounter issues:

1. Check `migration/migration.log` for errors
2. Use `./migration/rollback.sh --list` to see checkpoints
3. Review `migration/DESIGN.md` for technical details
4. Check git reflog: `git reflog | head -n 20`

## Summary

```bash
# Quick commands
./migration/run-migration.sh           # Run migration
./migration/rollback.sh                # Rollback if needed
cat migration/MIGRATION_REPORT.md     # View results
```

That's it! The migration is designed to be **safe**, **automated**, and **reversible**.

🦋 Good luck with your migration!
