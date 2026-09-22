- **Duplicates by stated key columns are a typed stage of the computation.** Sealed sv3-02
  (« vire les entrées qui ont le meme titre ET le meme artiste qu'une entrée précédente (garde
  la 1ere), garde l'ordre ») never compiled on any lane: the seat's typed computation could
  not say the key (the shape knew whole-row duplicates only), the write had no producer and
  the order constraint no carrier. The shape gains `distinct_by`, lowered right after the
  filter as a reduce that keeps the first occurrence in place with every column, recorded and
  replayed; the seat's instruction and schema name it, and a key is admitted only when the
  request names the column. A constraint that keeps the row order (« garde l'ordre », « keep
  the order », « mismo orden », six languages) is carried by the compute task, which keeps the
  source order by construction.
