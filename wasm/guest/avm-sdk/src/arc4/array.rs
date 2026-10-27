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

//! Static (`T[N]`) and dynamic (`T[]`) arrays.

use core::marker::PhantomData;

use super::desc::{dyn_prefix, read_array_elem, Desc};
use super::Decode;
use crate::trap;

/// An ARC-4 dynamic array `T[]`, borrowed from its encoding.
///
/// Decoding validates every element, so [`DynArray::get`] and iteration read
/// on access without failing.
pub struct DynArray<'a, T> {
    bytes: &'a [u8],
    len: usize,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for DynArray<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for DynArray<'_, T> {}

impl<T> core::fmt::Debug for DynArray<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DynArray")
            .field("len", &self.len)
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl<'a, T: Decode<'a>> DynArray<'a, T> {
    /// The number of elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the array has no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Element `index`, or `None` if it is out of range.
    #[inline]
    pub fn get(&self, index: usize) -> Option<T> {
        if index >= self.len {
            return None;
        }
        // SAFETY: a `DynArray` is only built from a validated encoding, and
        // `index` is in range.
        Some(unsafe { read_array_elem(self.bytes, self.len, index) })
    }

    /// Iterates over the elements.
    #[inline]
    pub fn iter(&self) -> Iter<'a, T> {
        Iter {
            array: *self,
            next: 0,
        }
    }

    /// The encoded elements, after the length prefix.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

impl<'a, T: Decode<'a>> Decode<'a> for DynArray<'a, T> {
    const DESC: Desc = Desc::dyn_array(&[T::DESC]);

    #[inline]
    unsafe fn read(bytes: &'a [u8]) -> Self {
        let Some((len, bytes)) = dyn_prefix(bytes) else {
            trap()
        };
        DynArray {
            bytes,
            len,
            marker: PhantomData,
        }
    }
}

impl<'a, T: Decode<'a>> super::Encode for DynArray<'a, T> {
    const STATIC_LEN: Option<usize> = None;
    fn encode(&self, out: &mut [u8]) -> Result<usize, super::EncodeError> {
        let prefix = u16::try_from(self.len)
            .map_err(|_| super::EncodeError)?
            .to_be_bytes();
        super::encode::copy(&prefix, out)?;
        super::encode::copy(self.bytes, out.get_mut(2..).ok_or(super::EncodeError)?)?;
        Ok(self.bytes.len() + 2)
    }
}

impl<'a, T: Decode<'a>> IntoIterator for DynArray<'a, T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

/// An iterator over a [`DynArray`].
pub struct Iter<'a, T> {
    array: DynArray<'a, T>,
    next: usize,
}

impl<'a, T: Decode<'a>> Iterator for Iter<'a, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        let item = self.array.get(self.next)?;
        self.next += 1;
        Some(item)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.array.len - self.next;
        (remaining, Some(remaining))
    }
}

impl<'a, T: Decode<'a>> ExactSizeIterator for Iter<'a, T> {}

impl<'a, T: Decode<'a>, const N: usize> Decode<'a> for [T; N] {
    const DESC: Desc = Desc::array(&[T::DESC], N);

    unsafe fn read(bytes: &'a [u8]) -> Self {
        // SAFETY: the caller validated `bytes` as this array.
        core::array::from_fn(|i| unsafe { read_array_elem(bytes, N, i) })
    }
}
