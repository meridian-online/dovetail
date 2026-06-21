# relate — follow-ups

From review-pr (spec 2026-06-21-relate-discover-verify-render, APPROVE cycle 2).
Candidates for a follow-up relate spec against card 0002.

1. **Make the DuckDB read-only boundary structural.** relate opens the connection
   read-write; the read-only discipline (choice 0001/0014) is currently enforced
   by convention + verified empirically (file SHA unchanged after a run). Opening
   with a `READ_ONLY` config would make it engine-enforced — small, high-value
   hardening directly reinforcing the choice the package leans on.

2. **Explicit parent minimum-cardinality floor.** Auto-accept currently leans on
   the confidence threshold to reject low-cardinality parents; there is no
   explicit cardinality floor. An accidentally-unique low-cardinality parent (e.g.
   a 2-row lookup whose ids happen to cover the child) leans on the threshold, not
   on a cardinality rule. The fixture doesn't exercise this — add a fixture case
   and an explicit floor.

3. **Self-referential FKs and composite/multi-column keys** are not discovered
   (noted on card 0002). Single-column, cross-table only for now.

4. **finetype semantic-type agreement** as a fourth evidence signal (deferred from
   this spec to keep finetype-model out of relate's first cut).

5. **The other renders** (choice 0010): Mermaid erDiagram, join/enrichment SQL,
   brightfield feed — this spec shipped constraint DDL only.
