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

//! Constant-product ASA/ASA AMM ported from TEALScript's example.
//! See ../README.md for intentional fixes and deployment limitations.
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::vec::Vec;
use avm_sdk::arc4::{Address, Asset, AssetTransferTxn, PaymentTxn};
use avm_sdk::{
    assert, assert_eq, assert_ne, asset, contract, globals, itxn, math, txn, Error, GlobalBytes,
    GlobalUint, TxnField,
};

avm_sdk::panic_handler!();

pub const TOTAL_SUPPLY: u64 = 10_000_000_000;
pub const SCALE: u64 = 1_000;
pub const FEE: u64 = 5;

#[contract]
pub struct ConstantProductAMM {
    #[key = "g"]
    governor: GlobalBytes,
    #[key = "a"]
    asset_a: GlobalUint,
    #[key = "b"]
    asset_b: GlobalUint,
    #[key = "p"]
    pool_token: GlobalUint,
    #[key = "r"]
    ratio: GlobalUint,
}

#[contract(alloc)]
impl ConstantProductAMM {
    /// Bare NoOp creation, matching createApplication in the source.
    pub fn approve(&self) -> Result<(), Error> {
        assert!(txn::application_id() == 0 && txn::uint(TxnField::OnCompletion) == 0);
        self.governor.write(&txn::sender()?);
        Ok(())
    }

    #[abimethod]
    pub fn set_governor(&self, governor: Address) -> Result<(), Error> {
        self.assert_governor()?;
        self.governor.write(&governor.0);
        Ok(())
    }

    #[abimethod]
    pub fn bootstrap(
        &self,
        seed: PaymentTxn,
        a_asset: Asset,
        b_asset: Asset,
    ) -> Result<u64, Error> {
        self.assert_governor()?;
        assert!(self.pool_token.get() == 0 && globals::group_size() == 2);
        assert!(a_asset.id != 0 && a_asset.id < b_asset.id);
        let app = globals::application_address()?;
        assert_eq!(seed.address(TxnField::Receiver)?, app);
        assert!(seed.uint(TxnField::Amount) >= 300_000);
        assert_eq!(seed.address(TxnField::CloseRemainderTo)?, [0; 32]);
        assert_eq!(seed.address(TxnField::RekeyTo)?, [0; 32]);

        let a_unit = asset::unit_name(a_asset.id)?.ok_or(Error::InvalidValue)?;
        let b_unit = asset::unit_name(b_asset.id)?.ok_or(Error::InvalidValue)?;
        let name = pool_token_name(&a_unit, &b_unit);
        let pool = itxn::AssetCreation {
            total: TOTAL_SUPPLY,
            decimals: 3,
            asset_name: &name,
            unit_name: b"dpt",
            manager: Some(app),
            reserve: Some(app),
            ..Default::default()
        }
        .submit();
        assert_ne!(pool, 0);
        itxn::asset_opt_in(a_asset.id)?;
        itxn::asset_opt_in(b_asset.id)?;
        self.asset_a.set(a_asset.id);
        self.asset_b.set(b_asset.id);
        self.pool_token.set(pool);
        Ok(pool)
    }

    #[abimethod]
    pub fn mint(
        &self,
        a_xfer: AssetTransferTxn,
        b_xfer: AssetTransferTxn,
        pool_asset: Asset,
        a_asset: Asset,
        b_asset: Asset,
    ) -> Result<(), Error> {
        self.assert_assets(pool_asset, a_asset, b_asset);
        let a_amount = verify_transfer(a_xfer, a_asset.id)?;
        let b_amount = verify_transfer(b_xfer, b_asset.id)?;
        let a_supply = sub(balance(a_asset.id)?, a_amount)?;
        let b_supply = sub(balance(b_asset.id)?, b_amount)?;
        let issued = sub(TOTAL_SUPPLY, balance(pool_asset.id)?)?;
        let to_mint = if issued == 0 {
            assert!(a_supply == 0 && b_supply == 0);
            initial_tokens(a_amount, b_amount)
        } else {
            mint_tokens(issued, a_supply, b_supply, a_amount, b_amount)?
        };
        assert!(to_mint > 0 && to_mint <= balance(pool_asset.id)?);
        itxn::asset_transfer(pool_asset.id, &txn::sender()?, to_mint);
        Ok(())
    }

    #[abimethod]
    pub fn burn(
        &self,
        pool_xfer: AssetTransferTxn,
        pool_asset: Asset,
        a_asset: Asset,
        b_asset: Asset,
    ) -> Result<(), Error> {
        self.assert_assets(pool_asset, a_asset, b_asset);
        let amount = verify_transfer(pool_xfer, pool_asset.id)?;
        let issued = sub(TOTAL_SUPPLY, sub(balance(pool_asset.id)?, amount)?)?;
        assert!(amount <= issued);
        let a_amount = mul_div(balance(a_asset.id)?, amount, issued)?;
        let b_amount = mul_div(balance(b_asset.id)?, amount, issued)?;
        let sender = txn::sender()?;
        itxn::asset_transfer(a_asset.id, &sender, a_amount);
        itxn::asset_transfer(b_asset.id, &sender, b_amount);
        self.update_ratio()
    }

    #[abimethod]
    pub fn swap(
        &self,
        swap_xfer: AssetTransferTxn,
        a_asset: Asset,
        b_asset: Asset,
    ) -> Result<(), Error> {
        self.assert_assets(
            Asset {
                id: self.pool_token.get(),
            },
            a_asset,
            b_asset,
        );
        let in_id = swap_xfer.uint(TxnField::XferAsset);
        assert!(in_id == a_asset.id || in_id == b_asset.id);
        let amount = verify_transfer(swap_xfer, in_id)?;
        let out_id = if in_id == a_asset.id {
            b_asset.id
        } else {
            a_asset.id
        };
        let to_swap = swap_tokens(amount, sub(balance(in_id)?, amount)?, balance(out_id)?)?;
        assert!(to_swap > 0);
        itxn::asset_transfer(out_id, &txn::sender()?, to_swap);
        self.update_ratio()
    }
}

impl ConstantProductAMM {
    fn assert_governor(&self) -> Result<(), Error> {
        assert_eq!(self.governor.try_read_owned()?, txn::sender()?);
        Ok(())
    }

    fn assert_assets(&self, pool: Asset, a: Asset, b: Asset) {
        assert_ne!(self.pool_token.get(), 0);
        assert_eq!(pool.id, self.pool_token.get());
        assert_eq!(a.id, self.asset_a.get());
        assert_eq!(b.id, self.asset_b.get());
    }

    fn update_ratio(&self) -> Result<(), Error> {
        let a = balance(self.asset_a.get())?;
        let b = balance(self.asset_b.get())?;
        // A complete withdrawal leaves an empty pool, not a division by zero.
        self.ratio.set(if a == 0 && b == 0 {
            0
        } else {
            mul_div(a, SCALE, b)?
        });
        Ok(())
    }
}

fn sub(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_sub(b).ok_or(Error::InvalidValue)
}

fn pool_token_name(a: &[u8], b: &[u8]) -> Vec<u8> {
    assert!(a.len() <= 8 && b.len() <= 8);
    [b"DPT-".as_slice(), a, b"-", b].concat()
}

fn balance(id: u64) -> Result<u64, Error> {
    asset::balance(&globals::application_address()?, id).ok_or(Error::InvalidValue)
}

fn verify_transfer(transfer: AssetTransferTxn, id: u64) -> Result<u64, Error> {
    assert_eq!(transfer.uint(TxnField::XferAsset), id);
    let amount = transfer.uint(TxnField::AssetAmount);
    assert!(amount > 0);
    assert_eq!(transfer.address(TxnField::Sender)?, txn::sender()?);
    assert_eq!(
        transfer.address(TxnField::AssetReceiver)?,
        globals::application_address()?
    );
    // Reject clawbacks, close-outs, and rekeys rather than treating them as deposits.
    for field in [
        TxnField::AssetSender,
        TxnField::AssetCloseTo,
        TxnField::RekeyTo,
    ] {
        assert_eq!(transfer.address(field)?, [0; 32]);
    }
    Ok(amount)
}

fn mul_div(a: u64, b: u64, denominator: u64) -> Result<u64, Error> {
    if denominator == 0 {
        return Err(Error::InvalidValue);
    }
    let (high, low) = math::mulw(a, b);
    let (quotient_high, quotient_low, _, _) = math::divmodw(high, low, 0, denominator);
    if quotient_high != 0 {
        return Err(Error::InvalidValue);
    }
    Ok(quotient_low)
}

fn initial_tokens(a: u64, b: u64) -> u64 {
    let (high, low) = math::mulw(a, b);
    math::sqrtw(high, low)
}

fn mint_tokens(issued: u64, a_supply: u64, b_supply: u64, a: u64, b: u64) -> Result<u64, Error> {
    // Preserve the source's two-stage rounding at SCALE precision.
    let ratio = mul_div(a, SCALE, a_supply)?.min(mul_div(b, SCALE, b_supply)?);
    mul_div(ratio, issued, SCALE)
}

fn swap_tokens(amount: u64, in_supply: u64, out_supply: u64) -> Result<u64, Error> {
    // Like the source's uint64 denominator, reject overflow instead of wrapping.
    let weighted = amount.checked_mul(SCALE - FEE).ok_or(Error::InvalidValue)?;
    let denominator = in_supply
        .checked_mul(SCALE)
        .and_then(|supply| supply.checked_add(weighted))
        .ok_or(Error::InvalidValue)?;
    mul_div(weighted, out_supply, denominator)
}

#[cfg(test)]
mod tests;
