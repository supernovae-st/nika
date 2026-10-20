- **Carriage returns leave a model's call arguments.** GLM 5.3 Flash slipped carriage returns
  between the words of its calls (`schedule\r_zone`, `Indian\r/Mauritius`) and copied its own
  earlier calls with more of them each time: 115 calls were refused for a key or a time zone that
  did not match. They are removed from every call's arguments before the tool runs and before the
  call is recorded, so the model never reads them back.
