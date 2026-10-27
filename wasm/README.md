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
read current and grouped transaction fields, read selected context fields, run
`ed25519verify_bare`, read and write the current application's global state
(application programs only), and (for a LogicSig) read its arguments. It cannot
yet touch local state or boxes. Applications can also query asset balances and
unit names and submit inner asset transactions through shared AVM paths.

## Guest ABI

Embedding ABI note: `wasm_lsig_run` now takes `inner_txn_cb` followed by
`log_cb` immediately after `asset_get_cb`, before `ctx`. Embedders must update their declaration and
rebuild the static library; guest imports remain opt-in.

The module must export:

- `program() -> i32`: a non-zero result approves the transaction.

The module may import, from the `env` module:

- `mulw(a: i64, b: i64, out: i32)`: unsigned 64×64 multiplication, matching
  AVM `mulw`. Writes 16 bytes: product high word, then low word.
- `divmodw(a_high: i64, a_low: i64, b_high: i64, b_low: i64, out: i32)`:
  unsigned 128-bit division, matching AVM `divmodw`. Writes 32 bytes: quotient
  high/low, then remainder high/low. A zero denominator traps; a quotient
  larger than uint64 is valid and retains its high word.
- `sqrt(n: i64) -> i64`: floor square root of an unsigned uint64, matching
  AVM `sqrt`.
- `sqrtw(high: i64, low: i64) -> i64`: floor square root of an unsigned
  128-bit high/low pair. This is a WASM extension, not an AVM opcode, and the
  unsigned result fits in uint64.
  All four arithmetic imports work in application and signature mode, use
  unsigned bit patterns in i64 operands/results, and execute integer-only
  host arithmetic (no Go callback). Output words are individually little-endian;
  wide values are high-word-first. Output pointers may be unaligned; negative
  pointers, missing memory, or an out-of-bounds destination trap without partial
  writes. Calibrated fixed host charges are 15/25/10/25 fuel respectively,
  charged before any work, including on failure. These are not TEAL opcode
  costs; see `WASM_PERF.md` for the linux/arm64 calibration and remaining
  cross-machine/worst-case validation requirements.
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
- `gtxn_uint(group_index: i32, field: i32) -> i64`: scalar uint field of a
  transaction in the current group, with the AVM's group-index, type, mode,
  version, and past-transaction-only effects checks.
- `gtxn_bytes(group_index: i32, field: i32, out: i32, len: i32) -> i32`:
  scalar byte field of a group transaction; buffer and length-query semantics
  match `txn_bytes`. Array group reads are not yet exposed.
- `global_uint(field: i32) -> i64` / `global_bytes(field: i32, out: i32,
  len: i32) -> i32`: selected AVM context fields, **not application state**.
  Currently supports `GroupSize=4`, `CurrentApplicationID=8`, and
  `CurrentApplicationAddress=10`. The latter two are application-only and use
  the assigned app ID even during creation. Other fields trap; in particular,
  `OpcodeBudget` is deferred until live WASM fuel can be reflected accurately.
  Types and byte-buffer semantics match the transaction reads.
- `asset_balance(account: i32, asset: i64, out: i32) -> i32`: application-only
  holding query. `account` points to a 32-byte address; `out` must be valid for
  8 bytes. Returns 1 and writes the balance as an unsigned little-endian uint64
  when the holding exists, or 0 without writing if absent. Zero balance is not
  absence. Invalid memory and unavailable resources trap. The asset operand
  follows AVM reference resolution (prefer explicit asset IDs).
- `asset_unit_name(asset: i64, out: i32, len: i32) -> i32`: application-only
  parameter query. Returns the name's byte length (0 for an existing empty name),
  or -1 for a missing asset. `len == 0` queries length without touching guest
  memory. -2 means buffer too small; -3 means invalid memory/negative length.
  These errors do not write partial results. Missing assets ignore the output
  pointer, but negative lengths still return -3. Unavailable resources trap,
  rather than reporting absence. Both queries use shared AVM helpers, including
  paired-holding availability under group resource sharing.
- `itxn_begin()` / `itxn_next()`: application-only inner asset group construction,
  with the AVM's defaults and lifecycle/version/clear-state checks.
- `itxn_field_uint(field: i32, value: i64)` / `itxn_field_bytes(field: i32,
  ptr: i32, len: i32)`: assign common header or asset configuration/transfer
  fields using AVM field numbers and validation. Only `acfg`/`axfer` types are
  allowed. Byte fields are copied; invalid memory, types, sizes, or resources
  trap. Empty bytes require no memory read, but negative pointers still trap.
- `itxn_submit()`: submit through the AVM execution path, enforcing fees,
  resource availability, authorization, group/inner limits, and ledger checks.
  Rejection, traps, and fuel exhaustion roll back with the outer group. Raw
  begin uses AVM fee defaults; SDK asset helpers explicitly set Fee=0.
- `itxn_created_asset_id() -> i64`: the last submitted inner transaction's
  CreatedAssetID; 0 for non-creation, trapping before any submission. Other inner
  result fields and inner application calls are not exposed. Inner app calls
  require live synchronization of the WASM and pooled AVM budgets first.
  Host fuel: begin, next, and submit each charge a flat 10 fuel; their work is
  charged at the protocol level, so submit has no per-transaction host charge.
  Field writes charge 200 (+ byte length); result reads charge 125.
- `log(ptr: i32, len: i32)`: appends application log bytes through shared AVM
  count/total-byte checks (32 calls, 1024 bytes total). Application-only, AVM v5+.
  Bad memory, negative pointers/lengths, and values over 4096 bytes trap;
  empty logs consume a call without reading memory. Rejection/traps roll back
  logs with the outer group. Provisional fuel is 200 + payload byte length,
  charged before host work; calibrate before release.
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
payload. Scalar group fields (`gtxn`) are available; array group fields
(`gtxna`) and general inner field reads are not. Only global (not local)
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
   Group/context uint reads charge 125 fuel; group byte reads charge 225 and
   context byte reads charge 190, before validation or callbacks.
   Asset balance queries charge **900 fuel** and unit-name queries **350 fuel**,
   before mode checks, memory access, or ledger work. These weights account for
   both present and absent resources in the calibration fixtures. Inner begin,
   next, and submit each charge a flat **10 fuel** because their work is charged
   at the protocol level; there is no additional per-inner submit host charge.
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
  global_put_cb, global_del_cb, asset_get_cb, ctx, is_app)`: `1` approved, `0` rejected, `-1`
  error. `txn_cb` is the host's transaction/context-field provider (its callback
  now receives `source` and `group_index` before `field`; source 0=current,
  1=group, 2=context, so embedders must rebuild/update the callback),
  `ed25519verify_bare_cb` its ed25519 verifier, `global_get_cb`/`global_put_cb`/
  `global_del_cb` its global-state accessors, and `asset_get_cb` its asset reader.
  The new asset callback parameter changes the C ABI; embedders must rebuild and
  update their `wasm_lsig_run` calls. Asset query kind 0 reads a holding balance
  (32-byte account pointer), kind 1 a unit name (account ignored); it reports
  existence separately from byte length and encodes uints as 8 little-endian
  bytes. `ctx` is an opaque handle they receive
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
