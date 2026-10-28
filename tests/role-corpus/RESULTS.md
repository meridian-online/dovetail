# Role eval results

Each set's role calls scored against its answer key. Written by the `role_eval_harness` test with `DOVETAIL_WRITE_ROLE_RESULTS=1`; everything but the stamp is the same on every run of the same code.

Stamp: finetype 0.6.64, model sha256 86d7fcde7f92cbb2fb47bbe04ad65bfe991f9d698bf4d66c2a48e59fbf91fabe, dovetail 82d1fac

## Summary

| set | tier | typing | tables | role accuracy | fold P / R | combination P / R |
|---|---|---|---|---|---|---|
| retail | 1 | floor | 12 | 12/12 (100.0%) | 1.00 / 1.00 | 1.00 / 1.00 |
| retail | 1 | profiled | 12 | 12/12 (100.0%) | 1.00 / 1.00 | 1.00 / 1.00 |

## Tier 1, floor

Rows are the answer key's role, columns the call.

| key \ call | entity | event | reference | bridge | snapshot | unclear |
|---|---|---|---|---|---|---|
| entity | 4 | 0 | 0 | 0 | 0 | 0 |
| event | 0 | 3 | 0 | 0 | 0 | 0 |
| reference | 0 | 0 | 1 | 0 | 0 | 0 |
| bridge | 0 | 0 | 0 | 0 | 0 | 0 |
| snapshot | 0 | 0 | 0 | 0 | 3 | 0 |
| unclear | 0 | 0 | 0 | 0 | 0 | 1 |

Misses: none.

## Tier 1, profiled

Rows are the answer key's role, columns the call.

| key \ call | entity | event | reference | bridge | snapshot | unclear |
|---|---|---|---|---|---|---|
| entity | 4 | 0 | 0 | 0 | 0 | 0 |
| event | 0 | 3 | 0 | 0 | 0 | 0 |
| reference | 0 | 0 | 1 | 0 | 0 | 0 |
| bridge | 0 | 0 | 0 | 0 | 0 | 0 |
| snapshot | 0 | 0 | 0 | 0 | 3 | 0 |
| unclear | 0 | 0 | 0 | 0 | 0 | 1 |

Misses: none.
