- **Read number texts in the compiled laws without tripping the finite `tonumber`.**
  The number law of a compiled workflow and the order and arithmetic laws that read a decimal
  text now read an accepted text with `fromjson`. A text such as `1e999` stops with the law's
  own named error (`amount` is "1e999", not a number) instead of the generic
  `tonumber: not a finite number`, `1e400` keeps its exact digits for ordering, and an integer
  that no finite number carries is written as no number instead of failing to parse. Every
  finite value reads exactly as before.
