- **A Session asks the reasoning effort you name for every call it makes.** Set
  `NIKA_AUTHORING_REASONING` (`low`, `high` or `max`), or a host's typed setting. On a
  DeepSeek route whose catalog lists that level (`deepseek-v4-pro`), every authoring and
  repair request, every turn-routing label and every conversational turn is then sent
  with thinking enabled and that effort. Caps and temperature are unchanged, and
  `/status` says so. Any other word is refused. A route, a subscription seat or a
  reasoner that cannot carry the level refuses it before anything is sent. With no
  effort named, requests are sent as before.
  **`/details` states each explicit effort as the receipt recorded it.** For every call,
  it shows the level asked, the keys read back from the body sent (`unobserved` when
  none was read back), the reported usage and model, and that the effort the provider
  spent internally is unknown.
