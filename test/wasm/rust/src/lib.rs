#![no_std]

mod algokit;

use algokit::{get_global_uint, set_global_uint};

pub fn increment_counter() {
    let key = "counter";

    set_global_uint(888, key, get_global_uint(888, key) + 1);
}

pub fn get_counter() -> u64 {
    get_global_uint(888, "counter")
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    while get_counter() < 10 {
        increment_counter();
    }

    get_counter()
}
