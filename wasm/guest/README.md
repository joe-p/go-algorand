# WASM guest SDK

Guest-side Rust crates for authoring WASM programs (LogicSigs and application
programs). They are compiled to `wasm32-unknown-unknown`; the host executor that
runs them is the sibling `wasmi` crate in `../` (see `../README.md`).

This is a separate Cargo workspace from the host executor so `make wasmlib`
(`cargo build --manifest-path wasm/Cargo.toml`) is unaffected.

## Crates

- **`avm-sys/`** (`avm-sys`) — unopinionated, 1:1 Rust declarations for the
  `env` host ABI. It has no policy, allocator, or entry-point abstractions.
  Most programs depend on `avm-sdk` only, which wraps these behind a mockable
  host.

  | `env` import | binding |
  |---|---|
  | `arg_count() -> i32` | `arg_count() -> u32` |
  | `arg(i, out, len) -> i32` | `arg_into(i, &mut [u8]) -> Result<&[u8], ArgError>` |
  | `txn_uint(field) -> i64` | `txn_uint(TxnField) -> i64` |
  | `txn_bytes(field, out, len) -> i32` | `txn_bytes(TxnField, &mut [u8]) -> Result<usize, TxnError>` |
  | `txna_uint(field, index) -> i64` | `txna_uint(TxnField, u32) -> i64` |
   | `txna_bytes(field, index, out, len) -> i32` | `txna_bytes(TxnField, u32, &mut [u8]) -> Result<usize, TxnError>` |
   | `gtxn_uint(group_index, field) -> i64` | `gtxn_uint(u32, TxnField) -> u64` |
   | `gtxn_bytes(group_index, field, out, len) -> i32` | `gtxn_bytes(u32, TxnField, &mut [u8]) -> Result<usize, TxnError>` |
   | `global_uint(field) -> i64` | `global_uint(GlobalField) -> u64` |
   | `global_bytes(field, out, len) -> i32` | `global_bytes(GlobalField, &mut [u8]) -> Result<usize, TxnError>` |
   | `asset_balance(account, asset, out) -> i32` | `asset_balance(&[u8; 32], u64) -> Option<u64>` |
   | `asset_unit_name(asset, out, len) -> i32` | `asset_unit_name(u64, &mut [u8]) -> Result<Option<usize>, TxnError>` |
  | `app_global_get_uint(key, key_len) -> i64` | `app_global_get_uint(&[u8]) -> u64` |
  | `app_global_get_bytes(key, key_len, out, len) -> i32` | `app_global_get_bytes(&[u8], &mut [u8]) -> Result<&[u8], TxnError>` |
  | `app_global_put_uint(key, key_len, value)` | `app_global_put_uint(&[u8], u64)` |
  | `app_global_put_bytes(key, key_len, val, val_len)` | `app_global_put_bytes(&[u8], &[u8])` |
  | `app_global_del(key, key_len)` | `app_global_del(&[u8])` |
  | `ed25519verify_bare(...) -> i32` | `ed25519_verify_bare(&[u8], &[u8; 64], &[u8; 32]) -> bool` |

  `arg`/`arg_count` are signature-only, as in the AVM: they trap in an
  application program. An application reads its arguments with
  `txna_bytes(TxnField::ApplicationArgs, i, buf)` (or the `avm-sdk` helpers
  `txn::app_args_count` / `txn::app_arg_into`). The `app_global_*` functions are
  application-only and trap in a LogicSig.

  Cases the host *traps* on (an invalid transaction field, an oversized value)
  are not `Result`; a trap ends the guest, as in the AVM.

- **`avm-sdk/`** (`avm-sdk`) — the guest SDK. It depends on `avm-sys` and adds
  host operations grouped into `arg`, `txn`, and `global` modules, typed
  transaction helpers (`txn::sender`, `txn::application_id`, application
  arguments), `const`-constructible `GlobalUint` / `GlobalBytes` handles, the
  `#[contract]` and `#[program_entry]` attribute macros, and the lower-level
  `entry!` macro. It is `no_std` on wasm and does not allocate unless the guest
  opts in with `alloc` (`#[contract(alloc)]` on the impl, or
  `#[program_entry(alloc)]`), which installs a guest-side bump allocator so
  contracts can use `Vec`/`String`; see [Allocation](#allocation).

   Every SDK helper reaches the host through `avm_sdk::host()`. On wasm that is
  the concrete `HostAvm`, so calls compile to direct imports. On native builds
  it is a mock installed with `avm_sdk::testing::set_host(...)`, so the same
   program code runs in unit tests.

   Group scalar reads are `txn::group_uint` / `txn::group_bytes`. Context
   fields live in **`globals`**, separately from application state in `global`:
   `globals::group_size()`, `globals::application_id()` (assigned ID, even at
   creation), and `globals::application_address()`. Only those three context
   fields are currently exposed. Byte reads accept an empty buffer to query
   length without copying. Group array reads remain
   unavailable. Native mocks can override the four new `Avm` methods; their
   defaults panic so an unmodeled read cannot silently pass.

   Application-only inner assets use `itxn::AssetCreation { .. }.submit()`
   (returns the created ID), `itxn::asset_opt_in(asset_id)`, and
   `itxn::asset_transfer(asset_id, &receiver, amount)`. These helpers set Fee=0
   and require outer fee credit. Low-level `begin`, `next`, `set_uint`,
   `set_bytes`, `submit`, and `created_asset_id` mirror the AVM asset transaction
   lifecycle and defaults. Protocol/ledger failures trap; rollback is handled
   atomically by the outer transaction group. Inner app calls are not supported,
   and inner host fuel charges are provisional pending calibration.

   Application-only asset queries are `asset::balance(&address, asset_id)` and
   `asset::unit_name_into(asset_id, &mut buffer)`. They distinguish missing resources
   (`None`) from existing zero balances/empty names (`Some(0)`). An empty name
   buffer queries the required length. Missing holdings/assets are not the same
   as unavailable resources: missing returns `None`, but AVM resource-access
   failures trap, including unavailable account/asset holding pairs under group
   sharing. Both methods are mockable through `Avm`; their defaults panic.

   With the SDK's Cargo feature `alloc`, `asset::unit_name(asset_id)` returns
   `Result<Option<Vec<u8>>, TxnError>` instead. It allocates eight bytes (the
   protocol maximum), calls the host once, validates the returned length, and
   truncates to the actual name. Missing assets return `None`; existing empty
   names return `Some(vec![])`. Names need not be UTF-8. Install an allocator
   with `#[contract(alloc)]` on the impl or `#[program_entry(alloc)]`. The AMM
   example demonstrates owned names and `.concat()` without manual buffers.
   Both query APIs use the buffer-based `Avm::asset_unit_name(asset_id, buffer)`
   primitive. The public owned wrapper manages allocation and validates the
   returned length; `HostAvm` and native mocks only implement the buffer read.

   The `alloc` feature also provides owned byte readers (all return
   `Result<Vec<u8>, ...>`; bytes need not be UTF-8):

   | Owned API | Existing buffer-based API |
   | --- | --- |
   | `GlobalBytes::try_read_owned()` / `read_owned()` | `try_read(buf)` / `read(buf)` |
   | `global::get_bytes_owned(key)` | `global::get_bytes(key, buf)` |
   | `txn::app_arg(index)` | `txn::app_arg_into(index, buf)` |
   | `txn::bytes_owned(field)` | `txn::bytes(field, buf)` |
   | `txn::array_bytes_owned(field, index)` | `txn::array_bytes(field, index, buf)` |
   | `txn::group_bytes_owned(index, field)` | `txn::group_bytes(index, field, buf)` |
   | `Transaction::bytes_owned(field)` | `Transaction::bytes(field, buf)` |
   | `arg::get_owned(index)` | `arg::get(index, buf)` |

   `GlobalBytes::read_owned()` traps on errors rather than returning `Result`.
   Missing global-state keys read as empty. LogicSig arguments remain
   signature-only; state and application-argument operations retain their mode
   restrictions. All these wrappers share bounded, safe buffer handling and
   propagate host errors without introducing lossy UTF-8 conversion.

   These general owned readers allocate **4 KiB per call**, make one host read,
   then truncate the vector's length without shrinking capacity. This avoids
   length-query calls (the slice-returning state/LogicSig APIs cannot express a
   length-only read), but has a memory/zeroing cost even for short values. The
   bump allocator does not reclaim that capacity until the invocation ends;
   use existing buffer APIs in allocation-heavy loops or for tighter memory
   control. `asset::unit_name` retains its specialized eight-byte allocation.

   Fixed-size reads need no allocator: `txn::address(field)`,
   `txn::array_address(field, index)`, `txn::group_address(index, field)`, and
   `Transaction::address(field)` return `Result<[u8; 32], Error>`. They check
   the returned length and propagate read errors. Select an address-valued field;
   these helpers validate length, not field semantics. The AMM uses
   `seed.address(TxnField::Receiver)` and `governor.try_read_owned()` instead of
   local buffer-management helpers.

   Both modes can use host-backed arithmetic through `math::mulw(a, b)`
   (returns high/low product words), `math::divmodw(a_hi, a_lo, b_hi, b_lo)`
   (returns quotient high/low and remainder high/low), and `math::sqrt(n)`.
   These match unsigned AVM opcode semantics; division by zero traps.
   `math::sqrtw(high, low)` is a WASM extension for the floor square root of
   a full 128-bit operand. Native mocks must override the corresponding `Avm`
   methods. These helpers avoid embedding software wide arithmetic in guests;
   their fixed host fuel weights are 15/25/10/25 respectively, calibrated on
   linux/arm64 toward 1 ns/fuel (see `../../WASM_PERF.md`).

   Optional trap-only assertions can be imported with
   `use avm_sdk::{assert, assert_eq, assert_ne};`. They preserve the familiar
   assertion syntax (including optional trailing commas), evaluate operands
   once, and borrow comparison operands. They do not require `Debug` or support
   custom message arguments. Failure calls `avm_sdk::trap()` (a WASM trap or a
   generic native panic), avoiding assertion formatting code in guests. Normal
   Rust indexing/copy operations still have their own bounds-panic paths. Use
   `core::assert!` / `core::assert_eq!` / `core::assert_ne!` when formatted panic
   diagnostics are wanted instead.

  A contract is usually a struct of state fields plus an `impl` block, both
  marked `#[contract]`. On the struct, each field's key is its name
  (`#[key = "..."]` overrides it) and the macro computes the global-state schema
  as `<Counter as avm_sdk::Contract>::GLOBAL_UINTS` / `GLOBAL_BYTES`. On the
  impl, it exports `program`, which calls `Counter::new().approve()`, and marks
  `approve` `#[inline(always)]` so every state key folds to a constant. The impl
  is also where ARC-4 method dispatch will be generated.

  ```rust
  #[contract]
  pub struct Counter {
      count: GlobalUint,
  }

  #[contract]
  impl Counter {
      pub fn approve(&self) -> Result<(), Error> {
          if txn::application_id() == 0 {
              return Ok(());
          }
          self.count.set(self.count.get() + 1);
          Ok(())
      }
  }

  #[test]
  fn increments() {
      let mock = avm_sdk::testing::set_host(MockAvm::default());
      assert!(Counter::new().approve().is_ok());
      // `mock` derefs to the MockAvm; dropping it restores the previous host.
  }
  ```

  Mark methods `#[abimethod]` to make the contract an ARC-4 application:

  ```rust
  #[contract]
  impl Counter {
      #[abimethod]
      pub fn increment(&self, delta: u64) -> Result<(), Error> { ... }

      #[abimethod]
      pub fn set_label(&self, label: &str) { ... }
  }
  ```

  The router matches application argument 0 against each method's selector
  (`increment(uint64)void`), requires `NoOp` and exactly one ABI-encoded
  argument per parameter (parameters 15 onward arrive packed in a tuple in
  argument 15, as ARC-4 specifies), and decodes them strictly. Methods run on
  an existing app, or only at creation with `#[abimethod(create)]`. Bare calls
  go to `approve` if there is one; otherwise only a plain create is approved.
  Methods return `()`, an ABI value, or `Result<T, E>`. Successful non-void
  calls log `0x151f7c75` followed by the ABI encoding; errors reject without a
  return log. A returned `false` is a successful ABI bool, not rejection.
  Encoding is allocation-free and bounded to 1020 payload bytes; oversized
  returns reject. Earlier logs consume the same 1024-byte protocol allowance.
  `avm_sdk::log(&bytes)` also exposes raw application logging.
  `<Counter as avm_sdk::arc4::Arc4Contract>::METHODS` lists the signatures and
  selectors for tooling.

  Parameter types, all decoded without allocating (dynamic values borrow from
  one 2 KiB argument buffer):

  | ABI type | Rust type |
  |---|---|
  | `uint8` … `uint128` | `u8` … `u128` |
  | `uintN`, `ufixedNxM` | `arc4::Uint<N / 8>` (`U256`, `U512`), `arc4::UFixed<N / 8, M>` |
  | `bool`, `byte`, `address` | `bool`, `arc4::Byte`, `arc4::Address` |
  | `byte[N]`, `T[N]` | `[u8; N]`, `[T; N]` |
  | `byte[]`, `string` | `&[u8]`, `&str` |
  | `T[]` | `arc4::DynArray<T>` |
  | `(T1,T2,…)` | Rust tuples, up to 16 elements |

  Top-level `arc4::Account`, `Asset`, and `Application` consume uint8 indices
  into the call's foreign arrays. Account/application index 0 resolves to
  sender/current transaction ApplicationID (0 at creation); asset index 0 is
  the first foreign asset. Values passed to methods contain resolved addresses
  or IDs, not indices. Invalid indices reject before running the method.
  `Transaction`, `PaymentTxn`, `KeyRegistrationTxn`, `AssetConfigTxn`,
  `AssetTransferTxn`, `AssetFreezeTxn`, and `ApplicationCallTxn` consume no
  application argument. They select immediately preceding group transactions
  in signature order and validate types; `.group_index()`, `.uint(field)`, and
  `.bytes(field, &mut buf)` access them. Only encoded parameters count toward
  ARC-4 tuple packing. References and transactions cannot be nested or returned.

  **Cost.** Module size dominates a call's cost: setup charges 6 fuel per
  module byte, and wasmi charges another 7 fuel per byte of each function the
  first time it runs. The router therefore gives each method its own entry
  function, so a call only compiles the method it invokes. Two habits keep
  contracts small:

  - Link with a small stack. Rust's default 1 MiB stack is 16 pages of initial
    memory at 350 fuel each; the ARC-4 examples' `build.rs` sets 32 KiB.
  - Avoid operations that can panic (`slice[a..b]`, `slice[i]`,
    `copy_from_slice`). A panic path links about 1 KB of core's formatting code
    even though the panic handler ignores the message. Use `get` and trap or
    return an error instead, as the SDK does.

  Argument decoding is built to the same rule. Each ARC-4 type has a `const`
  `arc4::Desc` describing its shape, and the router validates every argument
  with the handful of shared checkers those descriptors point to, so a new
  parameter type costs 16 bytes of data rather than a generic copy of the
  validator, and a contract links only the checkers its types use. The
  method entry then reads the validated arguments without any checks.

  `#[program_entry]` is the same export for a free function, for programs
  without declared state.

  The SDK does not re-export the raw `avm-sys` functions, because they bypass
  the mock. Programs that want the raw ABI depend on `avm-sys` directly, as the
  `rust-counter`, `rust-signed-claim`, and `ed25519-verify` examples do.

- **`avm-sdk-macros/`** (`avm-sdk-macros`) — the `#[contract]` and
  `#[program_entry]` proc macros, re-exported by `avm-sdk`. Each generates
  `program() -> i32` and, with the `alloc` argument, the `#[global_allocator]`
  static.

- **`ed25519-dalek/`** — a vendored fork of `ed25519-dalek` 2.2.0 (upstream is
  BSD-3-Clause; see its `LICENSE`). It keeps the crate name and API, and is
  selected with `[patch.crates-io]` in the workspace root, so a guest depends on
  `ed25519-dalek` normally. On `wasm32`, `VerifyingKey::verify` and
  `verify_strict` call `avm_sys::ed25519_verify_bare` (the
  `env.ed25519verify_bare` host function) instead of doing curve arithmetic in
  the guest, and `VerifyingKey::from_bytes` skips
  point decompression because the host validates the key. On native targets it
  behaves like upstream, so the fork can still be built and tested off-chain.

  The switch means on-chain verification matches the AVM's opcode exactly (Go's
  `crypto/ed25519` semantics, *not* dalek's cofactored/small-order rules) and
  bills a calibrated host charge (28,500 fuel = 2850 AVM budget units) rather than
  guest instructions.

  The diff against upstream 2.2.0 is `ed25519-dalek-host.patch` (only
  `src/verifying.rs` and `src/errors.rs`); apply it from the crate root with
  `patch -p1 < ../ed25519-dalek-host.patch` or
  `git apply ../ed25519-dalek-host.patch`. The crate's `Cargo.toml` is maintained
  separately.

- **`examples/ed25519-verify/`** — an example guest that reads
  `(data, signature, public key)` from arguments and verifies with the
  `ed25519-dalek` API, exercising the patch.

- **`examples/rust-counter/`** — a baseline compiler-generated application
  program: reads a big-endian delta and the sender, and updates global state. It
  floors the size, setup, and budget pooling of a real Rust app.

- **`examples/rust-signed-claim/`** — a representative signature-authorized
  application: verifies an ed25519 signature over the amount with the patched
  `ed25519-dalek` API, enforces per-sender replay protection, and updates global
  state. Its normal-limit size and budget measurements are in
  `../../WASM_CONTRACTS.md`.

- **`examples/avm-sdk-counter/`** — `rust-counter` rewritten with `avm-sdk`.
  It is a `#[contract]` struct with a `GlobalUint` counter, uses the
  transaction helpers for the sender and application arguments, and builds the
  per-sender key in a `Vec` to exercise the optional bump allocator. The Rust
  runner executes its committed fixture with stub transaction and global-state
  callbacks.

- **`examples/avm-sdk-arc4-counter/`** — an ARC-4 application with
  `increment(uint64)void` and `set_label(string)void`, routed by `#[contract]`.
  It does not allocate. The Go test `TestWasmAppArc4Counter` calls its committed
  fixture with arguments encoded by the `avm-abi` package.

- **`examples/avm-sdk-arc4-types/`** — ARC-4 methods taking dynamic arrays,
  nested tuples with packed bools, and 17 parameters (packed past the 15th).
  The Go test `TestWasmAppArc4Types` calls its fixture with `avm-abi`
  encodings.

  `examples/README.md` tabulates every example's size, initial memory, setup
  cost, and call cost.

- **`runner/`** — host-side tests that run the committed guest `.wasm` fixtures
  through the real executor with a stub `ed25519verify_bare`.

## Example

```rust
#![cfg_attr(target_arch = "wasm32", no_std)]
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use avm_sdk::{arg::get as arg_into, entry};

entry!(|| {
    let mut data = [0u8; 4096];
    let mut sig = [0u8; 64];
    let mut pk = [0u8; 32];
    let data_len = match arg_into(0, &mut data) { Ok(d) => d.len(), Err(_) => return false };
    if arg_into(1, &mut sig).map(<[u8]>::len) != Ok(64) { return false; }
    if arg_into(2, &mut pk).map(<[u8]>::len) != Ok(32) { return false; }
    let Ok(key) = VerifyingKey::from_bytes(&pk) else { return false };
    key.verify(&data[..data_len], &Signature::from_bytes(&sig)).is_ok()
});
```

## Building and testing

```bash
# Rust unit/integration tests (host target)
cargo test --manifest-path wasm/guest/Cargo.toml

# Build the guest fixtures used by the Rust and Go tests
make wasm-guest-fixtures
```

`make wasm-guest-fixtures` builds the example guests for
`wasm32-unknown-unknown` and copies the results to
`data/transactions/logic/testdata/wasm/` (`ed25519_dalek.wasm`,
`app_rust_counter.wasm`, `app_rust_signed_claim.wasm`,
`app_avm_sdk_counter.wasm`, `app_avm_sdk_arc4_counter.wasm`, and
`app_avm_sdk_arc4_types.wasm`). The Go unit and end-to-end tests load the first
three and the two ARC-4 fixtures; the Rust runner loads the `avm-sdk` counter.

## Resource limits

The example fits the current limits: ~4.7 KiB of program (the program byte cap
is `MaxAbsoluteLogicSigProgramSize`, 16000 in `vFuture`). The LSig cost budget is
`LogicSigMaxCost`, 20,000, converted to 200,000 fuel at 10 fuel/cost unit. The guest linear memory cap
is 4 MiB.

These limits are tighter for application programs. The compiler-generated
`app_rust_signed_claim.wasm` is 4,522 bytes (three pages) and has a 4,506
deterministic setup cost, so even deploying it needs a pooled group; see
`../../WASM_CONTRACTS.md`.

## Allocation

The host ABI copies into caller-provided buffers and supports length queries
(`len == 0`), so guests do not need an allocator. `avm-sdk` is no-alloc by
default.

When a program benefits from `Vec`/`String`, opt in with `#[contract(alloc)]`
on the contract impl, or `#[program_entry(alloc)]`. That installs [`BumpAllocator`](../avm-sdk/src/alloc.rs)
as the crate's `#[global_allocator]` on `wasm32`. It is a bump-pointer arena:

- The heap starts at the current linear-memory size on the first allocation.
  A Rust `wasm32` module's initial memory already covers static data and the
  shadow stack, so that region is free.
- Memory grows by whole 64 KiB pages with `memory.grow`; growth is bounded by the
  executor's 4 MiB linear-memory cap.
- `dealloc` is a no-op. Each invocation runs in a fresh VM instance that is
  destroyed afterwards, so there is nothing to reclaim during the call, and an
  allocation-heavy loop can exhaust the cap. Prefer fixed buffers or `heapless`
  when allocation is not necessary.

This is the same model `soroban-sdk` uses for its optional `alloc` feature. It
adds no host imports, so the ABI and the executor are unchanged.
