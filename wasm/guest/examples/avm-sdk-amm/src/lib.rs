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

use avm_sdk::arc4::{Address, Asset, AssetTransferTxn, PaymentTxn, Transaction};
use avm_sdk::{asset, contract, globals, itxn, txn, Error, GlobalBytes, GlobalUint, TxnField};

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

#[contract]
impl ConstantProductAMM {
    /// Bare NoOp creation, matching createApplication in the source.
    pub fn approve(&self) -> Result<(), Error> {
        require(txn::application_id() == 0 && txn::uint(TxnField::OnCompletion) == 0)?;
        self.governor.write(&txn::sender()?);
        Ok(())
    }

    #[abimethod]
    pub fn set_governor(&self, governor: Address) -> Result<(), Error> {
        self.require_governor()?;
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
        self.require_governor()?;
        require(self.pool_token.get() == 0 && globals::group_size() == 2)?;
        require(a_asset.0 != 0 && a_asset.0 < b_asset.0)?;
        let app = globals::application_address()?;
        require(group_address(seed, TxnField::Receiver)? == app)?;
        require(seed.uint(TxnField::Amount) >= 300_000)?;
        require(group_address(seed, TxnField::CloseRemainderTo)? == [0; 32])?;
        require(group_address(seed, TxnField::RekeyTo)? == [0; 32])?;

        let mut name = [0; 21]; // "DPT-" + two ASA unit names (<=8 bytes) + "-".
        let mut a_unit = [0; 8];
        let mut b_unit = [0; 8];
        let a_len = asset::unit_name(a_asset.0, &mut a_unit)?.ok_or(Error::InvalidValue)?;
        let b_len = asset::unit_name(b_asset.0, &mut b_unit)?.ok_or(Error::InvalidValue)?;
        let name = pool_token_name(
            a_unit.get(..a_len).ok_or(Error::InvalidValue)?,
            b_unit.get(..b_len).ok_or(Error::InvalidValue)?,
            &mut name,
        )?;
        let pool = itxn::AssetCreation {
            total: TOTAL_SUPPLY,
            decimals: 3,
            asset_name: name,
            unit_name: b"dpt",
            manager: Some(app),
            reserve: Some(app),
            ..Default::default()
        }
        .submit();
        require(pool != 0)?;
        itxn::asset_opt_in(a_asset.0)?;
        itxn::asset_opt_in(b_asset.0)?;
        self.asset_a.set(a_asset.0);
        self.asset_b.set(b_asset.0);
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
        self.require_assets(pool_asset, a_asset, b_asset)?;
        let a_amount = verify_transfer(a_xfer, a_asset.0)?;
        let b_amount = verify_transfer(b_xfer, b_asset.0)?;
        let a_supply = sub(balance(a_asset.0)?, a_amount)?;
        let b_supply = sub(balance(b_asset.0)?, b_amount)?;
        let issued = sub(TOTAL_SUPPLY, balance(pool_asset.0)?)?;
        let to_mint = if issued == 0 {
            require(a_supply == 0 && b_supply == 0)?;
            initial_tokens(a_amount, b_amount)
        } else {
            mint_tokens(issued, a_supply, b_supply, a_amount, b_amount)?
        };
        require(to_mint > 0 && to_mint <= balance(pool_asset.0)?)?;
        itxn::asset_transfer(pool_asset.0, &txn::sender()?, to_mint);
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
        self.require_assets(pool_asset, a_asset, b_asset)?;
        let amount = verify_transfer(pool_xfer, pool_asset.0)?;
        let issued = sub(TOTAL_SUPPLY, sub(balance(pool_asset.0)?, amount)?)?;
        require(amount <= issued)?;
        let a_amount = mul_div(balance(a_asset.0)?, amount, issued)?;
        let b_amount = mul_div(balance(b_asset.0)?, amount, issued)?;
        let sender = txn::sender()?;
        itxn::asset_transfer(a_asset.0, &sender, a_amount);
        itxn::asset_transfer(b_asset.0, &sender, b_amount);
        self.update_ratio()
    }

    #[abimethod]
    pub fn swap(
        &self,
        swap_xfer: AssetTransferTxn,
        a_asset: Asset,
        b_asset: Asset,
    ) -> Result<(), Error> {
        self.require_assets(Asset(self.pool_token.get()), a_asset, b_asset)?;
        let in_id = swap_xfer.uint(TxnField::XferAsset);
        require(in_id == a_asset.0 || in_id == b_asset.0)?;
        let amount = verify_transfer(swap_xfer, in_id)?;
        let out_id = if in_id == a_asset.0 {
            b_asset.0
        } else {
            a_asset.0
        };
        let to_swap = swap_tokens(amount, sub(balance(in_id)?, amount)?, balance(out_id)?)?;
        require(to_swap > 0)?;
        itxn::asset_transfer(out_id, &txn::sender()?, to_swap);
        self.update_ratio()
    }
}

impl ConstantProductAMM {
    fn require_governor(&self) -> Result<(), Error> {
        require(self.governor.try_read(&mut [0; 32])? == txn::sender()?)
    }

    fn require_assets(&self, pool: Asset, a: Asset, b: Asset) -> Result<(), Error> {
        require(self.pool_token.get() != 0)?;
        require(
            pool.0 == self.pool_token.get()
                && a.0 == self.asset_a.get()
                && b.0 == self.asset_b.get(),
        )
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

fn require(condition: bool) -> Result<(), Error> {
    if condition {
        Ok(())
    } else {
        Err(Error::InvalidValue)
    }
}

fn sub(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_sub(b).ok_or(Error::InvalidValue)
}

fn pool_token_name<'a>(a: &[u8], b: &[u8], out: &'a mut [u8]) -> Result<&'a [u8], Error> {
    require(a.len() <= 8 && b.len() <= 8)?;
    let name = out
        .get_mut(..5 + a.len() + b.len())
        .ok_or(Error::InvalidValue)?;
    // Iterator copies avoid both indexing panics and copy_from_slice's
    // length-mismatch panic machinery. The checked slice above fits every byte.
    let bytes = b"DPT-".iter().chain(a).chain(b"-").chain(b);
    for (dst, src) in name.iter_mut().zip(bytes) {
        *dst = *src;
    }
    Ok(name)
}

fn balance(id: u64) -> Result<u64, Error> {
    asset::balance(&globals::application_address()?, id).ok_or(Error::InvalidValue)
}

fn group_address<const KIND: u64>(
    transaction: Transaction<KIND>,
    field: TxnField,
) -> Result<[u8; 32], Error> {
    let mut address = [0; 32];
    require(transaction.bytes(field, &mut address)? == 32)?;
    Ok(address)
}

fn verify_transfer(transfer: AssetTransferTxn, id: u64) -> Result<u64, Error> {
    require(transfer.uint(TxnField::XferAsset) == id)?;
    let amount = transfer.uint(TxnField::AssetAmount);
    require(amount > 0)?;
    require(group_address(transfer, TxnField::Sender)? == txn::sender()?)?;
    require(group_address(transfer, TxnField::AssetReceiver)? == globals::application_address()?)?;
    // Reject clawbacks, close-outs, and rekeys rather than treating them as deposits.
    for field in [
        TxnField::AssetSender,
        TxnField::AssetCloseTo,
        TxnField::RekeyTo,
    ] {
        require(group_address(transfer, field)? == [0; 32])?;
    }
    Ok(amount)
}

fn mul_div(a: u64, b: u64, denominator: u64) -> Result<u64, Error> {
    let value = (u128::from(a) * u128::from(b))
        .checked_div(u128::from(denominator))
        .ok_or(Error::InvalidValue)?;
    u64::try_from(value).map_err(|_| Error::InvalidValue)
}

fn initial_tokens(a: u64, b: u64) -> u64 {
    let product = u128::from(a) * u128::from(b);
    // Binary search with an exclusive u128 upper bound, including u64::MAX.
    let (mut low, mut high) = (0u128, 1u128 << 64);
    while low + 1 < high {
        let mid = (low + high) / 2;
        if mid * mid <= product {
            low = mid;
        } else {
            high = mid;
        }
    }
    low as u64
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
