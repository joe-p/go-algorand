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

//! Unopinionated Rust declarations for the AVM WASM `env` host ABI.
//!
//! This crate is a 1:1 binding layer: each function corresponds to one import,
//! with typed error sentinels and the raw `TxnField` discriminants. It contains
//! no policy, no allocator, and no approval or entry-point abstractions. Most
//! programs should use `avm-sdk`, which wraps these functions behind a mockable
//! host and adds typed state, transaction helpers, and the `program_entry`
//! macro.
//!
//! The guest ABI (see `wasm/README.md`) is:
//!
//! | import | Rust binding |
//! |---|---|
//! | `arg_count() -> i32` | [`arg_count`] |
//! | `arg(i, out, len) -> i32` | [`arg_into`] |
//! | `txn_uint(field) -> i64` | [`txn_uint`] |
//! | `txn_bytes(field, out, len) -> i32` | [`txn_bytes`] |
//! | `txna_uint(field, index) -> i64` | [`txna_uint`] |
//! | `txna_bytes(field, index, out, len) -> i32` | [`txna_bytes`] |
//! | `gtxn_uint(group_index, field) -> i64` | [`gtxn_uint`] |
//! | `gtxn_bytes(group_index, field, out, len) -> i32` | [`gtxn_bytes`] |
//! | `global_uint(field) -> i64` | [`global_uint`] |
//! | `global_bytes(field, out, len) -> i32` | [`global_bytes`] |
//! | `asset_balance(account, asset, out) -> i32` | [`asset_balance`] |
//! | `asset_unit_name(asset, out, len) -> i32` | [`asset_unit_name`] |
//! | `app_global_get_uint(key, key_len) -> i64` | [`app_global_get_uint`] |
//! | `app_global_get_bytes(key, key_len, out, len) -> i32` | [`app_global_get_bytes`] |
//! | `app_global_put_uint(key, key_len, value)` | [`app_global_put_uint`] |
//! | `app_global_put_bytes(key, key_len, val, val_len)` | [`app_global_put_bytes`] |
//! | `app_global_del(key, key_len)` | [`app_global_del`] |
//! | `ed25519verify_bare(d, dl, s, sl, p, pl) -> i32` | [`ed25519_verify_bare`] |
//!
//! `arg`/`arg_count` are signature-only, like the AVM's `arg` opcodes: they
//! trap when the module runs as an application program. An application reads its
//! arguments from the transaction instead, with [`txna_bytes`] and
//! [`TxnField::ApplicationArgs`]. The `app_global_*` functions are the reverse:
//! application-only, and they trap when the module runs as a LogicSig.
//!
//! Some host functions *trap* rather than return an error (an invalid or
//! out-of-version transaction field, an integer field read as bytes, or an
//! oversized value). A trap ends the guest, matching the AVM, so those cases
//! are not modelled as `Result`. The functions that do report errors return a
//! typed error enum.
//!
//! # Example
//!
//! ```ignore
//! #![no_std]
//! use avm_sys::{arg_into, ed25519_verify_bare};
//!
//! #[no_mangle]
//! pub extern "C" fn program() -> i32 {
//!     let mut data = [0u8; 4096];
//!     let mut sig = [0u8; 64];
//!     let mut pk = [0u8; 32];
//!     let data_len = match arg_into(0, &mut data) {
//!         Ok(d) => d.len(),
//!         Err(_) => return 0,
//!     };
//!     if arg_into(1, &mut sig).is_err() || arg_into(2, &mut pk).is_err() {
//!         return 0;
//!     }
//!     if ed25519_verify_bare(&data[..data_len], &sig, &pk) { 1 } else { 0 }
//! }
//! ```
//!
//! This crate is `no_std` and does not allocate.

#![no_std]

/// Maximum size of a LogicSig argument, and of any byte value crossing the host
/// boundary. Matches `transactions.MaxLogicSigArgSize` and the AVM's
/// `maxStringSize`.
pub const MAX_ARG_BYTES: usize = 4096;

// Error sentinels returned by `env.arg`.
const ARG_ERR_INDEX: i32 = -1;
const ARG_ERR_BUFFER: i32 = -2;
const ARG_ERR_MEMORY: i32 = -3;

// Error sentinels returned by `env.txn_bytes`. Invalid or unavailable fields
// trap the guest instead of returning a sentinel.
const TXN_ERR_BUFFER: i32 = -2;
const TXN_ERR_MEMORY: i32 = -3;

// Error sentinels returned by `env.app_global_get_bytes`. Invalid reads (a
// wrong-typed value) trap the guest instead of returning a sentinel.
const GLOBAL_ERR_BUFFER: i32 = -2;
const GLOBAL_ERR_MEMORY: i32 = -3;

#[cfg(target_arch = "wasm32")]
mod sys {
    //! Raw `env` imports. Only linked when compiling for `wasm32`.

    #[link(wasm_import_module = "env")]
    extern "C" {
        pub fn mulw(a: i64, b: i64, out: i32);
        pub fn divmodw(a_high: i64, a_low: i64, b_high: i64, b_low: i64, out: i32);
        pub fn sqrt(n: i64) -> i64;
        pub fn sqrtw(high: i64, low: i64) -> i64;
        pub fn arg_count() -> i32;
        pub fn arg(index: i32, out: i32, len: i32) -> i32;
        pub fn txn_uint(field: i32) -> i64;
        pub fn txn_bytes(field: i32, out: i32, len: i32) -> i32;
        pub fn txna_uint(field: i32, index: i32) -> i64;
        pub fn txna_bytes(field: i32, index: i32, out: i32, len: i32) -> i32;
        pub fn gtxn_uint(group_index: i32, field: i32) -> i64;
        pub fn gtxn_bytes(group_index: i32, field: i32, out: i32, len: i32) -> i32;
        pub fn global_uint(field: i32) -> i64;
        pub fn global_bytes(field: i32, out: i32, len: i32) -> i32;
        pub fn asset_balance(account: i32, asset: i64, out: i32) -> i32;
        pub fn asset_unit_name(asset: i64, out: i32, len: i32) -> i32;
        pub fn itxn_begin();
        pub fn log(ptr: i32, len: i32);
        pub fn itxn_next();
        pub fn itxn_field_uint(field: i32, value: i64);
        pub fn itxn_field_bytes(field: i32, ptr: i32, len: i32);
        pub fn itxn_submit();
        pub fn itxn_created_asset_id() -> i64;
        pub fn app_global_get_uint(key: i32, key_len: i32) -> i64;
        pub fn app_global_get_bytes(key: i32, key_len: i32, out: i32, len: i32) -> i32;
        pub fn app_global_put_uint(key: i32, key_len: i32, value: i64);
        pub fn app_global_put_bytes(key: i32, key_len: i32, val: i32, val_len: i32);
        pub fn app_global_del(key: i32, key_len: i32);
        pub fn ed25519verify_bare(
            data: i32,
            data_len: i32,
            sig: i32,
            sig_len: i32,
            pk: i32,
            pk_len: i32,
        ) -> i32;
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod sys {
    //! Native stubs so the crate builds and links for host targets (e.g. to
    //! type-check `cargo test`). The host functions only exist inside the wasm
    //! executor; calling one natively is a bug.

    macro_rules! unavailable {
        ($name:literal) => {
            panic!(concat!(
                "avm_sys::",
                $name,
                " is only available on wasm32 (inside a WASM LogicSig)"
            ))
        };
    }

    pub unsafe fn arg_count() -> i32 {
        unavailable!("arg_count")
    }
    pub unsafe fn mulw(_a: i64, _b: i64, _out: i32) {
        unavailable!("mulw")
    }
    pub unsafe fn divmodw(_a_high: i64, _a_low: i64, _b_high: i64, _b_low: i64, _out: i32) {
        unavailable!("divmodw")
    }
    pub unsafe fn sqrt(_n: i64) -> i64 {
        unavailable!("sqrt")
    }
    pub unsafe fn sqrtw(_high: i64, _low: i64) -> i64 {
        unavailable!("sqrtw")
    }
    pub unsafe fn arg(_index: i32, _out: i32, _len: i32) -> i32 {
        unavailable!("arg")
    }
    pub unsafe fn txn_uint(_field: i32) -> i64 {
        unavailable!("txn_uint")
    }
    pub unsafe fn txn_bytes(_field: i32, _out: i32, _len: i32) -> i32 {
        unavailable!("txn_bytes")
    }
    pub unsafe fn txna_uint(_field: i32, _index: i32) -> i64 {
        unavailable!("txna_uint")
    }
    pub unsafe fn txna_bytes(_field: i32, _index: i32, _out: i32, _len: i32) -> i32 {
        unavailable!("txna_bytes")
    }
    pub unsafe fn gtxn_uint(_group_index: i32, _field: i32) -> i64 {
        unavailable!("gtxn_uint")
    }
    pub unsafe fn gtxn_bytes(_group_index: i32, _field: i32, _out: i32, _len: i32) -> i32 {
        unavailable!("gtxn_bytes")
    }
    pub unsafe fn global_uint(_field: i32) -> i64 {
        unavailable!("global_uint")
    }
    pub unsafe fn global_bytes(_field: i32, _out: i32, _len: i32) -> i32 {
        unavailable!("global_bytes")
    }
    pub unsafe fn asset_balance(_account: i32, _asset: i64, _out: i32) -> i32 {
        unavailable!("asset_balance")
    }
    pub unsafe fn asset_unit_name(_asset: i64, _out: i32, _len: i32) -> i32 {
        unavailable!("asset_unit_name")
    }
    pub unsafe fn log(_ptr: i32, _len: i32) {
        unavailable!("log")
    }
    pub unsafe fn itxn_begin() {
        unavailable!("itxn_begin")
    }
    pub unsafe fn itxn_next() {
        unavailable!("itxn_next")
    }
    pub unsafe fn itxn_field_uint(_field: i32, _value: i64) {
        unavailable!("itxn_field_uint")
    }
    pub unsafe fn itxn_field_bytes(_field: i32, _ptr: i32, _len: i32) {
        unavailable!("itxn_field_bytes")
    }
    pub unsafe fn itxn_submit() {
        unavailable!("itxn_submit")
    }
    pub unsafe fn itxn_created_asset_id() -> i64 {
        unavailable!("itxn_created_asset_id")
    }
    pub unsafe fn app_global_get_uint(_key: i32, _key_len: i32) -> i64 {
        unavailable!("app_global_get_uint")
    }
    pub unsafe fn app_global_get_bytes(_key: i32, _key_len: i32, _out: i32, _len: i32) -> i32 {
        unavailable!("app_global_get_bytes")
    }
    pub unsafe fn app_global_put_uint(_key: i32, _key_len: i32, _value: i64) {
        unavailable!("app_global_put_uint")
    }
    pub unsafe fn app_global_put_bytes(_key: i32, _key_len: i32, _val: i32, _val_len: i32) {
        unavailable!("app_global_put_bytes")
    }
    pub unsafe fn app_global_del(_key: i32, _key_len: i32) {
        unavailable!("app_global_del")
    }
    pub unsafe fn ed25519verify_bare(
        _data: i32,
        _data_len: i32,
        _sig: i32,
        _sig_len: i32,
        _pk: i32,
        _pk_len: i32,
    ) -> i32 {
        unavailable!("ed25519verify_bare")
    }
}

/// Why [`arg_into`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArgError {
    /// There is no argument with that index.
    OutOfRange,
    /// The destination buffer is smaller than the argument.
    BufferTooSmall,
    /// The guest has no `memory` export, or the write was out of bounds.
    Memory,
    /// An unrecognised sentinel, which should not happen.
    Unknown,
}

impl core::fmt::Display for ArgError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            ArgError::OutOfRange => "no such argument",
            ArgError::BufferTooSmall => "destination buffer too small",
            ArgError::Memory => "invalid or missing linear memory",
            ArgError::Unknown => "unknown argument error",
        };
        f.write_str(msg)
    }
}

/// Why [`txn_bytes`] failed. Invalid transaction fields trap instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TxnError {
    /// The destination buffer is smaller than the field's value.
    BufferTooSmall,
    /// The guest has no `memory` export, or the write was out of bounds.
    Memory,
    /// An unrecognised sentinel, which should not happen.
    Unknown,
}

impl core::fmt::Display for TxnError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            TxnError::BufferTooSmall => "destination buffer too small",
            TxnError::Memory => "invalid or missing linear memory",
            TxnError::Unknown => "unknown transaction field error",
        };
        f.write_str(msg)
    }
}

/// A transaction field for [`txn_uint`], [`txn_bytes`], [`txna_uint`], and
/// [`txna_bytes`].
///
/// The discriminants match Go's `logic.TxnField` exactly; the host validates
/// the field against the current protocol. All fields are listed so the enum
/// stays in lockstep with Go. Scalar reads support current and group transactions;
/// array reads currently support only the current transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(missing_docs)]
pub enum TxnField {
    Sender = 0,
    Fee = 1,
    FirstValid = 2,
    FirstValidTime = 3,
    LastValid = 4,
    Note = 5,
    Lease = 6,
    Receiver = 7,
    Amount = 8,
    CloseRemainderTo = 9,
    VotePK = 10,
    SelectionPK = 11,
    VoteFirst = 12,
    VoteLast = 13,
    VoteKeyDilution = 14,
    Type = 15,
    TypeEnum = 16,
    XferAsset = 17,
    AssetAmount = 18,
    AssetSender = 19,
    AssetReceiver = 20,
    AssetCloseTo = 21,
    GroupIndex = 22,
    TxID = 23,
    ApplicationID = 24,
    OnCompletion = 25,
    ApplicationArgs = 26,
    NumAppArgs = 27,
    Accounts = 28,
    NumAccounts = 29,
    ApprovalProgram = 30,
    ClearStateProgram = 31,
    RekeyTo = 32,
    ConfigAsset = 33,
    ConfigAssetTotal = 34,
    ConfigAssetDecimals = 35,
    ConfigAssetDefaultFrozen = 36,
    ConfigAssetUnitName = 37,
    ConfigAssetName = 38,
    ConfigAssetURL = 39,
    ConfigAssetMetadataHash = 40,
    ConfigAssetManager = 41,
    ConfigAssetReserve = 42,
    ConfigAssetFreeze = 43,
    ConfigAssetClawback = 44,
    FreezeAsset = 45,
    FreezeAssetAccount = 46,
    FreezeAssetFrozen = 47,
    Assets = 48,
    NumAssets = 49,
    Applications = 50,
    NumApplications = 51,
    GlobalNumUint = 52,
    GlobalNumByteSlice = 53,
    LocalNumUint = 54,
    LocalNumByteSlice = 55,
    ExtraProgramPages = 56,
    Nonparticipation = 57,
    Logs = 58,
    NumLogs = 59,
    CreatedAssetID = 60,
    CreatedApplicationID = 61,
    LastLog = 62,
    StateProofPK = 63,
    ApprovalProgramPages = 64,
    NumApprovalProgramPages = 65,
    ClearStateProgramPages = 66,
    NumClearStateProgramPages = 67,
    RejectVersion = 68,
}

/// Context fields currently exposed by [`global_uint`] and [`global_bytes`].
/// Discriminants match Go's `logic.GlobalField`. These are not application
/// global-state keys. Other AVM globals (notably live OpcodeBudget) are not yet
/// exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum GlobalField {
    /// Number of transactions in this group, in either execution mode.
    GroupSize = 4,
    /// ID assigned to the current application, including during creation.
    /// Application-only, unlike `txn ApplicationID` which is zero at creation.
    CurrentApplicationID = 8,
    /// The current application's 32-byte address. Application-only.
    CurrentApplicationAddress = 10,
}

/// The number of LogicSig arguments.
#[inline]
pub fn arg_count() -> u32 {
    let n = unsafe { sys::arg_count() };
    if n < 0 {
        0
    } else {
        n as u32
    }
}

/// Copies argument `index` into `buf`, returning the bytes that were written.
///
/// A successful read marks the argument as read, which the node requires: a
/// LogicSig must read every argument it carries. There is no length query, so
/// pass a buffer at least as large as the argument (`MAX_ARG_BYTES` always
/// suffices). On error nothing is written and the argument is not marked read.
#[inline]
pub fn arg_into(index: u32, buf: &mut [u8]) -> Result<&[u8], ArgError> {
    let ret = unsafe {
        sys::arg(
            index as i32,
            buf.as_mut_ptr() as usize as i32,
            buf.len() as i32,
        )
    };
    if ret >= 0 {
        Ok(&buf[..ret as usize])
    } else {
        Err(match ret {
            ARG_ERR_INDEX => ArgError::OutOfRange,
            ARG_ERR_BUFFER => ArgError::BufferTooSmall,
            ARG_ERR_MEMORY => ArgError::Memory,
            _ => ArgError::Unknown,
        })
    }
}

/// Full unsigned product, returned as (high word, low word), like AVM mulw.
#[inline]
pub fn mulw(a: u64, b: u64) -> (u64, u64) {
    let mut words = [0u64; 2];
    // WASM memory is little-endian. The host writes two complete u64 words.
    unsafe { sys::mulw(a as i64, b as i64, words.as_mut_ptr() as usize as i32) };
    (words[0], words[1])
}

/// Unsigned 128-bit division, like AVM divmodw. Inputs are high/low words;
/// outputs are (quotient high, quotient low, remainder high, remainder low).
/// A zero denominator traps. A full-width quotient does not overflow.
#[inline]
pub fn divmodw(a_high: u64, a_low: u64, b_high: u64, b_low: u64) -> (u64, u64, u64, u64) {
    let mut words = [0u64; 4];
    unsafe {
        sys::divmodw(
            a_high as i64,
            a_low as i64,
            b_high as i64,
            b_low as i64,
            words.as_mut_ptr() as usize as i32,
        )
    };
    (words[0], words[1], words[2], words[3])
}

/// Integer floor square root of a uint64, like AVM sqrt.
#[inline]
pub fn sqrt(n: u64) -> u64 {
    unsafe { sys::sqrt(n as i64) as u64 }
}

/// Integer floor square root of an unsigned 128-bit high/low pair.
/// This is a WASM extension, not an AVM opcode; the result fits in u64.
#[inline]
pub fn sqrtw(high: u64, low: u64) -> u64 {
    unsafe { sys::sqrtw(high as i64, low as i64) as u64 }
}

/// Reads a uint64 transaction field, like the AVM's `txn`.
///
/// An invalid or out-of-version field, or a field whose value is a byte string,
/// traps the guest, as an invalid `txn` fails the AVM.
#[inline]
pub fn txn_uint(field: TxnField) -> i64 {
    unsafe { sys::txn_uint(field as i32) }
}

/// Reads a byte-string transaction field, like the AVM's `txn`.
///
/// Returns the field's length, which may exceed `buf.len()` if the buffer was
/// too small; in that case nothing is written and callers can retry with a
/// larger buffer. Passing an empty `buf` returns the length without copying.
/// An invalid or out-of-version field, or an integer field, traps the guest.
#[inline]
pub fn txn_bytes(field: TxnField, buf: &mut [u8]) -> Result<usize, TxnError> {
    let ret = unsafe {
        sys::txn_bytes(
            field as i32,
            buf.as_mut_ptr() as usize as i32,
            buf.len() as i32,
        )
    };
    if ret >= 0 {
        Ok(ret as usize)
    } else {
        Err(match ret {
            TXN_ERR_BUFFER => TxnError::BufferTooSmall,
            TXN_ERR_MEMORY => TxnError::Memory,
            _ => TxnError::Unknown,
        })
    }
}

/// Reads a uint64 array transaction field element, like the AVM's `txna`.
///
/// An invalid or out-of-version field, a field that is not an array field, an
/// out-of-range index, or a byte-string element traps the guest.
#[inline]
pub fn txna_uint(field: TxnField, index: u32) -> i64 {
    unsafe { sys::txna_uint(field as i32, index as i32) }
}

/// Reads a byte-string array transaction field element, like the AVM's `txna`.
///
/// Returns the element's length, which may exceed `buf.len()` if the buffer was
/// too small; in that case nothing is written and callers can retry. Passing an
/// empty `buf` returns the length without copying. An invalid or out-of-version
/// field, a field that is not an array field, an out-of-range index, or an
/// integer element traps the guest.
#[inline]
pub fn txna_bytes(field: TxnField, index: u32, buf: &mut [u8]) -> Result<usize, TxnError> {
    let ret = unsafe {
        sys::txna_bytes(
            field as i32,
            index as i32,
            buf.as_mut_ptr() as usize as i32,
            buf.len() as i32,
        )
    };
    if ret >= 0 {
        Ok(ret as usize)
    } else {
        Err(match ret {
            TXN_ERR_BUFFER => TxnError::BufferTooSmall,
            TXN_ERR_MEMORY => TxnError::Memory,
            _ => TxnError::Unknown,
        })
    }
}

/// Reads a scalar uint64 field from transaction `group_index`. Invalid indices,
/// unavailable effects, array fields, and wrong-typed fields trap as in the AVM.
#[inline]
pub fn gtxn_uint(group_index: u32, field: TxnField) -> u64 {
    unsafe { sys::gtxn_uint(group_index as i32, field as i32) as u64 }
}

/// Copies a scalar byte field from transaction `group_index`. An empty buffer
/// queries the length without copying. Invalid fields or indices trap.
#[inline]
pub fn gtxn_bytes(group_index: u32, field: TxnField, buf: &mut [u8]) -> Result<usize, TxnError> {
    let ret = unsafe {
        sys::gtxn_bytes(
            group_index as i32,
            field as i32,
            buf.as_mut_ptr() as usize as i32,
            buf.len() as i32,
        )
    };
    field_bytes_result(ret)
}

/// Reads a uint64 context field. Wrong types and unavailable modes trap.
#[inline]
pub fn global_uint(field: GlobalField) -> u64 {
    unsafe { sys::global_uint(field as i32) as u64 }
}

/// Copies a context byte field. An empty buffer queries its length without
/// copying. Wrong types and unavailable modes trap.
#[inline]
pub fn global_bytes(field: GlobalField, buf: &mut [u8]) -> Result<usize, TxnError> {
    let ret = unsafe {
        sys::global_bytes(
            field as i32,
            buf.as_mut_ptr() as usize as i32,
            buf.len() as i32,
        )
    };
    field_bytes_result(ret)
}

#[inline]
fn field_bytes_result(ret: i32) -> Result<usize, TxnError> {
    if ret >= 0 {
        Ok(ret as usize)
    } else {
        Err(match ret {
            TXN_ERR_BUFFER => TxnError::BufferTooSmall,
            TXN_ERR_MEMORY => TxnError::Memory,
            _ => TxnError::Unknown,
        })
    }
}

/// Reads an account's asset balance. `None` means no holding, which is distinct
/// from `Some(0)` (opted in with no units). Unavailable resources trap, rather
/// than returning `None`. Application-only; follows AVM asset reference rules.
#[inline]
pub fn asset_balance(account: &[u8; 32], asset: u64) -> Option<u64> {
    let mut value = [0u8; 8];
    let exists = unsafe {
        sys::asset_balance(
            account.as_ptr() as usize as i32,
            asset as i64,
            value.as_mut_ptr() as usize as i32,
        )
    };
    match exists {
        0 => None,
        1 => Some(u64::from_le_bytes(value)),
        _ => {
            // Avoid pulling panic-formatting code into a size-sensitive guest.
            #[cfg(target_arch = "wasm32")]
            core::arch::wasm32::unreachable();
            #[cfg(not(target_arch = "wasm32"))]
            panic!("invalid asset balance result");
        }
    }
}

/// Copies an asset's unit name. `None` means the asset is missing; `Some(0)`
/// means an existing empty name. An empty buffer queries length. ABI errors do
/// not write memory. Unavailable resources trap. Application-only.
#[inline]
pub fn asset_unit_name(asset: u64, buf: &mut [u8]) -> Result<Option<usize>, TxnError> {
    let ret = unsafe {
        sys::asset_unit_name(
            asset as i64,
            buf.as_mut_ptr() as usize as i32,
            buf.len() as i32,
        )
    };
    if ret == -1 {
        Ok(None)
    } else {
        field_bytes_result(ret).map(Some)
    }
}

/// Starts an inner asset transaction group. Application-only; invalid lifecycle
/// transitions and clear-state restrictions trap as in the AVM.
#[inline]
pub fn itxn_begin() {
    unsafe { sys::itxn_begin() }
}

/// Appends an application log. AVM log count/total-byte limits apply; failures trap.
#[inline]
pub fn log(bytes: &[u8]) {
    unsafe { sys::log(bytes.as_ptr() as usize as i32, bytes.len() as i32) }
}

/// Adds the next inner transaction to the pending group.
#[inline]
pub fn itxn_next() {
    unsafe { sys::itxn_next() }
}

/// Sets an integer field. Only asset config/transfer fields and common header
/// fields are exposed; TypeEnum accepts acfg=3 or axfer=4. Wrong types trap.
#[inline]
pub fn itxn_field_uint(field: TxnField, value: u64) {
    unsafe { sys::itxn_field_uint(field as i32, value as i64) }
}

/// Sets a byte-string field, copying the bytes into the host transaction.
/// Wrong types, unavailable resources, oversized values, and bad memory trap.
#[inline]
pub fn itxn_field_bytes(field: TxnField, bytes: &[u8]) {
    unsafe {
        sys::itxn_field_bytes(
            field as i32,
            bytes.as_ptr() as usize as i32,
            bytes.len() as i32,
        )
    }
}

/// Submits the pending asset group through the AVM ledger path. Fees and all
/// protocol/resource limits still apply. No inner app calls are supported.
#[inline]
pub fn itxn_submit() {
    unsafe { sys::itxn_submit() }
}

/// The last submitted inner transaction's CreatedAssetID. Traps before any
/// submission; returns 0 if the last transaction did not create an asset.
#[inline]
pub fn itxn_created_asset_id() -> u64 {
    unsafe { sys::itxn_created_asset_id() as u64 }
}

/// Reads the uint64 global `key` from the current application, the AVM's
/// `app_global_get`. A missing key reads as 0; a stored byte-string value traps
/// the guest, as a mismatched AVM value would fail.
#[inline]
pub fn app_global_get_uint(key: &[u8]) -> u64 {
    unsafe { sys::app_global_get_uint(key.as_ptr() as usize as i32, key.len() as i32) as u64 }
}

/// Copies the byte-string global `key` from the current application, the AVM's
/// `app_global_get` for a byte value. A missing key reads as empty; a stored
/// uint value traps the guest. If `buf` is too small nothing is written and
/// [`TxnError::BufferTooSmall`] is returned.
#[inline]
pub fn app_global_get_bytes<'a>(key: &[u8], buf: &'a mut [u8]) -> Result<&'a [u8], TxnError> {
    let ret = unsafe {
        sys::app_global_get_bytes(
            key.as_ptr() as usize as i32,
            key.len() as i32,
            buf.as_mut_ptr() as usize as i32,
            buf.len() as i32,
        )
    };
    if ret >= 0 {
        let len = ret as usize;
        if len > buf.len() {
            return Err(TxnError::BufferTooSmall);
        }
        Ok(&buf[..len])
    } else {
        Err(match ret {
            GLOBAL_ERR_BUFFER => TxnError::BufferTooSmall,
            GLOBAL_ERR_MEMORY => TxnError::Memory,
            _ => TxnError::Unknown,
        })
    }
}

/// Writes the uint64 `value` to global `key`, the AVM's `app_global_put`. A
/// limit violation (a too-long key or value) traps the guest, aborting the
/// application call.
#[inline]
pub fn app_global_put_uint(key: &[u8], value: u64) {
    unsafe {
        sys::app_global_put_uint(key.as_ptr() as usize as i32, key.len() as i32, value as i64);
    }
}

/// Writes the byte-string `value` to global `key`, the AVM's `app_global_put`
/// for a byte value.
#[inline]
pub fn app_global_put_bytes(key: &[u8], value: &[u8]) {
    unsafe {
        sys::app_global_put_bytes(
            key.as_ptr() as usize as i32,
            key.len() as i32,
            value.as_ptr() as usize as i32,
            value.len() as i32,
        );
    }
}

/// Deletes global `key`, the AVM's `app_global_del`. Deleting an absent key is
/// not an error.
#[inline]
pub fn app_global_del(key: &[u8]) {
    unsafe {
        sys::app_global_del(key.as_ptr() as usize as i32, key.len() as i32);
    }
}

/// Verifies an ed25519 signature, the AVM's `ed25519verify_bare`.
///
/// Returns `true` when `sig` is a valid signature over `data` under `pk`. The
/// operands are verified by the host (Go), so this matches the AVM exactly. A
/// key or signature of the wrong length, an oversized `data`, or an
/// out-of-bounds range traps the guest; taking fixed-size arrays makes the
/// length cases impossible.
#[inline]
pub fn ed25519_verify_bare(data: &[u8], sig: &[u8; 64], pk: &[u8; 32]) -> bool {
    let ret = unsafe {
        sys::ed25519verify_bare(
            data.as_ptr() as usize as i32,
            data.len() as i32,
            sig.as_ptr() as usize as i32,
            sig.len() as i32,
            pk.as_ptr() as usize as i32,
            pk.len() as i32,
        )
    };
    ret == 1
}
