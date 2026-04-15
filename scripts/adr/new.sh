#!/usr/bin/env bash
# Scaffold a new ADR from template with next sequential number.
# Usage: scripts/adr/new.sh <short-kebab-title>
# Example: scripts/adr/new.sh wasm-plugin-sandbox
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ADR_DIR="$REPO_ROOT/docs/adr"

if [ $# -lt 1 ]; then
  echo "Usage: scripts/adr/new.sh <short-kebab-title>" >&2
  echo "Example: scripts/adr/new.sh wasm-plugin-sandbox" >&2
  exit 2
fi

TITLE_KEBAB="$1"

# Validate kebab-case
if ! printf '%s' "$TITLE_KEBAB" | grep -qE '^[a-z0-9][a-z0-9-]*[a-z0-9]$'; then
  echo "ERROR: title must be lowercase kebab-case (e.g. 'wasm-plugin-sandbox')" >&2
  exit 2
fi

# Find next number
LAST_NUM=000
for _f in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$_f" ] || continue
  _num="$(basename "$_f" | grep -oE 'adr-[0-9]{3}' | grep -oE '[0-9]{3}')"
  [ "$_num" -gt "$LAST_NUM" ] 2>/dev/null && LAST_NUM="$_num"
done
NEXT_NUM=$(printf "%03d" $((10#$LAST_NUM + 1)))
NEXT_ID="ADR-${NEXT_NUM}"
FILENAME="adr-${NEXT_NUM}-${TITLE_KEBAB}.md"
FILEPATH="$ADR_DIR/$FILENAME"

if [ -f "$FILEPATH" ]; then
  echo "ERROR: $FILEPATH already exists" >&2
  exit 1
fi

TODAY=$(date +%Y-%m-%d)

cat >"$FILEPATH" <<ADREOF
---
id: ${NEXT_ID}
title: "<Short decision in imperative form>"
status: proposed
date: ${TODAY}
phase: ""
deciders: ["@ThibautMelen"]
tags: []
affects_crates: []
affects_layers: []
supersedes: []
superseded_by: []
related: []
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: []
---

# ${NEXT_ID}: <Short decision in imperative form>

## Context

What is the problem we're solving? What forces are in play -- technical,
organizational, project-specific? Keep it factual. ~3-6 sentences.

Include grep-verifiable evidence (file paths, commit SHAs, LOC counts) if the
context references the codebase. An ADR should be auditable in 3 years.

## Decision

The decision in one clear sentence, then details. What did we choose? What did
we explicitly reject? Cite specific types, modules, or patterns when concrete.

## Consequences

### Positive
- Concrete wins from this decision.
- Use bullets. No vague adjectives.

### Negative
- Honest trade-offs. Every decision has costs.
- Include the cost we're accepting (not hypothetical).

### Neutral
- Ripple effects that are neither wins nor losses but worth noting.

## Evidence / Affected code

- \`path/to/file.rs:line\` -- what lives here
- Commit \`abc1234\` -- when this first shipped
- Related crate: \`nika-foo\`

## Alternatives considered

### Alt A -- <name>
Short description. Why rejected.

### Alt B -- <name>
Short description. Why rejected.

## Related

- ADR-XXX (supersedes / is-superseded-by / complements)
- \`docs/architecture/<relevant-doc>.md\`
- External references (papers, RFC numbers, rust-analyzer patterns, etc.)

## Notes

Any follow-ups, open questions, or review triggers. When should we revisit?
ADREOF

echo "Created $FILEPATH"
echo "Next steps:"
echo "  1. Edit the frontmatter title and body"
echo "  2. Set status to 'proposed' while discussing, 'accepted' once committed"
echo "  3. Run scripts/adr/validate.sh to check"
echo "  4. Commit with: docs(adr): ${NEXT_ID} add <title>"
