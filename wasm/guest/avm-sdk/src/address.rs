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

//! Typed Algorand addresses.
//!
//! [`Address`] is shared by ARC-4 address values, transaction/context address
//! helpers, asset queries, inner transactions, and typed state slots such as
//! `GlobalBytes<Address>`.

use crate::global::StateBytes;

/// A 32-byte Algorand address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address(pub [u8; 32]);

impl Address {
    /// The all-zero address, used for fields that are not set.
    pub const ZERO: Address = Address([0; 32]);
}

impl From<[u8; 32]> for Address {
    #[inline]
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<Address> for [u8; 32] {
    #[inline]
    fn from(address: Address) -> Self {
        address.0
    }
}

impl AsRef<[u8]> for Address {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl StateBytes for Address {
    const LEN: Option<usize> = Some(32);

    #[inline]
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok().map(Address)
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
