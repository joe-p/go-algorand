const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.resolveTargetQuery(.{
        .cpu_arch = .wasm32,
        .os_tag = .freestanding,
    });

    const optimize = std.builtin.OptimizeMode.ReleaseSmall;

    const lib = b.addExecutable(.{
        .name = "program",
        .root_source_file = b.path("src/lib.zig"),
        .target = target,
        .optimize = optimize,
    });

    lib.import_memory = true;


    lib.entry = .disabled;
    lib.rdynamic = true;
    b.installArtifact(lib);
}

