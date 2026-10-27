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

use super::*;
use avm_sdk::arc4::{Arc4Contract, RETURN_PREFIX};
use avm_sdk::testing::set_host;
use avm_sdk::{ArgError, Avm, GlobalField, TxnError};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

const USER: [u8; 32] = [7; 32];
const APP: [u8; 32] = [8; 32];
const A: u64 = 10;
const B: u64 = 20;
const POOL: u64 = 30;

#[derive(Default, Clone)]
struct Fields {
    uints: BTreeMap<u32, u64>,
    bytes: BTreeMap<u32, Vec<u8>>,
}

impl Fields {
    fn uint(&self, field: TxnField) -> u64 {
        *self.uints.get(&(field as u32)).unwrap_or(&0)
    }

    fn bytes(&self, field: TxnField, out: &mut [u8]) -> usize {
        copy(
            self.bytes
                .get(&(field as u32))
                .map(Vec::as_slice)
                .unwrap_or(&[0; 32]),
            out,
        )
    }
}

fn copy(value: &[u8], out: &mut [u8]) -> usize {
    if value.len() <= out.len() {
        out[..value.len()].copy_from_slice(value);
    }
    value.len()
}

struct Mock {
    app_id: Cell<u64>,
    completion: Cell<u64>,
    sender: Cell<[u8; 32]>,
    group: RefCell<Vec<Fields>>,
    args: RefCell<Vec<Vec<u8>>>,
    uints: RefCell<BTreeMap<Vec<u8>, u64>>,
    bytes: RefCell<BTreeMap<Vec<u8>, Vec<u8>>>,
    balances: RefCell<BTreeMap<u64, u64>>,
    unit_names: RefCell<BTreeMap<u64, Vec<u8>>>,
    pending: RefCell<Fields>,
    inner: RefCell<Vec<Fields>>,
    logs: RefCell<Vec<Vec<u8>>>,
}

impl Mock {
    fn new() -> Self {
        Self {
            app_id: Cell::new(1),
            completion: Cell::new(0),
            sender: Cell::new(USER),
            group: RefCell::default(),
            args: RefCell::default(),
            uints: RefCell::default(),
            bytes: RefCell::default(),
            balances: RefCell::default(),
            unit_names: RefCell::new([(A, b"AAA".to_vec()), (B, b"BBB".to_vec())].into()),
            pending: RefCell::default(),
            inner: RefCell::default(),
            logs: RefCell::default(),
        }
    }

    fn call(&self, method: &str, args: &[&[u8]], group: Vec<Fields>) -> i32 {
        let selector = ConstantProductAMM::METHODS
            .iter()
            .find(|m| m.name == method)
            .unwrap()
            .selector;
        *self.args.borrow_mut() = std::iter::once(selector.to_vec())
            .chain(args.iter().map(|a| a.to_vec()))
            .collect();
        *self.group.borrow_mut() = group;
        super::program()
    }

    fn initialized(&self) {
        self.global_put_bytes(b"g", &USER);
        self.global_put_uint(b"a", A);
        self.global_put_uint(b"b", B);
        self.global_put_uint(b"p", POOL);
    }

    fn holdings(&self, a: u64, b: u64, pool: u64) {
        *self.balances.borrow_mut() = [(A, a), (B, b), (POOL, pool)].into();
    }
}

impl Avm for Mock {
    fn arg_count(&self) -> u32 {
        unimplemented!()
    }
    fn arg_into<'a>(&self, _: u32, _: &'a mut [u8]) -> Result<&'a [u8], ArgError> {
        unimplemented!()
    }
    fn txn_uint(&self, field: TxnField) -> u64 {
        match field {
            TxnField::ApplicationID => self.app_id.get(),
            TxnField::OnCompletion => self.completion.get(),
            TxnField::NumAppArgs => self.args.borrow().len() as u64,
            TxnField::GroupIndex => self.group.borrow().len() as u64,
            TxnField::NumAssets => 3,
            _ => panic!("unexpected field {field:?}"),
        }
    }
    fn txn_bytes(&self, field: TxnField, out: &mut [u8]) -> Result<usize, TxnError> {
        assert_eq!(field, TxnField::Sender);
        Ok(copy(&self.sender.get(), out))
    }
    fn txna_uint(&self, field: TxnField, index: u32) -> u64 {
        assert_eq!(field, TxnField::Assets);
        [A, B, POOL][index as usize]
    }
    fn txna_bytes(&self, field: TxnField, index: u32, out: &mut [u8]) -> Result<usize, TxnError> {
        assert_eq!(field, TxnField::ApplicationArgs);
        Ok(copy(&self.args.borrow()[index as usize], out))
    }
    fn gtxn_uint(&self, index: u32, field: TxnField) -> u64 {
        self.group.borrow()[index as usize].uint(field)
    }
    fn gtxn_bytes(&self, index: u32, field: TxnField, out: &mut [u8]) -> Result<usize, TxnError> {
        Ok(self.group.borrow()[index as usize].bytes(field, out))
    }
    fn global_uint(&self, field: GlobalField) -> u64 {
        assert_eq!(field, GlobalField::GroupSize);
        self.group.borrow().len() as u64 + 1
    }
    fn global_bytes(&self, field: GlobalField, out: &mut [u8]) -> Result<usize, TxnError> {
        assert_eq!(field, GlobalField::CurrentApplicationAddress);
        Ok(copy(&APP, out))
    }
    fn global_get_uint(&self, key: &[u8]) -> u64 {
        self.uints.borrow().get(key).copied().unwrap_or(0)
    }
    fn global_put_uint(&self, key: &[u8], value: u64) {
        self.uints.borrow_mut().insert(key.to_vec(), value);
    }
    fn global_get_bytes<'a>(&self, key: &[u8], out: &'a mut [u8]) -> Result<&'a [u8], TxnError> {
        let bytes = self.bytes.borrow();
        let value = bytes.get(key).map(Vec::as_slice).unwrap_or_default();
        if value.len() > out.len() {
            return Err(TxnError::BufferTooSmall);
        }
        let len = copy(value, out);
        Ok(&out[..len])
    }
    fn global_put_bytes(&self, key: &[u8], value: &[u8]) {
        self.bytes.borrow_mut().insert(key.to_vec(), value.to_vec());
    }
    fn global_del(&self, _: &[u8]) {
        unimplemented!()
    }
    fn ed25519_verify_bare(&self, _: &[u8], _: &[u8; 64], _: &[u8; 32]) -> bool {
        unimplemented!()
    }
    fn asset_balance(&self, account: &[u8; 32], asset: u64) -> Option<u64> {
        assert_eq!(*account, APP);
        self.balances.borrow().get(&asset).copied()
    }
    fn asset_unit_name(&self, asset: u64, out: &mut [u8]) -> Result<Option<usize>, TxnError> {
        Ok(self
            .unit_names
            .borrow()
            .get(&asset)
            .map(|name| copy(name, out)))
    }
    fn itxn_begin(&self) {
        *self.pending.borrow_mut() = Fields::default();
    }
    fn itxn_field_uint(&self, field: TxnField, value: u64) {
        self.pending.borrow_mut().uints.insert(field as u32, value);
    }
    fn itxn_field_bytes(&self, field: TxnField, value: &[u8]) {
        self.pending
            .borrow_mut()
            .bytes
            .insert(field as u32, value.to_vec());
    }
    fn itxn_submit(&self) {
        let inner = self.pending.borrow().clone();
        assert_eq!(inner.uint(TxnField::Fee), 0);
        if inner.uint(TxnField::TypeEnum) == 3 {
            self.balances
                .borrow_mut()
                .insert(POOL, inner.uint(TxnField::ConfigAssetTotal));
        } else {
            let id = inner.uint(TxnField::XferAsset);
            let amount = inner.uint(TxnField::AssetAmount);
            let mut balances = self.balances.borrow_mut();
            let holding = balances.entry(id).or_default();
            if inner.bytes.get(&(TxnField::AssetReceiver as u32)).unwrap() != &APP {
                *holding = holding.checked_sub(amount).unwrap();
            }
        }
        self.inner.borrow_mut().push(inner);
    }
    fn itxn_created_asset_id(&self) -> u64 {
        POOL
    }
    fn log(&self, bytes: &[u8]) {
        self.logs.borrow_mut().push(bytes.to_vec());
    }
}

fn transfer(id: u64, amount: u64) -> Fields {
    Fields {
        uints: [
            (TxnField::TypeEnum as u32, 4),
            (TxnField::XferAsset as u32, id),
            (TxnField::AssetAmount as u32, amount),
        ]
        .into(),
        bytes: [
            (TxnField::Sender as u32, USER.to_vec()),
            (TxnField::AssetReceiver as u32, APP.to_vec()),
        ]
        .into(),
    }
}

fn seed(amount: u64) -> Fields {
    Fields {
        uints: [
            (TxnField::TypeEnum as u32, 1),
            (TxnField::Amount as u32, amount),
        ]
        .into(),
        bytes: [(TxnField::Receiver as u32, APP.to_vec())].into(),
    }
}

#[test]
fn creation_governance_and_lifecycle() {
    let host = set_host(Mock::new());
    host.app_id.set(0);
    assert_eq!(super::program(), 1);
    assert_eq!(host.bytes.borrow().get(b"g".as_slice()).unwrap(), &USER);
    host.app_id.set(1);
    assert_eq!(super::program(), 0);
    assert_eq!(host.call("set_governor", &[&[9; 32]], vec![]), 1);
    assert_eq!(host.call("set_governor", &[&USER], vec![]), 0);
    host.sender.set([9; 32]);
    assert_eq!(host.call("set_governor", &[&USER], vec![]), 1);
    host.completion.set(4); // UpdateApplication is not authorized.
    assert_eq!(host.call("set_governor", &[&USER], vec![]), 0);
}

#[test]
fn bootstrap_creates_pool_and_opt_ins_and_returns_id() {
    let host = set_host(Mock::new());
    host.global_put_bytes(b"g", &USER);
    assert_eq!(
        host.call("bootstrap", &[&[0], &[1]], vec![seed(300_000)]),
        1
    );
    assert_eq!(host.global_get_uint(b"p"), POOL);
    let inner = host.inner.borrow();
    assert_eq!(inner.len(), 3);
    assert_eq!(inner[0].uint(TxnField::ConfigAssetTotal), TOTAL_SUPPLY);
    assert_eq!(inner[0].uint(TxnField::ConfigAssetDecimals), 3);
    assert_eq!(
        inner[0].bytes[&(TxnField::ConfigAssetName as u32)],
        b"DPT-AAA-BBB"
    );
    assert_eq!(inner[0].bytes[&(TxnField::ConfigAssetManager as u32)], APP);
    assert_eq!(inner[0].bytes[&(TxnField::ConfigAssetReserve as u32)], APP);
    assert_eq!(inner[1].uint(TxnField::XferAsset), A);
    assert_eq!(inner[2].uint(TxnField::XferAsset), B);
    assert_eq!(
        host.logs.borrow()[0],
        [RETURN_PREFIX.as_slice(), &POOL.to_be_bytes()].concat()
    );
    drop(inner);
    assert_eq!(
        host.call("bootstrap", &[&[0], &[1]], vec![seed(300_000)]),
        0
    );
}

#[test]
fn bootstrap_rejects_bad_seed_order_and_sender() {
    for case in 0..6 {
        let host = set_host(Mock::new());
        host.global_put_bytes(b"g", &USER);
        let mut payment = seed(300_000);
        let mut args: [&[u8]; 2] = [&[0], &[1]];
        match case {
            0 => {
                payment.uints.insert(TxnField::Amount as u32, 299_999);
            }
            1 => {
                payment
                    .bytes
                    .insert(TxnField::Receiver as u32, USER.to_vec());
            }
            2 => {
                payment
                    .bytes
                    .insert(TxnField::CloseRemainderTo as u32, USER.to_vec());
            }
            3 => {
                payment
                    .bytes
                    .insert(TxnField::RekeyTo as u32, USER.to_vec());
            }
            4 => {
                args = [&[1], &[0]];
            }
            5 => {
                host.sender.set([9; 32]);
            }
            _ => unreachable!(),
        }
        assert_eq!(host.call("bootstrap", &args, vec![payment]), 0);
        assert!(host.inner.borrow().is_empty());
    }
}

#[test]
fn initial_and_subsequent_mints_transfer_lp_tokens() {
    for (a, b, pool, deposit_a, deposit_b, minted) in [
        (
            1_000_000,
            4_000_000,
            TOTAL_SUPPLY,
            1_000_000,
            4_000_000,
            2_000_000,
        ),
        (
            1_100_000,
            4_800_000,
            TOTAL_SUPPLY - 2_000_000,
            100_000,
            800_000,
            200_000,
        ),
    ] {
        let host = set_host(Mock::new());
        host.initialized();
        host.holdings(a, b, pool);
        assert_eq!(
            host.call(
                "mint",
                &[&[2], &[0], &[1]],
                vec![transfer(A, deposit_a), transfer(B, deposit_b)]
            ),
            1
        );
        let inner = host.inner.borrow();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].uint(TxnField::XferAsset), POOL);
        assert_eq!(inner[0].uint(TxnField::AssetAmount), minted);
    }
}

#[test]
fn swap_both_directions_returns_opposite_asset_and_preserves_product() {
    for (input, output, a, b, expected) in [
        (A, B, 1_010_000, 2_000_000, 19_703),
        (B, A, 2_000_000, 1_010_000, 19_703),
    ] {
        let host = set_host(Mock::new());
        host.initialized();
        host.holdings(a, b, TOTAL_SUPPLY - 1_000_000);
        assert_eq!(
            host.call("swap", &[&[0], &[1]], vec![transfer(input, 10_000)]),
            1
        );
        let inner = host.inner.borrow();
        assert_eq!(inner[0].uint(TxnField::XferAsset), output);
        assert_eq!(inner[0].uint(TxnField::AssetAmount), expected);
        let balances = host.balances.borrow();
        assert!(u128::from(balances[&A]) * u128::from(balances[&B]) >= 2_000_000_000_000);
        assert_eq!(
            host.global_get_uint(b"r"),
            balances[&A] * SCALE / balances[&B]
        );
    }
}

#[test]
fn partial_and_full_burn_use_issued_supply_before_deposit() {
    for amount in [500_000, 2_000_000] {
        let host = set_host(Mock::new());
        host.initialized();
        host.holdings(1_000_000, 4_000_000, TOTAL_SUPPLY - 2_000_000 + amount);
        assert_eq!(
            host.call("burn", &[&[2], &[0], &[1]], vec![transfer(POOL, amount)]),
            1
        );
        let inner = host.inner.borrow();
        assert_eq!(inner[0].uint(TxnField::AssetAmount), amount / 2);
        assert_eq!(inner[1].uint(TxnField::AssetAmount), amount * 2);
        assert_eq!(
            host.global_get_uint(b"r"),
            if amount == 2_000_000 { 0 } else { 250 }
        );
    }
}

#[test]
fn reject_malformed_deposits_before_any_inner_transaction() {
    for case in 0..10 {
        let host = set_host(Mock::new());
        host.initialized();
        host.holdings(1_010_000, 2_000_000, TOTAL_SUPPLY - 1_000_000);
        let mut deposit = transfer(A, 10_000);
        match case {
            0 => {
                deposit.uints.insert(TxnField::AssetAmount as u32, 0);
            }
            1 => {
                deposit.uints.insert(TxnField::XferAsset as u32, POOL);
            }
            2 => {
                deposit.bytes.insert(TxnField::Sender as u32, APP.to_vec());
            }
            3 => {
                deposit
                    .bytes
                    .insert(TxnField::AssetReceiver as u32, USER.to_vec());
            }
            4 => {
                deposit
                    .bytes
                    .insert(TxnField::AssetSender as u32, USER.to_vec());
            }
            5 => {
                deposit
                    .bytes
                    .insert(TxnField::AssetCloseTo as u32, USER.to_vec());
            }
            6 => {
                deposit
                    .bytes
                    .insert(TxnField::RekeyTo as u32, USER.to_vec());
            }
            7 => {
                deposit.uints.insert(TxnField::TypeEnum as u32, 1);
            }
            8 => {
                host.global_put_uint(b"p", 0);
            }
            9 => {
                deposit
                    .uints
                    .insert(TxnField::AssetAmount as u32, 1_010_001);
            }
            _ => unreachable!(),
        }
        assert_eq!(
            host.call("swap", &[&[0], &[1]], vec![deposit]),
            0,
            "case {case}"
        );
        assert!(host.inner.borrow().is_empty());
    }
}

#[test]
fn rounding_overflow_and_zero_denominators() {
    assert_eq!(initial_tokens(0, u64::MAX), 0);
    assert_eq!(initial_tokens(2, 4), 2);
    assert_eq!(initial_tokens(u64::MAX, u64::MAX), u64::MAX);
    assert_eq!(mul_div(u64::MAX, u64::MAX, u64::MAX), Ok(u64::MAX));
    assert!(mul_div(u64::MAX, 2, 1).is_err());
    assert!(mul_div(1, 1, 0).is_err());
    assert_eq!(
        mint_tokens(1_000_000, 1_000_000, 1_000_000, 999, 999),
        Ok(0)
    );
    assert!(mint_tokens(1, 0, 1, 1, 1).is_err());
    assert_eq!(swap_tokens(1, 1_000_000, 1), Ok(0));
    assert!(swap_tokens(u64::MAX, 1, 1).is_err());
    assert!(swap_tokens(1, u64::MAX, 1).is_err());
    assert!(swap_tokens(0, 0, 1).is_err());
}

#[test]
fn pool_names_cover_all_unit_name_lengths_and_short_output_buffers() {
    for a_len in 0..=8 {
        for b_len in 0..=8 {
            let a = vec![b'A'; a_len];
            let b = vec![b'B'; b_len];
            let expected = [b"DPT-".as_slice(), &a, b"-", &b].concat();
            let mut out = [0xff; 22];
            assert_eq!(pool_token_name(&a, &b, &mut out), Ok(expected.as_slice()));
            assert!(out[expected.len()..].iter().all(|byte| *byte == 0xff));
            for capacity in 0..expected.len() {
                let mut short = vec![0xff; capacity];
                assert!(pool_token_name(&a, &b, &mut short).is_err());
                assert!(short.iter().all(|byte| *byte == 0xff));
            }
        }
    }
    for (a, b) in [(vec![b'A'; 9], vec![]), (vec![], vec![b'B'; 9])] {
        let mut out = [0xff; 32];
        assert!(pool_token_name(&a, &b, &mut out).is_err());
        assert_eq!(out, [0xff; 32]);
    }
}

#[test]
fn bootstrap_rejects_missing_or_oversized_unit_names_without_panicking() {
    for asset in [A, B] {
        for oversized in [false, true] {
            let host = set_host(Mock::new());
            host.global_put_bytes(b"g", &USER);
            if oversized {
                // The host reports the required length without writing a partial name.
                host.unit_names.borrow_mut().insert(asset, vec![b'X'; 9]);
            } else {
                host.unit_names.borrow_mut().remove(&asset);
            }
            assert_eq!(
                host.call("bootstrap", &[&[0], &[1]], vec![seed(300_000)]),
                0
            );
            assert!(host.inner.borrow().is_empty());
        }
    }
}
