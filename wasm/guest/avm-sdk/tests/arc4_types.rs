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

//! Routing methods with nested ARC-4 types, and more than 15 parameters.

mod common;

use avm_sdk::arc4::{Address, Arc4Contract, Byte, DynArray, UFixed, U256};
use avm_sdk::testing::set_host;
use avm_sdk::{contract, GlobalBytes, GlobalU64};
use common::CallAvm;

#[contract]
pub struct Types {
    total: GlobalU64,
    text: GlobalBytes,
}

#[contract]
impl Types {
    /// Folds every parameter into `total` and `text`, so the test can check
    /// each one decoded.
    #[abimethod]
    #[allow(clippy::too_many_arguments)]
    pub fn complex(
        &self,
        pairs: DynArray<(u16, &[u8])>,
        flags: [bool; 3],
        owner: (Address, U256),
        tag: [u8; 4],
        raw: DynArray<Byte>,
        price: UFixed<8, 2>,
        name: &str,
        small: DynArray<u8>,
    ) {
        let mut total = 0u64;
        let mut text = [0u8; 64];
        let mut len = 0;
        let mut push = |bytes: &[u8]| {
            text[len..len + bytes.len()].copy_from_slice(bytes);
            len += bytes.len();
        };
        for (n, bytes) in pairs {
            total += u64::from(n);
            push(bytes);
        }
        for (i, flag) in flags.iter().enumerate() {
            total += u64::from(*flag) << (8 + i);
        }
        total += u64::from(owner.0 .0[31]) + u64::from(owner.1 .0[31]);
        push(&tag);
        push(raw.as_bytes());
        total += price.0.to_u64().unwrap();
        push(name.as_bytes());
        total += small.iter().map(u64::from).sum::<u64>();
        self.total.set(total);
        self.text.write(&text[..len]);
    }

    /// Records `sum(i * p_i)`, which depends on every parameter and its order.
    #[abimethod]
    #[allow(clippy::too_many_arguments)]
    pub fn wide(
        &self,
        p1: u64,
        p2: u64,
        p3: u64,
        p4: u64,
        p5: u64,
        p6: u64,
        p7: u64,
        p8: u64,
        p9: u64,
        p10: u64,
        p11: u64,
        p12: u64,
        p13: u64,
        p14: u64,
        p15: u64,
        p16: u64,
        p17: u64,
    ) {
        let params = [
            p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11, p12, p13, p14, p15, p16, p17,
        ];
        let total = params
            .iter()
            .enumerate()
            .map(|(i, p)| (i as u64 + 1) * p)
            .sum();
        self.total.set(total);
    }
}

const COMPLEX: [u8; 4] = [0x18, 0xde, 0x28, 0xc3];
const WIDE: [u8; 4] = [0xe0, 0x38, 0x62, 0xa0];

fn dynamic(bytes: &[u8]) -> Vec<u8> {
    let mut out = (bytes.len() as u16).to_be_bytes().to_vec();
    out.extend_from_slice(bytes);
    out
}

#[test]
fn signatures_name_nested_types() {
    let signatures: Vec<_> = Types::METHODS.iter().map(|m| m.signature).collect();
    assert_eq!(
        signatures,
        [
            "complex((uint16,byte[])[],bool[3],(address,uint256),byte[4],byte[],ufixed64x2,string,uint8[])void",
            "wide(uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64,uint64)void",
        ]
    );
    assert_eq!(Types::METHODS[0].selector, COMPLEX);
    assert_eq!(Types::METHODS[1].selector, WIDE);
    assert_eq!(Types::METHODS[0].args[2].ty, "(address,uint256)");
}

#[test]
fn nested_types_route_and_decode() {
    // (uint16,byte[])[] = [(1, "x"), (2, "yz")], from the Go avm-abi encoder.
    let pairs = [
        0x00, 0x02, 0x00, 0x04, 0x00, 0x0b, 0x00, 0x01, 0x00, 0x04, 0x00, 0x01, b'x', 0x00, 0x02,
        0x00, 0x04, 0x00, 0x02, b'y', b'z',
    ];
    let mut owner = vec![0u8; 64];
    owner[31] = 3;
    owner[63] = 4;
    let host = set_host(CallAvm::call(&[
        &COMPLEX,
        &pairs,
        &[0b1010_0000],
        &owner,
        b"tag!",
        &dynamic(b"raw"),
        &250u64.to_be_bytes(),
        &dynamic("名前".as_bytes()),
        &dynamic(&[10, 20]),
    ]));
    assert_eq!(program(), 1);
    let flags = (1 << 8) | (1 << 10);
    assert_eq!(host.uint(b"total"), Some(1 + 2 + flags + 3 + 4 + 250 + 30));
    assert_eq!(
        host.bytes(b"text"),
        Some([&b"xyztag!raw"[..], "名前".as_bytes()].concat())
    );
}

#[test]
fn parameters_past_fifteen_are_packed_into_a_tuple() {
    let params: Vec<[u8; 8]> = (1..=14u64).map(u64::to_be_bytes).collect();
    let packed: Vec<u8> = (15..=17u64).flat_map(u64::to_be_bytes).collect();
    let mut args: Vec<&[u8]> = vec![&WIDE];
    args.extend(params.iter().map(|p| &p[..]));
    args.push(&packed);
    assert_eq!(args.len(), 16);

    let host = set_host(CallAvm::call(&args));
    assert_eq!(program(), 1);
    let expected: u64 = (1..=17u64).map(|i| i * i).sum();
    assert_eq!(host.uint(b"total"), Some(expected));
    drop(host);

    // Passing all 17 separately is not ARC-4, and exceeds the AVM's 16.
    let mut unpacked = args[..15].to_vec();
    let tail: Vec<[u8; 8]> = (15..=17u64).map(u64::to_be_bytes).collect();
    unpacked.extend(tail.iter().map(|p| &p[..]));
    let _host = set_host(CallAvm::call(&unpacked));
    assert_eq!(program(), 0);
}
