use std::ffi::CString;
use std::os::raw::c_char;

#[link(wasm_import_module = "algorand")]
unsafe extern "C" {
    fn hello(message: *const c_char);
    fn get_global_uint(app: u64, key: *const c_char, len: i32) -> u64;
    fn set_global_uint(app: u64, key: *const c_char, len: i32, value: u64);
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    unsafe {
        let key = CString::new("counter").expect("CString::new failed");

        let counter_value = get_global_uint(888, key.as_ptr(), key.as_bytes().len() as i32);

        set_global_uint(
            888,
            key.as_ptr(),
            key.as_bytes().len() as i32,
            counter_value + 1,
        );

        let c_string = CString::new(format!(
            "Hi from Rust! Here's the global uint: {}",
            get_global_uint(888, key.as_ptr(), key.as_bytes().len() as i32)
        ))
        .expect("CString::new failed");

        hello(c_string.as_ptr());
    }
    1
}
