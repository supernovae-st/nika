#!/usr/bin/env bash
# Vector 17: RustSec advisories via cargo-audit.
set -uo pipefail
if ! command -v cargo-audit >/dev/null 2>&1; then
  echo "cargo-audit not installed (run: cargo install cargo-audit)"; exit 1
fi

if ! cargo audit --quiet 2>/dev/null; then
  vuln=$(cargo audit --json 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(len(d.get("vulnerabilities",{}).get("list",[])))' 2>/dev/null || echo "?")
  echo "${vuln} RustSec advisories"; exit 2
fi
echo "OK (no advisories)"; exit 0
