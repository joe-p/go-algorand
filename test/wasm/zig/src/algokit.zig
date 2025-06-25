extern "algorand" fn host_get_global_uint(app: u64, key: [*]const u8, len: i32) u64;
extern "algorand" fn host_set_global_uint(app: u64, key: [*]const u8, len: i32, value: u64) void;
extern "algorand" fn host_get_global_bytes(app: u64, key: [*]const u8, key_len: i32, byte_buffer: [*]u8, byte_buffer_len: i32) i32;
extern "algorand" fn host_set_global_bytes(app: u64, key: [*]const u8, key_len: i32, bytes: [*]const u8, bytes_len: i32) void;
extern "algorand" fn host_get_current_application_id() u64;
extern "algorand" fn host_bigint_add(a_ptr: [*]const u8, a_len: u32, b_ptr: [*]const u8, b_len: u32, c_ptr: [*]u8) u32;
extern "algorand" fn host_alloc(size: u32) [*]u8;
extern "algorand" fn host_dealloc(ptr: [*]u8) void;

pub fn get_global_uint(app: u64, key: []const u8) u64 {
    return host_get_global_uint(app, key.ptr, @intCast(key.len));
}

pub fn set_global_uint(app: u64, key: []const u8, value: u64) void {
    host_set_global_uint(app, key.ptr, @intCast(key.len), value);
}

const std = @import("std");
const Allocator = std.mem.Allocator;

const DEFAULT_BUFFER_LEN: i32 = 4000;

pub fn get_global_bytes(allocator: Allocator, app: u64, key: []const u8, buffer_len: i32) ![]u8 {
    var buffer = try allocator.alloc(u8, @intCast(buffer_len));
    errdefer allocator.free(buffer);

    const len = host_get_global_bytes(app, key.ptr, @intCast(key.len), buffer.ptr, buffer_len);

    if (len >= 0) {
        const actual_len = @as(usize, @intCast(len));
        if (actual_len < buffer.len) {
            buffer = try allocator.realloc(buffer, actual_len);
        }
        return buffer;
    } else {
        allocator.free(buffer);
        return try allocator.alloc(u8, 0);
    }
}

pub fn set_global_bytes(app: u64, key: []const u8, value: []const u8) void {
    host_set_global_bytes(app, key.ptr, @intCast(key.len), value.ptr, @intCast(value.len));
}

var cached_app_id: ?u64 = null;

pub fn get_current_application_id() u64 {
    if (cached_app_id) |id| {
        return id;
    }
    const id = host_get_current_application_id();
    cached_app_id = id;
    return id;
}

pub fn GlobalStateKey(comptime ValueType: type) type {
    return struct {
        const Self = @This();

        key: []const u8,

        pub fn init(key: []const u8) Self {
            return Self{ .key = key };
        }

        fn appId(self: *const Self) u64 {
            _ = self;
            return get_current_application_id();
        }

        pub fn get(self: *const Self) !ValueType {
            switch (ValueType) {
                u64 => return get_global_uint(self.appId(), self.key),
                u256 => {
                    const byte_slice = try get_global_bytes(host_allocator, self.appId(), self.key, 32);
                    const bytes: [32]u8 = byte_slice[0..32].*;

                    return std.mem.readInt(u256, &bytes, .big);
                },
                else => @compileError("Unsupported type for GlobalStateKey"),
            }
        }

        pub fn set(self: *const Self, value: ValueType) void {
            switch (ValueType) {
                u64 => set_global_uint(self.appId(), self.key, value),
                u256 => {
                    var buffer: [32]u8 = undefined;

                    std.mem.writeInt(u256, &buffer, value, .big);
                    set_global_bytes(self.appId(), self.key, &buffer);
                },
                else => @compileError("Unsupported type for GlobalStateKey"),
            }
        }

        pub fn getBytes(self: *const Self, allocator: Allocator) ![]u8 {
            if (ValueType != []const u8 and ValueType != []u8) {
                @compileError("getBytes only available for byte slice types");
            }
            return get_global_bytes(allocator, self.appId(), self.key, DEFAULT_BUFFER_LEN);
        }

        pub fn setBytes(self: *const Self, value: []const u8) void {
            if (ValueType != []const u8 and ValueType != []u8) {
                @compileError("setBytes only available for byte slice types");
            }
            set_global_bytes(self.appId(), self.key, value);
        }
    };
}

pub const HostAllocator = struct {
    pub fn allocator(self: *HostAllocator) Allocator {
        return .{
            .ptr = self,
            .vtable = &.{
                .alloc = alloc,
                .resize = resize,
                .free = free,
                .remap = remap,
            },
        };
    }

    fn alloc(
        ctx: *anyopaque,
        len: usize,
        ptr_align: std.mem.Alignment,
        ret_addr: usize,
    ) ?[*]u8 {
        _ = ctx;
        _ = ptr_align;
        _ = ret_addr;

        if (len == 0) return null;

        return host_alloc(@intCast(len));
    }

    fn resize(
        ctx: *anyopaque,
        buf: []u8,
        buf_align: std.mem.Alignment,
        new_len: usize,
        ret_addr: usize,
    ) bool {
        _ = ctx;
        _ = buf_align;
        _ = ret_addr;

        // Host allocator doesn't support resize, so we return false
        // This will cause the allocator to use alloc/free for reallocation
        _ = buf;
        _ = new_len;
        return false;
    }

    fn free(
        ctx: *anyopaque,
        buf: []u8,
        buf_align: std.mem.Alignment,
        ret_addr: usize,
    ) void {
        _ = ctx;
        _ = buf_align;
        _ = ret_addr;

        if (buf.len == 0) return;

        host_dealloc(buf.ptr);
    }

    fn remap(
        ctx: *anyopaque,
        buf: []u8,
        buf_align: std.mem.Alignment,
        new_len: usize,
        ret_addr: usize,
    ) ?[*]u8 {
        _ = ctx;
        _ = buf_align;
        _ = ret_addr;
        _ = buf;
        _ = new_len;

        // Host allocator doesn't support remap
        return null;
    }
};

pub var host_allocator_instance = HostAllocator{};
pub const host_allocator = host_allocator_instance.allocator();
