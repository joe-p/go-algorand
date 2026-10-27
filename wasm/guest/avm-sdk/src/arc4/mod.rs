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
//! 1.. are the ABI-encoded non-transaction parameters, one per argument. With more
//! than 15 encoded parameters, the 15th onward are packed into a tuple in argument
//! 15, as ARC-4 specifies.
//!
//! Parameters decode without allocating. The router reads every argument into
//! one [`MAX_APP_ARGS_BYTES`] buffer, and dynamic values ([`DynArray`], `&[u8]`,
//! `&str`) borrow from it. Decoding is strict: every length, offset, and
//! static size must match the encoding exactly, or the call is rejected.
//!
//! Validation is driven by data, not code. Each type has a `const` [`Desc`]
//! describing its shape, which [`validate`] checks with a few shared checkers,
//! so a contract pays for validation once rather than per type, and links
//! only the checkers its types use. A type's own code is only its
//! [`Decode::read`], which trusts the validated encoding.
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
//! Top-level [`Account`], [`Asset`], and [`Application`] resolve uint8 indices
//! into this call's foreign arrays (account/application index 0 is implicit).
//! [`Transaction`] and its typed aliases consume no application arguments:
//! they select the immediately preceding group transactions in signature order.
//! References/transactions cannot be nested in tuples/arrays or returned.
//!
//! Methods return `()`, an ABI value, or `Result<T,E>`. Non-void successes
//! emit `RETURN_PREFIX || Encode(value)`; errors reject without a return log.
//! [`Encode`] supports the scalar, tuple, static-array, string, byte-slice,
//! and borrowed dynamic-array types above without an allocator. A return larger
//! than 1020 encoded bytes rejects. Earlier logs still consume the shared
//! protocol log allowance, and host limit failures trap.

mod array;
mod desc;
mod encode;
mod reference;
mod tuple;
mod utf8;

pub use array::{DynArray, Iter};
pub use desc::{validate, Desc};
pub use encode::{log_return, Encode, EncodeError, RETURN_PREFIX};
pub use reference::{
    Account, Application, ApplicationCallTxn, Asset, AssetConfigTxn, AssetFreezeTxn,
    AssetTransferTxn, KeyRegistrationTxn, PaymentTxn, Transaction,
};

use crate::{trap, Error};

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

/// An ARC-4 value type: a [`Desc`] of its encoding, and a read of a validated
/// encoding.
///
/// Implemented for every supported parameter type (see the [module
/// docs](self)). Dynamic values borrow from `bytes`. Use [`Decode::decode`]
/// to validate and read in one step.
pub trait Decode<'a>: Sized {
    /// The shape of the encoding, which [`validate`] checks.
    const DESC: Desc;

    /// The encoded size of a static type, or `None` for a dynamic one.
    const STATIC_LEN: Option<usize> = Self::DESC.static_len();

    /// Whether this is `bool`, which packs into bits inside tuples and arrays.
    #[doc(hidden)]
    const IS_BOOL: bool = Self::DESC.is_bool();

    /// Reads a value from `bytes`, which must be exactly one encoded value.
    ///
    /// # Safety
    ///
    /// [`validate`]`(bytes, &Self::DESC)` must have returned `true`. A read
    /// does not check structure; on an invalid encoding it may return garbage
    /// or trap, and `&str` would be unsound.
    unsafe fn read(bytes: &'a [u8]) -> Self;

    /// Builds a packed `bool`. Only called when [`Decode::IS_BOOL`] is true.
    #[doc(hidden)]
    fn from_packed_bit(_bit: bool) -> Self {
        trap()
    }

    /// Validates `bytes` as exactly one encoded value and reads it.
    #[inline]
    fn decode(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        if validate(bytes, &Self::DESC) {
            // SAFETY: just validated.
            Ok(unsafe { Self::read(bytes) })
        } else {
            Err(DecodeError)
        }
    }
}

/// `bytes` as an array, trapping on a length mismatch.
#[inline]
fn exact<const N: usize>(bytes: &[u8]) -> [u8; N] {
    match bytes.try_into() {
        Ok(array) => array,
        Err(_) => trap(),
    }
}

/// Big-endian bytes to a `u64`. wasm has no byte swap, so `from_be_bytes` is
/// about 80 bytes of shifts per width; one loop shared by every width is
/// smaller.
#[inline(never)]
fn be_u64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |acc, &b| acc << 8 | u64::from(b))
}

impl Decode<'_> for u8 {
    const DESC: Desc = Desc::fixed(1);

    #[inline]
    unsafe fn read(bytes: &[u8]) -> Self {
        match bytes.first() {
            Some(&byte) => byte,
            None => trap(),
        }
    }
}

macro_rules! decode_uint {
    ($($ty:ty),*) => {$(
        impl Decode<'_> for $ty {
            const DESC: Desc = Desc::fixed(core::mem::size_of::<$ty>());

            #[inline]
            unsafe fn read(bytes: &[u8]) -> Self {
                be_u64(bytes) as $ty
            }
        }
    )*};
}

decode_uint!(u16, u32, u64);

impl Decode<'_> for u128 {
    const DESC: Desc = Desc::fixed(16);

    #[inline]
    unsafe fn read(bytes: &[u8]) -> Self {
        let (high, low) = match bytes.split_first_chunk::<8>() {
            Some(halves) => halves,
            None => trap(),
        };
        u128::from(be_u64(high)) << 64 | u128::from(be_u64(low))
    }
}

impl Decode<'_> for bool {
    const DESC: Desc = Desc::BOOL;

    #[inline]
    unsafe fn read(bytes: &[u8]) -> Self {
        bytes.first() == Some(&0x80)
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
    const DESC: Desc = Desc::fixed(1);

    #[inline]
    unsafe fn read(bytes: &[u8]) -> Self {
        // SAFETY: the same encoding as `u8`.
        Byte(unsafe { u8::read(bytes) })
    }
}

/// An ARC-4 `address`: a 32-byte public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address(pub [u8; 32]);

impl Decode<'_> for Address {
    const DESC: Desc = Desc::fixed(32);

    #[inline]
    unsafe fn read(bytes: &[u8]) -> Self {
        Address(exact(bytes))
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
        Some(be_u64(low))
    }
}

impl<const BYTES: usize> Decode<'_> for Uint<BYTES> {
    const DESC: Desc = Desc::fixed(BYTES);

    #[inline]
    unsafe fn read(bytes: &[u8]) -> Self {
        Uint(exact(bytes))
    }
}

/// An ARC-4 `ufixedNxM`: an unsigned `BYTES * 8`-bit value `v` representing
/// `v / 10^PRECISION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UFixed<const BYTES: usize, const PRECISION: u8>(pub Uint<BYTES>);

impl<const BYTES: usize, const PRECISION: u8> Decode<'_> for UFixed<BYTES, PRECISION> {
    const DESC: Desc = Desc::fixed(BYTES);

    #[inline]
    unsafe fn read(bytes: &[u8]) -> Self {
        UFixed(Uint(exact(bytes)))
    }
}

impl<'a> Decode<'a> for &'a [u8] {
    const DESC: Desc = Desc::dyn_array(&[u8::DESC]);

    #[inline]
    unsafe fn read(bytes: &'a [u8]) -> Self {
        match desc::dyn_prefix(bytes) {
            Some((_, rest)) => rest,
            None => trap(),
        }
    }
}

impl<'a> Decode<'a> for &'a str {
    const DESC: Desc = Desc::STR;

    #[inline]
    unsafe fn read(bytes: &'a [u8]) -> Self {
        // SAFETY: `Desc::STR` validates as UTF-8, with `utf8::is_valid`,
        // which the tests check against `core::str::from_utf8`.
        unsafe { core::str::from_utf8_unchecked(<&[u8]>::read(bytes)) }
    }
}

/// An application call's arguments, read into one buffer. Used by the
/// `#[contract]` router.
#[doc(hidden)]
pub struct Args {
    buf: [u8; MAX_APP_ARGS_BYTES],
    /// Argument `i` is `buf[bounds[i]..bounds[i + 1]]`.
    bounds: [u16; MAX_APP_ARGS + 1],
}

impl Default for Args {
    fn default() -> Self {
        Self::new()
    }
}

impl Args {
    /// An empty set of arguments, to be filled by [`Args::read`].
    #[inline(always)]
    pub const fn new() -> Args {
        Args {
            buf: [0; MAX_APP_ARGS_BYTES],
            bounds: [0; MAX_APP_ARGS + 1],
        }
    }

    /// Reads application arguments `0..count`, returning whether they fit.
    pub fn read(&mut self, count: u32) -> bool {
        if count as usize > MAX_APP_ARGS {
            return false;
        }
        let mut pos = 0;
        for index in 0..count {
            let Some(rest) = self.buf.get_mut(pos..) else {
                return false;
            };
            let Ok(arg) = crate::txn::app_arg_into(index, rest) else {
                return false;
            };
            pos += arg.len();
            match self.bounds.get_mut(index as usize + 1) {
                Some(end) => *end = pos as u16,
                None => trap(),
            }
        }
        true
    }

    /// Application argument `index`, as read by [`Args::read`].
    #[inline]
    pub fn get(&self, index: usize) -> &[u8] {
        match self.bounds.get(index..index + 2) {
            Some(&[start, end]) => desc::slice(&self.buf, usize::from(start), usize::from(end)),
            _ => trap(),
        }
    }

    /// Whether arguments `1..` are valid encodings of `descs`, in order.
    #[inline(never)]
    pub fn validate(&self, descs: &[&Desc]) -> bool {
        descs
            .iter()
            .enumerate()
            .all(|(i, desc)| validate(self.get(i + 1), desc))
    }

    /// Reads application argument `index` as a `T`.
    ///
    /// # Safety
    ///
    /// [`Args::validate`] must have accepted argument `index` as a `T::DESC`.
    #[inline]
    pub unsafe fn decode<'a, T: Decode<'a>>(&'a self, index: usize) -> T {
        // SAFETY: the caller's contract.
        unsafe { T::read(self.get(index)) }
    }
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
    /// ARC-4 return type, or `void`.
    pub returns: &'static str,
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
