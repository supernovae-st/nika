- **An `unwind` cleanup now spends under the run's own authority.** A cleanup
  task used to dispatch with no ledger, no budget and no guard. Under
  `--max-cost-usd 0`, a cleanup whose `model:` its producer's output decides
  reached the provider, while the same call as an ordinary task was refused
  (NIKA-1704). What a cleanup sent never reached the run's spend receipt, so
  a run could report `unmetered` after a paid request. A cleanup
  `invoke: workflow:` child started with no budget at all. A cleanup now rides
  the run's ledger like any task. It meets the same pre-send guard: the
  refusal comes before any byte and is journaled on the cleanup lane. Every
  request it sends is counted once: at its price when answered, or as an
  unknown charge when it failed or when the cleanup's own timer dropped it.
  A cleanup child runs under the run's remaining budget. Once the run's
  budget is crossed, a cleanup that can spend (a model call, an agent, a
  child workflow, image or speech generation) is refused before dispatch,
  while housekeeping cleanups still run. A cleanup stays best-effort: none
  of this fails its run. The remaining budget the guard reads is still a
  snapshot, never a reservation.
