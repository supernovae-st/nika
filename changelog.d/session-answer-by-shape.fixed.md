- **An answer that is the question's own shape binds without a routing call.**
  A `provider/name` typed alone at the model question, or an offered key
  typed alone at a choice, was first sent to the turn classifier before it
  bound. Such a line cannot change the request, ask about the question,
  run or cancel: it now binds at once, and the route records it as an
  answer by protocol. A sentence around the same value is still read.
