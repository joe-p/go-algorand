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

//! Application-only inner asset transactions. Protocol, fee, and resource
//! failures trap. Inner application calls and non-asset types are not exposed.

#[cfg(target_arch = "wasm32")]
use crate::Avm;
use crate::{address::Address, asset::Asset, globals, host, TxnField};

/// Begins a group with one pending transaction; mirrors `itxn_begin` defaults.
#[inline]
pub fn begin() {
    host().itxn_begin()
}

/// Adds the next transaction to the pending group.
#[inline]
pub fn next() {
    host().itxn_next()
}

/// Sets a uint field, including TypeEnum (acfg=3, axfer=4).
#[inline]
pub fn set_uint(field: TxnField, value: u64) {
    host().itxn_field_uint(field, value)
}

/// Copies a byte-string field into the pending transaction.
#[inline]
pub fn set_bytes(field: TxnField, bytes: &[u8]) {
    host().itxn_field_bytes(field, bytes)
}

/// Executes the pending group through the shared AVM submission path.
#[inline]
pub fn submit() {
    host().itxn_submit()
}

/// Last submitted inner transaction's created asset. A non-creation
/// transaction yields `Asset { id: 0 }`.
#[inline]
pub fn created_asset_id() -> Asset {
    host().itxn_created_asset_id().into()
}

/// Minimal creation parameters for a pool token. Omitted authorities remain
/// zero. Fee defaults to 0, requiring sufficient outer fee credit; unlike the
/// raw begin helper, this never silently spends the application's Algo on fees.
#[derive(Default)]
pub struct AssetCreation<'a> {
    pub total: u64,
    pub decimals: u32,
    pub asset_name: &'a [u8],
    pub unit_name: &'a [u8],
    pub manager: Option<Address>,
    pub reserve: Option<Address>,
    pub fee: u64,
}

impl AssetCreation<'_> {
    /// Creates the asset in a standalone inner group and returns it.
    pub fn submit(&self) -> Asset {
        begin();
        set_uint(TxnField::TypeEnum, 3);
        set_uint(TxnField::Fee, self.fee);
        set_uint(TxnField::ConfigAssetTotal, self.total);
        set_uint(TxnField::ConfigAssetDecimals, u64::from(self.decimals));
        set_bytes(TxnField::ConfigAssetName, self.asset_name);
        set_bytes(TxnField::ConfigAssetUnitName, self.unit_name);
        if let Some(addr) = self.manager.as_ref() {
            set_bytes(TxnField::ConfigAssetManager, addr.as_ref());
        }
        if let Some(addr) = self.reserve.as_ref() {
            set_bytes(TxnField::ConfigAssetReserve, addr.as_ref());
        }
        submit();
        created_asset_id()
    }
}

/// Sends an asset from the application with Fee=0, using outer fee credit.
/// The receiver and asset/holding must be available under AVM resource rules.
pub fn asset_transfer(asset: Asset, receiver: &Address, amount: u64) {
    begin();
    set_uint(TxnField::TypeEnum, 4);
    set_uint(TxnField::Fee, 0);
    set_uint(TxnField::XferAsset, asset.id);
    set_uint(TxnField::AssetAmount, amount);
    set_bytes(TxnField::AssetReceiver, receiver.as_ref());
    submit();
}

/// Opts the application into an asset via a zero-amount transfer to itself.
pub fn asset_opt_in(asset: Asset) {
    asset_transfer(asset, &globals::application_address(), 0);
}
