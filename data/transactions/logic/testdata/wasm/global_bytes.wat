;; Fee selects a context field. Expect the current application's address, also
;; supplied as the transaction sender for comparison.
(module
  (import "env" "txn_uint" (func $txn (param i32) (result i64)))
  (import "env" "txn_bytes" (func $sender (param i32 i32 i32) (result i32)))
  (import "env" "global_bytes" (func $global (param i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (if (i32.ne (call $global (i32.wrap_i64 (call $txn (i32.const 1)))
                             (i32.const 0) (i32.const 32)) (i32.const 32))
      (then (return (i32.const 0))))
    (drop (call $sender (i32.const 0) (i32.const 32) (i32.const 32)))
    (i32.and
      (i32.and (i64.eq (i64.load (i32.const 0)) (i64.load (i32.const 32)))
               (i64.eq (i64.load (i32.const 8)) (i64.load (i32.const 40))))
      (i32.and (i64.eq (i64.load (i32.const 16)) (i64.load (i32.const 48)))
               (i64.eq (i64.load (i32.const 24)) (i64.load (i32.const 56)))))))
