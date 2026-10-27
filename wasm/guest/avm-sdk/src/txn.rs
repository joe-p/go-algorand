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

//! The current transaction's fields.

use avm_sys::{TxnError, TxnField, MAX_ARG_BYTES};

#[cfg(target_arch = "wasm32")]
use crate::Avm;
use crate::{host, Error};

/// Reads a uint64 transaction field. Traps if the field is invalid or is a
/// byte-string field.
#[inline]
pub fn uint(field: TxnField) -> u64 {
    host().txn_uint(field)
}

/// Copies a byte-string transaction field into `buf`, returning its length.
///
/// A returned length greater than `buf.len()` means the buffer was too small and
/// nothing was written.
#[inline]
pub fn bytes(field: TxnField, buf: &mut [u8]) -> Result<usize, TxnError> {
    host().txn_bytes(field, buf)
}

/// Reads element `index` of a uint64 array transaction field.
#[inline]
pub fn array_uint(field: TxnField, index: u32) -> u64 {
    host().txna_uint(field, index)
}

/// Copies element `index` of a byte-string array transaction field into `buf`,
/// returning its length.
#[inline]
pub fn array_bytes(field: TxnField, index: u32, buf: &mut [u8]) -> Result<usize, TxnError> {
    host().txna_bytes(field, index, buf)
}

/// Reads a scalar uint field from transaction `group_index` in the current group.
#[inline]
pub fn group_uint(group_index: u32, field: TxnField) -> u64 {
    host().gtxn_uint(group_index, field)
}

/// Copies a scalar byte field from transaction `group_index`. An empty buffer
/// queries the field's length without copying. Invalid indices or fields trap.
#[inline]
pub fn group_bytes(group_index: u32, field: TxnField, buf: &mut [u8]) -> Result<usize, TxnError> {
    host().gtxn_bytes(group_index, field, buf)
}

/// Reads the current transaction's sender, which is always 32 bytes.
#[inline]
pub fn sender() -> Result<[u8; 32], Error> {
    let mut buf = [0u8; 32];
    let len = bytes(TxnField::Sender, &mut buf)?;
    if len != buf.len() {
        return Err(Error::WrongLength {
            expected: buf.len(),
            got: len,
        });
    }
    Ok(buf)
}

/// The current application's ID, or 0 during creation.
#[inline]
pub fn application_id() -> u64 {
    uint(TxnField::ApplicationID)
}

/// The number of application arguments, for an application program.
///
/// Equivalent to the AVM's `txn NumAppArgs`.
#[inline]
pub fn app_args_count() -> u32 {
    uint(TxnField::NumAppArgs) as u32
}

/// Copies application argument `index` into `buf`, returning the bytes written.
///
/// Equivalent to the AVM's `txna ApplicationArgs index`. An application reads
/// its arguments this way; LogicSig arguments come from [`crate::arg`].
///
/// If the argument is larger than `buf`, the call returns
/// [`TxnError::BufferTooSmall`] and nothing is written.
#[inline]
pub fn app_arg_into(index: u32, buf: &mut [u8]) -> Result<&[u8], TxnError> {
    let len = array_bytes(TxnField::ApplicationArgs, index, buf)?;
    if len > buf.len() {
        return Err(TxnError::BufferTooSmall);
    }
    Ok(&buf[..len])
}

/// Returns whether application argument `index` equals `expected`.
#[inline]
pub fn app_arg_eq(index: u32, expected: &[u8]) -> bool {
    let mut buf = [0u8; MAX_ARG_BYTES];
    match app_arg_into(index, &mut buf) {
        Ok(value) => value == expected,
        Err(_) => false,
    }
}
