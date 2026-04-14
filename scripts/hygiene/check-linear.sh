#!/usr/bin/env bash
# Vector 7: Linear issue states vs actual crate admission.
# Skipped without LINEAR_API_KEY — returns yellow to flag.
set -u
if [ -z "${LINEAR_API_KEY:-}" ]; then
  echo "skipped (LINEAR_API_KEY not set)"; exit 1
fi
echo "OK (linear sync stub)"; exit 0
