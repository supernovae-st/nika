- **The run ceiling stays the person's, and a run that would pass it does not start.**
  - « Mets le plafond à 0,001 $ », with nothing waiting, sets the saved workflow's next runs in
    this conversation. The Session says so itself, and the run card names it (« the one you set
    for its next runs »). Before, the conversation's model acknowledged it and the next run kept
    the old ceiling.
  - A new request that states no ceiling keeps the one the person stated earlier in the
    conversation, instead of the Session's $0.25 default. Before, a copy of a workflow ran under
    $0.25 after « budget max 0,10 $ par run ».
  - A run whose last run of the same bytes cost more than its ceiling is refused before it
    starts, with the ceiling to give instead. Before, it stopped partway and wrote nothing.
  - A run its ceiling stopped says « Arrêté · le plafond est atteint : 0,0035 $ dépensés pour un
    plafond de 0,0010 $ » with what never ran. It no longer asks to repair the workflow
    (`RunFacts::result_in`, `spent_usd`, `stopped_by_ceiling`).
  - A run cost under a tenth of a cent shows two significant digits ($0.00015, not $0.0002).
