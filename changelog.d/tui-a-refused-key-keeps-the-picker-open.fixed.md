- **A connection whose key was refused is not put in use.** After « Test the key » said OpenAI
  refused the key (401), the picker still let the person keep or choose that connection, and
  every request then failed with the same 401. Keeping it, choosing its row or one of its models,
  or typing over the list while it is the one in use now says « clé refusée par OpenAI (401) · ce
  choix n'est pas appliqué · vérifiez la clé ou choisissez une autre connexion ». The list stays
  open for another choice, and typed words come back to the box. A later key check that passes
  lifts it.
