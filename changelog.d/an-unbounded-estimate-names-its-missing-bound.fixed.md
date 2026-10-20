- **An unbounded estimate names what bounds it not.** « model output estimate · unbounded: a
  token or iteration bound is missing » stood beside a declared `max_tokens: 4096`, when only
  the number of items a step fans over was unknown. The line now names the tasks that declare
  no `max_tokens`, the tasks that run once per item of a list known only at run time, and a
  composed workflow with an unbounded step.
