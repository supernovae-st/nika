#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# test-ecosystem-coherence.py — the bot self-tests before every real run.
#
# The nightly is only useful if it CANNOT die silently and its severity
# ladder does what the doctrine says. This harness imports the bot,
# monkeypatches `fetch`, and drives the decision table offline:
#   T1  a fresh release (<24h) demotes tag-pins to WARN (grace window)
#   T2  a day-old release makes a stale tap a hard FAIL (exit 1)
#   T3  lockstep is WARN below engine 0.97, FAIL from 0.97
#   T4  a pre-release engine version (0.97.0-rc1) must not crash
#   T5  every surface unreadable → WARNs only, never a crash, exit 0
# Zero network: every URL resolves from the FIXTURES table.

import datetime
import importlib.util
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("bot", HERE / "ecosystem-coherence.py")
bot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bot)


def iso(hours_ago: float) -> str:
    dt = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(hours=hours_ago)
    return dt.strftime("%Y-%m-%dT%H:%M:%SZ")


def fixtures(*, tag="0.95.0", age_h=48.0, tap="0.95.0", site="0.95.0",
             sdk="0.90.0", npm="0.90.0", pack="0.1.0-draft", spec_v="0.1.0-draft",
             engine="0.95.0", vscode="0.96.0", docs="0.95.0", reg_n=21,
             action="0.95.0", starter="0.95.0", certeng="0.95.0",
             pack_sha="b" * 40, engine_pin=None):
    R = bot.RAW
    return {
        "https://api.github.com/repos/supernovae-st/nika/releases/latest":
            json.dumps({"tag_name": f"v{tag}", "published_at": iso(age_h)}),
        f"{R}/supernovae-st/homebrew-tap/main/Formula/nika.rb": f'  version "{tap}"\n',
        f"{R}/supernovae-st/nika.sh/main/src/content.ts": f"export const ENGINE_VERSION = 'v{site}'\n",
        f"{R}/supernovae-st/nika-client/main/package.json": json.dumps({"version": sdk}),
        "https://registry.npmjs.org/@supernovae-st%2Fnika-client": json.dumps({"dist-tags": {"latest": npm}}),
        f"{R}/supernovae-st/nika/main/crates/nika-pack/pack/VERSION": pack + "\n",
        f"{R}/supernovae-st/nika/main/crates/nika-pack/pack/SPEC_SHA": pack_sha + "\n",
        f"{R}/supernovae-st/nika/main/SPEC_PIN": "# pin\n" + (engine_pin or pack_sha) + "\n",
        f"{R}/supernovae-st/nika-spec/{pack_sha}/VERSION": spec_v + "\n",
        f"{R}/supernovae-st/nika-spec/{pack_sha}/canon.yaml": "providers: {}\n",
        f"{R}/supernovae-st/nika/main/crates/nika-pack/pack/canon.yaml": "providers: {}\n",
        f"{R}/supernovae-st/nika-vscode/main/package.json": json.dumps({"version": vscode}),
        "https://open-vsx.org/api/supernovae/nika-lang": json.dumps({"version": vscode}),
        f"{R}/supernovae-st/nika/main/Cargo.toml": f'version      = "{engine}"\n',
        f"{R}/supernovae-st/nika-docs/main/snippets/_status-snapshot.mdx": f'  version: "{docs}",\n',
        f"{R}/supernovae-st/nika-registry/main/SPEC_PIN": "# pin\n" + "a" * 40 + "\n",
        f"{R}/supernovae-st/nika-registry/main/index.json": json.dumps({"artifacts": [{}] * reg_n}),
        "https://api.github.com/repos/supernovae-st/nika-spec/commits/" + "a" * 40: "{}",
        f"{R}/supernovae-st/nika-action/v1/action.yml": f"    default: '{action}'\n",
        f"{R}/supernovae-st/nika-actions-starter/main/.github/workflows/nika-check.yml":
            f"        default: '{starter}'\n",
        f"{R}/supernovae-st/nika-registry/main/scripts/cert.py": f'ENGINE_VERSION = "{certeng}"\n',
        f"https://api.github.com/repos/supernovae-st/nika/compare/v{tag}...main":
            json.dumps({"ahead_by": 0, "commits": []}),
        **{f"{R}/supernovae-st/{repo}/main/.github/workflows/{wf}": "on:\n  schedule: []\n"
           for repo, wf in (("nika-docs","release-heal.yml"),("nika.sh","release-heal.yml"),
                            ("nika.sh","spec-resync.yml"),("nika-action","release-heal.yml"),
                            ("nika-actions-starter","release-heal.yml"),("nika-client","release-heal.yml"),
                            ("nika-plugins","release-heal.yml"),("nika-registry","release-heal.yml"),
                            ("nika-vscode","spec-pin-heal.yml"),("nika","spec-pin-heal.yml"))},
    }


def run(table):
    bot.FINDINGS.clear()
    # fetch RAISES on a missing fixture the way urllib raises on a miss.
    def fetch(url, timeout=20):
        if url in table:
            return table[url]
        raise OSError(f"no fixture for {url}")
    bot.fetch = fetch
    code = bot.main()
    return code, list(bot.FINDINGS)


fails = []


def check(name, cond, detail=""):
    print(f"{'✓' if cond else '✗'} {name}" + (f"  {detail}" if detail and not cond else ""))
    if not cond:
        fails.append(name)


# T1 · fresh release + stale tap → WARN (grace), exit 0
code, f = run(fixtures(age_h=2.0, tap="0.94.0"))
check("T1 grace demotes tag-pins", code == 0 and ("WARN", "tap", ) == tuple(next(x for x in f if x[1] == "tap")[:2]), f)

# T2 · old release + stale tap → FAIL, exit 1
code, f = run(fixtures(age_h=48.0, tap="0.94.0"))
check("T2 stale tap hard-fails post-grace", code == 1 and any(x[0] == "FAIL" and x[1] == "tap" for x in f), f)

# T3 · lockstep severity flips at 0.97
code, f = run(fixtures(engine="0.96.0", vscode="0.99.0"))
check("T3a lockstep WARN below 0.97", any(x[0] == "WARN" and "lockstep vscode" == x[1] for x in f), f)
code, f = run(fixtures(engine="0.97.0", site="0.97.0", tap="0.97.0", tag="0.97.0", docs="0.97.0", vscode="0.99.0"))
check("T3b lockstep FAIL from 0.97", code == 1 and any(x[0] == "FAIL" and "lockstep vscode" == x[1] for x in f), f)

# T3c · the engine pin and embedded pack marker are one identity.
code, f = run(fixtures(engine_pin="c" * 40))
check("T3c split engine/pack identity fails", code == 1 and any(
    x[0] == "FAIL" and x[1] == "pack identity" for x in f), f)

# T4 · pre-release engine version must not crash
try:
    code, f = run(fixtures(engine="0.97.0-rc1", vscode="0.99.0"))
    check("T4 pre-release engine survives", any("lockstep" in x[1] for x in f), f)
except Exception as exc:  # noqa: BLE001
    check("T4 pre-release engine survives", False, repr(exc))

# T5 · every surface dark → WARNs only, exit 0 (the bot reports, never dies)
try:
    code, f = run({"https://api.github.com/repos/supernovae-st/nika/releases/latest":
                   json.dumps({"tag_name": "v0.95.0", "published_at": iso(48)})})
    check("T5 dark surfaces = WARNs, no crash", code == 0 and all(x[0] == "WARN" for x in f), f)
except Exception as exc:  # noqa: BLE001
    check("T5 dark surfaces = WARNs, no crash", False, repr(exc))

# T6 · the served action default (the @v1 ref, what users consume) follows
# the tag ladder: grace WARN → hard FAIL; the deliberate-bump surfaces
# (starter · certifier) stay WARN even post-grace.
code, f = run(fixtures(age_h=2.0, action="0.94.0"))
check("T6a stale action@v1 = WARN in grace",
      code == 0 and any(x[0] == "WARN" and x[1] == "action@v1" for x in f), f)
code, f = run(fixtures(age_h=48.0, action="0.94.0"))
check("T6b stale action@v1 hard-fails post-grace",
      code == 1 and any(x[0] == "FAIL" and x[1] == "action@v1" for x in f), f)
code, f = run(fixtures(age_h=48.0, starter="0.90.0", certeng="0.90.0"))
check("T6c deliberate-bump surfaces stay WARN",
      code == 0 and any(x[1] == "actions-starter" for x in f)
      and any(x[1] == "registry certifier" for x in f)
      and all(x[0] == "WARN" for x in f if x[1] in ("actions-starter", "registry certifier")), f)

# T7 · guard-of-guards: a missing immune workflow is a WARN finding
# (fetch miss), never a crash; a trigger-less one is named.
tbl = fixtures(age_h=48.0)
del tbl[bot.RAW + "/supernovae-st/nika-registry/main/.github/workflows/release-heal.yml"]
tbl[bot.RAW + "/supernovae-st/nika-plugins/main/.github/workflows/release-heal.yml"] = "name: dead\n"
code, f = run(tbl)
check("T7 immune legs watched (missing=WARN · trigger-less named)",
      any(x[1] == "immune nika-registry" for x in f)
      and any(x[1] == "immune nika-plugins" and "no trigger" in x[2] for x in f)
      and all(x[0] == "WARN" for x in f if x[1].startswith("immune")), f)

print(f"\nself-test: {'PASS (10/10)' if not fails else 'FAIL ' + str(fails)}")
sys.exit(1 if fails else 0)
