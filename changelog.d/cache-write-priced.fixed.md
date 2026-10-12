- **A call that wrote to the provider's prompt cache keeps its price.** Nika priced a call
  that reported a cache write as unknown. It now prices the written tokens at the catalogue
  row's own cache-write rate (the five-minute one, the only lifetime Nika asks for). A row
  that states no write rate, a declared tariff and an admission tariff still leave such a
  call unknown, as do two write meters that disagree.
