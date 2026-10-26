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

//! Type descriptors: an ARC-4 type's shape as static data, interpreted by a
//! handful of shared checkers.
//!
//! Every [`Decode`] type has a `const` [`Desc`] tree. [`validate`] walks the
//! tree, so no validation code is generated per type; the per-type code is
//! only the read, which assumes a validated encoding. Module size is paid for
//! in fuel on every call, and a descriptor is 16 bytes of data where a generic
//! check would be a few hundred bytes of code.
//!
//! Each descriptor carries its checker as a function pointer rather than a
//! kind to dispatch on, so a contract links only the checkers its types use: a
//! contract of `uint64` and `string` parameters carries no tuple or array
//! code.

use super::utf8;
use super::Decode;
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

/// The `size` of a dynamic type.
const DYNAMIC: u16 = u16::MAX;

/// The `size` of `bool`, which is one byte on its own but packs into bits
/// inside tuples and arrays.
const BOOL: u16 = u16::MAX - 1;

/// The shape of an ARC-4 type.
///
/// A `Decode` impl exposes one as `DESC`. Built-in types' descriptors are
/// composed with the constructors here; a descriptor and its type's `read`
/// must agree on the encoding.
#[derive(Clone, Copy, Debug)]
pub struct Desc {
    /// Validates one encoding of this type.
    check: fn(&[u8], &Desc) -> bool,
    /// The static encoded size, or [`BOOL`] or [`DYNAMIC`].
    size: u16,
    /// An array's element count, or a tuple's head size: its static elements
    /// plus two bytes per dynamic one, precomputed so the checker need not
    /// walk the elements twice.
    aux: u16,
    /// The element descriptor of an array, or a tuple's elements in order.
    elems: &'static [Desc],
}

impl Desc {
    /// The descriptor of `bool`.
    pub const BOOL: Desc = Desc {
        check: check_bool,
        size: BOOL,
        aux: 0,
        elems: &[],
    };

    /// The descriptor of `string`.
    pub const STR: Desc = Desc {
        check: check_str,
        size: DYNAMIC,
        aux: 0,
        elems: &[],
    };

    /// The descriptor of a type that is exactly `size` bytes.
    pub const fn fixed(size: usize) -> Desc {
        Desc {
            check: check_fixed,
            size: Self::static_size(size),
            aux: 0,
            elems: &[],
        }
    }

    /// The descriptor of `T[count]`, given `&[T::DESC]`.
    pub const fn array(elem: &'static [Desc], count: usize) -> Desc {
        assert!(elem.len() == 1, "an array takes one element descriptor");
        assert!(count <= u16::MAX as usize, "array too long");
        let size = match elem[0].size {
            BOOL => Self::static_size(count.div_ceil(8)),
            DYNAMIC => DYNAMIC,
            size => Self::static_size(size as usize * count),
        };
        Desc {
            check: check_array,
            size,
            aux: count as u16,
            elems: elem,
        }
    }

    /// The descriptor of `T[]`, given `&[T::DESC]`.
    pub const fn dyn_array(elem: &'static [Desc]) -> Desc {
        assert!(elem.len() == 1, "an array takes one element descriptor");
        Desc {
            check: check_dyn_array,
            size: DYNAMIC,
            aux: 0,
            elems: elem,
        }
    }

    /// The descriptor of a tuple of `elems`.
    pub const fn tuple(elems: &'static [Desc]) -> Desc {
        // Consecutive bools share bytes, eight per byte. The heads hold each
        // static element, or a two-byte offset for a dynamic one.
        let mut head = 0;
        let mut is_static = true;
        let mut i = 0;
        while i < elems.len() {
            match elems[i].size {
                BOOL => {
                    let mut run: usize = 0;
                    while i < elems.len() && elems[i].size == BOOL {
                        run += 1;
                        i += 1;
                    }
                    head += run.div_ceil(8);
                    continue;
                }
                DYNAMIC => {
                    is_static = false;
                    head += 2;
                }
                size => head += size as usize,
            }
            i += 1;
        }
        Desc {
            check: check_tuple,
            size: if is_static {
                Self::static_size(head)
            } else {
                DYNAMIC
            },
            aux: Self::static_size(head),
            elems,
        }
    }

    const fn static_size(size: usize) -> u16 {
        assert!(size < BOOL as usize, "static type too large");
        size as u16
    }

    /// The encoded size of a static type, or `None` for a dynamic one.
    pub const fn static_len(&self) -> Option<usize> {
        match self.size {
            DYNAMIC => None,
            BOOL => Some(1),
            size => Some(size as usize),
        }
    }

    /// Whether this is `bool`, which packs into bits inside tuples and arrays.
    pub const fn is_bool(&self) -> bool {
        self.size == BOOL
    }

    /// The element descriptors: one for an array, all for a tuple.
    pub(crate) const fn elems(&self) -> &'static [Desc] {
        self.elems
    }
}

/// Whether `bytes` is exactly one valid encoding of the type `desc` describes.
///
/// It is strict: every length, offset, and static size must match.
#[inline]
pub fn validate(bytes: &[u8], desc: &Desc) -> bool {
    (desc.check)(bytes, desc)
}

fn check_fixed(bytes: &[u8], desc: &Desc) -> bool {
    bytes.len() == usize::from(desc.size)
}

fn check_bool(bytes: &[u8], _: &Desc) -> bool {
    matches!(bytes, [0x00] | [0x80])
}

fn check_str(bytes: &[u8], _: &Desc) -> bool {
    match dyn_prefix(bytes) {
        Some((len, rest)) => rest.len() == len && utf8::is_valid(rest),
        None => false,
    }
}

fn check_array(bytes: &[u8], desc: &Desc) -> bool {
    let count = usize::from(desc.aux);
    check_seq(bytes, desc.elems, count, 2 * count)
}

fn check_dyn_array(bytes: &[u8], desc: &Desc) -> bool {
    match dyn_prefix(bytes) {
        Some((len, rest)) => check_seq(rest, desc.elems, len, 2 * len),
        None => false,
    }
}

fn check_tuple(bytes: &[u8], desc: &Desc) -> bool {
    check_seq(bytes, desc.elems, desc.elems.len(), usize::from(desc.aux))
}

/// The element descriptor at `i` of a sequence: a tuple's `i`th, or an
/// array's only one.
#[inline]
fn elem_at(elems: &[Desc], i: usize) -> &Desc {
    let i = if elems.len() == 1 { 0 } else { i };
    match elems.get(i) {
        Some(elem) => elem,
        None => trap(),
    }
}

/// An offset that is not in the encoding.
const NONE: usize = usize::MAX;

/// The big-endian `u16` at `at`, or [`NONE`] if it is out of range.
///
/// A sentinel rather than an `Option`: an `Option<usize>` is returned through
/// memory, and the callers' slice bounds checks fail on `NONE` anyway.
#[inline]
fn read_u16(bytes: &[u8], at: usize) -> usize {
    match bytes.get(at..at.wrapping_add(2)) {
        Some(&[hi, lo]) => usize::from(u16::from_be_bytes([hi, lo])),
        _ => NONE,
    }
}

/// Splits the length prefix off a dynamic encoding.
#[inline]
pub(crate) fn dyn_prefix(bytes: &[u8]) -> Option<(usize, &[u8])> {
    let (prefix, rest) = bytes.split_first_chunk::<2>()?;
    Some((usize::from(u16::from_be_bytes(*prefix)), rest))
}

/// Checks a sequence of `count` elements: a tuple's, or an array's repeated
/// one. `head` is the size of the heads when the elements are dynamic.
///
/// Static elements only need their sizes: a fixed value has no structure, and
/// a static tuple or array is a sequence of static values, so the only check
/// is the total. A dynamic element's offset must point at its tail, the tails
/// must be in order, the first must start right after the heads, the last must
/// end at the end, and each tail must validate. Each tail is checked once the
/// next offset is known; the "tail" before the first is empty and at `head`,
/// which pins the first offset.
fn check_seq(bytes: &[u8], elems: &[Desc], count: usize, head: usize) -> bool {
    if let [elem] = elems {
        if elem.size != DYNAMIC {
            let len = if elem.is_bool() {
                count.div_ceil(8)
            } else {
                count * usize::from(elem.size)
            };
            return bytes.len() == len;
        }
    }
    const EMPTY: Desc = Desc::fixed(0);
    let mut pos = 0usize;
    let mut bit = 8;
    let mut start = head;
    let mut tail = &EMPTY;
    for i in 0..count {
        let elem = elem_at(elems, i);
        match elem.size {
            BOOL => {
                if bit == 8 {
                    bit = 0;
                    pos += 1;
                }
                bit += 1;
                continue;
            }
            DYNAMIC => {}
            size => {
                bit = 8;
                pos += usize::from(size);
                continue;
            }
        }
        bit = 8;
        let offset = read_u16(bytes, pos);
        pos += 2;
        match bytes.get(start..offset) {
            Some(tail_bytes) if validate(tail_bytes, tail) => {}
            _ => return false,
        }
        start = offset;
        tail = elem;
    }
    match bytes.get(start..) {
        Some(tail_bytes) => validate(tail_bytes, tail),
        None => false,
    }
}

/// Where one element's encoding is: `bytes[start..end]`, or bit `bit` (from
/// the most significant) of `bytes[start]` for a packed `bool`.
#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
    bit: u8,
}

impl Span {
    const fn bytes(start: usize, end: usize) -> Span {
        Span { start, end, bit: 0 }
    }
}

/// Locates element `i` of an array of `count` `elem`s that passed
/// [`check_seq`].
fn array_span(bytes: &[u8], elem: &Desc, count: usize, i: usize) -> Span {
    match elem.size {
        BOOL => Span {
            start: i / 8,
            end: i / 8 + 1,
            bit: (i % 8) as u8,
        },
        DYNAMIC => {
            let end = if i + 1 < count {
                read_u16(bytes, 2 * (i + 1))
            } else {
                bytes.len()
            };
            Span::bytes(read_u16(bytes, 2 * i), end)
        }
        size => {
            let n = usize::from(size);
            Span::bytes(i * n, (i + 1) * n)
        }
    }
}

/// Locates element `i` of a tuple of `elems` that passed [`check_seq`].
fn tuple_span(bytes: &[u8], elems: &[Desc], i: usize) -> Span {
    let mut pos = 0;
    let mut bit = 8;
    let mut start = 0;
    for (j, elem) in elems.iter().enumerate() {
        match elem.size {
            BOOL => {
                if bit == 8 {
                    bit = 0;
                    pos += 1;
                }
                if j == i {
                    return Span {
                        start: pos - 1,
                        end: pos,
                        bit,
                    };
                }
                bit += 1;
                continue;
            }
            DYNAMIC => {}
            size => {
                bit = 8;
                let n = usize::from(size);
                if j == i {
                    return Span::bytes(pos, pos + n);
                }
                pos += n;
                continue;
            }
        }
        bit = 8;
        let offset = read_u16(bytes, pos);
        pos += 2;
        if j == i {
            start = offset;
        } else if j > i {
            // The first dynamic element after `i` ends `i`'s tail.
            return Span::bytes(start, offset);
        }
    }
    Span::bytes(start, bytes.len())
}

#[inline]
fn span_bit(bytes: &[u8], span: Span) -> bool {
    let byte = match bytes.get(span.start) {
        Some(&byte) => byte,
        None => trap(),
    };
    byte & (0x80 >> span.bit) != 0
}

// The element accessors return the slice or bit rather than a `Span`, which
// would come back through memory and be bounds-checked at every generic call
// site. Each is linked only by the contracts whose types use it.

/// The encoding of element `i` of an array of `count` `elem`s.
#[inline(never)]
pub(crate) fn array_elem<'a>(bytes: &'a [u8], elem: &Desc, count: usize, i: usize) -> &'a [u8] {
    let span = array_span(bytes, elem, count, i);
    slice(bytes, span.start, span.end)
}

/// Packed `bool` `i` of a `bool` array.
#[inline(never)]
pub(crate) fn array_bit(bytes: &[u8], i: usize) -> bool {
    span_bit(
        bytes,
        Span {
            start: i / 8,
            end: i / 8 + 1,
            bit: (i % 8) as u8,
        },
    )
}

/// The encoding of element `i` of a tuple of `elems`.
#[inline(never)]
pub(crate) fn tuple_elem<'a>(bytes: &'a [u8], elems: &[Desc], i: usize) -> &'a [u8] {
    let span = tuple_span(bytes, elems, i);
    slice(bytes, span.start, span.end)
}

/// Packed `bool` element `i` of a tuple of `elems`.
#[inline(never)]
pub(crate) fn tuple_bit(bytes: &[u8], elems: &[Desc], i: usize) -> bool {
    span_bit(bytes, tuple_span(bytes, elems, i))
}

/// Reads element `i` of a tuple of `elems`.
///
/// # Safety
///
/// `bytes` must be an encoding of the tuple that [`validate`] accepted, and
/// `T` must be the type of element `i`.
#[inline(always)]
pub(crate) unsafe fn read_tuple_elem<'a, T: Decode<'a>>(
    bytes: &'a [u8],
    elems: &[Desc],
    i: usize,
) -> T {
    // `IS_BOOL` is a constant, so each instantiation keeps one branch.
    if T::IS_BOOL {
        T::from_packed_bit(tuple_bit(bytes, elems, i))
    } else {
        // SAFETY: the caller's contract.
        unsafe { T::read(tuple_elem(bytes, elems, i)) }
    }
}

/// Reads element `i` of an array of `count` `T`s.
///
/// # Safety
///
/// `bytes` must be the elements of an array that [`validate`] accepted, and
/// `i` must be less than `count`.
#[inline(always)]
pub(crate) unsafe fn read_array_elem<'a, T: Decode<'a>>(
    bytes: &'a [u8],
    count: usize,
    i: usize,
) -> T {
    // `IS_BOOL` and `STATIC_LEN` are constants, so each instantiation keeps
    // one branch. Static elements are at multiples of their size, which skips
    // the locator for the common `byte[N]`.
    if T::IS_BOOL {
        T::from_packed_bit(array_bit(bytes, i))
    } else if let Some(n) = T::STATIC_LEN {
        // SAFETY: the caller's contract.
        unsafe { T::read(slice(bytes, i * n, (i + 1) * n)) }
    } else {
        // SAFETY: the caller's contract.
        unsafe { T::read(array_elem(bytes, &T::DESC, count, i)) }
    }
}
