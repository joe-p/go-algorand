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

//! The current application's global state.
//!
//! [`GlobalU64`] and [`GlobalBytes`] are typed handles for keys known at
//! compile time; the free functions take runtime keys. Both handles implement
//! [`StateField`], so they can be fields of a `#[contract]` struct.
//!
//! [`GlobalU64`] accepts any value that converts to and from `u64`.
//! [`GlobalBytes`] accepts any [`StateBytes`] value, such as
//! [`crate::Address`] or `[u8; N]`.

use core::marker::PhantomData;

use avm_sys::TxnError;

#[cfg(feature = "alloc")]
use crate::rust_alloc::vec::Vec;
#[cfg(target_arch = "wasm32")]
use crate::Avm;
use crate::{host, trap, Error};

/// A uint64 global-state slot.
///
/// A missing key reads as 0. A handle holds only its key and resolves the host
/// on each access, so it can be a `const`:
///
/// ```ignore
/// const COUNT: GlobalU64 = GlobalU64::new(b"count");
/// ```
///
/// `new` is the `u64` constructor; use [`GlobalU64::new_typed`] for another
/// value type. Use [`get_uint`] and [`put_uint`] for runtime keys.
pub struct GlobalU64<T = u64> {
    key: &'static [u8],
    phantom: PhantomData<T>,
}

impl GlobalU64<u64> {
    /// Creates a `u64` handle for `key` on the current application.
    pub const fn new(key: &'static [u8]) -> Self {
        Self::new_typed(key)
    }
}

impl<T> GlobalU64<T> {
    /// Creates a handle for `key` with an explicit value type.
    pub const fn new_typed(key: &'static [u8]) -> Self {
        GlobalU64 {
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
        delete(self.key);
    }
}

impl<T> GlobalU64<T>
where
    T: From<u64> + Into<u64>,
{
    /// Reads the value.
    #[inline]
    pub fn get(&self) -> T {
        T::from(get_uint(self.key))
    }

    /// Writes the value.
    #[inline]
    pub fn set(&self, value: T) {
        put_uint(self.key, value.into());
    }
}

/// A byte-string global-state slot.
///
/// Reads take a caller-provided buffer. A missing key reads as empty. Use
/// [`GlobalBytes::try_read`] when the value may exceed the buffer;
/// [`GlobalBytes::read`] traps on that, matching the AVM. Like [`GlobalU64`],
/// a handle holds only its key.
///
/// The default `T = [u8]` is the raw byte-string handle. Use another value type
/// with [`GlobalBytes::new_typed`] or by naming it in a state struct:
///
/// ```ignore
/// const OWNER: GlobalBytes<Address> = GlobalBytes::new_typed(b"owner");
/// ```
///
/// Typed values use [`GlobalBytes::get`] and [`GlobalBytes::set`].
/// With the `alloc` feature, `try_read_owned` / `read_owned` manage a buffer and
/// return an owned byte vector instead.
pub struct GlobalBytes<T: ?Sized = [u8]> {
    key: &'static [u8],
    phantom: PhantomData<T>,
}

impl GlobalBytes<[u8]> {
    /// Creates a raw byte-string handle for `key` on the current application.
    pub const fn new(key: &'static [u8]) -> Self {
        Self::new_typed(key)
    }
}

impl<T: ?Sized> GlobalBytes<T> {
    /// Creates a handle for `key` with an explicit value type.
    pub const fn new_typed(key: &'static [u8]) -> Self {
        GlobalBytes {
            key,
            phantom: PhantomData,
        }
    }

    /// The state key.
    #[inline]
    pub const fn key(&self) -> &'static [u8] {
        self.key
    }

    /// Copies the value into `buf`.
    #[inline]
    pub fn try_read<'buf>(&self, buf: &'buf mut [u8]) -> Result<&'buf [u8], TxnError> {
        get_bytes(self.key, buf)
    }

    /// Returns an owned value; a missing key reads as empty. Allocates 4 KiB
    /// and reads once, without a length query. Requires a guest allocator.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn try_read_owned(&self) -> Result<Vec<u8>, TxnError> {
        get_bytes_owned(self.key)
    }

    /// Returns an owned value, trapping on read errors. See [`Self::try_read_owned`].
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn read_owned(&self) -> Vec<u8> {
        match self.try_read_owned() {
            Ok(value) => value,
            Err(_) => trap(),
        }
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
        put_bytes(self.key, value);
    }

    /// Deletes the key. Deleting an absent key is not an error.
    #[inline]
    pub fn delete(&self) {
        delete(self.key);
    }
}

impl<T> GlobalBytes<T>
where
    T: StateBytes,
{
    /// Reads the value, trapping if the stored bytes do not encode a `T`.
    ///
    /// A missing key reads as empty, which is not a valid `T` unless `T` accepts
    /// the empty byte string. The value is read through a 4 KiB buffer; use
    /// [`Self::try_get`] to control the buffer or read a larger value.
    #[inline]
    pub fn get(&self) -> T {
        let mut buf = [0u8; crate::MAX_ARG_BYTES];
        match self.try_get(&mut buf) {
            Ok(value) => value,
            Err(_) => trap(),
        }
    }

    /// Reads the value into `buf` and converts it to `T`.
    ///
    /// Returns [`Error::WrongLength`] when the value does not match a
    /// fixed-size `T`, or [`Error::InvalidValue`] when a variable-size `T`
    /// rejects it. A missing key reads as empty.
    #[inline]
    pub fn try_get(&self, buf: &mut [u8]) -> Result<T, Error> {
        let bytes = self.try_read(buf)?;
        T::from_bytes(bytes).ok_or(match T::LEN {
            Some(expected) => Error::WrongLength {
                expected,
                got: bytes.len(),
            },
            None => Error::InvalidValue,
        })
    }

    /// Writes the value.
    #[inline]
    pub fn set(&self, value: T) {
        self.write(value.as_bytes());
    }
}

/// A value that can be stored in a global byte-string slot.
///
/// Implement this to use a type with [`GlobalBytes`]. The SDK implements it
/// for [`crate::Address`] and for `[u8; N]`.
pub trait StateBytes: Sized {
    /// The stored length, if every value of this type has the same length.
    const LEN: Option<usize> = None;

    /// Reads the value from its stored bytes, or `None` if `bytes` is invalid.
    fn from_bytes(bytes: &[u8]) -> Option<Self>;

    /// The bytes to store.
    fn as_bytes(&self) -> &[u8];
}

impl<const N: usize> StateBytes for [u8; N] {
    const LEN: Option<usize> = Some(N);

    #[inline]
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok()
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self
    }
}

/// A value that can be a field of a `#[contract]` struct.
///
/// The macro builds each field with [`StateField::from_key`] and sums the
/// schema constants into [`crate::Contract`].
pub trait StateField {
    /// Global uint64 slots this field occupies.
    const GLOBAL_UINTS: u32 = 0;
    /// Global byte-string slots this field occupies.
    const GLOBAL_BYTES: u32 = 0;

    /// Creates the field for `key`.
    fn from_key(key: &'static [u8]) -> Self;
}

impl<T> StateField for GlobalU64<T> {
    const GLOBAL_UINTS: u32 = 1;

    #[inline]
    fn from_key(key: &'static [u8]) -> Self {
        Self::new_typed(key)
    }
}

impl<T: ?Sized> StateField for GlobalBytes<T> {
    const GLOBAL_BYTES: u32 = 1;

    #[inline]
    fn from_key(key: &'static [u8]) -> Self {
        Self::new_typed(key)
    }
}

/// Reads a uint64 value. A missing key reads as 0.
#[inline]
pub fn get_uint(key: &[u8]) -> u64 {
    host().global_get_uint(key)
}

/// Writes a uint64 value.
#[inline]
pub fn put_uint(key: &[u8], value: u64) {
    host().global_put_uint(key, value);
}

/// Copies a byte-string value into `buf`. A missing key reads as empty.
#[inline]
pub fn get_bytes<'buf>(key: &[u8], buf: &'buf mut [u8]) -> Result<&'buf [u8], TxnError> {
    host().global_get_bytes(key, buf)
}

/// Returns an owned byte-string value; a missing key reads as empty. Allocates
/// 4 KiB and reads once. Requires a guest allocator; use [`get_bytes`] to control
/// capacity or avoid allocation. Reading a stored uint as bytes still traps.
#[cfg(feature = "alloc")]
#[inline]
pub fn get_bytes_owned(key: &[u8]) -> Result<Vec<u8>, TxnError> {
    crate::bytes::read_owned(
        |buf| get_bytes(key, buf).map(|value| value.len()),
        TxnError::BufferTooSmall,
    )
}

/// Writes a byte-string value.
#[inline]
pub fn put_bytes(key: &[u8], value: &[u8]) {
    host().global_put_bytes(key, value);
}

/// Deletes a key. Deleting an absent key is not an error.
#[inline]
pub fn delete(key: &[u8]) {
    host().global_del(key);
}
