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

//! A mock host for application calls, shared by the ARC-4 tests.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;

use avm_sdk::{ArgError, Avm, TxnError, TxnField};

/// `OnCompletion` value for an opt-in.
pub const OPT_IN: u64 = 1;

#[derive(Clone)]
enum Value {
    Uint(u64),
    Bytes(Vec<u8>),
}

/// An application call: an app ID (0 for create), an `OnCompletion`, the
/// application arguments, and global state.
pub struct CallAvm {
    pub app_id: u64,
    pub on_completion: u64,
    pub app_args: Vec<Vec<u8>>,
    pub accounts: Vec<[u8; 32]>,
    pub assets: Vec<u64>,
    pub applications: Vec<u64>,
    pub group_types: Vec<u64>,
    pub logs: RefCell<Vec<Vec<u8>>>,
    globals: RefCell<BTreeMap<Vec<u8>, Value>>,
}

impl CallAvm {
    /// A `NoOp` call on existing app 888.
    pub fn call(args: &[&[u8]]) -> Self {
        Self::new(888, args)
    }

    /// A `NoOp` create.
    pub fn create(args: &[&[u8]]) -> Self {
        Self::new(0, args)
    }

    fn new(app_id: u64, args: &[&[u8]]) -> Self {
        CallAvm {
            app_id,
            on_completion: 0,
            app_args: args.iter().map(|arg| arg.to_vec()).collect(),
            accounts: vec![],
            assets: vec![],
            applications: vec![],
            group_types: vec![],
            logs: RefCell::default(),
            globals: RefCell::default(),
        }
    }

    pub fn uint(&self, key: &[u8]) -> Option<u64> {
        match self.globals.borrow().get(key) {
            Some(Value::Uint(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn bytes(&self, key: &[u8]) -> Option<Vec<u8>> {
        match self.globals.borrow().get(key) {
            Some(Value::Bytes(value)) => Some(value.clone()),
            _ => None,
        }
    }
}

fn copy_into(value: &[u8], buf: &mut [u8]) -> usize {
    if value.len() <= buf.len() {
        buf[..value.len()].copy_from_slice(value);
    }
    value.len()
}

impl Avm for CallAvm {
    fn log(&self, bytes: &[u8]) {
        self.logs.borrow_mut().push(bytes.to_vec());
    }
    fn arg_count(&self) -> u32 {
        panic!("arg_count traps in an application program")
    }
    fn arg_into<'a>(&self, _index: u32, _buf: &'a mut [u8]) -> Result<&'a [u8], ArgError> {
        panic!("arg traps in an application program")
    }
    fn txn_uint(&self, field: TxnField) -> u64 {
        match field {
            TxnField::ApplicationID => self.app_id,
            TxnField::OnCompletion => self.on_completion,
            TxnField::NumAppArgs => self.app_args.len() as u64,
            TxnField::NumAccounts => self.accounts.len() as u64,
            TxnField::NumAssets => self.assets.len() as u64,
            TxnField::NumApplications => self.applications.len() as u64,
            TxnField::GroupIndex => self.group_types.len() as u64,
            other => panic!("unexpected field {other:?}"),
        }
    }
    fn txn_bytes(&self, field: TxnField, _buf: &mut [u8]) -> Result<usize, TxnError> {
        panic!("unexpected field {field:?}")
    }
    fn txna_uint(&self, field: TxnField, index: u32) -> u64 {
        match field {
            TxnField::Assets => self.assets[index as usize],
            TxnField::Applications if index == 0 => self.app_id,
            TxnField::Applications => self.applications[index as usize - 1],
            _ => panic!("unexpected field {field:?}"),
        }
    }
    fn gtxn_uint(&self, index: u32, field: TxnField) -> u64 {
        assert_eq!(field, TxnField::TypeEnum);
        self.group_types[index as usize]
    }
    fn txna_bytes(&self, field: TxnField, index: u32, buf: &mut [u8]) -> Result<usize, TxnError> {
        if field == TxnField::Accounts {
            let address = if index == 0 {
                [42; 32]
            } else {
                self.accounts[index as usize - 1]
            };
            return Ok(copy_into(&address, buf));
        }
        assert_eq!(field, TxnField::ApplicationArgs);
        // The AVM fails the program on an out-of-range index.
        let arg = self
            .app_args
            .get(index as usize)
            .expect("ApplicationArgs index out of range");
        Ok(copy_into(arg, buf))
    }
    fn global_get_uint(&self, key: &[u8]) -> u64 {
        self.uint(key).unwrap_or(0)
    }
    fn global_get_bytes<'a>(&self, key: &[u8], buf: &'a mut [u8]) -> Result<&'a [u8], TxnError> {
        let value = self.bytes(key).unwrap_or_default();
        if value.len() > buf.len() {
            return Err(TxnError::BufferTooSmall);
        }
        let len = copy_into(&value, buf);
        Ok(&buf[..len])
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
    fn ed25519_verify_bare(&self, _data: &[u8], _sig: &[u8; 64], _pk: &[u8; 32]) -> bool {
        false
    }
}
