;; Fee selects a scalar field, FirstValid the group index, LastValid the expected value.
(module
  (import "env" "txn_uint" (func $txn (param i32) (result i64)))
  (import "env" "gtxn_uint" (func $gtxn (param i32 i32) (result i64)))
  (func (export "program") (result i32)
    (i64.eq
      (call $gtxn (i32.wrap_i64 (call $txn (i32.const 2)))
                  (i32.wrap_i64 (call $txn (i32.const 1))))
      (call $txn (i32.const 4)))))
