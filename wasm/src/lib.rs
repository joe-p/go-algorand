// Copyright (C) 2019-2026 Algorand Foundation Ltd.
// This file is part of go-algorand
//
// go-algorand is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// go-algorand is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with go-algorand.  If not, see <https://www.gnu.org/licenses/>.

//! A wasmi-based executor for WebAssembly LogicSigs.
//!
//! A WASM LogicSig is a raw WebAssembly module carried in `LogicSig.Logic`
//! (distinguished from AVM bytecode by the `\0asm` magic prefix). The guest can
//! read the LogicSig arguments and, through a callback into the host, the
//! fields of the transaction it authorizes. In the future more opcodes will be
//! added to the same `env` module.
//!
//! Guest ABI:
//!   * The module must export `program` with signature `() -> i32`. A non-zero
//!     result approves the transaction.
//!   * The module may import, from the `env` module:
//!       - `arg_count() -> i32`            number of LogicSig args
//!       - `arg(i: i32, out: i32, len: i32) -> i32`
//!             copy arg `i` into linear memory at `out`, up to `len` bytes.
//!             Returns the number of bytes copied, or a negative sentinel on
//!             error. A successful read marks the argument as read, which the
//!             node uses to enforce that a LogicSig reads every argument it
//!             carries.
//!       - `txn_uint(field: i32) -> i64`
//!             the value of a uint64 transaction field (the AVM's `txn` for
//!             fields that push an integer). An invalid field, or a field whose
//!             value is a byte string, traps the guest.
//!       - `txn_bytes(field: i32, out: i32, len: i32) -> i32`
//!             copy a byte-string transaction field into linear memory at
//!             `out`, up to `len` bytes. Returns the number of bytes copied.
//!             Passing `len == 0` returns the field's length without copying. A
//!             negative sentinel reports an ABI error (buffer too small, bad
//!             pointer); an invalid field, or a field whose value is an
//!             integer, traps the guest.
//!       - `ed25519verify_bare(data: i32, data_len: i32, sig: i32, sig_len: i32,
//!                            pk: i32, pk_len: i32) -> i32`
//!             verify the ed25519 signature `sig` over `data` with public key
//!             `pk`, the AVM's `ed25519verify_bare` opcode. Returns `1` when the
//!             signature is valid and `0` when it is not. The three byte ranges
//!             are copied out of linear memory and verified in Go, so the opcode
//!             matches the AVM. A negative length, an out-of-bounds range, a
//!             wrong-length key or signature, or oversized data traps.
//!
//! The transaction fields come from the host through `TxnFieldCb`, so the AVM's
//! field semantics (typing, version gating, allowed modes) have a single
//! source of truth in Go. Byte fields have the AVM's 4096-byte limit. Each txn
//! host invocation costs one additional fuel unit before performing host work.
//! `ed25519verify_bare` similarly delegates verification to Go and is charged
//! the opcode's AVM cost.
//!
//! Execution is deterministic (no floats, no auto-dispatch), fuel metered, and
//! linear memory is capped. The engine is rebuilt per call; this is simple and
//! avoids sharing mutable interpreter state across goroutines.

use std::panic::{catch_unwind, AssertUnwindSafe};

use wasmi::{
    Caller, Config, Engine, ExternType, Instance, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, TrapCode, TypedFunc, ValType,
};

/// Maximum number of LogicSig arguments. Must match `transactions.EvalMaxArgs`.
const MAX_ARGS: usize = 255;

/// Number of u64 words needed to record which arguments were read.
const ARGS_READ_WORDS: usize = MAX_ARGS.div_ceil(64);

/// Default cap on a guest's linear memory, in bytes. Matches the AVM's upper
/// bound on live data, `maxStringSize` (4096) times `maxStackDepth` (1000),
/// rounded up to a whole number of wasm pages (64 pages, 4 MiB).
///
/// This is a consensus-relevant resource cap that is not metered by execution
/// fuel: a guest can declare a large minimum memory and make the node allocate
/// it. It is fixed at 4 MiB while WASM LogicSigs are gated behind `vFuture`;
/// before release it should become a dedicated consensus parameter.
const DEFAULT_MEMORY_LIMIT: usize = 4 * 1024 * 1024;

/// `DEFAULT_MEMORY_LIMIT` expressed in wasm pages (64 KiB each). Tests that
/// must exceed the cap derive from this so the cap can change without the
/// tests silently passing.
#[cfg(test)]
const DEFAULT_MEMORY_PAGES: usize = DEFAULT_MEMORY_LIMIT / 65536;

/// A guest may have at most one table, with this many elements. Restricting
/// both the size and count bounds aggregate table allocation at instantiation,
/// which is not metered by execution fuel.
const MAX_TABLE_ELEMENTS: usize = 1024;

// Error sentinels returned by the `arg` host function.
const ARG_ERR_INDEX: i32 = -1;
const ARG_ERR_BUFFER: i32 = -2;
const ARG_ERR_MEMORY: i32 = -3;

// Error sentinels returned by the `txn_bytes` host function. Invalid or
// unavailable fields trap the guest instead (see `fetch_txn_field`).
const TXN_ERR_BUFFER: i32 = -2;
const TXN_ERR_MEMORY: i32 = -3;

/// Size of the error-message buffer shared with the host callbacks.
const ERR_BUF_LEN: usize = 256;

/// Must match Go's config.MaxAVMBytesSize. Go also enforces this limit. It
/// bounds any byte value crossing the boundary: transaction fields, and the
/// operands of byte-string opcodes.
const MAX_AVM_BYTES: usize = 4096;

/// Additional fuel charged per txn host invocation, including length queries.
const TXN_FUEL_COST: u64 = 1;

/// Fuel charged for one `ed25519verify_bare` call. Must match the opcode's cost
/// in the Go opcode table (`costly(1900)` for `ed25519verify_bare`).
const ED25519_VERIFY_BARE_FUEL_COST: u64 = 1900;

// Error sentinels returned by the global-state byte-string read. Invalid
// operations (a wrong-typed value) trap the guest instead.
const GLOBAL_ERR_BUFFER: i32 = -2;
const GLOBAL_ERR_MEMORY: i32 = -3;

/// Fuel charged per global-state host invocation. Must match the AVM opcode cost
/// in the Go opcode table: `app_global_get`, `app_global_put`, and
/// `app_global_del` are all `only(ModeApp)`, whose default cost is 1.
const GLOBAL_FUEL_COST: u64 = 1;

/// A transaction field provider, implemented in Go. `ctx` is an opaque handle
/// that Go maps back to the evaluation context; Rust never dereferences it.
///
/// `field` selects the field, `index` the element for an array field (ignored
/// when `expect_array` is 0), and `expect_array` whether the field is expected
/// to be an array field (`txna`-style) rather than a scalar one. The callback
/// writes the field's bytes into `buf` (up to `buf_cap`), reporting the required
/// length in `out_len` and whether the field is a byte string in `is_bytes`
/// (0 = uint, non-zero = bytes). It returns `0` on success, `-1` on error (a
/// message is written to `err`), and `-2` when `buf` is too small (the required
/// length is still written to `out_len`).
type TxnFieldCb = unsafe extern "C" fn(
    ctx: usize,
    field: u32,
    index: u32,
    expect_array: u32,
    buf: *mut u8,
    buf_cap: usize,
    out_len: *mut usize,
    is_bytes: *mut u8,
    err: *mut u8,
    err_cap: usize,
) -> i32;

/// A verifier for `ed25519verify_bare`, implemented in Go so the opcode has the
/// AVM's exact semantics. The three byte ranges are read out of guest memory
/// before the call. It returns `1` when the signature verifies, `0` when it does
/// not, and `-1` on error (a message is written to `err`).
type Ed25519VerifyCb = unsafe extern "C" fn(
    data: *const u8,
    data_len: usize,
    sig: *const u8,
    sig_len: usize,
    pk: *const u8,
    pk_len: usize,
    err: *mut u8,
    err_cap: usize,
) -> i32;

/// A global-state provider, implemented in Go. `ctx` is the opaque evaluation
/// context handle. `key` is the state key. The callback writes the value into
/// `buf` (up to `buf_cap`): a uint is 8 little-endian bytes, a byte string is
/// copied verbatim. It reports the value's length in `out_len`, whether it is a
/// byte string in `out_is_bytes` (0 = uint), and whether the key exists in
/// `out_exists`. It returns `0` on success, `-1` on error, and `-2` when `buf`
/// is too small (the required length is still written to `out_len`).
type GlobalGetCb = unsafe extern "C" fn(
    ctx: usize,
    key: *const u8,
    key_len: usize,
    buf: *mut u8,
    buf_cap: usize,
    out_len: *mut usize,
    out_is_bytes: *mut u8,
    out_exists: *mut u8,
    err: *mut u8,
    err_cap: usize,
) -> i32;

/// A global-state writer, implemented in Go. `val_is_uint` non-zero selects a
/// uint value (8 little-endian bytes in `val`); otherwise `val` is a byte
/// string. It returns `0` on success and `-1` on error.
type GlobalPutCb = unsafe extern "C" fn(
    ctx: usize,
    key: *const u8,
    key_len: usize,
    val: *const u8,
    val_len: usize,
    val_is_uint: u32,
    err: *mut u8,
    err_cap: usize,
) -> i32;

/// A global-state deleter, implemented in Go. It returns `0` on success and `-1`
/// on error.
type GlobalDelCb =
    unsafe extern "C" fn(ctx: usize, key: *const u8, key_len: usize, err: *mut u8, err_cap: usize) -> i32;

/// A single LogicSig argument passed across the C ABI as a pointer/length pair.
#[repr(C)]
pub struct WasmLsigArg {
    pub ptr: *const u8,
    pub len: usize,
}

/// The host callbacks available to a guest, implemented in Go. `ctx` is an
/// opaque handle passed to callbacks that need it; Rust never dereferences it. A
/// missing callback makes the corresponding opcode trap.
#[derive(Clone, Copy, Default)]
struct HostCallbacks {
    /// Provider for `txn` field reads. `None` in tests, or if a guest is run
    /// without a transaction.
    txn_field: Option<TxnFieldCb>,
    /// Provider for `ed25519verify_bare`. `None` in tests, or if a guest is run
    /// without a host.
    ed25519_verify_bare: Option<Ed25519VerifyCb>,
    /// Provider for global-state reads.
    global_get: Option<GlobalGetCb>,
    /// Provider for global-state writes.
    global_put: Option<GlobalPutCb>,
    /// Provider for global-state deletes.
    global_del: Option<GlobalDelCb>,
    /// Opaque handle passed back to `txn_field`.
    ctx: usize,
    /// Whether the guest is an application program rather than a LogicSig. In
    /// application mode the `arg`/`arg_count` host functions are unavailable,
    /// matching the AVM, where the `arg` opcodes are signature-only.
    is_app: bool,
}

/// Store-level state visible to host functions.
struct HostState {
    args: Vec<Vec<u8>>,
    accessed: [u64; ARGS_READ_WORDS],
    limits: StoreLimits,
    host: HostCallbacks,
}

impl HostState {
    fn mark_read(&mut self, index: usize) {
        self.accessed[index / 64] |= 1 << (index % 64);
    }
}

fn arg_count(caller: Caller<'_, HostState>) -> Result<i32, wasmi::Error> {
    if caller.data().host.is_app {
        return Err(wasmi::Error::new(
            "arg_count is not available in application mode",
        ));
    }
    Ok(caller.data().args.len() as i32)
}

fn arg(
    mut caller: Caller<'_, HostState>,
    index: i32,
    out_ptr: i32,
    out_len: i32,
) -> Result<i32, wasmi::Error> {
    if caller.data().host.is_app {
        return Err(wasmi::Error::new("arg is not available in application mode"));
    }
    if index < 0 {
        return Ok(ARG_ERR_INDEX);
    }
    let index = index as usize;

    // Clone the argument so the immutable borrow of the store ends before we
    // need a mutable one to write into linear memory.
    let bytes = match caller.data().args.get(index) {
        Some(arg) => arg.clone(),
        None => return Ok(ARG_ERR_INDEX),
    };

    if out_len < 0 || bytes.len() > out_len as usize {
        return Ok(ARG_ERR_BUFFER);
    }
    if out_ptr < 0 {
        return Ok(ARG_ERR_MEMORY);
    }

    let memory: Option<wasmi::Memory> = caller.get_export("memory").and_then(|e| e.into_memory());
    let Some(memory) = memory else {
        return Ok(ARG_ERR_MEMORY);
    };
    if memory.write(&mut caller, out_ptr as usize, &bytes).is_err() {
        return Ok(ARG_ERR_MEMORY);
    }

    caller.data_mut().mark_read(index);
    Ok(bytes.len() as i32)
}

/// Reads a NUL-terminated message from a fixed error buffer.
fn cstr(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// Fetches a field once into a caller-owned buffer. A short buffer still
/// reports the type and required length. Invalid or unavailable fields trap.
/// `index` selects the element for an array field and is meaningful only when
/// `expect_array` is true.
fn fetch_txn_field(
    caller: &mut Caller<'_, HostState>,
    field: i32,
    index: i32,
    expect_array: bool,
    buf: &mut [u8],
) -> Result<(bool, usize), wasmi::Error> {
    // Charge before doing any host work, including validation and callbacks.
    let remaining = caller.get_fuel()?;
    let remaining = remaining
        .checked_sub(TXN_FUEL_COST)
        .ok_or_else(|| wasmi::Error::from(TrapCode::OutOfFuel))?;
    caller.set_fuel(remaining)?;

    let cb = caller.data().host.txn_field;
    let ctx = caller.data().host.ctx;
    let Some(cb) = cb else {
        return Err(wasmi::Error::new(
            "txn host functions are not available in this build",
        ));
    };
    if field < 0 {
        return Err(wasmi::Error::new(format!("invalid txn field {field}")));
    }
    if expect_array && index < 0 {
        return Err(wasmi::Error::new(format!(
            "invalid txn array index {index}"
        )));
    }
    let index = if index < 0 { 0 } else { index as u32 };

    let mut err = [0u8; ERR_BUF_LEN];
    let mut len = 0usize;
    let mut is_bytes = 0u8;

    let ret = unsafe {
        cb(
            ctx,
            field as u32,
            index,
            expect_array as u32,
            buf.as_mut_ptr(),
            buf.len(),
            &mut len,
            &mut is_bytes,
            err.as_mut_ptr(),
            err.len(),
        )
    };

    if ret != 0 && ret != TXN_ERR_BUFFER {
        return Err(wasmi::Error::new(format!(
            "txn field {field} error: {}",
            cstr(&err)
        )));
    }

    if is_bytes != 0 && len > MAX_AVM_BYTES {
        return Err(wasmi::Error::new("txn byte field exceeds AVM size limit"));
    }
    Ok((is_bytes != 0, len))
}

/// Shared implementation of the scalar (`txn`) and array (`txna`) uint field
/// reads.
fn read_txn_uint(
    caller: &mut Caller<'_, HostState>,
    field: i32,
    index: i32,
    expect_array: bool,
) -> Result<i64, wasmi::Error> {
    let mut bytes = [0u8; 8];
    let (is_bytes, len) = fetch_txn_field(caller, field, index, expect_array, &mut bytes)?;
    if is_bytes {
        return Err(wasmi::Error::new(format!(
            "txn field {field} is not a uint"
        )));
    }
    if len != bytes.len() {
        return Err(wasmi::Error::new(format!(
            "txn uint field {field} had {len} bytes"
        )));
    }
    Ok(i64::from_le_bytes(bytes))
}

fn txn_uint(mut caller: Caller<'_, HostState>, field: i32) -> Result<i64, wasmi::Error> {
    read_txn_uint(&mut caller, field, 0, false)
}

fn txna_uint(
    mut caller: Caller<'_, HostState>,
    field: i32,
    index: i32,
) -> Result<i64, wasmi::Error> {
    read_txn_uint(&mut caller, field, index, true)
}

/// Shared implementation of the scalar (`txn`) and array (`txna`) byte-string
/// field reads.
fn read_txn_bytes(
    mut caller: &mut Caller<'_, HostState>,
    field: i32,
    index: i32,
    expect_array: bool,
    out_ptr: i32,
    out_len: i32,
) -> Result<i32, wasmi::Error> {
    let mut buf = [0u8; MAX_AVM_BYTES];
    // Length queries don't fetch or copy the field payload.
    let capacity = if out_len <= 0 {
        0
    } else {
        (out_len as usize).min(buf.len())
    };
    let (is_bytes, len) =
        fetch_txn_field(caller, field, index, expect_array, &mut buf[..capacity])?;
    if !is_bytes {
        return Err(wasmi::Error::new(format!(
            "txn field {field} is not a byte string"
        )));
    }
    if out_len < 0 {
        return Ok(TXN_ERR_MEMORY);
    }
    // A zero-length buffer queries the required length without touching memory.
    if out_len == 0 {
        return Ok(len as i32);
    }
    if out_ptr < 0 {
        return Ok(TXN_ERR_MEMORY);
    }
    if len > out_len as usize {
        return Ok(TXN_ERR_BUFFER);
    }

    let memory: Option<wasmi::Memory> = caller.get_export("memory").and_then(|e| e.into_memory());
    let Some(memory) = memory else {
        return Ok(TXN_ERR_MEMORY);
    };
    if memory
        .write(&mut caller, out_ptr as usize, &buf[..len])
        .is_err()
    {
        return Ok(TXN_ERR_MEMORY);
    }
    Ok(len as i32)
}

fn txn_bytes(
    mut caller: Caller<'_, HostState>,
    field: i32,
    out_ptr: i32,
    out_len: i32,
) -> Result<i32, wasmi::Error> {
    read_txn_bytes(&mut caller, field, 0, false, out_ptr, out_len)
}

fn txna_bytes(
    mut caller: Caller<'_, HostState>,
    field: i32,
    index: i32,
    out_ptr: i32,
    out_len: i32,
) -> Result<i32, wasmi::Error> {
    read_txn_bytes(&mut caller, field, index, true, out_ptr, out_len)
}

/// Copies a byte range out of a guest's linear memory into a fresh buffer. The
/// range must fit within `MAX_AVM_BYTES` so a guest cannot force an oversized
/// allocation; a negative length or pointer, a missing `memory` export, or a
/// range outside linear memory traps.
fn read_guest_bytes(
    caller: &mut Caller<'_, HostState>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, wasmi::Error> {
    if len < 0 {
        return Err(wasmi::Error::new("negative byte length"));
    }
    if ptr < 0 {
        return Err(wasmi::Error::new("negative byte pointer"));
    }
    let len = len as usize;
    if len > MAX_AVM_BYTES {
        return Err(wasmi::Error::new("byte value exceeds AVM size limit"));
    }
    let mut buf = vec![0u8; len];
    if buf.is_empty() {
        return Ok(buf);
    }
    let memory: Option<wasmi::Memory> = caller.get_export("memory").and_then(|e| e.into_memory());
    let Some(memory) = memory else {
        return Err(wasmi::Error::new("missing memory export"));
    };
    memory
        .read(&*caller, ptr as usize, &mut buf)
        .map_err(|_| wasmi::Error::new("byte range outside linear memory"))?;
    Ok(buf)
}

/// Charges `cost` fuel before host work, trapping if there is not enough.
fn charge_fuel(caller: &mut Caller<'_, HostState>, cost: u64) -> Result<(), wasmi::Error> {
    let remaining = caller.get_fuel()?;
    let remaining = remaining
        .checked_sub(cost)
        .ok_or_else(|| wasmi::Error::from(TrapCode::OutOfFuel))?;
    caller.set_fuel(remaining)
}

/// Rejects global-state access outside application mode, mirroring the AVM's
/// `only(ModeApp)` restriction.
fn require_app(caller: &Caller<'_, HostState>) -> Result<(), wasmi::Error> {
    if caller.data().host.is_app {
        Ok(())
    } else {
        Err(wasmi::Error::new(
            "global state is not available in signature mode",
        ))
    }
}

/// Calls the host's global-state reader, returning `(exists, len, is_bytes)`.
/// `buf` receives the value (a uint as 8 little-endian bytes, or a byte string).
fn host_global_get(
    caller: &mut Caller<'_, HostState>,
    key: &[u8],
    buf: &mut [u8],
) -> Result<(bool, usize, bool), wasmi::Error> {
    let cb = caller.data().host.global_get;
    let ctx = caller.data().host.ctx;
    let Some(cb) = cb else {
        return Err(wasmi::Error::new(
            "global state host functions are not available in this build",
        ));
    };
    let mut err = [0u8; ERR_BUF_LEN];
    let mut out_len = 0usize;
    let mut out_is_bytes = 0u8;
    let mut out_exists = 0u8;
    let ret = unsafe {
        cb(
            ctx,
            key.as_ptr(),
            key.len(),
            buf.as_mut_ptr(),
            buf.len(),
            &mut out_len,
            &mut out_is_bytes,
            &mut out_exists,
            err.as_mut_ptr(),
            err.len(),
        )
    };
    if ret != 0 && ret != GLOBAL_ERR_BUFFER {
        return Err(wasmi::Error::new(format!(
            "global state read error: {}",
            cstr(&err)
        )));
    }
    if out_is_bytes != 0 && out_len > MAX_AVM_BYTES {
        return Err(wasmi::Error::new(
            "global state byte value exceeds AVM size limit",
        ));
    }
    Ok((out_exists != 0, out_len, out_is_bytes != 0))
}

/// Implements the AVM's `app_global_get` for a uint value. A missing key reads
/// as 0; a stored byte-string value traps the guest.
fn app_global_get_uint(
    mut caller: Caller<'_, HostState>,
    key_ptr: i32,
    key_len: i32,
) -> Result<i64, wasmi::Error> {
    require_app(&caller)?;
    charge_fuel(&mut caller, GLOBAL_FUEL_COST)?;
    let key = read_guest_bytes(&mut caller, key_ptr, key_len)?;
    let mut buf = [0u8; 8];
    let (exists, len, is_bytes) = host_global_get(&mut caller, &key, &mut buf)?;
    if !exists {
        return Ok(0);
    }
    if is_bytes {
        return Err(wasmi::Error::new(
            "global state value is a byte string, not a uint",
        ));
    }
    if len != 8 {
        return Err(wasmi::Error::new(format!(
            "uint global state value had {len} bytes"
        )));
    }
    Ok(i64::from_le_bytes(buf))
}

/// Implements the AVM's `app_global_get` for a byte-string value. A missing key
/// reads as an empty (length 0) value; a stored uint value traps the guest.
fn app_global_get_bytes(
    mut caller: Caller<'_, HostState>,
    key_ptr: i32,
    key_len: i32,
    out_ptr: i32,
    out_len: i32,
) -> Result<i32, wasmi::Error> {
    require_app(&caller)?;
    charge_fuel(&mut caller, GLOBAL_FUEL_COST)?;
    let key = read_guest_bytes(&mut caller, key_ptr, key_len)?;
    let mut buf = [0u8; MAX_AVM_BYTES];
    // Length queries don't fetch or copy the value payload.
    let capacity = if out_len <= 0 {
        0
    } else {
        (out_len as usize).min(buf.len())
    };
    let (exists, len, is_bytes) = host_global_get(&mut caller, &key, &mut buf[..capacity])?;
    if !exists {
        return Ok(0);
    }
    if !is_bytes {
        return Err(wasmi::Error::new(
            "global state value is a uint, not a byte string",
        ));
    }
    if out_len < 0 {
        return Ok(GLOBAL_ERR_MEMORY);
    }
    // A zero-length buffer queries the required length without touching memory.
    if out_len == 0 {
        return Ok(len as i32);
    }
    if out_ptr < 0 {
        return Ok(GLOBAL_ERR_MEMORY);
    }
    if len > out_len as usize {
        return Ok(GLOBAL_ERR_BUFFER);
    }

    let memory: Option<wasmi::Memory> = caller.get_export("memory").and_then(|e| e.into_memory());
    let Some(memory) = memory else {
        return Ok(GLOBAL_ERR_MEMORY);
    };
    if memory
        .write(&mut caller, out_ptr as usize, &buf[..len])
        .is_err()
    {
        return Ok(GLOBAL_ERR_MEMORY);
    }
    Ok(len as i32)
}

/// Shared implementation of the global-state writes.
fn host_global_put(
    caller: &mut Caller<'_, HostState>,
    key: &[u8],
    val: &[u8],
    is_uint: bool,
) -> Result<(), wasmi::Error> {
    let cb = caller.data().host.global_put;
    let ctx = caller.data().host.ctx;
    let Some(cb) = cb else {
        return Err(wasmi::Error::new(
            "global state host functions are not available in this build",
        ));
    };
    let mut err = [0u8; ERR_BUF_LEN];
    let ret = unsafe {
        cb(
            ctx,
            key.as_ptr(),
            key.len(),
            val.as_ptr(),
            val.len(),
            is_uint as u32,
            err.as_mut_ptr(),
            err.len(),
        )
    };
    if ret != 0 {
        return Err(wasmi::Error::new(format!(
            "global state write error: {}",
            cstr(&err)
        )));
    }
    Ok(())
}

/// Implements the AVM's `app_global_put` for a uint value.
fn app_global_put_uint(
    mut caller: Caller<'_, HostState>,
    key_ptr: i32,
    key_len: i32,
    value: i64,
) -> Result<(), wasmi::Error> {
    require_app(&caller)?;
    charge_fuel(&mut caller, GLOBAL_FUEL_COST)?;
    let key = read_guest_bytes(&mut caller, key_ptr, key_len)?;
    host_global_put(&mut caller, &key, &value.to_le_bytes(), true)
}

/// Implements the AVM's `app_global_put` for a byte-string value.
fn app_global_put_bytes(
    mut caller: Caller<'_, HostState>,
    key_ptr: i32,
    key_len: i32,
    val_ptr: i32,
    val_len: i32,
) -> Result<(), wasmi::Error> {
    require_app(&caller)?;
    charge_fuel(&mut caller, GLOBAL_FUEL_COST)?;
    let key = read_guest_bytes(&mut caller, key_ptr, key_len)?;
    let val = read_guest_bytes(&mut caller, val_ptr, val_len)?;
    host_global_put(&mut caller, &key, &val, false)
}

/// Implements the AVM's `app_global_del`.
fn app_global_del(
    mut caller: Caller<'_, HostState>,
    key_ptr: i32,
    key_len: i32,
) -> Result<(), wasmi::Error> {
    require_app(&caller)?;
    charge_fuel(&mut caller, GLOBAL_FUEL_COST)?;
    let key = read_guest_bytes(&mut caller, key_ptr, key_len)?;
    let cb = caller.data().host.global_del;
    let ctx = caller.data().host.ctx;
    let Some(cb) = cb else {
        return Err(wasmi::Error::new(
            "global state host functions are not available in this build",
        ));
    };
    let mut err = [0u8; ERR_BUF_LEN];
    let ret = unsafe { cb(ctx, key.as_ptr(), key.len(), err.as_mut_ptr(), err.len()) };
    if ret != 0 {
        return Err(wasmi::Error::new(format!(
            "global state delete error: {}",
            cstr(&err)
        )));
    }
    Ok(())
}

/// Implements the AVM's `ed25519verify_bare`. The three operands are copied out
/// of guest memory and handed to Go, which performs the verification, so the
/// opcode matches the AVM exactly.
fn ed25519verify_bare(
    mut caller: Caller<'_, HostState>,
    data_ptr: i32,
    data_len: i32,
    sig_ptr: i32,
    sig_len: i32,
    pk_ptr: i32,
    pk_len: i32,
) -> Result<i32, wasmi::Error> {
    // Charge the opcode's cost before doing any host work, including the reads.
    let remaining = caller.get_fuel()?;
    let remaining = remaining
        .checked_sub(ED25519_VERIFY_BARE_FUEL_COST)
        .ok_or_else(|| wasmi::Error::from(TrapCode::OutOfFuel))?;
    caller.set_fuel(remaining)?;

    let cb = caller.data().host.ed25519_verify_bare;
    let Some(cb) = cb else {
        return Err(wasmi::Error::new(
            "ed25519verify_bare is not available in this build",
        ));
    };

    let data = read_guest_bytes(&mut caller, data_ptr, data_len)?;
    let sig = read_guest_bytes(&mut caller, sig_ptr, sig_len)?;
    let pk = read_guest_bytes(&mut caller, pk_ptr, pk_len)?;

    let mut err = [0u8; ERR_BUF_LEN];
    let ret = unsafe {
        cb(
            data.as_ptr(),
            data.len(),
            sig.as_ptr(),
            sig.len(),
            pk.as_ptr(),
            pk.len(),
            err.as_mut_ptr(),
            err.len(),
        )
    };
    match ret {
        1 => Ok(1),
        0 => Ok(0),
        _ => Err(wasmi::Error::new(format!(
            "ed25519verify_bare error: {}",
            cstr(&err)
        ))),
    }
}

/// Result of a single guest invocation.
#[derive(Debug)]
struct Outcome {
    pass: bool,
    fuel_used: u64,
    accessed: [u64; ARGS_READ_WORDS],
}

fn store_limits(memory_limit_bytes: usize) -> StoreLimits {
    StoreLimitsBuilder::new()
        .memory_size(memory_limit_bytes)
        // Size limits are per memory/table, so count limits are also needed
        // to bound aggregate guest allocation. These apply before start runs
        // as well as to subsequent memory.grow/table.grow instructions.
        .memories(1)
        .table_elements(MAX_TABLE_ELEMENTS)
        .tables(1)
        .instances(1)
        .build()
}

fn new_engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    // Floats are disabled: their NaN bit patterns are not stable enough to
    // reason about across platforms, and no LogicSig needs them yet.
    config.floats(false);
    config.set_max_stack_height(1 << 16);
    config.set_max_recursion_depth(1024);
    Engine::new(&config)
}

/// Loads and instantiates `wasm`, returning the store, instance, and `program`
/// export. The module's `start` function, if any, runs here.
fn instantiate(
    wasm: &[u8],
    args: Vec<Vec<u8>>,
    memory_limit_bytes: usize,
    fuel: u64,
    host: HostCallbacks,
) -> Result<(Store<HostState>, Instance, TypedFunc<(), i32>), String> {
    let engine = new_engine();
    let module = Module::new(&engine, wasm).map_err(|err| format!("invalid wasm module: {err}"))?;

    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("env", "arg_count", arg_count)
        .map_err(|err| format!("could not link env.arg_count: {err}"))?;
    linker
        .func_wrap("env", "arg", arg)
        .map_err(|err| format!("could not link env.arg: {err}"))?;
    linker
        .func_wrap("env", "txn_uint", txn_uint)
        .map_err(|err| format!("could not link env.txn_uint: {err}"))?;
    linker
        .func_wrap("env", "txn_bytes", txn_bytes)
        .map_err(|err| format!("could not link env.txn_bytes: {err}"))?;
    linker
        .func_wrap("env", "txna_uint", txna_uint)
        .map_err(|err| format!("could not link env.txna_uint: {err}"))?;
    linker
        .func_wrap("env", "txna_bytes", txna_bytes)
        .map_err(|err| format!("could not link env.txna_bytes: {err}"))?;
    linker
        .func_wrap("env", "app_global_get_uint", app_global_get_uint)
        .map_err(|err| format!("could not link env.app_global_get_uint: {err}"))?;
    linker
        .func_wrap("env", "app_global_get_bytes", app_global_get_bytes)
        .map_err(|err| format!("could not link env.app_global_get_bytes: {err}"))?;
    linker
        .func_wrap("env", "app_global_put_uint", app_global_put_uint)
        .map_err(|err| format!("could not link env.app_global_put_uint: {err}"))?;
    linker
        .func_wrap("env", "app_global_put_bytes", app_global_put_bytes)
        .map_err(|err| format!("could not link env.app_global_put_bytes: {err}"))?;
    linker
        .func_wrap("env", "app_global_del", app_global_del)
        .map_err(|err| format!("could not link env.app_global_del: {err}"))?;
    linker
        .func_wrap("env", "ed25519verify_bare", ed25519verify_bare)
        .map_err(|err| format!("could not link env.ed25519verify_bare: {err}"))?;

    let mut store = Store::new(
        &engine,
        HostState {
            args,
            accessed: [0; ARGS_READ_WORDS],
            limits: store_limits(memory_limit_bytes),
            host,
        },
    );
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(fuel)
        .map_err(|err| format!("could not set fuel: {err}"))?;

    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|err| format!("could not instantiate wasm module: {err}"))?;
    let program = instance
        .get_typed_func::<(), i32>(&store, "program")
        .map_err(|err| format!("missing or malformed `program` export: {err}"))?;

    Ok((store, instance, program))
}

fn func_type_is(ty: &ExternType, params: &[ValType], results: &[ValType]) -> bool {
    match ty {
        ExternType::Func(ft) => ft.params() == params && ft.results() == results,
        _ => false,
    }
}

/// Validates a WASM program's structure without executing it: the module must
/// import only the host functions available in `env`, and must export `program`
/// with type `() -> i32`.
fn validate_inner(wasm: &[u8]) -> Result<(), String> {
    let engine = new_engine();
    let module = Module::new(&engine, wasm).map_err(|err| format!("invalid wasm module: {err}"))?;

    for import in module.imports() {
        let ok = import.module() == "env"
            && match import.name() {
                "arg_count" => func_type_is(import.ty(), &[], &[ValType::I32]),
                "arg" => func_type_is(import.ty(), &[ValType::I32; 3], &[ValType::I32]),
                "txn_uint" => func_type_is(import.ty(), &[ValType::I32], &[ValType::I64]),
                "txn_bytes" => func_type_is(import.ty(), &[ValType::I32; 3], &[ValType::I32]),
                "txna_uint" => func_type_is(import.ty(), &[ValType::I32; 2], &[ValType::I64]),
                "txna_bytes" => func_type_is(import.ty(), &[ValType::I32; 4], &[ValType::I32]),
                "app_global_get_uint" => {
                    func_type_is(import.ty(), &[ValType::I32; 2], &[ValType::I64])
                }
                "app_global_get_bytes" => {
                    func_type_is(import.ty(), &[ValType::I32; 4], &[ValType::I32])
                }
                "app_global_put_uint" => func_type_is(
                    import.ty(),
                    &[ValType::I32, ValType::I32, ValType::I64],
                    &[],
                ),
                "app_global_put_bytes" => {
                    func_type_is(import.ty(), &[ValType::I32; 4], &[])
                }
                "app_global_del" => func_type_is(import.ty(), &[ValType::I32; 2], &[]),
                "ed25519verify_bare" => {
                    func_type_is(import.ty(), &[ValType::I32; 6], &[ValType::I32])
                }
                _ => false,
            };
        if !ok {
            return Err(format!(
                "unsupported import {}::{}",
                import.module(),
                import.name()
            ));
        }
    }

    let mut has_program = false;
    for export in module.exports() {
        if export.name() == "program" {
            if !func_type_is(export.ty(), &[], &[ValType::I32]) {
                return Err("`program` export must have type () -> i32".to_string());
            }
            has_program = true;
        }
    }
    if !has_program {
        return Err("missing `program` export".to_string());
    }
    Ok(())
}

/// Runs a WASM LogicSig to completion.
fn run_inner(
    wasm: &[u8],
    args: Vec<Vec<u8>>,
    fuel: u64,
    host: HostCallbacks,
) -> Result<Outcome, String> {
    if args.len() > MAX_ARGS {
        return Err(format!("too many LogicSig args: {}", args.len()));
    }

    let (mut store, _instance, program) =
        instantiate(wasm, args, DEFAULT_MEMORY_LIMIT, fuel, host)?;

    let result = program.call(&mut store, ());
    let pass = match result {
        Ok(value) => value != 0,
        Err(err) => {
            if err.as_trap_code() == Some(TrapCode::OutOfFuel) {
                return Err("wasm program ran out of fuel".to_string());
            }
            return Err(format!("wasm program trapped: {err}"));
        }
    };

    let remaining = store
        .get_fuel()
        .map_err(|err| format!("could not read remaining fuel: {err}"))?;
    let accessed = store.data().accessed;

    Ok(Outcome {
        pass,
        fuel_used: fuel.saturating_sub(remaining),
        accessed,
    })
}

/// Writes a NUL-terminated error message into a caller-provided buffer. The
/// buffer is always NUL-terminated when at least one byte is available.
unsafe fn write_error(err_buf: *mut u8, err_buf_len: usize, msg: &str) {
    if err_buf.is_null() || err_buf_len == 0 {
        return;
    }
    let bytes = msg.as_bytes();
    let copy_len = bytes.len().min(err_buf_len - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), err_buf, copy_len);
    *err_buf.add(copy_len) = 0;
}

unsafe fn args_from_raw(args: *const WasmLsigArg, args_len: usize) -> Result<Vec<Vec<u8>>, String> {
    if args_len == 0 {
        return Ok(Vec::new());
    }
    if args.is_null() {
        return Err("null args pointer".to_string());
    }
    let raw = std::slice::from_raw_parts(args, args_len);
    let mut out = Vec::with_capacity(args_len);
    for entry in raw {
        if entry.ptr.is_null() && entry.len != 0 {
            return Err("null arg pointer".to_string());
        }
        let bytes = if entry.len == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(entry.ptr, entry.len).to_vec()
        };
        out.push(bytes);
    }
    Ok(out)
}

/// Runs a WASM LogicSig.
///
/// Returns `1` if the program approved, `0` if it rejected, and `-1` on any
/// error (invalid module, trap, out of fuel). On error a message is written to
/// `err_buf`. `fuel_used` receives the fuel consumed and `args_read` receives
/// `ARGS_READ_WORDS` words recording which arguments were read.
///
/// `txn_cb`, if non-null, is the host's provider for `txn` field reads,
/// `ed25519verify_bare_cb` its verifier, and `global_get_cb`/`global_put_cb`/
/// `global_del_cb` its global-state accessors; `ctx` is passed back to them
/// unchanged. `is_app` is non-zero for an application program, which disables
/// the `arg`/`arg_count` host functions (they are signature-only, as in the
/// AVM).
///
/// # Safety
///
/// `code`, `args`, `fuel_used`, and `args_read` must be valid for the lengths
/// given. `err_buf` must be valid for `err_buf_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn wasm_lsig_run(
    code: *const u8,
    code_len: usize,
    args: *const WasmLsigArg,
    args_len: usize,
    fuel: u64,
    fuel_used: *mut u64,
    args_read: *mut u64,
    err_buf: *mut u8,
    err_buf_len: usize,
    txn_cb: Option<TxnFieldCb>,
    ed25519verify_bare_cb: Option<Ed25519VerifyCb>,
    global_get_cb: Option<GlobalGetCb>,
    global_put_cb: Option<GlobalPutCb>,
    global_del_cb: Option<GlobalDelCb>,
    ctx: usize,
    is_app: u32,
) -> i32 {
    let host = HostCallbacks {
        txn_field: txn_cb,
        ed25519_verify_bare: ed25519verify_bare_cb,
        global_get: global_get_cb,
        global_put: global_put_cb,
        global_del: global_del_cb,
        ctx,
        is_app: is_app != 0,
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        if code.is_null() && code_len != 0 {
            return Err("null program pointer".to_string());
        }
        let wasm = std::slice::from_raw_parts(code, code_len);
        let args = args_from_raw(args, args_len)?;
        run_inner(wasm, args, fuel, host)
    }));

    match result {
        Ok(Ok(outcome)) => {
            if !fuel_used.is_null() {
                *fuel_used = outcome.fuel_used;
            }
            if !args_read.is_null() {
                std::ptr::copy_nonoverlapping(
                    outcome.accessed.as_ptr(),
                    args_read,
                    ARGS_READ_WORDS,
                );
            }
            if outcome.pass {
                1
            } else {
                0
            }
        }
        Ok(Err(msg)) => {
            write_error(err_buf, err_buf_len, &msg);
            -1
        }
        Err(_) => {
            write_error(err_buf, err_buf_len, "internal panic in wasm executor");
            -1
        }
    }
}

/// Validates a WASM LogicSig without running it.
///
/// Returns `0` if the module is valid and `-1` otherwise, writing a message to
/// `err_buf` on failure.
///
/// # Safety
///
/// `code` must be valid for `code_len` bytes and `err_buf` valid for
/// `err_buf_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn wasm_lsig_validate(
    code: *const u8,
    code_len: usize,
    err_buf: *mut u8,
    err_buf_len: usize,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if code.is_null() && code_len != 0 {
            return Err("null program pointer".to_string());
        }
        let wasm = std::slice::from_raw_parts(code, code_len);
        validate_inner(wasm)
    }));

    match result {
        Ok(Ok(())) => 0,
        Ok(Err(msg)) => {
            write_error(err_buf, err_buf_len, &msg);
            -1
        }
        Err(_) => {
            write_error(err_buf, err_buf_len, "internal panic in wasm validator");
            -1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_wat(wat: &str, args: Vec<Vec<u8>>, fuel: u64) -> Result<Outcome, String> {
        let wasm = wat::parse_str(wat).expect("valid wat");
        run_inner(&wasm, args, fuel, HostCallbacks::default())
    }

    // Field indices used by `fake_txn_field`, matching the Go `TxnField` enum.
    const TXN_FIELD_SENDER: u32 = 0;
    const TXN_FIELD_FEE: u32 = 1;

    // fake_txn_field stands in for the Go callback: Sender is 32 bytes of 7,
    // Fee is the uint 42, Note is empty, and everything else is an error.
    unsafe extern "C" fn fake_txn_field(
        ctx: usize,
        field: u32,
        _index: u32,
        _expect_array: u32,
        buf: *mut u8,
        buf_cap: usize,
        out_len: *mut usize,
        is_bytes: *mut u8,
        err: *mut u8,
        err_cap: usize,
    ) -> i32 {
        if ctx != 0 {
            // Tests can supply a per-execution callback counter.
            *(ctx as *mut usize) += 1;
        }
        match field {
            TXN_FIELD_FEE => {
                *is_bytes = 0;
                *out_len = 8;
                if buf_cap < 8 {
                    return -2;
                }
                let val: u64 = 42;
                std::ptr::copy_nonoverlapping(val.to_le_bytes().as_ptr(), buf, 8);
                0
            }
            TXN_FIELD_SENDER => {
                *is_bytes = 1;
                *out_len = 32;
                if buf_cap < 32 {
                    return -2;
                }
                std::ptr::write_bytes(buf, 7, 32);
                0
            }
            5 => {
                // Empty Note, including its byte-string type.
                *is_bytes = 1;
                *out_len = 0;
                0
            }
            _ => {
                if !err.is_null() && err_cap >= 2 {
                    std::ptr::copy_nonoverlapping(b"x\0".as_ptr(), err, 2);
                }
                -1
            }
        }
    }

    fn run_wat_with_txn(wat: &str, fuel: u64) -> Result<Outcome, String> {
        let wasm = wat::parse_str(wat).expect("valid wat");
        run_inner(
            &wasm,
            vec![],
            fuel,
            HostCallbacks {
                txn_field: Some(fake_txn_field),
                ..Default::default()
            },
        )
    }

    // fake_ed25519_verify_bare stands in for the Go verifier: it approves iff
    // data is `DATA`, the signature is 64 bytes of 3, and the key is 32 bytes of
    // 4. Any other length reports the same errors as the AVM.
    const ED25519_DATA: &[u8] = b"message";

    unsafe extern "C" fn fake_ed25519_verify_bare(
        data: *const u8,
        data_len: usize,
        sig: *const u8,
        sig_len: usize,
        pk: *const u8,
        pk_len: usize,
        err: *mut u8,
        err_cap: usize,
    ) -> i32 {
        let data = std::slice::from_raw_parts(data, data_len);
        if sig_len != 64 {
            let msg = b"invalid signature\0";
            if !err.is_null() {
                std::ptr::copy_nonoverlapping(msg.as_ptr(), err, msg.len().min(err_cap));
            }
            return -1;
        }
        if pk_len != 32 {
            let msg = b"invalid public key\0";
            if !err.is_null() {
                std::ptr::copy_nonoverlapping(msg.as_ptr(), err, msg.len().min(err_cap));
            }
            return -1;
        }
        let sig = std::slice::from_raw_parts(sig, sig_len);
        let pk = std::slice::from_raw_parts(pk, pk_len);
        if data == ED25519_DATA && sig.iter().all(|&b| b == 3) && pk.iter().all(|&b| b == 4) {
            1
        } else {
            0
        }
    }

    fn run_wat_with_ed25519(wat: &str, fuel: u64) -> Result<Outcome, String> {
        let wasm = wat::parse_str(wat).expect("valid wat");
        run_inner(
            &wasm,
            vec![],
            fuel,
            HostCallbacks {
                ed25519_verify_bare: Some(fake_ed25519_verify_bare),
                ..Default::default()
            },
        )
    }

    /// Builds a program that calls `ed25519verify_bare` over `data`, a 64-byte
    /// signature, and a 32-byte public key built from the given fill bytes.
    fn ed25519_wat(sig_byte: u8, pk_byte: u8) -> String {
        let sig = format!("\\{sig_byte:02x}").repeat(64);
        let pk = format!("\\{pk_byte:02x}").repeat(32);
        format!(
            r#"(module
                (import "env" "ed25519verify_bare" (func $verify (param i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "message")
                (data (i32.const 100) "{sig}")
                (data (i32.const 200) "{pk}")
                (func (export "program") (result i32)
                    (call $verify (i32.const 0) (i32.const 7)
                                  (i32.const 100) (i32.const 64)
                                  (i32.const 200) (i32.const 32))))"#
        )
    }

    #[test]
    fn approves_when_program_returns_one() {
        let out = run_wat(
            r#"(module (func (export "program") (result i32) i32.const 1))"#,
            vec![],
            1000,
        )
        .unwrap();
        assert!(out.pass);
    }

    #[test]
    fn rejects_when_program_returns_zero() {
        let out = run_wat(
            r#"(module (func (export "program") (result i32) i32.const 0))"#,
            vec![],
            1000,
        )
        .unwrap();
        assert!(!out.pass);
    }

    #[test]
    fn reads_args() {
        let wat = r#"
            (module
                (import "env" "arg_count" (func $arg_count (result i32)))
                (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "program") (result i32)
                    (i32.const 0) (i32.const 0) (i32.const 8) (call $arg) (drop)
                    (call $arg_count)
                )
            )
        "#;
        let out = run_wat(wat, vec![b"hello".to_vec()], 1000).unwrap();
        assert!(out.pass);
        assert_eq!(out.accessed, [1, 0, 0, 0]);
    }

    #[test]
    fn unread_args_are_reported() {
        let wat = r#"
            (module
                (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "program") (result i32)
                    ;; read arg 1, not arg 0
                    (i32.const 1) (i32.const 0) (i32.const 8) (call $arg) (drop)
                    (i32.const 1)
                )
            )
        "#;
        let out = run_wat(wat, vec![b"a".to_vec(), b"b".to_vec()], 1000).unwrap();
        assert_eq!(out.accessed, [0b10, 0, 0, 0]);
    }

    #[test]
    fn invalid_arg_buffers_do_not_write_or_mark_read() {
        for (index, ptr, len, bytes, expected) in [
            (0, -1, 1, &b"x"[..], ARG_ERR_MEMORY),
            (0, i32::MIN, 1, &b"x"[..], ARG_ERR_MEMORY),
            (0, 0, -1, &b"x"[..], ARG_ERR_BUFFER),
            (0, 0, -1, &b""[..], ARG_ERR_BUFFER),
            (0, -1, 0, &b""[..], ARG_ERR_MEMORY),
            (0, 0, 0, &b"x"[..], ARG_ERR_BUFFER),
            (0, 65536, 1, &b"x"[..], ARG_ERR_MEMORY),
            (0, 65535, 2, &b"xx"[..], ARG_ERR_MEMORY),
            (-1, 0, 1, &b"x"[..], ARG_ERR_INDEX),
            (1, 0, 1, &b"x"[..], ARG_ERR_INDEX),
        ] {
            let wat = format!(
                r#"(module
                    (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "*")
                    (func (export "program") (result i32)
                        (i32.eq (call $arg (i32.const {index}) (i32.const {ptr}) (i32.const {len}))
                                (i32.const {expected}))
                        (i32.eq (i32.load8_u (i32.const 0)) (i32.const 42))
                        i32.and))"#
            );
            let out = run_wat(&wat, vec![bytes.to_vec()], 1000).unwrap();
            assert!(
                out.pass,
                "index={index}, ptr={ptr}, len={len}, bytes={bytes:?}"
            );
            assert_eq!(out.accessed, [0; ARGS_READ_WORDS]);
        }
    }

    #[test]
    fn arg_writes_at_memory_boundary() {
        for (ptr, bytes) in [(65535, &b"x"[..]), (65536, &b""[..])] {
            let len = bytes.len();
            let wat = format!(
                r#"(module
                    (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (func (export "program") (result i32)
                        (i32.eq (call $arg (i32.const 0) (i32.const {ptr}) (i32.const {len}))
                                (i32.const {len}))))"#
            );
            let out = run_wat(&wat, vec![bytes.to_vec()], 1000).unwrap();
            assert!(out.pass);
            assert_eq!(out.accessed, [1, 0, 0, 0]);
        }
    }

    #[test]
    fn instantiation_limits_guest_allocation() {
        let cases = [
            "(table 1000000 funcref)".to_string(),
            "(table 1025 funcref)".to_string(),
            "(table 1 funcref) (table 1 funcref)".to_string(),
            format!("(memory {})", DEFAULT_MEMORY_PAGES + 1),
            "(memory 1) (memory 1)".to_string(),
        ];
        for resources in cases {
            let wat = format!(
                r#"(module {resources}
                    (func (export "program") (result i32) i32.const 1))"#
            );
            let err = run_wat(&wat, vec![], 1000).unwrap_err();
            assert!(err.contains("could not instantiate"), "{resources}: {err}");
        }
    }

    #[test]
    fn resource_limits_apply_to_growth_in_start_and_program() {
        let wat = format!(
            r#"(module
                (memory 1)
                (table {MAX_TABLE_ELEMENTS} funcref)
                (func $check_limits
                    (if (i32.ne (memory.grow (i32.const {DEFAULT_MEMORY_PAGES})) (i32.const -1))
                        (then unreachable))
                    (if (i32.ne (table.grow (ref.null func) (i32.const 1)) (i32.const -1))
                        (then unreachable)))
                (start $check_limits)
                (func (export "program") (result i32)
                    call $check_limits
                    i32.const 1))"#
        );
        let out = run_wat(&wat, vec![], 20000).unwrap();
        assert!(out.pass);
    }

    #[test]
    fn out_of_fuel_is_an_error() {
        let wat = r#"
            (module (func (export "program") (result i32)
                (loop $l (br $l))
                i32.const 1))
        "#;
        let err = run_wat(wat, vec![], 100).unwrap_err();
        assert!(err.contains("fuel"), "{err}");
    }

    #[test]
    fn reads_txn_uint() {
        let wat = r#"
            (module
                (import "env" "txn_uint" (func $txn_uint (param i32) (result i64)))
                (func (export "program") (result i32)
                    (i64.eq (call $txn_uint (i32.const 1)) (i64.const 42))))
        "#;
        let out = run_wat_with_txn(wat, 1000).unwrap();
        assert!(out.pass);
    }

    #[test]
    fn reads_txn_bytes() {
        let wat = r#"
            (module
                (import "env" "txn_bytes" (func $txn_bytes (param i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "program") (result i32)
                    (i32.and
                        (i32.eq (call $txn_bytes (i32.const 0) (i32.const 65504) (i32.const 32))
                                (i32.const 32))
                        (i32.and
                            (i32.eq (i32.load8_u (i32.const 65504)) (i32.const 7))
                            (i32.eq (i32.load8_u (i32.const 65535)) (i32.const 7))))))
        "#;
        let out = run_wat_with_txn(wat, 1000).unwrap();
        assert!(out.pass);
    }

    #[test]
    fn txn_bytes_query_reports_length() {
        let wat = r#"
            (module
                (import "env" "txn_bytes" (func $txn_bytes (param i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "program") (result i32)
                    (i32.eq (call $txn_bytes (i32.const 0) (i32.const 0) (i32.const 0))
                            (i32.const 32))))
        "#;
        let out = run_wat_with_txn(wat, 1000).unwrap();
        assert!(out.pass);
    }

    #[test]
    fn txn_bytes_reports_small_buffer() {
        let wat = r#"
            (module
                (import "env" "txn_bytes" (func $txn_bytes (param i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "program") (result i32)
                    (i32.eq (call $txn_bytes (i32.const 0) (i32.const 0) (i32.const 4))
                            (i32.const -2))))
        "#;
        let out = run_wat_with_txn(wat, 1000).unwrap();
        assert!(out.pass);
    }

    #[test]
    fn txn_uint_on_byte_field_traps() {
        let wat = r#"
            (module
                (import "env" "txn_uint" (func $txn_uint (param i32) (result i64)))
                (func (export "program") (result i32)
                    (i32.wrap_i64 (call $txn_uint (i32.const 0)))))
        "#;
        let err = run_wat_with_txn(wat, 1000).unwrap_err();
        assert!(err.contains("not a uint"), "{err}");
    }

    #[test]
    fn txn_bytes_on_uint_field_traps() {
        let wat = r#"
            (module
                (import "env" "txn_bytes" (func $txn_bytes (param i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "program") (result i32)
                    (call $txn_bytes (i32.const 1) (i32.const 0) (i32.const 32))))
        "#;
        let err = run_wat_with_txn(wat, 1000).unwrap_err();
        assert!(err.contains("not a byte string"), "{err}");
    }

    #[test]
    fn txn_invalid_field_traps() {
        let wat = r#"
            (module
                (import "env" "txn_uint" (func $txn_uint (param i32) (result i64)))
                (func (export "program") (result i32)
                    (i32.wrap_i64 (call $txn_uint (i32.const 999)))))
        "#;
        let err = run_wat_with_txn(wat, 1000).unwrap_err();
        assert!(err.contains("error"), "{err}");
    }

    #[test]
    fn txn_without_host_traps() {
        let wat = r#"
            (module
                (import "env" "txn_uint" (func $txn_uint (param i32) (result i64)))
                (func (export "program") (result i32)
                    (i32.wrap_i64 (call $txn_uint (i32.const 1)))))
        "#;
        let err = run_wat(wat, vec![], 1000).unwrap_err();
        assert!(err.contains("not available"), "{err}");
    }

    #[test]
    fn txn_calls_host_once() {
        for body in [
            "(drop (call $uint (i32.const 1)))",
            "(drop (call $bytes (i32.const 0) (i32.const 0) (i32.const 32)))",
            "(drop (call $bytes (i32.const 0) (i32.const -1) (i32.const 0)))",
            "(drop (call $bytes (i32.const 5) (i32.const 0) (i32.const 32)))",
        ] {
            let wasm = wat::parse_str(format!(
                r#"
                (module
                    (import "env" "txn_uint" (func $uint (param i32) (result i64)))
                    (import "env" "txn_bytes" (func $bytes (param i32 i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (func (export "program") (result i32) {body} (i32.const 1)))
            "#
            ))
            .unwrap();
            let mut calls = 0usize;
            let (mut store, _, program) = instantiate(
                &wasm,
                vec![],
                DEFAULT_MEMORY_LIMIT,
                1000,
                HostCallbacks {
                    txn_field: Some(fake_txn_field),
                    ctx: (&mut calls as *mut usize) as usize,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(program.call(&mut store, ()).unwrap(), 1);
            assert_eq!(calls, 1);
        }
    }

    #[test]
    fn txn_charges_one_host_fuel_before_work() {
        let wasm = wat::parse_str(r#"(module (func (export "program") (result i32) i32.const 1))"#)
            .unwrap();
        let mut calls = 0usize;
        let (mut store, _, _) = instantiate(
            &wasm,
            vec![],
            DEFAULT_MEMORY_LIMIT,
            1000,
            HostCallbacks {
                txn_field: Some(fake_txn_field),
                ctx: (&mut calls as *mut usize) as usize,
                ..Default::default()
            },
        )
        .unwrap();

        // Direct host calls isolate the charge from Wasm instruction fuel.
        let uint = wasmi::Func::wrap(&mut store, txn_uint)
            .typed::<i32, i64>(&store)
            .unwrap();
        store.set_fuel(1).unwrap();
        assert_eq!(uint.call(&mut store, 1).unwrap(), 42);
        assert_eq!(store.get_fuel().unwrap(), 0);
        assert_eq!(calls, 1);
        let err = uint.call(&mut store, 1).unwrap_err();
        assert_eq!(err.as_trap_code(), Some(TrapCode::OutOfFuel));
        assert_eq!(calls, 1);

        let bytes = wasmi::Func::wrap(&mut store, txn_bytes)
            .typed::<(i32, i32, i32), i32>(&store)
            .unwrap();
        for (len, expected) in [(0, 32), (-1, TXN_ERR_MEMORY)] {
            store.set_fuel(1).unwrap();
            let before = calls;
            assert_eq!(bytes.call(&mut store, (0, 0, len)).unwrap(), expected);
            assert_eq!(store.get_fuel().unwrap(), 0);
            assert_eq!(calls, before + 1);
            let err = bytes.call(&mut store, (0, 0, len)).unwrap_err();
            assert_eq!(err.as_trap_code(), Some(TrapCode::OutOfFuel));
            assert_eq!(calls, before + 1);
        }
        // Even a field rejected before invoking Go pays the host cost.
        store.set_fuel(1).unwrap();
        let before = calls;
        assert!(uint.call(&mut store, -1).is_err());
        assert_eq!(store.get_fuel().unwrap(), 0);
        assert_eq!(calls, before);
    }

    #[test]
    fn txn_bytes_empty_and_invalid_buffers() {
        for (field, ptr, len, expected, memory) in [
            (5, 0, 32, 0, true),
            (5, -1, 0, 0, false),
            (0, -1, 32, TXN_ERR_MEMORY, true),
            (0, 0, -1, TXN_ERR_MEMORY, true),
            (0, 65505, 32, TXN_ERR_MEMORY, true),
            (0, 0, 32, TXN_ERR_MEMORY, false),
            (0, 0, 31, TXN_ERR_BUFFER, true),
        ] {
            let memory = if memory {
                r#"(memory (export "memory") 1)
                   (data (i32.const 0) "\09")
                   (data (i32.const 65535) "\09")"#
            } else {
                ""
            };
            // Invalid writes and empty values must leave guest memory intact,
            // including an out-of-bounds write's in-bounds prefix.
            let unchanged = if memory.is_empty() {
                "(i32.const 1)"
            } else {
                "(i32.and (i32.eq (i32.load8_u (i32.const 0)) (i32.const 9))
                          (i32.eq (i32.load8_u (i32.const 65535)) (i32.const 9)))"
            };
            let wat = format!(
                r#"
                (module
                    (import "env" "txn_bytes" (func $bytes (param i32 i32 i32) (result i32)))
                    {memory}
                    (func (export "program") (result i32)
                        (i32.and
                            (i32.eq (call $bytes (i32.const {field}) (i32.const {ptr}) (i32.const {len}))
                                (i32.const {expected}))
                            {unchanged})))
            "#
            );
            assert!(run_wat_with_txn(&wat, 1000).unwrap().pass);
        }
    }

    #[test]
    fn ed25519verify_bare_approves_and_rejects() {
        assert!(
            run_wat_with_ed25519(&ed25519_wat(3, 4), 100000)
                .unwrap()
                .pass
        );
        // The verifier returns 0 for a signature that does not match, which the
        // program reports as a rejection rather than an error.
        assert!(
            !run_wat_with_ed25519(&ed25519_wat(9, 4), 100000)
                .unwrap()
                .pass
        );
    }

    #[test]
    fn ed25519verify_bare_charges_opcode_cost() {
        // Just enough fuel for the call itself, not for the preceding
        // instructions, so the program must run out.
        let err = run_wat_with_ed25519(&ed25519_wat(3, 4), ED25519_VERIFY_BARE_FUEL_COST);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("fuel"));
    }

    #[test]
    fn ed25519verify_bare_bad_signature_length_traps() {
        let wat = r#"(module
            (import "env" "ed25519verify_bare" (func $verify (param i32 i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "program") (result i32)
                (call $verify (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 63)
                              (i32.const 0) (i32.const 32))))"#;
        let err = run_wat_with_ed25519(wat, 100000).unwrap_err();
        assert!(err.contains("invalid signature"), "{err}");
    }

    #[test]
    fn ed25519verify_bare_out_of_bounds_read_traps() {
        let wat = r#"(module
            (import "env" "ed25519verify_bare" (func $verify (param i32 i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "program") (result i32)
                (call $verify (i32.const 65530) (i32.const 10) (i32.const 0) (i32.const 64)
                              (i32.const 0) (i32.const 32))))"#;
        let err = run_wat_with_ed25519(wat, 100000).unwrap_err();
        assert!(err.contains("outside linear memory"), "{err}");
    }

    #[test]
    fn ed25519verify_bare_without_host_traps() {
        let err = run_wat(&ed25519_wat(3, 4), vec![], 100000).unwrap_err();
        assert!(err.contains("not available"), "{err}");
    }

    #[test]
    fn invalid_module_is_rejected() {
        assert!(validate_inner(b"not wasm").is_err());
    }

    #[test]
    fn validation_requires_program_export() {
        let wasm =
            wat::parse_str(r#"(module (func (export "other") (result i32) i32.const 1))"#).unwrap();
        assert!(validate_inner(&wasm).is_err());
    }

    #[test]
    fn validation_checks_program_signature() {
        let wasm = wat::parse_str(r#"(module (func (export "program") (result i64) i64.const 1))"#)
            .unwrap();
        assert!(validate_inner(&wasm).is_err());
    }

    #[test]
    fn validation_rejects_unknown_import() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "bogus" (func (result i32)))
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_err());
    }

    #[test]
    fn validation_accepts_args_host() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "arg_count" (func (result i32)))
                (import "env" "arg" (func (param i32 i32 i32) (result i32)))
                (import "env" "txn_uint" (func (param i32) (result i64)))
                (import "env" "txn_bytes" (func (param i32 i32 i32) (result i32)))
                (import "env" "ed25519verify_bare" (func (param i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_ok());
    }

    #[test]
    fn validation_rejects_bad_txn_signature() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "txn_uint" (func (param i32) (result i32)))
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_err());
    }

    #[test]
    fn validation_rejects_bad_ed25519_signature() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "ed25519verify_bare" (func (param i32 i32 i32 i32 i32) (result i32)))
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_err());
    }

    #[test]
    fn validation_accepts_txna() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "txna_uint" (func (param i32 i32) (result i64)))
                (import "env" "txna_bytes" (func (param i32 i32 i32 i32) (result i32)))
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_ok());
    }

    #[test]
    fn validation_rejects_bad_txna_signature() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "txna_uint" (func (param i32) (result i64)))
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_err());
    }

    #[test]
    fn txna_bytes_passes_index_and_array_flag() {
        // A callback that records the call's field/index/array flag and returns
        // a four-byte value.
        unsafe extern "C" fn cb(
            ctx: usize,
            field: u32,
            index: u32,
            expect_array: u32,
            buf: *mut u8,
            buf_cap: usize,
            out_len: *mut usize,
            is_bytes: *mut u8,
            _err: *mut u8,
            _err_cap: usize,
        ) -> i32 {
            let rec = &mut *(ctx as *mut (u32, u32, u32));
            *rec = (field, index, expect_array);
            *is_bytes = 1;
            *out_len = 4;
            if buf_cap < 4 {
                return -2;
            }
            std::ptr::copy_nonoverlapping([9u8; 4].as_ptr(), buf, 4);
            0
        }

        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "txna_bytes" (func $txna (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "program") (result i32)
                    (i32.eq (call $txna (i32.const 5) (i32.const 3) (i32.const 0) (i32.const 4))
                            (i32.const 4))))
            "#,
        )
        .unwrap();

        let mut rec = (0u32, 0u32, 0u32);
        let outcome = run_inner(
            &wasm,
            vec![],
            1000,
            HostCallbacks {
                txn_field: Some(cb),
                ctx: (&mut rec as *mut (u32, u32, u32)) as usize,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(outcome.pass);
        assert_eq!(rec, (5, 3, 1));
    }

    #[test]
    fn arg_is_unavailable_in_app_mode() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "arg_count" (func $arg_count (result i32)))
                (func (export "program") (result i32) (call $arg_count)))
            "#,
        )
        .unwrap();
        let err = run_inner(
            &wasm,
            vec![vec![1, 2, 3]],
            1000,
            HostCallbacks {
                is_app: true,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.contains("application mode"), "{err}");
    }

    // A stand-in for the Go global-state reader: key "k" is the uint 42, and any
    // other key is absent.
    unsafe extern "C" fn fake_global_get(
        _ctx: usize,
        key: *const u8,
        key_len: usize,
        buf: *mut u8,
        buf_cap: usize,
        out_len: *mut usize,
        out_is_bytes: *mut u8,
        out_exists: *mut u8,
        _err: *mut u8,
        _err_cap: usize,
    ) -> i32 {
        let key = std::slice::from_raw_parts(key, key_len);
        if key == b"k" {
            *out_is_bytes = 0;
            *out_len = 8;
            *out_exists = 1;
            if buf_cap < 8 {
                return -2;
            }
            std::ptr::copy_nonoverlapping(42u64.to_le_bytes().as_ptr(), buf, 8);
        } else {
            *out_is_bytes = 0;
            *out_len = 0;
            *out_exists = 0;
        }
        0
    }

    fn run_wat_with_global_get(wat: &str) -> Result<Outcome, String> {
        let wasm = wat::parse_str(wat).expect("valid wat");
        run_inner(
            &wasm,
            vec![],
            1000,
            HostCallbacks {
                global_get: Some(fake_global_get),
                is_app: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn global_get_uint_reads_value() {
        let out = run_wat_with_global_get(
            r#"
            (module
                (import "env" "app_global_get_uint" (func $get (param i32 i32) (result i64)))
                (memory (export "memory") 1)
                (data (i32.const 0) "k")
                (func (export "program") (result i32)
                    (i64.eq (call $get (i32.const 0) (i32.const 1)) (i64.const 42))))
            "#,
        )
        .unwrap();
        assert!(out.pass);
    }

    #[test]
    fn global_get_uint_absent_is_zero() {
        let out = run_wat_with_global_get(
            r#"
            (module
                (import "env" "app_global_get_uint" (func $get (param i32 i32) (result i64)))
                (memory (export "memory") 1)
                (data (i32.const 0) "other")
                (func (export "program") (result i32)
                    (i64.eqz (call $get (i32.const 0) (i32.const 5)))))
            "#,
        )
        .unwrap();
        assert!(out.pass);
    }

    #[test]
    fn global_state_traps_in_signature_mode() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "app_global_get_uint" (func $get (param i32 i32) (result i64)))
                (memory (export "memory") 1)
                (data (i32.const 0) "k")
                (func (export "program") (result i32)
                    (i64.eqz (call $get (i32.const 0) (i32.const 1)))))
            "#,
        )
        .unwrap();
        let err = run_inner(&wasm, vec![], 1000, HostCallbacks::default()).unwrap_err();
        assert!(err.contains("signature mode"), "{err}");
    }

    // A stand-in for the Go global-state writer: records `(val_is_uint, value)`.
    unsafe extern "C" fn fake_global_put(
        ctx: usize,
        _key: *const u8,
        _key_len: usize,
        val: *const u8,
        val_len: usize,
        val_is_uint: u32,
        _err: *mut u8,
        _err_cap: usize,
    ) -> i32 {
        let rec = &mut *(ctx as *mut (u32, u64));
        let bytes: [u8; 8] = std::slice::from_raw_parts(val, val_len).try_into().unwrap();
        *rec = (val_is_uint, u64::from_le_bytes(bytes));
        0
    }

    #[test]
    fn global_put_uint_passes_value() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
                (memory (export "memory") 1)
                (data (i32.const 0) "k")
                (func (export "program") (result i32)
                    (call $put (i32.const 0) (i32.const 1) (i64.const 7))
                    (i32.const 1)))
            "#,
        )
        .unwrap();
        let mut rec = (0u32, 0u64);
        let out = run_inner(
            &wasm,
            vec![],
            1000,
            HostCallbacks {
                global_put: Some(fake_global_put),
                ctx: (&mut rec as *mut (u32, u64)) as usize,
                is_app: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(out.pass);
        assert_eq!(rec, (1, 7));
    }

    #[test]
    fn validation_accepts_global_state() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "app_global_get_uint" (func (param i32 i32) (result i64)))
                (import "env" "app_global_get_bytes" (func (param i32 i32 i32 i32) (result i32)))
                (import "env" "app_global_put_uint" (func (param i32 i32 i64)))
                (import "env" "app_global_put_bytes" (func (param i32 i32 i32 i32)))
                (import "env" "app_global_del" (func (param i32 i32)))
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_ok());
    }

    #[test]
    fn validation_rejects_bad_global_signature() {
        let wasm = wat::parse_str(
            r#"
            (module
                (import "env" "app_global_get_uint" (func (param i32) (result i64)))
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_err());
    }
}
