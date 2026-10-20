- **Typing over a long `/intelligence` list is instant again.** With 655 rows open, each
  keystroke cost about 260 ms in a debug build: every frame compared the whole list once
  per row to learn which boxes were checked, and the filter lowercased every row's words
  on every key. The list now builds its lowercased search index once when it opens,
  reads the checked set with one comparison, and draws once per burst of waiting keys
  instead of once per key. A debug-build regression proof types 40 keys over 1,000 rows
  in three rounds and holds each keystroke's own work (edit, filter and one whole frame)
  in the quickest round to an average and a median of 25 ms, so tests running beside it
  cannot fail it; it measured 145 ms before and about 3 ms after.
