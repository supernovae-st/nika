- **A rehearsal tries the steps after a model step on an example of its answer.** A trial ran
  no model step, so it never ran the steps after one either. A workflow could pass its check and
  its trial, then fail its first run in a step that read the model's answer, or a date beside it,
  the wrong way. When a model step declares its answer's shape (`schema:`, `returns:`), a second
  pass stands an example of that shape in for the answer, over the room's files, writing nothing.
  A step behind it that fails on the shape (a jq type error, a binding that reads beneath a value)
  fails the trial with `NIKA-REHEARSAL-SHAPE`. A failure of content (an assertion, an empty
  filter) is the example's, never counted. Nothing this pass computes is shown as a result.
