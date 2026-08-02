#!/usr/bin/env bash
# Shared helpers for diamond CI ratchets.
# Source, do not execute.

set -euo pipefail

# The filter below decides what four ratchets are allowed to SEE, so it
# proves itself honest before any of them reads a verdict from it. A
# broken filter does not make a gate fail — it makes a gate pass quietly,
# and on 2026-08-02 one brace inside a string blanked whole files for the
# .unwrap() ratchet among others.
#
# The proof lives HERE, not in a caller: the pre-push runner ran it, CI
# invoked the check scripts directly and did not, so the very bug the
# self-test was written for would have stayed green in CI indefinitely.
# A dependency belongs with the thing that has it.
#
# Set NIKA_SKIP_FILTER_SELFTEST=1 only inside the self-test itself (it
# sources this file, and must not recurse).
_lib_prove_filter() {
  local selftest="${BASH_SOURCE[0]%/*}/test-strip-test-items.sh"
  [ -n "${NIKA_SKIP_FILTER_SELFTEST:-}" ] && return 0
  [ -x "$selftest" ] || return 0
  if ! NIKA_SKIP_FILTER_SELFTEST=1 bash "$selftest" >/dev/null 2>&1; then
    printf 'FAIL  the shared test-item filter is not honest — this ratchet cannot be trusted:\n' >&2
    NIKA_SKIP_FILTER_SELFTEST=1 bash "$selftest" >&2 || true
    exit 2
  fi
}
_lib_prove_filter

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

# Print the workspace member crate names (one per line, sorted, e.g. nika-error).
# Single source of truth — DO NOT re-parse Cargo.toml `members` in a gate; call
# this. Primary path is `cargo metadata --no-deps` (canonical + robust: it can't
# be fooled by array formatting and it correctly honours the `exclude` list).
# Fallback is an awk scan of the `members = [...]` array for hosts without
# cargo/python (some minimal CI images). Both yield the same 18 today (verified).
# Run from the workspace root (callers cd to ENGINE_ROOT first).
workspace_members() {
  if command -v cargo >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
    local out
    out="$(cargo metadata --format-version 1 --no-deps --offline 2>/dev/null \
      | python3 -c 'import sys, json
try:
    d = json.load(sys.stdin)
    print("\n".join(p["name"] for p in d["packages"]))
except Exception:
    pass' 2>/dev/null | sort -u)"
    if [ -n "$out" ]; then
      printf '%s\n' "$out"
      return 0
    fi
  fi
  # Fallback: awk the members array (cargo/python unavailable or metadata failed).
  awk '
    /^members[[:space:]]*=/ { grab = 1 }
    grab { line = line $0 }
    grab && /\]/ { print line; grab = 0 }
  ' Cargo.toml | grep -oE 'crates/nika-[a-z0-9-]+' | sed 's#crates/##' | sort -u
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
    # Blank out string literals, char literals and comments so brace
    # counting sees CODE only.
    #
    # Without this, a single `{` inside a string in a #[cfg(test)] fn
    # never closes: `skip` stays 1 and EVERY remaining line of the file
    # is blanked. Three ratchets read through this filter — dead-code,
    # error-one-voice, and .unwrap(), the most-cited invariant in the
    # house — so one stray brace in a test fixture silently exempted the
    # rest of a file from all three. Proven 2026-08-02 on a synthetic
    # crate: a real production `.unwrap()` counted 1 in the file and 0
    # after stripping.
    #
    # State is global on purpose: strings and block comments span lines.
    function code_only(s,   out, i, n, c, j, k, hashes, closing) {
      out = ""
      n = length(s)
      i = 1
      while (i <= n) {
        c = substr(s, i, 1)
        if (in_block) {
          if (c == "*" && substr(s, i + 1, 1) == "/") { in_block = 0; i += 2 }
          else { i++ }
          continue
        }
        if (in_raw) {
          if (c == "\"") {
            closing = 1
            for (k = 1; k <= raw_hashes; k++) {
              if (substr(s, i + k, 1) != "#") { closing = 0 }
            }
            if (closing) { in_raw = 0; i += raw_hashes + 1; continue }
          }
          i++
          continue
        }
        if (in_str) {
          if (c == "\\") { i += 2; continue }
          if (c == "\"") { in_str = 0 }
          i++
          continue
        }
        if (c == "/" && substr(s, i + 1, 1) == "/") { break }
        if (c == "/" && substr(s, i + 1, 1) == "*") { in_block = 1; i += 2; continue }
        if (c == "r" && (substr(s, i + 1, 1) == "\"" || substr(s, i + 1, 1) == "#")) {
          hashes = 0
          j = i + 1
          while (substr(s, j, 1) == "#") { hashes++; j++ }
          if (substr(s, j, 1) == "\"") {
            in_raw = 1; raw_hashes = hashes; i = j + 1; continue
          }
        }
        if (c == "\"") { in_str = 1; i++; continue }
        if (c == "'"'"'") {
          # A char literal closes; a lifetime (`'"'"'a`) never does.
          if (substr(s, i + 1, 1) == "\\") {
            j = i + 2
            while (j <= n && substr(s, j, 1) != "'"'"'") { j++ }
            if (j <= n) { i = j + 1; continue }
          } else if (substr(s, i + 2, 1) == "'"'"'") {
            i += 3; continue
          }
        }
        out = out c
        i++
      }
      return out
    }
    BEGIN { skip = 0; depth = 0; pending = 0; in_str = 0; in_raw = 0; in_block = 0 }
    {
      code = code_only($0)
      if (skip) {
        n = length(code)
        for (i = 1; i <= n; i++) {
          c = substr(code, i, 1)
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
        n = length(code)
        for (i = 1; i <= n; i++) {
          c = substr(code, i, 1)
          if (c == "{") {
            depth = 1; skip = 1; pending = 0
            for (j = i + 1; j <= n; j++) {
              c2 = substr(code, j, 1)
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
