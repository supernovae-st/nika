- **A run's card shows one task count and one cost.** The card under a finished run said
  « ✔ Ran in 1.7s · 20 tasks · $0.005 » while the run's result beside it said « 9 tasks ran (6
  from cache) · 2 skipped » and « cost · $0.0046 ». A run as small as « <$0.001 » read
  « cost · $0.0004 » there too. The card now counts tasks as the result does: the tasks that
  ran, those answered from cache, then those skipped, failed or never run. It also writes every
  cost on the card at the result's precision (`$0.0046`).
