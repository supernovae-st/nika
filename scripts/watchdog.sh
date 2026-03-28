#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# Nika Autonomous Session Watchdog
# ═══════════════════════════════════════════════════════════════════════════════
#
# Monitors an autonomous Claude Code session running in tmux session "nika-quality".
# Runs in a separate tmux session "nika-watchdog".
#
# Behaviour:
#   - Every 5 minutes: check last git commit timestamp
#   - 30+ min no commit: WARN — log warning, check if claude process is alive
#   - 60+ min no commit OR no claude process: DEAD — restart from mega-prompt
#
# Statistics tracked:
#   - Total commits since watchdog start
#   - Commits per hour
#   - Number of restarts
#   - Current phase (parsed from progress.md)
#
# Usage:
#   tmux new-session -d -s nika-watchdog
#   tmux send-keys -t nika-watchdog "/path/to/watchdog.sh" Enter
#
# Or directly:
#   ./scripts/watchdog.sh
#
# Requirements: bash 4.4+, git, tmux, pgrep, date (BSD or GNU)
# ═══════════════════════════════════════════════════════════════════════════════

set -Eeuo pipefail
shopt -s inherit_errexit

# ── Constants ──────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly SCRIPT_DIR
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd -P)"
readonly REPO_ROOT

readonly WORKER_SESSION='nika-quality'
# WATCHDOG_SESSION: this script's own tmux session name (documents context for operators)
# shellcheck disable=SC2034
readonly WATCHDOG_SESSION='nika-watchdog'

readonly MEGA_PROMPT_FILE="${REPO_ROOT}/docs/plans/sessions/mega-prompt.md"
readonly PROGRESS_FILE="${REPO_ROOT}/docs/plans/sessions/progress.md"
readonly LOG_FILE="${REPO_ROOT}/logs/watchdog.log"

readonly CHECK_INTERVAL_SECS=300    # 5 minutes
readonly STUCK_THRESHOLD_SECS=1800  # 30 minutes
readonly DEAD_THRESHOLD_SECS=3600   # 60 minutes

readonly CLAUDE_CMD='claude'
readonly RESTART_CMD="cd ${REPO_ROOT} && ${CLAUDE_CMD} --dangerously-skip-permissions --model opus"

# ANSI colors — disable when not a TTY
if [[ -t 1 ]]; then
  readonly C_RESET='\033[0m'
  readonly C_CYAN='\033[0;36m'
  readonly C_GREEN='\033[0;32m'
  readonly C_YELLOW='\033[1;33m'
  readonly C_RED='\033[0;31m'
  readonly C_BOLD='\033[1m'
else
  readonly C_RESET='' C_CYAN='' C_GREEN='' C_YELLOW='' C_RED='' C_BOLD=''
fi

# ── State ──────────────────────────────────────────────────────────────────────

WATCHDOG_START_TS=0
COMMITS_AT_START=0
RESTART_COUNT=0
LAST_WARN_TS=0  # avoid log spam: only warn once per stuck window

# ── Logging ───────────────────────────────────────────────────────────────────

_log_entry() {
  local level="$1"
  local msg="$2"
  local ts
  ts="$(date '+%Y-%m-%d %H:%M:%S')"
  # Structured log line to file (no ANSI)
  printf '[%s] [%s] %s\n' "${ts}" "${level}" "${msg}" >> "${LOG_FILE}"
  # Coloured output to TTY
  case "${level}" in
    INFO)  printf "${C_CYAN}[%s]${C_RESET} %s\n" "${ts}" "${msg}" ;;
    OK)    printf "${C_GREEN}[%s]${C_RESET} %s\n" "${ts}" "${msg}" ;;
    WARN)  printf "${C_YELLOW}[%s] WARN${C_RESET} %s\n" "${ts}" "${msg}" ;;
    ERROR) printf "${C_RED}[%s] ERROR${C_RESET} %s\n" "${ts}" "${msg}" ;;
    STAT)  printf "${C_BOLD}[%s]${C_RESET} %s\n" "${ts}" "${msg}" ;;
  esac
}

log_info()  { _log_entry 'INFO'  "$*"; }
log_ok()    { _log_entry 'OK'    "$*"; }
log_warn()  { _log_entry 'WARN'  "$*"; }
log_error() { _log_entry 'ERROR' "$*"; }
log_stat()  { _log_entry 'STAT'  "$*"; }

# ── Git helpers ────────────────────────────────────────────────────────────────

# Returns the Unix timestamp of the most recent commit, or 0 on failure.
last_commit_ts() {
  local ts
  ts="$(git -C "${REPO_ROOT}" log -1 --format='%ct' 2>/dev/null)" || true
  printf '%s' "${ts:-0}"
}

# Returns total number of commits reachable from HEAD.
total_commits() {
  git -C "${REPO_ROOT}" rev-list --count HEAD 2>/dev/null || printf '0'
}

# Returns short hash + subject of latest commit for display.
last_commit_summary() {
  git -C "${REPO_ROOT}" log -1 --format='%h %s' 2>/dev/null || printf '(no commits)'
}

# ── Process detection ──────────────────────────────────────────────────────────

# Returns 0 if at least one "claude" process is running, 1 otherwise.
claude_is_running() {
  pgrep -x "${CLAUDE_CMD}" > /dev/null 2>&1 \
    || pgrep -f "${CLAUDE_CMD}" > /dev/null 2>&1
}

# ── Progress file parser ───────────────────────────────────────────────────────

# Reads docs/plans/sessions/progress.md and extracts the current phase/status.
# Prints a single-line summary; prints "(progress.md not found)" if absent.
read_current_phase() {
  if [[ ! -f "${PROGRESS_FILE}" ]]; then
    printf '(progress.md not found)'
    return
  fi

  # Strategy: look for lines matching common progress patterns:
  #   - "## Phase N" or "## Wave N" headings
  #   - "Current:" or "Status:" key-value lines
  #   - "- [x]" done items to count progress
  local phase=''
  local status_line=''
  local done_count=0
  local total_count=0

  while IFS= read -r line; do
    # Capture last heading that looks like a phase/wave/session marker
    if [[ "${line}" =~ ^##[[:space:]]+(Phase|Wave|Session|Step)[[:space:]] ]]; then
      phase="${line#\#\# }"
    fi
    # Capture explicit "Current:" or "Status:" lines
    if [[ "${line}" =~ ^[Cc]urrent:[[:space:]] ]]; then
      status_line="${line#*: }"
    fi
    if [[ "${line}" =~ ^[Ss]tatus:[[:space:]] ]]; then
      status_line="${line#*: }"
    fi
    # Count checklist items — store patterns in vars to avoid [[ ]] parsing issues
    local pat_done='^[[:space:]]*-[[:space:]]\[x\]'
    local pat_any='^[[:space:]]*-[[:space:]]\[.?\]'
    if [[ "${line}" =~ ${pat_done} ]]; then
      (( done_count++ )) || true
    fi
    if [[ "${line}" =~ ${pat_any} ]]; then
      (( total_count++ )) || true
    fi
  done < "${PROGRESS_FILE}"

  local summary=''
  [[ -n "${status_line}" ]] && summary="${status_line}"
  [[ -z "${summary}" && -n "${phase}" ]] && summary="${phase}"
  [[ -z "${summary}" ]] && summary='(no phase detected)'
  [[ "${total_count}" -gt 0 ]] && summary="${summary} [${done_count}/${total_count} tasks done]"

  printf '%s' "${summary}"
}

# ── Statistics ─────────────────────────────────────────────────────────────────

print_stats() {
  local now
  now="$(date +%s)"
  local elapsed_secs=$(( now - WATCHDOG_START_TS ))
  local elapsed_min=$(( elapsed_secs / 60 ))

  local current_total
  current_total="$(total_commits)"
  local commits_since=$(( current_total - COMMITS_AT_START ))

  local commits_per_hour=0
  if [[ "${elapsed_secs}" -gt 0 ]]; then
    # Integer arithmetic: commits_per_hour = commits_since * 3600 / elapsed_secs
    commits_per_hour=$(( commits_since * 3600 / elapsed_secs ))
  fi

  local phase
  phase="$(read_current_phase)"

  local last_commit
  last_commit="$(last_commit_summary)"

  log_stat "─────────────────────────────────────────────────"
  log_stat "  Uptime       : ${elapsed_min} min"
  log_stat "  Commits      : ${commits_since} since start (${commits_per_hour}/hr)"
  log_stat "  Restarts     : ${RESTART_COUNT}"
  log_stat "  Phase        : ${phase}"
  log_stat "  Last commit  : ${last_commit}"
  log_stat "─────────────────────────────────────────────────"
}

# ── Restart logic ──────────────────────────────────────────────────────────────

# Sends a claude restart command into the worker tmux session.
# If tmux session doesn't exist, creates it.
restart_claude_session() {
  local reason="$1"
  local now
  now="$(date +%s)"

  (( RESTART_COUNT++ )) || true
  log_warn "RESTART #${RESTART_COUNT} triggered — reason: ${reason}"

  # Verify mega-prompt exists
  if [[ ! -f "${MEGA_PROMPT_FILE}" ]]; then
    log_error "mega-prompt.md not found at ${MEGA_PROMPT_FILE} — cannot restart"
    return 1
  fi

  local phase
  phase="$(read_current_phase)"
  log_info "  Current phase before restart: ${phase}"

  # Kill any lingering claude process to avoid conflicts
  if claude_is_running; then
    log_info "  Sending SIGTERM to running claude processes..."
    pkill -TERM -x "${CLAUDE_CMD}" 2>/dev/null || true
    pkill -TERM -f "${CLAUDE_CMD} --dangerously" 2>/dev/null || true
    # Give it 5 seconds to die gracefully
    local _wait
    for _wait in 1 2 3 4 5; do
      claude_is_running || break
      sleep 1
    done
    if claude_is_running; then
      log_warn "  claude did not exit cleanly — sending SIGKILL"
      pkill -KILL -x "${CLAUDE_CMD}" 2>/dev/null || true
      pkill -KILL -f "${CLAUDE_CMD} --dangerously" 2>/dev/null || true
    fi
  fi

  # Ensure the worker tmux session exists
  if ! tmux has-session -t "${WORKER_SESSION}" 2>/dev/null; then
    log_info "  Creating tmux session '${WORKER_SESSION}'..."
    tmux new-session -d -s "${WORKER_SESSION}" -c "${REPO_ROOT}"
  fi

  # Build the restart command — read prompt at execution time so it's always fresh
  local cmd
  cmd="${RESTART_CMD} -p \"\$(cat '${MEGA_PROMPT_FILE}')\""

  log_info "  Sending restart command to tmux session '${WORKER_SESSION}'..."
  tmux send-keys -t "${WORKER_SESSION}" "" ""      # cancel any pending input
  tmux send-keys -t "${WORKER_SESSION}" "${cmd}" "Enter"

  log_ok "  Restart command sent. Commit clock reset."
  log_info "  Restart #${RESTART_COUNT} at $(date '+%Y-%m-%d %H:%M:%S')"
}

# ── Signal handling ────────────────────────────────────────────────────────────

on_exit() {
  local exit_code=$?
  printf '\n'
  log_info "Watchdog shutting down (exit code ${exit_code})"
  print_stats
  log_info "Full log: ${LOG_FILE}"
  exit "${exit_code}"
}

on_sigint() {
  printf '\n'
  log_info "Received SIGINT — printing summary and exiting cleanly"
  print_stats
  log_info "Full log: ${LOG_FILE}"
  exit 0
}

trap on_exit EXIT
trap on_sigint INT TERM

# ── Validation ─────────────────────────────────────────────────────────────────

validate_environment() {
  local missing=0

  command -v git    > /dev/null 2>&1 || { log_error "git not found"; (( missing++ )) || true; }
  command -v tmux   > /dev/null 2>&1 || { log_error "tmux not found"; (( missing++ )) || true; }
  command -v pgrep  > /dev/null 2>&1 || { log_error "pgrep not found"; (( missing++ )) || true; }
  command -v date   > /dev/null 2>&1 || { log_error "date not found"; (( missing++ )) || true; }

  if [[ ! -d "${REPO_ROOT}/.git" ]]; then
    log_error "Not a git repository: ${REPO_ROOT}"
    (( missing++ )) || true
  fi

  if [[ "${missing}" -gt 0 ]]; then
    log_error "Environment validation failed — ${missing} missing requirement(s)"
    exit 1
  fi

  log_ok "Environment validated"
}

# ── Main check loop ────────────────────────────────────────────────────────────

run_check() {
  local now
  now="$(date +%s)"

  local last_ts
  last_ts="$(last_commit_ts)"

  # Handle repos with no commits yet
  if [[ "${last_ts}" -eq 0 ]]; then
    log_warn "No commits found in repository — skipping this cycle"
    return
  fi

  local age_secs=$(( now - last_ts ))
  local age_min=$(( age_secs / 60 ))

  # Determine state
  local process_alive=false
  claude_is_running && process_alive=true

  if [[ "${age_secs}" -ge "${DEAD_THRESHOLD_SECS}" ]] || ! "${process_alive}"; then
    # ── DEAD state ──────────────────────────────────────────────────
    local reason
    if ! "${process_alive}" && [[ "${age_secs}" -ge "${DEAD_THRESHOLD_SECS}" ]]; then
      reason="no claude process + ${age_min}min since last commit"
    elif ! "${process_alive}"; then
      reason="no claude process (last commit ${age_min}min ago)"
    else
      reason="${age_min}min since last commit (threshold: $(( DEAD_THRESHOLD_SECS / 60 ))min)"
    fi
    log_error "SESSION DEAD — ${reason}"
    restart_claude_session "${reason}"
    # Reset warn clock so stuck-warning fires fresh after restart
    LAST_WARN_TS=0

  elif [[ "${age_secs}" -ge "${STUCK_THRESHOLD_SECS}" ]]; then
    # ── STUCK state ─────────────────────────────────────────────────
    # Only log once per stuck window (don't spam every 5 min)
    local warn_age=$(( now - LAST_WARN_TS ))
    if [[ "${LAST_WARN_TS}" -eq 0 ]] || [[ "${warn_age}" -ge "${STUCK_THRESHOLD_SECS}" ]]; then
      log_warn "SESSION STUCK — ${age_min}min since last commit (threshold: $(( STUCK_THRESHOLD_SECS / 60 ))min)"
      if "${process_alive}"; then
        log_info "  claude process is ALIVE — waiting for it to make progress"
      else
        log_warn "  claude process NOT FOUND — may have crashed"
      fi
      LAST_WARN_TS="${now}"
    fi

  else
    # ── HEALTHY state ────────────────────────────────────────────────
    local last_summary
    last_summary="$(last_commit_summary)"
    log_ok "Healthy — last commit ${age_min}min ago: ${last_summary}"
    # Reset warn clock when healthy
    LAST_WARN_TS=0
  fi
}

# ── Entrypoint ─────────────────────────────────────────────────────────────────

main() {
  # Ensure log directory exists
  mkdir -p "$(dirname "${LOG_FILE}")"

  printf '\n'
  log_info "════════════════════════════════════════════════════"
  log_info "  Nika Watchdog — starting"
  log_info "  Repo        : ${REPO_ROOT}"
  log_info "  Worker tmux : ${WORKER_SESSION}"
  log_info "  Check every : $(( CHECK_INTERVAL_SECS / 60 )) min"
  log_info "  Stuck after : $(( STUCK_THRESHOLD_SECS / 60 )) min"
  log_info "  Dead after  : $(( DEAD_THRESHOLD_SECS / 60 )) min"
  log_info "  Log file    : ${LOG_FILE}"
  log_info "════════════════════════════════════════════════════"
  printf '\n'

  validate_environment

  WATCHDOG_START_TS="$(date +%s)"
  COMMITS_AT_START="$(total_commits)"

  log_info "Baseline: ${COMMITS_AT_START} total commits in repo"
  log_info "Phase    : $(read_current_phase)"
  printf '\n'

  # ── Main loop ──────────────────────────────────────────────────────
  local cycle=0
  while true; do
    (( cycle++ )) || true

    log_info "── Cycle ${cycle} ──────────────────────────────────────"
    run_check

    # Print stats every 6 cycles (~30 min)
    if (( cycle % 6 == 0 )); then
      print_stats
    fi

    sleep "${CHECK_INTERVAL_SECS}"
  done
}

main "$@"
