#[unsafe(no_mangle)]
fn fibonacci(n: u64) -> u64 {
    if n <= 1 {
        return n;
    }
    fibonacci(n - 1) + fibonacci(n - 2)
}

#[unsafe(no_mangle)]
fn no_op(n: u64) -> u64 {
    n
}
