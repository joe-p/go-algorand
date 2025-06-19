mod algokit {
    use std::ffi::CString;
    use std::os::raw::c_char;

    #[link(wasm_import_module = "algorand")]
    unsafe extern "C" {
        fn host_hello(message: *const c_char);
        fn host_get_global_uint(app: u64, key: *const c_char, len: i32) -> u64;
        fn host_set_global_uint(app: u64, key: *const c_char, len: i32, value: u64);
    }

    pub fn get_global_uint(app: u64, key: &str) -> u64 {
        unsafe {
            let c_key = CString::new(key).expect("CString::new failed");
            host_get_global_uint(app, c_key.as_ptr(), c_key.as_bytes().len() as i32)
        }
    }

    pub fn set_global_uint(app: u64, key: &str, value: u64) {
        unsafe {
            let c_key = CString::new(key).expect("CString::new failed");
            host_set_global_uint(app, c_key.as_ptr(), c_key.as_bytes().len() as i32, value);
        }
    }

    pub fn hello(message: &str) {
        unsafe {
            let c_message = CString::new(message).expect("CString::new failed");
            host_hello(c_message.as_ptr());
        }
    }
}

use algokit::{get_global_uint, hello, set_global_uint};

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    let key = "counter";

    let counter_value = get_global_uint(888, key);

    set_global_uint(888, key, counter_value + 1);

    hello(&format!(
        "Hi from Rust! Here's the global uint: {}",
        get_global_uint(888, key)
    ));
    1
}
