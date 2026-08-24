#!/usr/bin/env python3
"""The prod-LOC counter proves itself before it guards a wall.

check-crate-size.sh runs this and REFUSES to render a verdict if it fails —
the `_lib.sh` discipline, fail CLOSED. A budget nobody can demonstrate is a
number people argue with, and the argument is settled by deleting comments.

Both directions are pinned, and the second is the one that matters. Inventing
production lines inflates a budget loudly. HIDING production lines is silent,
and the same parser shape does both.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from importlib import import_module  # noqa: E402

prod_loc = import_module("prod-loc")

fails = 0
cases = 0


def expect(want: int, src: str, label: str) -> None:
    global fails, cases
    cases += 1
    got = prod_loc.prod_lines(src)
    if got == want:
        print(f"  ok   {label} ({got})")
    else:
        fails += 1
        print(f"  FAIL {label} — wanted {want}, got {got}", file=sys.stderr)


print("prod-loc · what the 15k wall is allowed to charge")

# --- the baseline shape ------------------------------------------------------
expect(
    3,
    'fn a() {\n    let x = 1;\n}\n',
    "plain code counts every line",
)
expect(
    1,
    'fn a() {}\n#[cfg(test)]\nmod t {\n    fn b() {}\n}\n',
    "a cfg(test) module is not production",
)

# --- BUG 1 · braces inside string literals -----------------------------------
# THE measured case, verbatim in shape from nika-runtime/src/expr.rs:642. The
# old counter read four unmatched `}`, decided the test module had closed, and
# charged everything after it to production.
expect(
    1,
    'fn a() {}\n'
    "#[cfg(test)]\n"
    "mod t {\n"
    '    fn b() { assert_eq!(f(" \\"}}\\" }}", 0), Some(6)); }\n'
    "    fn c() {}\n"
    "}\n",
    "braces in a STRING literal do not end a test module",
)
expect(
    1,
    'fn a() {}\n#[cfg(test)]\nmod t {\n    const S: &str = "}}}}}}";\n    fn c() {}\n}\n',
    "a string of nothing but closing braces does not end it",
)
expect(
    1,
    'fn a() {}\n#[cfg(test)]\nmod t {\n    const S: &str = r#"} } }"#;\n    fn c() {}\n}\n',
    "a RAW string of closing braces does not end it",
)
expect(
    1,
    "fn a() {}\n#[cfg(test)]\nmod t {\n    let c = '}';\n    fn c2() {}\n}\n",
    "a CHAR literal brace does not end it",
)
expect(
    1,
    'fn a() {}\n#[cfg(test)]\nmod t {\n    // } } }\n    fn c() {}\n}\n',
    "braces in a LINE comment do not end it",
)
expect(
    1,
    'fn a() {}\n#[cfg(test)]\nmod t {\n    /* }\n    } */\n    fn c() {}\n}\n',
    "braces in a multi-line BLOCK comment do not end it",
)

# --- BUG 2 · the attribute on a DECLARATION ----------------------------------
# `#[cfg(test)] mod foo;` governs that one item. The old counter, finding no
# `{` on the line, waited and adopted the NEXT block it met — here `fn real`,
# which is production. Silent, and the direction that hides code.
expect(
    3,
    "#[cfg(test)]\nmod t;\nfn real() {\n    let x = 1;\n}\n",
    "a cfg(test) DECLARATION does not swallow the next block",
)
expect(
    1,
    "#[cfg(test)]\nuse std::fmt;\nfn real() {}\n",
    "a cfg(test) `use` does not swallow the next block",
)

# --- production AFTER a test module still counts -----------------------------
expect(
    1,
    "#[cfg(test)]\nmod t {\n    fn b() {}\n}\nfn after() {}\n",
    "production after a test module is counted",
)

# --- comments and blanks ARE production --------------------------------------
# Deliberate. The wall is a maintainability budget; discounting comments would
# make deleting them the cheapest way under it — which is the defect #1203
# named, one layer down.
expect(
    4,
    "/// doc\n\n// note\nfn a() {}\n",
    "doc comments and blank lines count as production",
)
expect(
    1,
    "#[cfg(test)]\nmod t {\n\n    // a test comment\n    fn b() {}\n}\nfn a() {}\n",
    "comments and blanks INSIDE a test module do not count",
)

# --- the attribute variants --------------------------------------------------
expect(
    1,
    "fn a() {}\n#[cfg(all(test, unix))]\nmod t {\n    fn b() {}\n}\n",
    "cfg(all(test, ...)) is a test module",
)
expect(
    3,
    "fn a() {}\nimpl T {\n    #[test]\n    fn b() {}\n}\n",
    "a bare #[test] fn is not production",
)

print(f"\n{cases} case(s) · {fails} failure(s)")
sys.exit(1 if fails else 0)
