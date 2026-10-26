# wasm_lsig

A small Rust library, built on [wasmi](https://github.com/wasmi-labs/wasmi),
that executes WebAssembly programs. It is compiled to a static library and
linked into go-algorand's `data/transactions/logic` package through cgo.

## What a WASM program is

A program whose bytes begin with the WebAssembly magic number (`\0asm`) is run
by wasmi instead of the AVM. Two kinds of program are supported:

- a **LogicSig**, when the bytes appear in `LogicSig.Logic`. Nothing else about
  the LogicSig changes: the address is still the hash of the program bytes, and
  delegation signatures still sign those bytes. Gated by `EnableWasmLogicSig`.
- an **application** approval or clear-state program, when the bytes appear in
  `ApplicationCallTxnFields.ApprovalProgram` (or `ClearStateProgram`). Gated by
  `EnableWasmApp`. An app's two programs must be the same kind: both WASM or
  both AVM, and an existing app cannot be updated from one kind to the other.

Both are currently only honored in the future consensus version. A guest can
read the current transaction's fields (including the application arguments), run
`ed25519verify_bare`, read and write the current application's global state
(application programs only), and (for a LogicSig) read its arguments. It cannot
yet touch local state, boxes, or inner transactions.

## Guest ABI

The module must export:

- `program() -> i32`: a non-zero result approves the transaction.

The module may import, from the `env` module:

- `arg_count() -> i32`: the number of LogicSig arguments. Signature-only: it
  traps when the module runs as an application program.
- `arg(i: i32, out: i32, len: i32) -> i32`: copy argument `i` into linear
  memory at `out`, up to `len` bytes. Returns the number of bytes copied, or a
  negative value on error (`-1` unknown index, `-2` negative length or buffer
  too small, `-3` negative pointer, no `memory` export, or out-of-bounds write).
  Errors neither write memory nor mark the argument as read. A successful read
  marks the argument as read, which enforces the rule that a LogicSig reads every
  argument it carries. Signature-only: it traps when the module runs as an
  application program. An application reads its arguments with `txna_bytes`
  below.
- `txn_uint(field: i32) -> i64`: the value of a uint64 transaction field, i.e.
  the AVM's `txn` for fields that push an integer. Invalid, out-of-version, or
  byte-string fields trap the guest, as an invalid field would fail the AVM.
- `txn_bytes(field: i32, out: i32, len: i32) -> i32`: copy a byte-string
  transaction field into linear memory at `out`, up to `len` bytes. Returns the
  number of bytes copied. Passing `len == 0` returns the field's length without
  copying. A negative value reports an ABI error (`-2` buffer too small, `-3`
  bad pointer or missing `memory` export); invalid, out-of-version, or
  integer fields trap the guest.
- `txna_uint(field: i32, index: i32) -> i64`: like `txn_uint`, but for an array
  field element, i.e. the AVM's `txna`. A non-array field, an out-of-range
  index, or a byte-string element traps the guest.
- `txna_bytes(field: i32, index: i32, out: i32, len: i32) -> i32`: like
  `txn_bytes`, but for an array field element. The application arguments are
  `txna_bytes(ApplicationArgs=26, i, ...)`; the argument count is
  `txn_uint(NumAppArgs=27)`.
- `app_global_get_uint(key: i32, key_len: i32) -> i64`: the uint64 value at
  `key` in the current application's global state, the AVM's `app_global_get`. A
  missing key reads as 0. A stored byte-string value traps the guest.
- `app_global_get_bytes(key: i32, key_len: i32, out: i32, len: i32) -> i32`:
  like `app_global_get_uint`, but for a byte-string value. Returns the value's
  length; passing `len == 0` returns the length without copying. A missing key
  reads as empty (length 0). A stored uint value traps the guest.
- `app_global_put_uint(key: i32, key_len: i32, value: i64)`: writes `value` to
  `key` in the current app's global state, the AVM's `app_global_put`. A
  too-long key or value traps the guest.
- `app_global_put_bytes(key: i32, key_len: i32, val: i32, val_len: i32)`: like
  `app_global_put_uint`, but for a byte-string value.
- `app_global_del(key: i32, key_len: i32)`: deletes `key` from the current app's
  global state, the AVM's `app_global_del`. Deleting an absent key is not an
  error.
- `ed25519verify_bare(data: i32, data_len: i32, sig: i32, sig_len: i32,
  pk: i32, pk_len: i32) -> i32`: the AVM's `ed25519verify_bare`, verifying the
  ed25519 signature `sig` over `data` under public key `pk`. Returns `1` when
  the signature is valid and `0` when it is not. The three byte ranges are read
  out of linear memory and verified in Go. A negative length, an out-of-bounds
  range, oversized data, or a wrong-length key or signature traps the guest.

The `app_global_*` functions are application-only, mirroring the AVM's
`only(ModeApp)` restriction: they trap when the module runs as a LogicSig.

Field values come from the node, `ed25519verify_bare` is verified by the node,
and global-state reads/writes go through the same ledger accessors the AVM
opcodes use, so the AVM's semantics (typing, version gating, allowed modes, the
4096-byte value limit, schema and length limits, and ed25519 validation) have a
single source of truth. Each read invokes Go once; length queries do not copy the
payload. For now the `txn`/`txna` opcodes are limited to the current
transaction's fields (not `gtxn`, `gtxna`, or `itxn`), only global (not local)
application state is exposed, and `ed25519verify_bare` is the only crypto
opcode. More opcodes will be added to the same `env` module.

## Execution limits

- Fuel metered at **10 fuel per AVM cost unit**: a 20,000-cost LogicSig budget
  permits 200,000 fuel. Guest instruction weights use the custom
  `guest_operator_costs` table in `src/lib.rs`, not wasmi's defaults: every
  operator costs at least 1 fuel, and `call`/`call_indirect` cost 10 (wasmi
  charges 1 for `call` and 0 for `return`, `end`, `else`, and structural
  operators). Consumed fuel
  is divided by ten, rounding up per evaluation, before charging AVM/shared
  budgets. Fuel includes deterministic setup charges reserved before compilation
  and allocation (see `../WASM_PERF.md` and `src/setup.rs`), as well as execution
  and wasmi's lazy translation. Failed evaluations also retain consumed fuel;
  invalid/unaffordable preflights consume their supplied allowance. Host charges
  are calibrated toward 1 ns/fuel on the fixtures in
  `../WASM_PERF.md` (they predate the operator table, so each host call now
  also pays the 10-fuel `call`; the measured ratio is ~0.8-1.0 ns/fuel):
  `arg` 30, `arg_count` 10, `txn_uint`/`txna_uint` 125,
  `txn_bytes` 190, `txna_bytes` 200, global get uint/bytes 130/170, global put
  uint/bytes 410/390, and global delete 140 fuel. These fixed charges include
  length queries and invalid reads, on top of WASM instruction fuel.
  `ed25519verify_bare` has an explicit 28,500-fuel host charge (2850 AVM budget
  units, versus the TEAL opcode's 1900; 28,527 fuel for the whole call under the
  operator table). The raw host charge is independent of the conversion. The charge occurs
  before host work; insufficient fuel traps. Running out of fuel is an error.
- Each invocation permits at most one linear memory (4 MiB, matching the AVM's
  bound on live data of `maxStringSize` 4096 by `maxStackDepth` 1000), one table
  (1024 elements), and one instance. Size and count limits bound aggregate
  guest memory/table allocation, including initial allocation at instantiation.
  Instantiation fails if the initial allocation exceeds these limits;
  `memory.grow` and `table.grow` return `-1` when growth would exceed them.
- The Rust setup preflight caps encoded modules at 16 KiB and expanded locals
  at 50,000 per function. Go still enforces narrower mode/protocol byte limits.
- WebAssembly start sections are forbidden, including no-op start functions.
  Both static validation and execution reject them before instantiation; only
  the explicit `program` entry point is invoked. An exported `_start` name alone
  is not a start section and is never invoked automatically.
- `make wasm-setupcost` measures cold preflight, engine creation, compilation,
  host linker setup, instantiation, and teardown separately, without invoking
  `program`. It includes size sweeps used to calibrate the setup weights.
- Floats are disabled, and the engine uses portable dispatch, so execution is
  deterministic across platforms.

## C ABI

- `wasm_lsig_run(code, code_len, args, args_len, fuel, fuel_used, args_read,
  err_buf, err_buf_len, txn_cb, ed25519verify_bare_cb, global_get_cb,
  global_put_cb, global_del_cb, ctx, is_app)`: `1` approved, `0` rejected, `-1`
  error. `txn_cb` is the host's transaction-field provider,
  `ed25519verify_bare_cb` its ed25519 verifier, `global_get_cb`/`global_put_cb`/
  `global_del_cb` its global-state accessors, `ctx` an opaque handle they receive
  back, and `is_app` non-zero when the program is an application program (which
  disables the signature-only `arg`/`arg_count` and enables `app_global_*`).
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

Guest programs can also be written in Rust. `wasm/guest/` contains `avm-sys`
(the unopinionated `env` ABI bindings) and `avm-sdk` (typed helpers, the
host-backed `ed25519-dalek`, and examples); see `wasm/guest/README.md`. `make
wasm-guest-fixtures` rebuilds the committed Rust guest fixtures under
`data/transactions/logic/testdata/wasm/`.
