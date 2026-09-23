- **An answered rule is the literal program the Check preview compiles.** The morning audit of
  2026-09-22 saw a jq expression asked to a human accepted without validation, `nika check`
  clean, and the run failing: the assembler bound the answer as `${{ const.rule_expression }}`,
  a templated program the static jq compile-check (NIKA-VAR-005) never reads. The answer is now
  the compute task's own literal expression, so a program that does not parse refuses the
  candidate at compile time with the analyzer's one-line reason; the const disappears.
