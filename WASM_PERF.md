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
Total fuel now includes a deterministic setup charge reserved before compilation
or allocation, in addition to wasmi's execution and lazy-translation fuel.

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
| 0 | ~6.7 us | 13 | ~14.2 us | 9,205 | 614 | TEAL |
| 50 | ~14.2 us | 763 | ~14.9 us | 10,255 | 684 | ~tie (TEAL slightly faster) |
| 100 | ~21.8 us | 1,513 | ~15.8 us | 11,305 | 754 | WASM 1.4x |
| 300 | ~50.3 us | 4,513 | ~19.2 us | 15,505 | 1,034 | WASM 2.6x |
| 1000 | ~152.6 us | 15,013 | ~31.2 us | 30,205 | 2,014 | WASM 4.9x |

The endpoint slopes are **TEAL ~146 ns/iteration vs WASM ~17 ns/iteration**.
WASM has higher fixed evaluation overhead (including module compile and
instantiate, no cache yet); these samples cross between N=50 and N=100.
Short-run timing varies, so this is not a precise, stable crossover N.

Each WASM row now includes **8,382 setup fuel** (559 AVM units before execution).
At N=50, the remaining 1,873 fuel is execution/lazy translation, so the total
cost is `ceil((8382 + 1873) / 15) = 684`, rather than the previous 125 that
priced execution alone. Setup is charged once per evaluation, not per iteration.

Fib costs TEAL ~15 cost/iteration vs WASM ~21 fuel/iteration. With the 15:1
conversion, that guest execution costs ~1.4 AVM units/iteration, not 21.

## Host-call cost: the AVM wins

`TestWasmHostCallCost` (`data/transactions/logic/wasm_hostcost_test.go`, gated on
`WASM_OPCOST`) measures every host function through the real Go/cgo path, in app
and signature mode. For each fixture the per-call cost is the difference between
running the loop N times and not at all, divided by N. Fixed setup cancels, but
the result includes loop scaffolding as well as the host invocation. The fixed
setup charge cancels in this subtraction; it is still paid by each evaluation.
Cost/call
is marginal fuel divided by fifteen, not a separately rounded charge per call.

Medians of three runs (`WASM_OPCOST=1 go test ./data/transactions/logic/
-run '^TestWasmHostCallCost$' -v -count=3`):

| host call | host charge (fuel) | fuel/call | cost/call | ns/call | ns/fuel | AVM equivalent |
| --- | --- | --- | --- | --- | --- | --- |
| `txn_uint` | 125 | 137 | 9.13 | 136 | 0.99 | `txn` (1) |
| `txn_bytes` | 190 | 203 | 13.53 | 204 | 1.01 | `txn` (1) |
| `txna_bytes` | 200 | 214 | 14.27 | 203 | 0.95 | `txna` (1) |
| `app_global_get_uint` | 130 | 143 | 9.53 | 148 | 1.03 | `app_global_get` (1) |
| `app_global_get_bytes` | 170 | 184 | 12.27 | 191 | 1.04 | `app_global_get` (1) |
| `app_global_put_uint` | 410 | 427 | 28.47 | 414 | 0.97 | `app_global_put` (1) |
| `app_global_put_bytes` | 390 | 404 | 26.93 | 379 | 0.94 | `app_global_put` (1) |
| `app_global_del` | 140 | 152 | 10.13 | 149 | 0.98 | `app_global_del` (1) |
| `ed25519verify_bare` | 28,500 | 28,516 | 1901.07 | 29,364 | 1.03 | `ed25519verify_bare` (1900) |
| `arg` (sig) | 30 | 43 | 2.87 | 43 | 0.99 | `arg` (1) |
| `arg_count` (sig) | 10 | 20 | 1.33 | 18 | 0.91 | `args` (1) |

The calibrated fixtures now cluster around **0.91-1.04 ns/fuel**, similar to
guest instruction loops. The explicit host charge excludes guest/loop fuel,
which is still charged by wasmi. Signature verification costs 28,500 fuel plus
16 fuel of fixture overhead, preserving its explicit 1900-unit AVM cost.
State/transaction calls deliberately cost more AVM budget units than their TEAL
equivalents to account for the WASM boundary. These are fixture calibrations,
not guarantees for every payload size or platform.

A whole-app host-call-bound comparison (`BenchmarkAppGlobalLoop`,
`app_global_loop.wasm`) at **N=24 global increments** gives these medians of
three 2-second benchmark runs. The benchmark now explicitly lifts the cap to
**1,400 cost units** (a two-app-sized allowance); setup plus the zero-iteration
WASM fixture no longer fits a single 700-unit app allowance:

| engine | time | AVM cost | WASM fuel |
| --- | --- | --- | --- |
| TEAL | ~17.6 us | 397 | — |
| WASM | ~26.8 us | 1,398 | 20,966 |

At equal work the AVM is **1.5x faster**. Under the configured 1,400-unit budget
(21,000 WASM fuel), maxima are 86 increments for TEAL and 24 for WASM. Actual
protocol budgets are unchanged: this is an engine microbenchmark, not a claim
that a single default-budget application can run these loop counts.

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

## Setup fuel pricing

`wasm/src/setup.rs` reserves this deterministic charge before creating an engine,
compiling the module, building the linker, or allocating guest memory/tables:

```text
setup fuel = 5000
           + 6   × module bytes (including custom sections and data)
           + 180 × functions (defined + imported)
           + 300 × function types
           + 4   × signature values (params + results, per function/import)
           + 150 × globals
           + 400 × exports
           + 100 × data/element segments (active, passive, or declared)
           + 350 × initial 64 KiB memory pages
           + 4   × initial table entries
           + 10  × element-segment items (including overlapping segments)
```

These are conservative, rounded **consensus constants**, not runtime timers or
cache-dependent discounts. The base covers fixed work and teardown; byte pricing
covers scanning and eager validation. Entity counts account for work not captured
by encoded length: many tiny functions, globals, exports, repeated large
signatures, and zero-length segments. Charging all module bytes deliberately
overprices data-only modules rather than leaving opaque/custom sections free.

The preflight is a bounded binary parser, not an instruction validator/compiler.
It skips code instructions but reads local declarations and bounded type arities.
It rejects oversized modules (16,384 bytes, covering current app/LogicSig caps),
oversized initial memory/tables, unsupported non-function imports, starts,
duplicate sections, malformed counts, and more than 50,000 locals per function.
Existing runtime resource limits remain in force. Arithmetic is checked, and an
unaffordable charge fails before compilation/allocation. Static validation uses
the same preflight bounds but does not deduct an evaluation budget.

Setup consumes the same allowance as execution. Successful evaluations report
both in total fuel and round up once when converting to AVM cost. Compiler,
instantiation, and execution failures retain their setup charge plus consumed
execution; preflight failure consumes the supplied allowance. This matters for
clear-state errors that can be ignored by the caller. No cache or engine reuse
changes the charge.

wasmi currently validates eagerly and translates lazily on first use. Its existing
lazy-translation fuel is retained: setup pricing covers the separate preflight,
eager validation, linking, initialization, and teardown, not another translation
of the same function. Go/cgo and ledger overhead are not comprehensively priced
by these Rust setup weights.

## Module setup and instantiation (no guest execution)

`make wasm-setupcost` runs the ignored Rust `module_setup_cost_table` benchmark.
Each of 5,000 iterations runs the pricing preflight, creates a fresh engine,
compiles a module, builds the host linker, and instantiates a fresh store with
the setup charge deducted. It never calls `program`. This
separates **instantiation** (store creation, allocation, active data/element
initialization, import resolution, and export lookup) from **compilation**.
Teardown is timed separately; setup total excludes teardown. Fixture parsing,
static pre-validation, Go/cgo argument handling, and ledger setup are excluded.

Measured on the same linux/arm64 machine, release build; medians of three
5,000-iteration means. Selected rows from **50 size-sweep cases** follow. Times
are **microseconds per module**. Setup total also includes engine/linker creation
(printed separately by the harness); `ns/fuel` includes setup **and teardown**.

| module | bytes | preflight | compile | instantiate | setup total | teardown | setup fuel | ns/fuel |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| minimal `program` | 40 | 0.21 | 1.42 | 0.34 | 2.65 | 0.55 | 6,124 | 0.52 |
| 1 memory page | 45 | 0.23 | 1.53 | 0.73 | 3.16 | 0.60 | 6,504 | 0.58 |
| 16 memory pages | 45 | 0.21 | 1.47 | 4.45 | 6.78 | 0.55 | 11,754 | 0.62 |
| 64 memory pages (4 MiB) | 45 | 0.21 | 1.43 | 16.21 | 18.49 | 0.55 | 28,554 | 0.67 |
| 15,000 active data bytes | 15,055 | 0.26 | 1.99 | 0.90 | 3.85 | 0.60 | 96,664 | 0.05 |
| 100 unused functions | 641 | 1.89 | 14.89 | 1.23 | 18.70 | 0.82 | 28,130 | 0.69 |
| 1,000 unused functions | 6,045 | 16.66 | 135.76 | 8.96 | 162.35 | 3.37 | 226,154 | 0.73 |
| 2,000 unused functions | 12,045 | 33.24 | 269.96 | 17.60 | 321.59 | 4.90 | 446,154 | 0.73 |
| 12,000 nops | 12,042 | 0.21 | 25.54 | 0.35 | 26.81 | 0.56 | 78,136 | 0.35 |
| 4,000 nested blocks | 12,042 | 0.21 | 60.61 | 0.38 | 61.95 | 0.66 | 78,136 | 0.80 |
| 1,000 globals | 5,045 | 0.23 | 88.34 | 7.05 | 96.28 | 0.98 | 186,154 | 0.52 |
| 1,000 extra types | 3,043 | 26.18 | 102.94 | 0.38 | 130.05 | 0.59 | 324,142 | 0.40 |
| 1,000 empty data segments | 5,050 | 0.26 | 85.56 | 10.19 | 96.53 | 1.38 | 136,534 | 0.72 |
| 100 functions sharing 256 params | 715 | 2.73 | 68.51 | 1.23 | 73.24 | 0.90 | 130,874 | 0.57 |
| 1,000 extra exports | 6,952 | 0.32 | 170.54 | 138.94 | 312.17 | 18.62 | 448,076 | 0.74 |
| 1,024 null table entries | 47 | 0.22 | 1.50 | 0.45 | 2.85 | 0.58 | 10,262 | 0.33 |
| 1,024 active element items | 1,101 | 0.66 | 8.75 | 2.96 | 13.03 | 0.88 | 27,406 | 0.51 |
| 32,768 compressed locals | 44 | 0.21 | 1.90 | 0.34 | 3.12 | 0.57 | 6,148 | 0.60 |
| `app_fib.wasm` | 208 | 0.36 | 2.49 | 0.83 | 4.41 | 0.62 | 8,382 | 0.60 |
| `ed25519_dalek.wasm` | 4,761 | 1.40 | 19.89 | 5.57 | 27.59 | 0.91 | 48,160 | 0.59 |

Across all 150 case/run combinations, the highest measured setup-plus-teardown
ratio was **0.98 ns/fuel**. This is empirical headroom on this machine, not a
proof of a universal bound. Some synthetic cases intentionally exceed a normal
LogicSig/app allowance; the harness supplies a large budget to measure them.
The data-heavy row shows the deliberate conservatism of uniform byte pricing.

The synthetic cases isolate allocation, initialization, function/global/type
counts, compressed locals, repeated signatures, nested control, arithmetic,
segments, and exports. Fuel now accounts for this work through the formula above.
In particular,
a small encoded module can allocate substantial memory, and a module with many
unused functions can incur substantial compilation time. These timings do not
explain all of the Go application's fixed overhead: they measure only the Rust
backend, and exclude the Go/cgo/ledger work included in the earlier app tables.
Fresh engines prevent module-cache reuse, but allocator and OS caches are warm.

WebAssembly **start sections are forbidden**, even for a no-op start function.
The shared engine configuration rejects them during both static validation and
execution, before instantiation. An exported `_start` function without a start
section is not invoked automatically; only `program` is called.

## How to run

```
make wasmlib                                        # required by all of the below
make wasm-opcost                                    # instruction ns/fuel table
make wasm-setupcost                                 # separate compile/instantiate costs, no program call
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
- Bulk-memory copies are measured at 4096 bytes only. Setup weights are calibrated
  conservatively on this one machine, not a proven upper bound for all valid
  modules or validator platforms. Go/cgo/ledger overhead and static checking are
  not comprehensively priced by the Rust setup formula.
