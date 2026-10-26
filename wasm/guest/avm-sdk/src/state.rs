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

//! Typed handles for the current application's global state.

use core::marker::PhantomData;

use avm_sys::TxnError;

use crate::{trap, ActiveAvm};

/// A uint64 global-state slot.
///
/// A missing key reads as 0. The key must live for the program's lifetime; use
/// [`Avm::global_get_uint`] directly for runtime keys.
pub struct GlobalUint<T = u64> {
    avm: ActiveAvm,
    key: &'static [u8],
    phantom: PhantomData<T>,
}

impl<T> GlobalUint<T> {
    /// Creates a handle for `key` on the current application.
    pub const fn new(avm: ActiveAvm, key: &'static [u8]) -> Self {
        GlobalUint {
            avm,
            key,
            phantom: PhantomData,
        }
    }

    /// The state key.
    #[inline]
    pub const fn key(&self) -> &'static [u8] {
        self.key
    }

    /// Deletes the key. Deleting an absent key is not an error.
    #[inline]
    pub fn delete(&self) {
        self.avm.global_del(self.key);
    }
}

impl<T> GlobalUint<T>
where
    T: From<u64> + Into<u64>,
{
    /// Reads the value.
    #[inline]
    pub fn get(&self) -> T {
        T::from(self.avm.global_get_uint(self.key))
    }

    /// Writes the value.
    #[inline]
    pub fn set(&self, value: T) {
        self.avm.global_put_uint(self.key, value.into());
    }
}

/// A byte-string global-state slot.
///
/// Reads take a caller-provided buffer. A missing key reads as empty. Use
/// [`GlobalBytes::try_read`] when the value may exceed the buffer;
/// [`GlobalBytes::read`] traps on that, matching the AVM.
pub struct GlobalBytes {
    avm: ActiveAvm,
    key: &'static [u8],
}

impl GlobalBytes {
    /// Creates a handle for `key` on the current application.
    pub const fn new(avm: ActiveAvm, key: &'static [u8]) -> Self {
        GlobalBytes { avm, key }
    }

    /// The state key.
    #[inline]
    pub const fn key(&self) -> &'static [u8] {
        self.key
    }

    /// Copies the value into `buf`.
    #[inline]
    pub fn try_read<'buf>(&self, buf: &'buf mut [u8]) -> Result<&'buf [u8], TxnError> {
        self.avm.global_get_bytes(self.key, buf)
    }

    /// Copies the value into `buf`, trapping if the buffer is too small.
    #[inline]
    pub fn read<'buf>(&self, buf: &'buf mut [u8]) -> &'buf [u8] {
        match self.try_read(buf) {
            Ok(value) => value,
            Err(_) => trap(),
        }
    }

    /// Writes the value.
    #[inline]
    pub fn write(&self, value: &[u8]) {
        self.avm.global_put_bytes(self.key, value);
    }

    /// Deletes the key. Deleting an absent key is not an error.
    #[inline]
    pub fn delete(&self) {
        self.avm.global_del(self.key);
    }
}
