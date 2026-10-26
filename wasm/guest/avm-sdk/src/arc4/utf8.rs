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

//! A small UTF-8 validator.
//!
//! `core::str::from_utf8` is tuned for throughput and adds about 700 bytes to a
//! module, which every call pays for in setup fuel. Arguments are at most
//! 2 KiB, so a byte-at-a-time check is the better trade.

/// Whether `bytes` is well-formed UTF-8 (Unicode Table 3-7): no overlong
/// encodings, no surrogates, nothing above U+10FFFF.
///
/// It decodes each scalar and checks its range, which is smaller than
/// matching lead bytes against second-byte ranges.
pub(crate) fn is_valid(bytes: &[u8]) -> bool {
    let mut i = 0;
    while let Some(&lead) = bytes.get(i) {
        i += 1;
        if lead < 0x80 {
            continue;
        }
        // Continuation bytes, and the smallest scalar the width may encode.
        let (width, min) = match lead {
            0xC0..=0xDF => (1, 0x80),
            0xE0..=0xEF => (2, 0x800),
            0xF0..=0xF7 => (3, 0x1_0000),
            _ => return false,
        };
        let mut scalar = u32::from(lead) & (0x3F >> width);
        for _ in 0..width {
            match bytes.get(i) {
                Some(&byte) if byte & 0xC0 == 0x80 => {
                    scalar = scalar << 6 | u32::from(byte & 0x3F);
                }
                _ => return false,
            }
            i += 1;
        }
        if scalar < min || (0xD800..=0xDFFF).contains(&scalar) || scalar > 0x10_FFFF {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::is_valid;

    fn check(bytes: &[u8]) {
        assert_eq!(
            is_valid(bytes),
            core::str::from_utf8(bytes).is_ok(),
            "{bytes:02x?}"
        );
    }

    #[test]
    fn matches_core_on_all_short_sequences() {
        check(&[]);
        for a in 0..=255u8 {
            check(&[a]);
            for b in 0..=255u8 {
                check(&[a, b]);
                for c in 0..=255u8 {
                    check(&[a, b, c]);
                }
            }
        }
    }

    #[test]
    fn matches_core_on_four_byte_sequences() {
        // Every lead byte from 0xE0 up, every second byte, and continuation
        // bytes around the valid range's edges.
        let edges = [0x00, 0x7F, 0x80, 0x81, 0xBE, 0xBF, 0xC0, 0xFF];
        for a in 0xE0..=0xFFu8 {
            for b in 0..=255u8 {
                for &c in &edges {
                    for &d in &edges {
                        check(&[a, b, c, d]);
                    }
                }
            }
        }
    }

    #[test]
    fn mixed_strings() {
        for s in ["", "ascii", "héllo", "日本語", "🦀 crab", "a\u{10FFFF}z"] {
            assert!(is_valid(s.as_bytes()));
        }
        check(b"ok\xC3");
        check(b"\xF0\x9F\xA6");
        check(b"abc\xED\xA0\x80");
    }
}
