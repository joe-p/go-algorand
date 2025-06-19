use std::ffi::CString;
use std::os::raw::c_char;

#[link(wasm_import_module = "algorand")]
unsafe extern "C" {
    fn hello(message: *const c_char);
    fn get_u64() -> u64;
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    unsafe {
        let c_string = CString::new(format!("Hi from Rust! Here's the u64: {}", get_u64()))
            .expect("CString::new failed");

        hello(c_string.as_ptr());
    }
    1
}
