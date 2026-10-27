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

//! An opinionated SDK for authoring WASM programs for the AVM.
//!
//! `avm-sdk` wraps the [`avm_sys`] ABI bindings (the `arg`, `txn`,
//! global-state, and `ed25519verify_bare` imports) and adds:
//!
//! - [`arg`], [`txn`], and [`global`]: the host operations as free functions,
//!   plus typed helpers (`txn::sender`, `txn::app_arg_into`, ...).
//! - [`GlobalUint`] and [`GlobalBytes`], `const`-constructible handles for the
//!   current application's global state.
//! - [`contract`], an attribute macro for a contract's state struct and its
//!   `impl` block. On the struct it derives each key from the field name and
//!   computes the global-state schema ([`Contract`]); on the impl it exports
//!   `program() -> i32` calling `approve(&self)`.
//! - [`arc4`]: `#[abimethod]`s in a `#[contract]` impl are routed by ARC-4
//!   method selector, with their arguments decoded into Rust types.
//! - [`program_entry`], the same export for a free function.
//! - Both macros can install a guest-side bump allocator with `alloc`.
//! - [`entry!`], the lower-level macro for a closure body.
//!
//! Every helper reaches the host through [`host`]. On wasm that is the concrete
//! [`HostAvm`], so calls compile to direct imports. On native builds it is the
//! mock installed with `avm_sdk::testing::set_host`, so the same program code
//! runs in unit tests. The SDK does not re-export the raw `avm_sys` functions,
//! which would bypass the mock; depend on `avm-sys` directly to use them.
//!
//! The crate is `no_std` on wasm. Owned-value helpers require the Cargo feature
//! `alloc`; install a guest allocator with `#[program_entry(alloc)]` or
//! `#[contract(alloc)]` on the impl when using them.
//!
//! # Example
//!
//! ```ignore
//! use avm_sdk::{contract, txn, Error, GlobalUint};
//!
//! #[contract]
//! pub struct Counter {
//!     count: GlobalUint,
//! }
//!
//! #[contract]
//! impl Counter {
//!     pub fn approve(&self) -> Result<(), Error> {
//!         if txn::application_id().id == 0 {
//!             return Ok(());
//!         }
//!         self.count.set(self.count.get() + 1);
//!         Ok(())
//!     }
//! }
//! ```
//!
//! Native tests install a mock host and run the same code:
//!
//! ```ignore
//! let _host = avm_sdk::testing::set_host(MockAvm::default());
//! assert!(Counter::new().approve().is_ok());
//! ```
//!
//! The host ABI copies into caller-provided buffers and supports length queries
//! (`len == 0`), so the default SDK needs no allocator. Enable one only when a
//! program benefits from `Vec`/`String`; the allocator is a bump arena and
//! `dealloc` is a no-op, matching the per-invocation lifetime of the VM.

#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(feature = "alloc")]
extern crate alloc as rust_alloc;

mod avm;
#[cfg(any(target_arch = "wasm32", test))]
mod bump;
mod bytes;
mod error;

pub mod application;
pub mod arc4;
pub mod arg;
pub mod asset;
pub mod global;
pub mod globals;
pub mod itxn;
pub mod txn;

#[cfg(not(target_arch = "wasm32"))]
pub mod testing;

#[cfg(target_arch = "wasm32")]
mod alloc;
#[cfg(target_arch = "wasm32")]
pub use alloc::BumpAllocator;

pub use application::Application;
pub use asset::Asset;
pub use avm::{host, Avm, Host, HostAvm};
pub use avm_sdk_macros::{contract, program_entry};
pub use avm_sys::{ArgError, GlobalField, TxnError, TxnField, MAX_ARG_BYTES};
pub use error::Error;
pub use global::{GlobalBytes, GlobalUint, StateField};

/// Host-backed unsigned arithmetic, available in application and signature mode.
pub mod math {
    use crate::host;
    #[cfg(target_arch = "wasm32")]
    use crate::Avm;

    /// Full unsigned product as (high word, low word), matching AVM mulw.
    #[inline]
    pub fn mulw(a: u64, b: u64) -> (u64, u64) {
        host().mulw(a, b)
    }

    /// 128-bit unsigned division. Operands are high/low words; the result is
    /// (quotient high, quotient low, remainder high, remainder low).
    /// A zero denominator traps, matching AVM divmodw.
    #[inline]
    pub fn divmodw(a_high: u64, a_low: u64, b_high: u64, b_low: u64) -> (u64, u64, u64, u64) {
        host().divmodw(a_high, a_low, b_high, b_low)
    }

    /// Integer floor square root of a uint64, matching AVM sqrt.
    #[inline]
    pub fn sqrt(n: u64) -> u64 {
        host().sqrt(n)
    }

    /// Integer floor square root of a 128-bit high/low pair. WASM extension;
    /// the full-width result fits in u64.
    #[inline]
    pub fn sqrtw(high: u64, low: u64) -> u64 {
        host().sqrtw(high, low)
    }
}

/// Appends an application log. Protocol count and total-byte limits apply.
#[inline]
pub fn log(bytes: &[u8]) {
    host().log(bytes)
}

/// A contract's global-state schema, implemented by `#[contract]`.
///
/// An application declares how many global uint64 and byte-string slots it
/// uses when it is created; these are the counts from the struct's fields.
pub trait Contract {
    /// Global uint64 slots the contract uses.
    const GLOBAL_UINTS: u32;
    /// Global byte-string slots the contract uses.
    const GLOBAL_BYTES: u32;
}

/// Verifies an ed25519 signature with the AVM's `ed25519verify_bare`
/// semantics.
#[inline]
pub fn ed25519_verify_bare(data: &[u8], sig: &[u8; 64], pk: &[u8; 32]) -> bool {
    host().ed25519_verify_bare(data, sig, pk)
}

/// Runtime trap used when the AVM would fail the program.
///
/// On `wasm32` this lowers to `unreachable`, which the executor reports as a
/// guest trap. On the host target (unit tests) it panics.
#[inline]
pub fn trap() -> ! {
    #[cfg(target_arch = "wasm32")]
    core::arch::wasm32::unreachable();

    #[cfg(not(target_arch = "wasm32"))]
    panic!("avm-sdk trap");
}

/// Asserts a condition without constructing a panic message. Failure calls
/// [`trap`]: WASM traps immediately; native tests panic. The condition is
/// evaluated exactly once. Unlike `core::assert!`, message arguments are not
/// supported; use the core macro when formatted diagnostics are wanted.
#[macro_export]
macro_rules! assert {
    ($condition:expr $(,)?) => {
        if !$condition {
            $crate::trap();
        }
    };
}

/// Asserts equality without formatting operands or requiring `Debug`.
/// Operands are evaluated once, left to right, and borrowed rather than moved.
/// Failure calls [`trap`]; message arguments are not supported.
#[macro_export]
macro_rules! assert_eq {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left, right) => {
                if !(*left == *right) {
                    $crate::trap();
                }
            }
        }
    };
}

/// Asserts inequality without formatting operands or requiring `Debug`.
/// Operands are evaluated once, left to right, and borrowed rather than moved.
/// Failure calls [`trap`]; message arguments are not supported.
#[macro_export]
macro_rules! assert_ne {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left, right) => {
                if !(*left != *right) {
                    $crate::trap();
                }
            }
        }
    };
}

/// Converts a guest body's return value into an approval decision.
///
/// `true` and `Ok(())` approve; `false` and `Err(_)` reject. Used by
/// [`contract`] and [`program_entry`].
pub trait Approve {
    /// Whether the program approves the transaction.
    fn approve(self) -> bool;
}

impl Approve for bool {
    #[inline]
    fn approve(self) -> bool {
        self
    }
}

impl Approve for () {
    #[inline]
    fn approve(self) -> bool {
        true
    }
}

impl<E> Approve for Result<(), E> {
    #[inline]
    fn approve(self) -> bool {
        self.is_ok()
    }
}

/// Defines the module's `program` export from a fallible body.
///
/// The body is an expression callable as `FnOnce() -> R`, where `R` implements
/// [`Approve`]. It may be a closure or a function path. `true` / `Ok(())`
/// approves; `false` / `Err(_)` rejects.
///
/// ```ignore
/// avm_sdk::entry!(|| {
///     let mut buf = [0u8; 4096];
///     avm_sdk::arg::get(0, &mut buf).is_ok()
/// });
/// ```
#[macro_export]
macro_rules! entry {
    ($body:expr) => {
        #[no_mangle]
        pub extern "C" fn program() -> i32 {
            let approve: bool = $crate::Approve::approve(($body)());
            if approve {
                1
            } else {
                0
            }
        }
    };
}

/// Installs a panic handler appropriate for a WASM guest. Call this once at
/// crate scope:
///
/// ```ignore
/// avm_sdk::panic_handler!();
/// ```
#[macro_export]
macro_rules! panic_handler {
    () => {
        #[cfg(target_arch = "wasm32")]
        #[panic_handler]
        fn panic(_info: &core::panic::PanicInfo) -> ! {
            core::arch::wasm32::unreachable()
        }
    };
}
