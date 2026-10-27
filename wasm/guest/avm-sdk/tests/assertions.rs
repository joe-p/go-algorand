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

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test]
fn successful_assertions_and_optional_trailing_commas() {
    avm_sdk::assert!(true);
    avm_sdk::assert!(true,);
    avm_sdk::assert_eq!(1, 1);
    avm_sdk::assert_eq!(1, 1,);
    avm_sdk::assert_ne!(1, 2);
    avm_sdk::assert_ne!(1, 2,);
}

#[test]
fn failures_trap_natively() {
    assert!(catch_unwind(|| avm_sdk::assert!(false)).is_err());
    assert!(catch_unwind(|| avm_sdk::assert_eq!(1, 2)).is_err());
    assert!(catch_unwind(|| avm_sdk::assert_ne!(1, 1)).is_err());
}

#[test]
fn operands_are_evaluated_once_in_order_on_success_and_failure() {
    let calls = RefCell::new(Vec::new());
    let operand = |value| {
        calls.borrow_mut().push(value);
        value
    };
    avm_sdk::assert!({
        calls.borrow_mut().push(0);
        true
    });
    assert_eq!(*calls.borrow(), vec![0]);
    calls.borrow_mut().clear();
    avm_sdk::assert_eq!(operand(1), operand(1));
    assert_eq!(*calls.borrow(), vec![1, 1]);
    calls.borrow_mut().clear();
    avm_sdk::assert_ne!(operand(1), operand(2));
    assert_eq!(*calls.borrow(), vec![1, 2]);
    calls.borrow_mut().clear();
    assert!(catch_unwind(AssertUnwindSafe(|| avm_sdk::assert_eq!(
        operand(1),
        operand(2)
    )))
    .is_err());
    assert_eq!(*calls.borrow(), vec![1, 2]);
    calls.borrow_mut().clear();
    assert!(catch_unwind(AssertUnwindSafe(|| avm_sdk::assert_ne!(
        operand(3),
        operand(3)
    )))
    .is_err());
    assert_eq!(*calls.borrow(), vec![3, 3]);
}

#[test]
fn operands_need_not_be_copy_or_debug_and_remain_usable() {
    #[derive(PartialEq)]
    struct NoDebug(String);
    let left = NoDebug("left".into());
    let right = NoDebug("right".into());
    avm_sdk::assert_eq!(left, NoDebug("left".into()));
    avm_sdk::assert_ne!(left, right);
    assert_eq!(left.0, "left");
    assert_eq!(right.0, "right");
    avm_sdk::assert_eq!(String::from("hello"), "hello");
}
