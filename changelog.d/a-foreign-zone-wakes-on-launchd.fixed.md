- **A schedule in another zone than the Mac's can be armed with launchd.** launchd fires in the
  machine's zone, so `nika arm --emit launchd` refused a cadence such as `TZ=Europe/Paris 0 8 * *
  *` on a Mac set to another zone. The unit now wakes at each machine hour one of the schedule's
  slots can fall on over the coming year (10:00 and 11:00 on a Mauritius Mac for 08:00 Paris,
  summer and winter time). The firer reads every wake in the schedule's own zone: it runs the due
  slot and leaves the other wakes not due, without journaling them.
