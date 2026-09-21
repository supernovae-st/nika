- **A stated data shape is the code the workflow runs.** « count the rows
  per client », « merge them on the id column », « remove the duplicate
  lines and write the unique ones », « keep the 2 rows with the highest
  amount », « sort the rows by amount descending », « trie les lignes par
  montant décroissant » and « keep only the id and title of each ticket »
  compile on the deterministic door with no model: a closed grammar of the
  stated stages (a count or an aggregate per column, a sort with its key
  and direction, a top-N with its measure, a projection of listed fields, a
  removal of duplicates, a join of the read sources on one column) lowers
  into one typed shape, a headless clause the grammar reads whole is a
  compute step carrying its rule, a sort on a source column compares the
  number a CSV cell holds, a removal of duplicates over a text file runs
  over its lines, and an exclusion lead (« exclude », « drop », « supprime
  … ») is read as nothing, never inverted. A form that lacks its key, its
  measure or its column is asked, nothing is guessed.
