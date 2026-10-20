- **The run card speaks the person's language.** A French conversation's run card now reads
  « Terminé · `x.nika` · 9 étapes exécutées », « produit · … », « coût · 0,0004 $ · 1 appel
  tarifé », « plafond · 0,10 $ · le plus que ce run pouvait coûter » and « Échoué · … · `t` a
  échoué » (`RunFacts::result_in`). A run whose gate the person refused says « arrêté à votre
  « non » » rather than a plain success. A failure's code is said once. The failure card no
  longer tells the person to repair a workflow whose step failed for an outside cause: it asks
  for a repair only if the cause is the workflow's own.
