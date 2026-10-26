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

use avm_sdk::{
    ActiveAvm, ArgError, Avm, AvmExt, Error, GlobalBytes, GlobalUint, TxnError, TxnField,
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
}

impl MockAvm {
    fn leak(self) -> ActiveAvm {
        Box::leak(Box::new(self))
    }

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

#[test]
fn global_uint_round_trip() {
    let avm = MockAvm::default().leak();
    let counter = GlobalUint::new(avm, b"count");

    assert_eq!(counter.get(), 0);
    counter.set(7u64);
    assert_eq!(counter.get(), 7);
    counter.delete();
    assert_eq!(counter.get(), 0);
}

#[test]
fn global_bytes_round_trip() {
    let avm = MockAvm::default().leak();
    let slot = GlobalBytes::new(avm, b"name");

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
fn transaction_helpers() {
    let avm = MockAvm::default();
    avm.set_uint(TxnField::ApplicationID, 888);
    avm.set_uint(TxnField::NumAppArgs, 2);
    avm.set_bytes(TxnField::Sender, &[0x11; 32]);
    *avm.app_args.borrow_mut() = vec![b"first".to_vec(), b"second".to_vec()];
    let avm = avm.leak();

    assert_eq!(avm.application_id(), 888);
    assert_eq!(avm.app_args_count(), 2);
    assert_eq!(avm.sender().unwrap(), [0x11; 32]);
    assert!(avm.app_arg_eq(0, b"first"));
    assert!(!avm.app_arg_eq(0, b"second"));

    let mut buf = [0u8; 16];
    assert_eq!(avm.app_arg_into(1, &mut buf).unwrap(), b"second");
}

#[test]
fn logic_sig_arguments() {
    let avm = MockAvm::default();
    *avm.args.borrow_mut() = vec![b"signature".to_vec()];
    let avm = avm.leak();

    assert_eq!(avm.arg_count(), 1);
    let mut buf = [0u8; 16];
    assert_eq!(avm.arg_into(0, &mut buf).unwrap(), b"signature");
    assert_eq!(avm.arg_into(1, &mut buf), Err(ArgError::OutOfRange));
}

#[test]
fn crypto_helper() {
    let avm = MockAvm::default().leak();

    assert!(avm.ed25519_verify_bare(b"message", &[0xAB; 64], &[0xCD; 32]));
    assert!(!avm.ed25519_verify_bare(b"other", &[0xAB; 64], &[0xCD; 32]));
}

#[test]
fn wrong_sender_length_is_an_error() {
    let avm = MockAvm::default();
    avm.set_bytes(TxnField::Sender, &[0x11; 16]);
    let avm = avm.leak();

    assert_eq!(
        avm.sender(),
        Err(Error::WrongLength {
            expected: 32,
            got: 16
        })
    );
}
