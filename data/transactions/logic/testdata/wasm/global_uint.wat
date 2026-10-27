;; Fee selects a context field, LastValid the expected value.
(module
  (import "env" "txn_uint" (func $txn (param i32) (result i64)))
  (import "env" "global_uint" (func $global (param i32) (result i64)))
  (func (export "program") (result i32)
    (i64.eq (call $global (i32.wrap_i64 (call $txn (i32.const 1))))
            (call $txn (i32.const 4)))))
