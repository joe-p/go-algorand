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

//! Typed transaction helpers.

use avm_sys::{txn_uint, txna_bytes, TxnError, TxnField, MAX_ARG_BYTES};

use crate::{Avm, Error};

/// The number of application arguments, for an application program.
///
/// Equivalent to the AVM's `txn NumAppArgs`.
#[inline]
pub fn app_args_count() -> u32 {
    txn_uint(TxnField::NumAppArgs) as u32
}

/// Copies application argument `index` into `buf`, returning the bytes written.
///
/// An application reads its arguments here: `arg`/`arg_into` are signature-only
/// and trap in an application. The index is resolved by the host, so an
/// out-of-range index traps the guest; check [`app_args_count`] first when the
/// index is untrusted. If `buf` is too small the call returns
/// [`TxnError::BufferTooSmall`] and nothing is written.
#[inline]
pub fn app_arg_into(index: u32, buf: &mut [u8]) -> Result<&[u8], TxnError> {
    let len = txna_bytes(TxnField::ApplicationArgs, index, buf)?;
    if len > buf.len() {
        return Err(TxnError::BufferTooSmall);
    }
    Ok(&buf[..len])
}

/// Convenience methods on top of [`Avm`].
pub trait AvmExt {
    /// Reads the current transaction's sender, which is always 32 bytes.
    fn sender(&self) -> Result<[u8; 32], Error>;

    /// The current application's ID, or 0 during creation.
    fn application_id(&self) -> u64;

    /// The number of application arguments.
    fn app_args_count(&self) -> u32;

    /// Copies application argument `index` into `buf`.
    fn app_arg_into<'buf>(&self, index: u32, buf: &'buf mut [u8]) -> Result<&'buf [u8], TxnError>;

    /// Returns whether application argument `index` equals `expected`.
    fn app_arg_eq(&self, index: u32, expected: &[u8]) -> bool;
}

impl<T: Avm + ?Sized> AvmExt for T {
    #[inline]
    fn sender(&self) -> Result<[u8; 32], Error> {
        let mut buf = [0u8; 32];
        let len = self.txn_bytes(TxnField::Sender, &mut buf)?;
        if len != buf.len() {
            return Err(Error::WrongLength {
                expected: buf.len(),
                got: len,
            });
        }
        Ok(buf)
    }

    #[inline]
    fn application_id(&self) -> u64 {
        self.txn_uint(TxnField::ApplicationID)
    }

    #[inline]
    fn app_args_count(&self) -> u32 {
        self.txn_uint(TxnField::NumAppArgs) as u32
    }

    #[inline]
    fn app_arg_into<'buf>(&self, index: u32, buf: &'buf mut [u8]) -> Result<&'buf [u8], TxnError> {
        let len = self.txna_bytes(TxnField::ApplicationArgs, index, buf)?;
        if len > buf.len() {
            return Err(TxnError::BufferTooSmall);
        }
        Ok(&buf[..len])
    }

    #[inline]
    fn app_arg_eq(&self, index: u32, expected: &[u8]) -> bool {
        let mut buf = [0u8; MAX_ARG_BYTES];
        match self.app_arg_into(index, &mut buf) {
            Ok(value) => value == expected,
            Err(_) => false,
        }
    }
}
