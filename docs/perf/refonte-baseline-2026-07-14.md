# W0 refonte performance baseline — 2026-07-14

- machine: Apple M3 Pro
- rustc: 1.91.1 · profile: bench (optimized)
- engine: ccc38b425
- peak RSS: schema-bench 132 MiB · lsp-bench 3125 MiB

## Slopes (2k→10k · budget ×6.25)
SLOPE parse/chain: 2k→10k ×5.12
SLOPE parse/fan_out: 2k→10k ×5.30
SLOPE parse/fan_in: 2k→10k ×5.23
SLOPE parse/diamond: 2k→10k ×5.24
SLOPE parse/mesh: 2k→10k ×5.25
SLOPE analyze/chain: 2k→10k ×5.16
SLOPE analyze/fan_out: 2k→10k ×5.30
SLOPE analyze/fan_in: 2k→10k ×5.38
SLOPE analyze/diamond: 2k→10k ×5.18
SLOPE analyze/mesh: 2k→10k ×5.15
SLOPE check/chain: 2k→10k ×3.04
SLOPE check/fan_out: 2k→10k ×4.55
SLOPE check/fan_in: 2k→10k ×4.11
SLOPE check/diamond: 2k→10k ×3.17
SLOPE check/mesh: 2k→10k ×3.29
slope violations (>6.25): 0
SLOPE hover/chain: 2k→10k ×5.38
SLOPE completion/chain: 2k→10k ×5.13
SLOPE semantic_document/chain: 2k→10k ×3.84
SLOPE hover/diamond: 2k→10k ×5.95
SLOPE completion/diamond: 2k→10k ×5.62
SLOPE semantic_document/diamond: 2k→10k ×4.20
slope violations (>6.25): 0

## Raw measurements (p50/p95 µs)
```jsonl
{"op":"parse","topo":"chain","n":100,"p50_us":283,"p95_us":294}
{"op":"analyze","topo":"chain","n":100,"p50_us":201,"p95_us":207}
{"op":"check","topo":"chain","n":100,"p50_us":556,"p95_us":575}
{"op":"parse","topo":"chain","n":500,"p50_us":1451,"p95_us":1477}
{"op":"analyze","topo":"chain","n":500,"p50_us":1061,"p95_us":1093}
{"op":"check","topo":"chain","n":500,"p50_us":3117,"p95_us":3156}
{"op":"parse","topo":"chain","n":2000,"p50_us":6192,"p95_us":6286}
{"op":"analyze","topo":"chain","n":2000,"p50_us":4400,"p95_us":4406}
{"op":"check","topo":"chain","n":2000,"p50_us":16712,"p95_us":17070}
{"op":"parse","topo":"chain","n":10000,"p50_us":31710,"p95_us":32013}
{"op":"analyze","topo":"chain","n":10000,"p50_us":22720,"p95_us":23024}
{"op":"check","topo":"chain","n":10000,"p50_us":50828,"p95_us":51093}
{"op":"parse","topo":"fan_out","n":100,"p50_us":277,"p95_us":287}
{"op":"analyze","topo":"fan_out","n":100,"p50_us":179,"p95_us":199}
{"op":"check","topo":"fan_out","n":100,"p50_us":471,"p95_us":501}
{"op":"parse","topo":"fan_out","n":500,"p50_us":1418,"p95_us":1446}
{"op":"analyze","topo":"fan_out","n":500,"p50_us":916,"p95_us":945}
{"op":"check","topo":"fan_out","n":500,"p50_us":2417,"p95_us":2495}
{"op":"parse","topo":"fan_out","n":2000,"p50_us":5906,"p95_us":5948}
{"op":"analyze","topo":"fan_out","n":2000,"p50_us":3837,"p95_us":3946}
{"op":"check","topo":"fan_out","n":2000,"p50_us":9795,"p95_us":9880}
{"op":"parse","topo":"fan_out","n":10000,"p50_us":31314,"p95_us":31486}
{"op":"analyze","topo":"fan_out","n":10000,"p50_us":20339,"p95_us":20600}
{"op":"check","topo":"fan_out","n":10000,"p50_us":44593,"p95_us":44958}
{"op":"parse","topo":"fan_in","n":100,"p50_us":225,"p95_us":248}
{"op":"analyze","topo":"fan_in","n":100,"p50_us":51,"p95_us":58}
{"op":"check","topo":"fan_in","n":100,"p50_us":141,"p95_us":153}
{"op":"parse","topo":"fan_in","n":500,"p50_us":1133,"p95_us":1156}
{"op":"analyze","topo":"fan_in","n":500,"p50_us":297,"p95_us":309}
{"op":"check","topo":"fan_in","n":500,"p50_us":743,"p95_us":774}
{"op":"parse","topo":"fan_in","n":2000,"p50_us":4732,"p95_us":4886}
{"op":"analyze","topo":"fan_in","n":2000,"p50_us":1281,"p95_us":1294}
{"op":"check","topo":"fan_in","n":2000,"p50_us":3344,"p95_us":3377}
{"op":"parse","topo":"fan_in","n":10000,"p50_us":24755,"p95_us":24761}
{"op":"analyze","topo":"fan_in","n":10000,"p50_us":6891,"p95_us":6997}
{"op":"check","topo":"fan_in","n":10000,"p50_us":13750,"p95_us":13819}
{"op":"parse","topo":"diamond","n":100,"p50_us":358,"p95_us":381}
{"op":"analyze","topo":"diamond","n":100,"p50_us":197,"p95_us":218}
{"op":"check","topo":"diamond","n":100,"p50_us":535,"p95_us":574}
{"op":"parse","topo":"diamond","n":500,"p50_us":1888,"p95_us":1912}
{"op":"analyze","topo":"diamond","n":500,"p50_us":1108,"p95_us":1139}
{"op":"check","topo":"diamond","n":500,"p50_us":3181,"p95_us":3231}
{"op":"parse","topo":"diamond","n":2000,"p50_us":8026,"p95_us":8099}
{"op":"analyze","topo":"diamond","n":2000,"p50_us":4750,"p95_us":5169}
{"op":"check","topo":"diamond","n":2000,"p50_us":17359,"p95_us":17513}
{"op":"parse","topo":"diamond","n":10000,"p50_us":42040,"p95_us":42353}
{"op":"analyze","topo":"diamond","n":10000,"p50_us":24616,"p95_us":24861}
{"op":"check","topo":"diamond","n":10000,"p50_us":55113,"p95_us":55322}
{"op":"parse","topo":"mesh","n":100,"p50_us":453,"p95_us":477}
{"op":"analyze","topo":"mesh","n":100,"p50_us":228,"p95_us":246}
{"op":"check","topo":"mesh","n":100,"p50_us":666,"p95_us":692}
{"op":"parse","topo":"mesh","n":500,"p50_us":2349,"p95_us":2376}
{"op":"analyze","topo":"mesh","n":500,"p50_us":1289,"p95_us":1331}
{"op":"check","topo":"mesh","n":500,"p50_us":3749,"p95_us":3900}
{"op":"parse","topo":"mesh","n":2000,"p50_us":9866,"p95_us":10166}
{"op":"analyze","topo":"mesh","n":2000,"p50_us":5474,"p95_us":5570}
{"op":"check","topo":"mesh","n":2000,"p50_us":19422,"p95_us":19678}
{"op":"parse","topo":"mesh","n":10000,"p50_us":51791,"p95_us":52077}
{"op":"analyze","topo":"mesh","n":10000,"p50_us":28204,"p95_us":28981}
{"op":"check","topo":"mesh","n":10000,"p50_us":63993,"p95_us":65097}
{"op":"edit_local","topo":"diamond","n":2000,"p50_us":30144,"p95_us":30316}
{"op":"edit_structural","topo":"diamond","n":2000,"p50_us":23389,"p95_us":23600}
{"op":"hover","topo":"chain","n":100,"p50_us":624,"p95_us":654}
{"op":"completion","topo":"chain","n":100,"p50_us":401,"p95_us":419}
{"op":"semantic_document","topo":"chain","n":100,"p50_us":972,"p95_us":1002}
{"op":"hover","topo":"chain","n":500,"p50_us":3246,"p95_us":3368}
{"op":"completion","topo":"chain","n":500,"p50_us":1989,"p95_us":2045}
{"op":"semantic_document","topo":"chain","n":500,"p50_us":5210,"p95_us":5697}
{"op":"hover","topo":"chain","n":2000,"p50_us":13225,"p95_us":13601}
{"op":"completion","topo":"chain","n":2000,"p50_us":8516,"p95_us":8617}
{"op":"semantic_document","topo":"chain","n":2000,"p50_us":25044,"p95_us":25700}
{"op":"hover","topo":"chain","n":10000,"p50_us":71152,"p95_us":72070}
{"op":"completion","topo":"chain","n":10000,"p50_us":43700,"p95_us":44170}
{"op":"semantic_document","topo":"chain","n":10000,"p50_us":96260,"p95_us":97173}
{"op":"hover","topo":"diamond","n":100,"p50_us":699,"p95_us":730}
{"op":"completion","topo":"diamond","n":100,"p50_us":477,"p95_us":520}
{"op":"semantic_document","topo":"diamond","n":100,"p50_us":1051,"p95_us":1064}
{"op":"hover","topo":"diamond","n":500,"p50_us":3807,"p95_us":3867}
{"op":"completion","topo":"diamond","n":500,"p50_us":2607,"p95_us":2682}
{"op":"semantic_document","topo":"diamond","n":500,"p50_us":5819,"p95_us":5878}
{"op":"hover","topo":"diamond","n":2000,"p50_us":16077,"p95_us":16251}
{"op":"completion","topo":"diamond","n":2000,"p50_us":11161,"p95_us":11378}
{"op":"semantic_document","topo":"diamond","n":2000,"p50_us":30116,"p95_us":31829}
{"op":"hover","topo":"diamond","n":10000,"p50_us":95609,"p95_us":95712}
{"op":"completion","topo":"diamond","n":10000,"p50_us":62762,"p95_us":63084}
{"op":"semantic_document","topo":"diamond","n":10000,"p50_us":126378,"p95_us":126803}
```
