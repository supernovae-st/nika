- **Refuse non-finite numbers in `tonumber` and share one set of jq std shadows.**
  `tonumber` now emits exactly one finite number. `NaN`, `Infinity`, `-Infinity` and an
  overflowing text such as `1e400` stop with the named error `tonumber: not a finite number`
  before any predicate, comparison, sort, minimum, maximum or aggregate reads the value. Before,
  `select(. > 5)` dropped NaN, `select(. < 0)` kept it, and a sum became an infinity; the
  message prints no cell text. An explicit `try` or `?` still skips the operand. The `nika:jq`
  builtin, output bindings, the static checker and the compile verifier now load one definition
  (`nika_cap::JQ_STD_SHADOWS`), so output bindings gain the global `scan` they lacked, and all
  four run one probe set (`nika_cap::JQ_STD_SHADOW_PROBES`). Limits: `fromjson` still reads
  NaN, the infinities and overflowing decimals, and values a program computes (`infinite`,
  `1 / 0`, `pow`) are not guarded.
