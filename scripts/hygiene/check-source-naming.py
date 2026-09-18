#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Refuse live retired Nika program suffixes in tracked paths and text.

Canonical executable source is lowercase ``*.nika``. Project ``nika.yaml``
and runtime ``.nika/`` are different artifacts.

Exceptions are exact files only — never a directory prefix. Frozen
evidence pins the whole-file digest. Other rows pin hit-count and the
hash of the matching lines, so a new occurrence in an allowlisted file
is red.

    python3 scripts/hygiene/check-source-naming.py
    python3 scripts/hygiene/check-source-naming.py --selftest
    python3 scripts/hygiene/check-source-naming.py --dump-pins
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXCEPTIONS_PATH = ROOT / "scripts" / "hygiene" / "old-suffix-exceptions.json"
RETIRED = (".nika.yaml", ".nika.yml")
CATEGORIES = {"historical", "frozen", "negative", "ratchet"}
CONTENT_NEEDLES = (
    ".nika.yaml",
    ".nika.yml",
    r"\.nika\.ya?ml",
    r"\.nika\.yaml",
    r"\.nika\.yml",
    "*.nika.yaml",
    "*.nika.yml",
    "*.nika.ya?ml",
    ".nika.{yaml,yml}",
    ".nika.{yml,yaml}",
    # JSON-encoded regex (two backslashes in the file text).
    r"\\.nika\\.yaml",
    r"\\.nika\\.yml",
    r"\\.nika\\.ya?ml",
    r".nika\\.ya?ml",
)


def sha256_bytes(raw: bytes) -> str:
    return "sha256:" + hashlib.sha256(raw).hexdigest()


def lines_sha256(lines: list[str]) -> str:
    return sha256_bytes(("\n".join(lines) + "\n").encode("utf-8"))


def git_ls_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-c", "-o", "--exclude-standard", "-z"],
        cwd=ROOT,
        capture_output=True,
        check=True,
    ).stdout
    return [p.decode() for p in out.split(b"\0") if p]


def load_exceptions() -> tuple[list[dict], dict]:
    data = json.loads(EXCEPTIONS_PATH.read_text(encoding="utf-8"))
    rows = data.get("exceptions") or []
    seen: set[str] = set()
    for row in rows:
        for key in ("path", "category", "reason", "owner"):
            if not row.get(key):
                raise SystemExit(f"old-suffix exception missing {key}: {row}")
        path = row["path"]
        if path.endswith("/") or "*" in path or path.endswith("\\"):
            raise SystemExit(f"exception path must be an exact file, not a prefix/glob: {path}")
        if row["category"] not in CATEGORIES:
            raise SystemExit(f"unknown exception category: {row['category']}")
        if path in seen:
            raise SystemExit(f"duplicate exception path: {path}")
        seen.add(path)
        if row["category"] == "frozen":
            if not row.get("digest"):
                raise SystemExit(f"frozen exception missing digest: {path}")
        else:
            if "count" not in row or not row.get("lines_sha256"):
                raise SystemExit(f"content exception missing count/lines_sha256: {path}")
    return rows, data.get("selftest") or {}


def content_hit_lines(text: str) -> list[str]:
    return [line for line in text.splitlines() if any(n in line for n in CONTENT_NEEDLES)]


def read_rel(rel: str, overlay: dict[str, str] | None) -> tuple[bytes, str] | None:
    if overlay is not None and rel in overlay:
        text = overlay[rel]
        return text.encode("utf-8"), text
    path = ROOT / rel
    try:
        raw = path.read_bytes()
        return raw, raw.decode("utf-8")
    except (UnicodeDecodeError, IsADirectoryError, OSError):
        return None


def scan(
    exceptions: list[dict],
    *,
    overlay: dict[str, str] | None = None,
    extra_paths: list[str] | None = None,
) -> list[str]:
    by_path = {row["path"]: row for row in exceptions}
    files = git_ls_files()
    if extra_paths:
        files = list(dict.fromkeys([*files, *extra_paths]))
    failures: list[str] = []
    used: set[str] = set()
    for rel in files:
        row = by_path.get(rel)
        loaded = read_rel(rel, overlay)
        retired_path = rel.endswith(RETIRED)
        if loaded is None:
            if retired_path and row is None:
                failures.append(f"path {rel} · retired program suffix")
            continue
        raw, text = loaded
        hits = content_hit_lines(text)
        if retired_path:
            if row is None or row["category"] != "frozen":
                failures.append(f"path {rel} · retired program suffix")
            elif sha256_bytes(raw) != row["digest"]:
                failures.append(f"{rel} · frozen digest mismatch")
            else:
                used.add(rel)
            continue
        if not hits:
            continue
        if row is None:
            for i, line in enumerate(text.splitlines(), 1):
                if any(n in line for n in CONTENT_NEEDLES):
                    failures.append(f"{rel}:{i} · {line.strip()[:160]}")
            continue
        used.add(rel)
        digest = sha256_bytes(raw)
        if row["category"] == "frozen":
            if digest != row["digest"]:
                failures.append(f"{rel} · frozen digest mismatch")
            continue
        if len(hits) != row["count"]:
            failures.append(f"{rel} · hit count {len(hits)} != pinned {row['count']}")
        actual = lines_sha256(hits)
        if actual != row["lines_sha256"]:
            failures.append(f"{rel} · matching-lines hash mismatch")
    for row in exceptions:
        if row["path"] not in used:
            failures.append(f"stale exception · {row['path']}")
    return failures


def dump_pins() -> int:
    exceptions, _ = load_exceptions()
    files = set(git_ls_files())
    for row in exceptions:
        rel = row["path"]
        loaded = read_rel(rel, None)
        if loaded is None:
            print(f"# missing {rel}", file=sys.stderr)
            continue
        raw, text = loaded
        hits = content_hit_lines(text)
        print(f"{rel}")
        print(f"  digest: {sha256_bytes(raw)}")
        print(f"  count: {len(hits)}")
        print(f"  lines_sha256: {lines_sha256(hits)}")
        if rel not in files:
            print("  # not in git ls-files", file=sys.stderr)
    return 0


def _alias_needles_detected_independently() -> list[str]:
    misses: list[str] = []
    required = (
        ".nika.yaml",
        ".nika.yml",
        r"\.nika\.yaml",
        r"\.nika\.yml",
        r"\.nika\.ya?ml",
        "*.nika.yaml",
        "*.nika.yml",
        ".nika.{yaml,yml}",
        ".nika.{yml,yaml}",
    )
    for needle in required:
        if needle not in CONTENT_NEEDLES:
            misses.append(f"needle missing from CONTENT_NEEDLES: {needle}")

    escaped_line = "pattern " + r"\.nika\.yaml"
    if ".nika.yaml" in escaped_line:
        misses.append("escaped-only fixture accidentally contains plain spelling")
    elif not content_hit_lines(escaped_line):
        misses.append(f"escaped-only line not detected: {escaped_line}")

    brace = "accepts " + ".nika.{yaml,yml}"
    if ".nika.yaml" in brace:
        misses.append("brace fixture accidentally contains plain spelling")
    elif not content_hit_lines(brace):
        misses.append(f"brace line not detected: {brace}")

    glob = "globs " + "*.nika.yaml"
    if not content_hit_lines(glob):
        misses.append(f"glob line not detected: {glob}")

    json_line = r'"pattern": "^[^/].*\\.nika\\.yaml$"'
    if ".nika.yaml" in json_line:
        misses.append("json double-escape fixture accidentally contains plain spelling")
    elif not content_hit_lines(json_line):
        misses.append(f"json double-escaped line not detected: {json_line}")
    ya_line = r"alias .nika\\.ya?ml"
    if ".nika.yaml" in ya_line:
        misses.append("ya?ml double-escape fixture accidentally contains plain spelling")
    elif not content_hit_lines(ya_line):
        misses.append(f"json ya?ml double-escaped line not detected: {ya_line}")
    return misses


def selftest() -> int:
    bad = 0
    exceptions, cfg = load_exceptions()
    clean = scan(exceptions)
    if clean:
        print("selftest: live tree should be clean, got:")
        for item in clean:
            print(f"  {item}")
        bad += 1
    else:
        print("selftest: live tree clean")

    alias_misses = _alias_needles_detected_independently()
    if alias_misses:
        print("selftest: alias detection failed:")
        for item in alias_misses:
            print(f"  {item}")
        bad += 1
    else:
        print("selftest: escaped / glob / brace aliases detected independently")

    inject = cfg.get("inject_path")
    if inject:
        loaded = read_rel(inject, None)
        if loaded is None:
            print(f"selftest: inject_path unreadable: {inject}")
            bad += 1
        else:
            orig = loaded[1]
            injected = orig.rstrip() + "\n\n`nika run foo.nika.yaml`\n"
            chapter_fail = scan(exceptions, overlay={inject: injected})
            if not any(inject in item for item in chapter_fail):
                print("selftest: missed injection into allowlisted file", chapter_fail)
                bad += 1
            else:
                print(f"selftest: injection into {inject} detected")

    new_file = cfg.get("new_file")
    if new_file:
        proof_fail = scan(
            exceptions,
            extra_paths=[new_file],
            overlay={new_file: "nika check hello.nika.yaml\n"},
        )
        if not any(new_file in item for item in proof_fail):
            print("selftest: missed new unlisted file", proof_fail)
            bad += 1
        else:
            print(f"selftest: new file {new_file} detected")

    prefix_file = cfg.get("new_prefix_file")
    if prefix_file:
        prefix_fail = scan(
            exceptions,
            extra_paths=[prefix_file],
            overlay={prefix_file: "historical leftover foo.nika.yaml\n"},
        )
        if not any(prefix_file in item for item in prefix_fail):
            print("selftest: missed new file under former prefix", prefix_fail)
            bad += 1
        else:
            print(f"selftest: new prefix file {prefix_file} detected")

    escaped_file = cfg.get("escaped_file")
    if escaped_file:
        escaped_fail = scan(
            exceptions,
            extra_paths=[escaped_file],
            overlay={escaped_file: "regex " + r"\.nika\.yaml" + "\n"},
        )
        if not any(escaped_file in item for item in escaped_fail):
            print("selftest: missed escaped-alias-only file", escaped_fail)
            bad += 1
        else:
            print("selftest: escaped-alias-only file detected")

    again = scan(exceptions)
    if again:
        print("selftest: overlay leaked into live scan", again)
        bad += 1
    else:
        print("selftest: legitimate history and negative tests still pass")
    return bad


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--dump-pins", action="store_true")
    args = parser.parse_args()
    if args.dump_pins:
        return dump_pins()
    if args.selftest:
        return selftest()
    misses = _alias_needles_detected_independently()
    if misses:
        print("suffix-ratchet: alias scanner regression:", file=sys.stderr)
        for item in misses:
            print(f"  {item}", file=sys.stderr)
        return 1
    failures = scan(load_exceptions()[0])
    for item in failures:
        print(f"  {item}", file=sys.stderr)
    if failures:
        print(f"suffix-ratchet: retired suffix still live ({len(failures)} hit(s))", file=sys.stderr)
        return 1
    print("suffix-ratchet: ok — no live retired suffix outside pinned exceptions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
