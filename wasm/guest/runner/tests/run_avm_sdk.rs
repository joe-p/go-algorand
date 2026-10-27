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

//! Runs the `avm-sdk-counter` fixture through the real executor with stub
//! transaction and global-state callbacks. This proves the SDK's `program_entry`
//! export, typed global-state handles, and optional bump allocator all work
//! against wasmi, not just the native mock.
//!
//! The fixture is built by `make wasm-guest-fixtures`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use wasm_lsig::wasm_lsig_run;

/// The delta each call applies. Must match `txn_stub`.
const DELTA: u64 = 1000;
const FUEL: u64 = 1_000_000;

// `logic.TxnField` discriminants used by the fixture.
const FIELD_SENDER: u32 = 0;
const FIELD_APPLICATION_ID: u32 = 24;
const FIELD_APPLICATION_ARGS: u32 = 26;
const FIELD_NUM_APP_ARGS: u32 = 27;

const SENDER: [u8; 32] = [0x11; 32];

#[derive(Clone)]
enum Value {
    Uint(u64),
    Bytes(Vec<u8>),
}

thread_local! {
    static STATE: RefCell<BTreeMap<Vec<u8>, Value>> = RefCell::new(BTreeMap::new());
}

fn fixture() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../data/transactions/logic/testdata/wasm/app_avm_sdk_counter.wasm"
    );
    std::fs::read(path)
        .unwrap_or_else(|err| panic!("read {path} ({err}); run `make wasm-guest-fixtures`"))
}

unsafe fn write_uint(
    buf: *mut u8,
    buf_cap: usize,
    out_len: *mut usize,
    is_bytes: *mut u8,
    value: u64,
) -> i32 {
    *out_len = 8;
    *is_bytes = 0;
    if buf_cap < 8 {
        return -2;
    }
    if !buf.is_null() {
        let bytes = value.to_le_bytes();
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
    }
    0
}

unsafe fn write_bytes(
    buf: *mut u8,
    buf_cap: usize,
    out_len: *mut usize,
    is_bytes: *mut u8,
    value: &[u8],
) -> i32 {
    *out_len = value.len();
    *is_bytes = 1;
    if buf_cap < value.len() {
        return -2;
    }
    if !buf.is_null() && !value.is_empty() {
        std::ptr::copy_nonoverlapping(value.as_ptr(), buf, value.len());
    }
    0
}

/// A `txn`/`txna` provider for the fields the fixture reads.
unsafe extern "C" fn txn_stub(
    _ctx: usize,
    _source: u32,
    _group_index: u32,
    field: u32,
    index: u32,
    _expect_array: u32,
    buf: *mut u8,
    buf_cap: usize,
    out_len: *mut usize,
    is_bytes: *mut u8,
    _err: *mut u8,
    _err_cap: usize,
) -> i32 {
    match (field, index) {
        (FIELD_APPLICATION_ID, _) => write_uint(buf, buf_cap, out_len, is_bytes, 888),
        (FIELD_NUM_APP_ARGS, _) => write_uint(buf, buf_cap, out_len, is_bytes, 1),
        (FIELD_SENDER, _) => write_bytes(buf, buf_cap, out_len, is_bytes, &SENDER),
        (FIELD_APPLICATION_ARGS, 0) => {
            write_bytes(buf, buf_cap, out_len, is_bytes, &DELTA.to_be_bytes())
        }
        _ => -1,
    }
}

unsafe extern "C" fn global_get(
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
    STATE.with(|state| match state.borrow().get(key) {
        None => {
            *out_len = 0;
            *out_is_bytes = 0;
            *out_exists = 0;
            0
        }
        Some(Value::Uint(value)) => {
            *out_exists = 1;
            write_uint(buf, buf_cap, out_len, out_is_bytes, *value)
        }
        Some(Value::Bytes(value)) => {
            *out_exists = 1;
            write_bytes(buf, buf_cap, out_len, out_is_bytes, value)
        }
    })
}

unsafe extern "C" fn global_put(
    _ctx: usize,
    key: *const u8,
    key_len: usize,
    val: *const u8,
    val_len: usize,
    val_is_uint: u32,
    _err: *mut u8,
    _err_cap: usize,
) -> i32 {
    let key = std::slice::from_raw_parts(key, key_len).to_vec();
    let val = std::slice::from_raw_parts(val, val_len);
    let value = if val_is_uint != 0 {
        if val.len() != 8 {
            return -1;
        }
        Value::Uint(u64::from_le_bytes([
            val[0], val[1], val[2], val[3], val[4], val[5], val[6], val[7],
        ]))
    } else {
        Value::Bytes(val.to_vec())
    };
    STATE.with(|state| {
        state.borrow_mut().insert(key, value);
    });
    0
}

unsafe extern "C" fn global_del(
    _ctx: usize,
    key: *const u8,
    key_len: usize,
    _err: *mut u8,
    _err_cap: usize,
) -> i32 {
    let key = std::slice::from_raw_parts(key, key_len);
    STATE.with(|state| {
        state.borrow_mut().remove(key);
    });
    0
}

/// Runs the fixture as an application program and returns `(result, fuel, error)`.
fn run() -> (i32, u64, String) {
    let code = fixture();
    let mut fuel_used = 0u64;
    let mut args_read = [0u64; 4];
    let mut err = [0u8; 256];
    let ret = unsafe {
        wasm_lsig_run(
            code.as_ptr(),
            code.len(),
            std::ptr::null(),
            0,
            FUEL,
            &mut fuel_used,
            args_read.as_mut_ptr(),
            err.as_mut_ptr(),
            err.len(),
            Some(txn_stub),
            None,
            Some(global_get),
            Some(global_put),
            Some(global_del),
            None,
            None,
            None,
            0,
            // is_app = 1: the fixture is an application program.
            1,
        )
    };
    let end = err.iter().position(|&b| b == 0).unwrap_or(err.len());
    (
        ret,
        fuel_used,
        String::from_utf8_lossy(&err[..end]).into_owned(),
    )
}

fn state_uint(key: &[u8]) -> Option<u64> {
    STATE.with(|state| match state.borrow().get(key) {
        Some(Value::Uint(value)) => Some(*value),
        _ => None,
    })
}

#[test]
fn counter_updates_state_across_calls() {
    STATE.with(|state| state.borrow_mut().clear());

    let (ret, fuel, err) = run();
    assert_eq!(ret, 1, "expected approval, got {ret}: {err}");
    assert!(fuel > 0, "the executor reported no fuel use");
    assert_eq!(state_uint(b"count"), Some(DELTA));

    let (ret, _, err) = run();
    assert_eq!(ret, 1, "expected approval, got {ret}: {err}");
    assert_eq!(state_uint(b"count"), Some(2 * DELTA));
    assert_eq!(state_uint(&SENDER), Some(2 * DELTA));
}
