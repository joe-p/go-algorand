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

//! AVM context fields. Application global state lives in [`crate::global`].

#[cfg(target_arch = "wasm32")]
use crate::Avm;
use crate::{address::Address, application::Application, host, Error, GlobalField, TxnError};

/// Reads a uint64 context field. Wrong types and unavailable modes trap.
#[inline]
pub fn uint(field: GlobalField) -> u64 {
    host().global_uint(field)
}

/// Copies a byte-string context field. An empty buffer queries its length.
#[inline]
pub fn bytes(field: GlobalField, buf: &mut [u8]) -> Result<usize, TxnError> {
    host().global_bytes(field, buf)
}

/// Number of transactions in the current group (not just application calls).
#[inline]
pub fn group_size() -> u64 {
    uint(GlobalField::GroupSize)
}

/// Current application's assigned ID, including during creation.
#[inline]
pub fn application_id() -> Application {
    Application::from(uint(GlobalField::CurrentApplicationID))
}

/// Current application's address. Application-only.
#[inline]
pub fn application_address() -> Result<Address, Error> {
    crate::bytes::read_address(|buf| bytes(GlobalField::CurrentApplicationAddress, buf))
}
