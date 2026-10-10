- **A feed item dated only by its revision keeps its date.** `mode: feed`
  dropped an Atom entry's `<updated>` and a JSON Feed item's
  `date_modified`, so an item carrying only that date looked undated and
  a filter on today's items dropped it. Each item now carries `updated`
  when the source dates a revision of its own, an RFC 3339 instant like
  `published`; a date filter reads `published // updated`. An RSS item's
  `pubDate` stays its `published` alone.
