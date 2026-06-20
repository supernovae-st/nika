# Crate spec — `nika-policy`

| | |
|---|---|
| Status | **L1.5 admission target · DESIGN LOCKED · impl sequenced after the kernel migration settles** (Phase-B slice step 8 · cap-gate-before-IO · per D-2026-05-22-N9 + D-2026-06-10-N6) |
| Layer | L1.5 — service crate · the capability/policy enforcement layer between the L1 effects and the L2 verbs |
| Design | `PolicyEnforcer` impl of a NEW L0.5 `nika_kernel::policy::PolicyChecker` trait (operator-locked 2026-06-10) · **compose-only** — owns what the effects don't |
| LOC budget | well under the ≤1500/file + ≤15k/crate caps (enforced live by vectors 12+24) · live count · `scripts/crate-metrics.sh nika-policy` |
| Crate version | tracks workspace (`0.90.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` |
| NIKA codes | TBD at impl — kernel-side `PolicyError` per session-B "Pattern A" (typed error at the kernel) |

---

## §0 · Architecture — LOCKED (operator-confirmed 2026-06-10)

Two forks were surfaced (ASK · architecture + security + multi-component) and
resolved by the operator:

1. **A new kernel trait `nika_kernel::policy::PolicyChecker` (L0.5).** Verbs
   depend on the TRAIT, fully decoupled from this crate (Pattern 1 · kernel
   traits upfront · matches the fs/http/blob/process `*Dyn` companion shape).
   The brouillon's `nika_kernel::policy::PolicyChecker` reference was to a trait
   that **does not exist in Diamond** — it must be CRAFTED, following session
   B's in-flight "Pattern A" (typed `PolicyError` at the kernel ·
   `#[trait_variant::make(PolicyCheckerDyn: Send)]`).

2. **Compose-only — NOT brouillon parity.** `nika-policy` owns ONLY the
   policy primitives the effect crates don't:
   - **host allow/block lists** (`allowed_hosts` / `blocked_hosts` for fetch)
   - **token budgets** (cost/spend caps · genuinely net-new · in no effect)
   - **program/tool allow-block** (operator policy · e.g. "only git, cargo, npm")
   - the **`pre_validated` handshake** (when policy has done intent-aware
     validation, it vouches → the effect skips its baseline floor)

   It does **NOT** re-implement SSRF (lives in `nika-http` · safe-by-default)
   nor the dangerous-command blocklist (lives in `nika-exec-runner` ·
   safe-by-default). The brouillon's 1300 LOC was mostly the now-redundant SSRF
   + DNS-rebind logic — dropped. The mechanism is conservative, the policy is
   configurable + additive.

### The composition (defense-in-depth · two orthogonal layers)

```text
   L2 verb (exec/infer/invoke/agent)
        |  asks once:  policy.check_exec(cmd) / check_fetch(url) / check_budget(n)
        v
   L1.5 nika-policy  -- operator config: host lists, budgets, program allow-block
        |  on approve -> may set command.pre_validated = true  (intent-aware vouch)
        v
   L1 effect (nika-exec-runner / nika-http / ...)
           safe-by-default FLOOR always runs UNLESS pre_validated
           (blocklist / SSRF -- the mechanism's own conservative defense)
```

**Why two layers, not one chokepoint**: the effect floor is the un-bypassable
safety net (even a buggy/missing policy can't make exec unsafe); the policy
layer is the configurable, intent-aware, operator-owned enforcement.
compose-only keeps each concern in exactly one place (no SSRF/blocklist
duplication).

### Sequencing (concurrent-session discipline)

The kernel `PolicyChecker` trait is an L0.5 change. Session B is **actively
migrating the entire kernel** to "Pattern A" (every io trait -> typed error at
the kernel · `errors.rs` touched per-commit · browser/input/a11y/ocr/screen
in-flight 2026-06-10). Adding a `policy` module into a kernel being rewritten
module-by-module = high collision risk + would have to match a pattern still
landing. **Therefore the kernel-trait impl is sequenced AFTER session B's
kernel migration settles** (watch for the migration's close commit). This spec
(docs/ · collision-free territory) locks the design now so the implementation
is a clean execution once the kernel stabilises.

---

## §1 · The kernel trait (CRAFT at L0.5 · `nika-kernel-core/src/io/policy.rs`)

```rust
/// What a verb asks the policy before performing a capability.
#[non_exhaustive]
pub enum PolicyDecision {
    Allow,
    Block { reason: String },
}
impl PolicyDecision { pub fn is_allowed(&self) -> bool; }

/// Capability-policy enforcement -- the configurable layer above the floors.
#[trait_variant::make(PolicyCheckerDyn: Send)]
pub trait PolicyChecker: Send + Sync {
    /// May this program/command run? (operator allow-block, NOT the dangerous-
    /// pattern blocklist -- that is nika-exec-runner's floor.)
    fn check_exec(&self, command: &str) -> PolicyDecision;
    /// May this URL be fetched? (host allow/block, NOT SSRF -- nika-http's floor.)
    fn check_fetch(&self, url: &str) -> PolicyDecision;
    /// May `tokens` be spent against the budget?
    fn check_budget(&self, tokens: u64) -> PolicyDecision;
    /// May this tool be invoked? (tool allow-block.)
    fn check_tool(&self, tool: &str) -> PolicyDecision;
}
```

`PolicyError` (kernel-side · Pattern A) for the enforce path:
`Denied { reason }` · `BudgetExceeded { limit, requested }`.

## §2 · The crate (`nika-policy` · L1.5)

```rust
pub struct PolicyConfig { /* allowed_hosts, blocked_hosts, allowed_programs,
                            blocked_programs, allowed_tools, token_limit, ... */ } // serde
pub struct PolicyEnforcer { config: PolicyConfig, budget: TokenBudget }
pub struct TokenBudget { /* limit: Option<u64>, used: u64 */ }   // can_spend/spend/remaining

impl PolicyCheckerDyn for PolicyEnforcer { /* the 4 checks */ }
impl PolicyEnforcer {
    pub fn new(config: PolicyConfig) -> Self;
    pub fn enforce(&self, d: PolicyDecision) -> Result<(), PolicyError>;
    // budget mutation: reserve_tokens / record_spend / remaining_budget
    // the handshake: vouch(&self, cmd) sets pre_validated after an Allow
}
```

CRAFT from `git show brouillon:tools/nika-policy/src/lib.rs` — KEEP the budget +
host-list + decision logic, DROP every SSRF/DNS-rebind fn (`is_ssrf_blocked`,
`resolve_and_pin_ssrf`, `ssrf_safe_redirect_policy` · now nika-http's job).

## §3 · The 12 gates (plan)

| Gate | Plan |
|---|---|
| 1 SPEC | this file |
| 2 TDD | host allow/block · budget can_spend/overspend · program allow-block · tool gating · the vouch->pre_validated handshake · `PolicyChecker` object-safety |
| 5 MUTATION | budget arithmetic + decision branches = highly mutable · pin each |
| 6 PROPERTY | any host on blocked_hosts -> Block · any spend > remaining -> Block · allow-list exactness |
| 10 PARITY | brouillon budget + host-list + decision vectors re-asserted · SSRF DROPPED (now http) |
| 11 REVIEW | 3-agent swarm -- security-adversarial (can a host/program slip the allow-block? budget underflow? does compose-only leave a gap vs the floors?) |

## §4 · Consumers

`nika-verb-exec` (s11 · `check_exec` + vouch before exec-runner), `nika-verb-
infer` (s9 · `check_budget` before the LLM call · host-check for the provider),
`nika-verb-invoke` (s14 · `check_tool`), `nika-engine` (s17 · holds the
`PolicyEnforcer` · injects `&dyn PolicyCheckerDyn` into verbs).

## §5 · Dependencies

| dep | why |
|---|---|
| `nika-kernel` (path) | the `PolicyChecker` trait + `PolicyError` + `ShellCommand` (for vouch) |
| `serde` | `PolicyConfig` (operator-authored YAML/JSON) |
| dev: `proptest` | Gate 6 |

No SSRF/HTTP/process deps — composition, not re-implementation.
