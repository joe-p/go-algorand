# WASM — remaining work

## Feasibility questions (spike exit criteria)

The spike demonstrates execution, ledger integration, and competitive compute
performance. The following experiments could still change the feasibility
conclusion; the remaining sections mostly track productionization, not spike
requirements. Agree on acceptable validation-time and memory limits and a
representative contract before judging these results.

- [ ] **Can adversarial workloads stay within acceptable validation time and
      memory?** Measure budget-filling groups/blocks, including static validation,
      compilation, maximum memory allocation/growth, expensive instruction
      patterns, and large host-call payloads. Include concurrent evaluations and
      failed/clear-state executions. Current single-machine microbenchmarks do
      not establish a worst-case bound. Check that safe pricing does not make
      useful contracts impractical.
      - First pass on linux/arm64, 8 cores: strict max TEAL block (4 KiB `b~`)
        validates in 45.4-47.5 s / 1.88-1.97 GB; strict max WASM blocks
        measured at 0.57-1.09 s / 0.46-0.85 GB. Static validation is ~72-90 ms
        per max block.
        The default 700-unit app budget is below even a trivial WASM app's
        total cost (922-945), which is dominated by setup, so apps need pooling. See
        `WASM_ADVERSARIAL.md`. Remaining: app/inner-txn blocks, amd64/cold
        runs, and many-core memory aggregation.
- [x] **Can realistic, compiler-generated contracts fit actual size and budget
      limits?** Deploy and execute a representative Rust contract through
      algod/goal with normal protocol limits, without lifted benchmark caps.
      Record module size, setup/execution cost, and required budget pooling.
      Setup alone puts the Fibonacci fixture above one app's 700-unit allowance;
      determine whether useful contracts need excessive pooling or exceed size
      limits.
      - Measured with compiler-generated Rust fixtures deployed through algod on
        `ConsensusFuture`: a 560-byte stateful counter has 1,832 setup cost and
        needs 4 pooled app calls to deploy and 4 to call; a 4,522-byte
        ed25519 signed-claim contract has 4,821 setup cost (8,930 to call) and
        needs 8 calls to deploy and 13 of the maximum 16 to call. Creation runs
        the approval program, so deployment needs pooling too. A second
        signature check would exceed the 11,200 pooled ceiling. Size is not the
        binding limit; the 700/11,200 budget model is. A Rust clear-state
        program cannot fit the unpooled 700 budget. See `WASM_CONTRACTS.md`.
- [ ] **Do supported architectures produce identical results and fuel
      consumption?** Run the same corpus on amd64 and arm64, comparing acceptance,
      return values, state deltas, traps, and exact fuel/cost. Include fuel
      boundaries and memory/table growth. Portable integer execution is
      encouraging, but cross-platform agreement still needs demonstration.

## Current status

Status: an initial implementation is in place. WASM LogicSigs (recognized by the
`\0asm` magic in `LogicSig.Logic`) run under wasmi with access to arguments,
current-transaction fields, and `ed25519verify_bare`,
gated by the `EnableWasmLogicSig` consensus parameter (currently set only in
`vFuture`). A first cut of WASM **application** programs is also in place: a WASM
module may be an app's approval or clear-state program (both must be the same
kind), gated by `EnableWasmApp`, in `vFuture`. The
guest reads transaction fields (including the application arguments via a
`txna`-style accessor), runs `ed25519verify_bare`, and reads/writes global app
state. Local state, boxes, and inner transactions are not yet exposed. Both modes
use 10 fuel per AVM cost unit, with deterministic setup and calibrated host
charges (see `WASM_PERF.md`). This file tracks what is left.

## WASM stateful apps (global-state cut, in progress)

Done:

- [x] Recognize WASM app programs by the `\0asm` magic, dispatched from
      `EvalContract`/`CheckContract` (`data/transactions/logic/eval.go`) and
      gated by the new `EnableWasmApp` consensus parameter (`vFuture`).
- [x] Reject mixed approval/clear-state kinds (both WASM or both AVM) and reject
      changing an app between AVM and WASM, in `CheckContractVersions`.
- [x] Read application arguments through a `txna`-style host accessor
      (`txna_uint`/`txna_bytes`), matching the AVM, where `arg` is
      signature-only. `arg`/`arg_count` now trap in application mode.
- [x] Reuse the app page limits (`MaxAppProgramLen`, `MaxAppTotalProgramLen`,
      `ExtraProgramPages`); no new size parameters.
- [x] Treat a WASM app program as the protocol's maximum AVM version in inner
      app-call checks, so an AVM app can create or call a WASM app.
- [x] Global application state: typed per-op host functions
      (`app_global_get_uint`, `app_global_get_bytes`, `app_global_put_uint`,
      `app_global_put_bytes`, `app_global_del`) that share the AVM's ledger
      accessors and limits, plus calibrated host-side fuel charges (not AVM
      opcode-price parity).
- [x] Guest SDK bindings and helpers (`txna_uint`, `txna_bytes`,
      `app_args_count`, `app_arg_into`, `app_global_*`).

Still to do:

- [ ] Extend the stateful surface beyond global state: local state
      (`app_local_*`), boxes, `log`, inner transactions, foreign access, and
      scratch, wired to shared Go primitives so AVM and WASM can't disagree.
- [x] Emit tracer hooks (`BeforeProgram`/`AfterProgram`) for the WASM app path
      so `simulate` reports `app-budget-consumed` and a program trace frame.
      WASM has no AVM opcode-level trace; state-change explanations use the
      existing transaction/delta hooks.
- [ ] Decide the release path for `EnableWasmApp` (which numbered version).
- [ ] Confirm `WellSizedPrograms` page accounting is the right cap for WASM app
      modules, or add a WASM-specific app program size limit.
- [x] e2e test creating and calling a WASM app through algod/goal (raw
      `--approval-prog-raw`/`--clear-prog-raw` already accept the bytes). See
      `test/e2e-go/features/transactions/wasm_contract_test.go` and
      `test/scripts/e2e_subs/e2e-wasm-contract-budget.sh`; measurements are in
      `WASM_CONTRACTS.md`.
- [ ] Extend WASM app determinism and exact fuel-boundary coverage. Budget
      conversion, default-budget limits, and charging failed setup/execution
      already have tests.

## Consensus / protocol

- [ ] Update the sibling `../specs` repository to describe WASM LogicSigs/apps,
      `EnableWasmLogicSig`/`EnableWasmApp`, and resource accounting (per
      `AGENTS.md`, new consensus parameters require a spec update).
- [ ] Decide the release path for `EnableWasmLogicSig`: which numbered consensus
      version it moves to, and whether `LogicSigVersion` needs a bump when it
      graduates out of `vFuture`.
- [ ] Confirm `MaxAbsoluteLogicSigProgramSize` (16000 in `vFuture`) is the right
      cap for WASM programs, or add a separate WASM-specific program size limit.
- [ ] Decide whether resource accounting should carry a dedicated knob (e.g. a
      `WasmLogicSigMaxFuel` / fuel-per-cost rate) instead of reusing
      AVM/shared budgets at the current fixed 10 fuel per cost unit.
- [ ] Review the fixed per-invocation resource caps (one 4 MiB linear memory,
      one table of up to 1024 elements, one instance) before release, and decide
      whether they need dedicated consensus parameters. Both initial allocation
      and runtime growth are bounded by these caps. Initial memory/table
      allocation is now included in the deterministic setup charge; runtime
      growth uses wasmi's execution metering. Assess aggregate memory pressure
      under concurrent evaluations, not just per-invocation limits.
- [ ] Pin down the accepted WebAssembly feature set explicitly (currently floats
      off, no SIMD, no memory64, no auto-dispatch). Verify bulk-memory,
      reference-types, multi-value, etc. are intended and deterministic.
- [x] Forbid WebAssembly start sections during static validation and execution.
      An exported `_start` name alone is allowed but never automatically invoked.
- [ ] Cross-check behavior with `rejectOnCurveLogicSigPrograms` and any other
      consensus or policy checks that parse `Lsig.Logic` as AVM bytecode.

## Correctness / policy

- [ ] Extend the algod API on-curve escrow check
      (`daemon/algod/api/server/v2/handlers.go`) to WASM LogicSigs; today it
      parses the program as AVM and silently skips WASM programs.
- [x] Emit tracer hooks for WASM programs so `simulate` produces traces
      (`logic.EvalTracer` `BeforeProgram`/`AfterProgram` are now called on both
      the WASM signature and app paths).
- [ ] Ensure the `!cgo` / `gofuzz` fallback returns a clear, user-facing error
      through the normal transaction rejection paths, and confirm fuzz builds
      behave as intended.

## Performance

- [ ] Cache compiled wasmi modules: static validation currently compiles a module
      without instantiating it, and evaluation compiles again and instantiates a
      fresh store. Consider a bounded cache keyed by program hash and engine /
      protocol configuration; do not share mutable guest state or discount
      consensus fuel based on cache hits.
- [ ] Reuse an `Engine` across calls instead of building one per evaluation.
- [x] Benchmark uncached app execution against the AVM, matched host-work loops,
      instruction loops, and separate setup/teardown costs; see `WASM_PERF.md`.
- [ ] Extend measurements beyond warm-cache, single-machine microbenchmarks to
      cold runs, supported platforms, and realistic group/block validation.

## Extensibility

- [ ] Expose opcodes, transaction fields, and other data to guest programs via
      additional `env` host functions (design the ABI and add Go↔Rust callbacks
      as needed). Current-transaction scalar and array
      transaction-field reads (`txna_uint`/`txna_bytes`) are in place; `gtxn`,
      `gtxna`, `itxn`, `global`, and the remaining crypto/state opcodes are not.
- [ ] Add `goal`/clerk tooling to create, compile-to-WASM, and submit WASM
      LogicSigs, mirroring the existing TEAL lsig flow.
- [x] Provide a small guest-authoring SDK / ABI doc and example contracts.
      `wasm/guest` contains safe host bindings (`wasm-guest`), a host-backed
      `ed25519-dalek` fork selected via `[patch.crates-io]`, and an example guest
      with both Rust and Go end-to-end tests. Still to do: expose the full
      opcode/txn surface through the SDK, and build the guest fixture in CI
      (today only `make wasm-guest-fixtures` produces it).

## Build & packaging

- [ ] Add Rust toolchain installation to the developer setup
      (`scripts/configure_dev.sh`), CI, and Dockerfile; `make build` now requires
      `cargo` because `buildsrc` depends on `wasmlib`. Building the guest
      fixtures (`make wasm-guest-fixtures`) additionally needs the
      `wasm32-unknown-unknown` target.
- [ ] Decide how to ship the static library: build from source everywhere, or
      vendor prebuilt per-platform `.a` files like `crypto/libs`.
- [ ] Support all shipped platforms (darwin/linux/windows, amd64/arm64) and the
      cross-compile path (`CROSS_COMPILE_ARCH`, `make universal`).
- [ ] Verify static linking on each platform; the release `.a` currently pulls in
      parts of Rust `std` that produce glibc static-link warnings on Linux.
- [ ] Consider a `no_std` wasmi build to shrink the library and drop `std`
      dependencies.

## Testing

- [ ] Add e2e tests (`test/e2e-go/...`) exercising a WASM LogicSig through
      algod/goal.
- [x] Cover invalid `arg` buffers and memory-boundary writes, forbidden start
      sections, unknown imports, and allocation/growth limits in Rust tests.
      Setup tests/benchmarks also exercise modules with many functions.
- [ ] Extend argument-limit coverage, including empty/oversized args near the
      limits and the `EvalMaxArgs`/`ARGS_READ_WORDS` coupling.
- [ ] Determinism tests: same program + args yields the same result and identical
      fuel/cost across runs.
- [x] Test host access restrictions: signature-only arguments trap in app mode,
      global state traps in signature mode, and transaction field type/mode
      restrictions are enforced.
- [x] Test fuel exhaustion, 10:1 budget conversion/rounding, setup reservation,
      and charging host fuel before work.
- [ ] Extend exact-fuel-completion and one-unit-short coverage across guest
      execution, setup, and host paths (including mixed TEAL/WASM pooled budgets).

## Docs

- [ ] Note the new build prerequisite and `make wasmlib` in `AGENTS.md` (and any
      CONTRIBUTING/build docs).
- [ ] Document the WASM LogicSig ABI and behavior in the user-facing docs.
