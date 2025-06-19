#!/bin/bash

set -e

echo "Building tinygo..."
cd tinygo
tinygo build -o program.wasm -target wasm-unknown -no-debug -gc leaking -panic=trap -scheduler=none program.go
wasm-opt -Oz program.wasm -o program.wasm
ls -lh program.wasm
cd ..


echo "Building AssemblyScript..."
cd assembly_script
npm run asbuild:release
wasm-opt -Oz --enable-bulk-memory-opt build/release.wasm -o build/release.wasm
ls -lh build/release.wasm
cd ..

echo "Building Rust..."
cd rust
cargo build --release --target wasm32-unknown-unknown
wasm-opt -Oz target/wasm32-unknown-unknown/release/program.wasm -o target/wasm32-unknown-unknown/release/program.wasm
ls -lh target/wasm32-unknown-unknown/release/program.wasm

echo "Done!"
