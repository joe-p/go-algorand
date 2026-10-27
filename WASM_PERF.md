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
Guest instruction charges are unchanged by the conversion (the prototype
operator table under "Call-stack cost" is a separate change). Host charges are
calibrated toward **1 ns/fuel**, with an explicit 28,500-fuel host charge for
`ed25519verify_bare` (2850 AVM budget units, versus the TEAL opcode's 1900; the
whole call is 28,527 fuel under the prototype operator table). Outside the prototype operator
table, host weights remain unchanged from the 15:1 calibration; the setup
type, global, and memory-page weights were later lowered (see "Setup fuel
pricing").
Consumed fuel is converted back to AVM cost with rounding up per evaluation,
so shared budgets remain in AVM units, including mixed TEAL/WASM groups.
Total fuel now includes a deterministic setup charge reserved before compilation
or allocation, in addition to wasmi's execution and lazy-translation fuel.

Moving from 15:1 to 10:1 charges 50% more budget for the same raw fuel (before
rounding), targeting less wall time per allowed budget unit. The conversion is
deterministic and consensus-relevant; benchmark timing never changes it at
runtime. These microbenchmarks do not prove an AVM-equivalent worst-case
validation-time bound. Cost columns use AVM units; fuel uses wasmi's denomination.
Compute/host/global-loop timings were rerun for 10:1 below; their WASM fuel and
cost columns also include the prototype operator table. The setup table is
unchanged because setup never executes guest code.

The host weights were selected from the pre-calibration fixtures as
`whole-iteration ns / (1 ns/fuel) - guest/loop fuel`, rounded up to convenient
constants. This estimates the host portion by assuming the guest scaffolding
runs at the target 1 ns/fuel; it does **not** subtract separately measured
loop-only time or independently measure host-call or boundary time.
They are deterministic protocol weights, not runtime timers. These weights
predate the prototype operator table; each host call now additionally pays the
10-fuel `call` operator, which is why the refreshed table below measures
0.78-1.01 ns/fuel rather than ~1. Host budget prices need not equal their AVM
opcode equivalents; signature verification also retains its raw weight rather
than AVM price parity. `txna_uint` provisionally shares `txn_uint`'s charge.
Payload-size and cross-machine calibration remain future work.

## Compute-bound crossover: where WASM overtakes the AVM

`BenchmarkAppFibCrossover` (`data/transactions/logic/wasm_app_bench_test.go`)
compares a TEAL and a WASM application program that compute `fib(N)` in a loop.
It deliberately lifts `MaxAppProgramCost` so N is not budget-capped; these N are
not all reachable within one group, so it measures engine cost, not valid
transactions.

Refreshed on 2026-10-25 on linux/arm64 (8 cores), Go 1.25.3. Timings are
medians of three runs, with
`go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppFibCrossover$' -benchtime=2s -count=3`.
The WASM fuel and cost columns use the prototype operator table (see
"Call-stack cost"); the crossover itself is a wall-time result and is unaffected.

| N | TEAL time | TEAL cost | WASM time | WASM fuel | WASM cost | faster |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | ~6.4 us | 13 | ~13.3 us | 9,220 | 922 | TEAL 2.1x |
| 50 | ~13.4 us | 763 | ~14.2 us | 10,320 | 1,032 | TEAL 1.1x |
| 100 | ~21.0 us | 1,513 | ~15.3 us | 11,420 | 1,142 | WASM 1.4x |
| 300 | ~50.9 us | 4,513 | ~18.5 us | 15,820 | 1,582 | WASM 2.8x |
| 1000 | ~152.2 us | 15,013 | ~34.0 us | 31,220 | 3,122 | WASM 4.5x |

The endpoint slopes are **TEAL ~146 ns/iteration vs WASM ~21 ns/iteration**.
WASM has higher fixed evaluation overhead (including module compile and
instantiate, no cache yet); these samples cross between N=50 and N=100.
Short-run timing varies, so this is not a precise, stable crossover N.

Each WASM row includes **8,382 setup fuel** (839 AVM units before execution)
at the weights in force when measured; the fixture is 8,032 fuel at the current
weights. At N=50, the remaining 1,938 fuel is execution/lazy translation, so
the total cost was `ceil((8382 + 1938) / 10) = 1032`, versus 688 at 15:1.
Setup is charged once per evaluation, not per iteration. This fixture no longer
fits a single default 700-unit app budget even at N=0; it needs pooled budget.

Fib costs TEAL ~15 cost/iteration vs WASM ~22 fuel/iteration. With the 10:1
conversion, that guest execution costs ~2.2 AVM units/iteration, not 22.

For the validation-time ceiling question, time **per charged budget unit** is
more useful than same-work speedup. Using the unrounded median timings above:

| N | TEAL ns/cost | WASM ns/cost at 10:1 | WASM/TEAL time per cost |
| --- | --- | --- | --- |
| 100 | 13.9 | 13.4 | 0.97x |
| 300 | 11.3 | 11.7 | 1.04x |
| 1000 | 10.1 | 10.9 | 1.07x |

10:1 is substantially closer than 15:1, but does **not** establish a strict
AVM ceiling: N=300 and N=1000 still use 4-7% more time per cost unit. These
ratios are comparisons of the measured workloads, not worst-case
budget-filling group/block bounds. The denomination change does not make
execution faster; differences from the prior timing table are measurement
variation.

## Call-stack cost: recursion depth

`BenchmarkAppFibRecursive` and `BenchmarkAppCallDepth`
(`data/transactions/logic/wasm_app_bench_test.go`) run recursive application
programs in both engines. Recursive fib (`app_fib_rec.wasm`, TEAL `callsub`) has
depth N but an exponential number of calls. The depth sweep
(`app_rec_depth.wasm`, TEAL `callsub`) instead does O(1) work per frame, so the
number of calls equals the depth and cost growth is the cost of the call stack
itself. `app_depth_loop.wasm` and `tealDepthLoopAppSrc` are iterative controls
for the same N; their bodies are a slightly different instruction mix, so the
rec/loop gap is approximate, not an exact frame-cost subtraction.

**Prototype operator weights.** These numbers use the custom `OperatorCost`
table in `wasm/src/lib.rs` (`guest_operator_costs`): every operator costs at
least 1 fuel and `call`/`call_indirect` cost 10. wasmi's default charges 1 for
`call` but 0 for `nop`, `drop`, `block`, `loop`, `else`, `end`, `return`, and
`unreachable`, which undercharged the frame: a recursive frame measured ~20 ns
but only ~11 fuel, while loop code sat near 1 ns/fuel. `return` emits a real
runtime return (it pops a frame) and `else` can emit a branch, so neither is
free work; a floor of 1 also avoids depending on which operators wasmi happens
to erase at translation time. Fuel totals for any program containing calls
(including host calls) shift by +9 per call under this table. The compute,
host-call, and opcode tables have been refreshed for it; setup-only numbers are
unaffected. Because the explicit host charges were calibrated at 1 ns/fuel, the
extra `call` fuel puts host calls at 0.78-1.01 ns/fuel until those constants are
trimmed by 9-10 fuel.

Refreshed on 2026-10-25 on linux/arm64 (8 cores), Go 1.25.3. Timings are medians
of three runs, with
`go test ./data/transactions/logic/ -run '^$' -bench 'BenchmarkApp(FibRecursive|CallDepth)$' -benchtime=500ms -count=3`.

Recursive fib, result written to the global `fib`:

| N | TEAL time | TEAL cost | WASM time | WASM fuel | WASM cost | faster |
| --- | --- | --- | --- | --- | --- | --- |
| 5 | ~10.3 us | 148 | ~19.9 us | 11,008 | 1,101 | TEAL 1.9x |
| 10 | ~27.6 us | 1,687 | ~23.0 us | 14,653 | 1,466 | WASM 1.2x |
| 15 | ~205.9 us | 18,749 | ~65.1 us | 55,063 | 5,507 | WASM 3.2x |
| 20 | ~2,201 us | 207,970 | ~512 us | 503,218 | 50,322 | WASM 4.3x |

At N=20 recursive fib makes 21,891 calls (`2*fib(21)-1`). That is roughly
**9.5 cost/call** in TEAL and **23 fuel/call (2.3 cost/call)** in WASM. The cost
ratio is now **4.1x** and the wall-time ratio **4.3x**; before the operator
table, WASM was 7.9x cheaper but only 4.4x faster. This is the point of the
prototype: cost now tracks time for call-heavy code. The crossover is still
early (between N=5 and N=10) because of WASM's fixed setup cost.

Call-depth sweep, with `*-loop` for scale:

| depth | TEAL rec time | TEAL rec cost | TEAL loop cost | WASM rec time | WASM rec fuel | WASM rec cost | WASM loop cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 16 | ~10.4 us | 143 | 188 | ~19.5 us | 10,965 | 1,097 | 1,040 |
| 64 | ~14.6 us | 527 | 716 | ~20.5 us | 12,021 | 1,203 | 1,108 |
| 256 | ~32.0 us | 2,063 | 2,828 | ~27.4 us | 16,245 | 1,625 | 1,376 |
| 512 | ~51.4 us | 4,111 | 5,644 | ~28.4 us | 21,877 | 2,188 | 1,735 |

Marginal costs from depth 16 to 512 (496 frames). The two fixtures differ slightly
in setup (the recursive module defines an extra function, about 430 fuel more),
so compare slopes across depth rather than the totals between fixtures:

- **TEAL `callsub` frame: ~8 cost units and ~83 ns.** The cost delta is
  `(4111-143)/496`; the recursive body charges call, return, and its arithmetic
  opcodes.
- **WASM call frame: ~22 fuel (2.2 cost units) and ~18 ns.** The fuel delta is
  `(21877-10965)/496`; `call` plus the structural operators is 22 fuel. That is
  slightly conservative for the frame alone (~0.8 ns/fuel), while the fixed
  setup charge remains undercharged in wall time; at depth 512 the whole-eval
  figures land at 13.0 ns/cost for WASM versus 12.5 for TEAL.
- **AVM frames allocate; WASMI frames do not.** TEAL `B/op` grows from ~20.4 kB
  at depth 16 to ~61.1 kB at depth 512 (the `cx.callstack` slice, ~80 B/frame).
  WASM stays at ~17.7 kB/op across the sweep because wasmi reuses its value
  stack. This shows up in `allocs/op` (58 to 62 for TEAL, flat at 55 for WASM).

WASMI is configured with `max_recursion_depth` 1024 (`wasm/src/lib.rs`), so
depths above that trap as `wasm program trapped: call stack exhausted`. The trap
is charged like any other failure. `TestWasmAppRecursionDepth` checks that
boundary and the depth results; `TestWasmAppRecursiveFib` checks the recursive
fib values against known results.

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
| `txn_uint` | 125 | 147 | 14.70 | 137 | 0.93 | `txn` (1) |
| `txn_bytes` | 190 | 214 | 21.40 | 206 | 0.96 | `txn` (1) |
| `txna_bytes` | 200 | 225 | 22.50 | 207 | 0.92 | `txna` (1) |
| `app_global_get_uint` | 130 | 153 | 15.30 | 148 | 0.96 | `app_global_get` (1) |
| `app_global_get_bytes` | 170 | 195 | 19.50 | 189 | 0.97 | `app_global_get` (1) |
| `app_global_put_uint` | 410 | 437 | 43.70 | 408 | 0.93 | `app_global_put` (1) |
| `app_global_put_bytes` | 390 | 414 | 41.40 | 400 | 0.97 | `app_global_put` (1) |
| `app_global_del` | 140 | 162 | 16.20 | 155 | 0.96 | `app_global_del` (1) |
| `ed25519verify_bare` | 28,500 | 28,527 | 2852.70 | 28,711 | 1.01 | `ed25519verify_bare` (1900) |
| `arg` (sig) | 30 | 54 | 5.40 | 42 | 0.78 | `arg` (1) |
| `arg_count` (sig) | 10 | 31 | 3.10 | 19 | 0.60 | no direct equivalent |

The calibrated fixtures now cluster around **0.78-1.01 ns/fuel** (0.60 for
`arg_count`), below the 1 ns/fuel target. That is the prototype `call` weight:
each host invocation also pays the 10-fuel `call` operator, while the explicit
host charge was calibrated at 1 ns/fuel. Trimming 9-10 fuel from each host
charge would restore parity if the operator table is adopted. The explicit host
charge excludes guest/loop fuel, which is still charged by wasmi. Signature
verification costs 28,500 fuel plus 27 fuel of fixture overhead: 2850 budget
units for the host charge, compared with the TEAL opcode's 1900.
State/transaction host weights exceed their TEAL opcode prices under this
independent time-based calibration. The weights account for the host path, not
just crossing the WASM boundary, and their ratios to TEAL prices are not measured
slowdown factors. These are fixture calibrations, not guarantees for every
payload size or platform.

For example, `TXN_UINT_FUEL_COST = 125` is the explicit charge per invocation,
not the total iteration cost and not an independently measured 125 ns duration:

```text
txn_uint host charge       = 125 fuel = 12.50 AVM cost
guest scaffolding          =  22 fuel =  2.20 AVM cost
whole iteration            = 147 fuel = 14.70 AVM cost
```

The fixture measures about 137 ns per whole iteration, so the 147-fuel charge is
about 7% above the 1 ns/fuel target; the extra fuel is the prototype 10-fuel
`call` plus 12 fuel of original scaffolding. Later runs can differ (see the
matched table below) without changing the fixed 125-fuel protocol weight.
Neither 12.50 nor 14.70 establishes a corresponding slowdown relative to TEAL's
1-cost `txn` opcode.

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
transaction field, and one stores the result. Thus the comparison is **14.70
versus 11 for whole iterations**, or **12.50 versus 1 for explicit operation
charges**. Neither comparison isolates operation timings. Zero-iteration
subtraction removes fixed setup, not this repeated scaffolding, in both engines.

| host work | WASM cost/iter | TEAL cost/iter | WASM ns/iter | TEAL ns/iter | WASM/TEAL time |
| --- | --- | --- | --- | --- | --- |
| `txn_uint` (`FirstValid`) | 14.70 | 11 | 137 | 129 | 1.06x |
| `txn_bytes` (`Sender`, 32 bytes) | 21.40 | 11 | 206 | 128 | 1.61x |
| `txna_bytes` (`ApplicationArgs[0]`, 4 bytes) | 22.50 | 11 | 207 | 126 | 1.64x |
| `app_global_get_uint` (absent `counter`) | 15.30 | 12 | 148 | 134 | 1.10x |
| `app_global_get_bytes` (absent `b`) | 19.50 | 12 | 189 | 122 | 1.55x |
| `app_global_put_uint` (incrementing `counter`) | 43.70 | 16 | 408 | 409 | 1.00x |
| `app_global_put_bytes` (`b` = `x`) | 41.40 | 12 | 400 | 332 | 1.20x |
| `app_global_del` (absent `d`) | 16.20 | 11 | 155 | 130 | 1.19x |
| `ed25519verify_bare` (32-byte message) | 2852.70 | 1913 | 28,711 | 28,726 | 1.00x |
| `arg` (sig, 4 bytes) | 5.40 | 11 | 42 | 92 | 0.46x |

Thus **15.3 budget units versus the TEAL opcode price of 1 does not mean a
15.3x slowdown**. It also compares a WASM iteration with a single TEAL opcode:
the matched TEAL get iteration costs 12, not 1. On this workload the full WASM
get iteration is about 10% slower. Neither this ratio nor subtraction of the
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
| TEAL | ~10.5 us | 125 | — |
| WASM | ~20.3 us | 1,381 | 13,810 |

At equal work the AVM is **1.9x faster**. The WASM row includes the prototype
10-fuel `call` on each of the two host invocations per iteration. Under the configured 1,400-unit budget
(14,000 WASM fuel), maxima are 86 increments for TEAL and 7 for WASM (down from
24 at 15:1). Actual
protocol budgets are unchanged: this is an engine microbenchmark, not a claim
that a single default-budget application can run these loop counts.

## New host imports: calibration (2026-10-27)

The new timing harnesses are `TestWasmNewHostCallCost` in
`data/transactions/logic/wasm_new_hostcost_test.go` and
`TestWasmInnerSubmitCost` in `ledger/eval/wasm_hostcost_test.go`. They compile
temporary WAT fixtures using the existing Rust `wat2wasm` example, outside the
timed region. Build the release executor first:

```sh
make wasmlib
WASM_OPCOST=1 GOMAXPROCS=1 go test ./data/transactions/logic -run '^TestWasm(NewHostCallCost|HostCallCost)$' -v -count=3
WASM_OPCOST=1 GOMAXPROCS=1 go test ./ledger/eval -run '^TestWasmInnerSubmitCost$' -v -count=3
```

Each new harness interleaves zero-work and work evaluations of the **same
module**, alternating order, with fresh ledger state outside timing. This
cancels fixed compilation/instantiation work without accumulating created
assets or moving balances across samples. Both force `GOMAXPROCS=1` during
measurement and restore it afterward. Only the application budget is raised;
log and inner limits remain enforced. The logic harness uses the existing test
protocol (8 inners/group, 550-byte Note); the production harness uses the block
evaluator's protocol (up to 16/group).

Submission must be calibrated on the **production COW/Perform path**, not just
the logic test ledger. The production fixture prepares the same group in both
runs, then conditionally submits it. The child COW is discarded after each
evaluation. Transfers move one unit to another opted-in account; opt-in starts
with a missing holding. In a multi-inner opt-in group only the first inner
creates the holding; later ones repeat the zero self-transfer.

Measured on linux/arm64, 8-core Apple CPU, Go 1.25.3, release Rust executor.
Production-submit results below are medians of three runs, with 4,000 paired
samples per row in each run:

| inners/group | asset create (ns) | nonzero transfer (ns) | opt-in (ns) | pre-change host fuel |
| --- | ---: | ---: | ---: | ---: |
| 1 | 2,658 | 2,661 | 3,039 | 2,500 |
| 2 | 6,082 | 5,799 | 6,044 | 4,500 |
| 4 | 10,509 | 9,896 | 10,805 | 8,500 |
| 8 | 18,218 | 18,372 | 16,518 | 16,500 |
| 16 | 35,990 | 33,999 | 29,848 | 32,500 |

The table excludes group construction through the field/begin/next imports;
those are charged separately. The timing difference still includes the
conditional/call guest instructions. The production harness prints **AVM cost
differences**, which have evaluation-level rounding, not exact raw fuel.
At the existing ~1 ns/fuel calibration target, the initial timing-based
recommendation was **2,000 + 2,500 per inner** for submit, rather than
500 + 2,000. That recommendation was **not adopted**: begin, next, and submit
are charged at the protocol level and now each pay only a flat **10-fuel host
charge**, with no per-inner submit host charge. Their ns/fuel ratios should
therefore not be interpreted as standalone calibration failures.

The logic-ledger measurements below are medians of three runs (begin uses the
five-run follow-up described below). As in the
existing host table, times include guest loop/call scaffolding. The candidate
host weights allow for that small overhead; they are not rounded wall-clock
measurements inserted directly into the protocol.

| import / case | ns/iteration | pre-change host fuel | applied host fuel |
| --- | ---: | ---: | ---: |
| `gtxn_uint` | 139 | 125 | 125 (keep) |
| `gtxn_bytes`, 32 / 550 bytes | 219 / 243 | 190 | 225 |
| `global_uint` (`GroupSize`) | 96 | 125 | 125 (keep) |
| `global_bytes` (app address) | 199 | 190 | 190 (keep) |
| `asset_balance`, present / absent holding | 515 / 831 | 300 | 900 |
| `asset_unit_name`, 8 bytes / length / absent asset | 288 / 295 / 347 | 300 | 350 |
| `itxn_begin` | 695 | 300 | 10 (protocol-level charging) |
| `itxn_next`, averaged over 7 additions | 1,823 | 300 | 10 (protocol-level charging) |
| `itxn_field_uint` (`ConfigAssetTotal`) | 97 | 200 | 200 (keep conservatively) |
| `itxn_field_bytes` (`Note`), 0 / 32 / 256 / 550 bytes | 107 / 154 / 231 / 321 | 200 + bytes | 200 + bytes (keep) |
| `itxn_created_asset_id` | 121 | 125 | 125 (keep) |
| `log`, 0 / 32 / 256 / 1,024 bytes | 119 / 130 / 266 / 648 | 200 + bytes | 200 + bytes (keep) |
| `itxn_submit` | production table above | 500 + 2,000/txn | 10 flat (protocol-level charging) |

The existing calibration control still aligns: `ed25519verify_bare` takes
approximately 28 us for 28,527 total iteration fuel, and the matched
`txn_uint` control is approximately 135 ns for 147 fuel. The empty new loop is
approximately 9 ns for 10 fuel. The `host estimate` output column subtracts
non-host guest fuel at 1 ns/fuel, following the existing calibration method;
it does not independently isolate boundary time.

Do **not** calibrate only successful asset queries. A missing holding is a
successful query returning "not found", but internally constructs an error
before converting it into the existence flag; it is substantially slower here.
The named-asset path alone would incorrectly suggest that 300 fuel suffices.
The suggested fixed charges cover both paths measured, before any host work.

The read/query changes in the table are applied in the Rust fuel constants;
the begin/next/submit weights reflect the protocol-level charging policy rather
than the standalone timing recommendation. Preserve headroom rather than reducing the already conservative
log/field charges from these small fixtures. Single-call differences (especially
begin and large logs) are noisy because they subtract two complete WASM
evaluations: the three initial begin timings ranged from 260 to 1,371 ns.
Increasing begin to 40,000 paired evaluations and repeating five times reduced
the range to 615–863 ns, with a median of 695 ns. Reproduce that follow-up with:

```sh
WASM_OPCOST=1 GOMAXPROCS=1 go test ./data/transactions/logic -run '^TestWasmNewHostCallCost$/^itxn_begin$' -v -count=5
```

`itxn_next` has group-size-dependent fee scanning and slice growth, so a
16-wide production-path sweep would be needed before independently pricing its
host execution. These
measurements do not cover every field, maximum resource sharing, asset
close-out/reconfiguration, maximum transaction payloads, cold ledger/cache
misses, or other CPUs. In particular, query weights still need confirmation on
production ledger state. They are a practical calibration starting point, not
a consensus-safe worst-case proof.

## Instruction opcodes

`opcode_cost_table` (`wasm/src/lib.rs`, an `#[ignore]`d test) runs each allowed
instruction in a tight loop; `make wasm-opcost` prints it. Release build on
linux/arm64, 100k iterations, medians of five runs; module compile and
instantiate are excluded from both columns. The instruction-loop measurements
span ~0.51-1.19 ns/fuel, with `table.fill` and `table.copy` at the upper end.
`call` is now the lowest: its 10-fuel weight is set for recursive frame cost
(see "Call-stack cost"), not for the tight-loop path. Both time and fuel include
loop scaffolding, so these are not isolated opcode costs. The fuel column uses
the prototype operator table, so every previously free scaffolding operator
costs 1 and `call` costs 10.

| op | fuel/iter | ns/iter | ns/fuel |
| --- | --- | --- | --- |
| nop | 11.0 | 9.2 | 0.84 |
| i32.const | 12.0 | 9.2 | 0.76 |
| i64.const | 12.0 | 8.9 | 0.74 |
| local.get | 12.0 | 8.9 | 0.74 |
| local.set | 12.0 | 10.2 | 0.85 |
| local.tee | 13.0 | 9.7 | 0.75 |
| global.get | 12.0 | 11.5 | 0.95 |
| global.set | 12.0 | 11.3 | 0.95 |
| i32.add | 14.0 | 11.6 | 0.83 |
| i32.mul | 14.0 | 11.2 | 0.80 |
| i32.div_s | 14.0 | 11.7 | 0.84 |
| i32.rem_s | 14.0 | 10.5 | 0.75 |
| i32.rotl | 14.0 | 11.0 | 0.79 |
| i32.shl | 14.0 | 11.7 | 0.83 |
| i32.eq | 14.0 | 11.3 | 0.80 |
| i32.lt_s | 14.0 | 11.1 | 0.80 |
| i32.clz | 13.0 | 10.3 | 0.79 |
| i32.ctz | 13.0 | 10.4 | 0.80 |
| i32.popcnt | 13.0 | 11.4 | 0.88 |
| i32.eqz | 13.0 | 10.5 | 0.81 |
| i64.add | 14.0 | 11.2 | 0.80 |
| i64.mul | 14.0 | 11.6 | 0.83 |
| i64.div_s | 14.0 | 11.1 | 0.79 |
| i64.rotl | 14.0 | 10.6 | 0.76 |
| i64.clz | 13.0 | 10.9 | 0.84 |
| i64.popcnt | 13.0 | 11.6 | 0.90 |
| i32.load | 13.0 | 9.9 | 0.76 |
| i32.load8_u | 13.0 | 10.4 | 0.80 |
| i64.load | 13.0 | 10.0 | 0.77 |
| i32.store | 13.0 | 8.6 | 0.66 |
| i32.store8 | 13.0 | 10.1 | 0.77 |
| i64.store | 13.0 | 10.3 | 0.79 |
| memory.size | 12.0 | 10.4 | 0.87 |
| memory.grow0 | 13.0 | 10.0 | 0.77 |
| memory.copy (4096B) | 78.0 | 56.4 | 0.72 |
| memory.fill (4096B) | 78.0 | 74.3 | 0.95 |
| select | 15.0 | 10.4 | 0.69 |
| call | 24.0 | 12.3 | 0.51 |
| call_indirect | 25.0 | 17.6 | 0.70 |
| br_table | 18.0 | 12.9 | 0.72 |
| ref.null | 12.0 | 7.9 | 0.66 |
| ref.is_null | 13.0 | 8.9 | 0.69 |
| table.get | 13.0 | 11.0 | 0.84 |
| table.set | 13.0 | 9.4 | 0.73 |
| table.size | 12.0 | 10.5 | 0.88 |
| table.grow0 | 14.0 | 9.8 | 0.70 |
| table.fill | 14.0 | 15.8 | 1.13 |
| table.copy | 14.0 | 16.7 | 1.19 |
## Setup fuel pricing

`wasm/src/setup.rs` reserves this deterministic charge before creating an engine,
compiling the module, building the linker, or allocating guest memory/tables:

```text
setup fuel = 5000
           + 6   × module bytes (including custom sections and data)
           + 180 × functions (defined + imported)
           + 150 × function types
           + 4   × signature values (params + results, per function/import)
           + 100 × globals
           + 400 × exports
           + 100 × data/element segments (active, passive, or declared)
           + 300 × initial 64 KiB memory pages
           + 4   × initial table entries
           + 10  × element-segment items (including overlapping segments)
```

These are conservative, rounded **consensus constants**, not runtime timers or
cache-dependent discounts. Each weight keeps the size sweep that isolates it at
or under about 0.8 ns/fuel. The type, global, and memory-page weights were
lowered from 300, 150, and 350 after the sweeps below measured them at about
124, 91, and 245 ns per entity, which had left those terms at 0.40, 0.51, and
0.68 ns/fuel; the other weights were already within 0.7 to 0.8 on their sweeps.
Timing tables elsewhere in this document that include a setup charge predate
that change: subtract 150 per type, 50 per global, and 50 per memory page to
restate them. The base covers fixed work and teardown; byte pricing
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
| minimal `program` | 40 | 0.19 | 1.36 | 0.32 | 2.56 | 0.50 | 5,974 | 0.51 |
| 1 memory page | 45 | 0.21 | 1.49 | 0.65 | 3.03 | 0.55 | 6,304 | 0.57 |
| 16 memory pages | 45 | 0.21 | 1.42 | 4.30 | 6.61 | 0.53 | 10,804 | 0.66 |
| 64 memory pages (4 MiB) | 45 | 0.24 | 1.47 | 16.18 | 18.52 | 0.55 | 25,204 | 0.76 |
| 15,000 active data bytes | 15,055 | 0.24 | 1.95 | 0.87 | 3.78 | 0.57 | 96,464 | 0.05 |
| 100 unused functions | 641 | 1.78 | 14.44 | 1.14 | 18.06 | 0.80 | 27,980 | 0.67 |
| 1,000 unused functions | 6,045 | 16.06 | 130.75 | 8.70 | 156.26 | 3.25 | 226,004 | 0.71 |
| 2,000 unused functions | 12,045 | 31.94 | 260.09 | 16.96 | 309.80 | 4.74 | 446,004 | 0.71 |
| 12,000 nops | 12,042 | 0.20 | 24.63 | 0.34 | 25.88 | 0.53 | 77,986 | 0.34 |
| 4,000 nested blocks | 12,042 | 0.23 | 58.58 | 0.39 | 59.92 | 0.62 | 77,986 | 0.78 |
| 1,000 globals | 5,045 | 0.24 | 85.72 | 6.82 | 93.45 | 0.97 | 136,004 | 0.69 |
| 1,000 extra types | 3,043 | 25.56 | 101.23 | 0.36 | 128.01 | 0.65 | 173,992 | 0.74 |
| 1,000 empty data segments | 5,050 | 0.26 | 82.04 | 9.51 | 92.57 | 1.47 | 136,334 | 0.69 |
| 100 functions sharing 256 params | 715 | 2.67 | 66.08 | 1.18 | 70.71 | 0.89 | 130,574 | 0.55 |
| 1,000 extra exports | 6,952 | 0.32 | 148.26 | 144.87 | 294.86 | 26.69 | 447,776 | 0.72 |
| 1,024 null table entries | 47 | 0.23 | 1.51 | 0.44 | 2.90 | 0.57 | 10,112 | 0.34 |
| 1,024 active element items | 1,101 | 0.65 | 8.36 | 2.88 | 12.62 | 0.88 | 27,106 | 0.50 |
| 32,768 compressed locals | 44 | 0.20 | 1.83 | 0.33 | 3.05 | 0.60 | 5,998 | 0.61 |
| `app_fib.wasm` | 208 | 0.36 | 2.37 | 0.85 | 4.34 | 0.63 | 8,032 | 0.62 |
| `ed25519_dalek.wasm` | 4,761 | 1.35 | 18.90 | 5.33 | 26.36 | 0.87 | 45,460 | 0.60 |
| `app_rust_counter.wasm` | 560 | 0.63 | 4.78 | 5.33 | 11.37 | 0.71 | 16,512 | 0.73 |
| `app_rust_signed_claim.wasm` | 4,522 | 1.54 | 19.49 | 5.84 | 27.68 | 0.92 | 45,052 | 0.63 |
| `app_avm_sdk_counter.wasm` | 5,242 | 1.80 | 25.73 | 5.87 | 34.17 | 0.95 | 51,650 | 0.68 |
| `app_avm_sdk_arc4_counter.wasm` | 1,589 | 1.10 | 10.19 | 1.63 | 13.64 | 0.85 | 20,158 | 0.72 |
| `app_avm_sdk_arc4_types.wasm` | 5,797 | 1.78 | 27.39 | 1.73 | 31.63 | 0.96 | 49,966 | 0.65 |

Across all 165 case/run combinations at the current weights, the highest
measured setup-plus-teardown ratio was **0.84 ns/fuel**, for 4,000 nested
blocks; the lowered type, global, and memory-page terms peak at 0.77. This is empirical headroom on this machine, not a
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
- The application benchmarks drive `EvalContract` on the in-memory test ledger.
- Fuel/cost figures that include guest calls use the prototype operator table
  (see "Call-stack cost"). TEAL numbers and setup-only table numbers do not
  change with it.
- Bulk-memory copies are measured at 4096 bytes only. Setup weights are calibrated
  conservatively on this one machine, not a proven upper bound for all valid
  modules or validator platforms. Go/cgo/ledger overhead and static checking are
  not comprehensively priced by the Rust setup formula.
- Strict max TEAL/WASM block measurements, large host payloads, static
  validation, memory growth, failures, and useful-contract budgets are in
  `WASM_ADVERSARIAL.md`.
