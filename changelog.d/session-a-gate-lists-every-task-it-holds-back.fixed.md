- **A gate lists every task it holds back, at any depth.** The aside beside a
  paused run (« what your answer lets happen ») listed the gate's direct
  dependents only: behind « gate → format → send » the human saw the
  formatting and never the send. It now lists the closure — every task that
  follows the gate through an `after:` edge or a `with:` binding, and every
  task that follows one of those — in the workflow's order. Two effects
  behind one gate are both shown (acceptance A10).
