- **Stop asking which observed field a key means when the observation already answers it.**
  A compile no longer asks which observed field a seat key means when the request states the
  key value and the host observed that value in that key alone. « Sum integer amount_cents of
  paid rows by customer » names `paid`, never `status`: the seat reads it as `status == "paid"`,
  as a typed clause or as a verified program comparing `.status` to `"paid"` literally, and the
  host recorded `paid` among the values of `status` and of no other column, yet the round asked
  « Which observed field does `status` mean? ». The key is now bound by the observation: the
  grounding entry records `bound_by: observation` and the witness literal, the key keeps its
  grade, and an answer to its question is still read first. The question stays when the value
  is recorded in two columns or absent from the recorded sample, when the column has no
  recorded values, when the request never states the value, when the value also names a
  column, when the key is not observed, when the rule only tests containment, or when a program
  holds the value anywhere but in that comparison. A type the rule needs, such as an instant
  window, is still asked.
