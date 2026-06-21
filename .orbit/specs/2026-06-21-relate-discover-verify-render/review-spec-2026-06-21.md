# Spec Review

**Date:** 2026-06-21
**Reviewer:** Context-separated agent (fresh session)
**Spec:** 2026-06-21-relate-discover-verify-render
**Verdict:** APPROVE

---

## Review Depth

| Pass | Triggered by | Findings |
|------|-------------|----------|
| 1 — Structural scan | always | 2 |
| 2 — Assumption & failure | content signals (auto-accept inverts an accepted choice; full-data verification on a live DuckDB; FK write into a write-only descriptor type) | 2 |
| 3 — Adversarial | the auto-accept reframe contradicts the *currently accepted* text of choice 0007 | 1 |

Pass 3 was reached because the spec's headline move — verification-driven auto-accept — directly contradicts the decision text of an `accepted` choice (0007: "a human moves it to accepted... human curation is the gate"). The tabletop names this and commits to updating the choice, so it is a substrate-sync gap, not a design contradiction — hence a finding rather than a BLOCK.

---

## Findings

### [MEDIUM] The auto-accept reframe contradicts the still-current text of choice 0007 and the relate card
**Category:** constraint-conflict
**Pass:** 3
**Description:** The spec's load-bearing decision is that a *verified* high-confidence edge auto-accepts with no analyst action (goal, ac-04). But choice 0007 — status `accepted` — currently reads: "Discovery records each edge as `suggested`; **a human moves it to accepted** or rejected... human curation is the gate," and lists as a rejected option exactly what this spec now does ("Auto-promote high-confidence edges... a guess can become a hard constraint without a human seeing it"). Card 0002's accepted scenario likewise says "a candidate edge is recorded as a foreignKey with status **suggested**... I **mark it accepted**." So an implementer reading the substrate (not the tabletop) would find the spec and the accepted choice in direct conflict over who reaches `accepted`. The tabletop (Feeds-back-to-choices, Hot-wash) explicitly commits to *refining* choice 0007 — "`accepted` is reachable by verification-based auto-accept... update the choice body" — and to resolving choice 0009 (still `proposed`) to accepted. That update has not landed: choice 0007 still carries the pre-reframe text and choice 0009 is still `proposed`.
**Evidence:** choice 0007 decision text ("a human moves it to accepted"; "Auto-promote high-confidence edges" listed under *rejected* options); card 0002 scenario "Accept an edge and compile constraint DDL" (given "a suggested edge I have reviewed", when "I mark it accepted"); spec ac-04 ("verified AND high-confidence → accepted (no analyst action)"); tabletop lines 152–164 (the commitment to refine 0007 and resolve 0009).
**Recommendation:** This is a substrate-sync action, not a spec rework — the reframe is the author's explicit steer and the safety intent (no *unverified* edge becomes a constraint) is preserved by the ac-03 gate. Before or during implementation: (a) update choice 0007's decision body so `accepted` is reachable by verification-based auto-accept, keeping the "no unverified guess becomes a hard constraint" intent; (b) move choice 0009 proposed → accepted ("analyst loads; relate reads"); (c) refresh card 0002's first scenario so `accepted` is not described as exclusively human-set. The safety rail is unchanged — verification, not the analyst, now earns `accepted`. Not a reason to send the spec back; flagging so the implementer reconciles the choices rather than coding against the stale text.

### [LOW] "High-confidence" threshold for auto-accept is unpinned
**Category:** missing-requirement
**Pass:** 1
**Description:** ac-04 gates auto-accept on "verified **AND high-confidence**", and routes "plausible but unverified / ambiguous → suggested." But the confidence score is composed from three signals (ac-02: value overlap, cardinality, name similarity) with no pinned threshold or weighting that separates "high-confidence accepted" from "mid-confidence suggested." The fixture's case (d) is "an ambiguous mid-confidence edge that must surface as suggested" — so the band boundary is load-bearing for ac-01/ac-04, yet undefined. The AC is satisfiable by recording *a* threshold, so this is determinism-of-default, not untestability.
**Evidence:** ac-04 ("verified AND high-confidence → accepted"; "plausible but unverified / ambiguous → suggested"); ac-01 case (d) ("an ambiguous mid-confidence edge that must surface as suggested"); ac-02 names the three signals but no scoring/threshold.
**Recommendation:** Pin the threshold (and the signal combination) in implementation and record it in the ac-04 status-assignment logic plus the fixture manifest, so case (d)'s "suggested not accepted" outcome is reproducible and not a tuning accident. Note that *verification* (ac-03) is the hard safety gate regardless of where the confidence line sits — a mis-tuned threshold can only over- or under-*suggest*, never auto-accept an unverified edge. Implementation-time decision; capture it, no spec edit required.

### [LOW] duckdb must move from dev-dependency to real dependency, but no AC pins it
**Category:** test-gap
**Pass:** 2
**Description:** The spec's implementation note and the tabletop (Q8) both state "duckdb becomes a real (non-dev) dependency of dovetail-core." Today `duckdb` sits in `[dev-dependencies]` of `crates/dovetail-core/Cargo.toml` (it was test-only for survey's round-trip). relate issues read queries at runtime, so the move is required — but no AC asserts it, and the boundary it must respect (read-for-discovery, not pipeline execution — choice 0001) is stated only in prose. A build could pass tests with duckdb still dev-only if relate's query code lived under `#[cfg(test)]`, silently violating the runtime intent.
**Evidence:** `crates/dovetail-core/Cargo.toml` line 24–25 (`[dev-dependencies]` / `duckdb = ...`); spec implementation note ("duckdb becomes a real (non-dev) dependency"); choice 0001 (emit-don't-execute) — the boundary the move must not cross.
**Recommendation:** Acceptable as-is — ac-02/ac-03/ac-07 all require relate to connect-and-query at the CLI level, which forces duckdb into the runtime graph naturally; a dev-only placement would fail those ACs. The choice-0001 boundary (read-for-discovery only, no load/pipeline execution) is the thing for the PR reviewer to watch: confirm relate issues only read queries and never executes survey's emitted load. No AC needed; flagging so the dependency move and the read-only boundary are checked at PR time.

### [LOW] ac-05 writes foreignKeys into a descriptor type that is currently write-only and has no FK field
**Category:** assumption
**Pass:** 2
**Description:** ac-05 requires foreignKeys written INSIDE each resource's `TableSchema`, then the descriptor re-validated against the vendored Frictionless profile (ac-05) and the DDL applied against the fixture (ac-06). The current `TableSchema` struct (`datapackage.rs`) has only `fields` — no `foreignKeys` — and the descriptor types derive `Serialize` only, not `Deserialize`. The module header explicitly states "`foreignKeys`... is out of scope here — it belongs to" (the relate work). So this spec must both *add* the field and, for ac-06's "applies cleanly against the fixture DuckDB," likely *read back* an existing descriptor to compile DDL. The round-trip read path does not exist yet. This is expected extension work (the spec says "extends the datapackage module"), not a conflict — flagging the shape so it is not underestimated.
**Evidence:** `crates/dovetail-core/src/datapackage.rs` (`TableSchema { fields }` only; `#[derive(Debug, Clone, Serialize)]` — no Deserialize; header comment "foreignKeys... is out of scope here"); ac-05 (write FKs + re-validate against vendored profile); ac-06 (compile accepted edges to DDL that applies against the fixture).
**Recommendation:** Confirm during implementation whether the `foreignKeys` custom properties (evidence/confidence/status) round-trip through the vendored profile — the profile permits custom keys, but ac-05's "still validates" must be exercised with the FK entries present, not just the bare schema. Add the FK field to `TableSchema` and decide whether DDL compilation reads the written descriptor or works from in-memory edges (the latter avoids a Deserialize path). Implementation detail; ac-05/ac-06 as written already force the validation and apply-cleanly checks, which is what matters.

---

## Honest Assessment

This plan is ready to implement. It is well-formed and leads with the make-or-break: ac-01 builds a fixture whose four cases (holding FK, orphan near-miss, coincidental boolean overlap, ambiguous mid-confidence) are precisely the discriminations the capability lives or dies on, and ac-03 — the single gate AC — encodes the primary kill condition from the tabletop (parent-uniqueness, not just orphan-count) as a hard, machine-checkable rail. The verification-on-full-columns discipline, the prune-before-overlap performance guard, and the accepted-only DDL compile (with the DDL applying cleanly against the verified fixture) all give the build runnable verification rather than stand-ins. The ac-03 gate description passes the deterministic checks — non-empty, no placeholder token, well over the length floor, names a concrete inspectable property.

The one thing a PR reviewer must not miss is structural rather than technical: the spec's headline reframe (verification-driven auto-accept) currently *contradicts* the accepted text of choice 0007 and the relate card's scenarios, and depends on choice 0009 moving from proposed to accepted (Finding 1). The tabletop saw this and pre-committed to the choice updates — they simply have not landed in the substrate yet. That is a sync action to perform alongside implementation, not a design flaw, and the safety intent the original choice protected (no *unverified* guess becomes a hard constraint) is fully preserved by the ac-03 gate — hence APPROVE rather than REQUEST_CHANGES. The remaining findings (the unpinned high-confidence threshold, the duckdb dev→real move, and the foreignKeys field that must be added to a currently write-only descriptor type) are implementation-time decisions the build should record in the status-assignment logic, the fixture manifest, and the datapackage module; none blocks starting.
