#!/usr/bin/env python3
"""
Constellation Dashboard Generator

Reads memory files + live repo state, generates a self-contained HTML dashboard.
Run after every session: `python3 scripts/update-constellation-dashboard.py`

No external deps beyond stdlib. Output: docs/constellation/index.html
"""

import os
import re
import subprocess
import json
from pathlib import Path
from datetime import datetime

REPO_ROOT = Path(__file__).resolve().parent.parent
MEMORY_DIR = Path.home() / ".claude/projects/-Users-thibaut-dev-supernovae-nika/memory"
OUTPUT = REPO_ROOT / "docs/constellation/index.html"

# ── Data extraction ──────────────────────────────────────────────────────────

def run(cmd, cwd=None):
    """Run shell command, return stdout."""
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True,
                           cwd=cwd or REPO_ROOT)
    return result.stdout.strip()

def engine_loc():
    """Count engine LOC."""
    return int(run("find tools/nika-engine/src -name '*.rs' -exec cat {} + | wc -l").strip())

def workspace_loc():
    """Count workspace LOC."""
    return int(run("find tools/*/src -name '*.rs' -exec cat {} + | wc -l").strip())

def crate_count():
    """Count crates in workspace."""
    return int(run("find tools -maxdepth 2 -name Cargo.toml | wc -l").strip())

def head_sha():
    return run("git log -1 --format=%h")[:9]

def head_message():
    return run("git log -1 --format=%s")

def commits_since(ref):
    try:
        count = int(run(f"git rev-list --count {ref}..HEAD"))
        return count
    except:
        return 0

def dispatch_arms_live():
    """Parse dispatch.rs for NotImplemented count."""
    path = REPO_ROOT / "tools/nika-runtime/src/dispatch.rs"
    if not path.exists():
        return (0, 0)  # crate was deleted
    content = path.read_text()
    not_impl = content.count('NotImplemented { verb:')
    # 5 total arms - 2 arms get "NotImplemented" counted even in live ones via matches
    # Actual: grep "return Err.*NotImplemented" gives the real dead arms
    real_not_impl = len(re.findall(r'=> Err\(RuntimeError::NotImplemented', content))
    live = 5 - real_not_impl
    return (live, 5)

def crate_sizes():
    """Get LOC per crate."""
    crates = []
    for cargo in REPO_ROOT.glob("tools/*/Cargo.toml"):
        name = cargo.parent.name
        src = cargo.parent / "src"
        if src.exists():
            loc = int(run(f"find {src} -name '*.rs' -exec cat {{}} + | wc -l"))
            crates.append((name, loc))
    return sorted(crates, key=lambda x: -x[1])

def top_files():
    """Top 15 largest production files."""
    result = run("find tools -name '*.rs' ! -path '*/target/*' ! -name 'tests*.rs' -exec wc -l {} + | sort -rn | head -16")
    files = []
    for line in result.split('\n')[:-1]:  # skip "total"
        parts = line.strip().split(None, 1)
        if len(parts) == 2 and parts[0].isdigit():
            loc = int(parts[0])
            path = parts[1].replace(str(REPO_ROOT) + "/", "")
            files.append((path, loc))
    return files[:15]

def parse_loc_trajectory():
    """Parse the trajectory from constellation_status.md."""
    status = MEMORY_DIR / "project_constellation_status.md"
    if not status.exists():
        return []
    content = status.read_text()
    # Find the trajectory section
    match = re.search(r'Engine LOC Trajectory.*?```(.*?)```', content, re.DOTALL)
    if not match:
        return []
    trajectory = []
    for line in match.group(1).split('\n'):
        m = re.match(r'\s*(S\d+[\.\w-]*):?\s+([\d,]+)', line.strip())
        if m:
            session = m.group(1)
            loc = int(m.group(2).replace(',', ''))
            trajectory.append((session, loc))
    return trajectory

def parse_zombies():
    """Parse zombie backlog."""
    zombie_file = MEMORY_DIR / "project_zombie_backlog.md"
    if not zombie_file.exists():
        return {"open": [], "closed": []}
    content = zombie_file.read_text()
    open_zombies = []
    closed_zombies = []
    current = None
    for match in re.finditer(r'### (Z\d+)\s*—\s*(.+)', content):
        zid = match.group(1)
        title = match.group(2).strip()
        # Check if resolved
        section_start = match.end()
        section_end = content.find('###', section_start)
        if section_end == -1:
            section_end = len(content)
        section = content[section_start:section_end]
        if 'RESOLVED' in section or 'CLOSED' in section or 'STALE' in section:
            closed_zombies.append((zid, title))
        else:
            open_zombies.append((zid, title))
    return {"open": open_zombies, "closed": closed_zombies}

def milestone_status():
    """Hardcoded milestone state — update manually or parse memo."""
    # Simple: mark completed vs pending
    return [
        ("M1", "Deletion Audit Week", "pending", "Engine ≤138k"),
        ("M2", "resolve.rs move", "in_progress", "Engine -3,752 (S23)"),
        ("M3", "FetchAux trait design", "pending", "investment"),
        ("M4", "Structured Output design", "pending", "investment"),
        ("M5", "Structured extraction", "pending", "-2,000 LOC"),
        ("M6", "Fetch parity", "pending", "-1,600 LOC"),
        ("M7", "verb-agent extraction", "pending", "-6,500 LOC (multi-session)"),
        ("M8", "RUNNER REWIRE ⭐", "pending", "THE CLIFF"),
        ("M9", "Dissolution", "pending", "-10,000 LOC"),
        ("M10", "Runtime orch move", "pending", "-2,300 LOC"),
        ("M11", "AST extraction", "pending", "-4,000 LOC → ≤100k"),
        ("M12", "nika-tui split", "pending", "4 sub-crates"),
        ("M13", "nika-core split", "pending", "4 sub-crates"),
        ("M14", "CLI + Init shrink", "pending", "each ≤15k"),
        ("M15", "Agent v2 (2 feats)", "pending", "decision pending"),
        ("M16", "Phantom cleanup", "pending", "honest docs"),
        ("M17", "Per-file ≤1500", "pending", "enforcement"),
        ("M18", "Unwrap 0 hot path", "pending", "clippy deny"),
        ("M19", "Final slim ≤60k", "pending", "TRULY CLEAN"),
    ]

def gate_status():
    """Calculate gate status."""
    engine = engine_loc()
    live, total = dispatch_arms_live()

    return [
        {
            "id": "G1",
            "name": "Dispatch Proof",
            "desc": "runner calls dispatch() in production",
            "status": "❌",
            "evidence": "runner calls TaskExecutor, not dispatch()"
        },
        {
            "id": "G2",
            "name": "Runner Rewire",
            "desc": "all 5 arms live + TaskExecutor deleted",
            "status": "❌",
            "evidence": f"{live}/{total} arms compile, 0 production callers"
        },
        {
            "id": "G3",
            "name": "Delivery Rate",
            "desc": "2/5 last sessions must delete",
            "status": "✅",
            "evidence": "S18, DX-2, S21 delivered deletion"
        },
        {
            "id": "G4",
            "name": "Phantom Elimination",
            "desc": "0 phantom features in docs",
            "status": "❌",
            "evidence": "Shield L5 ML, remote sandbox, L1 taint still phantom"
        },
        {
            "id": "G5",
            "name": "Zombie Ceiling",
            "desc": "≤ 20 open zombies",
            "status": "✅",
            "evidence": "15 open (below cap)"
        },
    ]

def checkpoint_progress():
    """Calculate checkpoint 1 gate progress."""
    engine = engine_loc()
    checkpoint1_gates = {
        "Functional (6)": 1,  # F1 only
        "Quality (5)": 2,      # Q2, Q3
        "Architecture (5)": 0, # A1 fails (engine > 100k)
        "Security (5)": 2,     # S3 covers all 14 providers
        "Docs (5)": 0,
    }
    total_passed = sum(checkpoint1_gates.values())
    return {
        "total_passed": total_passed,
        "total_gates": 26,
        "breakdown": checkpoint1_gates,
    }

# ── HTML Generation ──────────────────────────────────────────────────────────

HTML_TEMPLATE = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>🦋 Nika Constellation Dashboard</title>
<style>
/* Solarized Dark theme */
:root {
  --base03: #002b36; --base02: #073642; --base01: #586e75;
  --base00: #657b83; --base0: #839496; --base1: #93a1a1;
  --base2: #eee8d5; --base3: #fdf6e3;
  --yellow: #b58900; --orange: #cb4b16; --red: #dc322f;
  --magenta: #d33682; --violet: #6c71c4; --blue: #268bd2;
  --cyan: #2aa198; --green: #859900;
}
* { box-sizing: border-box; }
body {
  background: var(--base03); color: var(--base1);
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  margin: 0; padding: 20px; line-height: 1.5;
}
.container { max-width: 1400px; margin: 0 auto; }
h1 {
  color: var(--base3); font-size: 2em; margin: 0 0 4px;
  border-bottom: 2px solid var(--base01); padding-bottom: 8px;
}
.subtitle { color: var(--base01); margin: 0 0 24px; font-size: 0.9em; }
h2 {
  color: var(--cyan); margin: 32px 0 12px;
  border-left: 4px solid var(--cyan); padding-left: 12px;
}
h3 { color: var(--yellow); margin: 16px 0 8px; }
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 16px; margin: 16px 0;
}
.card {
  background: var(--base02); border-radius: 6px;
  padding: 16px; border-left: 4px solid var(--base01);
}
.card.primary { border-left-color: var(--cyan); }
.card.success { border-left-color: var(--green); }
.card.warning { border-left-color: var(--yellow); }
.card.danger { border-left-color: var(--red); }
.metric {
  font-size: 2em; font-weight: bold; color: var(--base3);
  margin: 8px 0;
}
.metric-label {
  font-size: 0.85em; color: var(--base01);
  text-transform: uppercase; letter-spacing: 0.05em;
}
.progress-bar {
  background: var(--base02); border-radius: 4px;
  height: 20px; overflow: hidden; margin: 8px 0;
  border: 1px solid var(--base01);
}
.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--cyan), var(--green));
  transition: width 0.3s ease;
}
table {
  width: 100%; border-collapse: collapse; margin: 12px 0;
  background: var(--base02); border-radius: 6px; overflow: hidden;
}
th, td { padding: 10px 14px; text-align: left; }
th {
  background: var(--base01); color: var(--base3);
  font-weight: bold; text-transform: uppercase;
  font-size: 0.8em; letter-spacing: 0.05em;
}
tr:nth-child(even) { background: rgba(255,255,255,0.02); }
.status-live { color: var(--green); font-weight: bold; }
.status-dead { color: var(--red); font-weight: bold; }
.status-partial { color: var(--yellow); font-weight: bold; }
.status-pending { color: var(--base01); }
.trajectory-chart {
  font-family: monospace; white-space: pre;
  background: var(--base02); padding: 16px;
  border-radius: 6px; overflow-x: auto;
  color: var(--cyan); font-size: 0.85em;
}
.zombie-card {
  font-size: 0.85em; padding: 8px 12px;
  background: var(--base02); margin: 4px 0;
  border-radius: 4px; border-left: 3px solid var(--orange);
}
.zombie-card.closed { border-left-color: var(--green); }
footer {
  margin-top: 48px; padding-top: 16px;
  border-top: 1px solid var(--base01);
  color: var(--base01); font-size: 0.85em; text-align: center;
}
.loc-number { color: var(--cyan); font-weight: bold; font-family: monospace; }
.bar {
  display: inline-block; background: var(--cyan);
  height: 14px; vertical-align: middle; margin-right: 8px;
  border-radius: 2px;
}
.milestone-row {
  display: flex; gap: 12px; align-items: center;
  padding: 8px 12px; background: var(--base02);
  margin: 4px 0; border-radius: 4px; font-size: 0.9em;
}
.milestone-id {
  font-weight: bold; color: var(--cyan); min-width: 40px;
}
.milestone-name { flex: 1; }
.milestone-target { color: var(--base01); font-size: 0.85em; }
.milestone-status { min-width: 90px; text-align: right; }
.updated { color: var(--magenta); }
</style>
</head>
<body>
<div class="container">
  <h1>🦋 Nika Constellation Dashboard</h1>
  <p class="subtitle">Updated <span class="updated">{timestamp}</span> · HEAD <code>{head}</code> · {head_msg}</p>

  <h2>Current State</h2>
  <div class="grid">
    <div class="card primary">
      <div class="metric-label">Engine LOC</div>
      <div class="metric">{engine_loc:,}</div>
      <div class="metric-label">Target ≤ 100,000 (−{engine_to_target:,} to go)</div>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {engine_pct:.1f}%"></div>
      </div>
      <div class="metric-label">{engine_pct:.1f}% deleted of target</div>
    </div>

    <div class="card {dispatch_class}">
      <div class="metric-label">dispatch() Arms</div>
      <div class="metric">{dispatch_live}/{dispatch_total}</div>
      <div class="metric-label">Compile + test pass</div>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {dispatch_pct:.1f}%"></div>
      </div>
      <div class="metric-label">{dispatch_note}</div>
    </div>

    <div class="card">
      <div class="metric-label">Workspace</div>
      <div class="metric">{workspace_loc:,}</div>
      <div class="metric-label">{crate_count} crates · ~10,949 tests · 0 clippy</div>
    </div>

    <div class="card {cp1_class}">
      <div class="metric-label">Checkpoint 1 (v0.9)</div>
      <div class="metric">{cp1_passed}/{cp1_total}</div>
      <div class="metric-label">Gates passed</div>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {cp1_pct:.1f}%"></div>
      </div>
      <div class="metric-label">Need ≥ 90% to ship v0.9</div>
    </div>
  </div>

  <h2>Engine LOC Trajectory</h2>
  <div class="trajectory-chart">{trajectory}</div>

  <h2>Hard Gates</h2>
  <table>
    <thead><tr><th>Gate</th><th>Name</th><th>Status</th><th>Evidence</th></tr></thead>
    <tbody>{gates_rows}</tbody>
  </table>

  <h2>Milestone Progress</h2>
  <div>{milestones}</div>

  <h2>Top 10 Crates by LOC</h2>
  <table>
    <thead><tr><th>Crate</th><th>LOC</th><th>Visual</th></tr></thead>
    <tbody>{crate_rows}</tbody>
  </table>

  <h2>Largest Production Files</h2>
  <table>
    <thead><tr><th>File</th><th>LOC</th></tr></thead>
    <tbody>{file_rows}</tbody>
  </table>

  <h2>Zombie Backlog ({zombie_open_count} open / {zombie_closed_count} closed)</h2>
  <h3>Open</h3>
  {zombies_open}
  <h3>Closed</h3>
  {zombies_closed}

  <footer>
    <p>🦋 Nika Constellation Dashboard · Auto-generated · Solo dev + autonomous agents · AGPL-3.0</p>
    <p>Run: <code>python3 scripts/update-constellation-dashboard.py</code></p>
  </footer>
</div>
</body>
</html>"""

def generate_html():
    engine = engine_loc()
    workspace = workspace_loc()
    crates = crate_count()
    head = head_sha()
    head_msg = head_message()[:80]
    live, total = dispatch_arms_live()

    target = 100000
    engine_to_target = engine - target
    max_engine = 148792  # S12 baseline
    deleted_so_far = max_engine - engine
    total_needed = max_engine - target
    engine_pct = (deleted_so_far / total_needed) * 100

    dispatch_pct = (live / total) * 100 if total > 0 else 0
    dispatch_note = "⚠️ Compile only — 0 production callers" if live < 5 else "Live in production"
    dispatch_class = "warning" if live > 0 and live < 5 else ("danger" if live == 0 else "success")

    # Checkpoint 1
    cp1 = checkpoint_progress()
    cp1_pct = (cp1["total_passed"] / cp1["total_gates"]) * 100
    cp1_class = "success" if cp1_pct >= 90 else ("warning" if cp1_pct >= 50 else "danger")

    # Trajectory chart (ASCII)
    trajectory_data = parse_loc_trajectory()
    traj_lines = []
    if trajectory_data:
        max_loc = max(loc for _, loc in trajectory_data)
        target_bar = 100000
        for session, loc in trajectory_data:
            bar_width = int((loc / max_loc) * 60)
            bar = "█" * bar_width
            traj_lines.append(f"  {session:10s} {loc:>8,} {bar}")
        traj_lines.append(f"  {'TARGET':10s} {target_bar:>8,} " + "─" * 40)
    trajectory = "\n".join(traj_lines)

    # Gates
    gates = gate_status()
    gates_rows = ""
    for g in gates:
        gates_rows += f'<tr><td><strong>{g["id"]}</strong></td><td>{g["name"]}</td><td>{g["status"]}</td><td>{g["evidence"]}</td></tr>'

    # Milestones
    ms = milestone_status()
    ms_html = ""
    for mid, name, status, target in ms:
        status_class = f"status-{status.replace('_', '-')}"
        status_icon = {"completed": "✅", "in_progress": "🔄", "pending": "⬜"}[status]
        ms_html += f'''<div class="milestone-row">
          <span class="milestone-id">{mid}</span>
          <span class="milestone-name">{name}</span>
          <span class="milestone-target">{target}</span>
          <span class="milestone-status {status_class}">{status_icon} {status}</span>
        </div>'''

    # Crate table
    crates_list = crate_sizes()[:10]
    max_crate_loc = crates_list[0][1] if crates_list else 1
    crate_rows = ""
    for name, loc in crates_list:
        bar_width = int((loc / max_crate_loc) * 200)
        crate_rows += f'<tr><td>{name}</td><td class="loc-number">{loc:,}</td><td><span class="bar" style="width: {bar_width}px;"></span></td></tr>'

    # File table
    files = top_files()
    file_rows = ""
    for path, loc in files:
        file_rows += f'<tr><td><code>{path}</code></td><td class="loc-number">{loc:,}</td></tr>'

    # Zombies
    zombies_data = parse_zombies()
    zombies_open = ""
    for zid, title in zombies_data["open"]:
        zombies_open += f'<div class="zombie-card"><strong>{zid}</strong> — {title}</div>'
    zombies_closed = ""
    for zid, title in zombies_data["closed"][:10]:
        zombies_closed += f'<div class="zombie-card closed"><strong>{zid}</strong> — {title}</div>'

    # Use replace-based substitution to avoid CSS brace conflicts
    html = HTML_TEMPLATE
    replacements = dict(
        timestamp=datetime.now().strftime("%Y-%m-%d %H:%M"),
        head=head,
        head_msg=head_msg,
        engine_loc=engine,
        workspace_loc=workspace,
        crate_count=crates,
        engine_to_target=engine_to_target,
        engine_pct=engine_pct,
        dispatch_live=live,
        dispatch_total=total,
        dispatch_pct=dispatch_pct,
        dispatch_note=dispatch_note,
        dispatch_class=dispatch_class,
        cp1_passed=cp1["total_passed"],
        cp1_total=cp1["total_gates"],
        cp1_pct=cp1_pct,
        cp1_class=cp1_class,
        trajectory=trajectory,
        gates_rows=gates_rows,
        milestones=ms_html,
        crate_rows=crate_rows,
        file_rows=file_rows,
        zombie_open_count=len(zombies_data["open"]),
        zombie_closed_count=len(zombies_data["closed"]),
        zombies_open=zombies_open,
        zombies_closed=zombies_closed,
    )
    for k, v in replacements.items():
        html = html.replace("{" + k + "}", str(v))
    return html

if __name__ == "__main__":
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    html = generate_html()
    OUTPUT.write_text(html)
    size_kb = len(html.encode()) // 1024
    print(f"✅ Dashboard updated: {OUTPUT} ({size_kb} KB)")
    print(f"   Open with: open {OUTPUT}")
