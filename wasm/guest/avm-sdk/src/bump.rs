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

//! The bump-allocation bookkeeping, kept independent of wasm intrinsics so it
//! can be unit-tested on the host.

/// WebAssembly linear-memory page size.
pub(crate) const PAGE_SIZE: usize = 64 * 1024;

/// A cursor over the guest's linear memory.
///
/// The first allocation records the current memory size as the heap base. That
/// is safe because a Rust `wasm32` module's initial memory already covers its
/// static data and shadow stack, so everything above it is free. This is the
/// same approach `soroban-sdk` uses for its optional allocator.
pub(crate) struct BumpCursor {
    cursor: usize,
    limit: usize,
    initialized: bool,
}

impl BumpCursor {
    pub(crate) const fn new() -> Self {
        BumpCursor {
            cursor: 0,
            limit: 0,
            initialized: false,
        }
    }

    #[inline]
    fn align_up(value: usize, align: usize) -> Option<usize> {
        value.checked_add(align - 1).map(|v| v & !(align - 1))
    }

    /// Allocates `bytes` at `align` alignment.
    ///
    /// `memory_size` returns the current linear-memory size in bytes. `grow`
    /// grows by `pages` and returns the previous size in bytes, or `None` when
    /// growth failed. Returns `None` on overflow or exhaustion.
    pub(crate) fn alloc(
        &mut self,
        bytes: usize,
        align: usize,
        memory_size: impl FnOnce() -> usize,
        grow: impl FnOnce(usize) -> Option<usize>,
    ) -> Option<usize> {
        if align == 0 || !align.is_power_of_two() {
            return None;
        }
        // A zero-size allocation must return a non-null, aligned pointer and
        // must not consume memory.
        if bytes == 0 {
            return Some(align);
        }

        if !self.initialized {
            self.cursor = memory_size();
            self.limit = self.cursor;
            self.initialized = true;
        }

        let start = Self::align_up(self.cursor, align)?;
        let end = start.checked_add(bytes)?;
        if end <= self.limit {
            self.cursor = end;
            return Some(start);
        }

        let missing = end - self.limit;
        let pages = missing.checked_add(PAGE_SIZE - 1)? / PAGE_SIZE;
        let previous_size = grow(pages)?;
        self.limit = previous_size.checked_add(pages.checked_mul(PAGE_SIZE)?)?;

        let start = Self::align_up(self.cursor, align)?;
        let end = start.checked_add(bytes)?;
        if end > self.limit {
            return None;
        }
        self.cursor = end;
        Some(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_starts_at_the_current_memory_size() {
        let mut cursor = BumpCursor::new();
        let base = 17 * PAGE_SIZE;
        let mut grown = None;
        let ptr = cursor.alloc(
            8,
            8,
            || base,
            |pages| {
                grown = Some(pages);
                Some(base)
            },
        );
        // The base is the first free byte, so the first allocation grows.
        assert_eq!(ptr, Some(base));
        assert_eq!(grown, Some(1));
    }

    #[test]
    fn grows_by_whole_pages() {
        use core::cell::Cell;

        let mut cursor = BumpCursor::new();
        let size = Cell::new(PAGE_SIZE);
        let mut pages_seen = Vec::new();
        let mut grow = |pages: usize| {
            pages_seen.push(pages);
            let previous = size.get();
            size.set(previous + pages * PAGE_SIZE);
            Some(previous)
        };

        let first = cursor.alloc(PAGE_SIZE - 8, 8, || size.get(), &mut grow);
        assert_eq!(first, Some(PAGE_SIZE));

        // Eight bytes remain, so the next allocation needs another page.
        let second = cursor.alloc(16, 8, || size.get(), &mut grow);
        assert_eq!(second, Some(2 * PAGE_SIZE - 8));
        assert_eq!(pages_seen, [1, 1]);
    }

    #[test]
    fn honors_alignment() {
        let mut cursor = BumpCursor::new();
        let ptr = cursor.alloc(1, 8, || 3, |_| Some(3));
        assert_eq!(ptr, Some(8));
    }

    #[test]
    fn zero_size_is_dangling_but_aligned() {
        let mut cursor = BumpCursor::new();
        let ptr = cursor.alloc(0, 16, || 0, |_| panic!("must not grow"));
        assert_eq!(ptr, Some(16));
    }

    #[test]
    fn rejects_bad_alignment() {
        let mut cursor = BumpCursor::new();
        assert_eq!(cursor.alloc(1, 3, || 0, |_| None), None);
        assert_eq!(cursor.alloc(1, 0, || 0, |_| None), None);
    }

    #[test]
    fn reports_exhaustion() {
        let mut cursor = BumpCursor::new();
        assert_eq!(cursor.alloc(usize::MAX, 1, || 0, |_| Some(0)), None);
        assert_eq!(cursor.alloc(1, 1, || 0, |_| None), None);
    }
}
