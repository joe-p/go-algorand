def run(cmd)
  system(cmd, out: $stdout, err: :out)
end

target_dir = "#{__dir__}/target/release"
go_file = 'data/transactions/logic/rust.go'

run "rust2go-cli --src src/lib.rs --dst #{go_file}"

file_content = File.read(go_file)

file_content.sub!('package main', 'package logic')
file_content.sub!(
  'manually.',
  "manually.\n\n#cgo LDFLAGS: -L#{target_dir} -lgo_algorand"
)

File.open(go_file, 'w') { |file| file.puts file_content }

run 'cargo build --release'
run 'cargo build --manifest-path test/wasm/fibo/Cargo.toml --target wasm32-unknown-unknown --release'
run 'wamrc --size-level=3 --bounds-checks=1 -o fibo.aot test/wasm/fibo/target/wasm32-unknown-unknown/release/fibo.wasm'
run 'go test -count=1 ./data/transactions/logic -bench=Fibo'
