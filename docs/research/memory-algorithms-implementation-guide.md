# AI Memory System -- Implementable Algorithms Research Report

> Research date: 2026-03-31
> Sources: crates.io source code, academic papers, Rust ecosystem analysis

---

## 1. Spreading Activation (Collins & Loftus 1975 / ACT-R Anderson 2007)

### The Exact Algorithm

From ACT-R theory, the spreading activation formula is:

```
A_j = SUM_i (W_i / n_i) * S_ij
```

Where:
- `A_j` = activation received by node j
- `W_i` = source strength (activation level) of node i
- `n_i` = fan (number of outgoing connections from i) -- prevents popular nodes from dominating
- `S_ij` = associative strength between nodes i and j

### Pseudocode (BFS-based, from lucid-core)

```
function spread_activation(graph, seeds, config):
    activations = [0.0; num_nodes]
    for each seed in seeds:
        activations[seed] = seed_activation

    frontier = seeds
    visited = set(seeds)

    for depth in 0..max_depth:
        next_frontier = []
        next_activations = {}

        for source in frontier:
            if activations[source] < min_activation:
                continue

            fan = max(1, out_degree(source))

            for (target, strength) in neighbors(source):
                spread = (activations[source] / fan) * strength * decay_per_hop
                next_activations[target] += spread

                if target not in visited:
                    visited.add(target)
                    next_frontier.append(target)

        for (idx, activation) in next_activations:
            activations[idx] += activation

        frontier = next_frontier

    return activations
```

### Rust Crate: `lucid-core` v0.6.5

**License**: GPL-3.0 -- INCOMPATIBLE with AGPL-3.0-or-later for linking but fine for reference.

The `lucid-core` implementation (`src/spreading.rs`, 803 lines) includes:
- **Basic spreading activation** -- BFS with fan-out decay, bidirectional support
- **Temporal spreading** -- Based on TCM (Howard & Kahana 2002), forward/backward asymmetry
- **PageRank** -- Standard power iteration, bundled in the same file
- **Path finding** -- BFS shortest path between nodes

Key parameters in `SpreadingConfig`:
```rust
SpreadingConfig {
    decay_per_hop: 0.7,        // 0-1, how much activation decays per hop
    minimum_activation: 0.01,  // threshold to stop propagating
    max_nodes: 1000,           // cap on nodes visited
    bidirectional: true,       // spread backward too (0.7x reduced strength)
}
```

### Time Complexity

- **Per hop**: O(|frontier| * avg_degree)
- **Total**: O(depth * |frontier| * avg_degree)
- For 100K nodes with depth=3, avg_degree=10: ~3000 operations per seed. Yes, **well under 10ms**.
- lucid-core benchmarks confirm sub-millisecond for graphs up to 10K nodes.

### Also: `pensyve-core` v1.0.3 (Apache-2.0)

Has a separate ACT-R base-level activation implementation in `src/activation.rs`:

```rust
// B(m,t) = ln(SUM (t - t_k)^(-d))
fn base_level_activation(access_times: &[f64], now: f64, decay: f32) -> f32
```

With a ring buffer (`AccessRingBuffer`) for capped access history -- very practical for production.

---

## 2. Personalized PageRank (HippoRAG-style)

### Algorithm: Power Iteration

From `graphops` v0.1.3 (`src/ppr.rs`), the exact algorithm:

```
function personalized_pagerank(graph, damping, personalization, max_iter, tolerance):
    n = graph.node_count()
    p_vec = normalize(personalization)  // teleport distribution
    scores = p_vec.clone()

    for iter in 0..max_iter:
        dangling_sum = SUM(scores[i] for i where out_degree(i) == 0)

        for i in 0..n:
            new_scores[i] = (1 - damping) * p_vec[i]
                          + damping * dangling_sum * p_vec[i]

        for u in 0..n:
            if out_degree(u) > 0:
                share = damping * scores[u] / out_degree(u)
                for v in neighbors(u):
                    new_scores[v] += share

        diff = L1_norm(scores - new_scores)
        scores = new_scores
        if diff < tolerance:
            break

    return scores
```

### Parameters (from `PageRankConfig`)

```rust
PageRankConfig {
    damping: 0.85,           // probability of following an edge (vs teleporting)
    max_iterations: 100,     // convergence limit
    tolerance: 1e-6,         // L1 convergence threshold
}
```

### Rust Crates

**`graphops` v0.1.3** (MIT + Apache-2.0 dual license):
- `pagerank()` -- standard PageRank, ~60 LOC core
- `personalized_pagerank()` -- PPR with teleport bias, ~80 LOC core
- `pagerank_weighted()` -- edge-weight-aware variant
- All return `PageRankRun { scores, iterations, diff_l1, converged }`
- Uses a `Graph` trait adapter -- easy to implement for any graph backend

**`lucid-core`** also has a simpler PageRank (~40 LOC) in `spreading.rs`.

### Can Grafeo (Neo4j GDS) Compute PageRank Natively?

**Yes.** Neo4j Graph Data Science (GDS) library provides:
- `gds.pageRank.stream()` -- standard PageRank
- `gds.pageRank.stream({ sourceNodes: [...] })` -- personalized PageRank
- Parameters: `dampingFactor` (default 0.85), `maxIterations` (default 20), `tolerance` (default 1e-7)
- Runs natively in Neo4j, no Rust needed for this path

However, for a standalone Rust implementation (no Neo4j dependency), `graphops` at ~150 total LOC for both PageRank + PPR is minimal and clean.

### Performance on 100K Nodes

Power iteration converges in 15-30 iterations typically. Each iteration is O(|E|).
For 100K nodes with 1M edges: ~30M operations total. Easily under 100ms in Rust.

---

## 3. FSRS-6 Exact Algorithm (Spaced Repetition)

### Full 21-Parameter Set

From `fsrs` v5.2.0 (`src/inference.rs`):

```rust
static DEFAULT_PARAMETERS: [f32; 21] = [
    0.212,    // w[0]:  initial stability for rating 1 (Again)
    1.2931,   // w[1]:  initial stability for rating 2 (Hard)
    2.3065,   // w[2]:  initial stability for rating 3 (Good)
    8.2956,   // w[3]:  initial stability for rating 4 (Easy)
    6.4133,   // w[4]:  initial difficulty intercept
    0.8334,   // w[5]:  initial difficulty slope
    3.0194,   // w[6]:  difficulty update rate
    0.001,    // w[7]:  mean reversion weight
    1.8722,   // w[8]:  success stability: growth multiplier (exp)
    0.1666,   // w[9]:  success stability: stability decay exponent
    0.796,    // w[10]: success stability: retrievability factor
    1.4835,   // w[11]: failure stability: base multiplier
    0.0614,   // w[12]: failure stability: difficulty exponent
    0.2629,   // w[13]: failure stability: stability growth exponent
    1.6483,   // w[14]: failure stability: retrievability factor
    0.6014,   // w[15]: hard penalty multiplier
    1.8729,   // w[16]: easy bonus multiplier
    0.5425,   // w[17]: short-term stability: rating factor
    0.0912,   // w[18]: short-term stability: rating offset
    0.0658,   // w[19]: short-term stability: power exponent
    0.1542,   // w[20]: decay parameter (FSRS-6 default, was 0.5 in FSRS-5)
];
```

### State Transition Equations

**Memory State**: `(S, D)` where S = stability (days), D = difficulty (1-10)

**Forgetting Curve** (power law):
```
factor = (decay^(-1) * ln(0.9)).exp() - 1
R(t, S) = (t/S * factor + 1)^(-decay)
```
Where `decay = w[20]` (0.1542 for FSRS-6).

**Next Interval** (from desired retention):
```
I = S / factor * (R^(1/decay) - 1)
```

**Stability After Success** (rating 2, 3, or 4):
```
hard_penalty = w[15] if rating==2, else 1.0
easy_bonus = w[16] if rating==4, else 1.0
new_S = S * (exp(w[8]) * (11-D) * S^(-w[9]) * (exp(w[10]*(1-R)) - 1) * hard_penalty * easy_bonus + 1)
```

**Stability After Failure** (rating 1):
```
new_S = w[11] * D^(-w[12]) * ((S+1)^w[13] - 1) * exp(w[14]*(1-R))
// clamped: new_S >= S / exp(w[17] * w[18])
```

**Short-Term Stability** (same-day review, delta_t == 0):
```
sinc = exp(w[17] * (rating - 3 + w[18])) * S^(-w[19])
new_S = S * max(sinc, 1.0) if rating >= 3
new_S = S * sinc             if rating < 3
```

**Difficulty Update**:
```
delta_d = -w[6] * (rating - 3)
damped = (10 - D) * delta_d / 9      // linear damping
new_D = D + damped
new_D = w[7] * (init_D(4) - new_D) + new_D   // mean reversion toward "easy" baseline
```

**Initial Difficulty**:
```
init_D(rating) = w[4] - exp(w[5] * (rating - 1)) + 1
```

### Rust Implementation: `fsrs` v5.2.0

- **License**: BSD-3-Clause -- fully compatible with AGPL-3.0-or-later
- **Crate**: `fsrs` on crates.io
- **Dependency**: `burn` (tensor framework) -- heavy dependency
- **LOC**: ~2000 total (model, inference, training, simulation)
- **Key types**: `MemoryState { stability, difficulty }`, `FSRSItem`, `FSRSReview`

### Simplified Version (from pensyve-core)

`pensyve-core` implements a lighter FSRS-inspired model in `src/decay.rs` (53 LOC):

```rust
// Simplified forgetting curve: R(t, S) = (1 + t / (9 * S))^(-1)
fn retrievability(stability: f32, elapsed_days: f32) -> f32

// Simplified stability increase after success
fn reinforce(stability: f32, retrievability: f32, difficulty: u8) -> f32

// Simplified stability decrease after failure
fn on_forget(stability: f32, difficulty: u8) -> f32
```

This is much more practical for a memory system than the full FSRS optimizer.

---

## 4. Reciprocal Rank Fusion (RRF)

### Original Paper

Cormack, G. V., Clarke, C. L., & Buettcher, S. (2009).
"Reciprocal Rank Fusion outperforms Condorcet and individual Rank Learning Methods." SIGIR 2009.

### Formula

```
RRF_score(d) = SUM_r w_r / (k + rank_r(d))
```

Where:
- `rank_r(d)` is 1-indexed rank of document d in ranking r
- `k` is a smoothing constant (paper recommends k=60)
- `w_r` is an optional per-ranking weight

### Optimal k Value

- **k=60**: Cormack's recommendation, designed for web-scale IR (thousands of candidates)
- **Adaptive k**: For small corpora, k=60 kills discrimination between ranks

From `pensyve-core` (`src/rrf.rs`):
```rust
fn adaptive_k(candidate_count: usize, configured_k: u32) -> u32 {
    let auto_k = (candidate_count / 10).max(1) as u32;
    auto_k.min(configured_k)  // never exceed configured maximum
}
```

- 50 candidates -> k=5 (ratio rank1/rank50 = 11:1)
- 100 candidates -> k=10
- 1000+ candidates -> k=60 (capped)

### Rust Implementation (~40 LOC core, from pensyve-core)

```rust
fn reciprocal_rank_fusion(
    rankings: &[Vec<(Uuid, f32)>],  // multiple ranked lists
    weights: &[f32],                 // per-list weights
    k: u32,                          // smoothing constant
) -> Vec<(Uuid, f32)> {
    let k_f = f64::from(k);
    let mut scores: HashMap<Uuid, f64> = HashMap::new();

    for (ranking, &weight) in rankings.iter().zip(weights.iter()) {
        let w = f64::from(weight);
        for (rank_0, (id, _)) in ranking.iter().enumerate() {
            let rank_1 = (rank_0 + 1) as f64;
            *scores.entry(*id).or_insert(0.0) += w / (k_f + rank_1);
        }
    }

    let mut result: Vec<(Uuid, f32)> = scores
        .into_iter()
        .map(|(id, score)| (id, score as f32))
        .collect();
    result.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    result
}
```

**License**: Apache-2.0 (pensyve-core) -- fully compatible.

---

## 5. AGM Belief Revision

### Theory (Alchourron, Gardenfors, Makinson 1985)

AGM defines three operations on a belief set K:

1. **Expansion** K + A: Add belief A to K
   - `K + A = Cn(K union {A})` (close under logical consequences)

2. **Contraction** K - A: Remove belief A from K while maintaining consistency
   - Must satisfy 6 postulates including "recovery" and "minimal change"

3. **Revision** K * A: Add belief A, removing contradictions
   - `K * A = (K - not(A)) + A` (Levi identity)

### Implementation in a Knowledge Graph Context

**Minimal Change in a Knowledge Graph**:

The key insight is that "minimal change" maps to graph operations:

```
Expansion (add fact):
    1. Add new edge/node
    2. Run forward-chain implications (create implied edges)
    3. No conflict resolution needed

Contraction (remove fact):
    1. Identify the edge/node to remove
    2. Find all edges that DEPEND on this fact (transitive closure)
    3. Remove the minimal set that restores consistency
    4. "Recovery": if we re-add the fact, everything should come back

Revision (update fact):
    1. Find contradicting edges (same subject+predicate, different object)
    2. Invalidate old edges (temporal supersession -- pensyve/Graphiti pattern)
    3. Add new edge with valid_at = now
    4. Propagate changes to dependent facts
```

### Practical Implementation Pattern (from pensyve-core's graph.rs)

pensyve-core's `MemoryGraph` already implements temporal belief revision:

```rust
// Supersession = AGM revision for knowledge graphs
fn invalidate_edge(&mut self, from: Uuid, to: Uuid, superseded_by: Option<Uuid>) {
    // Find all edges from -> to that are still valid
    // Set invalid_at = now, record what superseded this edge
    // Traversal functions skip invalidated edges automatically
}

// Edge history = full provenance chain
fn get_edge_history(&self, entity_id: Uuid) -> Vec<&Edge> {
    // Returns ALL edges (valid + invalidated), sorted by valid_at
    // This is the AGM "recovery" property -- old beliefs are preserved
}
```

**Minimal change algorithm**:
```
function revise(graph, subject, predicate, new_object):
    // 1. Find existing edges matching (subject, predicate, *)
    existing = graph.get_valid_edges(subject)
        .filter(|e| e.predicate == predicate)

    // 2. Create new edge
    new_edge = Edge::new(subject, new_object, predicate)
    graph.add_edge_with_meta(new_edge)

    // 3. Invalidate contradicting edges (minimal change)
    for old_edge in existing:
        if old_edge.target != new_object:
            graph.invalidate_edge(subject, old_edge.target, Some(new_edge.id))

    // 4. Propagate (optional: find dependent edges and re-evaluate)
    propagate_revision(graph, subject, predicate)
```

---

## 6. Louvain Community Detection

### The Algorithm (Blondel et al. 2008)

Two-phase iterative process:

**Phase 1 -- Local Optimization**:
```
for each node i in random order:
    for each neighboring community C:
        compute delta_modularity of moving i to C
    move i to the community with maximum positive delta_Q
repeat until no improvement
```

**Phase 2 -- Aggregation**:
```
collapse each community into a single super-node
edges between communities become weighted edges between super-nodes
self-loops represent intra-community edges
```

Repeat Phase 1 + Phase 2 until modularity stabilizes.

**Delta Modularity Formula**:
```
delta_Q = (d_ij - resolution * (d_i * d_j) / (2 * m)) / m

where:
    d_ij = 2 * (sum of edge weights between node i and community j)
    d_i  = degree of node i
    d_j  = total degree of community j
    m    = total edge weight of the graph
```

### Rust Crate: `graphrs` v0.11.16

Full Louvain implementation in `src/algorithms/community/louvain.rs` (~580 LOC).

```rust
louvain_communities(
    &graph,
    weighted: bool,        // use edge weights?
    resolution: Option<f64>, // <1 = larger communities, >1 = smaller
    threshold: Option<f64>,  // convergence threshold
    seed: Option<u64>,       // for deterministic results
) -> Vec<HashSet<T>>
```

**License**: `graphrs` is MIT -- compatible with AGPL-3.0-or-later.

Also includes **Leiden algorithm** (improvement over Louvain) in the same crate.

### Alternative: `graphops` v0.1.3 Label Propagation

Simpler community detection (~60 LOC, label_propagation) in `src/partition.rs`:
- Much faster than Louvain
- Less optimal communities
- Good for initial clustering, then refine

### Can Grafeo (Neo4j GDS) Compute It Natively?

**Yes.** Neo4j GDS provides:
- `gds.louvain.stream()` -- full Louvain with modularity optimization
- `gds.leiden.stream()` -- Leiden algorithm (better than Louvain)
- `gds.labelPropagation.stream()` -- fast approximate communities
- All support weighted edges, seeding, resolution parameter

---

## 7. Bayesian Reliability Tracking

### The Math (Conjugate Prior)

```
Prior:     Beta(alpha, beta)       -- starts at Beta(1, 1) = uniform
Observe:   success or failure
Posterior: Beta(alpha + s, beta + f)

Reliability = alpha / (alpha + beta)
Confidence  = alpha + beta         -- higher = more data
```

### Rust Implementation (~15 lines)

```rust
#[derive(Clone, Debug)]
struct BayesianReliability {
    alpha: f32,  // successes + prior
    beta: f32,   // failures + prior
}

impl BayesianReliability {
    fn new() -> Self {
        Self { alpha: 1.0, beta: 1.0 }  // uniform prior
    }

    fn observe(&mut self, success: bool) {
        if success { self.alpha += 1.0; }
        else       { self.beta  += 1.0; }
    }

    fn reliability(&self) -> f32 {
        self.alpha / (self.alpha + self.beta)
    }

    fn confidence(&self) -> f32 {
        self.alpha + self.beta
    }

    // Lower bound of 95% credible interval (conservative estimate)
    fn lower_bound_95(&self) -> f32 {
        // Wilson score approximation for Beta distribution
        let n = self.alpha + self.beta - 2.0;
        let p = (self.alpha - 1.0) / n.max(1.0);
        let z = 1.96_f32;
        (p + z * z / (2.0 * n) - z * ((p * (1.0 - p) + z * z / (4.0 * n)) / n).sqrt())
            / (1.0 + z * z / n)
    }
}
```

### pensyve-core's Salience Model (Apache-2.0)

From `src/salience.rs`:
```rust
fn compute_salience(novelty: f32, importance: f32, extremity: f32, specificity: f32) -> f32 {
    (0.4 * novelty + 0.3 * importance + 0.1 * extremity + 0.2 * specificity).clamp(0.0, 1.0)
}

fn effective_stability(base_stability: f32, salience: f32, beta: f32) -> f32 {
    base_stability * (1.0 + beta * salience.clamp(0.0, 1.0))
}
```

---

## 8. Content-Hash Deduplication

### blake3 for Exact Match

```rust
use blake3;

fn content_hash(data: &[u8]) -> [u8; 32] {
    blake3::hash(data).into()
}

fn is_exact_duplicate(a: &[u8], b: &[u8]) -> bool {
    blake3::hash(a) == blake3::hash(b)
}
```

**Crate**: `blake3` v1.8.4, MIT + Apache-2.0 dual license.
**Performance**: ~5 GB/s on modern hardware, fastest general-purpose hash.

### Cosine Similarity for Near-Match

```rust
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    dot / (norm_a * norm_b)
}
```

### Merge Strategy When Near-Duplicate Found

```
function deduplicate_and_merge(new_memory, existing_memories, threshold=0.92):
    hash = blake3::hash(new_memory.content)

    // Phase 1: Exact match
    if existing = find_by_hash(hash):
        existing.access_count += 1
        existing.last_accessed = now
        return existing  // reinforce, don't duplicate

    // Phase 2: Near-match via embedding similarity
    new_embedding = embed(new_memory.content)
    for candidate in existing_memories:
        sim = cosine_similarity(new_embedding, candidate.embedding)
        if sim >= threshold:
            // Merge: keep the richer version
            merged = merge_memories(new_memory, candidate)
            merged.stability = reinforce(candidate.stability, ...)
            return merged

    // Phase 3: No match -- insert as new
    insert(new_memory)
    return new_memory
```

---

## License Compatibility Summary

| Crate | License | Compatible with AGPL? | Recommendation |
|-------|---------|----------------------|----------------|
| `fsrs` v5.2.0 | BSD-3-Clause | Yes | Use for full FSRS optimizer |
| `pensyve-core` v1.0.3 | Apache-2.0 | Yes | Reference for RRF, decay, salience |
| `lucid-core` v0.6.5 | GPL-3.0 | Copyleft conflict | Reference only, rewrite algorithms |
| `graphops` v0.1.3 | MIT + Apache-2.0 | Yes | Use for PPR, PageRank |
| `graphrs` v0.11.16 | MIT | Yes | Use for Louvain |
| `blake3` v1.8.4 | MIT + Apache-2.0 | Yes | Use directly |
| `petgraph` v0.8.3 | MIT + Apache-2.0 | Yes | Use as graph backend |

---

## Implementation Priority Matrix

| Algorithm | LOC to Implement | Impact | Dependency | Priority |
|-----------|-----------------|--------|------------|----------|
| RRF | ~40 | High (fusion) | None | P0 |
| Bayesian Reliability | ~15 | High (trust) | None | P0 |
| Content-Hash Dedup | ~30 | High (quality) | blake3 | P0 |
| FSRS (simplified) | ~60 | High (decay) | None | P1 |
| Spreading Activation | ~200 | High (retrieval) | None | P1 |
| Personalized PageRank | ~80 | Medium (ranking) | graphops or custom | P1 |
| AGM Belief Revision | ~100 | Medium (updates) | Graph backend | P2 |
| Louvain Community | Use crate | Medium (clustering) | graphrs | P2 |

---

## Key Takeaways

1. **RRF with adaptive k** is trivially implementable (~40 LOC) and proven effective. The `k = max(1, count/10)` formula from pensyve-core is well-tested.

2. **FSRS-6** has 21 parameters and complex tensor math in the full version. For a memory system, the simplified version from pensyve-core (retrievability + reinforce + on_forget, ~60 LOC) gives 80% of the benefit at 5% of the complexity.

3. **Spreading activation** from lucid-core is clean BFS with decay. The key insight is the fan-out normalization (`W_i / n_i`) which prevents hub nodes from dominating.

4. **PPR** from graphops is production-ready at ~80 LOC. Power iteration converges in 15-30 iterations. Can also be computed natively in Neo4j GDS.

5. **Louvain** is best consumed from `graphrs` (MIT, well-tested) rather than reimplemented. Neo4j GDS also provides it natively.

6. **AGM belief revision** maps naturally to temporal edge invalidation with supersession tracking, exactly as pensyve-core implements it.

7. **Bayesian reliability** is the simplest algorithm here (~15 LOC) but one of the most powerful for tracking source trustworthiness.

8. **blake3** for content hashing is a no-brainer dependency -- fastest hash, dual MIT/Apache-2.0 license.
