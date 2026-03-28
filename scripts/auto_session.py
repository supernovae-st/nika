#!/usr/bin/env python3
"""
Nika Autonomous Session Manager
================================
Launches Claude Code sessions, monitors progress via git commits,
auto-restarts on stall, tracks statistics, and ensures continuous work.

Usage:
    python3 scripts/auto_session.py

Runs in a loop until all sessions complete or manually stopped (Ctrl+C).
"""

import subprocess
import time
import json
import os
import signal
import sys
from datetime import datetime, timedelta
from pathlib import Path

# ── Configuration ──────────────────────────────────────────────────

REPO_ROOT = Path(__file__).parent.parent
TOOLS_DIR = REPO_ROOT / "tools"
PLANS_DIR = REPO_ROOT / "docs" / "plans" / "sessions"
LOGS_DIR = REPO_ROOT / "logs"
PROGRESS_FILE = PLANS_DIR / "progress.md"
MEGA_PROMPT = PLANS_DIR / "mega-prompt.md"
STATS_FILE = LOGS_DIR / "session-stats.json"

# Timing
CHECK_INTERVAL_SECS = 120        # Check every 2 minutes
STALL_THRESHOLD_SECS = 1800      # 30 min without commit = stalled
DEAD_THRESHOLD_SECS = 3600       # 60 min without commit = dead
MAX_RESTARTS = 20                # Max auto-restarts before giving up
MAX_BUDGET_PER_SESSION = 20.0    # USD per session

# Claude CLI
CLAUDE_CMD = [
    "claude",
    "--dangerously-skip-permissions",
    "--model", "opus",
    "--max-budget-usd", str(MAX_BUDGET_PER_SESSION),
    "-p",
]

# ── State ──────────────────────────────────────────────────────────

stats = {
    "started_at": None,
    "restarts": 0,
    "total_commits": 0,
    "commits_this_session": 0,
    "sessions_completed": [],
    "current_session": None,
    "last_commit_ts": None,
    "errors": [],
}

claude_process = None
running = True


# ── Helpers ────────────────────────────────────────────────────────

def log(level: str, msg: str):
    ts = datetime.now().strftime("%H:%M:%S")
    icon = {"INFO": "ℹ", "WARN": "⚠", "ERROR": "✗", "OK": "✓", "START": "▶"}
    print(f"[{ts}] {icon.get(level, '·')} {msg}")
    log_file = LOGS_DIR / "auto-session.log"
    with open(log_file, "a") as f:
        f.write(f"[{datetime.now().isoformat()}] [{level}] {msg}\n")


def git_last_commit_ts() -> float:
    """Get timestamp of last git commit."""
    try:
        result = subprocess.run(
            ["git", "log", "-1", "--format=%ct"],
            capture_output=True, text=True, cwd=REPO_ROOT, timeout=10
        )
        return float(result.stdout.strip()) if result.stdout.strip() else 0
    except Exception:
        return 0


def git_commit_count_since(since_ts: float) -> int:
    """Count commits since a timestamp."""
    try:
        since_str = datetime.fromtimestamp(since_ts).strftime("%Y-%m-%d %H:%M:%S")
        result = subprocess.run(
            ["git", "log", "--oneline", f"--since={since_str}"],
            capture_output=True, text=True, cwd=REPO_ROOT, timeout=10
        )
        return len(result.stdout.strip().split("\n")) if result.stdout.strip() else 0
    except Exception:
        return 0


def git_push():
    """Push any unpushed commits."""
    try:
        subprocess.run(
            ["git", "push"],
            capture_output=True, cwd=REPO_ROOT, timeout=30
        )
    except Exception:
        pass


def run_tests() -> bool:
    """Run cargo tests and return True if all pass."""
    try:
        result = subprocess.run(
            ["cargo", "test", "--workspace", "--lib"],
            capture_output=True, text=True, cwd=TOOLS_DIR, timeout=300
        )
        return "0 failed" in result.stdout or result.returncode == 0
    except Exception:
        return False


def is_claude_running() -> bool:
    """Check if a claude process is running."""
    try:
        result = subprocess.run(
            ["pgrep", "-f", "claude.*dangerously"],
            capture_output=True, timeout=5
        )
        return result.returncode == 0
    except Exception:
        return False


def kill_claude():
    """Kill any running claude processes."""
    global claude_process
    try:
        subprocess.run(["pkill", "-f", "claude.*dangerously"], timeout=10)
        time.sleep(2)
        subprocess.run(["pkill", "-9", "-f", "claude.*dangerously"], timeout=5)
    except Exception:
        pass
    claude_process = None


def read_prompt() -> str:
    """Read the mega-prompt from disk (always fresh)."""
    return MEGA_PROMPT.read_text()


def save_stats():
    """Persist stats to JSON."""
    LOGS_DIR.mkdir(exist_ok=True)
    with open(STATS_FILE, "w") as f:
        json.dump(stats, f, indent=2, default=str)


def update_progress(status: str, detail: str = ""):
    """Write progress file for monitoring and recovery."""
    content = f"""# Autonomous Session Progress

**Updated**: {datetime.now().isoformat()}
**Status**: {status}
**Restarts**: {stats['restarts']}
**Total commits**: {stats['total_commits']}
**Current session**: {stats.get('current_session', 'unknown')}

## Detail
{detail}

## Recovery
If this file is stale (>1h old), the session died. Restart with:
```bash
python3 scripts/auto_session.py
```

## Commit log (last 20)
"""
    try:
        result = subprocess.run(
            ["git", "log", "--oneline", "-20"],
            capture_output=True, text=True, cwd=REPO_ROOT, timeout=10
        )
        content += f"```\n{result.stdout}\n```\n"
    except Exception:
        content += "(git log unavailable)\n"

    PROGRESS_FILE.write_text(content)


# ── Core Logic ─────────────────────────────────────────────────────

def launch_claude() -> subprocess.Popen:
    """Launch a Claude Code session with the mega-prompt."""
    global claude_process

    prompt = read_prompt()
    log("START", f"Launching Claude session (restart #{stats['restarts']})")

    LOGS_DIR.mkdir(exist_ok=True)
    log_file = LOGS_DIR / f"claude-{datetime.now().strftime('%Y%m%d-%H%M%S')}.log"

    with open(log_file, "w") as lf:
        claude_process = subprocess.Popen(
            CLAUDE_CMD + [prompt],
            stdout=lf,
            stderr=subprocess.STDOUT,
            cwd=REPO_ROOT,
        )

    stats["commits_this_session"] = 0
    stats["last_commit_ts"] = time.time()
    log("OK", f"Claude PID: {claude_process.pid}, log: {log_file.name}")
    return claude_process


def check_health() -> str:
    """Check session health. Returns: HEALTHY / STALLED / DEAD / COMPLETED."""
    now = time.time()
    last_ts = stats.get("last_commit_ts", now)
    age = now - last_ts

    # Check if claude process is still alive
    if claude_process and claude_process.poll() is not None:
        exit_code = claude_process.returncode
        if exit_code == 0:
            return "COMPLETED"
        else:
            log("WARN", f"Claude exited with code {exit_code}")
            return "DEAD"

    if not is_claude_running():
        return "DEAD"

    # Check commit freshness
    current_ts = git_last_commit_ts()
    if current_ts > last_ts:
        stats["last_commit_ts"] = current_ts
        new_commits = git_commit_count_since(last_ts)
        stats["total_commits"] += new_commits
        stats["commits_this_session"] += new_commits
        log("OK", f"+{new_commits} commits (total: {stats['total_commits']})")
        return "HEALTHY"

    if age > DEAD_THRESHOLD_SECS:
        return "DEAD"
    elif age > STALL_THRESHOLD_SECS:
        return "STALLED"
    else:
        return "HEALTHY"


def handle_stall():
    """Handle a stalled session — warn but don't restart yet."""
    age = time.time() - stats.get("last_commit_ts", time.time())
    log("WARN", f"No commit for {int(age/60)} min — session may be stuck")
    update_progress("STALLED", f"No commit for {int(age/60)} minutes")


def handle_dead():
    """Handle a dead session — kill and restart with backoff."""
    if stats["restarts"] >= MAX_RESTARTS:
        log("ERROR", f"Max restarts ({MAX_RESTARTS}) reached. Stopping.")
        update_progress("FAILED", "Max restarts reached")
        return False

    log("ERROR", "Session dead — restarting...")
    kill_claude()
    time.sleep(3)

    # Push any unpushed work
    git_push()

    # Check if the process died QUICKLY (< 2 min) — likely credit/auth issue
    last_commit_age = time.time() - stats.get("last_commit_ts", time.time())
    if stats["commits_this_session"] == 0 and last_commit_age < 180:
        backoff = min(300 * (2 ** min(stats.get("quick_deaths", 0), 5)), 3600)
        stats["quick_deaths"] = stats.get("quick_deaths", 0) + 1
        log("WARN", f"Quick death detected (likely credit/auth issue). "
                     f"Waiting {backoff}s before retry...")
        update_progress("WAITING", f"Quick death #{stats['quick_deaths']}. "
                                    f"Waiting {backoff}s (credit/auth issue?)")
        time.sleep(backoff)
    else:
        stats["quick_deaths"] = 0  # Reset on healthy session

    stats["restarts"] += 1
    save_stats()
    update_progress("RESTARTING", f"Restart #{stats['restarts']}")

    launch_claude()
    return True


def handle_completed():
    """Handle a completed session."""
    log("OK", "Claude session completed normally")
    git_push()

    # Check if there are more sessions to run
    # The mega-prompt handles sequencing internally
    update_progress("COMPLETED", "Session finished. Check git log for results.")
    save_stats()


# ── Signal Handling ────────────────────────────────────────────────

def signal_handler(sig, frame):
    global running
    log("INFO", "Shutdown requested (Ctrl+C)")
    running = False
    kill_claude()
    git_push()
    update_progress("STOPPED", "Manual shutdown via Ctrl+C")
    save_stats()

    elapsed = time.time() - (stats.get("_start_time", time.time()))
    log("INFO", f"Summary: {stats['total_commits']} commits, "
                f"{stats['restarts']} restarts, "
                f"{int(elapsed/3600)}h {int((elapsed%3600)/60)}m elapsed")
    sys.exit(0)


signal.signal(signal.SIGINT, signal_handler)
signal.signal(signal.SIGTERM, signal_handler)


# ── Main Loop ──────────────────────────────────────────────────────

def main():
    global running

    LOGS_DIR.mkdir(exist_ok=True)

    log("INFO", "═" * 60)
    log("INFO", "  Nika Autonomous Session Manager")
    log("INFO", f"  Repo: {REPO_ROOT}")
    log("INFO", f"  Check interval: {CHECK_INTERVAL_SECS}s")
    log("INFO", f"  Stall threshold: {STALL_THRESHOLD_SECS/60:.0f}m")
    log("INFO", f"  Dead threshold: {DEAD_THRESHOLD_SECS/60:.0f}m")
    log("INFO", f"  Max restarts: {MAX_RESTARTS}")
    log("INFO", "═" * 60)

    stats["started_at"] = datetime.now().isoformat()
    stats["_start_time"] = time.time()
    stats["last_commit_ts"] = git_last_commit_ts() or time.time()
    save_stats()

    # Pre-flight: verify tests pass
    log("INFO", "Pre-flight: running tests...")
    if not run_tests():
        log("ERROR", "Pre-flight tests FAILED. Fix before running.")
        sys.exit(1)
    log("OK", "Pre-flight tests pass")

    # Launch first session
    launch_claude()
    update_progress("RUNNING", "Initial launch")

    # Monitor loop
    while running:
        time.sleep(CHECK_INTERVAL_SECS)

        if not running:
            break

        health = check_health()

        if health == "HEALTHY":
            update_progress("RUNNING", f"Healthy. {stats['commits_this_session']} commits this session.")

        elif health == "STALLED":
            handle_stall()

        elif health == "DEAD":
            if not handle_dead():
                break

        elif health == "COMPLETED":
            handle_completed()
            # Restart for next session (mega-prompt handles sequencing)
            if stats["restarts"] < MAX_RESTARTS:
                log("INFO", "Launching next session cycle...")
                stats["restarts"] += 1
                launch_claude()
            else:
                log("OK", "All work appears complete.")
                break

    # Final
    git_push()
    save_stats()
    log("OK", "Session manager exiting.")


if __name__ == "__main__":
    main()
