- **A model the seat prices is the seat's own model.** `openai/gpt-4o` was
  refused at check as « not an openai model, served by azure » because the
  wrong-seat rule consulted only each provider row's short model list. The
  rule now asks the pricing snapshot first: a model the seat prices is never
  a wrong seat, while `groq/grok-3` stays one.
