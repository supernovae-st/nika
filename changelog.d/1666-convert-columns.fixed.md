- **A compiled CSV keeps the requester's column order.** `nika:convert`
  (`to: csv`) accepts an optional `columns:` list, the header order to emit:
  the listed columns lead in that order (a listed column absent from every
  row is still emitted, empty), every unlisted key follows in the sorted
  order the default emits, and the formula guard still applies at write
  time; without `columns:` the header stays sorted. The assembler derives
  that order from a CSV source's first line (`source_columns`, a jq over the
  read text: `\r` trimmed, surrounding double quotes stripped) and binds it
  into the `<stem>_csv` stage of a `.csv` destination, so a filtered
  `order_id,customer,amount` file is written back as
  `order_id,customer,amount`; a source that is not a CSV leaves `columns:`
  out.
