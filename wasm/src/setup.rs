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

//! Deterministic pricing of work performed before guest execution. This is a
//! bounded binary preflight, NOT a replacement for wasmi's full validation.
use wasmparser::{ElementItems, Encoding, Parser, Payload, TypeRef};

// Covers today's largest app program (2048 * 8) and 16,000-byte LogicSigs.
// Go still enforces the narrower protocol/mode-specific limits.
pub(crate) const MAX_MODULE_BYTES: usize = 16 * 1024;

// Fixed consensus weights, calibrated conservatively from the size sweeps in
// WASM_PERF.md at approximately 1 ns/fuel. Never depend on clocks or caches.
const BASE: u64 = 5_000;
const BYTE: u64 = 6;
const FUNCTION: u64 = 180; // imports as well as definitions
const TYPE: u64 = 300;
const SIGNATURE_VALUE: u64 = 4; // params/results revalidated per function/import
const GLOBAL: u64 = 150;
const EXPORT: u64 = 400;
const SEGMENT: u64 = 100; // data and element segments, including passive ones
const MEMORY_PAGE: u64 = 350;
const TABLE_ENTRY: u64 = 4;
const ELEMENT_ITEM: u64 = 10; // includes passive/declared/overlapping segments

fn add(fuel: &mut u64, count: u64, weight: u64, budget: u64) -> Result<(), String> {
    *fuel = count
        .checked_mul(weight)
        .and_then(|cost| fuel.checked_add(cost))
        .ok_or("WASM setup fuel overflow")?;
    if *fuel > budget {
        return Err("wasm program ran out of fuel during setup".into());
    }
    Ok(())
}

// Counts advertised by untrusted sections must not cause unbounded allocation
// in a subsequent compiler. Every entry requires at least one encoded byte.
fn count(value: u32, bytes: usize) -> Result<u64, String> {
    if value as usize > bytes {
        return Err("invalid WASM setup section count".into());
    }
    Ok(u64::from(value))
}

/// Rejects unaffordable modules before creating an engine or compiling them.
/// Code bodies are skipped (except bounded local declarations); full instruction
/// validation happens only after the complete setup charge has been reserved.
pub(crate) fn fuel(wasm: &[u8], memory_limit: usize, budget: u64) -> Result<u64, String> {
    if wasm.len() > MAX_MODULE_BYTES {
        return Err("WASM module exceeds setup size limit (16384 bytes)".into());
    }
    let mut fuel = BASE;
    add(&mut fuel, wasm.len() as u64, BYTE, budget)?;
    let mut memories = 0u64;
    let mut tables = 0u64;
    let mut defined = 0;
    let mut bodies = 0;
    let mut seen = 0u16;
    // Bounded by encoded module length, and reserved only after paying TYPE.
    let mut arities = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload.map_err(|err| format!("invalid WASM setup: {err}"))?;
        if let Some((id, _)) = payload.as_section() {
            if id != 0 {
                let bit = 1u16
                    .checked_shl(u32::from(id))
                    .ok_or("unsupported WASM section")?;
                if seen & bit != 0 {
                    return Err("duplicate WASM setup section".into());
                }
                seen |= bit;
            }
        }
        match payload {
            Payload::Version {
                encoding: Encoding::Module,
                num: 1,
                ..
            } => {}
            Payload::Version { .. } => return Err("WASM setup requires a core module".into()),
            Payload::TypeSection(reader) => {
                let n = count(reader.count(), wasm.len())?;
                add(&mut fuel, n, TYPE, budget)?;
                arities.reserve(n as usize);
                for ty in reader.into_iter_err_on_gc_types() {
                    let ty = ty.map_err(|err| format!("invalid WASM type: {err}"))?;
                    arities.push((ty.params().len() + ty.results().len()) as u64);
                }
            }
            Payload::ImportSection(reader) => {
                add(
                    &mut fuel,
                    count(reader.count(), wasm.len())?,
                    FUNCTION,
                    budget,
                )?;
                for import in reader {
                    let import = import.map_err(|err| format!("invalid WASM import: {err}"))?;
                    let TypeRef::Func(index) = import.ty else {
                        return Err("unsupported WASM non-function import".into());
                    };
                    let arity = *arities
                        .get(index as usize)
                        .ok_or("invalid WASM function type index")?;
                    add(&mut fuel, arity, SIGNATURE_VALUE, budget)?;
                }
            }
            Payload::FunctionSection(reader) => {
                defined += count(reader.count(), wasm.len())?;
                add(&mut fuel, u64::from(reader.count()), FUNCTION, budget)?;
                for index in reader {
                    let index = index.map_err(|err| format!("invalid WASM function: {err}"))?;
                    let arity = *arities
                        .get(index as usize)
                        .ok_or("invalid WASM function type index")?;
                    add(&mut fuel, arity, SIGNATURE_VALUE, budget)?;
                }
            }
            Payload::GlobalSection(reader) => add(
                &mut fuel,
                count(reader.count(), wasm.len())?,
                GLOBAL,
                budget,
            )?,
            Payload::ExportSection(reader) => add(
                &mut fuel,
                count(reader.count(), wasm.len())?,
                EXPORT,
                budget,
            )?,
            Payload::MemorySection(reader) => {
                memories += u64::from(reader.count());
                if memories > 1 {
                    return Err("WASM setup memory count limit exceeded".into());
                }
                for memory in reader {
                    let memory = memory.map_err(|err| format!("invalid WASM memory: {err}"))?;
                    if memory.memory64 || memory.shared || memory.page_size_log2.is_some() {
                        return Err("unsupported WASM memory type".into());
                    }
                    if memory.initial > (memory_limit / 65536) as u64 {
                        return Err("WASM setup memory size limit exceeded".into());
                    }
                    add(&mut fuel, memory.initial, MEMORY_PAGE, budget)?;
                }
            }
            Payload::TableSection(reader) => {
                tables += u64::from(reader.count());
                if tables > 1 {
                    return Err("WASM setup table count limit exceeded".into());
                }
                for table in reader {
                    let table = table.map_err(|err| format!("invalid WASM table: {err}"))?;
                    if table.ty.initial > super::MAX_TABLE_ELEMENTS as u64 {
                        return Err("WASM setup table size limit exceeded".into());
                    }
                    add(&mut fuel, table.ty.initial, TABLE_ENTRY, budget)?;
                }
            }
            Payload::ElementSection(reader) => {
                add(
                    &mut fuel,
                    count(reader.count(), wasm.len())?,
                    SEGMENT,
                    budget,
                )?;
                for element in reader {
                    let element = element.map_err(|err| format!("invalid WASM element: {err}"))?;
                    let items = match element.items {
                        ElementItems::Functions(items) => items.count(),
                        ElementItems::Expressions(_, items) => items.count(),
                    };
                    add(&mut fuel, count(items, wasm.len())?, ELEMENT_ITEM, budget)?;
                }
            }
            Payload::DataSection(reader) => add(
                &mut fuel,
                count(reader.count(), wasm.len())?,
                SEGMENT,
                budget,
            )?,
            Payload::CodeSectionStart { count: n, .. } => {
                if u64::from(n) != defined {
                    return Err("invalid WASM function/code count".into());
                }
            }
            Payload::CodeSectionEntry(body) => {
                bodies += 1;
                let locals = body
                    .get_locals_reader()
                    .map_err(|err| format!("invalid WASM locals: {err}"))?;
                let mut total = 0u64;
                for local in locals {
                    let (n, _) = local.map_err(|err| format!("invalid WASM locals: {err}"))?;
                    total += u64::from(n);
                    // Match wasmparser/wasmi's expanded per-function local cap.
                    if total > 50_000 {
                        return Err("WASM setup local count limit exceeded".into());
                    }
                }
            }
            Payload::StartSection { .. } => return Err("WASM start functions are forbidden".into()),
            Payload::DataCountSection { count: n, .. } => {
                count(n, wasm.len())?;
            }
            Payload::CustomSection(_) | Payload::End(_) => {}
            _ => return Err("unsupported WASM setup section".into()),
        }
    }
    if bodies != defined {
        return Err("invalid WASM function/code count".into());
    }
    Ok(fuel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(source: &str) -> (Vec<u8>, u64) {
        let wasm = wat::parse_str(source).unwrap();
        let cost = fuel(&wasm, super::super::DEFAULT_MEMORY_LIMIT, u64::MAX).unwrap();
        (wasm, cost)
    }

    #[test]
    fn exact_formula_and_budget_boundary() {
        let (wasm, cost) = price("(module (func (export \"program\") (result i32) i32.const 1))");
        assert_eq!(
            cost,
            BASE + wasm.len() as u64 * BYTE + TYPE + FUNCTION + SIGNATURE_VALUE + EXPORT
        );
        assert_eq!(fuel(&wasm, 0, cost).unwrap(), cost);
        assert!(fuel(&wasm, 0, cost - 1).unwrap_err().contains("fuel"));
        assert!(fuel(&wasm, 0, 0).unwrap_err().contains("fuel"));
        // The price is independent of the available budget once it fits.
        assert_eq!(fuel(&wasm, 0, u64::MAX).unwrap(), cost);
    }

    #[test]
    fn initial_resources_and_all_element_segments_are_charged() {
        let (_, one) = price("(module (memory 1))");
        let (_, two) = price("(module (memory 2))");
        assert_eq!(two - one, MEMORY_PAGE);
        let (_, one) = price("(module (table 64 funcref))");
        let (_, two) = price("(module (table 65 funcref))");
        assert_eq!(two - one, TABLE_ENTRY);
        let (wasm, cost) = price("(module (func $f) (table 1 funcref) (elem (i32.const 0) $f) (elem (i32.const 0) $f) (elem func $f))");
        assert_eq!(
            cost,
            BASE + wasm.len() as u64 * BYTE
                + TYPE
                + FUNCTION
                + TABLE_ENTRY
                + 3 * (SEGMENT + ELEMENT_ITEM)
        );
    }

    #[test]
    fn repeated_signatures_are_charged_per_function() {
        let (wasm, cost) =
            price("(module (type $t (func (param i32 i64))) (func (type $t)) (func (type $t)))");
        assert_eq!(
            cost,
            BASE + wasm.len() as u64 * BYTE + TYPE + 2 * FUNCTION + 4 * SIGNATURE_VALUE
        );
    }

    #[test]
    fn custom_bytes_are_not_free() {
        let (mut wasm, before) = price("(module)");
        // A six-byte custom section, including an empty section name.
        wasm.extend_from_slice(&[0, 4, 0, 1, 2, 3]);
        assert_eq!(fuel(&wasm, 0, u64::MAX).unwrap(), before + 6 * BYTE);
    }

    #[test]
    fn malformed_and_oversized_inputs_are_bounded() {
        let mut huge = vec![0; MAX_MODULE_BYTES + 1];
        assert!(fuel(&huge, 0, u64::MAX).unwrap_err().contains("size limit"));
        huge.clear();
        huge.extend_from_slice(b"\0asm\x01\0\0\0\x01\x05\xff\xff\xff\xff\x0f");
        assert!(fuel(&huge, 0, u64::MAX)
            .unwrap_err()
            .contains("section count"));
        for bytes in [&b"\0asm\x01\0\0\0\x01\x80"[..], &b"not wasm"[..]] {
            assert!(fuel(bytes, 0, u64::MAX).is_err());
        }
        for source in [
            "(module (memory 65))",
            "(module (table 1025 funcref))",
            "(module (memory 1) (memory 1))",
        ] {
            let wasm = wat::parse_str(source).unwrap();
            assert!(fuel(&wasm, 4 * 1024 * 1024, u64::MAX)
                .unwrap_err()
                .contains("limit"));
        }
        let wasm = wat::parse_str("(module (import \"env\" \"memory\" (memory 1)))").unwrap();
        assert!(fuel(&wasm, 65536, u64::MAX).unwrap_err().contains("import"));
        // Compressed local declarations must not expand without a hard bound.
        let source = format!("(module (func (local {})))", "i32 ".repeat(50_001));
        let wasm = wat::parse_str(source).unwrap();
        assert!(fuel(&wasm, 0, u64::MAX)
            .unwrap_err()
            .contains("local count"));
    }

    #[test]
    fn arithmetic_overflow_is_rejected() {
        let mut fuel = u64::MAX;
        assert!(add(&mut fuel, 1, 1, u64::MAX)
            .unwrap_err()
            .contains("overflow"));
        assert!(add(&mut 0, u64::MAX, 2, u64::MAX)
            .unwrap_err()
            .contains("overflow"));
    }

    #[test]
    fn malformed_section_headers_and_random_bytes_do_not_panic() {
        let header = b"\0asm\x01\0\0\0";
        for tail in [
            &b"\x01\x01\0\x01\x01\0"[..],             // duplicate type sections
            &b"\x0c\x05\xff\xff\xff\xff\x0f"[..],     // enormous data count
            &b"\x03\x02\x01\0"[..],                   // missing type/code
            &b"\x01\x06\xff\xff\xff\xff\xff\x0f"[..], // overflowing LEB
        ] {
            let mut wasm = header.to_vec();
            wasm.extend_from_slice(tail);
            assert!(fuel(&wasm, 0, u64::MAX).is_err());
        }
        let mut seed = 0x1234abcd_u32;
        for len in 0..256 {
            let mut wasm = header.to_vec();
            for _ in 0..len {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                wasm.push((seed >> 24) as u8);
            }
            let _ = fuel(&wasm, 65536, u64::MAX);
        }
    }
}
