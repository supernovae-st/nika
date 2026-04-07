#!/usr/bin/env bash
# sync-editors.sh — Keep ALL Nika editor extensions in sync with Rust source.
#
# Extracts keyword lists (verbs, transforms, builtins, top-level fields, task fields,
# verb sub-fields) from the Rust source code and compares them against each editor
# config. Reports drift and optionally auto-fixes with --fix.
#
# Usage:
#   ./editors/sync-editors.sh          # Check for drift (dry run)
#   ./editors/sync-editors.sh --fix    # Auto-update editor configs
#   ./editors/sync-editors.sh --json   # Machine-readable output
#   ./editors/sync-editors.sh --verbose  # Show extracted keyword lists
#
# Run from the nika repo root:
#   ./editors/sync-editors.sh
#
# Exit codes:
#   0  All editors in sync (or --fix applied successfully)
#   1  Drift detected (or --fix was applied: re-run to confirm)
#   2  Fatal: missing required source files or tools
#
# AGPL-3.0-or-later — SuperNovae Studio

set -euo pipefail

# ── Paths ────────────────────────────────────────────────────────────────────
# -P: resolve symlinks so SCRIPT_DIR is always a real path
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"

CORE_DIR="$REPO_ROOT/tools/nika-core/src"

VSCODE_GRAMMAR="$REPO_ROOT/editors/vscode/syntaxes/nika.tmLanguage.json"
ZED_HIGHLIGHTS="$REPO_ROOT/editors/zed/languages/nika/highlights.scm"
NEOVIM_HIGHLIGHTS="$REPO_ROOT/editors/neovim/after/queries/yaml/highlights.scm"
HELIX_LANGUAGES="$REPO_ROOT/editors/helix/languages.toml"

# ── Flags ────────────────────────────────────────────────────────────────────
FIX=false
JSON_OUTPUT=false
VERBOSE=false

for arg in "$@"; do
  case "$arg" in
    --fix) FIX=true ;;
    --json) JSON_OUTPUT=true ;;
    --verbose|-v) VERBOSE=true ;;
    --help|-h)
      printf 'Usage: %s [--fix] [--json] [--verbose]\n\n' "$0"
      printf '  --fix      Auto-update editor configs to match source\n'
      printf '  --json     Machine-readable JSON output\n'
      printf '  --verbose  Show extracted keyword lists\n\n'
      printf 'Run from the nika repo root.\n\n'
      printf 'Exit codes: 0=in sync, 1=drift detected, 2=fatal error\n'
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$arg" >&2
      exit 2
      ;;
  esac
done

# ── Color output ─────────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
  RED='\033[0;31m'
  GREEN='\033[0;32m'
  YELLOW='\033[0;33m'
  BLUE='\033[0;34m'
  BOLD='\033[1m'
  DIM='\033[2m'
  RESET='\033[0m'
else
  RED='' GREEN='' YELLOW='' BLUE='' BOLD='' DIM='' RESET=''
fi

DRIFT_COUNT=0
TOTAL_CHECKS=0

# ── Dependency check ─────────────────────────────────────────────────────────
if ! command -v python3 &>/dev/null; then
  printf 'ERROR: python3 is required but not found in PATH\n' >&2
  exit 2
fi

# ── Helper functions ─────────────────────────────────────────────────────────

log_info()    { [[ "$JSON_OUTPUT" == false ]] && printf "${BLUE}[info]${RESET} %s\n" "$*" || true; }
log_ok()      { [[ "$JSON_OUTPUT" == false ]] && printf "${GREEN}  [ok]${RESET} %s\n" "$*" || true; }
log_drift()   {
  [[ "$JSON_OUTPUT" == false ]] && printf "${RED}  [DRIFT]${RESET} %s\n" "$*" || true
  DRIFT_COUNT=$(( DRIFT_COUNT + 1 ))
}
log_warn()    { [[ "$JSON_OUTPUT" == false ]] && printf "${YELLOW}  [warn]${RESET} %s\n" "$*" || true; }
log_verbose() { [[ "$VERBOSE" == true ]] && [[ "$JSON_OUTPUT" == false ]] && printf "${DIM}  %s${RESET}\n" "$*" || true; }

# Normalise a space-or-newline-delimited list into sorted unique newlines.
# Accepts both space-delimited ("a b c") and newline-delimited output from python3.
sort_unique() {
  printf '%s\n' $1 | sort -u | grep -v '^$' || true
}

# Count non-empty items in a space-or-newline-delimited list.
count_items() {
  local n
  n=$(printf '%s\n' $1 | grep -c '[^[:space:]]' || true)
  printf '%s' "${n:-0}"
}

# Compare two keyword lists (space-or-newline delimited), report drift.
# Increments TOTAL_CHECKS; increments DRIFT_COUNT via log_drift on mismatch.
compare_lists() {
  local source="$1"
  local editor="$2"
  local label="$3"

  local source_sorted editor_sorted
  source_sorted=$(sort_unique "$source")
  editor_sorted=$(sort_unique "$editor")

  # comm requires both inputs to be non-empty files; use process substitution.
  # If one side is empty, supply a truly empty stream (not echo "" which adds \n).
  local missing extra
  missing=$(comm -23 \
    <(printf '%s\n' ${source_sorted:+$source_sorted} | sort) \
    <(printf '%s\n' ${editor_sorted:+$editor_sorted} | sort) \
    2>/dev/null || true)
  extra=$(comm -13 \
    <(printf '%s\n' ${source_sorted:+$source_sorted} | sort) \
    <(printf '%s\n' ${editor_sorted:+$editor_sorted} | sort) \
    2>/dev/null || true)

  local src_count ed_count
  src_count=$(count_items "$source_sorted")
  ed_count=$(count_items "$editor_sorted")

  TOTAL_CHECKS=$(( TOTAL_CHECKS + 1 ))

  if [[ -z "$missing" && -z "$extra" ]]; then
    log_ok "$label: $src_count items in sync"
    return 0
  else
    log_drift "$label: source=$src_count, editor=$ed_count"
    if [[ "$JSON_OUTPUT" == false ]]; then
      if [[ -n "$missing" ]]; then
        printf '    %sMissing from editor:%s\n' "${RED}" "${RESET}"
        printf '%s\n' "$missing" | sed 's/^/      - /'
      fi
      if [[ -n "$extra" ]]; then
        printf '    %sExtra in editor (not in source):%s\n' "${YELLOW}" "${RESET}"
        printf '%s\n' "$extra" | sed 's/^/      + /'
      fi
    fi
    return 1
  fi
}

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# PHASE 1: Extract keyword lists from Rust source
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

log_info "${BOLD}Extracting keywords from Rust source...${RESET}"

# ── 5 Verbs (from ast/raw/action.rs) ────────────────────────────────────────
SOURCE_VERBS="infer exec fetch invoke agent"
log_verbose "Verbs: $SOURCE_VERBS"

# ── All extraction via Python (portable — no grep -P needed) ─────────────────
TRANSFORM_FILE="$CORE_DIR/binding/transform.rs"
BUILTINS_FILE="$CORE_DIR/catalogs/builtins.rs"
PARSER_FILE="$CORE_DIR/ast/raw/parser.rs"

for f in "$TRANSFORM_FILE" "$BUILTINS_FILE" "$PARSER_FILE"; do
  if [[ ! -f "$f" ]]; then
    printf 'ERROR: Cannot find source file: %s\n' "$f" >&2
    printf 'Run this script from the nika repo root.\n' >&2
    exit 2
  fi
done

# Extract transforms from KNOWN_TRANSFORM_NAMES
SOURCE_TRANSFORMS=$(
  python3 -c "
import re
with open('$TRANSFORM_FILE') as f:
    content = f.read()
m = re.search(r'pub static KNOWN_TRANSFORM_NAMES.*?=.*?\[([^]]+)\]', content, re.DOTALL)
if m:
    for name in re.findall(r'\"([^\"]+)\"', m.group(1)):
        print(name)
"
)
TRANSFORM_COUNT=$(count_items "$SOURCE_TRANSFORMS")
log_verbose "Transforms ($TRANSFORM_COUNT): $(printf '%s\n' $SOURCE_TRANSFORMS | tr '\n' ' ')"

# Extract builtins from KNOWN_BUILTIN_TOOLS
SOURCE_BUILTINS=$(
  python3 -c "
import re
with open('$BUILTINS_FILE') as f:
    content = f.read()
m = re.search(r'pub static KNOWN_BUILTIN_TOOLS.*?=.*?\[([^]]+)\]', content, re.DOTALL)
if m:
    for name in re.findall(r'\"([^\"]+)\"', m.group(1)):
        print(name)
"
)
BUILTIN_COUNT=$(count_items "$SOURCE_BUILTINS")
log_verbose "Builtins ($BUILTIN_COUNT): $(printf '%s\n' $SOURCE_BUILTINS | tr '\n' ' ')"

# Extract workflow keys from known_workflow_keys
SOURCE_WORKFLOW_KEYS=$(
  python3 -c "
import re
with open('$PARSER_FILE') as f:
    content = f.read()
m = re.search(r'let known_workflow_keys.*?=.*?\[([^]]+)\]', content, re.DOTALL)
if m:
    for name in re.findall(r'\"([^\"]+)\"', m.group(1)):
        print(name)
"
)
log_verbose "Workflow keys: $(printf '%s\n' $SOURCE_WORKFLOW_KEYS | tr '\n' ' ')"

# Extract task keys from KNOWN_TASK_KEYS
SOURCE_TASK_KEYS=$(
  python3 -c "
import re
with open('$PARSER_FILE') as f:
    content = f.read()
m = re.search(r'const KNOWN_TASK_KEYS.*?=.*?\[([^]]+)\]', content, re.DOTALL)
if m:
    for name in re.findall(r'\"([^\"]+)\"', m.group(1)):
        print(name)
"
)
log_verbose "Task keys: $(printf '%s\n' $SOURCE_TASK_KEYS | tr '\n' ' ')"

# Pre-compute: task fields minus verbs and id and infer-shorthand siblings
SOURCE_TASK_NON_VERB=$(
  echo "$SOURCE_TASK_KEYS" \
    | grep -vE '^(infer|exec|fetch|invoke|agent|id|max_tokens|temperature|system|extended_thinking|thinking_budget|response_format)$' \
    | sort -u
)

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# CHECK: VS Code (TextMate grammar — JSON)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

check_vscode() {
  log_info "${BOLD}VS Code${RESET} ($VSCODE_GRAMMAR)"

  if [[ ! -f "$VSCODE_GRAMMAR" ]]; then
    log_warn "File not found: $VSCODE_GRAMMAR"
    return
  fi

  # Use Python for all JSON parsing (portable)
  local extracted
  extracted=$(
    python3 -c "
import json, re

with open('$VSCODE_GRAMMAR') as f:
    grammar = json.load(f)
repo = grammar.get('repository', {})

def extract_first_group(pattern):
    m = re.search(r'\(([^)]+)\)', pattern)
    return m.group(1).split('|') if m else []

# Verbs
verbs_match = repo.get('verbs', {}).get('match', '')
verbs = extract_first_group(verbs_match)
print('VERBS:' + '|'.join(verbs))

# Top-level keys (from dedicated rules + top-level-keys pattern)
top_keys = set()
for special in ['schema-line', 'workflow-line', 'provider-line']:
    pat = repo.get(special, {}).get('match', '')
    m = re.search(r'\((\w+)\)', pat)
    if m:
        top_keys.add(m.group(1))
tlk = repo.get('top-level-keys', {}).get('match', '')
top_keys.update(extract_first_group(tlk))
print('TOP_KEYS:' + '|'.join(sorted(top_keys)))

# Task fields
tf = repo.get('task-fields', {}).get('match', '')
task_fields = extract_first_group(tf)
print('TASK_FIELDS:' + '|'.join(task_fields))

# Verb sub-fields
vsf = repo.get('verb-sub-fields', {}).get('match', '')
verb_subfields = extract_first_group(vsf)
print('VERB_SUBFIELDS:' + '|'.join(verb_subfields))
"
  )

  local vscode_verbs vscode_top vscode_task vscode_vsf
  vscode_verbs=$(echo "$extracted" | grep '^VERBS:' | sed 's/^VERBS://' | tr '|' '\n' | sort -u | grep -v '^$')
  vscode_top=$(echo "$extracted" | grep '^TOP_KEYS:' | sed 's/^TOP_KEYS://' | tr '|' '\n' | sort -u | grep -v '^$')
  vscode_task=$(echo "$extracted" | grep '^TASK_FIELDS:' | sed 's/^TASK_FIELDS://' | tr '|' '\n' | sort -u | grep -v '^$')
  vscode_vsf=$(echo "$extracted" | grep '^VERB_SUBFIELDS:' | sed 's/^VERB_SUBFIELDS://' | tr '|' '\n' | sort -u | grep -v '^$')

  compare_lists "$SOURCE_VERBS" "$vscode_verbs" "Verbs" || true

  # Source top-level: schema + workflow + provider are in dedicated rules + workflow keys
  local source_top
  source_top=$(printf '%s\n' "schema" "workflow" "provider" $SOURCE_WORKFLOW_KEYS | sort -u)
  compare_lists "$source_top" "$vscode_top" "Top-level keys" || true

  compare_lists "$SOURCE_TASK_NON_VERB" "$vscode_task" "Task fields" || true

  log_verbose "VS Code verb sub-fields count: $(count_items "$vscode_vsf")"
}

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# CHECK: Zed (Tree-sitter highlights.scm — regex patterns)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

check_zed() {
  log_info "${BOLD}Zed${RESET} ($ZED_HIGHLIGHTS)"

  if [[ ! -f "$ZED_HIGHLIGHTS" ]]; then
    log_warn "File not found: $ZED_HIGHLIGHTS"
    return
  fi

  # Zed uses "; ===...===" section markers and #match? with regex patterns
  local extracted
  extracted=$(
    python3 -c "
import re

with open('$ZED_HIGHLIGHTS') as f:
    content = f.read()

def find_match_after(content, section_name, capture):
    pattern = rf'; {re.escape(section_name)}.*?#match\? {re.escape(capture)} \"([^\"]+)\"'
    m = re.search(pattern, content, re.DOTALL)
    if m:
        inner = re.search(r'\^\(([^)]+)\)', m.group(1))
        if inner:
            return inner.group(1).split('|')
    return []

def find_eq_after(content, section_name, capture):
    pattern = rf'; {re.escape(section_name)}.*?#eq\? {re.escape(capture)} \"([^\"]+)\"'
    m = re.search(pattern, content, re.DOTALL)
    if m:
        return [m.group(1)]
    return []

verbs = find_match_after(content, '5 Semantic Verbs', '@keyword.function')
top_keys = set()
top_keys.update(find_eq_after(content, 'Schema declaration', '@keyword'))
top_keys.update(find_eq_after(content, 'Workflow name', '@keyword'))
top_keys.update(find_match_after(content, 'Top-level keywords', '@keyword'))
task_fields = find_match_after(content, 'Task-level fields', '@type')
verb_subfields = find_match_after(content, 'Verb sub-fields', '@property')

print('VERBS:' + '|'.join(verbs))
print('TOP_KEYS:' + '|'.join(sorted(top_keys)))
print('TASK_FIELDS:' + '|'.join(task_fields))
print('VERB_SUBFIELDS:' + '|'.join(verb_subfields))
"
  )

  local zed_verbs zed_top zed_task zed_vsf
  zed_verbs=$(echo "$extracted" | grep '^VERBS:' | sed 's/^VERBS://' | tr '|' '\n' | sort -u | grep -v '^$')
  zed_top=$(echo "$extracted" | grep '^TOP_KEYS:' | sed 's/^TOP_KEYS://' | tr '|' '\n' | sort -u | grep -v '^$')
  zed_task=$(echo "$extracted" | grep '^TASK_FIELDS:' | sed 's/^TASK_FIELDS://' | tr '|' '\n' | sort -u | grep -v '^$')
  zed_vsf=$(echo "$extracted" | grep '^VERB_SUBFIELDS:' | sed 's/^VERB_SUBFIELDS://' | tr '|' '\n' | sort -u | grep -v '^$')

  compare_lists "$SOURCE_VERBS" "$zed_verbs" "Verbs" || true

  local source_top
  source_top=$(printf '%s\n' "schema" "workflow" $SOURCE_WORKFLOW_KEYS | sort -u)
  compare_lists "$source_top" "$zed_top" "Top-level keys" || true

  compare_lists "$SOURCE_TASK_NON_VERB" "$zed_task" "Task fields" || true

  log_verbose "Zed verb sub-fields count: $(count_items "$zed_vsf")"
}

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# CHECK: Neovim (Tree-sitter highlights.scm — #any-of? patterns)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

check_neovim() {
  log_info "${BOLD}Neovim${RESET} ($NEOVIM_HIGHLIGHTS)"

  if [[ ! -f "$NEOVIM_HIGHLIGHTS" ]]; then
    log_warn "File not found: $NEOVIM_HIGHLIGHTS"
    return
  fi

  # Neovim uses "; ── Section Name ──..." markers and #any-of? with quoted strings
  local extracted
  extracted=$(
    python3 -c "
import re

with open('$NEOVIM_HIGHLIGHTS') as f:
    content = f.read()

def find_anyof_after(content, section_name, capture):
    pattern = rf'{re.escape(section_name)}.*?#any-of\? {re.escape(capture)}\s+([^)]+)\)'
    m = re.search(pattern, content, re.DOTALL)
    if m:
        return re.findall(r'\"([^\"]+)\"', m.group(1))
    return []

def find_eq_after(content, section_name, capture):
    pattern = rf'{re.escape(section_name)}.*?#eq\? {re.escape(capture)} \"([^\"]+)\"'
    m = re.search(pattern, content, re.DOTALL)
    if m:
        return [m.group(1)]
    return []

verbs = find_anyof_after(content, '5 Semantic Verbs', '@keyword.function')
top_keys = set()
top_keys.update(find_eq_after(content, 'Schema declaration', '@keyword'))
top_keys.update(find_eq_after(content, 'Workflow name', '@keyword'))
top_keys.update(find_anyof_after(content, 'Top-level keys', '@type'))
top_keys.update(find_anyof_after(content, 'Provider and model', '@attribute'))
task_fields = find_anyof_after(content, 'Task-level fields', '@field')
verb_subfields = find_anyof_after(content, 'Verb sub-fields', '@variable.parameter')

print('VERBS:' + '|'.join(verbs))
print('TOP_KEYS:' + '|'.join(sorted(top_keys)))
print('TASK_FIELDS:' + '|'.join(task_fields))
print('VERB_SUBFIELDS:' + '|'.join(verb_subfields))
"
  )

  local nvim_verbs nvim_top nvim_task nvim_vsf
  nvim_verbs=$(echo "$extracted" | grep '^VERBS:' | sed 's/^VERBS://' | tr '|' '\n' | sort -u | grep -v '^$')
  nvim_top=$(echo "$extracted" | grep '^TOP_KEYS:' | sed 's/^TOP_KEYS://' | tr '|' '\n' | sort -u | grep -v '^$')
  nvim_task=$(echo "$extracted" | grep '^TASK_FIELDS:' | sed 's/^TASK_FIELDS://' | tr '|' '\n' | sort -u | grep -v '^$')
  nvim_vsf=$(echo "$extracted" | grep '^VERB_SUBFIELDS:' | sed 's/^VERB_SUBFIELDS://' | tr '|' '\n' | sort -u | grep -v '^$')

  compare_lists "$SOURCE_VERBS" "$nvim_verbs" "Verbs" || true

  local source_top
  source_top=$(printf '%s\n' "schema" "workflow" $SOURCE_WORKFLOW_KEYS | sort -u)
  compare_lists "$source_top" "$nvim_top" "Top-level keys" || true

  compare_lists "$SOURCE_TASK_NON_VERB" "$nvim_task" "Task fields" || true

  log_verbose "Neovim verb sub-fields count: $(count_items "$nvim_vsf")"
}

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# CHECK: Helix (languages.toml — structure only)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

check_helix() {
  log_info "${BOLD}Helix${RESET} ($HELIX_LANGUAGES)"

  if [[ ! -f "$HELIX_LANGUAGES" ]]; then
    log_warn "File not found: $HELIX_LANGUAGES"
    return
  fi

  TOTAL_CHECKS=$(( TOTAL_CHECKS + 1 ))
  if grep -q 'nika-lsp' "$HELIX_LANGUAGES"; then
    log_ok "LSP server configured (nika-lsp)"
  else
    log_drift "Missing nika-lsp language server config"
  fi

  TOTAL_CHECKS=$(( TOTAL_CHECKS + 1 ))
  if grep -q 'nika.yaml' "$HELIX_LANGUAGES"; then
    log_ok "File type detection (*.nika.yaml)"
  else
    log_drift "Missing *.nika.yaml file type detection"
  fi

  local helix_queries="$REPO_ROOT/editors/helix/queries/nika/highlights.scm"
  if [[ -f "$helix_queries" ]]; then
    log_ok "Custom highlights.scm present"
  else
    log_warn "No custom highlights.scm — Helix uses YAML grammar highlighting only"
    log_verbose "Create editors/helix/queries/nika/highlights.scm for keyword highlighting"
  fi
}

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# PHASE 3: Auto-fix (--fix mode)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fix_vscode() {
  log_info "Fixing VS Code grammar..."

  if [[ ! -f "$VSCODE_GRAMMAR" ]]; then
    log_warn "VS Code grammar not found, skipping"
    return
  fi

  # Build pipe-joined keyword strings for regex substitution
  local top_level_pipe
  top_level_pipe=$(
    printf '%s\n' $SOURCE_WORKFLOW_KEYS \
      | grep -vE '^(schema|workflow|provider)$' \
      | sort -u \
      | tr '\n' '|' \
      | sed 's/|$//'
  )

  local task_fields_pipe
  task_fields_pipe=$(
    printf '%s\n' $SOURCE_TASK_NON_VERB | tr '\n' '|' | sed 's/|$//'
  )

  python3 << PYEOF
import json, re

with open('$VSCODE_GRAMMAR') as f:
    grammar = json.load(f)

repo = grammar['repository']
changed = False

# Update top-level-keys
old_match = repo['top-level-keys']['match']
new_match = re.sub(r'\([^)]+\)\(:\)', '($top_level_pipe)(:)', old_match, count=1)
if old_match != new_match:
    repo['top-level-keys']['match'] = new_match
    print('  Updated top-level-keys')
    changed = True

# Update task-fields
old_tf = repo['task-fields']['match']
new_tf = re.sub(r'\([^)]+\)\(:\)', '($task_fields_pipe)(:)', old_tf, count=1)
if old_tf != new_tf:
    repo['task-fields']['match'] = new_tf
    print('  Updated task-fields')
    changed = True

if changed:
    with open('$VSCODE_GRAMMAR', 'w') as f:
        json.dump(grammar, f, indent=2, ensure_ascii=False)
        f.write('\n')
    print('  VS Code grammar written.')
else:
    print('  No changes needed.')
PYEOF
}

fix_zed() {
  log_info "Fixing Zed highlights..."

  if [[ ! -f "$ZED_HIGHLIGHTS" ]]; then
    log_warn "Zed highlights not found, skipping"
    return
  fi

  local task_fields_pipe
  task_fields_pipe=$(printf '%s\n' $SOURCE_TASK_NON_VERB | tr '\n' '|' | sed 's/|$//')

  local top_keys_pipe
  top_keys_pipe=$(
    printf '%s\n' $SOURCE_WORKFLOW_KEYS \
      | grep -vE '^(schema|workflow)$' \
      | sort -u \
      | tr '\n' '|' \
      | sed 's/|$//'
  )

  python3 << PYEOF
import re

with open('$ZED_HIGHLIGHTS') as f:
    content = f.read()

# Update top-level keyword regex: the line with @keyword and #match?
# but NOT the schema/workflow-specific #eq? lines
# Target: (#match? @keyword "^(description|provider|...)$")
content = re.sub(
    r'(#match\? @keyword "\^)\([^)]+\)(\\\$")',
    r'\1($top_keys_pipe)\2',
    content,
    count=1
)

# Update verb regex: (#match? @keyword.function "^(infer|exec|...)$")
content = re.sub(
    r'(#match\? @keyword\.function "\^)\([^)]+\)(\\\$")',
    r'\1(infer|exec|fetch|invoke|agent)\2',
    content,
    count=1
)

# Update task-level fields: (#match? @type "^(with|depends_on|...)$")
content = re.sub(
    r'(#match\? @type "\^)\([^)]+\)(\\\$")',
    r'\1($task_fields_pipe)\2',
    content,
    count=1
)

with open('$ZED_HIGHLIGHTS', 'w') as f:
    f.write(content)

print('  Zed highlights written.')
PYEOF
}

fix_neovim() {
  log_info "Fixing Neovim highlights..."

  if [[ ! -f "$NEOVIM_HIGHLIGHTS" ]]; then
    log_warn "Neovim highlights not found, skipping"
    return
  fi

  local top_keys_quoted
  top_keys_quoted=$(
    printf '%s\n' $SOURCE_WORKFLOW_KEYS \
      | grep -vE '^(schema|workflow|provider|model)$' \
      | sort -u \
      | sed 's/.*/"&"/' \
      | tr '\n' ' ' \
      | sed 's/ $//'
  )

  local task_fields_quoted
  task_fields_quoted=$(
    printf '%s\n' $SOURCE_TASK_NON_VERB \
      | sed 's/.*/"&"/' \
      | tr '\n' ' ' \
      | sed 's/ $//'
  )

  python3 << PYEOF
import re

with open('$NEOVIM_HIGHLIGHTS') as f:
    content = f.read()

def replace_anyof_block(content, capture_name, new_keywords_str):
    """Replace entire multi-line #any-of? block for a given capture name.

    Matches from (#any-of? @capture ... through the closing )))).
    The pattern accounts for multi-line keyword lists.
    """
    # Match the #any-of? block: from (#any-of? @capture through ))))
    pattern = rf'(\(#any-of\? {re.escape(capture_name)}\s+).*?\)\)\)\)'

    # Format new keywords: wrap at ~80 chars with proper indentation
    words = new_keywords_str.strip().split()
    # Compute indentation from capture name
    prefix_len = len(f'(#any-of? {capture_name} ')
    indent = '        '  # 8 spaces (standard tree-sitter query indent)

    lines = []
    current_line = ''
    for w in words:
        test = current_line + (' ' if current_line else '') + w
        if len(test) > 70 and current_line:
            lines.append(current_line)
            current_line = w
        else:
            current_line = test
    if current_line:
        lines.append(current_line)

    if len(lines) <= 1:
        replacement = f'(#any-of? {capture_name} {lines[0] if lines else ""}))))'
    else:
        formatted = f'(#any-of? {capture_name}\n'
        for i, line in enumerate(lines):
            if i == len(lines) - 1:
                formatted += f'{indent}{line}))))'
            else:
                formatted += f'{indent}{line}\n'
        replacement = formatted

    new_content = re.sub(pattern, replacement, content, count=1, flags=re.DOTALL)
    return new_content

content = replace_anyof_block(content, '@type', '$top_keys_quoted')
content = replace_anyof_block(content, '@field', '$task_fields_quoted')

with open('$NEOVIM_HIGHLIGHTS', 'w') as f:
    f.write(content)

print('  Neovim highlights written.')
PYEOF
}

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# MAIN
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

if [[ "$JSON_OUTPUT" == false ]]; then
  printf '\n'
  printf "${BOLD}Nika Editor Sync${RESET} — checking keyword drift across editor extensions\n"
  printf "${DIM}Source of truth: tools/nika-core/src/ (transforms, builtins, parser keys)${RESET}\n"
  printf '\n'

  log_info "Source counts:"
  printf '  Verbs:          %s\n' "$(count_items "$SOURCE_VERBS")"
  printf '  Transforms:     %s\n' "$TRANSFORM_COUNT"
  printf '  Builtins:       %s\n' "$BUILTIN_COUNT"
  printf '  Workflow keys:  %s\n' "$(count_items "$SOURCE_WORKFLOW_KEYS")"
  printf '  Task keys:      %s\n' "$(count_items "$SOURCE_TASK_KEYS")"
  printf '\n'
fi

check_vscode
[[ "$JSON_OUTPUT" == false ]] && printf '\n' || true
check_zed
[[ "$JSON_OUTPUT" == false ]] && printf '\n' || true
check_neovim
[[ "$JSON_OUTPUT" == false ]] && printf '\n' || true
check_helix

if [[ "$JSON_OUTPUT" == false ]]; then
  printf '\n'

  # ── Summary ──────────────────────────────────────────────────────────────────
  printf "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}\n"
  if [[ "$DRIFT_COUNT" -eq 0 ]]; then
    printf "${GREEN}${BOLD}All %s checks passed — editors are in sync.${RESET}\n" "$TOTAL_CHECKS"
  else
    printf "${RED}${BOLD}%s drift(s) detected${RESET} across %s checks.\n" "$DRIFT_COUNT" "$TOTAL_CHECKS"
    if [[ "$FIX" == true ]]; then
      printf '\n'
      log_info "${BOLD}Applying fixes...${RESET}"
      fix_vscode
      fix_zed
      fix_neovim
      printf '\n'
      printf "${GREEN}${BOLD}Fixes applied. Re-run without --fix to verify.${RESET}\n"
    else
      printf "${YELLOW}Run with --fix to auto-update editor configs.${RESET}\n"
    fi
  fi
fi

# ── JSON output ──────────────────────────────────────────────────────────────
if [[ "$JSON_OUTPUT" == true ]]; then
  # Pass keyword lists via environment variables to avoid shell-injection in heredoc.
  # All lists are newline-delimited; Python splits on \n.
  NIKA_VERBS="$SOURCE_VERBS" \
  NIKA_TRANSFORMS="$SOURCE_TRANSFORMS" \
  NIKA_BUILTINS="$SOURCE_BUILTINS" \
  NIKA_WORKFLOW_KEYS="$SOURCE_WORKFLOW_KEYS" \
  NIKA_TASK_KEYS="$SOURCE_TASK_KEYS" \
  NIKA_DRIFT_COUNT="$DRIFT_COUNT" \
  NIKA_TOTAL_CHECKS="$TOTAL_CHECKS" \
  python3 - << 'PYEOF'
import json, os

def read_env_list(var):
    raw = os.environ.get(var, '')
    return sorted(set(item for item in raw.replace(' ', '\n').split('\n') if item.strip()))

source = {
    'verbs':         read_env_list('NIKA_VERBS'),
    'transforms':    read_env_list('NIKA_TRANSFORMS'),
    'builtins':      read_env_list('NIKA_BUILTINS'),
    'workflow_keys': read_env_list('NIKA_WORKFLOW_KEYS'),
    'task_keys':     read_env_list('NIKA_TASK_KEYS'),
}

print(json.dumps({
    'drift_count':  int(os.environ.get('NIKA_DRIFT_COUNT', '0')),
    'total_checks': int(os.environ.get('NIKA_TOTAL_CHECKS', '0')),
    'source':       source,
}, indent=2))
PYEOF
fi

[[ "$JSON_OUTPUT" == false ]] && printf '\n' || true

# Exit 1 if drift detected (CI-friendly: non-zero = needs attention)
if [[ "$DRIFT_COUNT" -gt 0 ]]; then
  exit 1
fi
