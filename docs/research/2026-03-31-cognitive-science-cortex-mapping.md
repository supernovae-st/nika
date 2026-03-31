# Cognitive Science Patterns for AI Memory Implementation

> Date: 2026-03-31
> Scope: 7 cognitive science theories mapped to Nika Cortex implementation
> Status: Research complete -- ready for design integration
> Companion to: `2026-03-31-nika-cortex-FINAL.md`

---

## Executive Summary

This report maps seven established cognitive science theories to concrete implementation patterns for Nika Cortex. The existing Cortex design (12 mechanisms, 8-signal retrieval, 6 memory levels) already embodies several of these principles implicitly. This report makes the connections explicit, identifies gaps, and proposes specific enhancements. The key insight across all seven theories: **memory is not storage -- it is an active, reconstructive, context-sensitive process that knows about itself.**

---

## 1. Schema Theory (Bartlett 1932, Piaget 1926)

### The Science

Sir Frederic Bartlett's 1932 "Remembering" demonstrated that human memory is **reconstructive, not reproductive**. His famous "War of the Ghosts" experiment showed subjects didn't recall the story verbatim -- they distorted it to fit their existing cultural schemas. Details that didn't match the schema were omitted or transformed. Details that did match were emphasized or fabricated.

Piaget extended this with two processes:
- **Assimilation**: New information absorbed into existing schemas (fast, low-effort)
- **Accommodation**: Schemas themselves restructured when new information can't be assimilated (slow, disruptive)

Key properties of schemas:
1. **Hierarchical**: Schemas contain sub-schemas (a "restaurant" schema contains "ordering", "eating", "paying" sub-schemas)
2. **Default-filling**: Missing slots are filled with defaults ("the restaurant probably had tables")
3. **Expectation-generating**: Schemas predict what comes next ("after ordering, food should arrive")
4. **Variable abstraction**: Schemas generalize across instances ("restaurants" covers fast food, fine dining, etc.)

### The Literature

- **Bartlett, F.C. (1932)**. *Remembering: A Study in Experimental and Social Psychology*. Cambridge University Press. The foundational work proving memory is reconstructive.
- **Piaget, J. (1926/1952)**. *The Origins of Intelligence in Children*. International Universities Press. Assimilation/accommodation framework.
- **Rumelhart, D.E. (1980)**. "Schemata: The building blocks of cognition." In *Theoretical Issues in Reading Comprehension*. Formal computational model of schema-based processing.
- **Tse, D., et al. (2007)**. "Schemas and Memory Consolidation." *Science*, 316(5821), 76-82. Proved that schema-consistent memories consolidate 48x faster than schema-inconsistent ones in the neocortex.
- **van Kesteren, M.T.R., et al. (2012)**. "How schema and novelty augment memory formation." *Trends in Neuroscience*. The SLIMM model showing medial prefrontal cortex mediates schema-memory interactions.

### Direct Application to Cortex

**What Cortex already has:**
- `NodeType` with `schema: Value` (JSON Schema for property validation) = proto-schemas
- Auto-evolving ontology (`Discovered` realm graduating to `User` at 10+ instances) = accommodation
- `MemoryKind` hierarchy (L0-L5) = hierarchical schema organization
- Contradiction detection (mechanism 6) = detecting when assimilation fails

**What's missing -- Schema-Guided Recall:**

The retrieval pipeline currently reconstructs context from stored text. Schema theory says it should **reconstruct memories using the schema structure itself**, filling in missing details from defaults.

```rust
/// Schema-guided recall: use NodeType schema to reconstruct complete memories
pub struct SchemaReconstructor {
    /// When a CortexNode has missing properties, fill from schema defaults
    fn reconstruct(&self, node: &CortexNode, node_type: &NodeType) -> ReconstructedMemory {
        let mut properties = node.properties.clone();

        // Fill missing fields from schema defaults
        if let Some(schema_props) = node_type.schema.get("properties") {
            for (key, spec) in schema_props.as_object().unwrap() {
                if !properties.contains_key(key) {
                    if let Some(default) = spec.get("default") {
                        properties.insert(key.clone(), default.clone());
                        // Track that this was schema-filled, not observed
                        reconstruction_log.push(ReconstructionEvent::DefaultFilled {
                            field: key.clone(),
                            source: ReconstructionSource::SchemaDefault,
                        });
                    }
                }
            }
        }

        ReconstructedMemory {
            node: node.clone(),
            filled_properties: properties,
            reconstruction_log,
            confidence_adjustment: self.confidence_penalty(reconstruction_log.len()),
        }
    }
}
```

**Schema-accelerated consolidation (Tse et al. 2007):**

Memories that match existing schemas should consolidate faster. In Cortex terms:

```rust
/// Schema-congruent memories get faster FSRS decay adjustment
fn schema_congruence_boost(node: &CortexNode, node_type: &NodeType) -> f64 {
    let schema_match_ratio = count_matching_properties(node, node_type)
        / total_schema_properties(node_type);

    // Tse et al.: schema-consistent = 48x faster consolidation
    // We model this as a stability multiplier on FSRS
    if schema_match_ratio > 0.8 {
        2.0  // High congruence: 2x stability (consolidates faster)
    } else if schema_match_ratio < 0.3 {
        0.5  // Low congruence: 0.5x stability (needs more rehearsal)
        // BUT also flag for accommodation -- the schema itself may need updating
    } else {
        1.0
    }
}
```

**Assimilation vs. Accommodation decision:**

```rust
enum SchemaOperation {
    /// New memory fits existing schema -- fast path
    Assimilate,
    /// New memory doesn't fit -- restructure the schema
    Accommodate {
        modified_fields: Vec<String>,
        new_required: Vec<String>,
    },
    /// Too novel -- create a new schema entirely
    CreateNew { suggested_name: String },
}

/// Triggered when Discovered realm reaches graduation threshold (10+ instances)
/// BUT ALSO when assimilation fails repeatedly for a given node_type
fn schema_evolution_decision(
    node: &CortexNode,
    node_type: &NodeType,
    recent_failures: &[AssimilationFailure],
) -> SchemaOperation {
    let failure_rate = recent_failures.len() as f64 / recent_insertions;

    if failure_rate < 0.1 {
        SchemaOperation::Assimilate
    } else if failure_rate < 0.5 {
        // Piaget: accommodation -- modify existing schema
        let modifications = infer_schema_changes(recent_failures);
        SchemaOperation::Accommodate {
            modified_fields: modifications.changed,
            new_required: modifications.added,
        }
    } else {
        // Too different -- spawn new type
        SchemaOperation::CreateNew {
            suggested_name: cluster_label(recent_failures),
        }
    }
}
```

**Connection to auto-evolving ontology:** This IS schema theory in action. When `instance_count` crosses thresholds and `Discovered` types graduate to `User`, that's accommodation. The insight is to make this process more intelligent -- don't just count instances, measure schema congruence.

---

## 2. Context-Dependent Memory / Encoding Specificity (Tulving 1973, Godden & Baddeley 1975)

### The Science

Endel Tulving's **Encoding Specificity Principle** (1973): "Specific encoding operations performed on what is perceived determine what is stored, and what is stored determines what retrieval cues are effective in providing access to what is stored."

Translation: You remember best when your retrieval context matches your encoding context.

The classic demonstration is Godden & Baddeley (1975): divers who learned word lists underwater recalled 40% more words when tested underwater vs. on land. The physical context served as an implicit retrieval cue.

This extends to:
- **State-dependent memory** (Eich 1980): internal state (mood, arousal) at encoding affects retrieval
- **Transfer-appropriate processing** (Morris et al. 1977): memory works best when the TYPE of processing at encoding matches retrieval
- **Environmental reinstatement** (Smith & Vela 2001): meta-analysis confirming context-dependent effects, with the caveat that the effect is strongest when other cues are absent

### The Literature

- **Tulving, E. & Thomson, D.M. (1973)**. "Encoding specificity and retrieval processes in episodic memory." *Psychological Review*, 80(5), 352-373. The foundational encoding specificity paper.
- **Godden, D.R. & Baddeley, A.D. (1975)**. "Context-dependent memory in two natural environments: On land and underwater." *British Journal of Psychology*, 66(3), 325-331. The underwater learning experiment.
- **Eich, E. (1980)**. "The cue-dependent nature of state-dependent retrieval." *Memory & Cognition*, 8(2), 157-173. State-dependent memory in pharmacological contexts.
- **Morris, C.D., Bransford, J.D., & Franks, J.J. (1977)**. "Levels of processing versus transfer appropriate processing." *Journal of Verbal Learning and Verbal Behavior*, 16(5), 519-533.
- **Smith, S.M. & Vela, E. (2001)**. "Environmental context-dependent memory." *Psychonomic Bulletin & Review*, 8(2), 203-220. Meta-analysis across 93 experiments.

### Direct Application to Cortex

**What Cortex already has:**
- `Source` enum (`Workflow{id}`, `User`, `Inferred`, `Consolidated`) = partial encoding context
- `properties: Value` = can store arbitrary context
- Workflow-scoped assembly mode (`MATCH (n) WHERE n.source STARTS WITH 'workflow:{id}'`)

**What's missing -- Rich Encoding Context:**

The current Source enum captures WHERE a memory came from, but not the full encoding context. Encoding specificity requires storing the FULL cognitive environment at encoding time.

```rust
/// The encoding context stored alongside every CortexNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingContext {
    // --- Environment ---
    /// Which workflow was running
    pub workflow_id: Option<String>,
    /// Which task produced this memory
    pub task_id: Option<String>,
    /// What provider/model was used
    pub provider: Option<String>,
    pub model: Option<String>,

    // --- Cognitive state ---
    /// What was the agent trying to do? (intent classification)
    pub intent: QueryIntent,  // Question | Action | Recall | Code | Visual
    /// What other memories were active at encoding time?
    pub co_active_nodes: Vec<NodeId>,
    /// What was the query/prompt that led to this memory?
    pub encoding_query: Option<String>,

    // --- Topic context ---
    /// Active topic clusters at encoding time (from Louvain)
    pub topic_context: Vec<String>,
    /// Tags from the workflow/task that were active
    pub workflow_tags: Vec<String>,
}
```

**Context-matching retrieval boost:**

```rust
/// Context-dependent retrieval: boost score when current context matches encoding context
fn context_match_score(
    node: &CortexNode,
    current_context: &RetrievalContext,
) -> f64 {
    let mut score = 0.0;
    let encoding = &node.encoding_context;

    // Same workflow = strong context match (Godden & Baddeley)
    if encoding.workflow_id == current_context.workflow_id {
        score += 0.3;
    }

    // Same intent type = transfer-appropriate processing (Morris et al.)
    if encoding.intent == current_context.intent {
        score += 0.2;
    }

    // Co-active node overlap = state-dependent recall (Eich)
    let overlap = encoding.co_active_nodes.intersection(&current_context.active_nodes);
    score += 0.2 * (overlap.len() as f64 / encoding.co_active_nodes.len().max(1) as f64);

    // Topic overlap = environmental reinstatement (Smith & Vela)
    let topic_overlap = encoding.topic_context.intersection(&current_context.topics);
    score += 0.15 * (topic_overlap.len() as f64 / encoding.topic_context.len().max(1) as f64);

    // Provider/model match = encoding-specific processing
    if encoding.model == current_context.model {
        score += 0.15;
    }

    score.min(1.0)
}
```

**This becomes Signal 9 in the retrieval pipeline** -- context congruence. The existing 8-signal pipeline would gain a 9th signal:

```
Existing 8 signals:
  1. BM25 (text)
  2. HNSW cosine (vector)
  3. PageRank (graph)
  4. ACT-R activation (cognitive state)
  5. Query intent (classification)
  6. Confidence x FSRS (reliability)
  7. Interference penalty
  8. Salience boost

New signal:
  9. Context congruence (encoding specificity)
```

**Implementation in the retrieval pipeline:**

The context congruence score would be computed in the Rust post-processing phase (alongside signals 4-8), using the `EncodingContext` stored with each node and compared against the current `RetrievalContext` (which is already available as part of the recall request).

---

## 3. Transfer-Appropriate Processing (Morris, Bransford & Franks 1977)

### The Science

Morris et al. (1977) challenged the prevailing "levels of processing" framework (Craik & Lockhart 1972) with a crucial finding: deep semantic encoding isn't ALWAYS best. What matters is the MATCH between how information was encoded and how it's tested.

- If you encoded phonologically (rhyme judgment) and are tested phonologically, you recall better than if you encoded semantically
- If you encoded semantically (meaning judgment) and are tested semantically, you recall better

The principle: **memory performance depends on the overlap between encoding operations and retrieval operations.**

### The Literature

- **Morris, C.D., Bransford, J.D., & Franks, J.J. (1977)**. "Levels of processing versus transfer appropriate processing." *Journal of Verbal Learning and Verbal Behavior*, 16(5), 519-533.
- **Roediger, H.L., Weldon, M.S., & Challis, B.H. (1989)**. "Explaining dissociations between implicit and explicit measures of retention: A processing account." In *Varieties of Memory and Consciousness*. Extended TAP to explain implicit/explicit memory dissociations.
- **Blaxton, T.A. (1989)**. "Investigating dissociations among memory measures: Support for a transfer-appropriate processing framework." *Journal of Experimental Psychology: Learning, Memory, and Cognition*, 15(4), 657-668.

### Direct Application to Cortex

**The core insight for AI memory:** If you stored a fact via text extraction, text-based retrieval works best. If you stored it via graph relationships, graph traversal works best. If you stored it via vector embedding, similarity search works best.

**What Cortex already has:**
- Multi-modal storage: Grafeo provides text (BM25), vector (HNSW), and graph (traversal) simultaneously
- Hybrid retrieval (8-signal RRF) fuses all three modalities

**What's missing -- Encoding-modality tracking:**

The system doesn't currently track HOW a memory was encoded. Was it extracted from text? Inferred from graph relationships? Derived from vector similarity?

```rust
/// Track the encoding modality for transfer-appropriate retrieval
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EncodingModality {
    /// Extracted from text (user input, workflow output, fetch result)
    TextExtraction,
    /// Derived from graph traversal (auto-linking, consolidation)
    GraphDerivation,
    /// Inferred from vector similarity (semantic clustering)
    VectorInference,
    /// Multi-modal (consolidated from multiple sources)
    MultiModal,
    /// Direct user assertion (nika:remember with explicit content)
    DirectAssertion,
}

impl CortexNode {
    /// Add to the existing node struct
    pub encoding_modality: EncodingModality,
}
```

**Transfer-appropriate retrieval weighting:**

```rust
/// Adjust signal weights based on encoding modality match
fn transfer_appropriate_weights(
    node: &CortexNode,
    query_modality: &QueryModality,
) -> SignalWeights {
    match (&node.encoding_modality, query_modality) {
        // Text-encoded, text-queried = boost BM25
        (EncodingModality::TextExtraction, QueryModality::TextSearch) => {
            SignalWeights { bm25: 1.5, cosine: 0.8, pagerank: 0.8, ..default() }
        }
        // Graph-derived, graph-queried = boost PageRank
        (EncodingModality::GraphDerivation, QueryModality::GraphTraversal) => {
            SignalWeights { bm25: 0.8, cosine: 0.8, pagerank: 1.5, ..default() }
        }
        // Vector-inferred, similarity-queried = boost HNSW
        (EncodingModality::VectorInference, QueryModality::SimilaritySearch) => {
            SignalWeights { bm25: 0.8, cosine: 1.5, pagerank: 0.8, ..default() }
        }
        // Multi-modal encoding = all paths equally valid
        (EncodingModality::MultiModal, _) => SignalWeights::default(),
        // Mismatch = slight penalty but still accessible
        _ => SignalWeights { bm25: 0.9, cosine: 0.9, pagerank: 0.9, ..default() }
    }
}
```

**Practical impact:** When `nika:recall` queries Cortex, the system should classify the query modality (already done implicitly via intent classification -- signal 5). The new insight is to cross-reference this against the encoding modality of candidate results, boosting matches and penalizing mismatches.

This makes the existing multi-modal storage (text + vector + graph) MORE than just redundancy -- each modality becomes a specialized retrieval PATH that's most effective for memories encoded through the same modality.

---

## 4. Interference Theory (McGeoch 1932, Underwood 1957)

### The Science

Three types of interference affect memory retrieval:

1. **Proactive interference (PI)**: Old memories block learning/recalling new ones. "I keep typing my old password."
2. **Retroactive interference (RI)**: New memories disrupt recall of old ones. "After learning Spanish, I can't remember my French."
3. **Output interference (OI)**: The very act of retrieving some items makes others harder to retrieve. Anderson, Bjork & Bjork (1994) showed that retrieving certain category exemplars inhibits recall of non-retrieved exemplars from the same category.

The mechanism behind OI is **retrieval-induced forgetting (RIF)** -- practiced items get strengthened while competing items get suppressed. This is not passive decay; it's active inhibition.

### The Literature

- **McGeoch, J.A. (1932)**. "Forgetting and the law of disuse." *Psychological Review*, 39(4), 352-370. The response competition theory of interference.
- **Underwood, B.J. (1957)**. "Interference and forgetting." *Psychological Review*, 64(1), 49-60. Proactive interference as the major cause of forgetting.
- **Anderson, M.C., Bjork, R.A., & Bjork, E.L. (1994)**. "Remembering can cause forgetting: Retrieval dynamics in long-term memory." *Journal of Experimental Psychology: Learning, Memory, and Cognition*, 20(5), 1063-1087. The retrieval-induced forgetting paper.
- **Anderson, M.C. (2003)**. "Rethinking interference theory: Executive control and the mechanisms of forgetting." *Journal of Memory and Language*, 49(4), 415-445. Inhibitory control account of RIF.
- **Storm, B.C. (2011)**. "The benefit of forgetting in thinking and remembering." *Current Directions in Psychological Science*, 20(5), 291-295. RIF as adaptive -- it declutters the retrieval set.

### Direct Application to Cortex

**What Cortex already has:**
- Mechanism 10: Interference detection (cosine > 0.9 between results = interference candidate)
- Signal 7: Interference penalty in the retrieval pipeline
- Consolidation (mechanism 5) clusters episodes into threads, which addresses some PI/RI

**What's missing -- Output Interference Tracking and Retrieval-Induced Boosting:**

The current design detects interference between STORED items but doesn't account for output interference -- the fact that RETRIEVING certain memories suppresses related unretrieved memories. This is a significant gap.

```rust
/// Track retrieval history per session for output interference management
pub struct RetrievalHistory {
    /// Nodes retrieved in this session, in order
    pub retrieved: Vec<(NodeId, DateTime<Utc>)>,
    /// Nodes that were candidates but NOT retrieved (suppressed)
    pub suppressed: Vec<(NodeId, DateTime<Utc>)>,
}

impl RetrievalHistory {
    /// After a recall, identify unretrieved related items and boost them
    /// This COUNTERACTS output interference (Anderson et al. 1994)
    pub fn anti_rif_boost(
        &self,
        retrieved_nodes: &[NodeId],
        graph: &GrafeoStore,
    ) -> Vec<(NodeId, f64)> {
        let mut boosts = Vec::new();

        for retrieved_id in retrieved_nodes {
            // Find neighbors that were NOT retrieved
            let neighbors = graph.neighbors(retrieved_id, 2); // 2-hop
            for neighbor in neighbors {
                if !retrieved_nodes.contains(&neighbor.id) {
                    // This neighbor is at risk of retrieval-induced forgetting
                    // Proactively boost it for future queries
                    let distance_penalty = 1.0 / (neighbor.distance as f64 + 1.0);
                    boosts.push((neighbor.id, 0.1 * distance_penalty));
                }
            }
        }

        boosts
    }
}
```

**The adaptive forgetting insight (Storm 2011):**

Output interference isn't always bad. When many similar items compete, RIF helps declutter the retrieval set. The system should distinguish:

```rust
/// Decide whether output interference should be counteracted or allowed
fn interference_strategy(
    retrieved: &[EvidencePacket],
    suppressed: &[EvidencePacket],
) -> InterferenceAction {
    let category_diversity = unique_categories(retrieved);
    let suppressed_relevance = mean_relevance(suppressed);

    if category_diversity < 3 && suppressed_relevance > 0.5 {
        // Few categories, high-relevance suppressed items
        // RIF is harmful here -- boost suppressed items
        InterferenceAction::BoostSuppressed
    } else if category_diversity > 5 && suppressed_relevance < 0.3 {
        // Many categories, low-relevance suppressed items
        // RIF is helpful -- let forgetting clean up the set
        InterferenceAction::AllowForgetting
    } else {
        InterferenceAction::Neutral
    }
}
```

**Proactive interference management:**

The existing mechanism 10 (cosine > 0.9) detects interference candidates but doesn't distinguish PI from RI. Adding temporal direction:

```rust
/// Classify interference type based on temporal order
fn classify_interference(
    node_a: &CortexNode,
    node_b: &CortexNode,
    cosine: f64,
) -> InterferenceType {
    if cosine < 0.9 {
        return InterferenceType::None;
    }

    if node_a.created_at < node_b.created_at {
        // A is older, might block recall of B
        InterferenceType::Proactive { blocker: node_a.id, blocked: node_b.id }
    } else {
        // B is newer, might overwrite recall of A
        InterferenceType::Retroactive { overwriter: node_b.id, overwritten: node_a.id }
    }
}
```

---

## 5. Metamemory (Nelson & Narens 1990, Flavell 1979)

### The Science

**Metamemory** is knowledge about one's own memory capabilities and contents. It has two levels:

1. **Meta-level** (monitoring): Knowing what you know and how well you know it
   - **Judgments of Learning (JOL)**: "I'll remember this" -- predictions about future recall
   - **Feeling of Knowing (FOK)**: "I know this but can't recall it right now"
   - **Confidence judgments**: "I'm 80% sure this is correct"
   - **Source monitoring**: "I know this from X source" (see section 7)

2. **Object-level** (control): Using metamemory knowledge to guide behavior
   - **Study allocation**: Spending more time on poorly-known items
   - **Retrieval strategy selection**: Choosing how to search based on what you know about your memory
   - **Output monitoring**: Checking retrieved answers against confidence

The Nelson & Narens (1990) framework describes monitoring and control as two interacting processes:

```
META LEVEL:    Monitor ←--------→ Control
                 ↑                    ↓
OBJECT LEVEL:  Memory operations (encoding, retrieval, storage)
```

### The Literature

- **Flavell, J.H. (1979)**. "Metacognition and cognitive monitoring: A new area of cognitive-developmental inquiry." *American Psychologist*, 34(10), 906-911. Coined "metacognition."
- **Nelson, T.O. & Narens, L. (1990)**. "Metamemory: A theoretical framework and new findings." *Psychology of Learning and Motivation*, 26, 125-173. The foundational monitoring/control framework.
- **Koriat, A. (1997)**. "Monitoring one's own knowledge during study: A cue-utilization approach to judgments of learning." *Journal of Experimental Psychology: General*, 126(4), 349-370. Cue-based JOL theory.
- **Dunlosky, J. & Metcalfe, J. (2009)**. *Metamemory*. SAGE Publications. Comprehensive textbook.
- **Fleming, S.M. & Dolan, R.J. (2012)**. "The neural basis of metacognitive ability." *Philosophical Transactions of the Royal Society B*, 367(1594), 1338-1349. Neural correlates of metacognition.

### Direct Application to Cortex

**What Cortex already has:**
- `nika:cortex_audit` (CSR score, orphans, stale, integrity) = basic metamemory
- `confidence: f64` on every node = partial confidence calibration
- FSRS-6 retrievability = implicit feeling-of-knowing
- `reflective` memory level (L4) = meta-observations about patterns

**What's missing -- Systematic Knowledge-About-Knowledge:**

```rust
/// Metamemory system: what the agent knows about what it knows
pub struct MetaMemory {
    /// Coverage map: knowledge density per topic/domain
    pub coverage: HashMap<String, CoverageStat>,
    /// Calibration: how accurate are our confidence scores?
    pub calibration: CalibrationTracker,
    /// Knowledge gaps: topics with few or low-confidence memories
    pub gaps: Vec<KnowledgeGap>,
    /// Retrieval strategy history: what worked best for what query types
    pub strategy_history: Vec<StrategyRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageStat {
    pub topic: String,
    /// How many memories in this topic
    pub node_count: u64,
    /// Average confidence of memories in this topic
    pub avg_confidence: f64,
    /// Average retrievability (FSRS) -- are these memories accessible?
    pub avg_retrievability: f64,
    /// Freshness: when was the newest memory in this topic created?
    pub newest: DateTime<Utc>,
    /// Contradiction density: how many contradictions in this topic?
    pub contradiction_ratio: f64,
    /// Assessed by: human-verified vs auto-generated vs inferred
    pub primary_source: Source,
}

#[derive(Debug, Clone)]
pub struct KnowledgeGap {
    pub topic: String,
    /// How the gap was detected
    pub detection_method: GapDetection,
    /// How important is filling this gap?
    pub importance: f64,
    /// Suggested action
    pub suggested_action: GapAction,
}

pub enum GapDetection {
    /// Query hit zero results
    QueryMiss,
    /// Low coverage stat detected during audit
    LowCoverage,
    /// Many queries about this topic, few memories
    QueryFrequencyMismatch,
    /// Neighboring topics well-covered, this one not
    TopologicalHole,
}

pub enum GapAction {
    /// Suggest running a workflow to fill the gap
    SuggestWorkflow(String),
    /// Flag for user attention
    FlagForUser,
    /// Auto-research (if agent has appropriate tools)
    AutoResearch,
}
```

**Confidence calibration (Koriat 1997):**

```rust
/// Track prediction accuracy to calibrate confidence scores over time
pub struct CalibrationTracker {
    /// Bins of confidence ranges and their actual accuracy
    pub bins: Vec<CalibrationBin>,
}

pub struct CalibrationBin {
    pub confidence_range: (f64, f64),  // e.g., (0.8, 0.9)
    pub predictions: u64,              // How many times we predicted in this range
    pub correct: u64,                  // How many times the prediction was validated
    pub actual_accuracy: f64,          // correct / predictions

    /// Calibration error = |confidence - actual_accuracy|
    /// Perfect calibration: error = 0 (80% confident items are correct 80% of the time)
    pub calibration_error: f64,
}

impl CalibrationTracker {
    /// Adjust future confidence scores based on calibration history
    /// If we're overconfident (our 0.9 items are only right 0.7 of the time),
    /// apply a deflation factor
    pub fn adjusted_confidence(&self, raw_confidence: f64) -> f64 {
        let bin = self.bin_for(raw_confidence);
        if bin.calibration_error > 0.1 {
            // Significant miscalibration -- adjust
            let adjustment = bin.actual_accuracy / ((bin.confidence_range.0 + bin.confidence_range.1) / 2.0);
            (raw_confidence * adjustment).clamp(0.0, 1.0)
        } else {
            raw_confidence
        }
    }
}
```

**Retrieval strategy selection (Nelson & Narens control function):**

```rust
/// Use metamemory to select retrieval strategy
fn select_retrieval_strategy(
    query: &str,
    meta: &MetaMemory,
    topic: &str,
) -> RetrievalStrategy {
    let coverage = meta.coverage.get(topic);

    match coverage {
        None => {
            // No knowledge about this topic -- broadest possible search
            RetrievalStrategy::BroadExploratory {
                signals: all_signals(),
                recursive_depth: 3,
            }
        }
        Some(stat) if stat.node_count > 50 && stat.avg_confidence > 0.7 => {
            // Rich, confident knowledge -- targeted retrieval
            RetrievalStrategy::TargetedConfident {
                signals: [BM25, HNSW, ACT_R],
                max_results: 5,
            }
        }
        Some(stat) if stat.contradiction_ratio > 0.2 => {
            // Contradictory knowledge -- retrieve ALL sides
            RetrievalStrategy::ContradictionAware {
                include_contradictions: true,
                present_both_sides: true,
            }
        }
        Some(stat) if stat.avg_retrievability < 0.3 => {
            // Poorly accessible memories -- use graph traversal to find them
            RetrievalStrategy::GraphHeavy {
                pagerank_weight: 2.0,
                recursive_depth: 2,
            }
        }
        _ => RetrievalStrategy::Default,
    }
}
```

**Implementation as `nika:cortex_meta` tool:**

```yaml
# Workflow usage
- id: check_knowledge
  invoke: "nika:cortex_meta"
  params:
    topic: "rust async patterns"
    action: "coverage"

# Returns:
# {
#   "node_count": 23,
#   "avg_confidence": 0.72,
#   "avg_retrievability": 0.45,
#   "gaps": ["error handling patterns", "pin/unpin"],
#   "calibration_error": 0.08,
#   "suggested_strategy": "graph_heavy"
# }
```

---

## 6. Prospective Memory (Einstein & McDaniel 1990)

### The Science

**Prospective memory** is remembering to perform an intended action at some future point. Unlike retrospective memory ("what happened?"), prospective memory is "what do I need to do?"

Two types:
1. **Event-based**: "When I see John, give him the book" -- triggered by an event/cue
2. **Time-based**: "At 3pm, call the dentist" -- triggered by time

Einstein & McDaniel's Multiprocess Theory distinguishes:
- **Monitoring**: Actively checking the environment for the target cue (resource-intensive)
- **Spontaneous retrieval**: The cue automatically triggers the intention (low-cost)

The key insight: prospective memory requires a **strategic monitoring** component that continuously scans the environment for trigger conditions, PLUS a stored intention that gets activated when the trigger fires.

### The Literature

- **Einstein, G.O. & McDaniel, M.A. (1990)**. "Normal aging and prospective memory." *Journal of Experimental Psychology: Learning, Memory, and Cognition*, 16(4), 717-726. The foundational dual-process model.
- **McDaniel, M.A. & Einstein, G.O. (2000)**. "Strategic and automatic processes in prospective memory retrieval: A multiprocess framework." *Applied Cognitive Psychology*, 14(7), S127-S144. The multiprocess theory.
- **Kliegel, M., Martin, M., McDaniel, M.A., & Einstein, G.O. (2002)**. "Complex prospective memory and executive control of working memory: A process model." *Psychologische Beitrage*, 44, 303-318.
- **Smith, R.E. (2003)**. "The cost of remembering to remember in event-based prospective memory." *Journal of Experimental Psychology: Learning, Memory, and Cognition*, 29(3), 347-361. Demonstrated that monitoring has a real cost on concurrent task performance.

### Direct Application to Cortex

**What Cortex already has:**
- Mechanism 12: Conditional triggers (Nocturne-inspired) -- this IS prospective memory
- `trigger_rules` SQLite table for pattern-based auto-recall
- Mechanism 4: Prospective indexing (Kumiho) -- anticipating future needs at write time

**What's missing -- Time-based triggers and monitoring cost accounting:**

```rust
/// Extended trigger system with both event-based and time-based triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProspectiveTrigger {
    pub id: TriggerId,
    /// The intention to fulfill
    pub intention: ProspectiveIntention,
    /// When to trigger
    pub trigger_type: TriggerType,
    /// Priority (affects monitoring frequency)
    pub priority: TriggerPriority,
    /// Has this been fulfilled?
    pub status: TriggerStatus,
    /// When was this intention created?
    pub created_at: DateTime<Utc>,
    /// Deadline (for urgency calculation)
    pub deadline: Option<DateTime<Utc>>,
}

pub enum TriggerType {
    /// Event-based: "when topic X is mentioned" (existing mechanism 12)
    EventBased {
        pattern: TriggerPattern,
        /// Cost of monitoring this trigger (Smith 2003)
        /// Higher cost = check less frequently
        monitoring_cost: f64,
    },
    /// Time-based: "after 24 hours, re-verify this fact"
    TimeBased {
        interval: Duration,
        /// Repeating or one-shot?
        recurrence: Recurrence,
    },
    /// Composite: "when X happens AND 24h have passed"
    Composite {
        event: TriggerPattern,
        time_condition: TimeCondition,
    },
}

pub enum ProspectiveIntention {
    /// Re-verify a fact (e.g., after 24h, check if API endpoint still works)
    Reverify { node_id: NodeId },
    /// Inject context into a workflow when trigger fires
    InjectContext { nodes: Vec<NodeId>, reason: String },
    /// Alert the user about something
    Alert { message: String, severity: AlertSeverity },
    /// Trigger consolidation of a topic
    Consolidate { topic: String },
    /// Auto-research to fill a knowledge gap
    Research { query: String, depth: usize },
}

pub enum Recurrence {
    OneShot,
    Repeating { max_occurrences: Option<u32> },
    /// Adaptive: frequency decreases as the fact stabilizes
    /// (maps to FSRS stability -- well-known facts checked less often)
    Adaptive { linked_node: NodeId },
}
```

**Monitoring cost budget (Smith 2003):**

Active monitoring consumes resources. The system should have a budget:

```rust
/// Monitor budget: total monitoring cost per tick must stay under threshold
const MAX_MONITORING_COST_PER_TICK: f64 = 1.0;

fn prioritize_triggers(
    triggers: &[ProspectiveTrigger],
    budget: f64,
) -> Vec<&ProspectiveTrigger> {
    let mut sorted = triggers.to_vec();
    sorted.sort_by(|a, b| {
        // Priority first, then urgency (deadline proximity)
        let a_urgency = a.urgency_score();
        let b_urgency = b.urgency_score();
        b_urgency.partial_cmp(&a_urgency).unwrap()
    });

    let mut remaining_budget = budget;
    let mut active = Vec::new();

    for trigger in &sorted {
        let cost = trigger.monitoring_cost();
        if remaining_budget >= cost {
            active.push(trigger);
            remaining_budget -= cost;
        }
        // Low-priority triggers that don't fit the budget get deferred
        // This is the "monitoring cost" from Smith 2003
    }

    active
}
```

**Time-based triggers in the daemon:**

The Nika daemon already has a job scheduler. Time-based prospective memory maps directly to daemon cron jobs:

```rust
/// Register time-based trigger with the daemon
async fn register_time_trigger(
    daemon: &DaemonClient,
    trigger: &ProspectiveTrigger,
) -> Result<()> {
    match &trigger.trigger_type {
        TriggerType::TimeBased { interval, recurrence } => {
            daemon.schedule_job(DaemonJob {
                id: trigger.id.to_string(),
                schedule: match recurrence {
                    Recurrence::OneShot => Schedule::Once(Utc::now() + *interval),
                    Recurrence::Repeating { .. } => Schedule::Every(*interval),
                    Recurrence::Adaptive { linked_node } => {
                        // Check frequency based on FSRS stability
                        let stability = get_fsrs_stability(linked_node)?;
                        let check_interval = Duration::hours((stability * 24.0) as i64);
                        Schedule::Every(check_interval)
                    }
                },
                action: JobAction::CortexTrigger(trigger.intention.clone()),
            }).await?;
        }
        _ => {} // Event-based handled by the retrieval pipeline
    }
    Ok(())
}
```

---

## 7. Source Monitoring (Johnson, Hashtroudi & Lindsay 1993)

### The Science

**Source monitoring** is the set of processes by which people determine the origins of their memories, knowledge, and beliefs. Johnson et al. (1993) identified four types:

1. **External source monitoring**: Distinguishing between two external sources ("Did Alice or Bob tell me this?")
2. **Internal source monitoring**: Distinguishing between internally generated sources ("Did I actually say this, or just think about saying it?")
3. **Reality monitoring**: Distinguishing between external and internal sources ("Did this really happen, or did I imagine it?")
4. **Temporal source monitoring**: Determining WHEN something was learned

Source monitoring failures (misattributions) cause:
- **Cryptomnesia**: Believing you originated an idea that came from elsewhere
- **False fame effect**: Familiarity from prior exposure mistaken for recognition of celebrity
- **Misinformation effect**: Post-event information alters the original memory

The **Source Monitoring Framework (SMF)** says source decisions are based on:
- **Perceptual detail**: Real events have more vivid sensory details
- **Contextual information**: Where, when, who was present
- **Affective information**: Emotional valence at encoding
- **Cognitive operations**: What mental operations were performed during encoding

### The Literature

- **Johnson, M.K., Hashtroudi, S., & Lindsay, D.S. (1993)**. "Source monitoring." *Psychological Bulletin*, 114(1), 3-28. The definitive framework.
- **Lindsay, D.S. (2008)**. "Source monitoring." In *Learning and Memory: A Comprehensive Reference*, Vol. 2, 325-348. Comprehensive update.
- **Glisky, E.L. & Kong, L.L. (2008)**. "Do young and older adults rely on different processes in source memory tasks?" *Journal of Experimental Psychology: Learning, Memory, and Cognition*, 34(4), 809-822.
- **Mitchell, K.J. & Johnson, M.K. (2009)**. "Source monitoring 15 years later: What have we learned from fMRI about the neural mechanisms of source memory?" *Psychological Bulletin*, 135(4), 638-677.

### Direct Application to Cortex

**What Cortex already has:**
- `Source` enum: `Workflow{id}`, `User`, `Inferred`, `Consolidated` = basic source tracking
- Provenance tracking (NovaNet ADR-042) = architectural commitment to source monitoring
- `superseded_by: Option<NodeId>` = temporal source chain

**What's missing -- Rich Source Metadata and Reality Monitoring:**

The current Source enum is too coarse. It doesn't distinguish between the richness of source contexts that Johnson et al. found critical for accurate source monitoring.

```rust
/// Rich source metadata following Johnson et al. (1993) SMF
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetadata {
    // --- External source monitoring ---
    /// Primary origin category
    pub source: Source,  // existing enum
    /// Specific external agent/system that provided this information
    pub external_agent: Option<String>,  // "grok-3", "user:thibaut", "workflow:podcast-gen"
    /// Was this observed directly or reported by another agent?
    pub observation_type: ObservationType,

    // --- Reality monitoring ---
    /// How was this memory generated?
    pub generation_type: GenerationType,
    /// Perceptual richness: how much detail was available at encoding?
    /// (Higher = more likely to be real/observed, Lower = more likely inferred)
    pub perceptual_richness: f64,

    // --- Temporal source monitoring ---
    /// Precise timestamp of the source event
    pub source_timestamp: DateTime<Utc>,
    /// Was this learned in a single event or accumulated over multiple?
    pub acquisition_pattern: AcquisitionPattern,

    // --- Cognitive operations at encoding ---
    /// What processing was done to produce this memory?
    pub encoding_operations: Vec<EncodingOperation>,
}

pub enum ObservationType {
    /// Directly observed in workflow output / API response
    Direct,
    /// Reported by another agent or memory consolidation
    Reported { reporter: String },
    /// Inferred from other memories (graph derivation)
    Inferred { basis_nodes: Vec<NodeId> },
}

pub enum GenerationType {
    /// Came from external data (fetch, user input, API)
    ExternallyGenerated,
    /// Generated by LLM inference
    LLMGenerated { model: String, temperature: f64 },
    /// Generated by consolidation/merging process
    ConsolidationGenerated,
    /// User explicitly asserted this
    UserAsserted,
}

pub enum AcquisitionPattern {
    /// Learned in a single event
    SingleExposure,
    /// Accumulated from multiple exposures (higher confidence)
    MultipleExposures { count: u32 },
    /// Gradually refined over time
    Progressive { revision_count: u32 },
}

pub enum EncodingOperation {
    TextExtraction,
    Summarization,
    Inference,
    Deduction,
    Analogy,
    PatternMatching,
    UserCorrection,
}
```

**Source confusion detection (cryptomnesia prevention):**

```rust
/// Detect potential source confusion in the memory graph
fn detect_source_confusion(
    node_a: &CortexNode,
    node_b: &CortexNode,
    cosine: f64,
) -> Option<SourceConfusionRisk> {
    if cosine < 0.85 {
        return None;
    }

    // Very similar content but different sources
    if node_a.source_metadata.source != node_b.source_metadata.source {
        // Check for reality monitoring confusion
        let one_external = matches!(
            node_a.source_metadata.generation_type,
            GenerationType::ExternallyGenerated
        );
        let one_internal = matches!(
            node_b.source_metadata.generation_type,
            GenerationType::LLMGenerated { .. }
        );

        if one_external && one_internal {
            return Some(SourceConfusionRisk::RealityMonitoring {
                external: node_a.id,
                internal: node_b.id,
                recommendation: "Verify which version is ground truth",
            });
        }

        return Some(SourceConfusionRisk::ExternalMismatch {
            sources: vec![
                node_a.source_metadata.external_agent.clone(),
                node_b.source_metadata.external_agent.clone(),
            ],
        });
    }

    None
}
```

**Source-weighted confidence:**

```rust
/// Adjust confidence based on source quality
fn source_adjusted_confidence(node: &CortexNode) -> f64 {
    let base = node.confidence;
    let meta = &node.source_metadata;

    let source_multiplier = match &meta.generation_type {
        // External data = highest trust
        GenerationType::ExternallyGenerated => 1.0,
        // User assertion = high trust
        GenerationType::UserAsserted => 0.95,
        // LLM generation = moderate trust, temperature-dependent
        GenerationType::LLMGenerated { temperature, .. } => {
            0.8 - (temperature * 0.2)  // Higher temperature = lower trust
        }
        // Consolidation = depends on source nodes
        GenerationType::ConsolidationGenerated => 0.75,
    };

    let acquisition_multiplier = match &meta.acquisition_pattern {
        AcquisitionPattern::SingleExposure => 0.8,
        AcquisitionPattern::MultipleExposures { count } => {
            (1.0 - 0.5_f64.powi(*count as i32)).min(1.0)  // Asymptotic to 1.0
        }
        AcquisitionPattern::Progressive { revision_count } => {
            0.85 + 0.03 * (*revision_count as f64).min(5.0)
        }
    };

    (base * source_multiplier * acquisition_multiplier).clamp(0.0, 1.0)
}
```

---

## Synthesis: How All Seven Theories Interlock

The seven cognitive science patterns are not independent -- they form a coherent system:

```
                    METAMEMORY (5)
                    "What do I know?"
                         │
                    monitors & controls
                         │
    ┌────────────────────┼────────────────────┐
    │                    │                    │
    ▼                    ▼                    ▼
SCHEMA THEORY (1)   INTERFERENCE (4)   SOURCE MONITORING (7)
"Structure of        "Competing          "Where did this
 knowledge"          memories"            come from?"
    │                    │                    │
    │ guides             │ detects            │ validates
    │ reconstruction     │ conflicts          │ provenance
    │                    │                    │
    └──────────┬─────────┴────────────────────┘
               │
               ▼
    ENCODING SPECIFICITY (2)  ←──→  TRANSFER-APPROPRIATE (3)
    "Context at encoding"         "Match encoding to retrieval"
               │
               │ stored alongside facts
               │
               ▼
    PROSPECTIVE MEMORY (6)
    "Future intentions"
    triggers on context match
```

### Mapping to Cortex's 12 Mechanisms

| Cognitive Theory | Cortex Mechanism(s) | Enhancement |
|-----------------|---------------------|-------------|
| Schema theory | Mechanism 11 (auto-link), NodeType ontology | Schema-guided reconstruction, schema congruence boost to FSRS |
| Encoding specificity | (new) | EncodingContext struct, context congruence as Signal 9 |
| Transfer-appropriate | Grafeo multi-modal retrieval | Encoding modality tracking, modality-matched signal weights |
| Interference (output) | Mechanism 10 (interference) | RetrievalHistory + anti-RIF boosting of unretrieved neighbors |
| Metamemory | nika:cortex_audit (partial) | CoverageStat, CalibrationTracker, strategy selection |
| Prospective memory | Mechanism 12 (triggers), Mechanism 4 (anticipation) | Time-based triggers in daemon, monitoring cost budget |
| Source monitoring | Source enum, provenance | SourceMetadata, source confusion detection, source-weighted confidence |

### Proposed New Retrieval Pipeline (10 signals)

```
Query
  │
  ├─→ ⓪ TRIGGER CHECK (prospective memory, mechanism 12)
  │
  ├─→ STRATEGY SELECTION (metamemory: what do we know about this topic?)
  │
  ├─→ GRAFEO HYBRID QUERY (signals 1-3, one call):
  │     ① BM25 on content + anticipations
  │     ② HNSW cosine on embedding
  │     ③ PageRank from entity seeds
  │     → Grafeo native RRF merge
  │
  ├─→ RUST POST-PROCESSING (signals 4-10):
  │     ④ ACT-R spreading activation
  │     ⑤ Query intent classification
  │     ⑥ Confidence × FSRS retrievability
  │     ⑦ Interference penalty (PI/RI/OI-aware)
  │     ⑧ Salience boost
  │     ⑨ Context congruence (encoding specificity)    ← NEW
  │     ⑩ Transfer-appropriate modality match           ← NEW
  │
  ├─→ FINAL RRF MERGE + SCHEMA RECONSTRUCTION
  │     Schema-guided default filling on retrieved nodes
  │     Source-weighted confidence adjustment
  │
  ├─→ TOKEN BUDGET FILTER
  │
  ├─→ OUTPUT INTERFERENCE TRACKING
  │     Record what was retrieved vs. suppressed
  │     Anti-RIF boost unretrieved neighbors for next query
  │
  └─→ METAMEMORY UPDATE
        Update coverage stats, calibration bins
        Log retrieval strategy performance
```

---

## Implementation Priority

### Phase 1 (MVP -- implement with Cortex v1)
1. **Source monitoring**: Extend `Source` enum to `SourceMetadata` -- low cost, high value
2. **Encoding context**: Store `EncodingContext` alongside nodes -- data capture, no retrieval changes
3. **Prospective time triggers**: Wire to daemon cron jobs -- infrastructure already exists

### Phase 2 (Signal expansion)
4. **Context congruence** (Signal 9): Add to post-processing pipeline
5. **Transfer-appropriate weighting** (Signal 10): Encoding modality tracking + signal weight adjustment
6. **Output interference tracking**: RetrievalHistory per session + anti-RIF boosting

### Phase 3 (Self-aware memory)
7. **Metamemory system**: CoverageStat, CalibrationTracker, strategy selection
8. **Schema reconstruction**: Schema-guided recall with default filling
9. **Schema congruence boost**: Accelerated consolidation for schema-congruent memories

### Phase 4 (Advanced)
10. **Source confusion detection**: Automated reality monitoring
11. **Adaptive monitoring budget**: Dynamic trigger prioritization based on resource cost
12. **Calibration feedback loop**: Continuous confidence adjustment from downstream validation

---

## Key Academic Sources (Complete Bibliography)

### Schema Theory
1. Bartlett, F.C. (1932). *Remembering*. Cambridge University Press.
2. Piaget, J. (1926/1952). *The Origins of Intelligence in Children*. International Universities Press.
3. Rumelhart, D.E. (1980). "Schemata: The building blocks of cognition." In *Theoretical Issues in Reading Comprehension*.
4. Tse, D., et al. (2007). "Schemas and Memory Consolidation." *Science*, 316(5821), 76-82.
5. van Kesteren, M.T.R., et al. (2012). "How schema and novelty augment memory formation." *Trends in Neuroscience*.

### Encoding Specificity / Context-Dependent Memory
6. Tulving, E. & Thomson, D.M. (1973). "Encoding specificity and retrieval processes in episodic memory." *Psychological Review*, 80(5), 352-373.
7. Godden, D.R. & Baddeley, A.D. (1975). "Context-dependent memory in two natural environments." *British Journal of Psychology*, 66(3), 325-331.
8. Eich, E. (1980). "The cue-dependent nature of state-dependent retrieval." *Memory & Cognition*, 8(2), 157-173.
9. Smith, S.M. & Vela, E. (2001). "Environmental context-dependent memory." *Psychonomic Bulletin & Review*, 8(2), 203-220.

### Transfer-Appropriate Processing
10. Morris, C.D., Bransford, J.D., & Franks, J.J. (1977). "Levels of processing versus transfer appropriate processing." *Journal of Verbal Learning and Verbal Behavior*, 16(5), 519-533.
11. Roediger, H.L., Weldon, M.S., & Challis, B.H. (1989). "Explaining dissociations between implicit and explicit measures of retention."
12. Blaxton, T.A. (1989). "Investigating dissociations among memory measures." *J. Exp. Psychol: LMC*, 15(4), 657-668.

### Interference Theory
13. McGeoch, J.A. (1932). "Forgetting and the law of disuse." *Psychological Review*, 39(4), 352-370.
14. Underwood, B.J. (1957). "Interference and forgetting." *Psychological Review*, 64(1), 49-60.
15. Anderson, M.C., Bjork, R.A., & Bjork, E.L. (1994). "Remembering can cause forgetting." *J. Exp. Psychol: LMC*, 20(5), 1063-1087.
16. Anderson, M.C. (2003). "Rethinking interference theory." *Journal of Memory and Language*, 49(4), 415-445.
17. Storm, B.C. (2011). "The benefit of forgetting." *Current Directions in Psychological Science*, 20(5), 291-295.

### Metamemory
18. Flavell, J.H. (1979). "Metacognition and cognitive monitoring." *American Psychologist*, 34(10), 906-911.
19. Nelson, T.O. & Narens, L. (1990). "Metamemory: A theoretical framework." *Psychology of Learning and Motivation*, 26, 125-173.
20. Koriat, A. (1997). "Monitoring one's own knowledge during study." *J. Exp. Psychol: General*, 126(4), 349-370.
21. Dunlosky, J. & Metcalfe, J. (2009). *Metamemory*. SAGE Publications.
22. Fleming, S.M. & Dolan, R.J. (2012). "The neural basis of metacognitive ability." *Phil. Trans. Royal Society B*, 367(1594), 1338-1349.

### Prospective Memory
23. Einstein, G.O. & McDaniel, M.A. (1990). "Normal aging and prospective memory." *J. Exp. Psychol: LMC*, 16(4), 717-726.
24. McDaniel, M.A. & Einstein, G.O. (2000). "Strategic and automatic processes in prospective memory retrieval." *Applied Cognitive Psychology*, 14(7), S127-S144.
25. Smith, R.E. (2003). "The cost of remembering to remember." *J. Exp. Psychol: LMC*, 29(3), 347-361.
26. Kliegel, M., et al. (2002). "Complex prospective memory and executive control." *Psychologische Beitrage*, 44, 303-318.

### Source Monitoring
27. Johnson, M.K., Hashtroudi, S., & Lindsay, D.S. (1993). "Source monitoring." *Psychological Bulletin*, 114(1), 3-28.
28. Lindsay, D.S. (2008). "Source monitoring." In *Learning and Memory: A Comprehensive Reference*, Vol. 2, 325-348.
29. Mitchell, K.J. & Johnson, M.K. (2009). "Source monitoring 15 years later." *Psychological Bulletin*, 135(4), 638-677.

### Additional Cross-Cutting
30. McClelland, J.L., McNaughton, B.L., & O'Reilly, R.C. (1995). "Why there are complementary learning systems." *Psychological Review*, 102(3), 419-457.
31. Tulving, E. (1972). "Episodic and semantic memory." In *Organization of Memory*.
32. Baddeley, A.D. (1974). "Working memory." *Psychology of Learning and Motivation*, 8, 47-89.

---

## Confidence Level

**High** for the cognitive science foundations -- these are well-established theories with decades of empirical support and hundreds of replications.

**Medium-High** for the implementation mappings -- the proposed Rust structs and algorithms are direct translations of the cognitive science principles, but the specific constants (signal weights, thresholds) will need tuning via empirical testing with real workflows.

**Medium** for the phasing recommendations -- depends on what bottlenecks emerge during Cortex v1 implementation.

---

## Methodology

- Theories drawn from established cognitive psychology literature (1932-2012)
- Implementation patterns designed against the existing Cortex architecture documented in `2026-03-31-nika-cortex-FINAL.md`
- Cross-referenced with AI memory systems literature from `memory-systems-2026-deep-research.md` and `memory-architecture-blueprint.md`
- Rust code patterns aligned with existing Nika workspace conventions
- No external search tools were available during this research session; analysis is based on deep knowledge of the cognitive science literature and the existing Cortex design documents
