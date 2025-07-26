#!/bin/bash

set -e

build_tinygo() {
    local dir="$1"
    local output="$2"
    echo "Building tinygo..."
    cd "$dir"
    tinygo build -o "$output" -target wasm-unknown -no-debug -gc leaking -panic=trap -scheduler=none -ldflags="-extldflags '--import-memory'" program.go
    minify_wasm "$output"
    cd ..
}

build_assemblyscript() {
    local dir="$1"
    local output="$2"
    echo "Building AssemblyScript..."
    cd "$dir"
    npm run asbuild:release
    minify_wasm "build/release.wasm"
    cd ..
}

build_rust() {
    local dir=$1
    local wasm_file=$2
    
    echo "Building Rust ($dir)..."
    cd "$dir"
    cargo build --release --target wasm32-unknown-unknown
    minify_wasm "target/wasm32-unknown-unknown/release/$wasm_file"
    cd ..
}

build_zig() {
    local dir="$1"
    local output="$2"
    echo "Building Zig..."
    cd "$dir"
    zig build
    minify_wasm "zig-out/bin/program.wasm"
    cd ..
}

minify_wasm() {
    local input="$1"
    echo "Minifying $input..."
    ../minify.sh "$input"
    echo ""
}

# Build tinygo
build_tinygo "tinygo" "program.wasm"

# Build AssemblyScript
build_assemblyscript "assembly_script" "build/release.wasm"

# Build Rust
build_rust "rust" "program.wasm"

# Build Zig
build_zig "zig" "zig-out/bin/program.wasm"

# Build Rust (fibo)
build_rust "fibo" "fibo.wasm"

# Build Rust (arc200)
build_rust "arc200" "arc200.wasm"

# Build Rust (int_1)
build_rust "int_1" "int_1.wasm"

echo "Done!"
