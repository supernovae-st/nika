- **A Session asks the reasoning effort you name for every LLM call it makes.** Set
  `NIKA_AUTHORING_REASONING` (`low`, `high` or `max`), or a host's typed setting. On a
  DeepSeek route whose catalog lists that level (`deepseek-v4-pro`), every authoring and
  repair request, every turn-routing label (a host's own classifier included) and every
  conversational turn is then sent with thinking enabled and that effort. Caps and
  temperature are unchanged, and `/status` says so. Any other word is refused. A route, a
  subscription seat, a reasoner or a classifier that cannot carry the level refuses it
  before anything is sent. The operator-selected TypeSafe decision seat is a separate
  backend: no effort is sent to it or claimed for it. With no effort named, requests are
  sent as before.
  **`/details` states each explicit effort as the receipt recorded it.** For every call,
  it shows the level asked, the keys read back from the body sent (`unobserved` when
  none was read back), the reported usage and model, and that the effort the provider
  spent internally is unknown. A call refused before sending is never counted as sent,
  and a call with no recorded answer is never counted as answered.
