- **`manqué: { rattraper: N }` fires the N latest missed slots.** A `nika serve` project beat
  declared with the bounded catch-up fires up to the N latest missed slots, one run each,
  oldest first, each run told only its own slot; the older slots are skipped first, journaled
  once. `nika check` reads the form (N a whole number from 1, `{ rattraper: 1 }` being
  `rattraper`); `nika arm` refuses it by name, as it refuses `rattraper`: catch-up is served.
