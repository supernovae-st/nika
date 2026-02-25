# Pull Request

## Summary

<!-- Brief description of what this PR does -->

## Related Issue

<!-- Fixes #(issue number) -->

## Type of Change

- [ ] 🐛 Bug fix (non-breaking change fixing an issue)
- [ ] ✨ New feature (non-breaking change adding functionality)
- [ ] 💥 Breaking change (fix or feature causing existing functionality to change)
- [ ] 📚 Documentation update
- [ ] 🔧 Refactoring (no functional changes)
- [ ] 🧪 Test improvements

## Changes Made

- Change 1
- Change 2

## Quality Gates Checklist

### Required (FORTRESS enforced)

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test` passes (all 2,793+ tests)
- [ ] Version remains 0.x.x (see [Version Lock Policy](../docs/plans/2025-02-25-nika-fortress-design.md))

### Code Quality

- [ ] My code follows the project's style guidelines
- [ ] I have performed a self-review of my code
- [ ] I have added tests that prove my fix/feature works
- [ ] New and existing tests pass locally with my changes
- [ ] My changes generate no new warnings
- [ ] No new `unsafe` code without justification

### Documentation

- [ ] I have updated the documentation accordingly
- [ ] I have commented my code where necessary
- [ ] CHANGELOG.md updated (if applicable)

## Screenshots (if applicable)

<!-- Add screenshots for TUI/UI changes -->

## Additional Notes

<!-- Any additional context for reviewers -->

---

**⚠️ VERSION LOCK:** Nika follows the **0.x.x forever** policy. PRs bumping to 1.0.0+ will be automatically rejected by CI.
