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

//! The `rust-counter` fixture rewritten with `avm-sdk`.
//!
//! It reads a big-endian `u64` delta from application argument 0, updates a
//! global counter, and keeps a per-sender running total. The per-sender key is
//! built in a `Vec`, so the optional bump allocator is exercised on every call.
//!
//! As with the other fixtures, creation executes the approval program, so a
//! create (`ApplicationID == 0`) simply approves.

#![cfg_attr(target_arch = "wasm32", no_std)]
extern crate alloc;

use alloc::vec::Vec;
use avm_sdk::{contract, global, txn, Error, GlobalUint};

avm_sdk::panic_handler!();

/// Largest total the counter will reach.
const MAX_COUNT: u64 = 1_000_000;

/// The counter's global state. `#[contract]` derives each key from the field
/// name and computes the schema.
#[contract]
pub struct Counter {
    /// The running total across all senders.
    count: GlobalUint,
}

// `#[contract]` on the impl exports `program`, which calls `approve`.
#[contract(alloc)]
impl Counter {
    pub fn approve(&self) -> Result<(), Error> {
        // Creation executes the approval program as well. Deploying must not
        // perform the action, so a create (ApplicationID 0) simply approves.
        if txn::application_id() == 0 {
            return Ok(());
        }
        if txn::app_args_count() != 1 {
            return Err(Error::InvalidValue);
        }

        let mut delta_bytes = [0u8; 8];
        let delta_bytes = txn::app_arg_into(0, &mut delta_bytes)?;
        if delta_bytes.len() != 8 {
            return Err(Error::WrongLength {
                expected: 8,
                got: delta_bytes.len(),
            });
        }
        let delta = u64::from_be_bytes([
            delta_bytes[0],
            delta_bytes[1],
            delta_bytes[2],
            delta_bytes[3],
            delta_bytes[4],
            delta_bytes[5],
            delta_bytes[6],
            delta_bytes[7],
        ]);
        if delta == 0 {
            return Err(Error::InvalidValue);
        }

        let sender = txn::sender()?;

        // A heap-allocated key proves the optional bump allocator is working.
        let mut sender_key = Vec::with_capacity(sender.len());
        sender_key.extend_from_slice(&sender);

        let Some(total) = self.count.get().checked_add(delta) else {
            return Err(Error::InvalidValue);
        };
        if total > MAX_COUNT {
            return Err(Error::InvalidValue);
        }
        let Some(sender_total) = global::get_uint(&sender_key).checked_add(delta) else {
            return Err(Error::InvalidValue);
        };

        self.count.set(total);
        global::put_uint(&sender_key, sender_total);
        Ok(())
    }
}
