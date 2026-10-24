# WASM program performance

This file covers AVM-vs-wasmi comparisons:

- **LogicSigs** (below): a block of fibonacci TEAL LogicSigs against the same
  under the wasmi executor.
- **Host-call-bound applications**: a TEAL and a WASM application program that
  each increment a uint64 global the same number of times (see "Stateful
  application" below). Here the AVM stays ahead at every N.
- **Compute-bound applications**: a TEAL and a WASM application program that
  compute fib(N), which locates the wall-clock **crossover** where WASM
  overtakes the AVM (see "Compute-bound crossover" below).

The LogicSig benchmarks live in `ledger/lsigbench_test.go`; the application
benchmarks live in `data/transactions/logic/wasm_app_bench_test.go`.

## LogicSig fibonacci

Benchmarks comparing the cost of executing a block of **fibonacci TEAL
LogicSigs** against a block of **fibonacci WASM LogicSigs**. The WASM side runs
under the wasmi executor added in this branch (see `wasm/` and
`data/transactions/logic/wasm*.go`).

The benchmarks live in `ledger/lsigbench_test.go`:

- `BenchmarkBlockEvaluatorLsigs` — **execution only**: the block eval /
  verification pass (`eval.Eval(validate=true)`), which is where the LogicSigs
  actually run.
- `BenchmarkBlockEvaluatorLsigsFull` — **in-memory validate + apply + commit**:
  `Ledger.Validate` (executes
  the LogicSigs and computes the state delta) followed by `AddValidatedBlock`
  and a wait for the block to be committed to an in-memory database. It does not
  measure durable disk persistence.

## Setup

- Both blocks contain 100 contract-account LogicSig payments. The sender is the
  hash of the program, so each lsig is the sole authorization (no per-txn
  ed25519 signature).
- Both programs compute the same `fib(N)`, where `N` is read from LogicSig
  argument 0 (4-byte big-endian). The TEAL program is assembled at test time;
  the WASM program is the `fib.wasm` fixture.
- `N` is the largest input that fits the shared `LogicSigMaxCost` budget
  (20000) in **both** engines. wasmi spends more of the budget per iteration
  than the AVM costs, so `N` is limited by WASM:

  | | max N | budget used at N=1012 |
  | --- | --- | --- |
  | TEAL | 1332 | 15193 / 20000 (~76%) |
  | WASM | 1012 | 19998 / 20000 (~100%) |

- Fibonacci values are masked to 2^62 so the AVM `+` (which fails on uint64
  overflow) cannot trap; `fib.wasm` applies the same mask.

## Results

Measured on linux/arm64 (8 cores), Go 1.25.3, with
`go test ./ledger/ -run '^$' -bench '^BenchmarkBlockEvaluatorLsigs' -benchtime=2s -count=3`.

| benchmark | teal-fib | wasm-fib | wasm speedup |
| --- | --- | --- | --- |
| execution only | ~6.38 ms/block (~63.8 µs/tx) | ~2.15 ms/block (~21.4 µs/tx) | ~3.0x |
| in-memory validate + apply + commit | ~6.73 ms/block (~67.3 µs/tx) | ~2.46 ms/block (~24.6 µs/tx) | ~2.7x |

The non-execution block work (state-delta apply, tracker notification, in-memory
commit) adds only ~5–15% on top of execution for these blocks, so the wasm-vs-TEAL
gap survives this in-memory processing path. This does not establish the speedup
for a disk-backed node.

Note this is a **wall-clock** comparison. On the metered axis, WASM is the more
expensive engine: it burns more fuel per iteration and pays a fixed startup cost
per lsig.

## How to run

```
go test ./ledger/ -run '^$' -bench '^BenchmarkBlockEvaluatorLsigs' -benchtime=2s
```

Requires cgo and the prebuilt `wasm/target/release/libwasm_lsig.a`
(`make wasmlib`).

## Stateful application: global increment

`BenchmarkAppGlobalLoop` (`data/transactions/logic/wasm_app_bench_test.go`)
compares a TEAL and a WASM application program that each increment the same
uint64 global `counter` N times, driven through `EvalContract` on the in-memory
test ledger. It isolates engine execution (no block/ledger persistence).

- The TEAL program (assembled at test time) reads N from `ApplicationArgs[0]`
  with `btoi` and loops on `app_global_get` / `app_global_put`.
- The WASM program is `app_global_loop.wasm`, which reads N from
  `ApplicationArgs[0]` and loops on the `app_global_get_uint` /
  `app_global_put_uint` host functions (see `wasm/README.md`).
- N is the largest value that fits the app budget (`MaxAppProgramCost`, 700) in
  **both** engines. WASM is limited by a large **fixed** fuel charge per
  evaluation, not by a higher per-increment cost, so N is limited by WASM:

  | | max N | budget at max N |
  | --- | --- | --- |
  | TEAL | 42 | 173 / 700 (~25%) |
  | WASM | 10 | 696 / 700 (~100%) |

- Both engines then run at N=10 (the shared maximum), so "same number of loops".

Measured on linux/arm64 (8 cores), Go 1.25.3, with
`go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppGlobalLoop$' -benchtime=2s -count=3`.

| benchmark | ns/op (10 increments) | cost |
| --- | --- | --- |
| teal-global-loop | ~11.4 µs | 173 / 700 |
| wasm-global-loop | ~20.6 µs | 696 / 700 |

WASM is about 1.8x slower in wall-clock terms at N=10, but this is almost
entirely fixed per-evaluation startup: WASM compiles and instantiates the module
on every evaluation (no module cache or engine reuse yet — see `TODO.md`). Even
at N=0 it costs ~15 us vs TEAL's ~7 us, while the per-increment wall-clock cost
is close (TEAL ~410 ns vs WASM ~425 ns). The metered budget agrees: WASM's fixed
charge is ~516 fuel per evaluation (TEAL's is ~13), with ~18 vs ~16 per
increment, so most of the 700 budget is spent before many loops run. The
in-memory test ledger also clones its global-state map on every put, a fixed cost
paid by both engines. This does not establish the speedup for a disk-backed node.

### How to run

```
go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppGlobalLoop$' -benchtime=2s -v
```

Requires cgo and the prebuilt `wasm/target/release/libwasm_lsig.a`
(`make wasmlib`).

## Compute-bound crossover: where WASM overtakes the AVM

The stateful benchmark above is **host-call-bound** (one state access per
iteration), so the AVM stays ahead at every N. A compute-bound program behaves
the opposite way: wasmi runs guest instructions much faster than the AVM
interprets opcodes, so past a small threshold WASM overtakes it despite the fixed
startup cost.

`BenchmarkAppFibCrossover` (`data/transactions/logic/wasm_app_bench_test.go`)
compares a TEAL and a WASM application program that compute fib(N) in a loop, at
several N. It deliberately lifts `MaxAppProgramCost` so N is not budget-capped;
these N are not all reachable within one group, so it is a microbenchmark of
engine cost, not a valid-transaction projection.

Measured on linux/arm64 (8 cores), Go 1.25.3, with
`go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppFibCrossover$' -benchtime=2s -count=3`.

| N | TEAL time | TEAL cost | WASM time | WASM fuel | faster |
| --- | --- | --- | --- | --- | --- |
| 0 | ~6.5 us | 13 | ~13.7 us | 624 | TEAL |
| 50 | ~13.8 us | 763 | ~14.1 us | 1,674 | ~tie (**crossover**) |
| 100 | ~21.0 us | 1,513 | ~15.1 us | 2,724 | WASM 1.4x |
| 300 | ~51.0 us | 4,513 | ~18.7 us | 6,924 | WASM 2.7x |
| 1000 | ~155 us | 15,013 | ~30.9 us | 21,624 | WASM 5.0x |

The slopes are the point: **TEAL ~150 ns/iteration vs WASM ~15 ns/iteration**
(wasmi is ~10x faster per iteration). WASM pays a fixed \~7 us per evaluation
(module compile + instantiate), so it wins once the loop is long enough to
amortize it: `150N + 6500 = 15N + 13700` gives **N ~= 56**. Below ~56 iterations
the AVM is faster; above it WASM is faster, and the margin keeps growing. This is
the same effect the fibonacci LogicSig benchmark above shows.

Note the metered axis cuts the *other* way from wall clock: per iteration fib
costs TEAL ~15 cost units and WASM ~21 fuel, so WASM burns more budget while
running faster. wasmi's fuel units are not proportional to wall-clock cost.

Together the two application benchmarks frame the tradeoff: WASM wins when a
program is **compute-dominated** (many guest instructions per host call) and
loses when it is **host-call-dominated** (a state/txn access per iteration).

### How to run

```
go test ./data/transactions/logic/ -run '^$' -bench '^BenchmarkAppFibCrossover$' -benchtime=2s -v
```

Requires cgo and the prebuilt `wasm/target/release/libwasm_lsig.a`
(`make wasmlib`).

## Caveats

These are microbenchmarks, not a projection of mainnet block-validation
performance:

- Synthetic block shape: 100 txns, all the same program, contract-account lsigs
  with no per-txn signature. Real blocks are byte-limited (up to 5 MB,
  thousands of txns) and mix payment/asset/app transactions.
- The execution-only benchmark bypasses the verified-transaction cache to force
  the LogicSigs to run every iteration. On a node, txns already accepted into
  the pool are cached, so their lsigs are not re-run when the block arrives.
- WASM pays module compile + instantiate per lsig: there is no module cache or
  engine reuse yet (see `TODO.md`). The WASM numbers are therefore the current,
  unoptimized implementation.
- The `Full` benchmark uses an in-memory ledger database: it includes `Validate`
  - `AddValidatedBlock` + in-memory block-DB commit, not disk I/O, fsync, or every
  background tracker commit (the account tracker lags by `MaxAcctLookback`).
- Single machine, warm in-memory ledger caches, and core-count dependent
  (`PaysetGroups` verifies concurrently).

For a more production-like picture, use a disk-backed ledger and measure a
realistic byte-limited block with a mixed transaction set and delegated lsigs,
and land module caching / engine reuse in the WASM path before drawing conclusions.
