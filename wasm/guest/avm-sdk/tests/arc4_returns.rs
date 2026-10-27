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

mod common;
use avm_sdk::arc4::{Arc4Contract, RETURN_PREFIX};
use avm_sdk::testing::set_host;
use avm_sdk::{contract, Error};
use common::CallAvm;

#[contract]
struct Returns {}
#[contract]
impl Returns {
    #[abimethod]
    fn value(&self, a: u64) -> Result<u64, Error> {
        if a == 0 {
            Err(Error::InvalidValue)
        } else {
            Ok(a)
        }
    }
    #[abimethod]
    fn flag(&self) -> bool {
        false
    }
    #[abimethod]
    fn tuple(&self) -> (bool, &'static str) {
        (true, "ok")
    }
    #[abimethod]
    fn void(&self) {}
    #[abimethod]
    fn large(&self) -> [u8; 1021] {
        [0; 1021]
    }
}

#[test]
fn nonvoid_signatures_and_return_logs() {
    assert_eq!(Returns::METHODS[0].signature, "value(uint64)uint64");
    assert_eq!(Returns::METHODS[0].returns, "uint64");
    for (index, args, encoded) in [
        (0, vec![u64::MAX.to_be_bytes().to_vec()], vec![255; 8]),
        (1, vec![], vec![0]), // false is an ABI return, not rejection
        (2, vec![], vec![128, 0, 3, 0, 2, b'o', b'k']),
    ] {
        let mut arguments = vec![Returns::METHODS[index].selector.to_vec()];
        arguments.extend(args);
        let slices: Vec<_> = arguments.iter().map(Vec::as_slice).collect();
        let host = set_host(CallAvm::call(&slices));
        assert_eq!(program(), 1);
        assert_eq!(
            *host.logs.borrow(),
            vec![[RETURN_PREFIX.to_vec(), encoded].concat()]
        );
    }
}

#[test]
fn errors_oversized_returns_and_void_do_not_log() {
    for (index, extra, approval) in [(0, vec![vec![0; 8]], 0), (3, vec![], 1), (4, vec![], 0)] {
        let mut args = vec![Returns::METHODS[index].selector.to_vec()];
        args.extend(extra);
        let slices: Vec<_> = args.iter().map(Vec::as_slice).collect();
        let host = set_host(CallAvm::call(&slices));
        assert_eq!(program(), approval);
        assert!(host.logs.borrow().is_empty());
    }
}

#[test]
fn return_log_payload_boundary_includes_dynamic_prefix() {
    for length in [0, 1018, 1019] {
        let host = set_host(CallAvm::call(&[]));
        let payload = vec![42u8; length];
        let pass = avm_sdk::arc4::log_return(&payload.as_slice());
        assert_eq!(pass, length <= 1018);
        if pass {
            let logs = host.logs.borrow();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0].len(), length + 6);
            assert_eq!(&logs[0][..4], &RETURN_PREFIX);
            assert_eq!(&logs[0][4..6], &(length as u16).to_be_bytes());
        } else {
            assert!(host.logs.borrow().is_empty());
        }
    }
}
