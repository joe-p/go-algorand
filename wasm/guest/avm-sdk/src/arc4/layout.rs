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

//! Non-generic layout helpers shared by tuples and arrays, so each decoded
//! type adds as little code as possible.

use super::Decode;
use super::DecodeError;
use crate::trap;

// Indexing here uses `get` and traps instead of panicking: a slice-index panic
// links core's formatting code, about 1 KB that every call pays for in setup
// fuel even though the panic handler ignores the message.

/// `bytes[start..end]`, trapping if out of range.
#[inline]
pub(crate) fn slice(bytes: &[u8], start: usize, end: usize) -> &[u8] {
    match bytes.get(start..end) {
        Some(slice) => slice,
        None => trap(),
    }
}

/// How one element is laid out in a tuple or array head.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct Layout {
    /// The static encoded size, or `None` for a dynamic element.
    pub static_len: Option<usize>,
    /// Whether the element is a `bool`, which packs into bits.
    pub is_bool: bool,
}

pub(crate) const fn layout<'a, T: Decode<'a>>() -> Layout {
    Layout {
        static_len: T::STATIC_LEN,
        is_bool: T::IS_BOOL,
    }
}

/// The encoded size of a sequence of static elements, or `None` if any is
/// dynamic. Consecutive `bool`s share bytes, eight per byte.
pub(crate) const fn static_len(elems: &[Layout]) -> Option<usize> {
    let mut len = 0;
    let mut i = 0;
    while i < elems.len() {
        if elems[i].is_bool {
            let mut run: usize = 0;
            while i < elems.len() && elems[i].is_bool {
                run += 1;
                i += 1;
            }
            len += run.div_ceil(8);
            continue;
        }
        match elems[i].static_len {
            Some(n) => len += n,
            None => return None,
        }
        i += 1;
    }
    Some(len)
}

/// Where one element's encoding is: `bytes[start..end]`, or bit `bit` (from
/// the most significant) of `bytes[start]` for a packed `bool`.
#[derive(Clone, Copy)]
pub(crate) struct Span {
    start: usize,
    end: usize,
    bit: u8,
}

const NO_BIT: u8 = u8::MAX;

impl Span {
    pub(crate) const EMPTY: Span = Span {
        start: 0,
        end: 0,
        bit: NO_BIT,
    };

    const fn bytes(start: usize, end: usize) -> Span {
        Span {
            start,
            end,
            bit: NO_BIT,
        }
    }
}

fn read_u16(bytes: &[u8], at: usize) -> Result<usize, DecodeError> {
    match bytes.get(at..at + 2) {
        Some(&[hi, lo]) => Ok(usize::from(u16::from_be_bytes([hi, lo]))),
        _ => Err(DecodeError),
    }
}

/// Splits a tuple encoding into its elements.
///
/// The forward pass lays out the heads, packing consecutive `bool`s into
/// shared bytes, and reads each dynamic element's offset. The backward pass
/// ends each tail where the next one starts, which also checks that the
/// offsets are in order and within `bytes`. The first tail must start right
/// after the heads; with no dynamic elements, the heads must be all of `bytes`.
pub(crate) fn split_tuple(
    bytes: &[u8],
    elems: &[Layout],
    spans: &mut [Span],
) -> Result<(), DecodeError> {
    const OPEN: usize = usize::MAX;
    let mut head = 0;
    let mut bit = 8;
    for (elem, span) in elems.iter().zip(spans.iter_mut()) {
        if elem.is_bool {
            if bit == 8 {
                bit = 0;
                head += 1;
            }
            *span = Span {
                start: head - 1,
                end: head,
                bit,
            };
            bit += 1;
            continue;
        }
        bit = 8;
        *span = match elem.static_len {
            Some(n) => Span::bytes(head, head + n),
            None => Span::bytes(read_u16(bytes, head)?, OPEN),
        };
        head += elem.static_len.unwrap_or(2);
    }
    if head > bytes.len() {
        return Err(DecodeError);
    }
    let mut end = bytes.len();
    for span in spans.iter_mut().rev() {
        if span.end == OPEN {
            if span.start > end {
                return Err(DecodeError);
            }
            span.end = end;
            end = span.start;
        }
    }
    if end != head {
        return Err(DecodeError);
    }
    Ok(())
}

/// Checks the structure of an array of `len` elements: its exact size for
/// static elements, or its offsets for dynamic ones.
pub(crate) fn check_array(bytes: &[u8], len: usize, elem: Layout) -> Result<(), DecodeError> {
    let expected = if elem.is_bool {
        Some(len.div_ceil(8))
    } else {
        elem.static_len.map(|n| n.saturating_mul(len))
    };
    if let Some(expected) = expected {
        return if bytes.len() == expected {
            Ok(())
        } else {
            Err(DecodeError)
        };
    }
    let mut previous = len.checked_mul(2).ok_or(DecodeError)?;
    if previous > bytes.len() {
        return Err(DecodeError);
    }
    for i in 0..len {
        let offset = read_u16(bytes, 2 * i)?;
        let in_order = if i == 0 {
            offset == previous
        } else {
            offset >= previous
        };
        if !in_order || offset > bytes.len() {
            return Err(DecodeError);
        }
        previous = offset;
    }
    Ok(())
}

/// Locates element `i` of an array that passed [`check_array`].
pub(crate) fn array_span(bytes: &[u8], len: usize, elem: Layout, i: usize) -> Span {
    if elem.is_bool {
        return Span {
            start: i / 8,
            end: i / 8 + 1,
            bit: (i % 8) as u8,
        };
    }
    if let Some(n) = elem.static_len {
        return Span::bytes(i * n, (i + 1) * n);
    }
    let offset = |i: usize| read_u16(bytes, 2 * i).unwrap_or(bytes.len());
    let end = if i + 1 < len {
        offset(i + 1)
    } else {
        bytes.len()
    };
    Span::bytes(offset(i), end)
}

/// Decodes the element at `span`.
#[inline]
pub(crate) fn decode_at<'a, T: Decode<'a>>(bytes: &'a [u8], span: Span) -> Result<T, DecodeError> {
    // `IS_BOOL` is a constant, so each instantiation keeps one branch.
    if !T::IS_BOOL {
        T::decode(slice(bytes, span.start, span.end))
    } else {
        let byte = match bytes.get(span.start) {
            Some(&byte) => byte,
            None => trap(),
        };
        Ok(T::from_packed_bit(byte & (0x80 >> span.bit) != 0))
    }
}
