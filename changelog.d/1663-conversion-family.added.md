- **A conversion between two structured files is the identity over the parsed records.**
  « convert ./fleet/mileage.csv (columns vehicle, driver, km) into ./out/mileage.json, a JSON
  array with one object per row using the column names as keys, same row order » (sealed
  sv3-14): the reader recognized nothing and the seat parsed the CSV with a model. The
  reader now reads the conversion family (a conversion head, two structured files of two
  formats): a read, the identity rule over the records, a write in the destination's format,
  no language step; the merge turns a seat's extract or draft that names the conversion
  between such a read and such a write into that computation.
