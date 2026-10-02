- **Read the last day of a month and an interval of weeks from its start date as a schedule.**
  A request whose trigger says « the last day of every month at 18:00 » or « le dernier jour de
  chaque mois à 18h » now proposes the cadence `0 18 L * *` on `requested_trigger.cron`, and
  « every other Monday at 09:00 from 2026-10-05 », « every 2 weeks on Monday at 9:00, starting
  2026-10-05 » or « toutes les deux semaines le lundi à 9h à partir du 2026-10-05 » proposes
  `every 2 weeks from 2026-10-05 09:00`, the forms the arming grammar holds. An interval of
  weeks without its start date is still asked, and the question now says the start date is what
  is missing; a start date on another weekday than the one named, a date that does not exist or
  a date not written YYYY-MM-DD proposes nothing. The binding adds the zone and validates the
  proposal with the cadence grammar, as for every other schedule. The deterministic reader
  keeps these words in the trigger head, a start date after « from », « starting » or « à partir
  du » included, even after the comma that closes the head.
