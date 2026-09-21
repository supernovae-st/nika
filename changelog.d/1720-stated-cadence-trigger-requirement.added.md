- **A stated cadence is a trigger requirement beside the candidate.**
  « Every morning at 9, read ./inbox/*.md and write a digest to ./digest.md »
  no longer drops its first clause into a Ready: the trigger is deployment,
  not workflow, so the candidate's bytes stay trigger-agnostic and the
  outcome carries `requested_trigger` next to `requested_boundary` (`kind`
  manual · schedule · webhook · event, the phrase verbatim, the cadence and
  time of day the words state in five languages, the payload input an event
  supplies, `status: requires_binding`), projected on the wire and the Serve
  schema and named in an `Applied` diagnostic for whoever binds it through a
  schedule or an ingress. A phrase with neither a cadence word nor a time of
  day is an event, never a guessed cron.
