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

//! With an `approve` method, bare calls (no arguments) go to it and method
//! calls still route by selector.

mod common;

use avm_sdk::testing::set_host;
use avm_sdk::{contract, GlobalU64};
use common::{CallAvm, OPT_IN};

#[contract]
pub struct Bare {
    calls: GlobalU64,
}

#[contract]
impl Bare {
    /// Handles every bare call, including opt-ins.
    pub fn approve(&self) -> bool {
        self.calls.set(self.calls.get() + 1);
        true
    }

    #[abimethod]
    pub fn ping(&self) {}
}

const PING: [u8; 4] = [0x18, 0x8b, 0x8f, 0xb4];

#[test]
fn bare_calls_go_to_approve() {
    let mut avm = CallAvm::call(&[]);
    avm.on_completion = OPT_IN;
    let host = set_host(avm);
    assert_eq!(program(), 1);
    assert_eq!(host.uint(b"calls"), Some(1));
}

#[test]
fn method_calls_skip_approve() {
    let host = set_host(CallAvm::call(&[&PING]));
    assert_eq!(program(), 1);
    assert_eq!(host.uint(b"calls"), None);
}
