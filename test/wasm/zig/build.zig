const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.resolveTargetQuery(.{
        .cpu_arch = .wasm32,
        .os_tag = .freestanding,
    });

    const optimize = std.builtin.OptimizeMode.ReleaseSmall;

    const lib_mod = b.addModule("program", .{
        .root_source_file = b.path("src/lib.zig"),
        .target = target,
        .optimize = optimize,
    });

    const lib = b.addExecutable(.{
        .name = "program",
        .root_module = lib_mod,
    });
    lib.import_memory = true;
    lib.entry = .disabled;
    lib.rdynamic = true;

    b.installArtifact(lib);
    const lib_check = b.addExecutable(.{
        .name = "program",
        .root_module = lib_mod,
    });

    const check = b.step("check", "Check if program compiles");
    check.dependOn(&lib_check.step);
}
