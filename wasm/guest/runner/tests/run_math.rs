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

//! Exercises SDK -> avm-sys -> WASM imports -> real executor, including word
//! order, unsigned bit patterns, quotient/remainder, and integer roots.
use wasm_lsig::wasm_lsig_run;

fn run(code: &[u8], fuel: u64, is_app: u32) -> (i32, u64, String) {
    let mut used = 0;
    let mut err = [0u8; 256];
    let result = unsafe {
        wasm_lsig_run(
            code.as_ptr(),
            code.len(),
            std::ptr::null(),
            0,
            fuel,
            &mut used,
            std::ptr::null_mut(),
            err.as_mut_ptr(),
            err.len(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            0,
            is_app,
        )
    };
    let end = err.iter().position(|b| *b == 0).unwrap_or(err.len());
    (
        result,
        used,
        String::from_utf8_lossy(&err[..end]).into_owned(),
    )
}

#[test]
fn sdk_math_works_in_both_modes_with_exact_fuel_boundary() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../data/transactions/logic/testdata/wasm/sdk_math.wasm"
    );
    let code = std::fs::read(path).expect("run make wasm-guest-fixtures");
    for is_app in [0, 1] {
        let (result, used, err) = run(&code, 1_000_000, is_app);
        assert_eq!(result, 1, "{err}");
        assert_eq!(run(&code, used, is_app).0, 1);
        assert_eq!(run(&code, used - 1, is_app).0, -1);
    }
}
