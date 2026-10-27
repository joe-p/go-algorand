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

//! ARC-4 asset/account/application references, a preceding asset transaction,
//! and a uint64 return log. This is a routing fixture, not an AMM.
#![cfg_attr(target_arch = "wasm32", no_std)]

use avm_sdk::arc4::{Account, AssetTransferTxn};
use avm_sdk::{asset::Asset, contract, txn, Application, Error, TxnField};
avm_sdk::panic_handler!();

#[contract]
pub struct Resources {}

#[contract]
impl Resources {
    #[abimethod]
    pub fn deposit(
        &self,
        asset: Asset,
        transfer: AssetTransferTxn,
        receiver: Account,
        application: Application,
    ) -> Result<u64, Error> {
        if asset.id != transfer.uint(TxnField::XferAsset) || application != txn::application_id() {
            return Err(Error::InvalidValue);
        }
        if transfer.address(TxnField::AssetReceiver)? != receiver.address {
            return Err(Error::InvalidValue);
        }
        Ok(transfer.uint(TxnField::AssetAmount))
    }
}
