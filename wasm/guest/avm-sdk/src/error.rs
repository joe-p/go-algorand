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

//! SDK-level errors.

use avm_sys::{ArgError, TxnError};

/// A recoverable failure. Returning one of these from a program rejects the
/// transaction; it does not trap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The destination buffer was too small.
    BufferTooSmall,
    /// The guest has no linear memory, or a pointer was invalid.
    Memory,
    /// The argument or field index was out of range.
    OutOfRange,
    /// A value had the wrong length for its type.
    WrongLength {
        /// The expected length in bytes.
        expected: usize,
        /// The observed length in bytes.
        got: usize,
    },
    /// A value failed a domain check (for example, a zero amount).
    InvalidValue,
    /// An unrecognised host error.
    Unknown,
}

impl From<ArgError> for Error {
    #[inline]
    fn from(err: ArgError) -> Self {
        match err {
            ArgError::OutOfRange => Error::OutOfRange,
            ArgError::BufferTooSmall => Error::BufferTooSmall,
            ArgError::Memory => Error::Memory,
            ArgError::Unknown => Error::Unknown,
            _ => Error::Unknown,
        }
    }
}

impl From<TxnError> for Error {
    #[inline]
    fn from(err: TxnError) -> Self {
        match err {
            TxnError::BufferTooSmall => Error::BufferTooSmall,
            TxnError::Memory => Error::Memory,
            TxnError::Unknown => Error::Unknown,
            _ => Error::Unknown,
        }
    }
}
