- **Arm a beat on the last day of each month, or every N weeks from a date.**
  A cadence can now say two things a plain cron cannot. `L` as the whole day-of-month field is
  the last day of each month (`TZ=Europe/Paris 0 9 L * *`: the 28th, 29th, 30th or 31st,
  whichever ends that month), and `every N weeks from DATE HH:MM` is an anchored interval
  (`TZ=Europe/Paris every 2 weeks from 2026-10-05 09:00`, `N` from 1 to 52), whose slots keep
  their civil time across a clock change. Neither is approximated: `1,L`, `L-2`, `LW` and an
  interval without its anchor date are refused by name, because the anchor says which week is
  on. systemd units say the month end exactly; launchd units, and both targets for the interval,
  wake a little more often and the firer fires only the real slot. `nika arm` readiness reports
  the zone and the next fire of both forms.
