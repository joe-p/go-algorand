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

//! Runs the committed `ed25519_dalek.wasm` guest through the real executor with
//! a stub `ed25519verify_bare`, proving the patched ed25519-dalek routes
//! verification to the host function instead of doing curve math in the guest.
//!
//! The fixture is built by `make wasm-guest-fixtures`.

use std::sync::atomic::{AtomicUsize, Ordering};

use wasm_lsig::{wasm_lsig_run, WasmLsigArg};

const DATA: &[u8] = b"wasm ed25519-dalek guest end to end";
const SIG_BYTE: u8 = 0xAB;
const PK_BYTE: u8 = 0xCD;
const FUEL: u64 = 20000;

// Matches `ARGS_READ_WORDS` in the executor (255 args / 64 bits per word).
const ARGS_READ_WORDS: usize = 4;

static VERIFY_CALLS: AtomicUsize = AtomicUsize::new(0);

// A stand-in for the Go verifier: approves iff the data, 64-byte signature, and
// 32-byte key match the fixed test vectors.
unsafe extern "C" fn verify_stub(
    data: *const u8,
    data_len: usize,
    sig: *const u8,
    sig_len: usize,
    pk: *const u8,
    pk_len: usize,
    _err: *mut u8,
    _err_cap: usize,
) -> i32 {
    VERIFY_CALLS.fetch_add(1, Ordering::SeqCst);
    if sig_len != 64 || pk_len != 32 {
        return -1;
    }
    let data = std::slice::from_raw_parts(data, data_len);
    let sig = std::slice::from_raw_parts(sig, sig_len);
    let pk = std::slice::from_raw_parts(pk, pk_len);
    if data == DATA && sig.iter().all(|&b| b == SIG_BYTE) && pk.iter().all(|&b| b == PK_BYTE) {
        1
    } else {
        0
    }
}

fn fixture() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../data/transactions/logic/testdata/wasm/ed25519_dalek.wasm"
    );
    std::fs::read(path)
        .unwrap_or_else(|err| panic!("read {path} ({err}); run `make wasm-guest-fixtures`"))
}

/// Runs the guest with the given args and returns `(result, error message)`.
/// `1` approved, `0` rejected, `-1` error.
fn run(args: &[&[u8]]) -> (i32, String) {
    let code = fixture();
    let raw: Vec<WasmLsigArg> = args
        .iter()
        .map(|a| WasmLsigArg {
            ptr: a.as_ptr(),
            len: a.len(),
        })
        .collect();
    let mut fuel_used = 0u64;
    let mut args_read = [0u64; ARGS_READ_WORDS];
    let mut err = [0u8; 256];
    let ret = unsafe {
        wasm_lsig_run(
            code.as_ptr(),
            code.len(),
            raw.as_ptr(),
            raw.len(),
            FUEL,
            &mut fuel_used,
            args_read.as_mut_ptr(),
            err.as_mut_ptr(),
            err.len(),
            None,
            Some(verify_stub),
            0,
        )
    };
    let end = err.iter().position(|&b| b == 0).unwrap_or(err.len());
    (ret, String::from_utf8_lossy(&err[..end]).into_owned())
}

#[test]
fn approves_a_valid_signature_through_the_host() {
    VERIFY_CALLS.store(0, Ordering::SeqCst);
    let sig = [SIG_BYTE; 64];
    let pk = [PK_BYTE; 32];
    let (ret, err) = run(&[DATA, &sig, &pk]);
    assert_eq!(ret, 1, "expected approval, got {ret}: {err}");
    assert!(
        VERIFY_CALLS.load(Ordering::SeqCst) > 0,
        "the host ed25519verify_bare was never called"
    );
}

#[test]
fn rejects_a_tampered_message() {
    let sig = [SIG_BYTE; 64];
    let pk = [PK_BYTE; 32];
    let (ret, err) = run(&[b"tampered", &sig, &pk]);
    assert_eq!(ret, 0, "expected rejection, got {ret}: {err}");
}
