const algokit = @import("algokit.zig");

fn increment_counter() void {
    const key = "counter";
    const app_id = algokit.get_current_application_id();
    algokit.set_global_uint(app_id, key, algokit.get_global_uint(app_id, key) + 1);
}

fn get_counter() u64 {
    return algokit.get_global_uint(algokit.get_current_application_id(), "counter");
}

export fn program() u64 {
    while (get_counter() < 10) {
        increment_counter();
    }
    
    return get_counter();
}
