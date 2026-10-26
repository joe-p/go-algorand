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
  parameter type the router supports.

## Size and cost

Costs are AVM budget units (10 wasmi fuel each). An application call has a
700-unit budget, pooled across the app calls in its group, so a 16-call group
has 11,200; a LogicSig has 20,000 on its own. `setup` is the deterministic
charge for loading the module, paid on every evaluation before the program
runs; `execution` is what the program itself uses; `total` is the whole
evaluation of one successful call. `calls` is how many app calls the group
needs to pool enough budget for that total.

| fixture | call | bytes | memory pages | setup | execution | total | calls |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `app_rust_counter.wasm` | | 560 | 17 | 1,832 | 423 | 2,255 | 4 |
| `app_rust_signed_claim.wasm` | | 4,522 | 17 | 4,821 | 4,109 | 8,930 | 13 |
| `ed25519_dalek.wasm` | LogicSig | 4,761 | 17 | 4,816 | 3,811 | 8,627 | |
| `app_avm_sdk_counter.wasm` | | 5,242 | 17 | 5,495 | 1,292 | 6,787 | 10 |
| `app_avm_sdk_arc4_counter.wasm` | `increment` | 1,589 | 1 | 2,176 | 816 | 2,992 | 5 |
| | `set_label` | | | | 992 | 3,168 | 5 |
| | bare create | | | | 284 | 2,460 | 4 |
| `app_avm_sdk_arc4_types.wasm` | `sum` | 5,797 | 1 | 5,217 | 1,370 | 6,587 | 10 |
| | `join` | | | | 2,184 | 7,401 | 11 |
| | `record` | | | | 3,677 | 8,894 | 13 |
| | `wide` | | | | 2,949 | 8,166 | 12 |

The signed-claim and LogicSig executions each include the 2,850-unit
`ed25519verify_bare` host charge. The LogicSig runs under its own 20,000-unit
budget rather than a pool, so it needs no extra calls.

What the table shows:

- **Setup dominates.** Setup charges 6 fuel per module byte and 350 fuel per
  page of initial memory, and wasmi charges another 7 fuel per byte of each
  function the first time it runs. Setup is 54 to 81 percent of every call
  here; the low end is the signed claim, whose signature check is a 2,850-unit
  host charge.
- **The default stack costs 595 units per call.** Rust links a 1 MiB stack,
  which is 16 of the 17 initial pages. The two ARC-4 examples link a 32 KiB
  stack from their `build.rs`, so they start with one page; that is why the
  ARC-4 counter's setup is 2,176 at 1,589 bytes while the `avm-sys` counter's
  is 1,832 at 560 bytes.
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
