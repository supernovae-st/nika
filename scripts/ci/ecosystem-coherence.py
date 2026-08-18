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


def semver_key(v):
    """`0.97.0` / `0.97.0-rc1` → (0, 97, 0) — a pre-release suffix must
    never crash the nightly (int("0-rc1") did, latently)."""
    return tuple(int(part.split("-")[0].split("+")[0]) for part in v.lstrip("v").split(".")[:3])


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

    # The VERSION pin above goes green while CONTENT drifts inside one
    # version (proven 2026-07-09: pack canon said 9 templates, spec said
    # 10, both files read 0.1.0-draft — the MCP oracle taught a stale
    # shelf). Hash the vendored canon against the spec's: a pin at the
    # wrong granularity is worse than no pin.
    import hashlib
    pack_canon = grab(f"{RAW}/supernovae-st/nika/main/crates/nika-pack/pack/canon.yaml", str, "pack canon")
    spec_canon = grab(f"{RAW}/supernovae-st/nika-spec/main/canon.yaml", str, "spec canon")
    if pack_canon and spec_canon and pack_canon != spec_canon:
        ph = hashlib.sha256(pack_canon.encode()).hexdigest()[:12]
        sh = hashlib.sha256(spec_canon.encode()).hexdigest()[:12]
        FINDINGS.append(("WARN", "pack content",
                         f"vendored canon {ph} != spec canon {sh} (same VERSION possible) — sync-pack before the next tag"))

    # The marketplace mirror (nika-plugins) claims byte-parity with engine
    # main; its own gate only fires on push/PR + a daily cron. This pin
    # gives the nightly issue the same eye (drift proven to reappear
    # within 24h of a resync, 2026-07-09).
    eng_skill = grab(f"{RAW}/supernovae-st/nika/main/.agents/plugins/nika/skills/nika-authoring/SKILL.md", str, "agents mirror")
    mir_skill = grab(f"{RAW}/supernovae-st/nika-plugins/main/.agents/plugins/nika/skills/nika-authoring/SKILL.md", str, "agents mirror")
    if eng_skill and mir_skill and eng_skill != mir_skill:
        FINDINGS.append(("WARN", "agents mirror",
                         "nika-plugins SKILL.md != engine main — the byte-parity claim drifted; resync the mirror"))

    vscode_repo = grab(f"{RAW}/supernovae-st/nika-vscode/main/package.json",
                       lambda t: json.loads(t)["version"], "vscode repo")
    ovsx = grab("https://open-vsx.org/api/supernovae/nika-lang",
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

    # Registry · a first-class citizen (operator lock 2026-07-06). The
    # SPEC_PIN the first-party showcases project from must be a real
    # nika-spec commit, and the index must carry artifacts — a wired,
    # living registry, not an orphan.
    spec_pin = grab(f"{RAW}/supernovae-st/nika-registry/main/SPEC_PIN",
                    lambda t: next(l.strip() for l in t.splitlines() if l.strip() and not l.startswith("#")),
                    "registry pin")
    reg_index = grab(f"{RAW}/supernovae-st/nika-registry/main/index.json",
                     lambda t: len(json.loads(t)["artifacts"]), "registry index")
    if reg_index is not None and reg_index == 0:
        FINDINGS.append(("FAIL", "registry", "index.json carries zero artifacts"))
    if spec_pin:
        try:
            fetch(f"https://api.github.com/repos/supernovae-st/nika-spec/commits/{spec_pin}")
        except Exception:  # noqa: BLE001 — an unresolvable pin is the finding
            FINDINGS.append(("FAIL", "registry", f"SPEC_PIN {spec_pin[:9]} not a nika-spec commit"))

    # The distribution defaults (2026-07-13 lesson: the action default sat
    # TWO waves behind and the fix-merge served no one until the v1 tag
    # rolled). Probe what users actually consume — the @v1 ref, not main —
    # so an unbumped default OR an unrolled tag is the same finding. The
    # starter template and the registry certifier are deliberate-bump
    # surfaces: WARN (visible nightly, never a red board on their own).
    act = grab(f"{RAW}/supernovae-st/nika-action/v1/action.yml",
               lambda t: next(l.split("'")[1] for l in t.splitlines() if "default: '0" in l),
               "action@v1")
    if act and act != tag:
        FINDINGS.append((tag_sev, "action@v1",
                         f"served engine-version default {act} != latest release {tag} — bump main + roll v1"))

    starter = grab(f"{RAW}/supernovae-st/nika-actions-starter/main/.github/workflows/nika-check.yml",
                   lambda t: next(l.split("'")[1] for l in t.splitlines() if "default: '0" in l),
                   "actions-starter")
    if starter and starter != tag:
        FINDINGS.append(("WARN", "actions-starter",
                         f"template engine-version default {starter} != latest release {tag}"))

    cert_engine = grab(f"{RAW}/supernovae-st/nika-registry/main/scripts/cert.py",
                       lambda t: next(l.split('"')[1] for l in t.splitlines() if l.startswith("ENGINE_VERSION")),
                       "registry certifier")
    if cert_engine and cert_engine != tag:
        FINDINGS.append(("WARN", "registry certifier",
                         f"cert.py pins nika {cert_engine} != latest release {tag} — bump + cert.py --write (sat 5 waves behind, 2026-07-13)"))

    # Guard-of-guards (2026-07-13): the immune system itself is a surface.
    # Every self-heal workflow the ecosystem now leans on must EXIST on
    # its repo's main — a deleted or renamed leg silently reopens the
    # drift class it killed. Existence only (content is each repo's own).
    IMMUNE = [
        ("nika-docs", "release-heal.yml"),
        ("nika.sh", "release-heal.yml"),
        ("nika.sh", "spec-resync.yml"),
        ("nika-action", "release-heal.yml"),
        ("nika-actions-starter", "release-heal.yml"),
        ("nika-client", "release-heal.yml"),
        ("nika-plugins", "release-heal.yml"),
        ("nika-registry", "release-heal.yml"),
        ("nika-vscode", "spec-pin-heal.yml"),
        # The pack rides the pin heal since 2026-08-18 (pack-resync.yml,
        # which followed spec HEAD beside a pin behind it, is retired).
        ("nika", "spec-pin-heal.yml"),
    ]
    for repo, wf in IMMUNE:
        got = grab(f"{RAW}/supernovae-st/{repo}/main/.github/workflows/{wf}", str,
                   f"immune {repo}")
        if got is not None and "on:" not in got:
            FINDINGS.append(("WARN", f"immune {repo}",
                             f"{wf} exists but carries no trigger — the leg is dead"))

    # Lockstep-at-convergence · WARN preview below 0.97 · FAIL from 0.97.
    if engine_main:
        lock_sev = "FAIL" if semver_key(engine_main) >= (0, 97, 0) else "WARN"
        for name, ver in (("vscode", vscode_repo), ("client-sdk", sdk_repo)):
            if ver and mm(ver) != mm(engine_main):
                FINDINGS.append((lock_sev, f"lockstep {name}",
                                 f"{name} {mm(ver)} != engine {mm(engine_main)} (doctrine active from 0.97)"))

    # Wave-ripeness (operator ruling 2026-07-13 · option C): the release
    # stays a DELIBERATE act — the mechanism proposes, the operator
    # disposes. Informational only, never a finding.
    ripe = grab(f"https://api.github.com/repos/supernovae-st/nika/compare/v{tag}...main",
                lambda t: json.loads(t), "wave ripeness")
    if ripe and isinstance(ripe.get("ahead_by"), int) and ripe["ahead_by"] > 0:
        heads = [c["commit"]["message"].split("\n")[0]
                 for c in ripe.get("commits", [])
                 if c["commit"]["message"].startswith(("feat", "fix"))][-3:]
        print(f"wave-ripeness: main is {ripe['ahead_by']} commits past v{tag}"
              + (" · headline candidates: " + " | ".join(heads) if heads else ""))

    fails = [f for f in FINDINGS if f[0] == "FAIL"]
    for sev, surface, detail in FINDINGS:
        print(f"{sev}  {surface:16s} {detail}")
    if not FINDINGS:
        print("GREEN — every pin holds")
    return 1 if fails else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001 — a dead bot must still say why
        print(f"FAIL  bot              crashed: {exc.__class__.__name__}: {exc}", file=sys.stderr)
        sys.exit(1)
