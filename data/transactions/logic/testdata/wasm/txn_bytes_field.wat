;; Select a field from argument 0; query and copy it, checking the length and
;; both endpoints of nonempty values. Test values consist entirely of byte 7.
(module
  (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
  (import "env" "txn_bytes" (func $txn_bytes (param i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (local $field i32) (local $len i32)
    (drop (call $arg (i32.const 0) (i32.const 0) (i32.const 1)))
    (local.set $field (i32.load8_u (i32.const 0)))
    (local.set $len (call $txn_bytes (local.get $field) (i32.const -1) (i32.const 0)))
    (if (i32.ne (local.get $len)
          (call $txn_bytes (local.get $field) (i32.const 1) (i32.const 4096)))
      (then (return (i32.const 0))))
    (if (result i32) (i32.eqz (local.get $len))
      (then (i32.const 1))
      (else
        (i32.and
          (i32.eq (i32.load8_u (i32.const 1)) (i32.const 7))
          (i32.eq (i32.load8_u (local.get $len)) (i32.const 7)))))))
