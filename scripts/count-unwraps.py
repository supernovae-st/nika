#!/usr/bin/env python3
"""Unwrap ratchet counter — counts .unwrap() and .expect() in production Rust source.

Usage:
    python3 scripts/count-unwraps.py          # Print counts
    python3 scripts/count-unwraps.py --check  # Compare against baseline
    python3 scripts/count-unwraps.py --list   # Print locations

Excludes:
- target/ and target-main/ directories
- Files named tests.rs, test_*.rs, tests_*.rs
- Code inside #[cfg(test)] modules (brace-depth tracking)
- Comment lines (// ...)
"""

import os
import sys

TOOLS_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "tools")
BASELINE = os.path.join(TOOLS_DIR, "unwrap-baseline.txt")

SKIP_DIRS = {"target", "target-main", "tests", "benches", "examples"}
SKIP_FILE_PREFIXES = ("test_", "tests_")
SKIP_FILE_NAMES = ("tests.rs",)


def count_unwraps(list_locations=False):
    unwrap_count = 0
    expect_count = 0
    unwrap_locs = []
    expect_locs = []

    for root, dirs, files in os.walk(TOOLS_DIR):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        if "/src/" not in root + "/" and not root.endswith("/src"):
            continue

        for fname in files:
            if not fname.endswith(".rs"):
                continue
            if fname in SKIP_FILE_NAMES or any(fname.startswith(p) for p in SKIP_FILE_PREFIXES):
                continue

            path = os.path.join(root, fname)
            _count_file(path, unwrap_locs, expect_locs, list_locations)

    unwrap_count = len(unwrap_locs)
    expect_count = len(expect_locs)
    return unwrap_count, expect_count, unwrap_locs, expect_locs


def _count_file(path, unwrap_locs, expect_locs, list_locations):
    try:
        with open(path) as fh:
            lines = fh.readlines()
    except Exception:
        return

    in_test_mod = False
    cfg_test_seen = False
    brace_depth = 0

    for i, line in enumerate(lines, 1):
        stripped = line.strip()

        # Skip comment-only lines
        if stripped.startswith("//"):
            # But still check for #[cfg(test)] on attribute lines
            continue

        # Track #[cfg(test)] attribute
        if "#[cfg(test)]" in stripped:
            cfg_test_seen = True
            continue

        # If we saw #[cfg(test)] and now see a mod declaration, enter test module
        if cfg_test_seen:
            if stripped.startswith("mod ") or stripped.startswith("pub mod "):
                in_test_mod = True
                brace_depth = 0
                cfg_test_seen = False
            elif stripped and not stripped.startswith("#"):
                # Non-attribute, non-empty line after #[cfg(test)] without mod = reset
                cfg_test_seen = False

        if in_test_mod:
            brace_depth += stripped.count("{") - stripped.count("}")
            if brace_depth <= 0 and "}" in stripped:
                in_test_mod = False
                brace_depth = 0
            continue

        rel = os.path.relpath(path, TOOLS_DIR)

        if ".unwrap()" in line:
            unwrap_locs.append(f"{rel}:{i}")
        if ".expect(" in line:
            expect_locs.append(f"{rel}:{i}")


def read_baseline():
    if not os.path.exists(BASELINE):
        return None
    with open(BASELINE) as f:
        for line in f:
            if line.startswith("total="):
                return int(line.strip().split("=")[1])
    return None


def main():
    list_mode = "--list" in sys.argv
    check_mode = "--check" in sys.argv

    unwrap_count, expect_count, unwrap_locs, expect_locs = count_unwraps(list_mode)
    total = unwrap_count + expect_count

    print("=== Unwrap Ratchet ===")
    print(f"  .unwrap(): {unwrap_count}")
    print(f"  .expect(): {expect_count}")
    print(f"  total:     {total}")

    if list_mode:
        print("\n--- .unwrap() locations ---")
        for loc in sorted(unwrap_locs):
            print(f"  {loc}")
        print(f"\n--- .expect() locations ---")
        for loc in sorted(expect_locs):
            print(f"  {loc}")

    if check_mode:
        baseline = read_baseline()
        if baseline is None:
            print(f"\nERROR: baseline not found at {BASELINE}")
            sys.exit(1)

        print(f"\n  baseline:  {baseline}")
        if total > baseline:
            print(f"FAIL: unwrap count increased ({baseline} -> {total})")
            sys.exit(1)
        else:
            print(f"OK: unwrap count within baseline ({total} <= {baseline})")


if __name__ == "__main__":
    main()
