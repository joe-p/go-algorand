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

//! An ARC-4 counter written with `avm-sdk`.
//!
//! `#[contract]` on the impl routes ARC-4 calls by selector:
//!
//! - `increment(uint64)void` adds a non-zero delta to the global `count`,
//!   capped at [`MAX_COUNT`].
//! - `set_label(string)void` stores a label of at most [`MAX_LABEL`] bytes.
//!
//! There is no `approve` method, so the only bare call accepted is a plain
//! (`NoOp`) create. The program does not allocate.

#![cfg_attr(target_arch = "wasm32", no_std)]

use avm_sdk::{contract, Error, GlobalBytes, GlobalUint};

avm_sdk::panic_handler!();

/// Largest total the counter will reach.
pub const MAX_COUNT: u64 = 1_000_000;

/// Longest label, in bytes.
pub const MAX_LABEL: usize = 32;

#[contract]
pub struct Counter {
    /// The running total.
    count: GlobalUint,
    /// A free-form label.
    label: GlobalBytes,
}

#[contract]
impl Counter {
    #[abimethod]
    pub fn increment(&self, delta: u64) -> Result<(), Error> {
        if delta == 0 {
            return Err(Error::InvalidValue);
        }
        let Some(total) = self.count.get().checked_add(delta) else {
            return Err(Error::InvalidValue);
        };
        if total > MAX_COUNT {
            return Err(Error::InvalidValue);
        }
        self.count.set(total);
        Ok(())
    }

    #[abimethod]
    pub fn set_label(&self, label: &str) -> Result<(), Error> {
        if label.len() > MAX_LABEL {
            return Err(Error::WrongLength {
                expected: MAX_LABEL,
                got: label.len(),
            });
        }
        self.label.write(label.as_bytes());
        Ok(())
    }
}
