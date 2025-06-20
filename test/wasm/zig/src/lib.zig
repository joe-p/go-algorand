const algokit = @import("algokit.zig");

fn increment_counter() void {
    const key = "counter";
    algokit.set_global_uint(888, key, algokit.get_global_uint(888, key) + 1);
}

fn get_counter() u64 {
    return algokit.get_global_uint(888, "counter");
}

export fn program() u64 {
    while (get_counter() < 10) {
        increment_counter();
    }
    
    return get_counter();
}
