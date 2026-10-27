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

//! `#[contract]` expansion: field keys, the schema, and the `program` export.
//!
//! This is its own test binary because `#[contract]` defines the crate's single
//! `program` export.

use std::cell::RefCell;
use std::collections::BTreeMap;

use avm_sdk::testing::set_host;
use avm_sdk::{
    contract, Address, ArgError, Avm, Contract, Error, GlobalBytes, GlobalU64, TxnError, TxnField,
};

/// A host with global state only; transaction fields read as 0.
#[derive(Default)]
struct StateOnly {
    uints: RefCell<BTreeMap<Vec<u8>, u64>>,
    bytes: RefCell<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl Avm for StateOnly {
    fn arg_count(&self) -> u32 {
        0
    }
    fn arg_into<'a>(&self, _index: u32, _buf: &'a mut [u8]) -> Result<&'a [u8], ArgError> {
        Err(ArgError::OutOfRange)
    }
    fn txn_uint(&self, _field: TxnField) -> u64 {
        0
    }
    fn txn_bytes(&self, _field: TxnField, _buf: &mut [u8]) -> Result<usize, TxnError> {
        Err(TxnError::Unknown)
    }
    fn txna_uint(&self, _field: TxnField, _index: u32) -> u64 {
        0
    }
    fn txna_bytes(
        &self,
        _field: TxnField,
        _index: u32,
        _buf: &mut [u8],
    ) -> Result<usize, TxnError> {
        Err(TxnError::Unknown)
    }
    fn global_get_uint(&self, key: &[u8]) -> u64 {
        self.uints.borrow().get(key).copied().unwrap_or(0)
    }
    fn global_get_bytes<'a>(&self, key: &[u8], buf: &'a mut [u8]) -> Result<&'a [u8], TxnError> {
        let bytes = self.bytes.borrow();
        let value = bytes.get(key).map(Vec::as_slice).unwrap_or(&[]);
        if value.len() > buf.len() {
            return Err(TxnError::BufferTooSmall);
        }
        buf[..value.len()].copy_from_slice(value);
        Ok(&buf[..value.len()])
    }
    fn global_put_uint(&self, key: &[u8], value: u64) {
        self.uints.borrow_mut().insert(key.to_vec(), value);
    }
    fn global_put_bytes(&self, key: &[u8], value: &[u8]) {
        self.bytes.borrow_mut().insert(key.to_vec(), value.to_vec());
    }
    fn global_del(&self, key: &[u8]) {
        self.uints.borrow_mut().remove(key);
        self.bytes.borrow_mut().remove(key);
    }
    fn ed25519_verify_bare(&self, _data: &[u8], _sig: &[u8; 64], _pk: &[u8; 32]) -> bool {
        false
    }
}

#[contract]
pub struct Registry {
    count: GlobalU64,
    #[key = "cap"]
    limit: GlobalU64,
    owner: GlobalBytes,
    delegate: GlobalBytes<Address>,
}

#[contract]
impl Registry {
    pub fn approve(&self) -> Result<(), Error> {
        let count = self.count.get() + 1;
        if count > self.limit.get() {
            return Err(Error::InvalidValue);
        }
        self.count.set(count);
        self.owner.write(b"alice");
        Ok(())
    }
}

#[test]
fn schema_counts_fields() {
    assert_eq!(Registry::GLOBAL_UINTS, 2);
    assert_eq!(Registry::GLOBAL_BYTES, 2);
}

#[test]
fn keys_come_from_field_names_or_key_attribute() {
    let registry = Registry::new();
    assert_eq!(registry.count.key(), b"count");
    assert_eq!(registry.limit.key(), b"cap");
    assert_eq!(registry.owner.key(), b"owner");
    assert_eq!(registry.delegate.key(), b"delegate");
}

#[test]
fn program_export_calls_approve() {
    let host = set_host(StateOnly::default());
    host.uints.borrow_mut().insert(b"cap".to_vec(), 1);

    assert_eq!(program(), 1);
    assert_eq!(host.uints.borrow()[b"count".as_slice()], 1);
    assert_eq!(host.bytes.borrow()[b"owner".as_slice()], b"alice");

    // The limit is reached, so the second call rejects.
    assert_eq!(program(), 0);
    assert_eq!(host.uints.borrow()[b"count".as_slice()], 1);
}
