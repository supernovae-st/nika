- **Refuse a candidate whose jq orders observed date-times with offsets as text.**
  A `nika:jq` comparison or sort over a record field compares text, and text order is not time
  order across offsets: `2026-09-01T02:30:00+02:00`, which is 00:30 UTC, sorts after the upper
  bound `2026-09-01T01:00:00` of the hour it lies inside, so a first-hour total came out 0
  instead of 8. When the host observed the field's values as ISO-8601 date-times in more than
  one offset or form (`Z` and `+00:00` count as two), or the compared bound is a date-time in
  another offset or form, the candidate is now refused (`TEXT ORDER ON INSTANTS`) with the
  repair: compare `fromdateiso8601` instants against a bound written with its offset. Values
  that share one offset and one form against a bound of the same offset, converted instants, a
  date-only bound and fields that hold no dates stay admitted. The evidence is the categorical
  values the host records, so a column whose sampled values are all distinct is not judged yet.
