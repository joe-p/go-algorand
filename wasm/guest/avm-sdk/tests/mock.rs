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

//! Native tests for the SDK's typed handles and transaction helpers.
//!
//! The `Avm` trait exists so guest logic can be exercised without the wasm
//! executor. `MockAvm` keeps its state behind `RefCell` because the trait takes
//! `&self`, matching the real host.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use avm_sdk::testing::set_host;
use avm_sdk::{
    arg, asset, ed25519_verify_bare, global, globals, txn, ArgError, Avm, Error, GlobalBytes,
    GlobalField, GlobalUint, TxnError, TxnField,
};

#[derive(Clone)]
enum Value {
    Uint(u64),
    Bytes(Vec<u8>),
}

#[derive(Default)]
struct MockAvm {
    globals: RefCell<BTreeMap<Vec<u8>, Value>>,
    args: RefCell<Vec<Vec<u8>>>,
    txn_uints: RefCell<BTreeMap<i32, u64>>,
    txn_bytes: RefCell<BTreeMap<i32, Vec<u8>>>,
    app_args: RefCell<Vec<Vec<u8>>>,
    group: Vec<MockAvm>,
    context_uints: BTreeMap<i32, u64>,
    context_bytes: BTreeMap<i32, Vec<u8>>,
    balances: BTreeMap<([u8; 32], u64), u64>,
    unit_names: BTreeMap<u64, Vec<u8>>,
    inner_calls: Rc<RefCell<Vec<(u32, i32, u64, Vec<u8>)>>>,
}

impl MockAvm {
    fn set_uint(&self, field: TxnField, value: u64) {
        self.txn_uints.borrow_mut().insert(field as i32, value);
    }

    fn set_bytes(&self, field: TxnField, value: &[u8]) {
        self.txn_bytes
            .borrow_mut()
            .insert(field as i32, value.to_vec());
    }
}

impl Avm for MockAvm {
    fn itxn_begin(&self) {
        self.inner_calls.borrow_mut().push((0, 0, 0, vec![]));
    }
    fn itxn_next(&self) {
        self.inner_calls.borrow_mut().push((1, 0, 0, vec![]));
    }
    fn itxn_field_uint(&self, field: TxnField, value: u64) {
        self.inner_calls
            .borrow_mut()
            .push((2, field as i32, value, vec![]));
    }
    fn itxn_field_bytes(&self, field: TxnField, bytes: &[u8]) {
        self.inner_calls
            .borrow_mut()
            .push((3, field as i32, 0, bytes.to_vec()));
    }
    fn itxn_submit(&self) {
        self.inner_calls.borrow_mut().push((4, 0, 0, vec![]));
    }
    fn itxn_created_asset_id(&self) -> u64 {
        1234
    }

    fn asset_balance(&self, account: &[u8; 32], asset: u64) -> Option<u64> {
        self.balances.get(&(*account, asset)).copied()
    }

    fn asset_unit_name(&self, asset: u64, buf: &mut [u8]) -> Result<Option<usize>, TxnError> {
        self.unit_names
            .get(&asset)
            .map(|name| copy_field(name, buf))
            .transpose()
    }

    fn gtxn_uint(&self, group_index: u32, field: TxnField) -> u64 {
        self.group[group_index as usize].txn_uint(field)
    }

    fn gtxn_bytes(
        &self,
        group_index: u32,
        field: TxnField,
        buf: &mut [u8],
    ) -> Result<usize, TxnError> {
        let fields = self.group[group_index as usize].txn_bytes.borrow();
        copy_field(fields.get(&(field as i32)).expect("missing field"), buf)
    }

    fn global_uint(&self, field: GlobalField) -> u64 {
        self.context_uints[&(field as i32)]
    }

    fn global_bytes(&self, field: GlobalField, buf: &mut [u8]) -> Result<usize, TxnError> {
        copy_field(&self.context_bytes[&(field as i32)], buf)
    }

    fn arg_count(&self) -> u32 {
        self.args.borrow().len() as u32
    }

    fn arg_into<'a>(&self, index: u32, buf: &'a mut [u8]) -> Result<&'a [u8], ArgError> {
        let args = self.args.borrow();
        let arg = args.get(index as usize).ok_or(ArgError::OutOfRange)?;
        if arg.len() > buf.len() {
            return Err(ArgError::BufferTooSmall);
        }
        buf[..arg.len()].copy_from_slice(arg);
        Ok(&buf[..arg.len()])
    }

    fn txn_uint(&self, field: TxnField) -> u64 {
        self.txn_uints
            .borrow()
            .get(&(field as i32))
            .copied()
            .unwrap_or(0)
    }

    fn txn_bytes(&self, field: TxnField, buf: &mut [u8]) -> Result<usize, TxnError> {
        let fields = self.txn_bytes.borrow();
        let value = fields.get(&(field as i32)).ok_or(TxnError::Unknown)?;
        if value.len() > buf.len() {
            return Err(TxnError::BufferTooSmall);
        }
        buf[..value.len()].copy_from_slice(value);
        Ok(value.len())
    }

    fn txna_uint(&self, _field: TxnField, _index: u32) -> u64 {
        0
    }

    fn txna_bytes(&self, field: TxnField, index: u32, buf: &mut [u8]) -> Result<usize, TxnError> {
        if field != TxnField::ApplicationArgs {
            return Err(TxnError::Unknown);
        }
        let args = self.app_args.borrow();
        let value = args.get(index as usize).ok_or(TxnError::Unknown)?;
        if value.len() > buf.len() {
            return Err(TxnError::BufferTooSmall);
        }
        buf[..value.len()].copy_from_slice(value);
        Ok(value.len())
    }

    fn global_get_uint(&self, key: &[u8]) -> u64 {
        match self.globals.borrow().get(key) {
            Some(Value::Uint(value)) => *value,
            Some(Value::Bytes(_)) => panic!("uint read of a byte-string value"),
            None => 0,
        }
    }

    fn global_get_bytes<'a>(&self, key: &[u8], buf: &'a mut [u8]) -> Result<&'a [u8], TxnError> {
        match self.globals.borrow().get(key) {
            Some(Value::Bytes(value)) => {
                if value.len() > buf.len() {
                    return Err(TxnError::BufferTooSmall);
                }
                buf[..value.len()].copy_from_slice(value);
                Ok(&buf[..value.len()])
            }
            Some(Value::Uint(_)) => panic!("byte-string read of a uint value"),
            None => Ok(&buf[..0]),
        }
    }

    fn global_put_uint(&self, key: &[u8], value: u64) {
        self.globals
            .borrow_mut()
            .insert(key.to_vec(), Value::Uint(value));
    }

    fn global_put_bytes(&self, key: &[u8], value: &[u8]) {
        self.globals
            .borrow_mut()
            .insert(key.to_vec(), Value::Bytes(value.to_vec()));
    }

    fn global_del(&self, key: &[u8]) {
        self.globals.borrow_mut().remove(key);
    }

    fn ed25519_verify_bare(&self, data: &[u8], sig: &[u8; 64], pk: &[u8; 32]) -> bool {
        data == b"message" && sig == &[0xAB; 64] && pk == &[0xCD; 32]
    }
}

/// A handle declared before any host exists; it resolves the host per access.
const COUNT: GlobalUint = GlobalUint::new(b"count");

fn copy_field(value: &[u8], buf: &mut [u8]) -> Result<usize, TxnError> {
    if !buf.is_empty() {
        if value.len() > buf.len() {
            return Err(TxnError::BufferTooSmall);
        }
        buf[..value.len()].copy_from_slice(value);
    }
    Ok(value.len())
}

#[test]
fn grouped_transactions_and_context_helpers() {
    let pay = MockAvm::default();
    pay.set_uint(TxnField::Amount, u64::MAX);
    pay.set_bytes(TxnField::Receiver, &[0x12; 32]);
    let mut avm = MockAvm::default();
    avm.group.push(pay);
    avm.context_uints.insert(GlobalField::GroupSize as i32, 3);
    avm.context_uints
        .insert(GlobalField::CurrentApplicationID as i32, 888);
    avm.context_bytes.insert(
        GlobalField::CurrentApplicationAddress as i32,
        vec![0x12; 32],
    );
    let _host = set_host(avm);

    assert_eq!(txn::group_uint(0, TxnField::Amount), u64::MAX);
    assert_eq!(globals::group_size(), 3);
    assert_eq!(globals::application_id(), 888);
    assert_eq!(globals::application_address().unwrap(), [0x12; 32]);
    assert_eq!(
        txn::group_bytes(0, TxnField::Receiver, &mut []).unwrap(),
        32
    );
    assert_eq!(
        globals::bytes(GlobalField::CurrentApplicationAddress, &mut []).unwrap(),
        32
    );
    let mut buf = [0; 32];
    assert_eq!(
        txn::group_bytes(0, TxnField::Receiver, &mut buf).unwrap(),
        32
    );
    assert_eq!(buf, [0x12; 32]);
    let mut small = [0xFF; 31];
    assert_eq!(
        txn::group_bytes(0, TxnField::Receiver, &mut small),
        Err(TxnError::BufferTooSmall)
    );
    assert_eq!(small, [0xFF; 31]);
}

#[test]
fn wrong_application_address_length_is_an_error() {
    let mut avm = MockAvm::default();
    avm.context_bytes
        .insert(GlobalField::CurrentApplicationAddress as i32, vec![0; 31]);
    let _host = set_host(avm);
    assert!(matches!(
        globals::application_address(),
        Err(Error::WrongLength {
            expected: 32,
            got: 31
        })
    ));
}

#[test]
fn asset_queries_distinguish_missing_and_empty() {
    let account = [0x12; 32];
    let mut avm = MockAvm::default();
    avm.balances.insert((account, 777), u64::MAX);
    avm.balances.insert((account, 778), 0);
    avm.unit_names.insert(777, b"USDC".to_vec());
    avm.unit_names.insert(778, vec![]);
    let _host = set_host(avm);

    assert_eq!(asset::balance(&account, 777), Some(u64::MAX));
    assert_eq!(asset::balance(&account, 778), Some(0));
    assert_eq!(asset::balance(&account, 779), None);
    assert_eq!(asset::unit_name(777, &mut []).unwrap(), Some(4));
    let mut buf = [0xFF; 8];
    assert_eq!(asset::unit_name(777, &mut buf).unwrap(), Some(4));
    assert_eq!(&buf[..4], b"USDC");
    assert_eq!(asset::unit_name(778, &mut buf).unwrap(), Some(0));
    assert_eq!(asset::unit_name(779, &mut buf).unwrap(), None);
    let mut small = [0xFF; 3];
    assert_eq!(
        asset::unit_name(777, &mut small),
        Err(TxnError::BufferTooSmall)
    );
    assert_eq!(small, [0xFF; 3]);
}

#[test]
fn global_uint_round_trip() {
    let _host = set_host(MockAvm::default());

    assert_eq!(COUNT.get(), 0);
    COUNT.set(7);
    assert_eq!(COUNT.get(), 7);
    COUNT.delete();
    assert_eq!(COUNT.get(), 0);
}

#[test]
fn global_uint_typed() {
    let _host = set_host(MockAvm::default());
    let slot = GlobalUint::<u64>::new_typed(b"typed");

    slot.set(3);
    assert_eq!(global::get_uint(b"typed"), 3);
}

#[test]
fn global_bytes_round_trip() {
    let _host = set_host(MockAvm::default());
    let slot = GlobalBytes::new(b"name");

    let mut buf = [0u8; 16];
    assert_eq!(slot.try_read(&mut buf).unwrap(), b"");
    slot.write(b"alice");
    assert_eq!(slot.read(&mut buf), b"alice");

    let mut small = [0u8; 2];
    assert_eq!(slot.try_read(&mut small), Err(TxnError::BufferTooSmall));

    slot.delete();
    assert_eq!(slot.try_read(&mut buf).unwrap(), b"");
}

#[test]
fn global_runtime_keys() {
    let host = set_host(MockAvm::default());

    global::put_uint(b"n", 5);
    global::put_bytes(b"b", b"xyz");
    assert_eq!(global::get_uint(b"n"), 5);
    let mut buf = [0u8; 8];
    assert_eq!(global::get_bytes(b"b", &mut buf).unwrap(), b"xyz");

    global::delete(b"n");
    assert_eq!(global::get_uint(b"n"), 0);
    assert!(host.globals.borrow().contains_key(b"b".as_slice()));
}

#[test]
fn transaction_helpers() {
    let avm = MockAvm::default();
    avm.set_uint(TxnField::ApplicationID, 888);
    avm.set_uint(TxnField::NumAppArgs, 2);
    avm.set_bytes(TxnField::Sender, &[0x11; 32]);
    *avm.app_args.borrow_mut() = vec![b"first".to_vec(), b"second".to_vec()];
    let _host = set_host(avm);

    assert_eq!(txn::application_id(), 888);
    assert_eq!(txn::uint(TxnField::ApplicationID), 888);
    assert_eq!(txn::app_args_count(), 2);
    assert_eq!(txn::sender().unwrap(), [0x11; 32]);
    assert!(txn::app_arg_eq(0, b"first"));
    assert!(!txn::app_arg_eq(0, b"second"));

    let mut buf = [0u8; 16];
    assert_eq!(txn::app_arg_into(1, &mut buf).unwrap(), b"second");
}

#[test]
fn logic_sig_arguments() {
    let avm = MockAvm::default();
    *avm.args.borrow_mut() = vec![b"signature".to_vec()];
    let _host = set_host(avm);

    assert_eq!(arg::count(), 1);
    let mut buf = [0u8; 16];
    assert_eq!(arg::get(0, &mut buf).unwrap(), b"signature");
    assert_eq!(arg::get(1, &mut buf), Err(ArgError::OutOfRange));
}

#[test]
fn crypto_helper() {
    let _host = set_host(MockAvm::default());

    assert!(ed25519_verify_bare(b"message", &[0xAB; 64], &[0xCD; 32]));
    assert!(!ed25519_verify_bare(b"other", &[0xAB; 64], &[0xCD; 32]));
}

#[test]
fn wrong_sender_length_is_an_error() {
    let avm = MockAvm::default();
    avm.set_bytes(TxnField::Sender, &[0x11; 16]);
    let _host = set_host(avm);

    assert_eq!(
        txn::sender(),
        Err(Error::WrongLength {
            expected: 32,
            got: 16
        })
    );
}

#[test]
fn guard_restores_previous_host() {
    let outer = MockAvm::default();
    outer.set_uint(TxnField::ApplicationID, 1);
    let _outer = set_host(outer);
    {
        let inner = MockAvm::default();
        inner.set_uint(TxnField::ApplicationID, 2);
        let _inner = set_host(inner);
        assert_eq!(txn::application_id(), 2);
    }
    assert_eq!(txn::application_id(), 1);
}

#[test]
fn inner_asset_helpers_use_zero_fees_and_correct_fields() {
    use avm_sdk::itxn;
    let mut mock = MockAvm::default();
    mock.context_bytes
        .insert(GlobalField::CurrentApplicationAddress as i32, vec![7; 32]);
    let calls = Rc::clone(&mock.inner_calls);
    let _guard = set_host(mock);
    let id = itxn::AssetCreation {
        total: u64::MAX,
        decimals: 3,
        asset_name: b"pool",
        unit_name: b"dpt",
        manager: Some([7; 32]),
        reserve: Some([7; 32]),
        ..Default::default()
    }
    .submit();
    assert_eq!(id, 1234);
    assert!(calls
        .borrow()
        .contains(&(2, TxnField::ConfigAssetTotal as i32, u64::MAX, vec![])));
    assert!(calls
        .borrow()
        .contains(&(2, TxnField::Fee as i32, 0, vec![])));
    assert!(calls
        .borrow()
        .contains(&(3, TxnField::ConfigAssetName as i32, 0, b"pool".to_vec())));
    calls.borrow_mut().clear();
    itxn::asset_opt_in(777).unwrap();
    assert_eq!(
        *calls.borrow(),
        vec![
            (0, 0, 0, vec![]),
            (2, TxnField::TypeEnum as i32, 4, vec![]),
            (2, TxnField::Fee as i32, 0, vec![]),
            (2, TxnField::XferAsset as i32, 777, vec![]),
            (2, TxnField::AssetAmount as i32, 0, vec![]),
            (3, TxnField::AssetReceiver as i32, 0, vec![7; 32]),
            (4, 0, 0, vec![]),
        ]
    );
    calls.borrow_mut().clear();
    itxn::asset_transfer(777, &[8; 32], 42);
    assert!(calls
        .borrow()
        .contains(&(2, TxnField::AssetAmount as i32, 42, vec![])));
    assert!(calls
        .borrow()
        .contains(&(3, TxnField::AssetReceiver as i32, 0, vec![8; 32])));
    itxn::begin();
    itxn::next();
    assert_eq!(calls.borrow().last().unwrap().0, 1);
}

#[test]
#[should_panic(expected = "no AVM test host installed")]
fn no_host_panics() {
    txn::application_id();
}
