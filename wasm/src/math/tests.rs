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

use super::*;
use crate::{instantiate, run_inner, validate_inner, HostCallbacks, DEFAULT_MEMORY_LIMIT};
use wasmi::TrapCode;

const IMPORTS: &str = r#"
    (import "env" "mulw" (func $mulw (param i64 i64 i32)))
    (import "env" "divmodw" (func $divmodw (param i64 i64 i64 i64 i32)))
    (import "env" "sqrt" (func $sqrt (param i64) (result i64)))
    (import "env" "sqrtw" (func $sqrtw (param i64 i64) (result i64)))
"#;

fn module(body: &str, memory: bool) -> Vec<u8> {
    let memory = if memory {
        r#"(memory (export "memory") 1)"#
    } else {
        ""
    };
    wat::parse_str(format!(
        r#"(module {IMPORTS} {memory}
        (func (export "program") (result i32) {body}))"#
    ))
    .unwrap()
}

fn check(body: &str) {
    let wasm = module(body, true);
    validate_inner(&wasm).unwrap();
    for is_app in [false, true] {
        let host = HostCallbacks {
            is_app,
            ..Default::default()
        };
        let outcome = run_inner(&wasm, vec![], 1_000_000, host).unwrap();
        assert!(outcome.pass, "{body}");
        assert!(
            run_inner(&wasm, vec![], outcome.fuel_used, host)
                .unwrap()
                .pass
        );
        assert!(run_inner(&wasm, vec![], outcome.fuel_used - 1, host).is_err());
    }
}

fn words_check(words: &[u64], out: usize) -> String {
    let mut result = String::new();
    for (i, value) in words.iter().enumerate() {
        result.push_str(&format!(
            "(if (i64.ne (i64.load (i32.const {})) (i64.const {})) (then unreachable))\n",
            out + i * 8,
            *value as i64
        ));
    }
    result.push_str("i32.const 1");
    result
}

#[test]
fn unsigned_wide_arithmetic_and_word_order() {
    let mut pairs = vec![
        (0, 0),
        (0, u64::MAX),
        (1, 1),
        (1, u64::MAX),
        (1 << 63, 2),
        (u64::MAX, u64::MAX),
    ];
    let mut seed = 0x123456789abcdef0u64;
    for _ in 0..16 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let a = seed;
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        pairs.push((a, seed));
    }
    for (a, b) in pairs {
        let product = u128::from(a) * u128::from(b);
        // An unaligned result ending exactly at the memory boundary also works.
        for out in [1, 65520] {
            check(&format!(
                "(call $mulw (i64.const {}) (i64.const {}) (i32.const {out})) {}",
                a as i64,
                b as i64,
                words_check(&[(product >> 64) as u64, product as u64], out)
            ));
        }
    }
    for (a, b) in [
        (0, 1),
        (1, 2),
        (u128::MAX, 1),
        (u128::MAX, u128::MAX),
        (u128::MAX, (1u128 << 64) + 1),
        (u128::MAX, 1 << 127),
        (1 << 126, 1 << 127),
        (1 << 127, (1 << 64) - 1),
        ((123456789u128 << 64) | 987654321, 123456789),
    ] {
        let (q, r) = (a / b, a % b);
        for out in [1, 65504] {
            check(&format!("(call $divmodw (i64.const {}) (i64.const {}) (i64.const {}) (i64.const {}) (i32.const {out})) {}",
                (a >> 64) as i64, a as i64, (b >> 64) as i64, b as i64,
                words_check(&[(q >> 64) as u64, q as u64, (r >> 64) as u64, r as u64], out)));
        }
    }
}

// Independent integer-only reference, not std's isqrt used by the host.
fn sqrt_reference(n: u128) -> u64 {
    let (mut low, mut high) = (0, 1u128 << 64);
    while low + 1 < high {
        let mid = (low + high) / 2;
        if mid * mid <= n {
            low = mid;
        } else {
            high = mid;
        }
    }
    low as u64
}

#[test]
fn roots_floor_at_boundaries_without_memory() {
    let mut values = vec![
        0,
        1,
        2,
        3,
        4,
        8,
        9,
        10,
        u128::from(u64::MAX),
        1 << 64,
        1 << 127,
        u128::MAX,
    ];
    for root in [2u128, 65535, u32::MAX.into(), u64::MAX.into()] {
        let square = root * root;
        values.extend([square - 1, square, square + 1]);
    }
    for value in values {
        let expected = sqrt_reference(value) as i64;
        let mut body = format!(
            "(i64.eq (call $sqrtw (i64.const {}) (i64.const {})) (i64.const {expected}))",
            (value >> 64) as i64,
            value as i64
        );
        if value <= u128::from(u64::MAX) {
            body = format!(
                "(i32.and {body} (i64.eq (call $sqrt (i64.const {})) (i64.const {expected})))",
                value as i64
            );
        }
        let wasm = module(&body, false);
        validate_inner(&wasm).unwrap();
        assert!(
            run_inner(&wasm, vec![], 1_000_000, HostCallbacks::default())
                .unwrap()
                .pass
        );
    }
}

#[test]
fn bad_result_pointers_and_zero_divisor_trap() {
    for (name, size, args) in [
        ("mulw", 16, "(i64.const 2) (i64.const 3)"),
        (
            "divmodw",
            32,
            "(i64.const 0) (i64.const 7) (i64.const 0) (i64.const 3)",
        ),
    ] {
        for (ptr, memory, message) in [
            (-1, true, "negative math result pointer"),
            (65536 - size + 1, true, "outside linear memory"),
            (i32::MAX, true, "outside linear memory"),
            (0, false, "missing memory export"),
        ] {
            let wasm = module(
                &format!("(call ${name} {args} (i32.const {ptr})) i32.const 1"),
                memory,
            );
            let error = run_inner(&wasm, vec![], 1_000_000, HostCallbacks::default()).unwrap_err();
            assert!(error.contains(message), "{error}");
        }
    }
    let wasm = module("(call $divmodw (i64.const -1) (i64.const -1) (i64.const 0) (i64.const 0) (i32.const 0)) i32.const 1", true);
    let error = run_inner(&wasm, vec![], 1_000_000, HostCallbacks::default()).unwrap_err();
    assert!(error.contains("division by zero"), "{error}");
}

#[test]
fn charges_precede_work_and_failed_calls_do_not_partially_write() {
    let wasm = module("i32.const 1", true);
    let (mut store, instance, _) = instantiate(
        &wasm,
        vec![],
        DEFAULT_MEMORY_LIMIT,
        1_000_000,
        HostCallbacks::default(),
    )
    .unwrap();
    let memory = instance.get_memory(&store, "memory").unwrap();
    macro_rules! metered {
        ($func:ident, $cost:ident, $params:ty, $result:ty, $args:expr, $needs_memory:expr) => {{
            let func = wasmi::Func::wrap(&mut store, $func)
                .typed::<$params, $result>(&store)
                .unwrap();
            memory.data_mut(&mut store).fill(0x7e);
            store.set_fuel($cost - 1).unwrap();
            assert_eq!(
                func.call(&mut store, $args).unwrap_err().as_trap_code(),
                Some(TrapCode::OutOfFuel)
            );
            assert!(memory.data(&store).iter().all(|b| *b == 0x7e));
            store.set_fuel($cost).unwrap();
            let result = func.call(&mut store, $args);
            // Direct host calls have no guest instance in Caller. Wide calls
            // therefore charge exactly their weight, then reject missing memory.
            if $needs_memory {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("missing memory export"));
            } else {
                result.unwrap();
            }
            assert_eq!(store.get_fuel().unwrap(), 0);
        }};
    }
    metered!(mulw, MULW_FUEL_COST, (i64, i64, i32), (), (2, 3, 0), true);
    metered!(
        divmodw,
        DIVMODW_FUEL_COST,
        (i64, i64, i64, i64, i32),
        (),
        (0, 7, 0, 3, 0),
        true
    );
    metered!(sqrt, SQRT_FUEL_COST, i64, i64, -1, false);
    metered!(sqrtw, SQRTW_FUEL_COST, (i64, i64), i64, (-1, -1), false);

    let div = wasmi::Func::wrap(&mut store, divmodw)
        .typed::<(i64, i64, i64, i64, i32), ()>(&store)
        .unwrap();
    for call in [
        "(call $mulw (i64.const 2) (i64.const 3) (i32.const 65521))",
        "(call $divmodw (i64.const 0) (i64.const 7) (i64.const 0) (i64.const 3) (i32.const 65505))",
        "(call $divmodw (i64.const 0) (i64.const 7) (i64.const 0) (i64.const 0) (i32.const 0))",
    ] {
        let wasm = module(&format!("{call} i32.const 1"), true);
        let (mut store, instance, program) = instantiate(
            &wasm,
            vec![],
            DEFAULT_MEMORY_LIMIT,
            1_000_000,
            HostCallbacks::default(),
        )
        .unwrap();
        let memory = instance.get_memory(&store, "memory").unwrap();
        memory.data_mut(&mut store).fill(0x7e);
        assert!(program.call(&mut store, ()).is_err());
        assert!(memory.data(&store).iter().all(|b| *b == 0x7e));
    }
    store.set_fuel(DIVMODW_FUEL_COST - 1).unwrap();
    assert_eq!(
        div.call(&mut store, (0, 7, 0, 0, -1))
            .unwrap_err()
            .as_trap_code(),
        Some(TrapCode::OutOfFuel)
    );
}

#[test]
fn rejects_wrong_math_import_signatures() {
    for (name, params, results) in [
        ("mulw", "i64 i64 i32", ""),
        ("divmodw", "i64 i64 i64 i64 i32", ""),
        ("sqrt", "i64", "(result i64)"),
        ("sqrtw", "i64 i64", "(result i64)"),
    ] {
        for declaration in [
            format!("(param {params} i32) {results}"),
            format!("(param {params}) (result i32)"),
        ] {
            let wasm = wat::parse_str(format!(
                r#"(module (import "env" "{name}" (func {declaration}))
                (func (export "program") (result i32) i32.const 1))"#
            ))
            .unwrap();
            assert!(validate_inner(&wasm).is_err(), "{name}/{declaration}");
        }
    }
}
