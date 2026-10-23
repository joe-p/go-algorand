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
//! (distinguished from AVM bytecode by the `\0asm` magic prefix). For now the
//! guest program can only read the LogicSig arguments; in the future host
//! functions exposing opcodes, transaction fields, etc. will be added to the
//! same `env` module.
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

// Error sentinels returned by the `arg` host function.
const ARG_ERR_INDEX: i32 = -1;
const ARG_ERR_BUFFER: i32 = -2;
const ARG_ERR_MEMORY: i32 = -3;

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

    if bytes.len() > out_len.max(0) as usize {
        return ARG_ERR_BUFFER;
    }

    let memory: Option<wasmi::Memory> = caller.get_export("memory").and_then(|e| e.into_memory());
    let Some(memory) = memory else {
        return ARG_ERR_MEMORY;
    };
    if memory
        .write(&mut caller, out_ptr.max(0) as usize, &bytes)
        .is_err()
    {
        return ARG_ERR_MEMORY;
    }

    caller.data_mut().mark_read(index);
    bytes.len() as i32
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

    let mut store = Store::new(
        &engine,
        HostState {
            args,
            accessed: [0; ARGS_READ_WORDS],
            limits: store_limits(memory_limit_bytes),
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
fn run_inner(wasm: &[u8], args: Vec<Vec<u8>>, fuel: u64) -> Result<Outcome, String> {
    if args.len() > MAX_ARGS {
        return Err(format!("too many LogicSig args: {}", args.len()));
    }

    let (mut store, _instance, program) = instantiate(wasm, args, DEFAULT_MEMORY_LIMIT, fuel)?;

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
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if code.is_null() && code_len != 0 {
            return Err("null program pointer".to_string());
        }
        let wasm = std::slice::from_raw_parts(code, code_len);
        let args = args_from_raw(args, args_len)?;
        run_inner(wasm, args, fuel)
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
        run_inner(&wasm, args, fuel)
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
                (memory (export "memory") 1)
                (func (export "program") (result i32) i32.const 1))
            "#,
        )
        .unwrap();
        assert!(validate_inner(&wasm).is_ok());
    }
}
