- **GPT-6 Sol and GPT-6.1 Sol take `max_completion_tokens`.** OpenAI refused them with « 'max_tokens'
  is not supported with this model » (HTTP 400) on the first message: the catalog knew the
  parameter only for GPT-6 Astra. Both observed models now send `max_completion_tokens`; the rule
  stays exact, with no claim for the rest of the family.
