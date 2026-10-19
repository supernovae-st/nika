- **A Session's last run names its job, receipt, outputs and the verifier's seal facts; `sealed`
  keeps its meaning.** `work.run` gains `job`, `receipt`, `outputs` (kept whole up to 16 KiB
  encoded, absent past it: the job keeps them) and `seal_verified` / `signed` as `nika trace
  verify` attests them for the run's journal, absent when no verifier judged it. The Session
  judges a native run's local journal when it observes the run and holds those facts beside its
  kept value, which stays the one an earlier engine reads; an opaque trace identity is never read
  as a local journal. `sealed` still names what the Stop's story reads, the settlement's receipt
  naming the chain head: a run that stopped at a wave boundary without a run key reads stopped,
  never cut, and no verifier's judgment is read from it.
