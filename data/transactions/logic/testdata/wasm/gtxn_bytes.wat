;; Fee selects a scalar field, FirstValid the group index. Expect a 32-byte value
;; equal to the current sender, comparing all four words after copying.
(module
  (import "env" "txn_uint" (func $txn (param i32) (result i64)))
  (import "env" "txn_bytes" (func $sender (param i32 i32 i32) (result i32)))
  (import "env" "gtxn_bytes" (func $gtxn (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (if (i32.ne (call $gtxn (i32.wrap_i64 (call $txn (i32.const 2)))
                           (i32.wrap_i64 (call $txn (i32.const 1)))
                           (i32.const 0) (i32.const 32)) (i32.const 32))
      (then (return (i32.const 0))))
    (drop (call $sender (i32.const 0) (i32.const 32) (i32.const 32)))
    (i32.and
      (i32.and (i64.eq (i64.load (i32.const 0)) (i64.load (i32.const 32)))
               (i64.eq (i64.load (i32.const 8)) (i64.load (i32.const 40))))
      (i32.and (i64.eq (i64.load (i32.const 16)) (i64.load (i32.const 48)))
               (i64.eq (i64.load (i32.const 24)) (i64.load (i32.const 56)))))))
