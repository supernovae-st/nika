- **A rehearsal holds the folders and patterns the request names.** A request naming a folder
  (« inbox/ ») was not rehearsed: the room refused the folder as no regular file, and
  `nika:glob` was outside what a rehearsal runs. The room now copies a named folder with each
  file under it, and makes an empty one empty: the empty day is rehearsed too. It copies a named
  pattern by its matches. A rehearsal runs `nika:glob` and `nika:remove_file` over its own copy. A
  listing that no named folder or pattern holds is refused before any room, as a read is.
