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

//! Allocation-free ARC-4 encoding and return logs.

use super::{Address, Byte, UFixed, Uint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeError;

/// Encodes one ARC-4 value into a bounded buffer. On error the buffer may have
/// been partially written; callers must not log it. Reference and transaction
/// parameter types deliberately do not implement this trait.
pub trait Encode {
    const STATIC_LEN: Option<usize>;
    const IS_BOOL: bool = false;
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError>;
    #[doc(hidden)]
    fn bool_value(&self) -> bool {
        false
    }
}

pub(crate) fn copy(bytes: &[u8], out: &mut [u8]) -> Result<usize, EncodeError> {
    out.get_mut(..bytes.len())
        .ok_or(EncodeError)?
        .copy_from_slice(bytes);
    Ok(bytes.len())
}

macro_rules! uint {
    ($($ty:ty),*) => {$(impl Encode for $ty {
        const STATIC_LEN: Option<usize> = Some(core::mem::size_of::<Self>());
        fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> { copy(&self.to_be_bytes(), out) }
    })*};
}
uint!(u8, u16, u32, u64, u128);
impl Encode for bool {
    const STATIC_LEN: Option<usize> = Some(1);
    const IS_BOOL: bool = true;
    fn bool_value(&self) -> bool {
        *self
    }
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        copy(&[if *self { 0x80 } else { 0 }], out)
    }
}
impl Encode for Byte {
    const STATIC_LEN: Option<usize> = Some(1);
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        self.0.encode(out)
    }
}
impl Encode for Address {
    const STATIC_LEN: Option<usize> = Some(32);
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        copy(&self.0, out)
    }
}
impl<const N: usize> Encode for Uint<N> {
    const STATIC_LEN: Option<usize> = Some(N);
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        copy(&self.0, out)
    }
}
impl<const N: usize, const P: u8> Encode for UFixed<N, P> {
    const STATIC_LEN: Option<usize> = Some(N);
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        self.0.encode(out)
    }
}
fn dynamic(bytes: &[u8], out: &mut [u8]) -> Result<usize, EncodeError> {
    let len = u16::try_from(bytes.len()).map_err(|_| EncodeError)?;
    copy(&len.to_be_bytes(), out)?;
    copy(bytes, out.get_mut(2..).ok_or(EncodeError)?)?;
    Ok(bytes.len() + 2)
}
impl Encode for &[u8] {
    const STATIC_LEN: Option<usize> = None;
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        dynamic(self, out)
    }
}
impl Encode for &str {
    const STATIC_LEN: Option<usize> = None;
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        dynamic(self.as_bytes(), out)
    }
}

// An object-safe adapter lets arrays and all tuple arities share one encoder.
pub(crate) trait Value {
    fn layout(&self) -> (Option<usize>, bool);
    fn bit(&self) -> bool;
    fn write(&self, out: &mut [u8]) -> Result<usize, EncodeError>;
}
impl<T: Encode> Value for T {
    fn layout(&self) -> (Option<usize>, bool) {
        (T::STATIC_LEN, T::IS_BOOL)
    }
    fn bit(&self) -> bool {
        self.bool_value()
    }
    fn write(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        self.encode(out)
    }
}
pub(crate) const fn static_size(layout: &[(Option<usize>, bool)]) -> Option<usize> {
    let mut size = 0usize;
    let mut bits = 0;
    let mut i = 0;
    while i < layout.len() {
        let (len, boolean) = layout[i];
        if boolean {
            if bits % 8 == 0 {
                size += 1;
            }
            bits += 1;
        } else {
            bits = 0;
            match len {
                Some(n) => {
                    size = match size.checked_add(n) {
                        Some(n) => n,
                        None => return None,
                    }
                }
                None => return None,
            }
        }
        i += 1;
    }
    Some(size)
}
pub(crate) fn sequence(values: &[&dyn Value], out: &mut [u8]) -> Result<usize, EncodeError> {
    let mut head = 0usize;
    let mut bits = 0;
    for value in values {
        let (size, boolean) = value.layout();
        let add = if boolean {
            let add = usize::from(bits % 8 == 0);
            bits += 1;
            add
        } else {
            bits = 0;
            size.unwrap_or(2)
        };
        head = head.checked_add(add).ok_or(EncodeError)?;
    }
    if head > out.len() {
        return Err(EncodeError);
    }
    out[..head].fill(0);
    let mut pos = 0;
    let mut tail = head;
    bits = 0;
    for value in values {
        let (size, boolean) = value.layout();
        if boolean {
            if bits > 0 && bits % 8 == 0 {
                pos += 1;
            }
            if value.bit() {
                out[pos] |= 0x80 >> (bits % 8);
            }
            bits += 1;
        } else {
            if bits > 0 {
                pos += 1;
                bits = 0;
            }
            if let Some(size) = size {
                let end = pos.checked_add(size).ok_or(EncodeError)?;
                let n = value.write(out.get_mut(pos..end).ok_or(EncodeError)?)?;
                if n != size {
                    return Err(EncodeError);
                }
                pos = end;
            } else {
                let offset = u16::try_from(tail).map_err(|_| EncodeError)?;
                out[pos..pos + 2].copy_from_slice(&offset.to_be_bytes());
                pos += 2;
                let n = value.write(out.get_mut(tail..).ok_or(EncodeError)?)?;
                tail = tail.checked_add(n).ok_or(EncodeError)?;
                if tail > out.len() {
                    return Err(EncodeError);
                }
            }
        }
    }
    Ok(tail)
}
impl<T: Encode, const N: usize> Encode for [T; N] {
    // ARC-4 arrays of dynamic elements remain dynamic even at length zero.
    const STATIC_LEN: Option<usize> = match T::STATIC_LEN {
        None => None,
        Some(_) => static_size(&[(T::STATIC_LEN, T::IS_BOOL); N]),
    };
    fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        let values: [&dyn Value; N] = core::array::from_fn(|i| &self[i] as &dyn Value);
        sequence(&values, out)
    }
}
impl Encode for () {
    const STATIC_LEN: Option<usize> = Some(0);
    fn encode(&self, _out: &mut [u8]) -> Result<usize, EncodeError> {
        Ok(0)
    }
}

/// ARC-4's return-log prefix (SHA-512/256("return") first four bytes).
pub const RETURN_PREFIX: [u8; 4] = [0x15, 0x1f, 0x7c, 0x75];

/// Logs an ARC-4 return without allocating. A value that cannot fit in the
/// protocol's 1024-byte log allowance rejects without logging. Host log-limit
/// failures still trap (including prior logs consuming the allowance).
pub fn log_return<T: Encode>(value: &T) -> bool {
    let mut bytes = [0u8; 1024];
    bytes[..4].copy_from_slice(&RETURN_PREFIX);
    match value.encode(&mut bytes[4..]) {
        Ok(n) if n <= bytes.len() - 4 => {
            crate::log(&bytes[..n + 4]);
            true
        }
        _ => false,
    }
}
