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

//! Integer-only arithmetic imports. All i64 values carry unsigned bit patterns.
//! Wide operands/results use high-word-first order, each word little-endian in
//! linear memory. These operations are available in both evaluation modes.

use crate::{charge_fuel, HostState};
use wasmi::Caller;

// Fixed host weights in fuel, calibrated on linux/arm64 toward 1 ns/fuel.
// See TestWasmMathHostCallCost and WASM_PERF.md for operand sweeps, guest/loop
// subtraction, and safety headroom. Charge failures before arithmetic/memory
// work too. Cross-machine and adversarial validation are still required.
const MULW_FUEL_COST: u64 = 15;
const DIVMODW_FUEL_COST: u64 = 25;
const SQRT_FUEL_COST: u64 = 10;
const SQRTW_FUEL_COST: u64 = 25;

fn wide(high: i64, low: i64) -> u128 {
    (u128::from(high as u64) << 64) | u128::from(low as u64)
}

/// Validates the entire destination before writing any bytes, including for
/// unaligned pointers and addresses near the end of memory.
fn write_words(
    caller: &mut Caller<'_, HostState>,
    out: i32,
    words: &[u64],
) -> Result<(), wasmi::Error> {
    let start =
        usize::try_from(out).map_err(|_| wasmi::Error::new("negative math result pointer"))?;
    let end = start
        .checked_add(words.len() * 8)
        .ok_or_else(|| wasmi::Error::new("math result outside linear memory"))?;
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmi::Error::new("missing memory export"))?;
    let bytes = memory
        .data_mut(caller)
        .get_mut(start..end)
        .ok_or_else(|| wasmi::Error::new("math result outside linear memory"))?;
    for (dst, word) in bytes.chunks_exact_mut(8).zip(words) {
        dst.copy_from_slice(&word.to_le_bytes());
    }
    Ok(())
}

/// Writes [product high, product low] (16 bytes).
pub(super) fn mulw(
    mut caller: Caller<'_, HostState>,
    a: i64,
    b: i64,
    out: i32,
) -> Result<(), wasmi::Error> {
    charge_fuel(&mut caller, MULW_FUEL_COST)?;
    let product = u128::from(a as u64) * u128::from(b as u64);
    write_words(&mut caller, out, &[(product >> 64) as u64, product as u64])
}

/// Writes [quotient high, quotient low, remainder high, remainder low]
/// (32 bytes). A zero divisor traps, matching AVM divmodw.
pub(super) fn divmodw(
    mut caller: Caller<'_, HostState>,
    a_high: i64,
    a_low: i64,
    b_high: i64,
    b_low: i64,
    out: i32,
) -> Result<(), wasmi::Error> {
    charge_fuel(&mut caller, DIVMODW_FUEL_COST)?;
    let a = wide(a_high, a_low);
    let b = wide(b_high, b_low);
    if b == 0 {
        return Err(wasmi::Error::new("divmodw division by zero"));
    }
    let (quotient, remainder) = (a / b, a % b);
    write_words(
        &mut caller,
        out,
        &[
            (quotient >> 64) as u64,
            quotient as u64,
            (remainder >> 64) as u64,
            remainder as u64,
        ],
    )
}

/// floor(sqrt(n)), matching AVM sqrt without floating-point operations.
pub(super) fn sqrt(mut caller: Caller<'_, HostState>, n: i64) -> Result<i64, wasmi::Error> {
    charge_fuel(&mut caller, SQRT_FUEL_COST)?;
    Ok((n as u64).isqrt() as i64)
}

/// Extension of sqrt to a 128-bit operand; the result always fits in u64.
pub(super) fn sqrtw(
    mut caller: Caller<'_, HostState>,
    high: i64,
    low: i64,
) -> Result<i64, wasmi::Error> {
    charge_fuel(&mut caller, SQRTW_FUEL_COST)?;
    Ok(wide(high, low).isqrt() as u64 as i64)
}

#[cfg(test)]
mod tests;
