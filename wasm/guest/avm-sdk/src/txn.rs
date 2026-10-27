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
//!
//! Transaction field reads are infallible. A field that is invalid, out of
//! version, unavailable for the transaction's type, or otherwise unreadable
//! returns its zero value: `0` for a uint, [`Address::ZERO`] for an address,
//! and an empty value for bytes. A destination buffer that is too small also
//! reads as empty; query the length first with an empty buffer when the value
//! might not fit.

use avm_sys::{TxnError, TxnField, MAX_ARG_BYTES};

#[cfg(feature = "alloc")]
use crate::rust_alloc::vec::Vec;
#[cfg(target_arch = "wasm32")]
use crate::Avm;
use crate::{address::Address, application::Application, host};

/// Reads a uint64 transaction field. Invalid or non-uint fields read as 0.
#[inline]
pub fn uint(field: TxnField) -> u64 {
    host().txn_uint(field)
}

/// Copies a byte-string transaction field into `buf`, returning its length.
///
/// An empty buffer queries the field's length without copying. A field that
/// does not fit in `buf`, or cannot be read, returns 0 and writes nothing.
#[inline]
pub fn bytes(field: TxnField, buf: &mut [u8]) -> usize {
    host().txn_bytes(field, buf).unwrap_or(0)
}

/// Returns an owned byte-string transaction field, or an empty vector if the
/// field cannot be read. Allocates a 4 KiB buffer and reads once. Requires a
/// guest allocator. Use [`bytes`] to control buffer capacity or avoid
/// allocation.
#[cfg(feature = "alloc")]
#[inline]
pub fn bytes_owned(field: TxnField) -> Vec<u8> {
    crate::bytes::read_owned_or_empty(|buf| bytes(field, buf))
}

/// Reads an address-valued transaction field without allocating. Returns
/// [`Address::ZERO`] if the field is not a readable 32-byte value. The caller
/// selects the field; this does not validate its type.
#[inline]
pub fn address(field: TxnField) -> Address {
    crate::bytes::read_address(|buf| bytes(field, buf))
}

/// Reads element `index` of a uint64 array transaction field. Invalid fields
/// and out-of-range indices read as 0.
#[inline]
pub fn array_uint(field: TxnField, index: u32) -> u64 {
    host().txna_uint(field, index)
}

/// Copies element `index` of a byte-string array transaction field into `buf`,
/// returning its length. See [`bytes`] for the failure behavior.
#[inline]
pub fn array_bytes(field: TxnField, index: u32, buf: &mut [u8]) -> usize {
    host().txna_bytes(field, index, buf).unwrap_or(0)
}

#[inline]
pub(crate) fn try_array_bytes(
    field: TxnField,
    index: u32,
    buf: &mut [u8],
) -> Result<usize, TxnError> {
    host().txna_bytes(field, index, buf)
}

/// Returns an owned byte-string array element, or an empty vector if it cannot
/// be read. Like [`bytes_owned`], allocates 4 KiB and reads once.
#[cfg(feature = "alloc")]
#[inline]
pub fn array_bytes_owned(field: TxnField, index: u32) -> Vec<u8> {
    crate::bytes::read_owned_or_empty(|buf| array_bytes(field, index, buf))
}

/// Reads an address array element without allocating; see [`address`].
#[inline]
pub fn array_address(field: TxnField, index: u32) -> Address {
    crate::bytes::read_address(|buf| array_bytes(field, index, buf))
}

/// Reads a scalar uint field from transaction `group_index` in the current
/// group. Invalid indices and fields read as 0.
#[inline]
pub fn group_uint(group_index: u32, field: TxnField) -> u64 {
    host().gtxn_uint(group_index, field)
}

/// Copies a scalar byte field from transaction `group_index`. An empty buffer
/// queries the field's length without copying. See [`bytes`] for the failure
/// behavior.
#[inline]
pub fn group_bytes(group_index: u32, field: TxnField, buf: &mut [u8]) -> usize {
    host().gtxn_bytes(group_index, field, buf).unwrap_or(0)
}

/// Returns an owned byte field from a group transaction, or an empty vector if
/// it cannot be read. Like [`bytes_owned`], allocates 4 KiB and reads once.
#[cfg(feature = "alloc")]
#[inline]
pub fn group_bytes_owned(group_index: u32, field: TxnField) -> Vec<u8> {
    crate::bytes::read_owned_or_empty(|buf| group_bytes(group_index, field, buf))
}

/// Reads an address-valued group transaction field without allocating; see
/// [`address`].
#[inline]
pub fn group_address(group_index: u32, field: TxnField) -> Address {
    crate::bytes::read_address(|buf| group_bytes(group_index, field, buf))
}

/// Reads the current transaction's sender. Returns [`Address::ZERO`] if it
/// cannot be read.
#[inline]
pub fn sender() -> Address {
    address(TxnField::Sender)
}

/// The current application's ID, or `Application { id: 0 }` during creation.
#[inline]
pub fn application_id() -> Application {
    Application::from(uint(TxnField::ApplicationID))
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
/// If the argument is larger than `buf`, or cannot be read, the result is empty
/// and nothing is written.
#[inline]
pub fn app_arg_into(index: u32, buf: &mut [u8]) -> &[u8] {
    let len = array_bytes(TxnField::ApplicationArgs, index, buf);
    if len > buf.len() {
        return &[];
    }
    &buf[..len]
}

#[inline]
pub(crate) fn try_app_arg_into(index: u32, buf: &mut [u8]) -> Result<&[u8], TxnError> {
    let len = try_array_bytes(TxnField::ApplicationArgs, index, buf)?;
    if len > buf.len() {
        return Err(TxnError::BufferTooSmall);
    }
    Ok(&buf[..len])
}

/// Returns an owned application argument, or an empty vector if it cannot be
/// read. Allocates 4 KiB and reads once; requires a guest allocator. See
/// [`app_arg_into`] for the buffer-based API.
#[cfg(feature = "alloc")]
#[inline]
pub fn app_arg(index: u32) -> Vec<u8> {
    array_bytes_owned(TxnField::ApplicationArgs, index)
}

/// Returns whether application argument `index` equals `expected`.
#[inline]
pub fn app_arg_eq(index: u32, expected: &[u8]) -> bool {
    let mut buf = [0u8; MAX_ARG_BYTES];
    match try_app_arg_into(index, &mut buf) {
        Ok(value) => value == expected,
        Err(_) => false,
    }
}
