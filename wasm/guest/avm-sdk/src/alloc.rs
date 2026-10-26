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

//! Optional guest-side bump allocator.
//!
//! Install it with `#[avm_sdk::program_entry(alloc)]`. It is deliberately
//! simple: allocations advance through linear memory, memory grows by whole
//! pages, and `dealloc` does nothing. The VM instance is destroyed after each
//! invocation, so there is nothing to reclaim during the call.
//!
//! The trade-off is that an allocation-heavy loop will exhaust the executor's
//! linear-memory cap. Programs that do not need `Vec`/`String` should leave the
//! allocator out and use caller-provided buffers.

use core::alloc::{GlobalAlloc, Layout};

use crate::bump::{BumpCursor, PAGE_SIZE};

/// A bump-pointer [`GlobalAlloc`] over the guest's linear memory.
pub struct BumpAllocator;

// Safety: wasm32 is single-threaded and the executor never calls the allocator
// concurrently. The reference is scoped to one `GlobalAlloc` call and no code
// reached from the cursor can re-enter the allocator.
static mut ALLOCATOR: BumpCursor = BumpCursor::new();

impl BumpAllocator {
    /// A const constructor suitable for a `#[global_allocator]` static.
    pub const fn new() -> Self {
        BumpAllocator
    }
}

impl Default for BumpAllocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let cursor = unsafe { &mut *core::ptr::addr_of_mut!(ALLOCATOR) };
        let ptr = cursor.alloc(
            layout.size(),
            layout.align(),
            || core::arch::wasm32::memory_size(0) * PAGE_SIZE,
            |pages| {
                let previous = core::arch::wasm32::memory_grow(0, pages);
                if previous == usize::MAX {
                    None
                } else {
                    previous.checked_mul(PAGE_SIZE)
                }
            },
        );
        match ptr {
            Some(ptr) => ptr as *mut u8,
            None => core::ptr::null_mut(),
        }
    }

    #[inline]
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // The bump allocator never reclaims memory.
    }
}
