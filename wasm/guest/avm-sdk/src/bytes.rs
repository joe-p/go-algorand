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

//! Shared buffer handling for public SDK byte readers.

use crate::address::Address;
#[cfg(test)]
use crate::TxnError;

/// Reads a fixed 32-byte address, returning [`Address::ZERO`] for any failed or
/// mis-sized read. Transaction and context field reads are infallible.
#[inline]
pub(crate) fn read_address(read: impl FnOnce(&mut [u8]) -> usize) -> Address {
    let mut address = [0; 32];
    if read(&mut address) == address.len() {
        Address(address)
    } else {
        Address::ZERO
    }
}

/// Reads an owned byte value, returning an empty vector if the read fails or
/// reports a value larger than the buffer. Bounded by [`crate::MAX_ARG_BYTES`].
#[cfg(feature = "alloc")]
#[inline]
pub(crate) fn read_owned_or_empty(
    read: impl FnOnce(&mut [u8]) -> usize,
) -> crate::rust_alloc::vec::Vec<u8> {
    let mut bytes = crate::rust_alloc::vec![0; crate::MAX_ARG_BYTES];
    let len = read(&mut bytes);
    if len > bytes.len() {
        return crate::rust_alloc::vec::Vec::new();
    }
    bytes.truncate(len);
    bytes
}

#[cfg(feature = "alloc")]
#[inline]
pub(crate) fn read_owned<E>(
    read: impl FnOnce(&mut [u8]) -> Result<usize, E>,
    buffer_too_small: E,
) -> Result<crate::rust_alloc::vec::Vec<u8>, E> {
    use crate::rust_alloc::vec;

    let mut bytes = vec![0; crate::MAX_ARG_BYTES];
    let len = read(&mut bytes)?;
    if len > bytes.len() {
        return Err(buffer_too_small);
    }
    bytes.truncate(len);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_reads_require_exactly_32_bytes() {
        assert_eq!(
            read_address(|buf| {
                buf.copy_from_slice(&[7; 32]);
                32
            }),
            Address([7; 32])
        );
        for len in [0, 31, 33, usize::MAX] {
            assert_eq!(read_address(|_| len), Address::ZERO);
        }
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_reader_checks_reported_length_and_reads_once() {
        let mut calls = 0;
        let value = read_owned(
            |buf| {
                calls += 1;
                assert_eq!(buf.len(), crate::MAX_ARG_BYTES);
                buf[..2].copy_from_slice(b"ok");
                Ok(2)
            },
            crate::TxnError::BufferTooSmall,
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(value, b"ok");
        assert_eq!(
            read_owned(|_| Ok(usize::MAX), TxnError::BufferTooSmall),
            Err(crate::TxnError::BufferTooSmall)
        );
        assert_eq!(
            read_owned(|_| Err(TxnError::Memory), TxnError::BufferTooSmall),
            Err(crate::TxnError::Memory)
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn infallible_owned_reader_returns_empty_on_failure() {
        let value = read_owned_or_empty(|buf| {
            buf[..3].copy_from_slice(b"abc");
            3
        });
        assert_eq!(value, b"abc");
        assert!(read_owned_or_empty(|_| 0).is_empty());
        assert!(read_owned_or_empty(|_| usize::MAX).is_empty());
    }
}
