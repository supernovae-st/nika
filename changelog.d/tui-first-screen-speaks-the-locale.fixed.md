- **The first screen speaks this machine's language.** On a French machine the top bar and the
  first screen read English (« Prepare with: … », « No workflow in … yet ») until the person's
  first French line. Before the person writes, the TUI's own words now take the locale (the first
  of `LC_ALL`, `LC_MESSAGES` and `LANG` that is set): « Préparer avec : … ». The person's words
  still decide from their first line on, and a conversation resumed in French speaks French.
