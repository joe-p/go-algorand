;; A WASM application program that reads its first argument through the array
;; transaction-field accessor: txna_bytes(ApplicationArgs=26, 0, out, len).
;; It approves iff the argument is exactly three bytes long.
(module
  (import "env" "txna_bytes" (func $txna_bytes (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (i32.eq (call $txna_bytes (i32.const 26) (i32.const 0) (i32.const 0) (i32.const 64))
            (i32.const 3))))
