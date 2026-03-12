# v0.27 Implementation Plan: Stub Commands → Full Features

**Status**: In Progress
**Session**: Claude Code B (parallel to Phase 1 Core Types work)
**Focus**: daemon, sync, setup, backup modules

---

## Context

Another Claude session is working on Phase 1 Core Types:
- `src/core/paths.rs` ✅ Complete (606 lines, 25 tests)
- `src/core/migration.rs` ✅ In progress
- `src/registry/` updates in progress
- `src/secrets/` updates in progress

This session focuses on **non-overlapping modules**:

---

## Module Implementation Order

### 1. Daemon Module (`src/daemon/`)

**Purpose**: Unix socket server for keychain IPC, background services

```
src/daemon/
├── mod.rs           # Re-exports + DaemonHandle
├── server.rs        # UnixListener + JSON protocol
├── client.rs        # Client for connecting to daemon
├── protocol.rs      # Request/Response types
└── pid.rs           # PID file management with flock()
```

**Key Types**:
- `DaemonServer` - Async Unix socket server
- `DaemonClient` - Connect to running daemon
- `DaemonRequest` - Commands (GetSecret, Ping, Shutdown, etc.)
- `DaemonResponse` - Responses (Secret, Pong, Error, etc.)
- `DaemonHandle` - Start/stop management

**Tests**:
- `test_daemon_start_stop`
- `test_daemon_pid_file_locking`
- `test_daemon_request_response`
- `test_daemon_concurrent_clients`

### 2. Sync Module (`src/sync/`)

**Purpose**: Synchronize MCP config to Claude Code

```
src/sync/
├── mod.rs           # Re-exports
├── editor.rs        # EditorKind enum
├── claude_code.rs   # Claude Code config sync
└── merge.rs         # MCP config merging logic
```

**Key Types**:
- `EditorKind` - Enum: ClaudeCode (more later)
- `SyncConfig` - What to sync, where
- `SyncResult` - Success/failure with details

**Tests**:
- `test_sync_claude_code_config`
- `test_sync_merge_preserves_user_servers`
- `test_sync_creates_backup`

### 3. Setup Module (`src/setup/`)

**Purpose**: Interactive onboarding wizard

```
src/setup/
├── mod.rs           # Re-exports
├── wizard.rs        # Interactive prompts
├── nika.rs          # Nika-specific setup
└── doctor.rs        # Health check diagnostics
```

**Key Types**:
- `SetupWizard` - Interactive flow
- `SetupStep` - Enum of wizard steps
- `DoctorResult` - Health check results

**Tests**:
- `test_setup_creates_nika_home`
- `test_doctor_detects_missing_providers`

### 4. Backup Module (`src/backup/`)

**Purpose**: Create/restore backups of Nika config

```
src/backup/
├── mod.rs           # Re-exports
├── archive.rs       # tar.gz creation
├── restore.rs       # Restore from backup
└── prune.rs         # Cleanup old backups
```

**Key Types**:
- `BackupArchive` - Metadata + contents
- `BackupManifest` - What's included
- `RestoreOptions` - Selective restore

**Tests**:
- `test_backup_create_and_list`
- `test_backup_restore`
- `test_backup_prune_old`

---

## TDD Approach

For each module:

1. **Write failing test first**
2. **Implement minimal code to pass**
3. **Refactor while tests pass**
4. **Add integration tests**

---

## Dependencies

```toml
# Already in Cargo.toml
tokio = { version = "1.43", features = ["full", "net", "signal"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# May need to add
flate2 = "1.0"       # For backup compression
tar = "0.4"          # For backup archives
```

---

## Timeline

| Day | Module | Deliverables |
|-----|--------|--------------|
| 1 | daemon | Protocol types, server, client |
| 2 | sync | Claude Code sync, merge logic |
| 3 | setup | Wizard, doctor |
| 4 | backup | Archive, restore, prune |
| 5 | Integration | Wire to main.rs, E2E tests |

---

## Non-Goals (This Session)

- ❌ Touch `src/core/` (other session)
- ❌ Touch `src/registry/` (other session)
- ❌ Touch `src/secrets/` (other session)
- ❌ Modify `main.rs` CLI wiring (wait for other session)

---

## Coordination Point

When both sessions complete:
1. Other session commits Phase 1 Core Types
2. This session commits daemon, sync, setup, backup
3. Final session wires everything in main.rs
