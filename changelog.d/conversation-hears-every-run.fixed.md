- **The conversation knows about a run refused before it started.** Only a run that ended (exit 0
  or 1, with a readable trace) reached the conversation's author. A run refused before it started
  (exit 2, its cost floor above the ceiling) or around it reached nothing, so « pourquoi le run a
  échoué ? » right after « NIKA-1709 · refusing to start » met « aucun run n'a échoué ». Every
  observed run now rides the person's next line to the author, as a success already did; a failed
  run of the author's own candidate is still repaired at once.
