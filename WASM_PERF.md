# WASM LogicSig performance

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
  |---|---|---|
  | TEAL | 1332 | 15193 / 20000 (~76%) |
  | WASM | 1012 | 19998 / 20000 (~100%) |

- Fibonacci values are masked to 2^62 so the AVM `+` (which fails on uint64
  overflow) cannot trap; `fib.wasm` applies the same mask.

## Results

Measured on linux/arm64 (8 cores), Go 1.25.3, with
`go test ./ledger/ -run '^$' -bench '^BenchmarkBlockEvaluatorLsigs' -benchtime=2s -count=3`.

| benchmark | teal-fib | wasm-fib | wasm speedup |
|---|---|---|---|
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
  + `AddValidatedBlock` + in-memory block-DB commit, not disk I/O, fsync, or every
  background tracker commit (the account tracker lags by `MaxAcctLookback`).
- Single machine, warm in-memory ledger caches, and core-count dependent
  (`PaysetGroups` verifies concurrently).

For a more production-like picture, use a disk-backed ledger and measure a
realistic byte-limited block with a mixed transaction set and delegated lsigs,
and land module caching / engine reuse in the WASM path before drawing conclusions.
