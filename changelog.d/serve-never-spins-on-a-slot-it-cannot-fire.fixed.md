- **`nika serve` no longer spins on a slot it cannot fire.** When the CLI firer had
  already fired a project beat's slot, or held its beat lock, or the workflow failed
  admission, or the queue was full, the slot stayed due in the resident's own store and
  the scheduler woke again at once, re-admitting the workflow in a busy loop: an idle
  resident burned seconds of CPU per minute until the slot aged out. A slot the CLI
  firer answered is now recorded as skipped (`answered_elsewhere`), and any other slot
  that stays due is looked at again after the 5-second rescan. A slot that comes due
  while the schedule's previous run still runs is now skipped once under
  `chevauchement: sauter` (the default), instead of firing late when that run ends.
