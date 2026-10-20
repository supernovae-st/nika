- **A revision says what changed in its bytes, and when nothing did.**
  QA gauntlet q03 and q04: « Changed from the proposal you saw: model
  claude-haiku-5-5 added » and the author's « J'ai retiré cette ligne »
  appeared over bytes and a proposal id that never changed, because the
  line came from the values the author bound. A change is now said only
  where the bytes gained or lost that value, with the size of the diff
  (or its first lines), and the very same bytes read « Same workflow as
  the proposal you saw: its bytes did not change. »
