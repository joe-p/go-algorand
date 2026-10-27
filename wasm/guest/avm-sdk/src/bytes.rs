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

use crate::{Error, TxnError};

#[inline]
pub(crate) fn read_address(
    read: impl FnOnce(&mut [u8]) -> Result<usize, TxnError>,
) -> Result<[u8; 32], Error> {
    let mut address = [0; 32];
    let len = read(&mut address)?;
    if len != address.len() {
        return Err(Error::WrongLength {
            expected: address.len(),
            got: len,
        });
    }
    Ok(address)
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
    fn reported_address_length_is_checked_even_if_host_did_not_copy() {
        for len in [0, 31, 33, usize::MAX] {
            assert_eq!(
                read_address(|_| Ok(len)),
                Err(Error::WrongLength {
                    expected: 32,
                    got: len
                })
            );
        }
        assert_eq!(read_address(|_| Err(TxnError::Memory)), Err(Error::Memory));
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
            TxnError::BufferTooSmall,
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(value, b"ok");
        assert_eq!(
            read_owned(|_| Ok(usize::MAX), TxnError::BufferTooSmall),
            Err(TxnError::BufferTooSmall)
        );
        assert_eq!(
            read_owned(|_| Err(TxnError::Memory), TxnError::BufferTooSmall),
            Err(TxnError::Memory)
        );
    }
}
