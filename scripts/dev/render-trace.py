#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
"""render-trace — the nika-cli display grammar, runnable before the Rust.

Renders a workflow trace (NDJSON of `nika-event` wire events) using the
EXACT grammar locked in docs/crate-specs/nika-cli.md §3: audit-as-greeting
header · glyph state machine · braille spinner · live cost meter · final
card. The render is a pure fold over the event stream — the same law the
Rust `display` module must obey.

Usage:
  scripts/dev/render-trace.py --demo            # the wow (synthetic run)
  scripts/dev/render-trace.py --demo-fail       # the failure card
  scripts/dev/render-trace.py <trace.ndjson>    # replay a real trace
  ... [--speed 6] [--ascii] [--no-color] [--summary]

Understands the 11 shipped EventKind slugs + the §3bis PROPOSED extension
kinds (cost_incurred · retry_attempted) so the design is visible forward.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time

# ── glyph themes (spec §3.1 — BOTH are first-class) ─────────────────────────
UNICODE = {
    "pending": "○",
    "running": "◐",
    "ok": "✔",
    "failed": "✖",
    "retrying": "↻",
    "skipped": "⊘",
    "cancelled": "◼",
    "logo": "🦋",
}
ASCII = {
    "pending": ".",
    "running": ">",
    "ok": "ok",
    "failed": "X",
    "retrying": "r",
    "skipped": "-",
    "cancelled": "x",
    "logo": "[nika]",
}
SPINNER = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
SPARK = "▁▂▃▄▅▆▇"


class Term:
    """The ONE colour seam (spec §3.4) — semantic, never decorative."""

    def __init__(self, color: bool, ascii_theme: bool, animate: bool) -> None:
        self.color = color
        self.g = ASCII if ascii_theme else UNICODE
        self.animate = animate

    def paint(self, code: str, text: str) -> str:
        if not self.color:
            return text
        codes = {"cyan": "36", "green": "32", "red": "31", "yellow": "33", "dim": "2", "bold": "1"}
        return f"\x1b[{codes[code]}m{text}\x1b[0m"

    def glyph(self, state: str, tick: int) -> str:
        if state == "running" and self.animate and self.g is UNICODE:
            return self.paint("cyan", SPINNER[tick % len(SPINNER)].ljust(2))
        raw = self.g[state].ljust(2)  # pad BEFORE painting (ANSI breaks ljust)
        color = {
            "running": "cyan",
            "ok": "green",
            "failed": "red",
            "retrying": "yellow",
        }.get(state, "dim")
        return self.paint(color, raw)


def field(ev: dict, key: str, default=None):
    """Tolerant KeyValue extractor (list-of-pairs or plain dict)."""
    fields = ev.get("fields", {})
    if isinstance(fields, dict):
        return fields.get(key, default)
    for kv in fields:
        if kv.get("key") == key:
            return kv.get("value", default)
    return default


class Run:
    """The fold state: what the event stream says the run looks like."""

    def __init__(self) -> None:
        self.workflow = "?"
        self.ceiling = None
        self.permits: list[str] = []
        self.order: list[str] = []  # stable topological row order
        self.tasks: dict[str, dict] = {}
        self.cost = 0.0
        self.spark: list[int] = []
        self.verdict: str | None = None
        self.started: float | None = None
        self.elapsed = 0.0

    def task(self, tid: str) -> dict:
        if tid not in self.tasks:
            self.order.append(tid)
            self.tasks[tid] = {"state": "pending", "note": "", "detail": ""}
        return self.tasks[tid]

    def apply(self, ev: dict) -> None:
        kind = ev.get("kind", "")
        tid = field(ev, "task")
        if kind == "workflow_started":
            self.workflow = field(ev, "workflow", "workflow")
            self.ceiling = field(ev, "ceiling_usd")
            self.permits = field(ev, "permits", []) or []
        elif kind == "task_scheduled" and tid:
            self.task(tid)
        elif kind == "task_started" and tid:
            self.task(tid).update(state="running", note=field(ev, "note", ""))
        elif kind == "task_completed" and tid:
            self.task(tid).update(state="ok", note=field(ev, "note", ""))
        elif kind == "task_failed" and tid:
            self.task(tid).update(
                state="failed", note=field(ev, "note", ""), detail=field(ev, "detail", "")
            )
        elif kind == "task_skipped" and tid:
            self.task(tid).update(state="skipped", note=field(ev, "note", "when: false"))
        elif kind == "retry_attempted" and tid:  # §3bis proposed kind
            self.task(tid).update(state="retrying", note=field(ev, "note", "retrying"))
        elif kind == "cost_incurred":  # §3bis proposed kind
            self.cost += float(field(ev, "usd", 0.0))
            self.spark.append(int(field(ev, "tokens", 1)))
        elif kind in ("workflow_completed", "workflow_failed"):
            self.verdict = "ok" if kind == "workflow_completed" else "failed"

    def done_count(self) -> int:
        return sum(1 for t in self.tasks.values() if t["state"] in ("ok", "failed", "skipped"))


def sparkline(samples: list[int]) -> str:
    tail = samples[-3:]
    if not tail:
        return ""
    top = max(max(tail), 1)
    return "".join(SPARK[min(len(SPARK) - 1, (v * (len(SPARK) - 1)) // top)] for v in tail)


def render(run: Run, term: Term, tick: int) -> list[str]:
    """One frame (spec §3.3) — pure function of the fold state."""
    ceiling = f" · ceiling ≤ ${run.ceiling:.2f}" if run.ceiling else ""
    lines = [
        f"  {term.g['logo']} nika (render-trace) · {term.paint('bold', run.workflow)}"
        f" · {len(run.order)} tasks{ceiling}",
    ]
    if run.permits:
        lines.append(
            f"     permits {term.paint('green', '✓' if term.g is UNICODE else 'OK')} "
            + term.paint("dim", " · ".join(run.permits))
        )
    lines.append("")
    width = max((len(t) for t in run.order), default=10)
    for tid in run.order:
        task = run.tasks[tid]
        note = task["note"]
        if task["state"] == "running" and run.spark:
            note = f"{note} {term.paint('cyan', sparkline(run.spark))}"
        lines.append(f"  {term.glyph(task['state'], tick)} {tid:<{width}}  {term.paint('dim', note)}")
    cost = f"${run.cost:.3f}"
    if run.ceiling:
        cost += f" of ≤${run.ceiling:.2f}"
    meter = f"{run.done_count()}/{len(run.order)} done · {cost} · elapsed {run.elapsed:.1f}s"
    lines.append("  " + term.paint("dim", f"── {meter} ".ljust(64, "─")))
    if run.verdict == "failed":
        for tid in run.order:
            task = run.tasks[tid]
            if task["state"] == "failed" and task["detail"]:
                lines.append("")
                lines.append(f"  {term.glyph('failed', 0)}{term.paint('bold', task['detail'])}")
                code = next((w for w in task["detail"].split() if w.startswith("NIKA-")), None)
                if code:
                    lines.append(term.paint("dim", f"    fix: see `nika explain {code}`"))
    return lines


def replay(events: list[dict], term: Term, speed: float, summary: bool) -> int:
    run = Run()
    first_ts = events[0].get("timestamp", 0) if events else 0
    drawn = 0
    tick = 0
    last_ts = first_ts
    for ev in events:
        ts = ev.get("timestamp", last_ts)
        if not summary and term.animate:
            wait = max(0.0, (ts - last_ts) / 1000.0 / speed)
            steps = max(1, int(wait / 0.08))
            for _ in range(steps):  # spinner ticks while "time passes"
                run.elapsed = (ts - first_ts) / 1000.0
                drawn = redraw(run, term, tick, drawn)
                tick += 1
                time.sleep(min(0.08, wait / steps))
        last_ts = ts
        run.elapsed = (ts - first_ts) / 1000.0
        run.apply(ev)
        if not summary:
            drawn = redraw(run, term, tick, drawn)
    if summary:
        print("\n".join(render(run, term, 0)))
    return 0 if run.verdict == "ok" else 1


def redraw(run: Run, term: Term, tick: int, drawn: int) -> int:
    frame = render(run, term, tick)
    if term.animate and drawn:
        sys.stdout.write(f"\x1b[{drawn}F\x1b[J")  # cursor up + clear (in-place)
    print("\n".join(frame))
    sys.stdout.flush()
    return len(frame) if term.animate else 0


def demo_events(fail: bool) -> list[dict]:
    """The spec §3.3 storyboard as a synthetic event stream (ms timeline)."""

    def ev(ts, kind, **fields):
        return {"timestamp": ts, "kind": kind, "fields": fields}

    base = [
        ev(0, "workflow_started", workflow="veille-news", ceiling_usd=0.04,
           permits=["network:read(hn.algolia.com)", "fs:write(./out)"]),
        *[ev(10, "task_scheduled", task=t) for t in
          ("fetch_top", "extract_ai", "summarize", "write_md", "notify_slack")],
        ev(20, "task_started", task="fetch_top", note="invoke · nika:fetch"),
        ev(1200, "task_completed", task="fetch_top", note="http 200 · 1.2s · 34 KB"),
        ev(1210, "task_started", task="extract_ai", note="exec · jq"),
        ev(1340, "task_completed", task="extract_ai", note="jq · 0.1s · 12 items"),
        ev(1350, "task_started", task="summarize", note="infer · claude-sonnet"),
        ev(1900, "cost_incurred", usd=0.004, tokens=180),
        ev(2600, "cost_incurred", usd=0.005, tokens=320),
        ev(3400, "cost_incurred", usd=0.002, tokens=210),
    ]
    if fail:
        return base + [
            ev(3500, "retry_attempted", task="summarize", note="429 rate-limit · retry 1/2"),
            ev(4600, "retry_attempted", task="summarize", note="429 rate-limit · retry 2/2"),
            ev(5600, "task_failed", task="summarize", note="NIKA-431 · provider refused",
               detail="summarize failed · NIKA-431 provider refused (429) · retried 2×"),
            ev(5610, "task_skipped", task="write_md", note="upstream failed"),
            ev(5620, "task_skipped", task="notify_slack", note="upstream failed"),
            ev(5630, "workflow_failed", workflow="veille-news"),
        ]
    return base + [
        ev(4400, "task_completed", task="summarize", note="claude-sonnet · 3.1s · $0.011"),
        ev(4410, "task_started", task="write_md", note="invoke · nika:write → ./out"),
        ev(4700, "task_completed", task="write_md", note="2.1 KB written"),
        ev(4710, "task_skipped", task="notify_slack", note="when: env.CI != 'true'"),
        ev(4720, "workflow_completed", workflow="veille-news"),
    ]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("trace", nargs="?", help="trace NDJSON path (or use --demo)")
    ap.add_argument("--demo", action="store_true", help="render the synthetic success run")
    ap.add_argument("--demo-fail", action="store_true", help="render the synthetic failure run")
    ap.add_argument("--speed", type=float, default=6.0, help="replay compression (default 6x)")
    ap.add_argument("--ascii", action="store_true", help="force the ASCII glyph theme")
    ap.add_argument("--no-color", action="store_true")
    ap.add_argument("--summary", action="store_true", help="final card only (no replay)")
    args = ap.parse_args()

    if args.demo or args.demo_fail:
        events = demo_events(fail=args.demo_fail)
    elif args.trace:
        with open(args.trace, encoding="utf-8") as fh:
            events = [json.loads(line) for line in fh if line.strip()]
    else:
        ap.print_help()
        return 3

    tty = sys.stdout.isatty()
    color = tty and not args.no_color and not os.environ.get("NO_COLOR")
    animate = tty and not args.summary and not os.environ.get("NIKA_REDUCED_MOTION")
    term = Term(color=color, ascii_theme=args.ascii, animate=animate)
    if not animate and not args.summary:
        args.summary = True  # plain surfaces get the final card (CI-stable)
    return replay(events, term, args.speed, args.summary)


if __name__ == "__main__":
    sys.exit(main())
