---
name: nika-deep-verify
description: Launch 6 parallel Haiku agents for comprehensive project verification
---

# Nika Deep Verify

Comprehensive multi-agent verification system for the Nika project.

## Overview

This command launches 6 specialized Haiku subagents in parallel to thoroughly verify:
- Specification quality and completeness
- Code implementation alignment
- .claude/ structure and configuration
- Rust conventions and best practices
- Documentation consistency
- Business logic correctness

## Execution Protocol

### Phase 1: Parallel Agent Launch

Launch ALL 6 verification agents simultaneously using the Task tool with `model: haiku`:

```
IMPORTANT: Launch all 6 agents in a SINGLE message with 6 parallel Task tool calls.
Each agent runs independently and returns a validation report.
```

**Agents to launch in parallel:**

1. **verify-spec** (spec-validator)
   - Prompt: "You are the spec-validator agent. Read spec/SPEC.md and validate its structure, completeness, and consistency. Follow the checklist in your agent definition. Return a detailed validation report with score X/10."

2. **verify-code** (code-validator)
   - Prompt: "You are the code-validator agent. Analyze src/ and verify it implements spec/SPEC.md correctly. Check action implementations, error codes, and type safety. Return a detailed validation report with score X/10."

3. **verify-claude-structure** (claude-validator)
   - Prompt: "You are the claude-structure-validator agent. Analyze .claude/ directory structure. Verify settings.json, hooks, skills, agents, and commands are valid. Return a detailed validation report with score X/10."

4. **verify-rust-conventions** (rust-conventions)
   - Prompt: "You are the rust-conventions-validator agent. Analyze src/ for Rust best practices: error handling, ownership, type system, API design. Return a detailed validation report with score X/10."

5. **verify-docs** (docs-validator)
   - Prompt: "You are the docs-validator agent. Verify CLAUDE.md, README.md, and code comments are aligned and accurate. Check version consistency. Return a detailed validation report with score X/10."

6. **verify-logic** (logic-validator)
   - Prompt: "You are the logic-validator agent. Trace spec requirements to code implementation. Verify business logic is correctly implemented. Return a detailed validation report with score X/10."

### Phase 2: Results Aggregation

After all agents complete, aggregate results:

```markdown
# Nika Deep Verify Report

**Timestamp:** YYYY-MM-DD HH:MM:SS
**Overall Status:** PASS | WARN | FAIL
**Overall Score:** X/60 (sum of all agent scores)

## Agent Reports

### 1. Spec Validation (X/10)
[Agent report content]

### 2. Code Validation (X/10)
[Agent report content]

### 3. Claude Structure (X/10)
[Agent report content]

### 4. Rust Conventions (X/10)
[Agent report content]

### 5. Documentation (X/10)
[Agent report content]

### 6. Logic Validation (X/10)
[Agent report content]

## Critical Issues (must fix)
- Issue 1
- Issue 2

## Warnings (should fix)
- Warning 1
- Warning 2

## Improvements Suggested
1. Priority improvement
2. Another improvement

## Action Items
- [ ] Fix critical issue 1
- [ ] Fix critical issue 2
- [ ] Address warning 1
```

### Phase 3: Save Report

Save the consolidated report to `.claude/reports/deep-verify-YYYY-MM-DD.md`

## Usage

```
/nika-deep-verify
```

Or with focus area:
```
/nika-deep-verify spec      # Only spec validation
/nika-deep-verify code      # Only code validation
/nika-deep-verify full      # All 6 agents (default)
```

## Technical Notes

- Uses `model: haiku` for cost efficiency
- Parallel execution reduces total time
- Each agent has restricted tool access (principle of least privilege)
- Results are aggregated and deduplicated
- Critical issues are highlighted for immediate action

## Quality Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 50-60 | PASS | All good, minor improvements optional |
| 35-49 | WARN | Some issues need attention |
| 0-34 | FAIL | Critical issues must be fixed |
