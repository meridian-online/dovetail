# Tabletop — jaq passthrough shim

**Date:** 2026-06-21
**Cards in scope:** 0003-transform-shim
**Output spec:** .orbit/specs/2026-06-21-jaq-passthrough-shim/spec.yaml (to be crystallised by /orb:spec)

---

## Capability ambition

Give dovetail a pinned, byte-stable `dovetail jaq <program> <file>` — true
passthrough to an embedded, vendored jaq — so what discovery explores with is
exactly what emitted scripts run, and a reviewer reads the actual jq program.
jaq is the parity keystone: it covers JSON/YAML/TOML/XML conversion and the
NDJSON hoist survey's deferred `.yaml` escalation rung will call. calamine and
calcard are a deliberate later spec.

## Goal (Q1 / carving)

`dovetail jaq <program> <file>` runs a jq program through embedded jaq and emits
NDJSON by default, byte-for-byte reproducibly, with the resolved jaq version
stamped. Scope is jaq only; calamine (Excel→Parquet) and calcard (ics/vcf→JSON)
are confirmed in principle (choice 0011) but deferred to their own spec. Carving
defence: **parity is the hard part** — proving the embedded-vs-reference claim
deserves its own focused spec before the format adapters pile on.

## Values (Q2)

- **Load-bearing: discovery-execution parity (choice 0006).** The embedded jaq
  used in discovery IS the embedded jaq emitted scripts call — same vendored
  binary both times, byte-identical to itself by construction. Everything bends
  to preserve this.
- **Output value: legibility of the program (brief principle 4).** Trust lives
  in the transform spec, not the executor — jaq is *true passthrough*, never
  rewrites the program, so the `.jq` a reviewer reads is exactly what ran.

## Trade-offs (Q3)

- **Full jq-language coverage → curated ingestion idioms.** The first spec
  proves passthrough + parity on the common idioms (`.results[]`, `select`,
  `map`, hoist), not the entire jq surface. *Acceptable.*
- **Reference parity with canonical jq → embedded determinism.** Where embedded
  jaq and system jq diverge (a reimplementation, not a fork), we scope the
  parity corpus to the equivalent subset rather than chase 100% jaq==jq.
  *Expensive-but-worth-it* — see kill conditions.
- **All three tools now → jaq alone first.** *Acceptable* (carving).

### CLI surface (choice 0011, confirmed)

- This spec ships: `dovetail jaq <program> <file>` → **NDJSON** default.
- Confirmed in principle, deferred to their spec: `dovetail calamine <file>
  [--sheet N]` → Parquet; `dovetail calcard <file>` → JSON.
- Choice 0011 is thereby **half-resolved**: jaq surface settled; calamine/calcard
  surfaces agreed but not yet implemented.

### Verification posture

All scenarios `verifies: capability` — real embedded jaq runs over real inputs,
and a real byte-diff against canonical system jq on the curated equivalent
subset. No stand-ins. The parity corpus is curated, not exhaustive; that scoping
is a documented trade-off (above), not a stand-in.

## Failure modes (Q4)

- **Embedded jaq diverges from canonical jq on a common ingestion idiom** →
  *halt-worthy*: the parity claim weakens. Trigger: a curated-subset program's
  embedded output differs from system jq. Pivot in Q10.
- **Passthrough silently rewrites/normalises the program** → *halt-worthy*: it
  would break legibility. The program a reviewer reads must be byte-equal to what
  was passed.
- **Output not reproducible run-to-run** (nondeterministic ordering) →
  *halt-worthy*: breaks discovery-execution parity.
- jaq crate doesn't expose a usable embedding API. *Hygiene/detour* — resolve at
  implement.
- jq absent on the test machine. *Hygiene* — the byte-diff test skips-with-notice
  rather than failing when jq is unavailable.

## Lateral approaches (Q5) — held in reserve

- **Shell to system jq always (no embedding).** Rejected: breaks the
  no-system-dependency + determinism line (choice 0005); jq becomes mandatory.
- **serde_json / hand-rolled transforms instead of a jq engine.** Rejected: loses
  the jq program as the auditable artifact (the output value).
- **Test the full jq language surface.** Not rejected — held as the expansion
  path once the ingestion-idiom subset is green.

## Success criteria (Q6) — binary, measurable

1. `dovetail jaq <program> <file>` produces NDJSON for the curated program corpus
   over fixture inputs, exit 0.
2. The program passed is byte-equal to the program the embedded engine receives
   (passthrough, no rewrite) — asserted.
3. For every program in the curated equivalent subset, embedded jaq's output is
   byte-identical to canonical system jq's output.
4. Output is reproducible: two runs of the same (program, input) are byte-equal.
5. The resolved jaq version is stamped and retrievable (feeds choice 0013).
6. Documented-divergence list exists for any jq construct excluded from the
   parity subset, each with a reason.

## Escalation triggers (Q7)

- **A common ingestion idiom (`.results[]`, `select`, `map`, hoist) diverges
  between embedded jaq and system jq** → halt; surface the program, both outputs,
  and the divergence; propose narrowing or documenting.
- **The jaq crate can't do true passthrough** (forces an AST round-trip that
  normalises the program) → halt; surface the constraint against the legibility
  value.

## Adjacent code (Q8)

- New `dovetail-core` transform module wrapping embedded jaq (crate selection —
  `jaq-core` / `jaq-interpret` / `jaq-std` — is an implement-time detour; verify
  the embedding API exposes raw-program passthrough).
- New `dovetail jaq` CLI subcommand on the existing thin bin (alongside `survey`).
- The `--engine jq` reference path (choice 0005) — the verification hook, and the
  shell-out used by the parity byte-diff test.
- **Future seam:** survey's deferred `.yaml` escalation rung will emit steps that
  call `dovetail jaq ...`. This spec builds the surface that rung depends on; the
  rung itself is a later survey spec. No code coupling yet — flag the contract.
- Version stamping connects to choice 0013 (provenance header) — keep the stamp
  shape compatible with whatever 0013 settles.

## Budget (Q9) — Claude-execution pace

- Embedded jaq wiring + passthrough + NDJSON emit: ~0.5 day.
- Parity corpus + byte-diff harness + divergence documentation: ~0.5–1 day.
- **Total ≈ 1–1.5 working days.** Lighter than survey — no ML, no DuckDB.

## Kill conditions (Q10)

- **Primary — "embedded jaq is reference-equivalent to jq on ingestion idioms."**
  Kill signal: common idioms in the curated subset diverge from system jq even
  after scoping. **Pivot:** narrow the parity claim to the proven-equivalent set
  and document the boundary explicitly — parity becomes "equivalent on this
  stated subset" rather than "equivalent to jq". (The corpus is curated to the
  equivalent subset from the start; this fires if even that subset can't hold.)
- **Secondary — "the jaq crate supports true passthrough."** Kill signal: the
  embedding API forces a program normalisation that breaks byte-equal passthrough.
  **Pivot:** preserve the raw program string alongside execution and emit it
  verbatim, accepting the engine parses a normalised copy internally.
- **Tertiary — "embedded determinism holds."** Kill signal: output ordering is
  nondeterministic. **Pivot:** pin a deterministic output mode or sort key.

## Feeds back to choices

- **Choice 0011 (shim CLI surface)** → jaq surface settled (`dovetail jaq
  <program> <file>` → NDJSON); calamine/calcard surfaces agreed in principle.
  Move toward `accepted` for the jaq half once /orb:spec lands; keep open for the
  calamine/calcard detail.
- **Choice 0005 (embedded jaq default, jq optional)** → this spec is its first
  concrete realisation; the `--engine jq` path becomes both a feature and the
  parity-test reference.
- **Choice 0006 (parity)** → the load-bearing value; vendoring makes
  discovery-execution parity structural.
- **Choice 0013 (provenance header)** → the jaq version stamp is the first
  provenance field; keep its shape compatible with 0013's eventual decision.

## Hot-wash

- **recurred:** the two-parity-claims distinction (self-parity vs reference
  parity) — easy to conflate, and conflating them sets up a test that chases a
  known-false "jaq == jq".
- **surprised:** jaq being a reimplementation (not a jq fork) makes "byte-identical
  to jq" a scoped claim, not a global one — this reshaped Q4 into Q10.
- **friction:** the shim's value is partly latent — its real consumer (survey's
  `.yaml` rung) doesn't exist yet, so the spec must justify itself on parity +
  the format-conversion utility, not on a live downstream caller.
- **meta-patterns-for-future-tabletops:** when a capability rests on a "parity"
  or "equivalence" claim, split it into self-equivalence (usually structural) vs
  reference-equivalence (often scoped) *before* writing verification ACs — the
  test design depends entirely on which one you mean.
