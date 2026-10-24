# WASM LogicSigs — remaining work

Status: an initial implementation is in place. WASM LogicSigs (recognized by the
`\0asm` magic in `LogicSig.Logic`) run under wasmi with access to arguments only,
gated by the `EnableWasmLogicSig` consensus parameter (currently set only in
`vFuture`). This file tracks what is left.

## Consensus / protocol

- [ ] Update the sibling `../specs` repository to describe WASM LogicSigs and the
      `EnableWasmLogicSig` parameter (per `AGENTS.md`, new consensus parameters
      require a spec update). Not done because that repo is absent from this
      workspace.
- [ ] Decide the release path for `EnableWasmLogicSig`: which numbered consensus
      version it moves to, and whether `LogicSigVersion` needs a bump when it
      graduates out of `vFuture`.
- [ ] Confirm `MaxAbsoluteLogicSigProgramSize` (16000 in `vFuture`) is the right
      cap for WASM programs, or add a separate WASM-specific program size limit.
- [ ] Decide whether resource accounting should carry a dedicated knob (e.g. a
      `WasmLogicSigMaxFuel` / fuel-per-cost rate) instead of reusing
      `LogicSigMaxCost` 1:1 with wasmi fuel.
- [ ] Review the fixed per-invocation resource caps (one 4 MiB linear memory,
      one table of up to 1024 elements, one instance) before release, and decide
      whether they need dedicated consensus parameters. Both initial allocation
      and runtime growth are bounded by these caps. Linear memory is not metered
      by execution fuel, so a minimal guest can declare a large minimum memory
      and force the node to allocate it; 4 MiB per concurrent evaluation is a
      real memory-pressure surface that should be metered or capped by a
      consensus parameter.
- [ ] Pin down the accepted WebAssembly feature set explicitly (currently floats
      off, no SIMD, no memory64, no auto-dispatch). Verify bulk-memory,
      reference-types, multi-value, etc. are intended and deterministic.
- [ ] Decide policy for modules with a `start` function (currently allowed and
      run during execution, skipped during static validation).
- [ ] Cross-check behavior with `rejectOnCurveLogicSigPrograms` and any other
      consensus or policy checks that parse `Lsig.Logic` as AVM bytecode.

## Correctness / policy

- [ ] Extend the algod API on-curve escrow check
      (`daemon/algod/api/server/v2/handlers.go`) to WASM LogicSigs; today it
      parses the program as AVM and silently skips WASM programs.
- [ ] Emit tracer hooks for WASM programs so `simulate` produces traces
      (`logic.EvalTracer` `BeforeProgram`/`AfterProgram` are not called on the
      WASM path).
- [ ] Ensure the `!cgo` / `gofuzz` fallback returns a clear, user-facing error
      through the normal transaction rejection paths, and confirm fuzz builds
      behave as intended.

## Performance

- [ ] Cache the compiled/instantiated wasmi module: verification currently
      compiles and instantiates once for the static check and again for
      evaluation. Consider caching keyed by program hash, with a bounded cache.
- [ ] Reuse an `Engine` across calls instead of building one per evaluation.
- [ ] Benchmark cold start and steady-state execution against the AVM.

## Extensibility

- [ ] Expose opcodes, transaction fields, and other data to guest programs via
      additional `env` host functions (design the ABI and add Go↔Rust callbacks
      as needed; the args-only host was built to extend this way).
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
- [ ] More unit coverage: buffer-too-small and out-of-bounds `arg` writes,
      empty/oversized args near the limits, modules with `start`, modules with
      many functions, modules importing unknown `env` functions, and the
      `EvalMaxArgs`/`ARGS_READ_WORDS` coupling.
- [ ] Determinism tests: same program + args yields the same result and identical
      fuel/cost across runs.
- [ ] Negative tests that a WASM program cannot reach ledger/txn/opcode data
      (no such host functions yet).
- [ ] Test the fuel-exhaustion boundary and the exact-fuel-completion case.

## Docs

- [ ] Note the new build prerequisite and `make wasmlib` in `AGENTS.md` (and any
      CONTRIBUTING/build docs).
- [ ] Document the WASM LogicSig ABI and behavior in the user-facing docs.
