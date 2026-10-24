;; A WASM application program that reads the number of arguments through the
;; scalar transaction-field accessor and approves iff there is at least one.
(module
  (import "env" "txn_uint" (func $txn_uint (param i32) (result i64)))
  (func (export "program") (result i32)
    (i64.ne (call $txn_uint (i32.const 27)) (i64.const 0))))
