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

//! Native-test host installation.
//!
//! The wasm build always uses [`crate::HostAvm`]. Native tests need to inject a
//! mock, so the active host is stored in a thread-local. [`set_host`] returns a
//! guard that restores the previous host when dropped, which keeps tests
//! isolated even when the test harness reuses threads.

use std::cell::Cell;
use std::ops::Deref;

use crate::avm::Avm;

thread_local! {
    static HOST: Cell<Option<&'static dyn Avm>> = const { Cell::new(None) };
}

/// The installed host. Dereferences to it so a test can inspect its state, and
/// restores the previous host when dropped.
pub struct HostGuard<A: 'static> {
    host: &'static A,
    previous: Option<&'static dyn Avm>,
}

impl<A> Deref for HostGuard<A> {
    type Target = A;

    fn deref(&self) -> &A {
        self.host
    }
}

impl<A> Drop for HostGuard<A> {
    fn drop(&mut self) {
        HOST.with(|slot| slot.set(self.previous));
    }
}

/// Installs `host` for the current thread until the returned guard is dropped.
///
/// The host is leaked so the SDK can hand out `'static` references to it; one
/// small allocation per test is the intended use.
pub fn set_host<A: Avm + 'static>(host: A) -> HostGuard<A> {
    let host: &'static A = Box::leak(Box::new(host));
    let previous = HOST.with(|slot| slot.replace(Some(host)));
    HostGuard { host, previous }
}

/// Returns the current thread's host, panicking if none is installed.
pub(crate) fn current_host() -> &'static dyn Avm {
    HOST.with(|slot| {
        slot.get()
            .expect("no AVM test host installed; call avm_sdk::testing::set_host first")
    })
}
