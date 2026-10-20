- **A run whose turn ended never reads running.** When a run's turn ended before its
  settlement frame arrived (an environment that refused it, for one), the header, the
  pinned row and its task cards kept `◐ running` for as long as the session lasted, while
  the report on the same screen said `exit 3`. No frame outlives its turn, so such a run
  now reads `ended · no settlement received` (`ended · no frame received` when nothing
  arrived), and a task it saw start reads `no end received`, never running. A settled run
  keeps its settlement.
