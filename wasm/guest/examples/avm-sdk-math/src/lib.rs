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

//! End-to-end fixture for host-backed wide arithmetic and square roots.
#![cfg_attr(target_arch = "wasm32", no_std)]

use avm_sdk::{math, program_entry};
avm_sdk::panic_handler!();

#[program_entry]
fn approve() -> bool {
    let (high, low) = math::mulw(u64::MAX, u64::MAX);
    (high, low) == (u64::MAX - 1, 1)
        && math::divmodw(high, low, 0, u64::MAX) == (0, u64::MAX, 0, 0)
        && math::divmodw(u64::MAX, u64::MAX, 1, 1) == (0, u64::MAX, 0, 0)
        && math::divmodw(u64::MAX, u64::MAX, 0, 1) == (u64::MAX, u64::MAX, 0, 0)
        && math::divmodw(0, 7, 0, 3) == (0, 2, 0, 1)
        && math::sqrt(u64::MAX) == u64::from(u32::MAX)
        && math::sqrt(8) == 2
        && math::sqrtw(high, low) == u64::MAX
        && math::sqrtw(0, 8) == 2
}
