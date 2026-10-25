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

WASM now receives **10 fuel per AVM cost unit**, in both signature and
application mode. A 20,000-cost LogicSig budget therefore permits 200,000 fuel.
Guest instruction charges are unchanged. Host charges are calibrated toward
**1 ns/fuel**, with 28,500 fuel for `ed25519verify_bare` (2850 AVM budget units,
versus the TEAL opcode's 1900). All raw instruction, host, and setup weights
remain unchanged from the 15:1 calibration.
Consumed fuel is converted back to AVM cost with rounding up per evaluation,
so shared budgets remain in AVM units, including mixed TEAL/WASM groups.
Total fuel now includes a deterministic setup charge reserved before compilation
or allocation, in addition to wasmi's execution and lazy-translation fuel.

Moving from 15:1 to 10:1 charges 50% more budget for the same raw fuel (before
rounding), targeting less wall time per allowed budget unit. The conversion is
deterministic and consensus-relevant; benchmark timing never changes it at
runtime. These microbenchmarks do not prove an AVM-equivalent worst-case
validation-time bound. Cost columns use AVM units; fuel uses wasmi's denomination.
Compute/host/global-loop timings were rerun for 10:1 below. Instruction and setup
tables retain their raw-fuel calibration measurements because those weights and
code paths are unchanged by this conversion.

The host weights were selected from the pre-calibration fixtures as
`whole-iteration ns / (1 ns/fuel) - guest/loop fuel`, rounded up to convenient
constants. This estimates the host portion by assuming the guest scaffolding
runs at the target 1 ns/fuel; it does **not** subtract separately measured
loop-only time or independently measure host-call or boundary time.
They are deterministic protocol weights, not runtime timers. Host budget prices
need not equal their AVM opcode equivalents; signature verification also retains
its raw weight rather than AVM price parity. `txna_uint` provisionally shares
`txn_uint`'s charge.
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
| 0 | ~7.4 us | 13 | ~16.8 us | 9,205 | 921 | TEAL |
| 50 | ~15.6 us | 763 | ~16.8 us | 10,255 | 1,026 | TEAL 1.1x |
| 100 | ~22.9 us | 1,513 | ~17.2 us | 11,305 | 1,131 | WASM 1.3x |
| 300 | ~52.8 us | 4,513 | ~20.9 us | 15,505 | 1,551 | WASM 2.5x |
| 1000 | ~158.2 us | 15,013 | ~32.8 us | 30,205 | 3,021 | WASM 4.8x |

The endpoint slopes are **TEAL ~151 ns/iteration vs WASM ~16 ns/iteration**.
WASM has higher fixed evaluation overhead (including module compile and
instantiate, no cache yet); these samples cross between N=50 and N=100.
Short-run timing varies, so this is not a precise, stable crossover N.

Each WASM row includes **8,382 setup fuel** (839 AVM units before execution).
At N=50, the remaining 1,873 fuel is execution/lazy translation, so the total
cost is `ceil((8382 + 1873) / 10) = 1026`, versus 684 at 15:1.
Setup is charged once per evaluation, not per iteration. This fixture no longer
fits a single default 700-unit app budget even at N=0; it needs pooled budget.

Fib costs TEAL ~15 cost/iteration vs WASM ~21 fuel/iteration. With the 10:1
conversion, that guest execution costs ~2.1 AVM units/iteration, not 21.

For the validation-time ceiling question, time **per charged budget unit** is
more useful than same-work speedup. Using the unrounded median timings above:

| N | TEAL ns/cost | WASM ns/cost at 10:1 | WASM/TEAL time per cost |
| --- | --- | --- | --- |
| 100 | 15.1 | 15.2 | 1.01x |
| 300 | 11.7 | 13.5 | 1.15x |
| 1000 | 10.5 | 10.9 | 1.03x |

10:1 is substantially closer than 15:1, but does **not** establish a strict
AVM ceiling: N=300 still uses about 15% more time per cost unit. These ratios
are comparisons of the measured workloads, not worst-case budget-filling
group/block bounds. The denomination change does not make execution faster;
differences from the prior timing table are measurement variation.

## Host-call cost: the AVM wins

`TestWasmHostCallCost` (`data/transactions/logic/wasm_hostcost_test.go`, gated on
`WASM_OPCOST`) measures every host function through the real Go/cgo path, in app
and signature mode. For each fixture, time and fuel per iteration are the
difference between running the loop N times and not at all, divided by N:

```text
ns/iter   = (time(N) - time(0)) / N
fuel/iter = (fuel(N) - fuel(0)) / N
cost/iter = fuel/iter / 10
```

Fixed setup cancels, but repeated loop scaffolding, operand preparation, and
result handling remain alongside the host invocation. The fixed setup charge
is still paid by each evaluation. `cost/iter` is marginal fuel divided by ten,
not a separately rounded charge per call. Only `host charge (fuel)` below is
the explicit host-function weight; the other columns describe whole iterations.

Medians of three runs (`WASM_OPCOST=1 go test ./data/transactions/logic/
-run '^TestWasmHostCallCost$' -v -count=3`):

| host call | host charge (fuel) | fuel/iter | cost/iter | ns/iter | ns/fuel | TEAL opcode (opcode-only cost) |
| --- | --- | --- | --- | --- | --- | --- |
| `txn_uint` | 125 | 137 | 13.70 | 137 | 1.00 | `txn` (1) |
| `txn_bytes` | 190 | 203 | 20.30 | 218 | 1.07 | `txn` (1) |
| `txna_bytes` | 200 | 214 | 21.40 | 215 | 1.01 | `txna` (1) |
| `app_global_get_uint` | 130 | 143 | 14.30 | 148 | 1.03 | `app_global_get` (1) |
| `app_global_get_bytes` | 170 | 184 | 18.40 | 192 | 1.04 | `app_global_get` (1) |
| `app_global_put_uint` | 410 | 427 | 42.70 | 426 | 1.00 | `app_global_put` (1) |
| `app_global_put_bytes` | 390 | 404 | 40.40 | 392 | 0.97 | `app_global_put` (1) |
| `app_global_del` | 140 | 152 | 15.20 | 151 | 0.99 | `app_global_del` (1) |
| `ed25519verify_bare` | 28,500 | 28,516 | 2851.60 | 30,618 | 1.07 | `ed25519verify_bare` (1900) |
| `arg` (sig) | 30 | 43 | 4.30 | 41 | 0.95 | `arg` (1) |
| `arg_count` (sig) | 10 | 20 | 2.00 | 18 | 0.87 | no direct equivalent |

The calibrated fixtures now cluster around **0.87-1.07 ns/fuel**, similar to
guest instruction loops. The explicit host charge excludes guest/loop fuel,
which is still charged by wasmi. Signature verification costs 28,500 fuel plus
16 fuel of fixture overhead: 2850 budget units for the host charge, compared
with the TEAL opcode's 1900.
State/transaction host weights exceed their TEAL opcode prices under this
independent time-based calibration. The weights account for the host path, not
just crossing the WASM boundary, and their ratios to TEAL prices are not measured
slowdown factors. These are fixture calibrations, not guarantees for every
payload size or platform.

For example, `TXN_UINT_FUEL_COST = 125` is the explicit charge per invocation,
not the total iteration cost and not an independently measured 125 ns duration:

```text
txn_uint host charge       = 125 fuel = 12.50 AVM cost
guest scaffolding          =  12 fuel =  1.20 AVM cost
whole iteration            = 137 fuel = 13.70 AVM cost
```

The earlier fixture measured about 137 ns per whole iteration, consistent with
the target 1 ns/fuel. Later runs can differ (see the matched table below) without
changing the fixed 125-fuel protocol weight. Neither 12.50 nor 13.70 establishes
a corresponding slowdown relative to TEAL's 1-cost `txn` opcode.

### Matched TEAL host-work loops

The host-call harness now also runs matched TEAL fixtures: the same transaction
fields, keys, values, argument sizes, and valid signature, using each engine's
own loop. Both timings subtract a zero-iteration evaluation before dividing by
N. Debug tracing is disabled in both engines. Loop counts are encoded
little-endian for WASM's `i32.load` and big-endian for TEAL's `btoi`.

Measured on 2026-10-25, linux/arm64, Go 1.25.3, using the host-call command above
with `-count=3`. Times are medians of three runs; ratios divide the displayed
medians. Costs are marginal **whole-iteration** costs, including loop control,
operand preparation, and result handling, not just the named operation.

For example, the matched TEAL `txn_uint` iteration costs **11**, although
`txn FirstValid` itself costs **1**:

```teal
loop:
  load 0          // remaining iterations
  int 0
  ==
  bnz done
  txn FirstValid  // operation being measured
  store 1         // save result, matching the WASM fixture
  load 0
  int 1
  -
  store 0
  b loop
```

All eleven instructions cost 1: nine implement loop control, one reads the
transaction field, and one stores the result. Thus the comparison is **13.70
versus 11 for whole iterations**, or **12.50 versus 1 for explicit operation
charges**. Neither comparison isolates operation timings. Zero-iteration
subtraction removes fixed setup, not this repeated scaffolding, in both engines.

| host work | WASM cost/iter | TEAL cost/iter | WASM ns/iter | TEAL ns/iter | WASM/TEAL time |
| --- | --- | --- | --- | --- | --- |
| `txn_uint` (`FirstValid`) | 13.70 | 11 | 126.4 | 120.8 | 1.05x |
| `txn_bytes` (`Sender`, 32 bytes) | 20.30 | 11 | 198.3 | 121.3 | 1.63x |
| `txna_bytes` (`ApplicationArgs[0]`, 4 bytes) | 21.40 | 11 | 195.9 | 126.3 | 1.55x |
| `app_global_get_uint` (absent `counter`) | 14.30 | 12 | 146.3 | 129.9 | 1.13x |
| `app_global_get_bytes` (absent `b`) | 18.40 | 12 | 177.6 | 117.9 | 1.51x |
| `app_global_put_uint` (incrementing `counter`) | 42.70 | 16 | 413.6 | 414.6 | 1.00x |
| `app_global_put_bytes` (`b` = `x`) | 40.40 | 12 | 384.0 | 311.8 | 1.23x |
| `app_global_del` (absent `d`) | 15.20 | 11 | 143.2 | 122.8 | 1.17x |
| `ed25519verify_bare` (32-byte message) | 2851.60 | 1913 | 26996.4 | 26644.8 | 1.01x |
| `arg` (sig, 4 bytes) | 4.30 | 11 | 39.2 | 86.5 | 0.45x |

Thus **14.3 budget units versus the TEAL opcode price of 1 does not mean a
14.3x slowdown**. It also compares a WASM iteration with a single TEAL opcode:
the matched TEAL get iteration costs 12, not 1. On this workload the full WASM
get iteration is about 13% slower. Neither this ratio nor subtraction of the
two timings isolates the Rust/C/Go boundary: the engines have different loop,
operand, dispatch, and result-handling costs. No loop-only control is subtracted.
These measurements do not determine an isolated TEAL opcode time or establish
whether its protocol price is appropriate.

Both get fixtures read **absent keys**, and delete is also an absent-key case;
these do not measure successful value retrieval or deletion. The bytes put
repeats the same one-byte value, whereas the uint put changes its value on each
iteration. `arg_count` has no direct TEAL equivalent and is omitted here:
TEAL `args` indexes an argument rather than returning the argument count.

A whole-app host-call-bound comparison (`BenchmarkAppGlobalLoop`,
`app_global_loop.wasm`) at **N=7 global increments** gives these medians of
three 2-second benchmark runs. The benchmark now explicitly lifts the cap to
**1,400 cost units** (a two-app-sized allowance); setup plus the zero-iteration
WASM fixture no longer fits a single 700-unit app allowance:

| engine | time | AVM cost | WASM fuel |
| --- | --- | --- | --- |
| TEAL | ~11.1 us | 125 | — |
| WASM | ~21.6 us | 1,371 | 13,707 |

At equal work the AVM is **1.9x faster**. Under the configured 1,400-unit budget
(14,000 WASM fuel), maxima are 86 increments for TEAL and 7 for WASM (down from
24 at 15:1). Actual
protocol budgets are unchanged: this is an engine microbenchmark, not a claim
that a single default-budget application can run these loop counts.

## Instruction opcodes

`opcode_cost_table` (`wasm/src/lib.rs`, an `#[ignore]`d test) runs each allowed
instruction in a tight loop; `make wasm-opcost` prints it. Release build on
linux/arm64, 100k iterations, one run; module compile and instantiate are
excluded from both columns. The instruction-loop measurements span
~0.56-1.33 ns/fuel, with `call_indirect`, `table.fill`, and `table.copy` at the
upper end. Both time and fuel include loop scaffolding, so these are not isolated
opcode costs. Guest fuel weights are unchanged by the 10:1 budget conversion;
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
