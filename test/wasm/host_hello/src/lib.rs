use std::ffi::CString;
use std::os::raw::c_char;

#[link(wasm_import_module = "algorand")]
unsafe extern "C" {
    fn hello(message: *const c_char);
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    unsafe {
        let c_string = CString::new("Hi from Rust!").expect("CString::new failed");

        hello(c_string.as_ptr());
    }
    1
}
