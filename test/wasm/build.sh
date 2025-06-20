#!/bin/bash

set -e

echo "Building tinygo..."
cd tinygo
tinygo build -o program.wasm -target wasm-unknown -no-debug -gc leaking -panic=trap -scheduler=none program.go
wasm-opt -Oz program.wasm -o program.wasm
echo "*** TinyGo WASM Size: `ls -lh program.wasm | awk '{print $5}'` bytes ***"
echo ""
cd ..

echo "Building AssemblyScript..."
cd assembly_script
npm run asbuild:release
wasm-opt -Oz --enable-bulk-memory-opt build/release.wasm -o build/release.wasm
echo "*** AssemblyScript WASM Size: `ls -lh build/release.wasm | awk '{print $5}'` bytes ***"
echo ""
cd ..

echo "Building Rust..."
cd rust
cargo build --release --target wasm32-unknown-unknown
wasm-opt -Oz target/wasm32-unknown-unknown/release/program.wasm -o target/wasm32-unknown-unknown/release/program.wasm
echo "*** Rust WASM Size: `ls -lh target/wasm32-unknown-unknown/release/program.wasm | awk '{print $5}'` bytes ***"
echo ""
cd ..

echo "Building Zig..."
cd zig
zig build-exe src/lib.zig -target wasm32-freestanding -O ReleaseSmall -fno-entry -rdynamic --export=program -femit-bin=program.wasm
wasm-opt -Oz program.wasm -o program.wasm
echo "*** Zig WASM Size: `ls -lh program.wasm | awk '{print $5}'` bytes ***"
echo ""

echo "Done!"
