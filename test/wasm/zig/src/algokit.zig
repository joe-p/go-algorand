extern "algorand" fn host_get_global_uint(app: u64, key: [*]const u8, len: i32) u64;
extern "algorand" fn host_set_global_uint(app: u64, key: [*]const u8, len: i32, value: u64) void;

pub fn get_global_uint(app: u64, key: []const u8) u64 {
    return host_get_global_uint(app, key.ptr, @intCast(key.len));
}

pub fn set_global_uint(app: u64, key: []const u8, value: u64) void {
    host_set_global_uint(app, key.ptr, @intCast(key.len), value);
}
