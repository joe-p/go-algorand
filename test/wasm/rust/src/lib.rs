mod algokit;

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
