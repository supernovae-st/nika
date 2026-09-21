- **A key rename and a row limit are typed stages of the computation, and a ranking without
  its count asks for it.** « rename item to product », « the top 3 by units », « les 5 plus
  vendus » had no typed form: the seat fell back to a language-model draft that kept the
  old keys or invented rows (sealed v2 key-rename and ranking seeds), and « die
  meistverkauften Artikel » with no count compiled to a full descending sort. The typed
  computation now carries `renames` (a source column to a name the request states, lowered
  as `with_entries`) and `limit` (a number the request states, as digits or as a word,
  lowered as `.[:N]` after the sort); a descending sort under a ranking word with no count
  asks `const.top_n` instead of assuming one, and the answer bounds the sort. A renamed
  header is never overwritten by the source's column order.
