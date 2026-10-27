# Constant-product AMM

An allocation-free Rust/`avm-sdk` port of the TEALScript
[`ConstantProductAMM` example](https://github.com/algorandfoundation/TEALScript/blob/dev/examples/amm/amm.algo.ts).
This is an experimental example, **not an audited production AMM**.

## Build and test

From `wasm/guest`:

```sh
cargo test -p avm-sdk-amm
cargo build --release -p avm-sdk-amm --target wasm32-unknown-unknown
```

The output is `target/wasm32-unknown-unknown/release/avm_sdk_amm.wasm`.
It exports the SDK's `program_entry` and uses a 32 KiB stack. Native tests
exercise ARC-4 routing, state, deposit validation, and inner transactions with
a mock host; they do not model protocol budgets, fee pooling, minimum balances,
resource availability, or atomic rollback.

The stripped release module is **16,068 bytes**, or **11,346 bytes** after
Binaryen 133 `wasm-opt -Oz` post-processing. The owned-unit-name-only version
measured 16,107 / 11,343 bytes; adding the owned governor read and SDK address
helpers changes that by -39 / +3 bytes. Initial memory is one page; bootstrap grows it to
two 64 KiB pages. Moving ownership from the SDK wrapper to the `Avm` trait
produced byte-for-byte identical raw and optimized modules. See the
[comparison and reproduction commands](../README.md#size-and-cost).

The contract imports SDK trap-only `assert!`, `assert_eq!`, and `assert_ne!`
and enables the SDK's `alloc` feature and `#[contract(alloc)]`. Asset unit names
are owned `Vec<u8>` values; the pool name is built with `.concat()` instead of
manual buffer offsets/copies. The guest still uses host `mulw`, `divmodw`, and `sqrtw`, rather than
bundling software wide arithmetic. Both SDK-assert builds fit the executor's
16,384-byte size cap; normal-budget deployability remains unverified.

Governor reads use `GlobalBytes::try_read_owned()` (one host call with a 4 KiB
allocation whose capacity is retained for the invocation). Group address reads
use `Transaction::address(field)` without allocation or contract-local buffer
handling. The underlying `Avm` methods remain buffer-based.

Failed assertions trap on WASM rather than returning an ABI method error;
the SDK macros do not format operands or accept custom message arguments.
Native rejection tests invoke methods directly and catch panics, because
unwinding through the generated `extern "C"` entry point would abort the process.

## Interface

Create with a bare NoOp call to record the creator as governor. Allocate four
global uints and one global byte slice. The state keys match the source:
`g` (governor), `a`/`b` (underlying ASAs), `p` (LP ASA), `r` (reserve A/B × 1000).
Updates, deletes, opt-ins, and other bare calls are rejected.

| ARC-4 signature | Preceding transactions | Inner transactions |
| --- | --- | --- |
| `set_governor(address)void` | none | none |
| `bootstrap(pay,asset,asset)uint64` | seed payment | LP creation, two ASA opt-ins |
| `mint(axfer,axfer,asset,asset,asset)void` | A deposit, B deposit | LP transfer |
| `burn(axfer,asset,asset,asset)void` | LP deposit | A transfer, B transfer |
| `swap(axfer,asset,asset)void` | A or B deposit | opposite ASA transfer |

Asset arguments are ARC-4 foreign-asset **indices**, not raw IDs. Mint and burn
take assets in LP/A/B order; bootstrap and swap take A/B. Transaction arguments
consume the immediately preceding group transactions, not application arguments.
Only the governor may bootstrap or change the governor. A and B must be distinct,
nonzero ASAs ordered by ID; native Algo pools are not supported.

Supply is 10,000,000,000 LP units with three decimals; swap fees are 5/1000
(0.5%). Initial LP issuance is floor(sqrt(A × B)); later issuance preserves the
source's two-stage rounding at 1000 precision. Excess unbalanced deposits are
not refunded. Burns return each reserve pro rata. The ratio is updated after
swaps and burns, as in the source (not after mints).

## Intentional differences from the source

- Initial mint actually transfers the calculated LP tokens to the depositor.
- Swap pays the **opposite** asset, not the input asset.
- Bootstrap is one-shot, disallows Algo ID 0, and rejects payment close/rekey fields.
- Deposits reject clawbacks, asset close-outs, and rekeys, and verify sender,
  receiver, asset ID, and nonzero amount.
- Initial mint requires zero previously issued LP and zero pre-deposit reserves;
  donations into an uninitialized or fully withdrawn pool prevent reinitialization.
- A complete burn sets the ratio to zero rather than dividing by zero.
- Products use host-backed `mulw`/`divmodw` with checked conversion/division. Swap's
  denominator retains the source's checked uint64 arithmetic; oversized inputs
  fail instead of wrapping. Initial square root uses the WASM `sqrtw` extension
  to support the full u64 × u64 product.
- Inner transactions explicitly pay zero fees, relying on outer fee credit.

## Execution requirements and limitations

The application needs its base Algo minimum balance **plus** minimum balances
for creating the LP ASA and holding both underlying ASAs. The source's seed
check of at least 300,000 microAlgos is retained; that amount alone does not
fund a fresh application's base minimum balance. Pre-fund or seed sufficiently.
Callers must opt into assets they receive, supply available assets/accounts,
and cover all inner transaction fees through pooled outer fees.

This repository's WASM executor charges setup and instruction fuel against AVM
budget. Normal-budget deployability has **not** been established for this
example. In particular, bootstrap retains the source's exact two-transaction
group requirement, so it cannot add app calls to pool budget. An execution
environment with sufficient budget is required; compiling and passing native
tests do not demonstrate that a mainnet-budget call will succeed.

Like the source, the interface has no minimum-output or deadline parameters,
no locked minimum liquidity, no donation recovery, and no protection against
underlying asset freeze/clawback authorities. Do not use with real funds without
addressing those limitations and independently reviewing the implementation.
