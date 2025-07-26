#!/bin/bash

set -e

echo "Building tinygo..."
cd tinygo
tinygo build -o program.wasm -target wasm-unknown -no-debug -gc leaking -panic=trap -scheduler=none -ldflags="-extldflags '--import-memory'" program.go
../minify.sh ./program.wasm 
echo ""
cd ..

echo "Building AssemblyScript..."
cd assembly_script
npm run asbuild:release
../minify.sh build/release.wasm
echo ""
cd ..

echo "Building Rust..."
cd rust
cargo build --release --target wasm32-unknown-unknown
../minify.sh target/wasm32-unknown-unknown/release/program.wasm
echo ""
cd ..

echo "Building Zig..."
cd zig
zig build
../minify.sh zig-out/bin/program.wasm
echo ""
cd ..


echo "Building Rust (fibo)"
cd fibo
cargo build --release --target wasm32-unknown-unknown
../minify.sh target/wasm32-unknown-unknown/release/fibo.wasm
echo ""
cd ..


echo "Building Rust (arc200)"
cd arc200
cargo build --release --target wasm32-unknown-unknown
../minify.sh target/wasm32-unknown-unknown/release/arc200.wasm
echo ""
cd ..

echo "Building Rust (int_1)"
cd int_1
cargo build --release --target wasm32-unknown-unknown
../minify.sh target/wasm32-unknown-unknown/release/int_1.wasm
echo ""
cd ..

echo "Done!"
