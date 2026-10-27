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

//! Top-level ARC-4 references and preceding group transactions.

#[cfg(feature = "alloc")]
use crate::rust_alloc::vec::Vec;
use crate::{txn, Error, TxnError, TxnField};

/// A resolved ARC-4 `asset` (the actual asset ID, not the encoded uint8 index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub id: u64,
}

impl Asset {
    /// Resolves a zero-based index into this call's foreign assets.
    pub fn resolve(index: u8) -> Option<Self> {
        (u64::from(index) < txn::uint(TxnField::NumAssets)).then(|| Self {
            id: txn::array_uint(TxnField::Assets, u32::from(index)),
        })
    }
}

/// A resolved ARC-4 `application`; index 0 means the current transaction's
/// ApplicationID (0 during creation), not the assigned creation ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Application {
    pub id: u64,
}

impl Application {
    pub fn resolve(index: u8) -> Option<Self> {
        (u64::from(index) <= txn::uint(TxnField::NumApplications)).then(|| Self {
            id: txn::array_uint(TxnField::Applications, u32::from(index)),
        })
    }
}

/// A resolved ARC-4 `account`; index 0 means the sender, indices 1.. select
/// this call's foreign accounts. This is distinct from an ARC-4 `address`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account {
    pub address: [u8; 32],
}

impl Account {
    pub fn resolve(index: u8) -> Option<Self> {
        if u64::from(index) > txn::uint(TxnField::NumAccounts) {
            return None;
        }
        let mut address = [0; 32];
        (txn::array_bytes(TxnField::Accounts, u32::from(index), &mut address).ok()? == 32)
            .then_some(Self { address })
    }
}

/// An ARC-4 transaction parameter. It consumes no application argument: the
/// router selects the immediately preceding transactions in signature order.
/// KIND=0 accepts any type; the aliases select an exact AVM TypeEnum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transaction<const KIND: u64 = 0> {
    group_index: u32,
}

pub type PaymentTxn = Transaction<1>;
pub type KeyRegistrationTxn = Transaction<2>;
pub type AssetConfigTxn = Transaction<3>;
pub type AssetTransferTxn = Transaction<4>;
pub type AssetFreezeTxn = Transaction<5>;
pub type ApplicationCallTxn = Transaction<6>;

impl<const KIND: u64> Transaction<KIND> {
    /// Resolves transaction `offset` within the `count` preceding transactions.
    /// Rejects underflow, non-preceding offsets, and wrong transaction types.
    #[doc(hidden)]
    pub fn resolve(offset: u32, count: u32) -> Option<Self> {
        if offset >= count {
            return None;
        }
        let current = u32::try_from(txn::uint(TxnField::GroupIndex)).ok()?;
        let group_index = current.checked_sub(count)?.checked_add(offset)?;
        if KIND != 0 && txn::group_uint(group_index, TxnField::TypeEnum) != KIND {
            return None;
        }
        Some(Self { group_index })
    }

    pub fn group_index(self) -> u32 {
        self.group_index
    }
    pub fn uint(self, field: TxnField) -> u64 {
        txn::group_uint(self.group_index, field)
    }
    pub fn bytes(self, field: TxnField, out: &mut [u8]) -> Result<usize, TxnError> {
        txn::group_bytes(self.group_index, field, out)
    }

    /// Returns an owned byte field. Allocates 4 KiB; see [`txn::group_bytes_owned`].
    #[cfg(feature = "alloc")]
    pub fn bytes_owned(self, field: TxnField) -> Result<Vec<u8>, TxnError> {
        txn::group_bytes_owned(self.group_index, field)
    }

    /// Reads a 32-byte field without allocating; see [`txn::group_address`].
    pub fn address(self, field: TxnField) -> Result<[u8; 32], Error> {
        txn::group_address(self.group_index, field)
    }
}
