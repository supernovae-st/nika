# CI/CD Enhancement Plan v0.5.2

**Date:** 2026-02-20
**Version:** v0.5.1 → v0.5.2
**Focus:** Chat UX, Conversational Testing, Provider/MCP Integration

## Executive Summary

This plan enhances Nika's CI/CD pipelines to:
1. Add comprehensive conversational agent tests
2. Test all Chat UX v2 widgets and features
3. Verify all providers (Claude, OpenAI) with real API calls
4. Test MCP integrations (NovaNet, multi-MCP scenarios)
5. Create production-ready release workflow

## Current State Analysis

### Existing CI Jobs (ci.yml)
- ✅ check, fmt, clippy, deny (quality gates)
- ✅ test, coverage (unit tests)
- ✅ build (release binary)
- ✅ integration (Neo4j + NovaNet MCP)
- ✅ llm-integration (OpenAI, Anthropic)
- ✅ mvp8-verification (5 phases)

### Gaps Identified
- ❌ No TUI widget tests in CI
- ❌ No Chat UX feature tests
- ❌ No conversational agent flow tests
- ❌ No multi-MCP coordination tests
- ❌ No extended thinking verification in CI
- ❌ No benchmark regression tests

## Phase 1: New Test Suites

### 1.1 Conversational Agent Tests (`tests/conversational_agent_test.rs`)

Test multi-turn agent conversations with:
- Tool calling sequences
- Context accumulation
- Spawn agent nesting
- Error recovery flows

```rust
#[tokio::test]
async fn test_agent_multi_turn_conversation()
#[tokio::test]
async fn test_agent_tool_calling_sequence()
#[tokio::test]
async fn test_agent_context_accumulation()
#[tokio::test]
async fn test_spawn_agent_nesting()
```

### 1.2 Chat UX Widget Tests (`tests/chat_ux_test.rs`)

Test all new Chat UX v2 widgets:
- SessionContextBar rendering
- ActivityStack hot/warm/cold states
- CommandPalette keyboard navigation
- InferStreamBox streaming display
- McpCallBox status transitions

### 1.3 Provider Integration Tests (`tests/provider_integration_test.rs`)

Test real provider calls (with API keys):
- Claude with extended thinking
- Claude without extended thinking
- OpenAI GPT-4
- OpenAI GPT-3.5
- Provider auto-detection

### 1.4 MCP Integration Tests (`tests/mcp_integration_test.rs`)

Test MCP scenarios:
- Single MCP (NovaNet)
- Multi-MCP coordination
- MCP tool discovery
- MCP error handling
- NovaNet introspection

## Phase 2: Example Workflows

### 2.1 Provider Test Workflows
- `examples/test-claude-extended-thinking.nika.yaml`
- `examples/test-openai-gpt4.nika.yaml`
- `examples/test-provider-auto-detect.nika.yaml`

### 2.2 MCP Test Workflows
- `examples/test-multi-mcp-agent.nika.yaml` (exists, enhance)
- `examples/test-novanet-introspect.nika.yaml`
- `examples/test-mcp-error-recovery.nika.yaml`

### 2.3 Conversational Workflows
- `examples/test-conversational-agent.nika.yaml`
- `examples/test-spawn-agent-chain.nika.yaml`

## Phase 3: GitHub Actions Enhancements

### 3.1 New Workflow: chat-ux.yml
```yaml
name: Chat UX Tests
on: [push, pull_request]
jobs:
  tui-tests:
    - Run TUI widget tests
    - Test Chat UX components
    - Verify keyboard handling
```

### 3.2 Enhanced ci.yml
- Add TUI test job
- Add benchmark regression job
- Add conversational test job

### 3.3 Enhanced release.yml
- Add Chat UX verification
- Add conversational agent verification
- Generate changelog from commits

## Phase 4: Release Process

### 4.1 Version Bump
- Update Cargo.toml: 0.5.1 → 0.5.2
- Update CHANGELOG.md
- Update CLAUDE.md version references

### 4.2 Release Notes
```
## v0.5.2 - Chat UX Enhancement

### New Features
- Chat UX v2 with SessionContextBar
- ActivityStack (hot/warm/cold)
- CommandPalette (⌘K)
- InferStreamBox (streaming display)
- McpCallBox (inline MCP calls)

### Testing
- Conversational agent tests
- Chat UX widget tests
- Provider integration tests
- MCP integration tests

### CI/CD
- chat-ux.yml workflow
- Enhanced test coverage
- Benchmark regression checks
```

## Implementation Order

1. **Tests First** (TDD)
   - Write conversational_agent_test.rs
   - Write chat_ux_test.rs
   - Write provider_integration_test.rs
   - Write mcp_integration_test.rs

2. **Example Workflows**
   - Create test workflows
   - Validate with `cargo run -- validate`

3. **CI/CD Updates**
   - Create chat-ux.yml
   - Update ci.yml
   - Update release.yml

4. **Release**
   - Version bump
   - Commit with conventional format
   - Push and tag v0.5.2
   - Create GitHub release

## Success Criteria

- [ ] All 1028+ tests pass
- [ ] New test files: 4 (conversational, chat_ux, provider, mcp)
- [ ] New workflows: 5+ example .nika.yaml files
- [ ] New CI job: chat-ux.yml
- [ ] Version: v0.5.2 tagged
- [ ] Release: GitHub release created

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| API rate limits | Use mock providers in CI, real APIs only on release |
| Neo4j unavailable | Skip integration tests gracefully |
| TUI tests fail headless | Use virtual terminal buffer |

---

**Author:** Claude Opus 4.5
**Co-Author:** Thibaut @ SuperNovae Studio
