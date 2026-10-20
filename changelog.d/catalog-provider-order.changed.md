- **`nika catalog` lists providers in the spec's presentation order.** Within its
  LOCAL and CLOUD sections the rows follow the vendored design tokens'
  `presentation.providers_order` (local engines first, then Mistral, Hugging Face,
  OpenAI, xAI, Anthropic, DeepSeek, Google, Groq, NVIDIA and OpenRouter), the
  order the language server already teaches; a provider the tokens do not name
  comes after every named one, by id. The command keeps no order of its own.
