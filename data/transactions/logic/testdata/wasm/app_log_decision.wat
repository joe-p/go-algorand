;; Emits a log, then approves with zero args, rejects with one, traps with two,
;; and exhausts fuel with three. Used to test outer-group atomicity.
(module
  (import "env" "log" (func $log (param i32 i32)))
  (import "env" "txn_uint" (func $txn (param i32) (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "\15\1f\7c\75\00\00\00\00\00\00\00\2a")
  (func (export "program") (result i32)
    (call $log (i32.const 0) (i32.const 12))
    (if (i64.eq (call $txn (i32.const 27)) (i64.const 2)) (then (unreachable)))
    (if (i64.eq (call $txn (i32.const 27)) (i64.const 3)) (then (loop $forever (br $forever))))
    (i64.eqz (call $txn (i32.const 27)))))
