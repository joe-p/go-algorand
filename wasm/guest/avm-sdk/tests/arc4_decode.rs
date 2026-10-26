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

//! `arc4::Decode` against encodings produced by the Go `avm-abi` package.

use avm_sdk::arc4::{Address, Byte, Decode, DecodeError, DynArray, UFixed, Uint, U256};

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn decode<'a, T: Decode<'a>>(bytes: &'a [u8]) -> T {
    T::decode(bytes).unwrap_or_else(|err| panic!("decode {bytes:02x?}: {err:?}"))
}

#[test]
fn tuple_with_string_and_packed_bools() {
    // (uint64,string,bool[3]) = (7, "hi", [true, false, true])
    let bytes = hex("0000000000000007000ba000026869");
    let (n, s, flags): (u64, &str, [bool; 3]) = decode(&bytes);
    assert_eq!((n, s, flags), (7, "hi", [true, false, true]));
}

#[test]
fn dynamic_arrays() {
    let bytes = hex("0003000100020003");
    let values: DynArray<u16> = decode(&bytes);
    assert_eq!(values.iter().collect::<Vec<_>>(), [1, 2, 3]);

    let bytes = hex("000300060009000d000161000262630000");
    let names: DynArray<&str> = decode(&bytes);
    assert_eq!(names.iter().collect::<Vec<_>>(), ["a", "bc", ""]);
    assert_eq!(names.get(1), Some("bc"));
    assert_eq!(names.get(3), None);

    let bytes = hex("0009b180");
    let bits: DynArray<bool> = decode(&bytes);
    assert_eq!(
        bits.iter().collect::<Vec<_>>(),
        [true, false, true, true, false, false, false, true, true]
    );
}

#[test]
fn consecutive_bools_pack_in_tuples() {
    // (bool,bool,uint8,bool) = (true, false, 5, true)
    let bytes = hex("800580");
    let value: (bool, bool, u8, bool) = decode(&bytes);
    assert_eq!(value, (true, false, 5, true));
    assert_eq!(<(bool, bool, u8, bool)>::STATIC_LEN, Some(3));
}

#[test]
fn arrays_of_tuples_and_nested_dynamic_values() {
    // (uint16,byte[])[] = [(1, "x"), (2, "yz")]
    let bytes = hex("00020004000b00010004000178000200040002797a");
    let pairs: DynArray<(u16, &[u8])> = decode(&bytes);
    assert_eq!(
        pairs.iter().collect::<Vec<_>>(),
        [(1, &b"x"[..]), (2, &b"yz"[..])]
    );

    // (string,uint16[])[] = [("p", [1]), ("", [])]
    let bytes = hex("00020004000f00040007000170000100010004000600000000");
    let entries: DynArray<(&str, DynArray<u16>)> = decode(&bytes);
    let entries: Vec<_> = entries
        .iter()
        .map(|(s, values)| (s, values.iter().collect::<Vec<_>>()))
        .collect();
    assert_eq!(entries, [("p", vec![1]), ("", vec![])]);

    // bool[][] = [[true], [], [false, true]]
    let bytes = hex("000300060009000b0001800000000240");
    let rows: DynArray<DynArray<bool>> = decode(&bytes);
    let rows: Vec<Vec<bool>> = rows.iter().map(|row| row.iter().collect()).collect();
    assert_eq!(rows, [vec![true], vec![], vec![false, true]]);
}

#[test]
fn nested_tuple_with_static_array_and_address() {
    // ((uint8,string),bool[2],address) = ((9, "nested"), [false, true], 0..32)
    let bytes = hex("002340000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f09000300066e6573746564");
    let ((n, s), flags, addr): ((u8, &str), [bool; 2], Address) = decode(&bytes);
    assert_eq!((n, s, flags), (9, "nested", [false, true]));
    assert_eq!(addr.0, core::array::from_fn::<u8, 32, _>(|i| i as u8));
}

#[test]
fn wide_and_fixed_point_numbers() {
    let bytes = hex("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20");
    let value: U256 = decode(&bytes);
    assert_eq!(value.0[0], 1);
    assert_eq!(value.0[31], 0x20);
    assert_eq!(value.to_u64(), None);

    let fixed: UFixed<8, 2> = decode(&hex("0000000000003039"));
    assert_eq!(fixed.0.to_u64(), Some(12345));

    let wide: u128 = decode(&hex("00000010000000000000000000000000"));
    assert_eq!(wide, 1 << 100);

    let small: Uint<3> = decode(&hex("000102"));
    assert_eq!(small.to_u64(), Some(0x0102));

    assert_eq!(decode::<Byte>(&hex("07")), Byte(7));
}

#[test]
fn static_arrays() {
    assert_eq!(decode::<[u8; 4]>(&hex("01020304")), [1, 2, 3, 4]);
    assert_eq!(
        decode::<[&str; 2]>(&hex("0004000800026162000163")),
        ["ab", "c"]
    );
    assert_eq!(
        decode::<[bool; 10]>(&hex("c040")),
        [true, true, false, false, false, false, false, false, false, true]
    );
    assert_eq!(<[bool; 10]>::STATIC_LEN, Some(2));
    assert_eq!(<[&str; 2]>::STATIC_LEN, None);
}

#[test]
fn empty_tuple() {
    assert_eq!(decode::<()>(&[]), ());
    assert!(<()>::decode(&[0]).is_err());
}

#[test]
fn malformed_encodings_are_rejected() {
    type Check = fn(&[u8]) -> Result<(), DecodeError>;
    let cases: &[(&str, Check)] = &[
        // Trailing byte after a static tuple.
        ("80058000", |b| {
            <(bool, bool, u8, bool)>::decode(b).map(drop)
        }),
        // Truncated head.
        ("8005", |b| <(bool, bool, u8, bool)>::decode(b).map(drop)),
        // First tail offset not right after the heads (0x000c instead of 0x000b).
        ("0000000000000007000ca000026869", |b| {
            <(u64, &str, [bool; 3])>::decode(b).map(drop)
        }),
        // A string length prefix that overruns its tail.
        ("0000000000000007000ba000036869", |b| {
            <(u64, &str, [bool; 3])>::decode(b).map(drop)
        }),
        // Array length larger than its elements.
        ("0004000100020003", |b| DynArray::<u16>::decode(b).map(drop)),
        // Array element offsets out of order.
        ("000300060005000d000161000262630000", |b| {
            DynArray::<&str>::decode(b).map(drop)
        }),
        // Offset past the end.
        ("00010010", |b| DynArray::<&str>::decode(b).map(drop)),
        // Invalid UTF-8 inside an array of strings.
        ("0001000200 01ff", |b| DynArray::<&str>::decode(b).map(drop)),
        // A standalone bool that is neither 0x00 nor 0x80.
        ("01", |b| bool::decode(b).map(drop)),
        // Wrong static array size.
        ("010203", |b| <[u8; 4]>::decode(b).map(drop)),
        // Missing array length prefix.
        ("00", |b| DynArray::<u8>::decode(b).map(drop)),
    ];
    for (encoding, decode) in cases {
        let bytes = hex(&encoding.replace(' ', ""));
        assert!(decode(&bytes).is_err(), "accepted {encoding}");
    }
}
