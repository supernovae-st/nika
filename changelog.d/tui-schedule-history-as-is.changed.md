- **The workspace reads the Session's schedule histories as they are.** `ProjectView::schedules`
  now holds `nika_session::ScheduleHistory` itself (built with its `new`), and the TUI's own copy
  of it (`ScheduleView`, `SlotView`, both added in this preview) is gone. Whether this machine
  holds a schedule's unit is the lent trigger host's answer (`TriggerHost::installed`), so a test
  host proves the rail's « Actif ● » without touching launchd. Nothing a person sees changes.
