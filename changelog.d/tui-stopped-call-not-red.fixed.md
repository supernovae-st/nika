- **A call you stopped no longer reads as a failure in ASCII.** In the ASCII glyph column, a call
  stopped by the person read « x call 1 - … - stopped, a charge may be unknown », and the TUI
  painted it in the failure's red, since « x » is also the failure's ASCII mark. A stopped call
  now reads « - call 1 - … », never red, as its Unicode « ✖ » row already was not.
