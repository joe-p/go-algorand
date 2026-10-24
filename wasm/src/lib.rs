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
//!
//! The transaction fields come from the host through `TxnFieldCb`, so the AVM's
//! field semantics (typing, version gating, allowed modes) have a single
//! source of truth in Go. Byte fields have the AVM's 4096-byte limit. Each txn
//! host invocation costs one additional fuel unit before performing host work.
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

/// Default cap on a guest's linear memory, in bytes.
const DEFAULT_MEMORY_LIMIT: usize = 64 * 1024;

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

/// Size of the error-message buffer shared with the host callback.
const TXN_ERR_BUF_LEN: usize = 256;

/// Must match Go's config.MaxAVMBytesSize. Go also enforces this limit.
const MAX_TXN_BYTES: usize = 4096;

/// Additional fuel charged per txn host invocation, including length queries.
const TXN_FUEL_COST: u64 = 1;

/// A transaction field provider, implemented in Go. `ctx` is an opaque handle
/// that Go maps back to the evaluation context; Rust never dereferences it.
///
/// The callback writes the field's bytes into `buf` (up to `buf_cap`),
/// reporting the required length in `out_len` and whether the field is a byte
/// string in `is_bytes` (0 = uint, non-zero = bytes). It returns `0` on
/// success, `-1` on error (a message is written to `err`), and `-2` when `buf`
/// is too small (the required length is still written to `out_len`).
type TxnFieldCb = unsafe extern "C" fn(
    ctx: usize,
    field: u32,
    buf: *mut u8,
    buf_cap: usize,
    out_len: *mut usize,
    is_bytes: *mut u8,
    err: *mut u8,
    err_cap: usize,
) -> i32;

/// A single LogicSig argument passed across the C ABI as a pointer/length pair.
#[repr(C)]
pub struct WasmLsigArg {
    pub ptr: *const u8,
    pub len: usize,
}

/// Store-level state visible to host functions.
struct HostState {
    args: Vec<Vec<u8>>,
    accessed: [u64; ARGS_READ_WORDS],
    limits: StoreLimits,
    /// Provider for `txn` field reads. `None` in tests, or if a guest is run
    /// without a transaction; calling a txn host function then traps.
    txn_cb: Option<TxnFieldCb>,
    /// Opaque handle passed back to `txn_cb`.
    txn_ctx: usize,
}

impl HostState {
    fn mark_read(&mut self, index: usize) {
        self.accessed[index / 64] |= 1 << (index % 64);
    }
}

fn arg_count(caller: Caller<'_, HostState>) -> i32 {
    caller.data().args.len() as i32
}

fn arg(mut caller: Caller<'_, HostState>, index: i32, out_ptr: i32, out_len: i32) -> i32 {
    if index < 0 {
        return ARG_ERR_INDEX;
    }
    let index = index as usize;

    // Clone the argument so the immutable borrow of the store ends before we
    // need a mutable one to write into linear memory.
    let bytes = match caller.data().args.get(index) {
        Some(arg) => arg.clone(),
        None => return ARG_ERR_INDEX,
    };

    if out_len < 0 || bytes.len() > out_len as usize {
        return ARG_ERR_BUFFER;
    }
    if out_ptr < 0 {
        return ARG_ERR_MEMORY;
    }

    let memory: Option<wasmi::Memory> = caller.get_export("memory").and_then(|e| e.into_memory());
    let Some(memory) = memory else {
        return ARG_ERR_MEMORY;
    };
    if memory.write(&mut caller, out_ptr as usize, &bytes).is_err() {
        return ARG_ERR_MEMORY;
    }

    caller.data_mut().mark_read(index);
    bytes.len() as i32
}

/// Reads a NUL-terminated message from a fixed error buffer.
fn cstr(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// Fetches a field once into a caller-owned buffer. A short buffer still
/// reports the type and required length. Invalid or unavailable fields trap.
fn fetch_txn_field(
    caller: &mut Caller<'_, HostState>,
    field: i32,
    buf: &mut [u8],
) -> Result<(bool, usize), wasmi::Error> {
    // Charge before doing any host work, including validation and callbacks.
    let remaining = caller.get_fuel()?;
    let remaining = remaining
        .checked_sub(TXN_FUEL_COST)
        .ok_or_else(|| wasmi::Error::from(TrapCode::OutOfFuel))?;
    caller.set_fuel(remaining)?;

    let cb = caller.data().txn_cb;
    let ctx = caller.data().txn_ctx;
    let Some(cb) = cb else {
        return Err(wasmi::Error::new(
            "txn host functions are not available in this build",
        ));
    };
    if field < 0 {
        return Err(wasmi::Error::new(format!("invalid txn field {field}")));
    }

    let mut err = [0u8; TXN_ERR_BUF_LEN];
    let mut len = 0usize;
    let mut is_bytes = 0u8;

    let ret = unsafe {
        cb(
            ctx,
            field as u32,
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

    if is_bytes != 0 && len > MAX_TXN_BYTES {
        return Err(wasmi::Error::new("txn byte field exceeds AVM size limit"));
    }
    Ok((is_bytes != 0, len))
}

fn txn_uint(mut caller: Caller<'_, HostState>, field: i32) -> Result<i64, wasmi::Error> {
    let mut bytes = [0u8; 8];
    let (is_bytes, len) = fetch_txn_field(&mut caller, field, &mut bytes)?;
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

fn txn_bytes(
    mut caller: Caller<'_, HostState>,
    field: i32,
    out_ptr: i32,
    out_len: i32,
) -> Result<i32, wasmi::Error> {
    let mut buf = [0u8; MAX_TXN_BYTES];
    // Length queries don't fetch or copy the field payload.
    let capacity = if out_len <= 0 {
        0
    } else {
        (out_len as usize).min(buf.len())
    };
    let (is_bytes, len) = fetch_txn_field(&mut caller, field, &mut buf[..capacity])?;
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
    txn_cb: Option<TxnFieldCb>,
    txn_ctx: usize,
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

    let mut store = Store::new(
        &engine,
        HostState {
            args,
            accessed: [0; ARGS_READ_WORDS],
            limits: store_limits(memory_limit_bytes),
            txn_cb,
            txn_ctx,
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

/// Validates a WASM LogicSig's structure without executing it: the module must
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
    txn_cb: Option<TxnFieldCb>,
    txn_ctx: usize,
) -> Result<Outcome, String> {
    if args.len() > MAX_ARGS {
        return Err(format!("too many LogicSig args: {}", args.len()));
    }

    let (mut store, _instance, program) =
        instantiate(wasm, args, DEFAULT_MEMORY_LIMIT, fuel, txn_cb, txn_ctx)?;

    let result = program.call(&mut store, ());
    let pass = match result {
        Ok(value) => value != 0,
        Err(err) => {
            if err.as_trap_code() == Some(TrapCode::OutOfFuel) {
                return Err("wasm LogicSig ran out of fuel".to_string());
            }
            return Err(format!("wasm LogicSig trapped: {err}"));
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
/// `txn_cb`, if non-null, is the host's provider for `txn` field reads; `txn_ctx`
/// is passed back to it unchanged.
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
    txn_ctx: usize,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if code.is_null() && code_len != 0 {
            return Err("null program pointer".to_string());
        }
        let wasm = std::slice::from_raw_parts(code, code_len);
        let args = args_from_raw(args, args_len)?;
        run_inner(wasm, args, fuel, txn_cb, txn_ctx)
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
        run_inner(&wasm, args, fuel, None, 0)
    }

    // Field indices used by `fake_txn_field`, matching the Go `TxnField` enum.
    const TXN_FIELD_SENDER: u32 = 0;
    const TXN_FIELD_FEE: u32 = 1;

    // fake_txn_field stands in for the Go callback: Sender is 32 bytes of 7,
    // Fee is the uint 42, Note is empty, and everything else is an error.
    unsafe extern "C" fn fake_txn_field(
        ctx: usize,
        field: u32,
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
        run_inner(&wasm, vec![], fuel, Some(fake_txn_field), 0)
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
        for resources in [
            "(table 1000000 funcref)",
            "(table 1025 funcref)",
            "(table 1 funcref) (table 1 funcref)",
            "(memory 2)",
            "(memory 1) (memory 1)",
        ] {
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
                    (if (i32.ne (memory.grow (i32.const 1)) (i32.const -1))
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
                Some(fake_txn_field),
                (&mut calls as *mut usize) as usize,
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
            Some(fake_txn_field),
            (&mut calls as *mut usize) as usize,
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
}
