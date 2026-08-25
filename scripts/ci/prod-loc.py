#!/usr/bin/env python3
"""Count PRODUCTION lines of a Rust file — src/ minus in-file #[cfg(test)].

Reads paths on stdin, prints `<count>\t<path>` per line.

This is the counter behind the 15k crate-size wall. It used to live inline in
check-crate-size.sh and it was blind in two ways, both measured on the tree
2026-08-25 while qualifying #1203:

  1. BRACES INSIDE STRING LITERALS.  It stripped char literals and nothing
     else, so this line — real, at nika-runtime/src/expr.rs:642 —

         assert_eq!(nika_tmpl::find_island_close(" \\"}}\\" }}", 0), Some(6));

     fed it four closing braces that no code ever opened.  Depth went
     negative, the tracker decided `mod tests` had ended, and the remaining
     412 lines of that test module were charged to the PRODUCTION budget.
     nika-runtime measured 14994/15000 when ~14583 was the truth.

  2. `#[cfg(test)] mod foo;` — an attribute on a DECLARATION, not a block.
     Seeing no `{` on that line, it kept waiting and adopted the NEXT block
     it met, whatever that block was, as the test region.  That direction is
     the dangerous one: it hides production lines instead of inventing them.

The direction of bug 1 is loud (a budget inflates and someone eventually
argues with the number).  The direction of bug 2 is silent.  A counter that
can be wrong in both directions at once is not a budget, it is a rumour — and
this one guards a wall whose stated purpose is maintainability, so the lines
it wrongly charges get paid for in deleted doc comments.

`_lib.sh::strip_test_items` (awk) had already learned rule 1 on 2026-08-02 and
rule 2 by construction; three python copies — here, check-unwraps.sh and
check-expect.sh — never did.  This file is the one copy that is now correct AND
proven (scripts/ci/test-prod-loc.py).  The other two are filed, not fixed here:
they judge different things and changing their verdicts is its own change.

WHAT COUNTS: every line not inside a `#[cfg(test)]` / `#[test]` item — blank
lines and comments INCLUDED.  That is deliberate.  The budget is a
maintainability budget; a comment is production text that a reader must read,
and a counter that discounted comments would reward deleting them.
"""

import re
import sys

# `#[cfg(test)]`, `#[cfg(all(test, ...))]`, `#[cfg(any(test, ...))]`, `#[test]`.
CFG_TEST = re.compile(r"#\[cfg\(\s*(?:(?:all|any)\s*\(\s*)?test\b")
BARE_TEST = re.compile(r"#\[test\]")


def code_only(line: str, state: dict) -> str:
    """The line with strings, raw strings, char literals and comments removed.

    Only braces in real CODE may move the depth counter. `state` carries the
    multi-line modes (block comment, raw string, plain string) across lines.
    """
    out = []
    i = 0
    n = len(line)
    while i < n:
        c = line[i]
        if state["block"]:
            if c == "*" and line[i + 1 : i + 2] == "/":
                state["block"] = False
                i += 2
            else:
                i += 1
            continue
        if state["raw"]:
            if c == '"' and line[i + 1 : i + 1 + state["hashes"]] == "#" * state["hashes"]:
                state["raw"] = False
                i += 1 + state["hashes"]
            else:
                i += 1
            continue
        if state["str"]:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                state["str"] = False
            i += 1
            continue
        if c == "/" and line[i + 1 : i + 2] == "/":
            break  # line comment — nothing after it is code
        if c == "/" and line[i + 1 : i + 2] == "*":
            state["block"] = True
            i += 2
            continue
        # raw string: r"..." · r#"..."# · br##"..."##
        m = re.match(r'b?r(#*)"', line[i:])
        if m and (i == 0 or not (line[i - 1].isalnum() or line[i - 1] == "_")):
            state["raw"] = True
            state["hashes"] = len(m.group(1))
            i += m.end()
            continue
        if c == '"':
            state["str"] = True
            i += 1
            continue
        # A char literal closes; a lifetime (`'a`) never does.
        m = re.match(r"b?'(?:\\.|[^'\\])'", line[i:])
        if m:
            i += m.end()
            continue
        out.append(c)
        i += 1
    return "".join(out)


def prod_lines(text: str) -> int:
    lines = text.split("\n")
    # `"a\nb\n".split("\n")` is `['a', 'b', '']` — three elements for two
    # lines. The inline counter this replaced charged that phantom to the
    # budget, once per file: ~45 lines on nika-runtime alone, and nobody could
    # see it because it never appeared in any file. `wc -l` semantics.
    if lines and lines[-1] == "":
        lines.pop()

    total = 0
    depth = 0
    test_entry = None  # brace depth at which a #[cfg(test)] item opened
    pending = False  # saw the attribute, waiting for `{` or `;`
    state = {"str": False, "raw": False, "hashes": 0, "block": False}

    for line in lines:
        code = code_only(line, state)
        stripped = line.strip()
        if not pending and test_entry is None and (CFG_TEST.match(stripped) or BARE_TEST.match(stripped)):
            pending = True

        in_test = test_entry is not None or pending
        if not in_test:
            total += 1

        for c in code:
            if c == "{":
                depth += 1
                if pending:
                    test_entry = depth
                    pending = False
            elif c == "}":
                depth -= 1
                if test_entry is not None and depth < test_entry:
                    test_entry = None
            elif c == ";" and pending:
                # `#[cfg(test)] mod foo;` — a DECLARATION, not a block. The
                # attribute governs that item alone; adopting the next block
                # we happen to meet would hide arbitrary production code.
                pending = False
    return total


def main() -> None:
    for path in sys.stdin.read().split():
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        print(f"{prod_lines(text)}\t{path}")


if __name__ == "__main__":
    main()
