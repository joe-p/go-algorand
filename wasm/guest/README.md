# WASM guest SDK

Guest-side Rust crates for authoring WASM programs (LogicSigs and application
programs). They are compiled to `wasm32-unknown-unknown`; the host executor that
runs them is the sibling `wasmi` crate in `../` (see `../README.md`).

This is a separate Cargo workspace from the host executor so `make wasmlib`
(`cargo build --manifest-path wasm/Cargo.toml`) is unaffected.

## Crates

- **`host/`** (`wasm-guest`) — idiomatic, safe Rust bindings for the `env` host
  functions. It wraps the raw imports in typed APIs so a guest never touches
  pointers or error sentinels, and provides the `entry!` macro to define the
  `program` export. It is `no_std` and does not allocate.

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
  `txna_bytes(TxnField::ApplicationArgs, i, buf)` (or the `app_args_count` /
  `app_arg_into` helpers). The `app_global_*` functions are application-only and
  trap in a LogicSig.

  Cases the host *traps* on (an invalid transaction field, an oversized value)
  are not `Result`; a trap ends the guest, as in the AVM.

- **`ed25519-dalek/`** — a vendored fork of `ed25519-dalek` 2.2.0 (upstream is
  BSD-3-Clause; see its `LICENSE`). It keeps the crate name and API, and is
  selected with `[patch.crates-io]` in the workspace root, so a guest depends on
  `ed25519-dalek` normally. On `wasm32`, `VerifyingKey::verify` and
  `verify_strict` call the `env.ed25519verify_bare` host function instead of
  doing curve arithmetic in the guest, and `VerifyingKey::from_bytes` skips
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

- **`runner/`** — host-side tests that run the committed guest `.wasm` fixtures
  through the real executor with a stub `ed25519verify_bare`.

## Example

```rust
#![cfg_attr(target_arch = "wasm32", no_std)]
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use wasm_guest::{arg_into, entry};

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

# Build the guest fixture used by the Go end-to-end tests
make wasm-guest-fixtures
```

`make wasm-guest-fixtures` builds `examples/ed25519-verify` for
`wasm32-unknown-unknown` and copies the result to
`data/transactions/logic/testdata/wasm/ed25519_dalek.wasm`, which
`data/transactions/logic/wasm_test.go` and `data/transactions/verify/wasm_test.go`
load.

## Resource limits

The example fits the current limits: ~4.7 KiB of program (the program byte cap
is `MaxAbsoluteLogicSigProgramSize`, 16000 in `vFuture`). The LSig cost budget is
`LogicSigMaxCost`, 20,000, converted to 200,000 fuel at 10 fuel/cost unit. The guest linear memory cap
is 4 MiB.
