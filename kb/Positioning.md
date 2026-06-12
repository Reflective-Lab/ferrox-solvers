---
tags: [positioning, pitch, optimization]
source: llm
date: 2026-06-12
---
# Positioning

Why Ferrox exists, why it plays well with LLMs, and what the underlying
solvers are uniquely good at. Companion pitches live in the Arbiter and Soter
knowledge bases; this note is the Ferrox chapter of the same story.

## Elevator Pitch

Ferrox gives the Converge platform **exact mathematical optimization as a
safe, product-shaped capability**. It wraps two world-class native solvers —
Google OR-Tools (CP-SAT, GLOP, min-cost flow) and HiGHS (MIP/LP) — behind
narrow, `unsafe`-forbidden Rust contracts, and exposes them not as a pile of
algorithm entrypoints but as named *Suggestors*: typed request in, typed plan
out, with explicit confidence semantics.

It matters because the problems it solves — crew scheduling, job-shop
sequencing, vehicle routing, flow allocation, resource assignment — are
exactly the problems where "pretty good" answers leak real money, and where
heuristics quietly degrade as instances grow.

## Why It Plays Well With LLMs

An LLM is excellent at *formulating* an optimization problem from messy human
intent, and provably bad at *solving* one — it cannot do exact combinatorial
search, and it cannot tell you whether its schedule is optimal or merely
plausible. Ferrox is the other half of that brain:

- The Suggestor contract (stable seed prefixes, typed JSON-shaped
  request/plan data) is precisely the shape a tool-using agent needs.
- The machine-readable catalog (`ferrox::catalog::recommend_for_use_case`)
  lets an agent *select the right solver* from product intent without knowing
  solver internals.
- The LLM translates the world into a model; Ferrox returns a
  provably-optimal (or gap-quantified) plan with diagnostics the LLM can
  explain back to the user.

Language in, certainty out.

## OR-Tools

OR-Tools is Google's open-source optimization suite, and its crown jewel is
**CP-SAT** — by broad consensus the best constraint-programming solver in the
world (it has dominated the MiniZinc Challenge for years). CP-SAT fuses
SAT-solving with lazy clause generation and linear relaxations, which makes
it unbeatable on *finite-domain combinatorial* problems that mix logic and
arithmetic: scheduling with optional intervals and no-overlap, job-shop
sequencing, routing via circuit constraints, and anything with
`AllDifferent`-style structure. That is the niche where pure MIP solvers
struggle — disjunctive, highly combinatorial models with weak linear
relaxations.

Around CP-SAT, the suite carries **GLOP** (a precise simplex LP solver),
**PDLP** (first-order method for huge-scale LP/QP), the specialized
**routing library** (guided local search for VRP variants), and exact graph
algorithms — **max flow, min-cost flow, linear-sum assignment**
(Hungarian-style). Ferrox deliberately wraps only the slices with a product
case: CP-SAT, GLOP, and SimpleMinCostFlow. See
[[Architecture/Capability Map]] for the full exposure posture.

## HiGHS And The CVE Angle

HiGHS is the leading open-source **MIP/LP solver** — dual-revised simplex,
interior point, and branch-and-cut, born from Edinburgh research and now the
default backend in SciPy and many modeling stacks. It covers the workload
CP-SAT is wrong for: continuous and mixed-integer *linear* models where
integrality and a quantified optimality gap matter (`HighsMipSuggestor`
reports that gap explicitly).

From a security standpoint, HiGHS is the easy dependency to defend: a
self-contained C++ codebase with essentially no bundled third-party
dependency tree, so its CVE surface is minimal. Contrast OR-Tools, which
bundles Abseil and Protobuf; Protobuf in particular carries a steady drip of
CVEs (parsing DoS issues), which is why
[[Building/Dependency Policy]] names "CVE in a bundled dep" as the main
*unscheduled* upgrade trigger for OR-Tools, while HiGHS upgrades stay
leisurely and fix-driven. Both are vendored from tag-pinned, commit-verified
sources and swept by `just security-audit`.

## Boundaries (One-Line Reminders)

- Ferrox answers: *what is the best feasible plan?* (`Searched`,
  optimization)
- Arbiter answers: *should this concrete request be allowed now?*
  (`Decided`)
- Soter answers: *can any modeled request violate this invariant?*
  (`Searched`, symbolic)
- Ferrox is not an SMT layer and CP-SAT is not SMT — see the SMT posture in
  [[Architecture/Capability Map]].
