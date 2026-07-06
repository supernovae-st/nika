#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# ecosystem-coherence.py — the nightly cross-repo drift bot.
#
# The 2026-07-06 audit found the ecosystem one release behind on two
# consumer surfaces within hours of a tag — twice in one day. Human
# cascade discipline does not survive a release cadence; this bot does.
#
# Two severities (operator lock 2026-07-06):
#   FAIL · pins that MUST be equal — tap formula == latest release tag ·
#          site ENGINE_VERSION == latest release tag · npm published ==
#          client-sdk repo version · engine pack VERSION == spec VERSION.
#          A release younger than 24h demotes its tag-pins to WARN (the
#          cascade window).
#   WARN · operator-gated or by-design lag — OpenVSX published vs vscode
#          repo (publish = tag, operator) · docs status snapshot vs main
#          workspace version (docs describe main, which moves).
#
# Lockstep-at-convergence (operator lock 2026-07-06): from engine 0.97.0
# the satellites (vscode · client-sdk · agents plugin) adopt the engine's
# major.minor. Until then the lockstep row is WARN (preview); from 0.97
# it graduates to FAIL automatically — the bot carries the doctrine.
#
# Read-only · public HTTP only · zero secrets.

import datetime
import json
import os
import sys
import urllib.request

RAW = "https://raw.githubusercontent.com"
FINDINGS = []  # (severity, surface, detail)


def fetch(url, timeout=20):
    headers = {"User-Agent": "nika-coherence-bot"}
    # Actions provides GITHUB_TOKEN — authenticated api.github.com calls
    # dodge the anonymous rate limit (the bot's only GitHub API call).
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith("https://api.github.com/"):
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read().decode()


def grab(url, extract, surface):
    try:
        return extract(fetch(url))
    except Exception as e:  # noqa: BLE001 — a fetch miss is a finding, not a crash
        FINDINGS.append(("WARN", surface, f"unreadable ({e.__class__.__name__}) · {url}"))
        return None


def mm(v):
    return ".".join(v.lstrip("v").split(".")[:2])


def main():
    rel = json.loads(fetch("https://api.github.com/repos/supernovae-st/nika/releases/latest"))
    tag = rel["tag_name"].lstrip("v")
    published = datetime.datetime.fromisoformat(rel["published_at"].replace("Z", "+00:00"))
    age_h = (datetime.datetime.now(datetime.timezone.utc) - published).total_seconds() / 3600
    grace = age_h < 24
    tag_sev = "WARN" if grace else "FAIL"
    print(f"latest release: v{tag} · {age_h:.1f}h old · tag-pin severity: {tag_sev}")

    tap = grab(f"{RAW}/supernovae-st/homebrew-tap/main/Formula/nika.rb",
               lambda t: next(l.split('"')[1] for l in t.splitlines() if l.strip().startswith('version "')),
               "tap")
    if tap and tap != tag:
        FINDINGS.append((tag_sev, "tap", f"formula {tap} != latest release {tag}"))

    site = grab(f"{RAW}/supernovae-st/nika.sh/main/src/content.ts",
                lambda t: next(l.split("'")[1] for l in t.splitlines() if "ENGINE_VERSION" in l).lstrip("v"),
                "site")
    if site and site != tag:
        FINDINGS.append((tag_sev, "site", f"ENGINE_VERSION v{site} != latest release {tag}"))

    sdk_repo = grab(f"{RAW}/supernovae-st/nika-client/main/package.json",
                    lambda t: json.loads(t)["version"], "client-sdk repo")
    npm = grab("https://registry.npmjs.org/@supernovae-st%2Fnika-client",
               lambda t: json.loads(t)["dist-tags"]["latest"], "npm")
    if sdk_repo and npm and sdk_repo != npm:
        FINDINGS.append(("FAIL", "npm", f"published {npm} != repo {sdk_repo}"))

    pack = grab(f"{RAW}/supernovae-st/nika/main/crates/nika-pack/pack/VERSION", str.strip, "pack")
    spec = grab(f"{RAW}/supernovae-st/nika-spec/main/VERSION", str.strip, "spec")
    if pack and spec and pack != spec:
        FINDINGS.append(("FAIL", "pack", f"engine pack {pack} != spec {spec} — re-vendor before the next tag"))

    vscode_repo = grab(f"{RAW}/supernovae-st/nika-vscode/main/package.json",
                       lambda t: json.loads(t)["version"], "vscode repo")
    ovsx = grab("https://open-vsx.org/api/supernovae-st/nika-vscode",
                lambda t: json.loads(t)["version"], "openvsx")
    if vscode_repo and ovsx and vscode_repo != ovsx:
        FINDINGS.append(("WARN", "vscode publish", f"OpenVSX {ovsx} lags repo {vscode_repo} (publish = tag · operator)"))

    engine_main = grab(f"{RAW}/supernovae-st/nika/main/Cargo.toml",
                       lambda t: next(l.split('"')[1] for l in t.splitlines() if l.replace(" ", "").startswith('version=')),
                       "engine main")
    docs_ver = grab(f"{RAW}/supernovae-st/nika-docs/main/snippets/_status-snapshot.mdx",
                    lambda t: next(l.split('"')[1] for l in t.splitlines() if '"version"' in l or "version:" in l),
                    "docs snapshot")
    if engine_main and docs_ver and docs_ver != engine_main:
        FINDINGS.append(("WARN", "docs", f"status snapshot {docs_ver} != main workspace {engine_main} — rerun mintlify-snapshot.sh"))

    # Lockstep-at-convergence · WARN preview below 0.97 · FAIL from 0.97.
    if engine_main:
        lock_sev = "FAIL" if tuple(map(int, engine_main.split("."))) >= (0, 97, 0) else "WARN"
        for name, ver in (("vscode", vscode_repo), ("client-sdk", sdk_repo)):
            if ver and mm(ver) != mm(engine_main):
                FINDINGS.append((lock_sev, f"lockstep {name}",
                                 f"{name} {mm(ver)} != engine {mm(engine_main)} (doctrine active from 0.97)"))

    fails = [f for f in FINDINGS if f[0] == "FAIL"]
    for sev, surface, detail in FINDINGS:
        print(f"{sev}  {surface:16s} {detail}")
    if not FINDINGS:
        print("GREEN — every pin holds")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
