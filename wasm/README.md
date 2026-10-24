# wasm_lsig

A small Rust library, built on [wasmi](https://github.com/wasmi-labs/wasmi),
that executes WebAssembly LogicSigs. It is compiled to a static library and
linked into go-algorand's `data/transactions/logic` package through cgo.

## What a WASM LogicSig is

A LogicSig whose `Logic` bytes begin with the WebAssembly magic number
(`\0asm`) is run by wasmi instead of the AVM. This is only honored when the
protocol enables `EnableWasmLogicSig` (currently only in the future consensus
version). Nothing else about the LogicSig changes: the address is still the
hash of the program bytes, and delegation signatures still sign those bytes.

## Guest ABI

The module must export:

- `program() -> i32`: a non-zero result approves the transaction.

The module may import, from the `env` module:

- `arg_count() -> i32`: the number of LogicSig arguments.
- `arg(i: i32, out: i32, len: i32) -> i32`: copy argument `i` into linear
  memory at `out`, up to `len` bytes. Returns the number of bytes copied, or a
  negative value on error (`-1` unknown index, `-2` negative length or buffer
  too small, `-3` negative pointer, no `memory` export, or out-of-bounds write).
  Errors neither write memory nor mark the argument as read. A successful read
  marks the argument as read, which enforces the rule that a LogicSig reads every
  argument it carries.
- `txn_uint(field: i32) -> i64`: the value of a uint64 transaction field, i.e.
  the AVM's `txn` for fields that push an integer. Invalid, out-of-version, or
  byte-string fields trap the guest, as an invalid field would fail the AVM.
- `txn_bytes(field: i32, out: i32, len: i32) -> i32`: copy a byte-string
  transaction field into linear memory at `out`, up to `len` bytes. Returns the
  number of bytes copied. Passing `len == 0` returns the field's length without
  copying. A negative value reports an ABI error (`-2` buffer too small, `-3`
  bad pointer or missing `memory` export); invalid, out-of-version, or
  integer fields trap the guest.

Field values come from the node, so the AVM's field semantics (typing, version
gating, allowed modes) have a single source of truth. For now only the current
transaction's non-array fields are exposed (the `txn` opcode, not `txna`,
`gtxn`, or `itxn`). Later opcodes will be added to the same `env` module.

## Execution limits

- Fuel metered; the AVM cost budget is reused as the fuel limit, one fuel unit
  per cost unit. Running out of fuel is an error.
- Each invocation permits at most one linear memory (64 KiB), one table
  (1024 elements), and one instance. Size and count limits bound aggregate
  guest memory/table allocation, including initial allocation before `start`.
  Instantiation fails if the initial allocation exceeds these limits;
  `memory.grow` and `table.grow` return `-1` when growth would exceed them.
- Floats are disabled, and the engine uses portable dispatch, so execution is
  deterministic across platforms.

## C ABI

- `wasm_lsig_run(code, code_len, args, args_len, fuel, fuel_used, args_read,
  err_buf, err_buf_len, txn_cb, txn_ctx)`: `1` approved, `0` rejected, `-1`
  error. `txn_cb` is the host's transaction-field provider and `txn_ctx` is an
  opaque handle it receives back.
- `wasm_lsig_validate(code, code_len, err_buf, err_buf_len)`: `0` valid,
  `-1` invalid.

## Building

`make wasmlib` builds the release static library at
`wasm/target/release/libwasm_lsig.a`. `make build` depends on it. Run `cargo
test` in this directory for the Rust unit tests.

The `.wat` fixtures under `data/transactions/logic/testdata/wasm` are compiled
to `.wasm` with:

```
cargo run --example wat2wasm -- <input.wat> <output.wasm>
```
