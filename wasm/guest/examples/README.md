# Example guests

Each directory is a Rust crate compiled to `wasm32-unknown-unknown` and
committed as a fixture under `data/transactions/logic/testdata/wasm/`, where
the Go and Rust tests run it through the real executor. `make
wasm-guest-fixtures` rebuilds and copies all of them.

| example | fixture | kind | SDK |
| --- | --- | --- | --- |
| `rust-counter/` | `app_rust_counter.wasm` | application | `avm-sys` |
| `rust-signed-claim/` | `app_rust_signed_claim.wasm` | application | `avm-sys` + `ed25519-dalek` |
| `ed25519-verify/` | `ed25519_dalek.wasm` | LogicSig | `avm-sys` + `ed25519-dalek` |
| `avm-sdk-counter/` | `app_avm_sdk_counter.wasm` | application | `avm-sdk`, allocating |
| `avm-sdk-arc4-counter/` | `app_avm_sdk_arc4_counter.wasm` | ARC-4 application | `avm-sdk` |
| `avm-sdk-arc4-types/` | `app_avm_sdk_arc4_types.wasm` | ARC-4 application | `avm-sdk` |
| `avm-sdk-arc4-resources/` | `app_avm_sdk_arc4_resources.wasm` | ARC-4 references/returns | `avm-sdk` |
| `avm-sdk-amm/` | not copied (local build: `avm_sdk_amm.wasm`) | ARC-4 AMM application | `avm-sdk` |
| `avm-sdk-math/` | `sdk_math.wasm` | application / LogicSig | `avm-sdk` |

- **`rust-counter`** reads a big-endian delta and the sender and updates a
  global counter plus a per-sender total, with fixed buffers and no allocation.
  It floors the size and cost of a compiler-generated application.
- **`rust-signed-claim`** is a bearer voucher: it verifies an ed25519
  signature over the claim amount, enforces one claim per sender, and caps the
  total. Its normal-limit measurements are discussed in `../../../WASM_CONTRACTS.md`.
- **`ed25519-verify`** is a LogicSig that verifies a signature over argument 0
  through the vendored `ed25519-dalek` fork, which calls the host's
  `ed25519verify_bare`.
- **`avm-sdk-counter`** is `rust-counter` rewritten with `avm-sdk`: a
  `#[contract]` struct with `GlobalUint` state, building the per-sender key in
  a `Vec` to exercise the bump allocator.
- **`avm-sdk-arc4-counter`** routes `increment(uint64)void` and
  `set_label(string)void` by selector, without allocating.
- **`avm-sdk-arc4-types`** takes dynamic arrays, nested tuples with packed
  bools, and 17 parameters (packed past the 15th), to exercise every
  ordinary value parameter type the router supports.
- **`avm-sdk-arc4-resources`** resolves asset/account/application references,
  validates a preceding asset transfer, and returns its amount in an ARC-4 log.
  It is a routing fixture, not a deposit-safe AMM implementation. With a 32 KiB
  stack it is 5,119 bytes and uses 6,691 budget units (10 pooled app calls) for
  the successful Go integration case. Log host pricing is provisional. Run
  `go test ./data/transactions/logic -run TestWasmARC4ResourceArgumentsAndReturns -v`.

## Size and cost

`avm-sdk-math/` exercises host-backed `mulw`, `divmodw`, `sqrt`, and the
128-bit `sqrtw` extension. Its compiled Rust fixture runs in both modes:
`cargo test -p avm-sdk-runner --test run_math` from `wasm/guest`, or
`go test ./data/transactions/logic -run '^TestWasmSDKMath$' -v`.

`avm-sdk-amm/` is a standalone Rust port of TEALScript's constant-product AMM,
with corrected initial LP issuance and swap direction. See its [README](avm-sdk-amm/README.md)
for the ABI, tests, and execution-budget limitations. Unlike the fixtures above,
it is built explicitly with `cargo build -p avm-sdk-amm --release --target wasm32-unknown-unknown`
from `wasm/guest`; it is not copied by `make wasm-guest-fixtures`.

Costs are AVM budget units (10 wasmi fuel each). An application call has a
700-unit budget, pooled across the app calls in its group, so a 16-call group
has 11,200; a LogicSig has 20,000 on its own. `setup` is the deterministic
charge for loading the module, paid on every evaluation before the program
runs; `execution` is what the program itself uses; `total` is the whole
evaluation of one successful call. `calls` is how many app calls the group
needs to pool enough budget for that total.

| fixture | call | bytes | memory pages | setup | execution | total | calls |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `app_rust_counter.wasm` | | 560 | 17 | 1,652 | 423 | 2,075 | 3 |
| `app_rust_signed_claim.wasm` | | 4,522 | 17 | 4,506 | 4,109 | 8,615 | 13 |
| `ed25519_dalek.wasm` | LogicSig | 4,761 | 17 | 4,546 | 3,811 | 8,357 | |
| `app_avm_sdk_counter.wasm` | | 5,242 | 17 | 5,165 | 1,292 | 6,457 | 10 |
| `app_avm_sdk_arc4_counter.wasm` | `increment` | 1,589 | 1 | 2,016 | 816 | 2,832 | 5 |
| | `set_label` | | | | 992 | 3,008 | 5 |
| | bare create | | | | 284 | 2,300 | 4 |
| `app_avm_sdk_arc4_types.wasm` | `sum` | 5,797 | 1 | 4,997 | 1,370 | 6,367 | 10 |
| | `join` | | | | 2,184 | 7,181 | 11 |
| | `record` | | | | 3,677 | 8,674 | 13 |
| | `wide` | | | | 2,949 | 7,946 | 12 |
| `avm_sdk_amm.wasm` (local build) | | 11,580 | 1 | — | — | — | — |
| `sdk_math.wasm` | | 722 | 1 | — | — | — | — |

AMM and math fixture costs have not yet been measured; dashes indicate unavailable
measurements. Host-backed wide arithmetic and square roots reduced the AMM from
13,103 to 11,580 bytes, without narrowing arithmetic or changing rounding.

The signed-claim and LogicSig executions each include the 2,850-unit
`ed25519verify_bare` host charge. The LogicSig runs under its own 20,000-unit
budget rather than a pool, so it needs no extra calls.

What the table shows:

- **Setup dominates.** Setup charges 6 fuel per module byte and 300 fuel per
  page of initial memory, and wasmi charges another 7 fuel per byte of each
  function the first time it runs. Setup is 52 to 80 percent of every call
  here; the low end is the signed claim, whose signature check is a 2,850-unit
  host charge.
- **The default stack costs 510 units per call.** Rust links a 1 MiB stack,
  which is 16 of the 17 initial pages. The two ARC-4 examples link a 32 KiB
  stack from their `build.rs`, so they start with one page; that is why the
  ARC-4 counter's setup is 2,016 at 1,589 bytes while the `avm-sys` counter's
  is 1,652 at 560 bytes.
- **Allocation is expensive, mostly through panics.** `avm-sdk-counter` is
  the same program as `rust-counter` plus a `Vec`. The bump allocator and
  `alloc`'s growth path are about 1 KB of the extra 4.7 KB; the rest is core's
  integer formatting and panic machinery, linked by the `Vec`'s
  capacity-overflow and allocation-error paths even though the panic handler
  ignores the message.
- **Parameter types are cheap, methods are not free.** ARC-4 validation is
  driven by 16-byte descriptors and a few shared checkers, so the types
  example pays for tuples, arrays, and strings once. The remaining size is the
  router, one entry function per method, and the method bodies.

## Reproducing the numbers

```bash
# Rebuild every fixture from the examples.
make wasm-guest-fixtures

# Bytes and initial memory pages.
ls -l data/transactions/logic/testdata/wasm/
wasm-tools print data/transactions/logic/testdata/wasm/app_rust_counter.wasm | grep '(memory'

# Setup fuel for each fixture (divide by 10 for units).
make wasm-setupcost

# Call cost and app calls needed, under the normal 16-call pool.
WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmRustContractBudget$' -v -count=1

# Per-method costs of the ARC-4 fixtures and the LogicSig's cost, logged by
# their tests.
go test ./data/transactions/logic -run 'TestWasmAppArc4|TestWasmEd25519DalekGuest' -v -count=1
```

Sizes are from `cargo build --release` with the workspace profile in
`../Cargo.toml` (`opt-level = "z"`, LTO, one codegen unit, `panic = "abort"`,
stripped), Rust 1.97.1.
