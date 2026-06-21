# Tabletop — relate: discover, verify, render

**Date:** 2026-06-21
**Cards in scope:** 0002-relate
**Output spec:** .orbit/specs/2026-06-21-relate-discover-verify-render/spec.yaml (to be crystallised by /orb:spec)

---

## Capability ambition

Discover how DuckDB tables relate — find candidate FK edges, score each with its
evidence, **verify each against the data**, and write them as Frictionless
`foreignKeys`. Verified high-confidence edges auto-accept and compile to a render
with **no analyst toil**; only the genuinely uncertain band is surfaced for
optional review. The product gives the best model it can without making the
analyst adjudicate the obvious.

## Goal (Q1 / carving)

`dovetail relate` over a loaded DuckDB: discover candidate edges with core
evidence (value overlap, cardinality, name similarity), verify referential
integrity against the data, write scored `foreignKeys` into the descriptor with
auto-assigned status, and compile **accepted** edges to constraint DDL. Carving:
**discover + render** in one spec (the model is only real once it compiles to
something). Deferred: finetype semantic-type evidence (keeps finetype-model out
of this spec), and the Mermaid / brightfield / join-SQL renders (choice 0010).

## Values (Q2)

- **Load-bearing: best model, no analyst toil** (four pillars — author-level
  interaction: agents pay the compression cost so the author doesn't). A per-edge
  manual accept queue is exactly the toil this rejects.
- **Safety rail: no UNVERIFIED guess becomes a hard constraint** (choice 0007's
  real intent). Verification is what lets us honour both values at once — an edge
  that provably holds in the data is not a guess.

## Trade-offs (Q3)

- **Manual human-in-the-loop accept → verification-driven auto-accept.** The
  human only sees the uncertain band. *This is the headline reframe* — refines
  choice 0007 (see feedback).
- **All four evidence signals → core three (overlap, cardinality, name sim).**
  finetype semantic-type agreement deferred. *Acceptable* (keeps finetype-model
  out; a follow-up adds it).
- **Full render set → constraint DDL only.** Mermaid/join-SQL/brightfield are
  choice 0010's later spec. *Acceptable.*
- **One-step convenience → analyst loads, relate reads** (the seam, choice 0009).
  *Acceptable* and keeps choice 0001 pristine — see below.

### The seam (choice 0009, resolved here)

relate **reads an existing DuckDB** the analyst loaded (by running survey's
emitted `.sql` themselves). dovetail runs no load step; relate issues only read
queries to compute evidence and verify integrity. This resolves choice 0009
toward *"analyst loads; relate reads"* and keeps choice 0001 (emit-don't-execute)
clean — querying-for-discovery is like survey's detection sampling, not running a
pipeline.

### Verification posture

All scenarios `verifies: capability`. Fixtures are a DuckDB with known cases: a
holding FK (orders.customer_id ⊆ customers.id, zero orphans) that must
auto-accept; a near-miss with orphan rows that must NOT accept; a coincidental
low-cardinality overlap (e.g. a boolean/flag column) that must NOT accept; an
ambiguous mid-confidence edge that must surface as suggested. Real DuckDB
queries, real verification — no stand-ins.

## Failure modes (Q4)

- **Coincidental overlap passes verification and auto-accepts a non-relationship**
  (e.g. two `status` columns sharing values, or a boolean) → *halt-worthy*: the
  core risk. A real FK references a unique/PK parent — verification must include a
  parent-uniqueness/cardinality gate, not just orphan-count.
- **A real FK is missed** (overlap computed on a sample misses true membership) →
  *halt-worthy*: verify on full columns, not samples, for the integrity check.
- **Auto-accept compiles a constraint that breaks the load** → mitigated by
  verify-first: an accepted constraint provably holds at discovery time.
- relate connects to the wrong/empty DB. *Hygiene.*
- Pairwise comparison is O(columns²) and slow on wide schemas. *Hygiene* — prune
  by type/name before the expensive overlap query.

## Lateral approaches (Q5) — held in reserve

- **Scratch-DB runner** (relate runs survey's `.sql` itself): rejected as default
  (choice 0001 tension); the held fallback if the two-step seam proves clunky.
- **Discovery from descriptors/metadata only** (no live DB): rejected — weak
  evidence, no real value-overlap or verification.
- **Manual per-edge adjudication**: rejected — the toil the load-bearing value
  exists to eliminate.
- **finetype semantic-type agreement as a fourth signal**: held for the follow-up.

## Success criteria (Q6) — binary, measurable

1. relate connects to a DuckDB, reads table/column schemas, and computes pairwise
   candidate edges using the core signals.
2. For each candidate, relate verifies referential integrity against the FULL
   data (orphan-row count) plus a parent-uniqueness/cardinality gate.
3. Edges are written as `foreignKeys` with auto-assigned status: verified +
   high-confidence → accepted; verified-false → rejected; plausible-but-uncertain
   → suggested.
4. Only `accepted` edges compile to constraint DDL (choice 0007); suggested and
   rejected do not.
5. On the fixture DB: the holding FK auto-accepts with zero analyst action; the
   orphan near-miss and the coincidental boolean overlap do NOT accept; the
   ambiguous edge surfaces as suggested.

## Escalation triggers (Q7)

- **A verified edge looks coincidental** (passes orphan check but parent column is
  low-cardinality / non-unique) → the uniqueness gate should catch it; if the gate
  is ambiguous, surface rather than auto-accept.
- **Pairwise discovery exceeds a time budget on a wide schema** → halt; report and
  propose type/name pruning before the overlap pass.

## Adjacent code (Q8)

- New `dovetail-core` `relate` module: schema read, candidate generation,
  evidence scoring, verification queries, status assignment.
- `foreignKeys` written INTO each resource's Table Schema in the Data Package
  descriptor (Frictionless puts FKs inside tableSchema as
  `{fields, reference:{resource, fields}}`) — extends the `datapackage` module
  from the survey work, with dovetail's evidence/confidence/status as custom
  properties on each entry (choice 0003).
- Constraint-DDL renderer (accepted edges only) — likely a sibling of `emit`.
- `duckdb` crate as a real (not just dev) dependency now — relate queries a live
  DB. Note the boundary: read-for-discovery, not pipeline execution (choice 0001).
- `dovetail relate` CLI subcommand on the thin bin.

## Budget (Q9) — Claude-execution pace

- Schema read + candidate generation + evidence (overlap/cardinality/name): ~0.75 day.
- Verification queries + status assignment + DDL render: ~0.75 day.
- Fixtures (a multi-table DuckDB with the four known cases) + tests: ~0.5 day.
- **Total ≈ 2 working days.** No ML (finetype evidence deferred).

## Kill conditions (Q10)

- **Primary — "verification cleanly separates real FKs from coincidental overlap."**
  Kill signal: verified+accepted edges still include coincidental non-relationships
  (low-cardinality value coincidences). **Pivot:** strengthen the gate — require
  the parent side to be unique/PK-like (a real FK references a key), not just
  orphan-free. If even that overcalls, drop auto-accept to suggested for the
  coincidence-prone shapes.
- **Secondary — "analyst-loads-then-relate is the right seam."** Kill signal: the
  two-step workflow is too clunky in practice. **Pivot:** the held scratch-DB
  runner (choice 0009's other option).
- **Tertiary — "core signals suffice without finetype."** Kill signal: too many
  misses/overcalls without semantic typing. **Pivot:** pull in the deferred
  finetype semantic-type agreement signal.

## Feeds back to choices

- **Choice 0009 (the seam)** → RESOLVE toward *analyst loads; relate reads an
  existing DuckDB; dovetail runs no load step*. Move proposed → accepted once
  /orb:spec lands.
- **Choice 0007 (accepted-only compile)** → REFINE: `accepted` is reachable by
  **verification-based auto-accept** (verified + high-confidence), not only manual
  adjudication. The safety intent is unchanged — no *unverified* guess becomes a
  hard constraint — but the human is no longer in the loop for the obvious edges.
  This is the headline decision; update the choice body.
- **Choice 0003 (foreignKeys canonical)** → realised; FKs live inside each
  resource's Table Schema with evidence/confidence/status as custom properties.
- **Choice 0010 (relate renders)** → partly: constraint DDL is the first render;
  Mermaid / join SQL / brightfield deferred to its spec.

## Hot-wash

- **recurred:** the tension between human-in-the-loop (brief, choice 0007) and the
  four-pillars "don't burden the author" — resolved by making *verification*, not
  the analyst, the thing that earns "accepted".
- **surprised:** the author's reframe inverted my default. I'd carried the brief's
  "human-curated" framing as per-edge adjudication; the author's "best model, no
  extra work" exposed that as unexamined toil. Verification was the lever hiding in
  plain sight (relate already has to query the data).
- **friction:** "verified" needs a sharper definition than "zero orphans" — a
  boolean column has zero orphans against another boolean column. The
  parent-uniqueness gate is load-bearing and became the primary kill condition.
- **meta-patterns-for-future-tabletops:** when the brief says "human-in-the-loop",
  ask *which* human action and whether a machine-checkable property could stand in
  for it. Often the human is a proxy for "verified", and verification can be
  automated — turning a workflow burden into a check.
