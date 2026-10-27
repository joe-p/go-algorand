;; Fee selects payload size, FirstValid log count, LastValid approval (0 rejects).
(module
  (import "env" "log" (func $log (param i32 i32)))
  (import "env" "txn_uint" (func $txn (param i32) (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "\15\1f\7c\75")
  (func (export "program") (result i32) (local $n i64)
    (local.set $n (call $txn (i32.const 2)))
    (block $done (loop $again
      (br_if $done (i64.eqz (local.get $n)))
      (call $log (i32.const 0) (i32.wrap_i64 (call $txn (i32.const 1))))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (br $again)))
    (i64.ne (call $txn (i32.const 4)) (i64.const 0))))
