#![no_std]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

use alloc::vec;
use alloc::vec::Vec;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

#[link(wasm_import_module = "algorand")]
unsafe extern "C" {
    fn host_get_global_uint(app: u64, key: *const u8, len: i32) -> u64;
    fn host_set_global_uint(app: u64, key: *const u8, len: i32, value: u64);

    fn host_get_global_bytes(
        app: u64,
        key: *const u8,
        key_len: i32,
        byte_buffer: *mut u8,
        byte_buffer_len: i32,
    ) -> i32;
    fn host_set_global_bytes(
        app: u64,
        key: *const u8,
        key_len: i32,
        bytes: *const u8,
        bytes_len: i32,
    );

    fn host_get_current_application_id() -> u64;

    fn host_bigint_add(
        aPtr: *const u8,
        aLen: u32,
        bPtr: *const u8,
        bLen: u32,
        cPtr: *mut u8,
    ) -> u32;

    fn host_alloc(size: u32) -> *mut u8;
    fn host_dealloc(ptr: *mut u8);
}

struct HostAllocator;

unsafe impl GlobalAlloc for HostAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return ptr::null_mut();
        }

        // Host allocator uses 4k chunks, providing excellent alignment
        // No need for custom alignment handling
        unsafe { host_alloc(layout.size() as u32) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if !ptr.is_null() {
            unsafe {
                host_dealloc(ptr);
            }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if layout.size() != 0 {
            // Deallocate the old pointer
            unsafe {
                host_dealloc(ptr);
            }
        }

        if new_size == 0 {
            return ptr::null_mut();
        }

        unsafe { host_alloc(new_size as u32) }
    }
}

#[global_allocator]
static ALLOCATOR: HostAllocator = HostAllocator;

pub fn get_global_uint(app: u64, key: &[u8]) -> u64 {
    unsafe {
        let key_bytes = key;
        host_get_global_uint(app, key_bytes.as_ptr(), key_bytes.len() as i32)
    }
}

pub fn set_global_uint(app: u64, key: &[u8], value: u64) {
    unsafe {
        let key_bytes = key;
        host_set_global_uint(app, key_bytes.as_ptr(), key_bytes.len() as i32, value);
    }
}

pub fn get_global_bytes(app: u64, key: &[u8]) -> Vec<u8> {
    unsafe {
        let mut buffer = Vec::<u8>::with_capacity(128 as usize);
        let buffer_ptr = buffer.as_mut_ptr();

        host_get_global_bytes(app, key.as_ptr(), key.len() as i32, buffer_ptr, 128 as i32);

        buffer
    }
}

pub fn set_global_bytes(app: u64, key: &[u8], value: &[u8]) {
    unsafe {
        let key_bytes = key;
        host_set_global_bytes(
            app,
            key_bytes.as_ptr(),
            key_bytes.len() as i32,
            value.as_ptr(),
            value.len() as i32,
        );
    }
}

/// Caches the current application ID to avoid repeated calls to the host function.
static mut APP_ID: Option<u64> = None;

pub fn get_current_application_id() -> u64 {
    unsafe {
        APP_ID.unwrap_or_else(|| {
            let id = host_get_current_application_id();
            APP_ID = Some(id);
            id
        })
    }
}

pub struct GlobalStateKey<ValueType> {
    pub key: &'static [u8],
    phantom: core::marker::PhantomData<ValueType>,
}

impl<ValueType> GlobalStateKey<ValueType> {
    pub const fn new(key: &'static [u8]) -> Self {
        GlobalStateKey {
            key,
            phantom: core::marker::PhantomData,
        }
    }

    #[inline(always)]
    fn app_id(&self) -> u64 {
        get_current_application_id()
    }
}

impl<T> GlobalStateKey<T>
where
    T: for<'a> From<&'a [u8]>,
    T: AsRef<[u8]>,
{
    pub fn get(&self) -> T {
        let bytes = get_global_bytes(self.app_id(), self.key);
        T::from(bytes.as_slice())
    }

    pub fn set(&self, value: &T) {
        set_global_bytes(self.app_id(), self.key, value.as_ref());
    }
}

macro_rules! uint_type {
    ($name:ident, $bits:expr) => {
        #[derive(PartialEq, Eq, Debug, Clone, Copy)]
        pub struct $name {
            pub bytes: [u8; ($bits + 7) / 8],
        }

        impl $name {
            pub const BITS: usize = $bits;
            pub const BYTES: usize = ($bits + 7) / 8;

            pub fn new() -> Self {
                Self {
                    bytes: [0; ($bits + 7) / 8],
                }
            }
        }

        impl core::ops::Add for $name {
            type Output = $name;

            fn add(self, other: $name) -> $name {
                let mut result = vec![0; $name::BYTES];
                let len = unsafe {
                    host_bigint_add(
                        self.bytes.as_ptr(),
                        $name::BYTES as u32,
                        other.bytes.as_ptr(),
                        $name::BYTES as u32,
                        result.as_mut_ptr(),
                    )
                };

                if len as usize > $name::BYTES {
                    panic!("Result length exceeds buffer size");
                }

                result.truncate(len as usize);
                let mut val = $name::new();
                val.bytes.copy_from_slice(&result[..$name::BYTES]);
                val
            }
        }

        impl From<&[u8]> for $name {
            fn from(bytes: &[u8]) -> Self {
                let mut val = $name::new();
                val.bytes.copy_from_slice(bytes);
                val
            }
        }

        impl AsRef<[u8]> for $name {
            fn as_ref(&self) -> &[u8] {
                &self.bytes
            }
        }
    };
}

uint_type!(Uint256, 256);
