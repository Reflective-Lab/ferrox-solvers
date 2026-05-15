# Ferrox

[![CI](https://github.com/Reflective-Lab/ferrox-solvers/actions/workflows/ci.yml/badge.svg)](https://github.com/Reflective-Lab/ferrox-solvers/actions/workflows/ci.yml)
[![Coverage](https://github.com/Reflective-Lab/ferrox-solvers/actions/workflows/coverage.yml/badge.svg)](https://github.com/Reflective-Lab/ferrox-solvers/actions/workflows/coverage.yml)
[![Security](https://github.com/Reflective-Lab/ferrox-solvers/actions/workflows/security.yml/badge.svg)](https://github.com/Reflective-Lab/ferrox-solvers/actions/workflows/security.yml)
[![Stability](https://github.com/Reflective-Lab/ferrox-solvers/actions/workflows/stability.yml/badge.svg)](https://github.com/Reflective-Lab/ferrox-solvers/actions/workflows/stability.yml)
[![Crates.io](https://img.shields.io/crates/v/converge-ferrox-solver.svg)](https://crates.io/crates/converge-ferrox-solver)
[![docs.rs](https://docs.rs/converge-ferrox-solver/badge.svg)](https://docs.rs/converge-ferrox-solver)
[![dependency status](https://deps.rs/repo/github/Reflective-Lab/ferrox-solvers/status.svg)](https://deps.rs/repo/github/Reflective-Lab/ferrox-solvers)
![MSRV](https://img.shields.io/badge/MSRV-1.94.0-blue)
<img alt="gitleaks badge" src="https://img.shields.io/badge/protected%20by-gitleaks-blue">
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Constraint solving as a Converge Suggestor.**

Cargo package: `converge-ferrox-solver`. Rust library name remains `ferrox`;
server and sys packages use the same `converge-ferrox-*` prefix.

LLMs are remarkable at understanding intent, drafting plans, explaining tradeoffs, and generating candidate solutions.
They are not optimisers.
Given a staffing problem with 60 tasks, 12 agents, and tight time windows, a language model will produce a reasonable-sounding schedule — but it cannot prove that schedule is the best possible, cannot guarantee every constraint is met, and cannot tell you how far from optimal it is.

Ferrox fills that gap.
It exposes industrial-strength mathematical solvers — Google OR-Tools CP-SAT, OR-Tools SimpleMinCostFlow, and HiGHS MIP — as first-class Converge Suggestors that live alongside LLM agents in the same Formation.
The LLM understands the business context.
Ferrox finds the provably correct answer within it.

---

## Repository Guide

Ferrox is a Converge extension. Converge owns the shared suggestor contract and
promotion authority; Ferrox owns solver models, native solver bindings,
confidence semantics, solver-backed suggestors, typed request/plan payloads,
typed proposal provenance, and suggestor-boundary tracing.

### Layout

```text
crates/ferrox/        Solver library and Converge suggestors
crates/ferrox-server/ gRPC service wrapper
crates/ortools-sys/   OR-Tools native binding wrapper
crates/highs-sys/     HiGHS native binding wrapper
examples/             Standalone examples
```

### Development

```sh
just check       # default, OR-Tools, HiGHS, and full feature checks
just test        # pure Rust tests
just test-full   # native solver tests, requires native deps
just lint        # fmt-check plus clippy
just deps        # build native solver dependencies
just doc         # generate docs
```

Ferrox earns its keep through provable optimality, but native solver builds are
not free. A first-time clone should budget 15-30 minutes for the OR-Tools C++
compile before OR-Tools-backed CP-SAT checks can run. HiGHS also needs a native
build for MIP checks:

```sh
make ortools
make highs
```

Downstream repos, CI jobs, and contributors that build Ferrox from outside this
workspace must point the sys crates at those build directories:

```sh
export FERROX_ORTOOLS_ROOT=/path/to/ferrox-solvers/vendor/ortools/build
export FERROX_HIGHS_ROOT=/path/to/ferrox-solvers/vendor/highs/build
```

Project docs:

- [AGENTS.md](AGENTS.md) - agent entrypoint and boundary rules.
- [CHANGELOG.md](CHANGELOG.md) - release notes.
- [CONTRIBUTING.md](CONTRIBUTING.md) - contribution guide.
- [SECURITY.md](SECURITY.md) - vulnerability reporting and operator notes.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) - community expectations.

---

## The Problem LLMs Cannot Solve Alone

Most real business decisions are constrained optimisation problems dressed in plain language:

- "Schedule our field crews for next week" — thousands of valid schedules exist; only one is cheapest
- "Which projects should we fund this quarter?" — capital is finite, returns are interdependent, regulations apply
- "Route our delivery vehicles through the city" — time windows, service durations, and a hard return deadline
- "Plan our factory floor for the next shift" — machines cannot share jobs; precedence cannot be violated

A language model is extraordinarily good at the surrounding work: understanding the request, pulling relevant context, communicating the result.
The inner loop — "given these constraints, what is the optimal assignment?" — is where mathematical solvers are decisive.

Ferrox makes those solvers available to any Converge Formation, with confidence scores that tell the Formation exactly how trustworthy each answer is.

---

## Formations and Suggestors

Converge is an open Agent OS built around two primitives:

**Suggestor** — an agent that reads facts from a shared context and proposes new facts, tagged with a confidence score.
Any number of Suggestors can run against the same context simultaneously.

**Formation** — a group of Suggestors registered in a single Engine.
The Engine runs all accepting Suggestors, collects their proposals, and lets consumers pick the highest-confidence answer — or compare all of them.

Ferrox contributes Suggestors that compete on provable quality.
For every problem class, two implementations are available:

Solver requests and plans are typed Converge fact payloads inside the process.
The `ContextKey` and fact id route the fact; the payload family and version
identify the schema. JSON or other wire serialization belongs at process,
storage, replay, and transport borders, not between Ferrox Suggestors and the
Converge context.

| Problem class | Fast / portable surface | Confidence | Native / stronger surface | Confidence |
|---|---|---|---|---|
| Task scheduling (MAATW) | `converge_optimization::suggestors::GreedySchedulerSuggestor` | ≤ 0.65 | `CpSatSchedulerSuggestor` | ≤ 1.0 |
| Job Shop scheduling | `PackSuggestor<JobShopSchedulingPack>` or `GreedyJobShopSuggestor` | ≤ 0.75 | `CpSatJobShopSuggestor` | ≤ 1.0 |
| Vehicle routing (VRPTW) | `converge_optimization::suggestors::NearestNeighborTimeWindowRoutingSuggestor` | ≤ 0.60 | `CpSatVrptwSuggestor` | ≤ 1.0 |
| Linear programs | — | — | `GlopLpSuggestor` | ≤ 1.0 |
| Mixed-integer programs | — | — | `HighsMipSuggestor` | ≤ 1.0 |
| General CP-SAT | — | — | `CpSatSuggestor` | ≤ 1.0 |
| Network flow / min-cost flow | — | — | `MinCostFlowSuggestor` | ≤ 1.0 |

The greedy Suggestor answers in microseconds.
The solver Suggestor runs in parallel and either proves the greedy answer was optimal, or beats it.
The Formation selects by confidence — no orchestration code required.

For product code that needs to choose a registration set, Ferrox exposes a
machine-readable catalog:

```rust
use ferrox::catalog::{recommend_for_use_case, CommonUseCase};

let candidates = recommend_for_use_case(CommonUseCase::FieldCrewScheduling);
assert_eq!(candidates[0].symbol, "CpSatSchedulerSuggestor");
```

The catalog includes Ferrox native Suggestors, `converge-optimization` pure
Rust baselines and Packs, and explicit deferred entries such as SMT
counterexample search. Use it to avoid routing a use case to a solver just
because the solver happens to be compiled in.

### How confidence works

```
optimal solution found  →  confidence = visit_ratio  (1.0 if all tasks scheduled)
feasible but not proven →  confidence = visit_ratio × 0.85
infeasible or error     →  confidence = 0.0
greedy heuristic        →  confidence = throughput_ratio × cap  (cap ≤ 0.65)
```

A greedy plan capped at 0.65 will always yield to a proven optimal plan at 1.0.
If the solver times out with a feasible-but-not-proven plan (0.85), the Formation can still use it with appropriate uncertainty.

---

## Benchmarks

### Multi-Agent Task Assignment with Time Windows
60 tasks · 12 specialist agents · 5 skills · 360 min horizon

```
GreedySchedulerSuggestor   56 / 60 tasks   93.3%    0.03 ms   confidence 0.60
CpSatSchedulerSuggestor    60 / 60 tasks  100.0%     260 ms   confidence 1.00  ← optimal
```

### Job Shop Scheduling
15 jobs × 10 machines — Taillard-style instance

```
GreedyJobShopSuggestor     makespan 2038    0.3 ms   confidence 0.55
CpSatJobShopSuggestor      makespan 1044   30.0 s    confidence 0.85  ← feasible (48.8% improvement)
```

### Vehicle Routing with Time Windows
20 customers — Solomon-style instance · depot at (50, 50) · horizon 480 min

```
NearestNeighborSuggestor    5 / 20 customers   < 0.1 ms   confidence 0.15
CpSatVrptwSuggestor         8 / 20 customers     4.9 s    confidence 0.40  ← optimal (+60%)
```

---

## Four Business Flows

Each flow below shows how LLMs, Cedar policy, Knapsack/MIP, and constraint solvers work as peers in a Formation.
No single technology handles the full decision.
Each does what it is actually good at.

---

### Flow 1 — Investment Portfolio Allocation

**Scenario:** A fund manager wants to deploy €50 M across a shortlist of projects.
Each project has a return estimate, a risk score, a sector, and a minimum ticket size.
Regulations prohibit concentrating more than 40 % of capital in any single sector.
ESG policy requires at least three sustainable projects in the portfolio.

| Step | Actor | Role |
|------|-------|------|
| 1 | LLM Suggestor | Reads analyst notes and CRM history. Writes a narrative summary of each candidate to `ContextKey::Seeds`. Tags which candidates are flagged as ESG-eligible. |
| 2 | Cedar policy | Enforces hard regulatory rules — sector concentration cap, minimum ticket, excluded geographies. Any candidate that violates policy is removed from context before solvers see it. |
| 3 | `HighsMipSuggestor` | Formulates a binary knapsack: select projects to maximise expected return subject to total capital ≤ €50 M, sector caps, and ESG count ≥ 3. Returns the optimal portfolio with proven optimality gap. |
| 4 | LLM Suggestor | Reads the optimal portfolio from `ContextKey::Strategies`. Drafts the investment committee memo, explains the tradeoffs, and flags any candidates that were close to inclusion. |

**Why the solver, not the LLM, picks the portfolio:**
The MIP solver can evaluate 2^30 combinations in seconds and prove no better combination exists.
An LLM cannot.
It will produce a plausible-sounding list that may miss €2 M of return and violate a sector cap it failed to track.

---

### Flow 2 — Field Service Crew Scheduling

**Scenario:** A utilities company has 40 field technicians and 180 work orders for the coming week.
Each work order requires a specific certification, has a customer-committed time window, and a service duration.
Technicians have different skill sets, working hours, and geographic zones.
Labour agreements cap overtime.

| Step | Actor | Role |
|------|-------|------|
| 1 | LLM Suggestor | Parses the incoming work orders from unstructured emails and PDFs. Extracts customer, location, window, skill requirement, and priority. Seeds `scheduling-request:week-42` into context. |
| 2 | Cedar policy | Enforces labour agreement rules — no technician works more than 10 hours, no consecutive overnight shifts, union jurisdiction by zone. Removes violations before the scheduler runs. |
| 3 | `GreedySchedulerSuggestor` | Runs EDF + earliest-available in < 1 ms. Immediately seeds a baseline plan. Confidence ≤ 0.65. |
| 3 | `CpSatSchedulerSuggestor` | Runs in parallel. Finds the maximum number of work orders that can be scheduled within all constraints. Returns optimal (or feasible) plan with proven gap. Confidence ≤ 1.0. |
| 4 | LLM Suggestor | Takes the CP-SAT plan from context. Writes the technician briefing emails, drafts customer notifications for unscheduled orders, and suggests overflow options. |

**Why this cannot be done with an LLM alone:**
A 40-person, 180-job scheduling problem with time windows is NP-hard.
The LLM would produce a schedule that looks reasonable but misses 20–30 jobs a skilled human planner or solver would have fit.
The CP-SAT model proves the maximum achievable.

---

### Flow 3 — Multi-Stop Delivery Routing

**Scenario:** A logistics operator runs a same-day delivery fleet.
At 9 AM, 60 new delivery requests arrive with pick-up windows, drop-off windows, and service times.
Each vehicle must return to the depot by 6 PM.
The objective is to maximise deliveries completed; cost per vehicle is fixed so maximising throughput maximises margin.

| Step | Actor | Role |
|------|-------|------|
| 1 | LLM Suggestor | Reads customer messages, extracts delivery addresses, time preferences, and special instructions (fragile, signature required). Geocodes addresses. Seeds `vrptw-request:2026-04-22` into context. |
| 2 | Cedar policy | Applies driver hours-of-service rules, vehicle payload limits, and restricted delivery zones. Flags any delivery that cannot legally be served and writes that back to context before routing. |
| 3 | `NearestNeighborSuggestor` | Runs in < 1 ms. Provides an instant baseline route for dispatch visibility. |
| 3 | `CpSatVrptwSuggestor` | Runs in parallel. Uses `AddCircuit` + time-window propagation to find the route that maximises customers visited while respecting all windows and the return deadline. Returns proven-optimal (or best-found-feasible) route. |
| 4 | LLM Suggestor | Reads the optimal route from context. Generates turn-by-turn driver instructions, customer ETA notifications, and a capacity summary for operations. |

**Why routing is not a prompt:**
VRPTW is one of the canonical NP-hard combinatorial problems in operations research.
A greedy nearest-neighbour misses 60 % more customers than CP-SAT on tight-window instances.
Those missed deliveries are missed revenue and broken SLAs.

---

### Flow 4 — Factory Production Scheduling

**Scenario:** A precision manufacturer runs a 10-machine job shop.
Each evening, a new batch of 15–30 jobs arrives, each requiring a fixed sequence of machining operations.
No two jobs can occupy the same machine simultaneously.
The target is to minimise makespan — finishing the batch as early as possible to free capacity for the next shift.

| Step | Actor | Role |
|------|-------|------|
| 1 | LLM Suggestor | Reads the ERP export and production notes. Identifies rush jobs, quality-hold items, and maintenance windows. Seeds `jspbench-request:shift-evening` into context with a structured `JobShopRequest`. |
| 2 | Cedar policy | Enforces maintenance windows (machine M03 offline 22:00–23:00), operator certification requirements for certain operations, and priority overrides for rush orders. Modifies the request in context accordingly. |
| 3 | `GreedyJobShopSuggestor` | SPT list scheduling in < 1 ms. Provides an immediate baseline for the floor supervisor screen. Confidence 0.55. |
| 3 | `CpSatJobShopSuggestor` | CP-SAT interval variables + `NoOverlap` per machine. Proven minimum makespan. Confidence 1.0 on optimal, 0.85 if time budget exhausted. On the 15×10 benchmark: 48.8 % shorter than greedy. |
| 4 | LLM Suggestor | Takes the optimal schedule from context. Generates shift handover notes, machine loading reports, and flags if any rush jobs have been delayed beyond their committed window. |

**Why the floor supervisor needs the solver, not just the LLM:**
A job shop with 15 jobs and 10 machines has more valid orderings than atoms in the observable universe.
Greedy SPT gets you to the floor faster.
CP-SAT gets you out of the factory 49 % sooner.
On a three-shift operation, that difference compounds into days of recovered capacity per month.

---

## Solvers

| Library | Version | Algorithm | Best for |
|---------|---------|-----------|----------|
| Google OR-Tools CP-SAT / SimpleMinCostFlow | 9.15 | DPLL(T), LNS, clause learning, network simplex-style flow | Scheduling, routing, combinatorial assignment, min-cost flow |
| HiGHS | 1.14 | Revised simplex + branch-and-cut | LP relaxations, pure MIP, capital allocation |

Both are compiled from source and linked statically into the gRPC server.
No external services, no API calls, no rate limits.

Solver-backed Ferrox-owned plans carry Converge's shared
`ExecutionIdentity` in their `solver_identity` field alongside the `solver`
label. The identity records the native backend, pinned version, expected and
actual checkout commit, source mode, build flags, runtime solver config, and
producer crate version so audit can distinguish the same model solved by
different native bits or runtime settings. Greedy plans use the same contract
with no native identity.

When Ferrox solves a generic Converge contract, such as
`FormationPlan`, solver identity stays out of the generic plan. The CP-SAT
formation suggestor emits Converge's generic
`converge.execution_identity.evidence` evaluation fact linked to the strategy
plan id.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  Converge Formation (Engine)                                 │
│                                                              │
│  ContextKey::Seeds                                           │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  vrptw-request:run-001    { depot, customers, ... }  │    │
│  └─────────────────────────────────────────────────────┘    │
│                          │                                   │
│           ┌──────────────┼──────────────┐                   │
│           ▼                             ▼                   │
│  NearestNeighborSuggestor      CpSatVrptwSuggestor          │
│  (sub-ms, confidence 0.15)     (seconds, confidence 0.40)   │
│           │                             │                   │
│           ▼                             ▼                   │
│  ContextKey::Strategies                                      │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  vrptw-plan-greedy:run-001   { route: [...] }        │    │
│  │  vrptw-plan-cpsat:run-001    { route: [...] }        │    │
│  └─────────────────────────────────────────────────────┘    │
│                          │                                   │
│           ▼ (highest confidence wins)                        │
│  LLM Suggestor reads vrptw-plan-cpsat:run-001               │
│  → drafts driver instructions, customer ETAs                │
└──────────────────────────────────────────────────────────────┘
```

Each Suggestor writes to a solver-prefixed key so all plans coexist.
Downstream consumers select by confidence score.
The LLM never sees raw constraint data — it sees structured, solved plans.

---

## Running the Showcases

Build the C++ solver libraries first:

```bash
make all          # builds OR-Tools v9.15 and HiGHS v1.14 from source
```

Then run any showcase:

```bash
just example-maatw      # Multi-Agent Task Assignment with Time Windows
just example-jspbench   # Job Shop Scheduling (15 jobs × 10 machines)
just example-vrptw      # Vehicle Routing with Time Windows (20 customers)
just example-cp         # Sudoku via CP-SAT (generic CpSatSuggestor)
just example-flow       # Min-cost network flow (MinCostFlowSuggestor)
just example-mip        # Capital allocation via HiGHS MIP
```

Each example registers both a greedy and an optimal Suggestor in a Formation, runs both, and prints the quality comparison with confidence scores.

---

## gRPC Server

Ferrox ships a production-ready gRPC server that exposes all Suggestors over the network.
Any Converge Formation can call it as a Provider.

```bash
just server             # local, no TLS
just up                 # Docker Compose with mTLS
```

Authentication via `Authorization: Bearer <token>` (set `FERROX_AUTH_TOKEN`).
TLS certificates in `./tls/` — generate dev certs with `just tls-dev-certs`.
Native solve calls run on Tokio's blocking pool behind a concurrency limiter.
Set `FERROX_SERVER_MAX_BLOCKING_SOLVES` to raise the default limit of `1` when
the host has enough cores and memory for concurrent solver requests.

---

## Adding a Suggestor

Implement the `Suggestor` trait from `converge-pack`:

```rust
#[async_trait]
impl Suggestor for MyCustomSuggestor {
    fn name(&self) -> &str { "MyCustomSuggestor" }

    fn dependencies(&self) -> &[ContextKey] { &[ContextKey::Seeds] }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("O(n log n) — describe what this costs and what scale it handles")
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds).iter().any(|f| f.id.starts_with("my-request:"))
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        // read from Seeds, compute, write to Strategies
        AgentEffect::with_proposals(vec![...])
    }
}
```

Register it in an Engine:

```rust
let mut engine = Engine::new();
engine.register_suggestor(GreedySuggestor);
engine.register_suggestor(MyCustomSuggestor);   // competes on the same seeds
```

The Formation handles concurrency, confidence ranking, and fact deduplication.

---

## Project Layout

```
crates/
  ferrox/               Core library — all Suggestors and problem types
    src/
      scheduling/       MAATW — task assignment with time windows
      jobshop/          JSP  — job shop scheduling (N jobs, M machines)
      vrptw/            VRPTW — vehicle routing with time windows
      cp/               Generic CP-SAT Suggestor (any CpSatRequest)
      lp/               Generic LP Suggestor (GLOP)
      network_flow/     Network-flow Suggestor (OR-Tools SimpleMinCostFlow)
      mip/              Generic MIP Suggestor (HiGHS)
  ferrox-server/        gRPC server (TLS, auth, Docker)
  ortools-sys/          Rust FFI to OR-Tools CP-SAT, GLOP, and SimpleMinCostFlow
  highs-sys/            Rust FFI to HiGHS

examples/
  maatw/                Formation demo: task scheduling
  jspbench/             Formation demo: job shop
  vrptw/                Formation demo: vehicle routing
  cp_sudoku/            Formation demo: sudoku via generic CP-SAT
  network_flow/         Formation demo: min-cost flow via SimpleMinCostFlow
  highs_mip/            Formation demo: capital allocation via MIP

proto/                  Protobuf definitions (ferrox.v1)
vendor/
  ortools/              OR-Tools v9.15 source
  highs/                HiGHS v1.14 source
```

---

## Why Rust

Rust gives ferrox zero-copy FFI to C++ solver libraries with no garbage-collection pauses, no JVM warm-up, and no Python GIL.
The OR-Tools and HiGHS bindings call directly into the solver shared libraries.
An end-to-end Formation run — seed to plan — adds no observable latency beyond what the solver itself takes.

The `unsafe` keyword does not appear in ferrox library code.
All C boundary code is in `ortools-sys` and `highs-sys`, wrapped in safe Rust APIs before any Suggestor touches them.
