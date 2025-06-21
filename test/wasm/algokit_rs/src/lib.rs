#![no_std]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
use core::ptr;

use alloc::vec::Vec;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

const DEFAULT_BUFFER_LEN: i32 = 4000;

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

pub fn get_global_bytes(app: u64, key: &[u8], buffer_len: i32) -> Vec<u8> {
    unsafe {
        let mut buffer = Vec::<u8>::with_capacity(buffer_len as usize);
        let buffer_ptr = buffer.as_mut_ptr();

        let len =
            host_get_global_bytes(app, key.as_ptr(), key.len() as i32, buffer_ptr, buffer_len);

        if len >= 0 {
            buffer.set_len(len as usize);
            buffer.shrink_to_fit();
            buffer
        } else {
            Vec::new() // Return an empty vector if the length is negative
        }
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

pub fn get_current_application_id() -> u64 {
    unsafe { host_get_current_application_id() }
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

impl GlobalStateKey<u64> {
    pub fn get(&self) -> u64 {
        get_global_uint(self.app_id(), self.key)
    }
    pub fn set(&self, value: u64) {
        set_global_uint(self.app_id(), self.key, value);
    }
}

impl GlobalStateKey<&[u8]> {
    pub fn get(&self) -> Vec<u8> {
        get_global_bytes(self.app_id(), self.key, DEFAULT_BUFFER_LEN)
    }

    pub fn set(&self, value: &[u8]) {
        set_global_bytes(self.app_id(), self.key, value);
    }
}

pub trait AvmBytes {
    fn as_bytes(&self) -> &[u8];
    fn from_bytes(bytes: &[u8]) -> Self;
}

impl<T: AvmBytes> GlobalStateKey<T> {
    pub fn get(&self) -> T {
        let bytes = get_global_bytes(self.app_id(), self.key, DEFAULT_BUFFER_LEN);
        T::from_bytes(bytes.as_slice())
    }

    pub fn set(&self, value: &T) {
        set_global_bytes(self.app_id(), self.key, value.as_bytes());
    }
}

#[cfg(feature = "bigint")]
impl GlobalStateKey<crypto_bigint::U256> {
    pub fn get(&self) -> crypto_bigint::U256 {
        let bytes = get_global_bytes(self.app_id(), self.key, DEFAULT_BUFFER_LEN);
        crypto_bigint::U256::from_be_slice(bytes.as_slice())
    }

    pub fn set(&self, value: &crypto_bigint::U256) {
        set_global_bytes(self.app_id(), self.key, value.to_be_bytes().as_ref());
    }
}
