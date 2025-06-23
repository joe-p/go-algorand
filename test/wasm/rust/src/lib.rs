#![no_std]

use algokit::{get_current_application_id, get_global_uint, set_global_uint};

pub fn increment_counter() {
    let key = b"counter";
    let app_id = get_current_application_id();

    set_global_uint(app_id, key, get_global_uint(app_id, key) + 1);
}

pub fn get_counter() -> u64 {
    get_global_uint(get_current_application_id(), b"counter")
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    while get_counter() < 25 {
        increment_counter();
    }

    get_counter()
}
