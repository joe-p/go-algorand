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

//! ARC-4 application calls.
//!
//! `#[contract]` on an impl block routes ARC-4 calls to its `#[abimethod]`
//! methods. Application argument 0 is the 4-byte method selector and arguments
//! 1.. are the ABI-encoded parameters, one per argument. A method with more
//! than 15 parameters receives the 15th onward packed into a tuple in argument
//! 15, as ARC-4 specifies.
//!
//! Parameters decode without allocating. The router reads every argument into
//! one [`MAX_APP_ARGS_BYTES`] buffer, and dynamic values ([`DynArray`], `&[u8]`,
//! `&str`) borrow from it. Decoding is strict: every length, offset, and
//! static size must match the encoding exactly, or the call is rejected.
//!
//! | ABI type | Rust type |
//! |---|---|
//! | `uint8` … `uint128` | `u8`, `u16`, `u32`, `u64`, `u128` |
//! | `uintN` (any N) | [`Uint<N / 8>`](Uint), or [`U256`] / [`U512`] |
//! | `ufixedNxM` | [`UFixed<N / 8, M>`](UFixed) |
//! | `bool` | `bool` |
//! | `byte` | [`Byte`] |
//! | `address` | [`Address`] |
//! | `byte[N]` | `[u8; N]` |
//! | `T[N]` | `[T; N]` |
//! | `byte[]` | `&[u8]` |
//! | `T[]` | [`DynArray<T>`](DynArray) |
//! | `string` | `&str` |
//! | `(T1,T2,…)` | `(T1, T2, …)`, up to 16 elements |
//!
//! Methods return nothing (`void`): ARC-4 return values are logged, and the
//! host has no `log` function yet. Reference types (`account`, `asset`,
//! `application`) and transaction types are not supported.

mod array;
mod layout;
mod tuple;
mod utf8;

pub use array::{DynArray, Iter};

use crate::Error;

/// The AVM's limit on the number of application arguments (`MaxAppArgs`).
pub const MAX_APP_ARGS: usize = 16;

/// The AVM's limit on the total size of an application call's arguments
/// (`MaxAppTotalArgLen`). The router reads all arguments into a buffer this
/// size.
pub const MAX_APP_ARGS_BYTES: usize = 2048;

/// The value of the `OnCompletion` field for a plain (`NoOp`) application call.
pub const NO_OP: u64 = 0;

/// An argument that is not a valid encoding of its ARC-4 type.
///
/// It carries no detail: the router rejects the call either way, and a
/// zero-sized error keeps `Result<T, DecodeError>` as small as `Option<T>`,
/// which matters because module size is paid for in fuel on every call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeError;

impl From<DecodeError> for Error {
    #[inline]
    fn from(_: DecodeError) -> Self {
        Error::InvalidValue
    }
}

/// Decodes one ARC-4 value from exactly its encoding.
///
/// Implemented for every supported parameter type (see the [module
/// docs](self)). Dynamic values borrow from `bytes`.
pub trait Decode<'a>: Sized {
    /// The encoded size of a static type, or `None` for a dynamic one.
    const STATIC_LEN: Option<usize>;

    /// Whether this is `bool`, which packs into bits inside tuples and arrays.
    #[doc(hidden)]
    const IS_BOOL: bool = false;

    /// A tuple's element layouts, kept in static data rather than built by code.
    #[doc(hidden)]
    const ELEMS: &'static [layout::Layout] = &[];

    /// Decodes `bytes`, which must be exactly one encoded value.
    fn decode(bytes: &'a [u8]) -> Result<Self, DecodeError>;

    /// Builds a packed `bool`. Only called when [`Decode::IS_BOOL`] is true.
    #[doc(hidden)]
    fn from_packed_bit(_bit: bool) -> Self {
        crate::trap()
    }
}

#[inline]
fn exact<const N: usize>(bytes: &[u8]) -> Result<[u8; N], DecodeError> {
    bytes.try_into().map_err(|_| DecodeError)
}

macro_rules! decode_uint {
    ($($ty:ty),*) => {$(
        impl Decode<'_> for $ty {
            const STATIC_LEN: Option<usize> = Some(core::mem::size_of::<$ty>());

            #[inline]
            fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
                Ok(<$ty>::from_be_bytes(exact(bytes)?))
            }
        }
    )*};
}

decode_uint!(u8, u16, u32, u64, u128);

impl Decode<'_> for bool {
    const STATIC_LEN: Option<usize> = Some(1);
    const IS_BOOL: bool = true;

    #[inline]
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        match exact::<1>(bytes)? {
            [0x80] => Ok(true),
            [0x00] => Ok(false),
            _ => Err(DecodeError),
        }
    }

    #[inline]
    fn from_packed_bit(bit: bool) -> Self {
        bit
    }
}

/// An ARC-4 `byte`. It encodes like `uint8` but has its own name in method
/// signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Byte(pub u8);

impl Decode<'_> for Byte {
    const STATIC_LEN: Option<usize> = Some(1);

    #[inline]
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        Ok(Byte(exact::<1>(bytes)?[0]))
    }
}

/// An ARC-4 `address`: a 32-byte public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address(pub [u8; 32]);

impl Decode<'_> for Address {
    const STATIC_LEN: Option<usize> = Some(32);

    #[inline]
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        exact(bytes).map(Address)
    }
}

/// An ARC-4 `uintN` of `BYTES * 8` bits, as big-endian bytes.
///
/// Use `u8` … `u128` for the native widths; this covers the rest, up to
/// `uint512` (`Uint<64>`). The method signature names it `uint{BYTES * 8}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uint<const BYTES: usize>(pub [u8; BYTES]);

/// An ARC-4 `uint256`.
pub type U256 = Uint<32>;

/// An ARC-4 `uint512`.
pub type U512 = Uint<64>;

impl<const BYTES: usize> Uint<BYTES> {
    /// The value, if it fits in a `u64`.
    pub fn to_u64(&self) -> Option<u64> {
        let split = BYTES.saturating_sub(8);
        let (high, low) = self.0.split_at(split);
        if high.iter().any(|&byte| byte != 0) {
            return None;
        }
        Some(low.iter().fold(0, |acc, &byte| acc << 8 | u64::from(byte)))
    }
}

impl<const BYTES: usize> Decode<'_> for Uint<BYTES> {
    const STATIC_LEN: Option<usize> = Some(BYTES);

    #[inline]
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        exact(bytes).map(Uint)
    }
}

/// An ARC-4 `ufixedNxM`: an unsigned `BYTES * 8`-bit value `v` representing
/// `v / 10^PRECISION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UFixed<const BYTES: usize, const PRECISION: u8>(pub Uint<BYTES>);

impl<const BYTES: usize, const PRECISION: u8> Decode<'_> for UFixed<BYTES, PRECISION> {
    const STATIC_LEN: Option<usize> = Some(BYTES);

    #[inline]
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        Uint::decode(bytes).map(UFixed)
    }
}

impl<'a> Decode<'a> for &'a [u8] {
    const STATIC_LEN: Option<usize> = None;

    #[inline]
    fn decode(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let Some((prefix, rest)) = bytes.split_first_chunk::<2>() else {
            return Err(DecodeError);
        };
        let len = u16::from_be_bytes(*prefix) as usize;
        if rest.len() != len {
            return Err(DecodeError);
        }
        Ok(rest)
    }
}

impl<'a> Decode<'a> for &'a str {
    const STATIC_LEN: Option<usize> = None;

    #[inline]
    fn decode(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let bytes = <&[u8]>::decode(bytes)?;
        if !utf8::is_valid(bytes) {
            return Err(DecodeError);
        }
        // SAFETY: `is_valid` accepts exactly the well-formed UTF-8 sequences;
        // the tests check it against `core::str::from_utf8`.
        Ok(unsafe { core::str::from_utf8_unchecked(bytes) })
    }
}

/// Reads application arguments `0..count` into `buf`, recording each one's
/// range in `spans`. Used by the `#[contract]` router.
#[doc(hidden)]
pub fn read_app_args(
    buf: &mut [u8; MAX_APP_ARGS_BYTES],
    count: u32,
    spans: &mut [(usize, usize); MAX_APP_ARGS],
) -> bool {
    if count as usize > MAX_APP_ARGS {
        return false;
    }
    let mut pos = 0;
    for (index, span) in (0..count).zip(spans.iter_mut()) {
        let Some(rest) = buf.get_mut(pos..) else {
            return false;
        };
        let Ok(arg) = crate::txn::app_arg_into(index, rest) else {
            return false;
        };
        let end = pos + arg.len();
        *span = (pos, end);
        pos = end;
    }
    true
}

/// Application argument `index`, as read by [`read_app_args`]. Used by the
/// `#[contract]` router.
#[doc(hidden)]
#[inline]
pub fn app_arg<'a>(
    buf: &'a [u8; MAX_APP_ARGS_BYTES],
    spans: &[(usize, usize); MAX_APP_ARGS],
    index: usize,
) -> &'a [u8] {
    match spans.get(index) {
        Some(&(start, end)) => layout::slice(buf, start, end),
        None => crate::trap(),
    }
}

/// Decodes application argument `index` as a `T`. Used by the `#[contract]`
/// router; not inlined, so a method with several parameters of one type shares
/// a single copy of its decoding.
#[doc(hidden)]
#[inline(never)]
pub fn decode_arg<'a, T: Decode<'a>>(
    buf: &'a [u8; MAX_APP_ARGS_BYTES],
    spans: &[(usize, usize); MAX_APP_ARGS],
    index: usize,
) -> Result<T, DecodeError> {
    T::decode(app_arg(buf, spans, index))
}

/// One `#[abimethod]`, as described to off-chain tooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Method {
    /// The ABI method name.
    pub name: &'static str,
    /// The full ARC-4 signature, e.g. `increment(uint64)void`.
    pub signature: &'static str,
    /// The first 4 bytes of SHA-512/256 of `signature`.
    pub selector: [u8; 4],
    /// The parameters, in order.
    pub args: &'static [Arg],
    /// Whether the method is callable only at creation (`ApplicationID == 0`).
    /// Other methods are callable only on an existing application.
    pub create: bool,
}

/// One `#[abimethod]` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arg {
    /// The Rust parameter name.
    pub name: &'static str,
    /// The ABI type, e.g. `uint64`.
    pub ty: &'static str,
}

/// The ARC-4 interface of a contract, implemented by `#[contract]` on an impl
/// block that has `#[abimethod]` methods.
///
/// It is metadata for tooling and tests; nothing in `program` reads it, so it
/// is not linked into the wasm module.
pub trait Arc4Contract: crate::Contract {
    /// The contract's ABI methods.
    const METHODS: &'static [Method];
}
