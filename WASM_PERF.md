# WASM program performance

wasmi versus the AVM. The trade-off has two regimes, and these two tables are the
point:

- **Compute-bound**: wasmi runs guest instructions much faster than the AVM
  interprets opcodes, so past a small loop count WASM wins outright.
- **Host-call-bound**: each state/transaction host call crosses Rust→C→Go, so
  WASM pays more per unit than the AVM and never catches up.

These are microbenchmarks, not a projection of mainnet block validation; see
Caveats.

## Compute-bound crossover: where WASM overtakes the AVM

`BenchmarkAppFibCrossover` (`data/transactions/logic/wasm_app_bench_test.go`)
compares a TEAL and a WASM application program that compute `fib(N)` in a loop.
It deliberately lifts `MaxAppProgramCost` so N is not budget-capped; these N are
not all reachable within one group, so it measures engine cost, not valid
transactions.

Measured on linux/arm64 (8 cores), Go 1.25.3, with
`go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppFibCrossover$' -benchtime=2s -count=3`.

| N | TEAL time | TEAL cost | WASM time | WASM fuel | faster |
| --- | --- | --- | --- | --- | --- |
| 0 | ~6.5 us | 13 | ~13.7 us | 624 | TEAL |
| 50 | ~13.8 us | 763 | ~14.1 us | 1,674 | ~tie (**crossover**) |
| 100 | ~21.0 us | 1,513 | ~15.1 us | 2,724 | WASM 1.4x |
| 300 | ~51.0 us | 4,513 | ~18.7 us | 6,924 | WASM 2.7x |
| 1000 | ~155 us | 15,013 | ~30.9 us | 21,624 | WASM 5.0x |

The slopes are the point: **TEAL ~150 ns/iteration vs WASM ~15 ns/iteration**.
WASM pays a fixed ~7 us per evaluation (module compile + instantiate, no cache
yet), so `150N + 6500 = 15N + 13700` gives the **crossover at N ≈ 56**: below it
the AVM is faster, above it WASM is faster and the margin grows.

The metered axis runs the other way: fib costs TEAL ~15 cost/iteration vs WASM
~21 fuel/iteration, so WASM burns more budget while running faster. wasmi's fuel
units are not proportional to wall-clock cost.

## Host-call cost: the AVM wins

`TestWasmHostCallCost` (`data/transactions/logic/wasm_hostcost_test.go`, gated on
`WASM_OPCOST`) measures every host function through the real Go/cgo path, in app
and signature mode. For each fixture the per-call cost is the difference between
running the loop N times and not at all, so setup and loop scaffolding cancel:

| host call | fuel/call | ns/call | ns/fuel | AVM equivalent |
| --- | --- | --- | --- | --- |
| `txn_uint` | 13 | 131 | 10.1 | `txn` (1) |
| `txn_bytes` | 14 | 202 | 14.4 | `txn` (1) |
| `txna_bytes` | 15 | 203 | 13.5 | `txna` (1) |
| `app_global_get_uint` | 14 | 141 | 10.1 | `app_global_get` (1) |
| `app_global_get_bytes` | 15 | 182 | 12.1 | `app_global_get` (1) |
| `app_global_put_uint` | 18 | 424 | 23.5 | `app_global_put` (1) |
| `app_global_put_bytes` | 15 | 373 | 24.8 | `app_global_put` (1) |
| `app_global_del` | 13 | 145 | 11.1 | `app_global_del` (1) |
| `ed25519verify_bare` | 1916 | 29,177 | 15.2 | `ed25519verify_bare` (1900) |
| `arg` (sig) | 13 | 34 | 2.6 | `arg` (1) |
| `arg_count` (sig) | 10 | 13 | 1.3 | `args` (1) |

Host calls are ~2-25x more expensive per fuel than guest instructions (\~1 ns/fuel
from the instruction harness below): the cgo crossing plus Go-side work. But they
land in the AVM's own per-op band — the AVM's state ops are also \~25 ns/cost for
a `1`-cost opcode, and `ed25519verify_bare` (both engines charge 1900) is ~15
ns/fuel. So at a 1:1 fuel:cost mapping the worst case is the same order as the
AVM, and `LogicSigMaxCost` = 20,000 fuel is a defensible first-cut budget. For
headroom, raise the `app_global_put_*` fuel cost a few units rather than
rescaling.

A whole-app host-call-bound comparison (`BenchmarkAppGlobalLoop`,
`app_global_loop.wasm`) shows the same result end to end.

## Instruction opcodes

`opcode_cost_table` (`wasm/src/lib.rs`, an `#[ignore]`d test) runs each allowed
instruction in a tight loop; `make wasm-opcost` prints it. Release build on
linux/arm64, 100k iterations; module compile and instantiate are excluded from
both columns. The point is that the table is **flat** — ~0.6-1.3 ns/fuel across
every family, with `call_indirect`, `table.fill`, and `table.copy` the only rows
above the `nop` baseline (1.12), and `memory.copy`/`memory.fill` of 4096 bytes
metered in line with their speed. No single-instruction outlier.

| op | fuel/iter | ns/iter | ns/fuel |
| --- | --- | --- | --- |
| nop | 9.0 | 10.1 | 1.12 |
| i32.const | 10.0 | 7.7 | 0.77 |
| i64.const | 10.0 | 5.9 | 0.59 |
| local.get | 10.0 | 9.3 | 0.93 |
| local.set | 11.0 | 7.4 | 0.67 |
| local.tee | 11.0 | 8.7 | 0.79 |
| global.get | 10.0 | 8.5 | 0.85 |
| global.set | 11.0 | 10.5 | 0.95 |
| i32.add | 12.0 | 9.0 | 0.75 |
| i32.mul | 12.0 | 10.3 | 0.85 |
| i32.div_s | 12.0 | 8.7 | 0.73 |
| i32.rem_s | 12.0 | 9.0 | 0.75 |
| i32.rotl | 12.0 | 8.8 | 0.73 |
| i32.shl | 12.0 | 8.5 | 0.71 |
| i32.eq | 12.0 | 11.3 | 0.94 |
| i32.lt_s | 12.0 | 10.3 | 0.86 |
| i32.clz | 11.0 | 8.4 | 0.76 |
| i32.ctz | 11.0 | 10.1 | 0.91 |
| i32.popcnt | 11.0 | 9.7 | 0.88 |
| i32.eqz | 11.0 | 10.0 | 0.91 |
| i64.add | 12.0 | 9.2 | 0.77 |
| i64.mul | 12.0 | 10.1 | 0.84 |
| i64.div_s | 12.0 | 9.7 | 0.81 |
| i64.rotl | 12.0 | 9.9 | 0.83 |
| i64.clz | 11.0 | 10.2 | 0.93 |
| i64.popcnt | 11.0 | 10.6 | 0.96 |
| i32.load | 11.0 | 9.8 | 0.89 |
| i32.load8_u | 11.0 | 10.4 | 0.94 |
| i64.load | 11.0 | 9.2 | 0.83 |
| i32.store | 12.0 | 10.8 | 0.90 |
| i32.store8 | 12.0 | 10.3 | 0.86 |
| i64.store | 12.0 | 10.1 | 0.84 |
| memory.size | 10.0 | 9.7 | 0.97 |
| memory.grow0 | 11.0 | 8.5 | 0.77 |
| memory.copy (4096B) | 77.0 | 53.4 | 0.69 |
| memory.fill (4096B) | 77.0 | 87.8 | 1.14 |
| select | 13.0 | 10.6 | 0.81 |
| call | 12.0 | 10.2 | 0.85 |
| call_indirect | 13.0 | 16.5 | 1.27 |
| br_table | 13.0 | 12.7 | 0.97 |
| ref.null | 10.0 | 8.3 | 0.83 |
| ref.is_null | 11.0 | 8.7 | 0.79 |
| table.get | 11.0 | 10.3 | 0.94 |
| table.set | 12.0 | 11.7 | 0.97 |
| table.size | 10.0 | 9.0 | 0.90 |
| table.grow0 | 12.0 | 9.8 | 0.81 |
| table.fill | 13.0 | 16.4 | 1.26 |
| table.copy | 13.0 | 16.4 | 1.26 |

## How to run

```
make wasmlib                                        # required by all of the below
make wasm-opcost                                    # instruction ns/fuel table
WASM_OPCOST=1 go test ./data/transactions/logic/ -run TestWasmHostCallCost -v
go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppFibCrossover$' -benchtime=2s
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
