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

//! A baseline compiler-generated WASM application program.
//!
//! It reads a big-endian `u64` delta from application argument 0, reads the
//! sender, and updates a global counter, with a per-sender running total. It
//! uses the same argument, transaction-field, and global-state host surface an
//! AVM application would. It is deliberately small: no crypto, no allocation,
//! and fixed-size buffers. This floors the size and setup cost of a Rust
//! application, in contrast to `rust-signed-claim`.

#![cfg_attr(target_arch = "wasm32", no_std)]

use wasm_guest::{
    app_arg_into, app_global_get_uint, app_global_put_uint, entry, txn_bytes, txn_uint, TxnField,
};

/// Largest total the counter will reach. A real contract would use a
/// consensus-visible limit instead of a compiled-in one.
const MAX_COUNT: u64 = 1_000_000;

entry!(|| {
    // Creation executes the approval program as well. Deploying must not
    // perform the action, so a create (ApplicationID 0) simply approves.
    if txn_uint(TxnField::ApplicationID) == 0 {
        return true;
    }

    let mut amount_bytes = [0u8; 8];
    let Ok(amount_bytes) = app_arg_into(0, &mut amount_bytes) else {
        return false;
    };
    if amount_bytes.len() != 8 {
        return false;
    }
    let delta = u64::from_be_bytes([
        amount_bytes[0],
        amount_bytes[1],
        amount_bytes[2],
        amount_bytes[3],
        amount_bytes[4],
        amount_bytes[5],
        amount_bytes[6],
        amount_bytes[7],
    ]);
    if delta == 0 {
        return false;
    }

    let mut sender = [0u8; 32];
    if txn_bytes(TxnField::Sender, &mut sender).map(|n| n == 32) != Ok(true) {
        return false;
    }

    let Some(count) = app_global_get_uint(b"count").checked_add(delta) else {
        return false;
    };
    if count > MAX_COUNT {
        return false;
    }
    let Some(sender_total) = app_global_get_uint(&sender).checked_add(delta) else {
        return false;
    };

    app_global_put_uint(b"count", count);
    app_global_put_uint(&sender, sender_total);
    true
});

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
