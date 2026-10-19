- **An approval gate takes a typed yes or no.** A workflow's approval gate said `answer
  yes or no`, yet typing `oui` or `yes` only showed `nothing sent · ↑↓ choose, then Enter`:
  its field took no words. A whole-line `oui`, `yes`, `y`, `ok`, `valide` or `approuve` now
  picks Approve, `non`, `no`, `n` or `refuse` picks Refuse, and `why` or `pourquoi` picks
  Explain: the act's radio fills while the word is typed and `Enter` sends it, once. Other
  words send nothing and say so, a typed command is sent as before, and `Esc` empties the
  words and never answers no (NIK-194).
