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

//! The host abstraction.

use avm_sys::{self, ArgError, TxnError, TxnField};

/// Every host operation the current ABI exposes.
///
/// Program code does not take a host; the SDK helpers resolve it through
/// [`host`]. The trait exists so native tests can install a mock with
/// [`crate::testing::set_host`].
///
/// The methods mirror the raw bindings one-for-one. Cases the host traps (an
/// invalid transaction field, an oversized value, a type mismatch) are not
/// `Result`; a trap ends the program, as in the AVM.
pub trait Avm {
    /// The number of LogicSig arguments. Signature-only: traps in an
    /// application program.
    fn arg_count(&self) -> u32;

    /// Copies LogicSig argument `index` into `buf`. Signature-only.
    fn arg_into<'a>(&self, index: u32, buf: &'a mut [u8]) -> Result<&'a [u8], ArgError>;

    /// Reads a uint64 transaction field.
    fn txn_uint(&self, field: TxnField) -> u64;

    /// Copies a byte-string transaction field into `buf`, returning its length.
    fn txn_bytes(&self, field: TxnField, buf: &mut [u8]) -> Result<usize, TxnError>;

    /// Reads a uint64 array transaction field element.
    fn txna_uint(&self, field: TxnField, index: u32) -> u64;

    /// Copies a byte-string array transaction field element into `buf`.
    fn txna_bytes(&self, field: TxnField, index: u32, buf: &mut [u8]) -> Result<usize, TxnError>;

    /// Reads a uint64 global-state value. A missing key reads as 0.
    fn global_get_uint(&self, key: &[u8]) -> u64;

    /// Copies a byte-string global-state value into `buf`. A missing key reads
    /// as empty.
    fn global_get_bytes<'a>(&self, key: &[u8], buf: &'a mut [u8]) -> Result<&'a [u8], TxnError>;

    /// Writes a uint64 global-state value.
    fn global_put_uint(&self, key: &[u8], value: u64);

    /// Writes a byte-string global-state value.
    fn global_put_bytes(&self, key: &[u8], value: &[u8]);

    /// Deletes a global-state key. Deleting an absent key is not an error.
    fn global_del(&self, key: &[u8]);

    /// Verifies an ed25519 signature with the AVM's `ed25519verify_bare`
    /// semantics.
    fn ed25519_verify_bare(&self, data: &[u8], sig: &[u8; 64], pk: &[u8; 32]) -> bool;
}

/// The active host type.
///
/// On wasm this is the concrete [`HostAvm`], so the compiler can inline the host
/// wrappers. Native tests install a mock with [`crate::testing::set_host`], and
/// `Host` is then `dyn Avm`.
#[cfg(target_arch = "wasm32")]
pub type Host = HostAvm;

#[cfg(not(target_arch = "wasm32"))]
pub type Host = dyn Avm;

/// Returns the active host.
///
/// On wasm this is a constant reference to the [`HostAvm`]. On native builds it
/// reads the thread-local test host installed by [`crate::testing::set_host`].
#[cfg(target_arch = "wasm32")]
pub fn host() -> &'static Host {
    &HOST_AVM
}

#[cfg(not(target_arch = "wasm32"))]
pub fn host() -> &'static Host {
    crate::testing::current_host()
}

/// The production host backed by the real `env` imports.
///
/// On wasm this is the only [`Host`]. Native tests swap in a mock via
/// [`crate::testing::set_host`].
pub struct HostAvm;

#[cfg(target_arch = "wasm32")]
static HOST_AVM: HostAvm = HostAvm;

impl Avm for HostAvm {
    #[inline]
    fn arg_count(&self) -> u32 {
        avm_sys::arg_count()
    }

    #[inline]
    fn arg_into<'a>(&self, index: u32, buf: &'a mut [u8]) -> Result<&'a [u8], ArgError> {
        avm_sys::arg_into(index, buf)
    }

    #[inline]
    fn txn_uint(&self, field: TxnField) -> u64 {
        avm_sys::txn_uint(field) as u64
    }

    #[inline]
    fn txn_bytes(&self, field: TxnField, buf: &mut [u8]) -> Result<usize, TxnError> {
        avm_sys::txn_bytes(field, buf)
    }

    #[inline]
    fn txna_uint(&self, field: TxnField, index: u32) -> u64 {
        avm_sys::txna_uint(field, index) as u64
    }

    #[inline]
    fn txna_bytes(&self, field: TxnField, index: u32, buf: &mut [u8]) -> Result<usize, TxnError> {
        avm_sys::txna_bytes(field, index, buf)
    }

    #[inline]
    fn global_get_uint(&self, key: &[u8]) -> u64 {
        avm_sys::app_global_get_uint(key)
    }

    #[inline]
    fn global_get_bytes<'a>(&self, key: &[u8], buf: &'a mut [u8]) -> Result<&'a [u8], TxnError> {
        avm_sys::app_global_get_bytes(key, buf)
    }

    #[inline]
    fn global_put_uint(&self, key: &[u8], value: u64) {
        avm_sys::app_global_put_uint(key, value);
    }

    #[inline]
    fn global_put_bytes(&self, key: &[u8], value: &[u8]) {
        avm_sys::app_global_put_bytes(key, value);
    }

    #[inline]
    fn global_del(&self, key: &[u8]) {
        avm_sys::app_global_del(key);
    }

    #[inline]
    fn ed25519_verify_bare(&self, data: &[u8], sig: &[u8; 64], pk: &[u8; 32]) -> bool {
        avm_sys::ed25519_verify_bare(data, sig, pk)
    }
}
