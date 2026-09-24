- **Unnamed output files require a destination.** A request that asks for a
  file without naming it (« Résume mes notes dans un fichier », « summarize
  my notes into a file ») no longer compiles READY as a draft that writes
  nothing. The deterministic reader records the write, and the compiler asks
  its exact path (`const.output_path`) before any candidate or grant exists.
  A plan recorded by an earlier engine gains that question on its next
  answer round. The native and sketch doors refuse a candidate that drops
  such a write (`UNWRITTEN DESTINATION`). An answer that names no file
  (prose) keeps the path question open instead of leaving the compile with
  nothing to answer. A named destination, a file the request already has («
  dans le fichier », « in my file ») and quoted words compile as before.
