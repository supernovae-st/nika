# W0 refonte performance baseline — 2026-07-13

The committed reference every breaking wave is judged against (plan gate 8).
Reproduce: `bash scripts/bench-refonte.sh` (runs both `refonte_baseline`
benches in release, snapshots RB lines + slopes + process peak-RSS here).

- machine: Apple M3 Pro (operator workstation · darwin)
- rustc: 1.97.0 · profile: bench (optimized) · engine @ main f5db09288 (0.103.0)
- iterations: adaptive per size (100→40 · 500→15 · 2k→8/12 · 10k→4/5) · warmup 1

## The slope law (2k→10k · linear = ×5 · budget ×6.25)

- parse/chain: 2k→10k ×5.17
- parse/fan_out: 2k→10k ×5.25
- parse/fan_in: 2k→10k ×5.37
- parse/diamond: 2k→10k ×5.17
- parse/mesh: 2k→10k ×5.11
- analyze/chain: 2k→10k ×5.13
- analyze/fan_out: 2k→10k ×5.06
- analyze/fan_in: 2k→10k ×6.20
- analyze/diamond: 2k→10k ×5.61
- analyze/mesh: 2k→10k ×5.42
- check/chain: 2k→10k ×2.74
- check/fan_out: 2k→10k ×4.71
- check/fan_in: 2k→10k ×4.57
- check/diamond: 2k→10k ×2.97
- check/mesh: 2k→10k ×3.25
- hover/chain: 2k→10k ×13.57 ← SLOPE
- completion/chain: 2k→10k ×5.30
- semantic_document/chain: 2k→10k ×4.13
- hover/diamond: 2k→10k ×20.38 ← SLOPE
- completion/diamond: 2k→10k ×7.06 ← SLOPE
- semantic_document/diamond: 2k→10k ×5.74

**Violations: 3** — hover ×13.6 (chain) / ×20.4 (diamond) and
completion/diamond ×7.1. Absolute cost: hover p50 = 450ms at diamond-10k
(22ms at 2k). Filed as a perfection-ledger entry + engine issue: the hover
DAG card's per-request transitive work grows super-linearly; the fix is the
first consumer of this harness. parse/analyze/check are all sub-budget
(parse ~×5.2 linear · analyze ×5.1-6.2 · check ×2.7-4.7 sub-linear).

## The local-invalidation law (recorded honestly)

`edit_local` p50 = 26.7ms vs `edit_structural` p50 = 20.2ms on diamond-2k —
both are the FULL re-pipeline: the engine is a full-recompute architecture
(single-file world, no incremental invalidation). The law « a local edit
must not recompute the world » is an objective this baseline anchors, not a
property that holds today.

## Proposed per-class budgets (baseline-derived · lock at W0 receipt)

| class | budget @2k | budget @10k | basis |
|---|---|---|---|
| warm hover | 45ms | 60ms | 2× baseline@2k · 10k after slope fix |
| warm completion | 20ms | 110ms | 2× baseline |
| semanticDocument | 55ms | 300ms | 2× baseline |
| full check pipeline | 55ms | 250ms | 2× baseline (parse+analyze+check) |

## Extensions owed (wave-gated · not measurable yet)

- workflow call graphs · composition depth/width · agent multi-callables ·
  evidence/decision bundles → join the harness in W-COMP / W-DEC.
- per-op allocation counts → needs a sanctioned measurement lane (the
  workspace forbids `unsafe`, so no counting GlobalAlloc in-tree); process
  peak-RSS comes from the runner via `/usr/bin/time -l`.
- CI ratchet: smoke sizes on PR lanes + full periodic run on the operator
  machine — wiring owed to a CI session (noise-insensitive by design).

## Raw measurements (p50/p95 µs)

```jsonl
{"op":"parse","topo":"chain","n":100,"p50_us":383,"p95_us":446}
{"op":"analyze","topo":"chain","n":100,"p50_us":115,"p95_us":127}
{"op":"check","topo":"chain","n":100,"p50_us":347,"p95_us":412}
{"op":"parse","topo":"chain","n":500,"p50_us":2019,"p95_us":2064}
{"op":"analyze","topo":"chain","n":500,"p50_us":622,"p95_us":628}
{"op":"check","topo":"chain","n":500,"p50_us":2050,"p95_us":2292}
{"op":"parse","topo":"chain","n":2000,"p50_us":8719,"p95_us":8846}
{"op":"analyze","topo":"chain","n":2000,"p50_us":2821,"p95_us":2877}
{"op":"check","topo":"chain","n":2000,"p50_us":12936,"p95_us":13629}
{"op":"parse","topo":"chain","n":10000,"p50_us":49144,"p95_us":58652}
{"op":"analyze","topo":"chain","n":10000,"p50_us":16213,"p95_us":16950}
{"op":"check","topo":"chain","n":10000,"p50_us":41973,"p95_us":44928}
{"op":"parse","topo":"fan_out","n":100,"p50_us":401,"p95_us":815}
{"op":"analyze","topo":"fan_out","n":100,"p50_us":99,"p95_us":170}
{"op":"check","topo":"fan_out","n":100,"p50_us":288,"p95_us":657}
{"op":"parse","topo":"fan_out","n":500,"p50_us":2094,"p95_us":3043}
{"op":"analyze","topo":"fan_out","n":500,"p50_us":507,"p95_us":648}
{"op":"check","topo":"fan_out","n":500,"p50_us":1440,"p95_us":1506}
{"op":"parse","topo":"fan_out","n":2000,"p50_us":9283,"p95_us":9919}
{"op":"analyze","topo":"fan_out","n":2000,"p50_us":2184,"p95_us":2310}
{"op":"check","topo":"fan_out","n":2000,"p50_us":6354,"p95_us":7783}
{"op":"parse","topo":"fan_out","n":10000,"p50_us":46705,"p95_us":49129}
{"op":"analyze","topo":"fan_out","n":10000,"p50_us":13850,"p95_us":14816}
{"op":"check","topo":"fan_out","n":10000,"p50_us":31415,"p95_us":32279}
{"op":"parse","topo":"fan_in","n":100,"p50_us":225,"p95_us":297}
{"op":"analyze","topo":"fan_in","n":100,"p50_us":54,"p95_us":58}
{"op":"check","topo":"fan_in","n":100,"p50_us":129,"p95_us":162}
{"op":"parse","topo":"fan_in","n":500,"p50_us":1155,"p95_us":1283}
{"op":"analyze","topo":"fan_in","n":500,"p50_us":310,"p95_us":322}
{"op":"check","topo":"fan_in","n":500,"p50_us":745,"p95_us":916}
{"op":"parse","topo":"fan_in","n":2000,"p50_us":5466,"p95_us":7972}
{"op":"analyze","topo":"fan_in","n":2000,"p50_us":1442,"p95_us":1477}
{"op":"check","topo":"fan_in","n":2000,"p50_us":3107,"p95_us":4197}
{"op":"parse","topo":"fan_in","n":10000,"p50_us":26962,"p95_us":27465}
{"op":"analyze","topo":"fan_in","n":10000,"p50_us":8362,"p95_us":8370}
{"op":"check","topo":"fan_in","n":10000,"p50_us":15190,"p95_us":15238}
{"op":"parse","topo":"diamond","n":100,"p50_us":390,"p95_us":584}
{"op":"analyze","topo":"diamond","n":100,"p50_us":124,"p95_us":141}
{"op":"check","topo":"diamond","n":100,"p50_us":358,"p95_us":387}
{"op":"parse","topo":"diamond","n":500,"p50_us":2085,"p95_us":2156}
{"op":"analyze","topo":"diamond","n":500,"p50_us":738,"p95_us":798}
{"op":"check","topo":"diamond","n":500,"p50_us":2211,"p95_us":2323}
{"op":"parse","topo":"diamond","n":2000,"p50_us":9125,"p95_us":9307}
{"op":"analyze","topo":"diamond","n":2000,"p50_us":3158,"p95_us":3359}
{"op":"check","topo":"diamond","n":2000,"p50_us":13498,"p95_us":13945}
{"op":"parse","topo":"diamond","n":10000,"p50_us":47526,"p95_us":47933}
{"op":"analyze","topo":"diamond","n":10000,"p50_us":17444,"p95_us":17577}
{"op":"check","topo":"diamond","n":10000,"p50_us":40663,"p95_us":40696}
{"op":"parse","topo":"mesh","n":100,"p50_us":428,"p95_us":456}
{"op":"analyze","topo":"mesh","n":100,"p50_us":158,"p95_us":168}
{"op":"check","topo":"mesh","n":100,"p50_us":461,"p95_us":493}
{"op":"parse","topo":"mesh","n":500,"p50_us":2330,"p95_us":2408}
{"op":"analyze","topo":"mesh","n":500,"p50_us":899,"p95_us":942}
{"op":"check","topo":"mesh","n":500,"p50_us":2703,"p95_us":2944}
{"op":"parse","topo":"mesh","n":2000,"p50_us":9790,"p95_us":10061}
{"op":"analyze","topo":"mesh","n":2000,"p50_us":4007,"p95_us":4080}
{"op":"check","topo":"mesh","n":2000,"p50_us":15653,"p95_us":16483}
{"op":"parse","topo":"mesh","n":10000,"p50_us":51809,"p95_us":53508}
{"op":"analyze","topo":"mesh","n":10000,"p50_us":22495,"p95_us":24087}
{"op":"check","topo":"mesh","n":10000,"p50_us":53946,"p95_us":67522}
{"op":"edit_local","topo":"diamond","n":2000,"p50_us":26164,"p95_us":30036}
{"op":"edit_structural","topo":"diamond","n":2000,"p50_us":20634,"p95_us":21974}
{"op":"hover","topo":"chain","n":100,"p50_us":579,"p95_us":833}
{"op":"completion","topo":"chain","n":100,"p50_us":437,"p95_us":542}
{"op":"semantic_document","topo":"chain","n":100,"p50_us":870,"p95_us":1049}
{"op":"hover","topo":"chain","n":500,"p50_us":3328,"p95_us":3814}
{"op":"completion","topo":"chain","n":500,"p50_us":2230,"p95_us":2467}
{"op":"semantic_document","topo":"chain","n":500,"p50_us":4744,"p95_us":5252}
{"op":"hover","topo":"chain","n":2000,"p50_us":18359,"p95_us":19315}
{"op":"completion","topo":"chain","n":2000,"p50_us":9693,"p95_us":10167}
{"op":"semantic_document","topo":"chain","n":2000,"p50_us":23992,"p95_us":24733}
{"op":"hover","topo":"chain","n":10000,"p50_us":249204,"p95_us":254574}
{"op":"completion","topo":"chain","n":10000,"p50_us":51386,"p95_us":53776}
{"op":"semantic_document","topo":"chain","n":10000,"p50_us":99161,"p95_us":101601}
{"op":"hover","topo":"diamond","n":100,"p50_us":571,"p95_us":696}
{"op":"completion","topo":"diamond","n":100,"p50_us":422,"p95_us":470}
{"op":"semantic_document","topo":"diamond","n":100,"p50_us":839,"p95_us":1054}
{"op":"hover","topo":"diamond","n":500,"p50_us":3885,"p95_us":4177}
{"op":"completion","topo":"diamond","n":500,"p50_us":2262,"p95_us":2479}
{"op":"semantic_document","topo":"diamond","n":500,"p50_us":4926,"p95_us":5257}
{"op":"hover","topo":"diamond","n":2000,"p50_us":22077,"p95_us":22553}
{"op":"completion","topo":"diamond","n":2000,"p50_us":9815,"p95_us":10359}
{"op":"semantic_document","topo":"diamond","n":2000,"p50_us":25573,"p95_us":26264}
{"op":"hover","topo":"diamond","n":10000,"p50_us":449853,"p95_us":545383}
{"op":"completion","topo":"diamond","n":10000,"p50_us":69254,"p95_us":69890}
{"op":"semantic_document","topo":"diamond","n":10000,"p50_us":146901,"p95_us":149526}
```
