# WASM guest SDK

Guest-side Rust crates for authoring WASM programs (LogicSigs and application
programs). They are compiled to `wasm32-unknown-unknown`; the host executor that
runs them is the sibling `wasmi` crate in `../` (see `../README.md`).

This is a separate Cargo workspace from the host executor so `make wasmlib`
(`cargo build --manifest-path wasm/Cargo.toml`) is unaffected.

## Crates

- **`avm-sys/`** (`avm-sys`) — unopinionated, 1:1 Rust declarations for the
  `env` host ABI. It has no policy, allocator, or entry-point abstractions.
  `avm-sdk` re-exports these items; most programs depend on `avm-sdk` only.

  | `env` import | binding |
  |---|---|
  | `arg_count() -> i32` | `arg_count() -> u32` |
  | `arg(i, out, len) -> i32` | `arg_into(i, &mut [u8]) -> Result<&[u8], ArgError>` |
  | `txn_uint(field) -> i64` | `txn_uint(TxnField) -> i64` |
  | `txn_bytes(field, out, len) -> i32` | `txn_bytes(TxnField, &mut [u8]) -> Result<usize, TxnError>` |
  | `txna_uint(field, index) -> i64` | `txna_uint(TxnField, u32) -> i64` |
  | `txna_bytes(field, index, out, len) -> i32` | `txna_bytes(TxnField, u32, &mut [u8]) -> Result<usize, TxnError>` |
  | `app_global_get_uint(key, key_len) -> i64` | `app_global_get_uint(&[u8]) -> u64` |
  | `app_global_get_bytes(key, key_len, out, len) -> i32` | `app_global_get_bytes(&[u8], &mut [u8]) -> Result<&[u8], TxnError>` |
  | `app_global_put_uint(key, key_len, value)` | `app_global_put_uint(&[u8], u64)` |
  | `app_global_put_bytes(key, key_len, val, val_len)` | `app_global_put_bytes(&[u8], &[u8])` |
  | `app_global_del(key, key_len)` | `app_global_del(&[u8])` |
  | `ed25519verify_bare(...) -> i32` | `ed25519_verify_bare(&[u8], &[u8; 64], &[u8; 32]) -> bool` |

  `arg`/`arg_count` are signature-only, as in the AVM: they trap in an
  application program. An application reads its arguments with
  `txna_bytes(TxnField::ApplicationArgs, i, buf)` (or the `avm-sdk` helpers
  `app_args_count` / `app_arg_into`). The `app_global_*` functions are
  application-only and trap in a LogicSig.

  Cases the host *traps* on (an invalid transaction field, an oversized value)
  are not `Result`; a trap ends the guest, as in the AVM.

- **`avm-sdk/`** (`avm-sdk`) — the guest SDK. It depends on `avm-sys` and adds
  an `Avm` trait (so guest logic can be tested with a native mock), typed
  `GlobalUint` / `GlobalBytes` handles, transaction helpers (`sender`,
  `application_id`, application arguments), the `#[program_entry]` attribute
  macro, and the lower-level `entry!` macro. It is `no_std` and does not
  allocate unless the guest opts in with `#[program_entry(alloc)]`, which
  installs a guest-side bump allocator so contracts can use `Vec`/`String`; see
  [Allocation](#allocation).

- **`avm-sdk-macros/`** (`avm-sdk-macros`) — the `#[program_entry]` proc macro,
  re-exported by `avm-sdk`. It generates `program() -> i32` and, with the
  `alloc` argument, the `#[global_allocator]` static.

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
  It uses `GlobalUint` for the global counter, the transaction helpers for the
  sender and application arguments, and builds the per-sender key in a `Vec` to
  exercise the optional bump allocator. The Rust runner executes its committed
  fixture with stub transaction and global-state callbacks.

- **`runner/`** — host-side tests that run the committed guest `.wasm` fixtures
  through the real executor with a stub `ed25519verify_bare`.

## Example

```rust
#![cfg_attr(target_arch = "wasm32", no_std)]
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use avm_sdk::{arg_into, entry};

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
`app_rust_counter.wasm`, `app_rust_signed_claim.wasm`, and
`app_avm_sdk_counter.wasm`). The Go unit and end-to-end tests load the first
three; the Rust runner loads the `avm-sdk` counter.

## Resource limits

The example fits the current limits: ~4.7 KiB of program (the program byte cap
is `MaxAbsoluteLogicSigProgramSize`, 16000 in `vFuture`). The LSig cost budget is
`LogicSigMaxCost`, 20,000, converted to 200,000 fuel at 10 fuel/cost unit. The guest linear memory cap
is 4 MiB.

These limits are tighter for application programs. The compiler-generated
`app_rust_signed_claim.wasm` is 4,522 bytes (three pages) and has a 4,821
deterministic setup cost, so even deploying it needs a pooled group; see
`../../WASM_CONTRACTS.md`.

## Allocation

The host ABI copies into caller-provided buffers and supports length queries
(`len == 0`), so guests do not need an allocator. `avm-sdk` is no-alloc by
default.

When a program benefits from `Vec`/`String`, opt in with
`#[program_entry(alloc)]`. That installs [`BumpAllocator`](../avm-sdk/src/alloc.rs)
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
