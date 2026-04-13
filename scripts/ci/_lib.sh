#!/usr/bin/env bash
# Shared helpers for diamond CI ratchets.
# Source, do not execute.

set -euo pipefail

# List tracked .rs files under any */src/ directory (excluding tests/benches/examples).
rs_src_files() {
  git ls-files '*.rs' | grep -E '(^|/)src/' | grep -vE '(^|/)(tests|benches|examples)/' || true
}

# Like rs_src_files but also excludes files whose basename is `tests.rs`
# (convention: `src/tests.rs` and `src/foo/tests.rs` are 100% test code).
# Use this for ratchets that should mirror clippy's `#[cfg(test)]` exclusion.
rs_prod_files() {
  rs_src_files | grep -vE '(^|/)tests\.rs$' || true
}

# List every tracked .rs file (any location).
rs_all_files() {
  git ls-files '*.rs' || true
}

# List every tracked Cargo.toml that declares a [package] (i.e. not the workspace root).
package_manifests() {
  git ls-files '*Cargo.toml' 'Cargo.toml' \
    | while read -r m; do
        [ "$m" = "Cargo.toml" ] && continue
        grep -q '^\[package\]' "$m" 2>/dev/null && printf '%s\n' "$m"
      done || true
}

# Read a .rs file from stdin or path, emit the same number of lines but blank
# out every line that lives inside a `#[cfg(test)]` or `#[test]` item body.
# Heuristic only — mirrors clippy's `unwrap_used`/`expect_used` scoping closely
# enough for fast bash ratchets. The cargo-driven `check-clippy.sh` is the real
# source of truth (uses a real Rust parser via clippy-driver).
#
# Limitations:
#   - Does not understand braces inside string or char literals.
#   - Only handles `#[cfg(test)]` and `#[test]` on their own line. Variants like
#     `#[cfg(all(test, ...))]` are not stripped — fix those at clippy level.
#   - Files whose entire content is test code (e.g. included via
#     `#[cfg(test)] mod foo;`) cannot be detected from the file alone — exclude
#     by filename via `rs_prod_files` for the convention `tests.rs`.
strip_test_items() {
  awk '
    BEGIN { skip = 0; depth = 0; pending = 0 }
    {
      if (skip) {
        n = length($0)
        for (i = 1; i <= n; i++) {
          c = substr($0, i, 1)
          if (c == "{") {
            depth++
          } else if (c == "}") {
            depth--
            if (depth == 0) { skip = 0; break }
          }
        }
        print ""
        next
      }
      if (pending) {
        n = length($0)
        for (i = 1; i <= n; i++) {
          c = substr($0, i, 1)
          if (c == "{") {
            depth = 1; skip = 1; pending = 0
            for (j = i + 1; j <= n; j++) {
              c2 = substr($0, j, 1)
              if (c2 == "{") {
                depth++
              } else if (c2 == "}") {
                depth--
                if (depth == 0) { skip = 0; break }
              }
            }
            break
          }
          if (c == ";") { pending = 0; break }
        }
        print ""
        next
      }
      if ($0 ~ /^[[:space:]]*#\[cfg\(test\)\][[:space:]]*$/) {
        pending = 1; print ""; next
      }
      if ($0 ~ /^[[:space:]]*#\[test\][[:space:]]*$/) {
        pending = 1; print ""; next
      }
      # Strip line comments so naive grep ratchets do not trip on text like
      # `// no .expect()` inside doc/comment lines. Does not understand `//`
      # appearing inside a string literal — acceptable for this ratchet,
      # since `.unwrap()` / `.expect(` inside a URL string is a non-issue.
      pos = index($0, "//")
      if (pos > 0) { $0 = substr($0, 1, pos - 1) }
      print
    }
  ' "${1:-/dev/stdin}"
}
