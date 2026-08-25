#!/usr/bin/env bash
# Shared helpers for diamond CI ratchets.
# Source, do not execute.
#
# This file deliberately sets NO shell options. It is sourced, so `set -euo
# pipefail` here does not configure this library — it reconfigures the
# CALLER, at source time, overwriting the options the caller chose a few
# lines above. Six hygiene vectors declare `set -uo pipefail` with errexit
# OFF on purpose (they accumulate findings across greps and tools that
# return non-zero as their normal way of saying "found something"), and
# every one of them had errexit forced back on by this line.
#
# The damage is not a crash, it is a DOWNGRADE. The script dies at the first
# command that reports a finding, so the `if [ "$red" -ne 0 ]; then echo
# "...FAILED:"; exit 2; fi` block below it becomes unreachable code and a red
# verdict surfaces as a bare, unlabelled non-zero. check-gate5-attestation.sh
# had already found this and re-disabled errexit by hand — one net, in one of
# six places that needed it.
#
# Every caller sets its own options before sourcing this file. Keep it so.

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
  # TWO filters ship from this file and both decide what the ratchets see:
  # `strip_test_items` hides test items INSIDE a file, `rs_prod_files` hides
  # whole FILES. Each has its own self-test and each must prove itself, or the
  # ratchets built on it render a verdict they cannot back.
  local selftest
  [ -n "${NIKA_SKIP_FILTER_SELFTEST:-}" ] && return 0
  for selftest in \
    "${BASH_SOURCE[0]%/*}/test-strip-test-items.sh" \
    "${BASH_SOURCE[0]%/*}/test-rs-prod-files.sh"; do
    _lib_prove_one "$selftest"
  done
}

_lib_prove_one() {
  local selftest="$1"
  # Fail CLOSED. This read `[ -x "$selftest" ] || return 0`, which skipped the
  # proof and reported success when the self-test was absent OR merely not
  # executable — so `chmod -x` on one file disarmed four ratchets at once and
  # they carried on printing OK. It also tested the wrong bit: the self-test is
  # run as `bash "$selftest"` below, an invocation that never needs +x. A guard
  # that cannot prove itself must refuse to render a verdict, not emit a green.
  if [ ! -f "$selftest" ]; then
    printf 'FAIL  the shared test-item filter has no self-test at %s — this ratchet cannot be trusted\n' "$selftest" >&2
    exit 2
  fi
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

# List src files whose MODULE is declared under `#[cfg(test)]` — test-only by
# construction: the compiler never builds them outside a test profile, whatever
# their basename.
#
# `rs_prod_files` has always promised to "mirror clippy's `#[cfg(test)]`
# exclusion" and until 2026-08-18 delivered only the `tests.rs` BASENAME half.
# The other half leaked: 951 lines of test-only code across five crates were
# charged to the production budget — nika-providers 638 (`parity_tests.rs` ·
# `test_support.rs`) · nika-dap 122 (`anchor/fixtures.rs`) · nika-runtime 109
# (`adversarial/mod.rs`, whose own doc says "Test-only: no public surface, no
# production code") · nika-trace 58 · nika-verb-infer 24. That is a MISCOUNT,
# not a budget: the promise was already written above the function.
#
# The exemption is TIGHTENED, not merely widened: a module declared under
# `#[cfg(test)]` in one place AND normally in another stays production. A
# declaration only counts when `#[cfg(test)]` is the attribute that reaches it
# (other attributes and comments may sit between), so `#[cfg(test)] use …`
# never triggers it.
rs_test_only_files() {
  rs_src_files | python3 -c '
import os, re, sys

files = [l.strip() for l in sys.stdin if l.strip()]
known = set(files)

DECL = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-z_][a-z0-9_]*)\s*;")
CFG  = re.compile(r"^\s*#\[cfg\(test\)\]\s*$")
ATTR = re.compile(r"^\s*(#\[|//)")

test_roots, prod_roots = set(), set()

for f in files:
    try:
        lines = open(f, encoding="utf-8", errors="replace").read().split("\n")
    except OSError:
        continue
    stem = os.path.basename(f)[:-3]
    moddir = os.path.dirname(f) if stem in ("mod", "lib", "main") \
             else os.path.join(os.path.dirname(f), stem)
    pending = False
    for ln in lines:
        if CFG.match(ln):
            pending = True
            continue
        if not ln.strip() or ATTR.match(ln):
            continue                      # attributes/comments do not break the pairing
        m = DECL.match(ln)
        if m:
            (test_roots if pending else prod_roots).add(os.path.join(moddir, m.group(1)))
        pending = False

out = set()
for r in sorted(test_roots - prod_roots):   # declared BOTH ways ⇒ stays production
    out.add(r + ".rs")
    out.add(os.path.join(r, "mod.rs"))
    out.update(f for f in files if f.startswith(r + os.sep))

for f in sorted(out & known):
    print(f)
' || true
}

# Like rs_src_files but also excludes (a) files whose basename is `tests.rs`
# (convention: `src/tests.rs` and `src/foo/tests.rs` are 100% test code) and
# (b) files whose module is declared under `#[cfg(test)]` (see above).
# Use this for ratchets that should mirror clippy's `#[cfg(test)]` exclusion.
rs_prod_files() {
  local excluded
  excluded="$(rs_test_only_files)"
  if [ -z "$excluded" ]; then
    rs_src_files | grep -vE '(^|/)tests\.rs$' || true
  else
    rs_src_files | grep -vE '(^|/)tests\.rs$' | grep -vxF "$excluded" || true
  fi
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
      # `#[cfg(test)]` and the `all`/`any` composites (#1207). The private
      # python copies this filter replaced already knew the composite form —
      # `#[cfg(all(test, unix))]` guards the hermetic adapter fixtures in
      # nika-harness — while this one only knew the bare attribute. Reading
      # through here without teaching it that would have traded one blindness
      # for another, and reddened ten lines of real test code on main.
      #
      # `not` is excluded BY CONSTRUCTION rather than by a negative lookahead
      # POSIX awk does not have: the composite arm admits the literals `all`
      # and `any` and nothing else, so `#[cfg(not(test))]` — production code
      # that must stay visible — matches neither arm.
      #
      # The trailing class admits `)` as well as space and comma. Leaving it
      # out looked safe (it was what kept `not(test)` out) and silently
      # dropped `#[cfg(all(test))]`, valid Rust with no second predicate —
      # test code read as production. Two guards for one job is how the weak
      # one goes unnoticed: the literal list is the guard, and it is the only
      # one. Mutation-pinned in test-strip-test-items.sh from both sides.
      if ($0 ~ /^[[:space:]]*#\[cfg\(test\)\][[:space:]]*$/ ||
          ($0 ~ /^[[:space:]]*#\[cfg\([[:space:]]*(all|any)[[:space:]]*\([[:space:]]*test[[:space:],)]/ &&
           $0 ~ /\][[:space:]]*$/)) {
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
