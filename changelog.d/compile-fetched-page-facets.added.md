- **A facet of a fetched page is the fetch's own mode, never a draft.**
  « Fetch https://example.com/ and write the page title to ./title.txt »,
  « save the article text to ./page.md » and « écris le titre de la page
  dans ./titre.txt » compile with no model: after a fetch, an object made of
  page words, facet words and links names a facet of the fetched page, and
  each facet is one extract mode of `nika:fetch` (`metadata` for the title
  and the description, `article`, `text`, `markdown`, `raw`, `links`); the
  assembler fetches in the modes the writes need, `permits.net.http` is the
  fetched host and nothing else, and a page with no title fails the write
  loudly rather than writing a guess. `save` and `store` join the English
  write heads.
