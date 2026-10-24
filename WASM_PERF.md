# WASM program performance

wasmi versus the AVM. The trade-off has two regimes, and these two tables are the
point:

- **Compute-bound**: wasmi runs guest instructions much faster than the AVM
  interprets opcodes, so past a small loop count WASM wins outright.
- **Host-call-bound**: each state/transaction host call crosses Rust→C→Go, so
  WASM pays crossing overhead; the global-state loop below favors the AVM.

These are microbenchmarks, not a projection of mainnet block validation; see
Caveats.

## Budget denomination

WASM now receives **15 fuel per AVM cost unit**, in both signature and
application mode. A 20,000-cost LogicSig budget therefore permits 300,000 fuel.
Guest instruction charges are unchanged. Host charges are calibrated toward
**1 ns/fuel**, with 28,500 fuel for `ed25519verify_bare` (1900 AVM cost units).
Consumed fuel is converted back to AVM cost with rounding up per evaluation,
so shared budgets remain in AVM units, including mixed TEAL/WASM groups.

The measurements below use this conversion. The 15:1 ratio is a provisional
calibration, not a claim that each fuel unit represents identical wall-clock
work. The cost columns use AVM units; fuel columns use wasmi's denomination.

The host weights were selected from the pre-calibration fixtures as
`ns/call / (1 ns/fuel) - guest/loop fuel`, rounded up to convenient constants.
They are deterministic protocol weights, not runtime timers. Signature
verification retains its AVM price; the other host weights need not equal their
AVM opcode equivalents. `txna_uint` provisionally shares `txn_uint`'s charge.
Payload-size and cross-machine calibration remain future work.

## Compute-bound crossover: where WASM overtakes the AVM

`BenchmarkAppFibCrossover` (`data/transactions/logic/wasm_app_bench_test.go`)
compares a TEAL and a WASM application program that compute `fib(N)` in a loop.
It deliberately lifts `MaxAppProgramCost` so N is not budget-capped; these N are
not all reachable within one group, so it measures engine cost, not valid
transactions.

Refreshed on 2026-10-24 on linux/arm64 (8 cores), Go 1.25.3. Timings are
medians of three runs, with
`go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppFibCrossover$' -benchtime=2s -count=3`.

| N | TEAL time | TEAL cost | WASM time | WASM fuel | WASM cost | faster |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | ~6.6 us | 13 | ~17.7 us | 823 | 55 | TEAL |
| 50 | ~17.4 us | 763 | ~15.5 us | 1,873 | 125 | WASM 1.1x |
| 100 | ~25.2 us | 1,513 | ~17.0 us | 2,923 | 195 | WASM 1.5x |
| 300 | ~56.3 us | 4,513 | ~19.5 us | 7,123 | 475 | WASM 2.9x |
| 1000 | ~159.4 us | 15,013 | ~31.5 us | 21,823 | 1,455 | WASM 5.1x |

The endpoint slopes are **TEAL ~153 ns/iteration vs WASM ~14 ns/iteration**.
WASM has higher fixed evaluation overhead (including module compile and
instantiate, no cache yet), but is already faster by N=50 in this run. Short-run
timing is noisy: WASM N=0 ranges from 16.3-19.5 us, and TEAL N=100 from
22.2-33.5 us. The preceding run crossed between N=50 and N=100; these data do
not justify a precise, stable crossover N.

Fib costs TEAL ~15 cost/iteration vs WASM ~21 fuel/iteration. With the 15:1
conversion, that guest execution costs ~1.4 AVM units/iteration, not 21.

## Host-call cost: the AVM wins

`TestWasmHostCallCost` (`data/transactions/logic/wasm_hostcost_test.go`, gated on
`WASM_OPCOST`) measures every host function through the real Go/cgo path, in app
and signature mode. For each fixture the per-call cost is the difference between
running the loop N times and not at all, divided by N. Fixed setup cancels, but
the result includes loop scaffolding as well as the host invocation. Cost/call
is marginal fuel divided by fifteen, not a separately rounded charge per call.

Medians of three runs (`WASM_OPCOST=1 go test ./data/transactions/logic/
-run '^TestWasmHostCallCost$' -v -count=3`):

| host call | host charge (fuel) | fuel/call | cost/call | ns/call | ns/fuel | AVM equivalent |
| --- | --- | --- | --- | --- | --- | --- |
| `txn_uint` | 125 | 137 | 9.13 | 141 | 1.03 | `txn` (1) |
| `txn_bytes` | 190 | 203 | 13.53 | 201 | 0.99 | `txn` (1) |
| `txna_bytes` | 200 | 214 | 14.27 | 214 | 1.00 | `txna` (1) |
| `app_global_get_uint` | 130 | 143 | 9.53 | 152 | 1.06 | `app_global_get` (1) |
| `app_global_get_bytes` | 170 | 184 | 12.27 | 193 | 1.05 | `app_global_get` (1) |
| `app_global_put_uint` | 410 | 427 | 28.47 | 435 | 1.02 | `app_global_put` (1) |
| `app_global_put_bytes` | 390 | 404 | 26.93 | 399 | 0.99 | `app_global_put` (1) |
| `app_global_del` | 140 | 152 | 10.13 | 154 | 1.01 | `app_global_del` (1) |
| `ed25519verify_bare` | 28,500 | 28,516 | 1901.07 | 29,729 | 1.04 | `ed25519verify_bare` (1900) |
| `arg` (sig) | 30 | 43 | 2.87 | 45 | 1.05 | `arg` (1) |
| `arg_count` (sig) | 10 | 20 | 1.33 | 20 | 0.98 | `args` (1) |

The calibrated fixtures now cluster around **0.98-1.06 ns/fuel**, similar to
guest instruction loops. The explicit host charge excludes guest/loop fuel,
which is still charged by wasmi. Signature verification costs 28,500 fuel plus
16 fuel of fixture overhead, preserving its explicit 1900-unit AVM cost.
State/transaction calls deliberately cost more AVM budget units than their TEAL
equivalents to account for the WASM boundary. These are fixture calibrations,
not guarantees for every payload size or platform.

A whole-app host-call-bound comparison (`BenchmarkAppGlobalLoop`,
`app_global_loop.wasm`) at **N=22 global increments** gives these medians of
three 2-second benchmark runs:

| engine | time | AVM cost | WASM fuel |
| --- | --- | --- | --- |
| TEAL | ~18.0 us | 365 | — |
| WASM | ~28.5 us | 683 | 10,238 |

At equal work the AVM is **1.6x faster**. Under the same 700-unit app budget
(10,500 WASM fuel), the harness finds maxima of 42 increments for TEAL and 22
for WASM. The calibrated host charges now account for the extra state-call work
instead of preserving AVM state-op prices. Fixed evaluation overhead is still
unmetered, so the 15:1 conversion is not an exact equal-time guarantee.

## Instruction opcodes

`opcode_cost_table` (`wasm/src/lib.rs`, an `#[ignore]`d test) runs each allowed
instruction in a tight loop; `make wasm-opcost` prints it. Release build on
linux/arm64, 100k iterations, one run; module compile and instantiate are
excluded from both columns. The instruction-loop measurements span
~0.56-1.33 ns/fuel, with `call_indirect`, `table.fill`, and `table.copy` at the
upper end. Both time and fuel include loop scaffolding, so these are not isolated
opcode costs. Guest fuel weights are unchanged by the 15:1 budget conversion;
this instruction table is retained from the preceding run.

| op | fuel/iter | ns/iter | ns/fuel |
| --- | --- | --- | --- |
| nop | 9.0 | 6.8 | 0.75 |
| i32.const | 10.0 | 7.6 | 0.76 |
| i64.const | 10.0 | 8.4 | 0.84 |
| local.get | 10.0 | 9.2 | 0.92 |
| local.set | 11.0 | 8.4 | 0.77 |
| local.tee | 11.0 | 8.6 | 0.78 |
| global.get | 10.0 | 10.3 | 1.03 |
| global.set | 11.0 | 9.9 | 0.90 |
| i32.add | 12.0 | 6.7 | 0.56 |
| i32.mul | 12.0 | 10.0 | 0.84 |
| i32.div_s | 12.0 | 10.3 | 0.85 |
| i32.rem_s | 12.0 | 8.3 | 0.69 |
| i32.rotl | 12.0 | 9.4 | 0.78 |
| i32.shl | 12.0 | 11.9 | 0.99 |
| i32.eq | 12.0 | 8.4 | 0.70 |
| i32.lt_s | 12.0 | 7.5 | 0.63 |
| i32.clz | 11.0 | 10.5 | 0.95 |
| i32.ctz | 11.0 | 9.8 | 0.89 |
| i32.popcnt | 11.0 | 10.7 | 0.98 |
| i32.eqz | 11.0 | 10.1 | 0.92 |
| i64.add | 12.0 | 10.3 | 0.86 |
| i64.mul | 12.0 | 8.2 | 0.68 |
| i64.div_s | 12.0 | 10.3 | 0.86 |
| i64.rotl | 12.0 | 9.7 | 0.81 |
| i64.clz | 11.0 | 10.3 | 0.93 |
| i64.popcnt | 11.0 | 9.7 | 0.88 |
| i32.load | 11.0 | 8.5 | 0.77 |
| i32.load8_u | 11.0 | 10.2 | 0.93 |
| i64.load | 11.0 | 9.5 | 0.86 |
| i32.store | 12.0 | 9.1 | 0.76 |
| i32.store8 | 12.0 | 10.0 | 0.84 |
| i64.store | 12.0 | 11.4 | 0.95 |
| memory.size | 10.0 | 6.8 | 0.68 |
| memory.grow0 | 11.0 | 10.2 | 0.93 |
| memory.copy (4096B) | 77.0 | 53.6 | 0.70 |
| memory.fill (4096B) | 77.0 | 53.7 | 0.70 |
| select | 13.0 | 10.5 | 0.81 |
| call | 12.0 | 12.0 | 1.00 |
| call_indirect | 13.0 | 17.0 | 1.31 |
| br_table | 13.0 | 12.7 | 0.98 |
| ref.null | 10.0 | 8.0 | 0.80 |
| ref.is_null | 11.0 | 8.0 | 0.73 |
| table.get | 11.0 | 8.9 | 0.81 |
| table.set | 12.0 | 10.3 | 0.86 |
| table.size | 10.0 | 7.0 | 0.70 |
| table.grow0 | 12.0 | 7.0 | 0.58 |
| table.fill | 13.0 | 17.3 | 1.33 |
| table.copy | 13.0 | 15.2 | 1.17 |

## How to run

```
make wasmlib                                        # required by all of the below
make wasm-opcost                                    # instruction ns/fuel table
WASM_OPCOST=1 go test ./data/transactions/logic/ -run '^TestWasmHostCallCost$' -v -count=3
go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppFibCrossover$' -benchtime=2s -count=3
go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppGlobalLoop$' -benchtime=2s -count=3
go test ./ledger/ -run '^$' -bench '^BenchmarkBlockEvaluatorLsigs' -benchtime=2s
```

## Caveats

- Microbenchmarks on a single machine (linux/arm64, 8 cores), warm in-memory
  ledger caches, core-count dependent.
- No module cache or engine reuse yet (see `TODO.md`), so the WASM wall-clock
  numbers are the current, unoptimized implementation.
- The application benchmarks drive `EvalContract` on the in-memory test ledger
- Bulk-memory copies are measured at 4096 bytes only, and module size /
  instantiation is not metered at all (a large module with a trivial hot path
  pays ~0 fuel but real compile cost).
