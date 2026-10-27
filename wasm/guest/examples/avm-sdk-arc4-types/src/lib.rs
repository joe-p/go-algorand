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

//! ARC-4 methods taking dynamic arrays, nested tuples, and more than 15
//! parameters, written with `avm-sdk`.
//!
//! Each method folds its arguments into the global `total` (uint) and `text`
//! (bytes), so a test can check that every value decoded. The Go test
//! `TestWasmAppArc4Types` calls it with arguments encoded by the `avm-abi`
//! package.

#![cfg_attr(target_arch = "wasm32", no_std)]

use avm_sdk::arc4::{Address, DynArray};
use avm_sdk::{contract, Error, GlobalBytes, GlobalU64};

avm_sdk::panic_handler!();

/// The longest `text` a method builds.
const MAX_TEXT: usize = 128;

#[contract]
pub struct Types {
    total: GlobalU64,
    text: GlobalBytes,
}

/// Appends byte strings into a fixed buffer.
///
/// It avoids slice indexing and `copy_from_slice`, whose panic paths would link
/// core's formatting code (about 1 KB of module, paid in setup fuel).
struct Text {
    buf: [u8; MAX_TEXT],
    len: usize,
}

impl Text {
    fn new() -> Self {
        Text {
            buf: [0; MAX_TEXT],
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let end = self.len + bytes.len();
        let Some(dst) = self.buf.get_mut(self.len..end) else {
            return Err(Error::BufferTooSmall);
        };
        for (dst, src) in dst.iter_mut().zip(bytes) {
            *dst = *src;
        }
        self.len = end;
        Ok(())
    }

    fn as_bytes(&self) -> &[u8] {
        self.buf.get(..self.len).unwrap_or_default()
    }
}

#[contract]
impl Types {
    /// `total` = the sum of `values`.
    #[abimethod]
    pub fn sum(&self, values: DynArray<u64>) -> Result<(), Error> {
        let mut total = 0u64;
        for value in values {
            total = total.checked_add(value).ok_or(Error::InvalidValue)?;
        }
        self.total.set(total);
        Ok(())
    }

    /// `text` = `names` joined with commas; `total` = how many.
    #[abimethod]
    pub fn join(&self, names: DynArray<&str>) -> Result<(), Error> {
        let mut text = Text::new();
        for (i, name) in names.iter().enumerate() {
            if i > 0 {
                text.push(b",")?;
            }
            text.push(name.as_bytes())?;
        }
        self.total.set(names.len() as u64);
        self.text.write(text.as_bytes());
        Ok(())
    }

    /// `total` = `id` + 100 per set flag + the last address byte; `text` =
    /// `label` followed by each `(tag, payload)` pair's payload.
    #[abimethod]
    pub fn record(
        &self,
        entry: ((u8, &str), [bool; 2], Address),
        items: DynArray<(u16, &[u8])>,
    ) -> Result<(), Error> {
        let ((id, label), flags, owner) = entry;
        let mut total = u64::from(id) + u64::from(owner.0[31]);
        total += 100 * flags.iter().filter(|&&flag| flag).count() as u64;
        let mut text = Text::new();
        text.push(label.as_bytes())?;
        for (tag, payload) in items {
            total += u64::from(tag);
            text.push(payload)?;
        }
        self.total.set(total);
        self.text.write(text.as_bytes());
        Ok(())
    }

    /// `total` = `sum(i * p_i)`. Parameters 15 to 17 arrive packed in a tuple.
    #[abimethod]
    #[allow(clippy::too_many_arguments)]
    pub fn wide(
        &self,
        p1: u64,
        p2: u64,
        p3: u64,
        p4: u64,
        p5: u64,
        p6: u64,
        p7: u64,
        p8: u64,
        p9: u64,
        p10: u64,
        p11: u64,
        p12: u64,
        p13: u64,
        p14: u64,
        p15: u64,
        p16: u64,
        p17: u64,
    ) {
        let params = [
            p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11, p12, p13, p14, p15, p16, p17,
        ];
        let mut total = 0u64;
        for (i, p) in params.iter().enumerate() {
            total = total.wrapping_add((i as u64 + 1).wrapping_mul(*p));
        }
        self.total.set(total);
    }
}
