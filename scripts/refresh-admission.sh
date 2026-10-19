#!/usr/bin/env bash
# Refresh the generated OpenRouter rows of crates/nika-catalog/data/inference-admission.toml.
#
# The admission tariffs are a VENDORED snapshot, refreshed at maintenance time
# and never fetched at run time. Routes are listed in
# scripts/admission/sources.toml; every row follows from OpenRouter's public
# endpoint listing by the laws in scripts/admission/refresh_admission.py.
#
# Usage:
#   bash scripts/refresh-admission.sh                     # fetch public bytes, write the rows
#   bash scripts/refresh-admission.sh --from DIR          # replay saved bytes (DIR/as_of.txt)
#   bash scripts/refresh-admission.sh --check [--from DIR]    # write nothing, exit 4 on drift
#   bash scripts/refresh-admission.sh --capacity [--from DIR] # windows and caps, never merged
#   python3 scripts/admission/refresh_admission.py --selftest
#
# Fetched bytes land in target/admission-sources/<UTC date>/ (never committed).
# Exit 0 · clean or written · 2 · environment · 4 · drift, or a route refused or
# absent from the catalogue (its old row kept and named).
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/admission/refresh_admission.py "$@"
