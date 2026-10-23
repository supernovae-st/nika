- **An answer at a gate is recorded when the paused run spent part of its ceiling.** A run that
  paused at a `nika:prompt` gate after a priced step resumes under what its ceiling leaves, and
  its admission priced the whole workflow again, the finished step included. « non » and « oui »
  were both refused (`NIKA-1709 · … unavoidable cost floor $0.196608 exceeds --max-cost-usd
  $0.195214`), the answer was never recorded and the run stayed paused, from the Session, the
  terminal and `nika run --resume <trace> --answer <task>=<value>` alike. The CLI preflight and
  the runtime's own launch gate now price the work still to run: a task the resumed journal
  completed is a cache hit and costs nothing, with any child workflow or priced builtin it
  calls, and a branch the answer closes was already priced closed. What still runs is judged
  as before, and a fresh run's floor is unchanged.
