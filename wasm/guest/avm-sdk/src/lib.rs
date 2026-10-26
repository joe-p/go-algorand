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
//! `avm-sdk` wraps the raw [`avm_sys`] ABI bindings (the `arg`, `txn`,
//! global-state, and `ed25519verify_bare` imports) and adds:
//!
//! - [`Avm`], a trait abstracting the host, so guest logic can be tested with a
//!   native mock instead of the executor.
//! - [`GlobalUint`] and [`GlobalBytes`], typed handles for the current
//!   application's global state.
//! - [`AvmExt`], typed transaction helpers (`sender`, `application_id`,
//!   application arguments, and `ed25519verify_bare`).
//! - [`program_entry`], an attribute macro that exports `program() -> i32` and
//!   can optionally install a guest-side bump allocator.
//! - [`entry!`], the lower-level macro for a closure body.
//!
//! The crate is `no_std` (except when running its own tests) and does not
//! allocate unless the guest opts in with `#[program_entry(alloc)]`.
//!
//! # Example
//!
//! ```ignore
//! use avm_sdk::{ActiveAvm, AvmExt, Error, GlobalUint, program_entry};
//!
//! #[program_entry]
//! fn approve(avm: ActiveAvm) -> Result<(), Error> {
//!     if avm.application_id() == 0 {
//!         return Ok(());
//!     }
//!     let count = GlobalUint::<u64>::new(avm, b"count");
//!     count.set(count.get() + 1);
//!     Ok(())
//! }
//! ```
//!
//! The host ABI copies into caller-provided buffers and supports length queries
//! (`len == 0`), so the default SDK needs no allocator. Enable one only when a
//! program benefits from `Vec`/`String`; the allocator is a bump arena and
//! `dealloc` is a no-op, matching the per-invocation lifetime of the VM.

#![cfg_attr(not(test), no_std)]

mod avm;
#[cfg(any(target_arch = "wasm32", test))]
mod bump;
mod error;
mod state;
mod txn;

#[cfg(target_arch = "wasm32")]
mod alloc;
#[cfg(target_arch = "wasm32")]
pub use alloc::BumpAllocator;

pub use avm::{host, ActiveAvm, Avm, HostAvm, HOST_AVM};
pub use avm_sdk_macros::program_entry;
pub use avm_sys::{
    app_global_del, app_global_get_bytes, app_global_get_uint, app_global_put_bytes,
    app_global_put_uint, arg_count, arg_into, ed25519_verify_bare, txn_bytes, txn_uint, txna_bytes,
    txna_uint, ArgError, TxnError, TxnField, MAX_ARG_BYTES,
};
pub use error::Error;
pub use state::{GlobalBytes, GlobalUint};
pub use txn::{app_arg_into, app_args_count, AvmExt};

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

/// Converts a guest body's return value into an approval decision.
///
/// `true` and `Ok(())` approve; `false` and `Err(_)` reject. Used by
/// [`program_entry`].
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
///     arg_into(0, &mut buf).is_ok()
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
