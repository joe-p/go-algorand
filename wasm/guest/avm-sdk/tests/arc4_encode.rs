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

use avm_sdk::arc4::{Address, Byte, Decode, DynArray, Encode, UFixed, Uint};

fn check<T: Encode>(value: T, expected: &[u8]) {
    let mut out = [0xff; 2048];
    let n = value.encode(&mut out).unwrap();
    assert_eq!(&out[..n], expected);
    if n > 0 {
        assert!(value.encode(&mut out[..n - 1]).is_err());
    }
}

#[test]
fn scalar_and_dynamic_golden_encodings() {
    check(0x0102030405060708u64, &[1, 2, 3, 4, 5, 6, 7, 8]);
    check(u128::MAX, &[255; 16]);
    check(Uint([255; 64]), &[255; 64]);
    check(UFixed::<2, 3>(Uint([1, 2])), &[1, 2]);
    check(Byte(42), &[42]);
    check(Address([7; 32]), &[7; 32]);
    check(false, &[0]);
    check(true, &[128]);
    check("hi", &[0, 2, b'h', b'i']);
    check(&b"abc"[..], &[0, 3, b'a', b'b', b'c']);
    check("", &[0, 0]);
}

#[test]
fn tuple_offsets_and_boolean_runs_match_arc4() {
    check(
        (true, false, true, 0x1234u16, "hi", &b"x"[..]),
        &[0xa0, 0x12, 0x34, 0, 7, 0, 11, 0, 2, b'h', b'i', 0, 1, b'x'],
    );
    check([true; 9], &[255, 128]);
    check(([true; 9], false, true), &[255, 128, 64]);
    check(
        (
            true, true, true, true, true, true, true, true, true, 42u8, true,
        ),
        &[255, 128, 42, 128],
    );
    check(["a", "bc"], &[0, 4, 0, 7, 0, 1, b'a', 0, 2, b'b', b'c']);
    check(
        (("a", 9u8), "b"),
        &[0, 4, 0, 10, 0, 3, 9, 0, 1, b'a', 0, 1, b'b'],
    );
}

#[test]
fn borrowed_arrays_reencode_and_all_small_boolean_runs_roundtrip() {
    let empty: [&str; 0] = [];
    check((empty, 42u8), &[0, 3, 42]);
    let encoded = [0, 2, 0, 4, 0, 7, 0, 1, b'a', 0, 2, b'b', b'c'];
    let array = DynArray::<&str>::decode(&encoded).unwrap();
    check(array, &encoded);
    for mask in 0..512u16 {
        let bits: [bool; 9] = core::array::from_fn(|i| mask & (1 << i) != 0);
        let value = (true, bits, false, "tail");
        let mut out = [0; 64];
        let n = value.encode(&mut out).unwrap();
        let got = <(bool, [bool; 9], bool, &str)>::decode(&out[..n]).unwrap();
        assert_eq!(got, value);
    }
}
