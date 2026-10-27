;; Fee is the asset reference, LastValid the expected result (length or -1).
;; ApplicationArgs[0] contains the expected unit name (at most eight bytes).
(module
  (import "env" "txn_uint" (func $txn (param i32) (result i64)))
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "asset_unit_name" (func $name (param i64 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32) (local $len i32)
    (local.set $len (call $name (call $txn (i32.const 1)) (i32.const 0) (i32.const 8)))
    (if (i32.ne (local.get $len) (i32.wrap_i64 (call $txn (i32.const 4))))
      (then (return (i32.const 0))))
    (if (i32.lt_s (local.get $len) (i32.const 0)) (then (return (i32.const 1))))
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 32) (i32.const 8)))
    (i64.eq (i64.load (i32.const 0)) (i64.load (i32.const 32)))))
