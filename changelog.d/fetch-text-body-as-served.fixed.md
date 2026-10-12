- **`nika:fetch` returns a CSV, text or JSON body as served in its page modes.** The
  default `markdown` mode, like `article` and `text`, read every body as HTML: a
  `text/csv` export came back on one line with markdown escapes (`A\_1`), so a following
  `nika:convert from: csv` produced no rows while the run succeeded, and a `text/plain`
  body lost its line breaks. A body whose `Content-Type` is text other than HTML
  (`text/*`, JSON, XML, CSV, YAML, NDJSON, TOML and their `+json`/`+xml`/`+yaml`
  suffixes) now comes back unchanged, decoded by its BOM or declared charset, else as
  UTF-8, else as windows-1252. HTML, XHTML and untyped bodies keep the extraction.
