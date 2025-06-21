#![no_std]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

fn fibo(n: u64) -> u64 {
    if n <= 1 {
        return n;
    }
    fibo(n - 1) + fibo(n - 2)
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    // 7 is the highest we can go in TEAL without hitting opcode limit
    return fibo(7);
}
