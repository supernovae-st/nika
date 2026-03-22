# Mega Audit Plan — v0.38.0 Post-Release

> Execute in TOTAL AUTONOMY. Commit + push after each phase. Fix bugs immediately.

## Phase A: Dead Code Cleanup [IN PROGRESS - agent running]
- [x] Delete micro.rs (1,692 lines zombie widgets)
- [x] Fix PathBoundaryError.base_path dead field
- [x] Gate MediaType behind #[cfg(test)]
- [x] Remove 3 unused media error helpers
- [x] Delete nika-v0.15.0/ legacy directory

## Phase B: Stale Doc Comments
- [ ] Update all `use nika::` in doc comments to `use nika_engine::` (nika-mcp, nika-tui)
- [ ] Update stale version refs in doc comments

## Phase C: Real Provider E2E Tests
- [ ] OpenAI: full workflow with all verbs + structured output + artifacts
- [ ] xAI: test with grok model (when key available)
- [ ] Gemini: test with gemini model (when key available)
- [ ] Multi-provider: workflow using 2+ providers
- [ ] Verify all outputs are user-comprehensible

## Phase D: All Transforms Exhaustive Test
- [ ] Test ALL 27 transforms with real data
- [ ] Verify each produces correct output

## Phase E: Final Verification
- [ ] Full workspace cargo test
- [ ] Full clippy
- [ ] Build release binary
- [ ] Run 5+ real workflows end-to-end
- [ ] Verify `nika doctor` clean
- [ ] Verify `cargo install nika` works from crates.io

## Success Criteria
- Zero compilation errors
- Zero clippy warnings
- All e2e tests pass with real providers
- All workflow outputs are comprehensible
- All commits pushed to main
