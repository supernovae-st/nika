- **A conversation over its model's window folds and goes on.** When a request is over the
  model's context window, or a reply filled it, the Session folds the conversation once and asks
  again; a second overflow fails and names the window. A long run from one line can now be folded
  between its steps, a summary that would not shrink the conversation is not used, and a
  conversation too large for one summary request is summarized in whole-object parts within
  its estimated payload budget; an indivisible object or fixed prompt over budget is reported
  without folding originals.
