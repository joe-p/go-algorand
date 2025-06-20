use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

#[link(wasm_import_module = "algorand")]
unsafe extern "C" {
    fn host_get_global_uint(app: u64, key: *const u8, len: i32) -> u64;
    fn host_set_global_uint(app: u64, key: *const u8, len: i32, value: u64);
}

pub fn get_global_uint(app: u64, key: &str) -> u64 {
    unsafe {
        let key_bytes = key.as_bytes();
        host_get_global_uint(app, key_bytes.as_ptr(), key_bytes.len() as i32)
    }
}

pub fn set_global_uint(app: u64, key: &str, value: u64) {
    unsafe {
        let key_bytes = key.as_bytes();
        host_set_global_uint(app, key_bytes.as_ptr(), key_bytes.len() as i32, value);
    }
}
