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

//! LogicSig arguments.
//!
//! These are signature-only, like the AVM's `arg` opcodes: they trap when the
//! module runs as an application program. Applications read their arguments
//! with [`crate::txn::app_arg_into`].

use avm_sys::ArgError;

use crate::host;
#[cfg(target_arch = "wasm32")]
use crate::Avm;

/// The number of LogicSig arguments.
#[inline]
pub fn count() -> u32 {
    host().arg_count()
}

/// Copies LogicSig argument `index` into `buf`, returning the bytes written.
#[inline]
pub fn get(index: u32, buf: &mut [u8]) -> Result<&[u8], ArgError> {
    host().arg_into(index, buf)
}
