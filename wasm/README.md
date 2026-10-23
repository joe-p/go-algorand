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
  negative value on error (`-1` unknown index, `-2` buffer too small, `-3` no
  `memory` export or out-of-bounds write). A successful read marks the argument
  as read, which enforces the rule that a LogicSig reads every argument it
  carries.

In the future, host functions exposing opcodes, transaction fields, and the
like will be added to the same `env` module.

## Execution limits

- Fuel metered; the AVM cost budget is reused as the fuel limit, one fuel unit
  per cost unit. Running out of fuel is an error.
- Linear memory is capped (currently 64 KiB).
- Floats are disabled, and the engine uses portable dispatch, so execution is
  deterministic across platforms.

## C ABI

- `wasm_lsig_run(code, code_len, args, args_len, fuel, fuel_used, args_read,
  err_buf, err_buf_len)`: `1` approved, `0` rejected, `-1` error.
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
