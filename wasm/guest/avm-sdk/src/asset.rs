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

//! Application-only asset queries. Resource availability follows the AVM: a
//! missing resource returns `None`, but an unavailable reference traps.

#[cfg(feature = "alloc")]
use crate::rust_alloc::{vec, vec::Vec};
#[cfg(target_arch = "wasm32")]
use crate::Avm;
use crate::{host, txn, TxnError, TxnField};

/// A typed asset ID (the actual ID, not an encoded uint8 ARC-4 index).
///
/// ARC-4 methods receive resolved IDs; [`Asset::resolve`] maps a zero-based
/// foreign-asset index to its ID. Asset queries, inner transactions, and typed
/// state slots such as `GlobalUint<Asset>` use this same type. It converts to
/// and from `u64`.
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

impl From<u64> for Asset {
    #[inline]
    fn from(id: u64) -> Self {
        Self { id }
    }
}

impl From<Asset> for u64 {
    #[inline]
    fn from(asset: Asset) -> Self {
        asset.id
    }
}

/// The account's balance, or `None` if not opted in. An opted-in holding with
/// no units returns `Some(0)`. Prefer explicit `Asset` values over reference
/// slots.
#[inline]
pub fn balance(account: &[u8; 32], asset: Asset) -> Option<u64> {
    host().asset_balance(account, asset.id)
}

/// Copies an asset's unit name, returning its length, or `None` for a missing
/// asset. An empty buffer queries length; an existing empty name is `Some(0)`.
/// Errors do not write partial data. Unavailable references trap.
#[inline]
pub fn unit_name_into(asset: Asset, buf: &mut [u8]) -> Result<Option<usize>, TxnError> {
    host().asset_unit_name(asset.id, buf)
}

/// Returns an owned asset unit name, or `None` for a missing asset. An existing
/// empty name is `Some(Vec::new())`. Names are bytes, not necessarily UTF-8.
///
/// Allocates an eight-byte buffer (the protocol maximum) and makes one host
/// call. Requires a guest allocator, e.g. `#[contract(alloc)]` on the impl.
/// Unavailable references trap; oversized names return `BufferTooSmall`.
#[cfg(feature = "alloc")]
#[inline]
pub fn unit_name(asset: Asset) -> Result<Option<Vec<u8>>, TxnError> {
    let mut name = vec![0; 8];
    let Some(len) = unit_name_into(asset, &mut name)? else {
        return Ok(None);
    };
    if len > name.len() {
        return Err(TxnError::BufferTooSmall);
    }
    name.truncate(len);
    Ok(Some(name))
}
