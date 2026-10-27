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

use avm_sys::{self, ArgError, GlobalField, TxnError, TxnField};

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
    /// Full unsigned product as (high, low), matching AVM mulw.
    fn mulw(&self, _a: u64, _b: u64) -> (u64, u64) {
        unimplemented!("mulw")
    }
    /// 128-bit unsigned division as (quotient high/low, remainder high/low).
    /// A zero denominator traps, matching AVM divmodw.
    fn divmodw(
        &self,
        _a_high: u64,
        _a_low: u64,
        _b_high: u64,
        _b_low: u64,
    ) -> (u64, u64, u64, u64) {
        unimplemented!("divmodw")
    }
    /// Integer floor square root, matching AVM sqrt.
    fn sqrt(&self, _n: u64) -> u64 {
        unimplemented!("sqrt")
    }
    /// Integer floor square root of a 128-bit high/low pair (WASM extension).
    fn sqrtw(&self, _high: u64, _low: u64) -> u64 {
        unimplemented!("sqrtw")
    }
    /// Appends an application log, subject to AVM count and total-byte limits.
    fn log(&self, _bytes: &[u8]) {
        unimplemented!("log")
    }
    /// Begins an application-only inner asset group.
    fn itxn_begin(&self) {
        unimplemented!("itxn_begin")
    }
    /// Starts the next transaction in the pending inner group.
    fn itxn_next(&self) {
        unimplemented!("itxn_next")
    }
    /// Sets a uint field of the current pending inner asset transaction.
    fn itxn_field_uint(&self, _field: TxnField, _value: u64) {
        unimplemented!("itxn_field_uint")
    }
    /// Sets a byte-string field of the current pending inner asset transaction.
    fn itxn_field_bytes(&self, _field: TxnField, _bytes: &[u8]) {
        unimplemented!("itxn_field_bytes")
    }
    /// Submits the pending asset group. Host failures trap.
    fn itxn_submit(&self) {
        unimplemented!("itxn_submit")
    }
    /// CreatedAssetID of the last submitted transaction.
    fn itxn_created_asset_id(&self) -> u64 {
        unimplemented!("itxn_created_asset_id")
    }

    /// Reads a holding's balance; absence differs from an existing zero balance.
    /// Unavailable resources trap. Application-only.
    fn asset_balance(&self, _account: &[u8; 32], _asset: u64) -> Option<u64> {
        unimplemented!("asset_balance")
    }

    /// Copies an asset's unit name, or reports absence. An empty buffer queries
    /// length without copying. Application-only.
    fn asset_unit_name(&self, _asset: u64, _buf: &mut [u8]) -> Result<Option<usize>, TxnError> {
        unimplemented!("asset_unit_name")
    }

    /// Reads a scalar uint field from a group transaction. Mocks that do not
    /// model group reads fail explicitly rather than silently returning zero.
    fn gtxn_uint(&self, _group_index: u32, _field: TxnField) -> u64 {
        unimplemented!("gtxn_uint")
    }

    /// Copies a scalar byte field from a group transaction.
    fn gtxn_bytes(
        &self,
        _group_index: u32,
        _field: TxnField,
        _buf: &mut [u8],
    ) -> Result<usize, TxnError> {
        unimplemented!("gtxn_bytes")
    }

    /// Reads a uint context field (not an application state key).
    fn global_uint(&self, _field: GlobalField) -> u64 {
        unimplemented!("global_uint")
    }

    /// Copies a byte-string context field.
    fn global_bytes(&self, _field: GlobalField, _buf: &mut [u8]) -> Result<usize, TxnError> {
        unimplemented!("global_bytes")
    }

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
    fn mulw(&self, a: u64, b: u64) -> (u64, u64) {
        avm_sys::mulw(a, b)
    }
    #[inline]
    fn divmodw(&self, a_high: u64, a_low: u64, b_high: u64, b_low: u64) -> (u64, u64, u64, u64) {
        avm_sys::divmodw(a_high, a_low, b_high, b_low)
    }
    #[inline]
    fn sqrt(&self, n: u64) -> u64 {
        avm_sys::sqrt(n)
    }
    #[inline]
    fn sqrtw(&self, high: u64, low: u64) -> u64 {
        avm_sys::sqrtw(high, low)
    }
    #[inline]
    fn log(&self, bytes: &[u8]) {
        avm_sys::log(bytes)
    }
    #[inline]
    fn itxn_begin(&self) {
        avm_sys::itxn_begin()
    }
    #[inline]
    fn itxn_next(&self) {
        avm_sys::itxn_next()
    }
    #[inline]
    fn itxn_field_uint(&self, field: TxnField, value: u64) {
        avm_sys::itxn_field_uint(field, value)
    }
    #[inline]
    fn itxn_field_bytes(&self, field: TxnField, bytes: &[u8]) {
        avm_sys::itxn_field_bytes(field, bytes)
    }
    #[inline]
    fn itxn_submit(&self) {
        avm_sys::itxn_submit()
    }
    #[inline]
    fn itxn_created_asset_id(&self) -> u64 {
        avm_sys::itxn_created_asset_id()
    }

    #[inline]
    fn asset_balance(&self, account: &[u8; 32], asset: u64) -> Option<u64> {
        avm_sys::asset_balance(account, asset)
    }

    #[inline]
    fn asset_unit_name(&self, asset: u64, buf: &mut [u8]) -> Result<Option<usize>, TxnError> {
        avm_sys::asset_unit_name(asset, buf)
    }

    #[inline]
    fn gtxn_uint(&self, group_index: u32, field: TxnField) -> u64 {
        avm_sys::gtxn_uint(group_index, field)
    }

    #[inline]
    fn gtxn_bytes(
        &self,
        group_index: u32,
        field: TxnField,
        buf: &mut [u8],
    ) -> Result<usize, TxnError> {
        avm_sys::gtxn_bytes(group_index, field, buf)
    }

    #[inline]
    fn global_uint(&self, field: GlobalField) -> u64 {
        avm_sys::global_uint(field)
    }

    #[inline]
    fn global_bytes(&self, field: GlobalField, buf: &mut [u8]) -> Result<usize, TxnError> {
        avm_sys::global_bytes(field, buf)
    }

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
