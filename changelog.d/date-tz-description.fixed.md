- **`nika:date` documents every op that reads `tz:`.** The tool catalog
  described `tz` as applying to `now` only, so `op: format` looked unable
  to render local time, and workflows sliced ISO strings by hand instead.
  `format` renders its fields in `tz:` and `add`/`subtract` apply calendar
  units there. The description now says so, and adds that an input's own
  offset is not kept when `tz:` is absent.
